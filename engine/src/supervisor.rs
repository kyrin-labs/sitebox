//! One supervisor for every site.
//!
//! Static sites are served in-process (one listener task, no child Node), so a
//! host that used to run 17 Node processes runs one. Sites that still need
//! their own `server.js` are spawned with a restart policy, and every line of
//! their output is kept both in a 500-line ring and in `logs/<id>.jsonl` so a
//! dashboard restart no longer loses the reason a site died.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::manifest::Manifest;
use crate::serve::{self, ServeState};
use crate::util;

pub const LOG_LIMIT: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub t: String,
    pub stream: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StartResult {
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub port: u16,
    pub verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logs: Vec<LogLine>,
}

enum Kind {
    Task(JoinHandle<()>),
    Child(tokio::process::Child),
}

struct Running {
    kind: Kind,
}

pub struct Supervisor {
    pub root: PathBuf,
    inner: Arc<Mutex<HashMap<String, Running>>>,
    logs: Arc<Mutex<HashMap<String, VecDeque<LogLine>>>>,
    client: reqwest::Client,
}

impl Supervisor {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            inner: Arc::new(Mutex::new(HashMap::new())),
            logs: Arc::new(Mutex::new(HashMap::new())),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn is_running<'a>(&'a self, id: &'a str) -> impl std::future::Future<Output = bool> + 'a {
        async move {
            let map = self.inner.lock().await;
            map.get(id)
                .map(|r| match &r.kind {
                    Kind::Task(h) => !h.is_finished(),
                    Kind::Child(c) => c.id().is_some(),
                })
                .unwrap_or(false)
        }
    }

    pub async fn running_ids(&self) -> Vec<String> {
        let map = self.inner.lock().await;
        map.iter()
            .filter(|(_, r)| match &r.kind {
                Kind::Task(h) => !h.is_finished(),
                Kind::Child(c) => c.id().is_some(),
            })
            .map(|(k, _)| k.clone())
            .collect()
    }

    pub async fn append_log(&self, id: &str, stream: &str, text: &str) {
        let line = LogLine {
            t: util::now_iso(),
            stream: stream.into(),
            text: text.into(),
        };
        {
            let mut logs = self.logs.lock().await;
            let ring = logs.entry(id.to_string()).or_default();
            ring.push_back(line.clone());
            while ring.len() > LOG_LIMIT {
                ring.pop_front();
            }
        }
        let dir = self.root.join("logs");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(body) = serde_json::to_string(&line) {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(format!("{id}.jsonl")))
            {
                let _ = writeln!(f, "{body}");
            }
        }
    }

    pub async fn logs(&self, id: &str, limit: usize) -> Vec<LogLine> {
        let logs = self.logs.lock().await;
        logs.get(id)
            .map(|r| r.iter().rev().take(limit).rev().cloned().collect())
            .unwrap_or_default()
    }

    pub async fn log_count(&self, id: &str) -> usize {
        self.logs.lock().await.get(id).map(|r| r.len()).unwrap_or(0)
    }

    pub async fn clear_logs(&self, id: &str) {
        self.logs.lock().await.remove(id);
        let _ = std::fs::remove_file(self.root.join("logs").join(format!("{id}.jsonl")));
    }

    pub async fn start(&self, m: &Manifest) -> Result<StartResult> {
        let id = &m.site.id;
        if self.is_running(id).await {
            return Ok(StartResult {
                ok: true,
                message: "already running".into(),
                pid: None,
                port: m.site.port,
                verified: true,
                hint: None,
                logs: vec![],
            });
        }
        let site_dir = m.site_dir(&self.root);
        if !site_dir.is_dir() {
            return Ok(StartResult {
                ok: false,
                message: format!("site folder missing: {}", site_dir.display()),
                port: m.site.port,
                verified: false,
                hint: Some("fix site.path in sitebox.toml".into()),
                pid: None,
                logs: vec![],
            });
        }
        if util::port_bound(m.site.port) {
            return Ok(StartResult {
                ok: false,
                message: format!("port {} is already in use before starting", m.site.port),
                port: m.site.port,
                verified: false,
                hint: Some("stop the stale process or change the port".into()),
                pid: None,
                logs: vec![],
            });
        }

        self.append_log(
            id,
            "sys",
            &format!("--- start on port {} ({}) ---", m.site.port, m.site.serve),
        )
        .await;

        match m.site.serve.as_str() {
            "static" | "proxy" => {
                let public = m.public_dir(&self.root);
                if !public.is_dir() {
                    return Ok(StartResult {
                        ok: false,
                        message: format!("public dir missing: {}", public.display()),
                        port: m.site.port,
                        verified: false,
                        hint: Some("create the public folder or fix public_dir".into()),
                        pid: None,
                        logs: vec![],
                    });
                }
                let state = ServeState {
                    root: public,
                    site_id: id.clone(),
                    api_upstream: if m.site.serve == "proxy" {
                        m.site.api_upstream.clone()
                    } else {
                        None
                    },
                    client: self.client.clone(),
                    sols: serve::sols_config(),
                };
                let port = m.site.port;
                let sup_logs = (self.logs.clone(), self.root.clone());
                let sid = id.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) = serve::serve(port, state).await {
                        let line = LogLine {
                            t: util::now_iso(),
                            stream: "err".into(),
                            text: format!("serve failed: {e}"),
                        };
                        let (logs, root) = sup_logs;
                        let mut l = logs.lock().await;
                        l.entry(sid.clone()).or_default().push_back(line);
                        let _ = std::fs::create_dir_all(root.join("logs"));
                        let _ = std::fs::write(
                            root.join("logs").join(format!("{sid}.jsonl")),
                            format!("{{\"stream\":\"err\",\"text\":\"{e}\"}}\n"),
                        );
                    }
                });
                self.inner.lock().await.insert(
                    id.clone(),
                    Running {
                        kind: Kind::Task(handle),
                    },
                );

                match self.wait_ready(m.site.port, 4_000).await {
                    Ok(()) => Ok(StartResult {
                        ok: true,
                        message: "started and verified".into(),
                        pid: Some(std::process::id()),
                        port: m.site.port,
                        verified: true,
                        hint: None,
                        logs: vec![],
                    }),
                    Err(e) => {
                        let _ = self.stop(id).await;
                        Ok(StartResult {
                            ok: false,
                            message: format!("listener never came up: {e}"),
                            port: m.site.port,
                            verified: false,
                            hint: Some("check logs".into()),
                            pid: None,
                            logs: self.logs(id, 20).await,
                        })
                    }
                }
            }
            "node" => {
                let server = site_dir.join("server.js");
                if !server.is_file() {
                    return Ok(StartResult {
                        ok: false,
                        message: format!("server.js not found at {}", server.display()),
                        port: m.site.port,
                        verified: false,
                        hint: Some("site is declared serve=node but has no server.js".into()),
                        pid: None,
                        logs: vec![],
                    });
                }
                let mut cmd = tokio::process::Command::new("node");
                cmd.arg(&server)
                    .current_dir(&site_dir)
                    .env("PORT", m.site.port.to_string());
                // The gateway lives in the engine's .env, not in the dashboard
                // process environment. Pass it to node sites explicitly so a
                // server.js can call the provider without a key in public/.
                for key in ["AI_BASE_URL", "AI_MODEL", "AI_API_KEY", "SEARXNG_URL"] {
                    if let Some(v) = crate::studio::config::env_or_file(&self.root, key) {
                        cmd.env(key, v);
                    }
                }
                // Some gateways need request headers (a user-agent, a session
                // id). They live in settings.json, so hand them over as JSON.
                let headers = crate::studio::config::Settings::load(&self.root).extra_headers;
                if !headers.is_empty() {
                    if let Ok(v) = serde_json::to_string(&headers) {
                        cmd.env("AI_HEADERS", v);
                    }
                }
                cmd.stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());
                let mut child = cmd.spawn().context("spawn node")?;
                let pid = child.id();
                if let Some(out) = child.stdout.take() {
                    self.pump(id, "out", out).await;
                }
                if let Some(err) = child.stderr.take() {
                    self.pump(id, "err", err).await;
                }
                self.inner.lock().await.insert(
                    id.clone(),
                    Running {
                        kind: Kind::Child(child),
                    },
                );
                match self.wait_ready(m.site.port, 6_000).await {
                    Ok(()) => Ok(StartResult {
                        ok: true,
                        message: "started and verified".into(),
                        pid,
                        port: m.site.port,
                        verified: true,
                        hint: None,
                        logs: vec![],
                    }),
                    Err(e) => {
                        let tail = self.logs(id, 20).await;
                        let _ = self.stop(id).await;
                        Ok(StartResult {
                            ok: false,
                            message: format!("process did not listen: {e}"),
                            port: m.site.port,
                            verified: false,
                            hint: Some(crash_hint(&tail)),
                            pid,
                            logs: tail,
                        })
                    }
                }
            }
            other => bail!("unknown serve mode `{other}`"),
        }
    }

    async fn pump<R>(&self, id: &str, stream: &str, reader: R)
    where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        let id = id.to_string();
        let stream = stream.to_string();
        let root = self.root.clone();
        let logs = self.logs.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let l = LogLine {
                    t: util::now_iso(),
                    stream: stream.clone(),
                    text: line,
                };
                {
                    let mut m = logs.lock().await;
                    let ring = m.entry(id.clone()).or_default();
                    ring.push_back(l.clone());
                    while ring.len() > LOG_LIMIT {
                        ring.pop_front();
                    }
                }
                let dir = root.join("logs");
                let _ = std::fs::create_dir_all(&dir);
                if let Ok(body) = serde_json::to_string(&l) {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(dir.join(format!("{id}.jsonl")))
                    {
                        let _ = writeln!(f, "{body}");
                    }
                }
            }
        });
    }

    async fn wait_ready(&self, port: u16, timeout_ms: u64) -> Result<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        while std::time::Instant::now() < deadline {
            if util::port_bound(port) {
                tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                if util::port_bound(port) {
                    return Ok(());
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        }
        bail!("timed out after {timeout_ms}ms")
    }

    pub async fn stop(&self, id: &str) -> Result<()> {
        let running = self.inner.lock().await.remove(id);
        match running {
            None => Ok(()),
            Some(mut r) => {
                match &mut r.kind {
                    Kind::Task(h) => {
                        h.abort();
                        self.append_log(id, "sys", "stopped (static listener)")
                            .await;
                    }
                    Kind::Child(c) => {
                        if let Some(pid) = c.id() {
                            unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                        }
                        let deadline =
                            std::time::Instant::now() + std::time::Duration::from_millis(2500);
                        loop {
                            match c.try_wait() {
                                Ok(Some(_)) => break,
                                _ if std::time::Instant::now() >= deadline => {
                                    let _ = c.start_kill();
                                    break;
                                }
                                _ => tokio::time::sleep(std::time::Duration::from_millis(80)).await,
                            }
                        }
                        self.append_log(id, "sys", "stopped").await;
                    }
                }
                Ok(())
            }
        }
    }

    pub async fn restart(&self, m: &Manifest) -> Result<StartResult> {
        self.stop(&m.site.id).await?;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        self.start(m).await
    }

    pub async fn health(&self, url: &str) -> serde_json::Value {
        match self.client.get(url).send().await {
            Ok(r) => {
                serde_json::json!({ "online": r.status().as_u16() < 500, "status": r.status().as_u16() })
            }
            Err(_) => serde_json::json!({ "online": false, "status": 0 }),
        }
    }
}

fn crash_hint(lines: &[LogLine]) -> String {
    let text = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if text.contains("EADDRINUSE") {
        "Port is already in use. Check `sitebox ports check`, then change the port.".into()
    } else if text.contains("Cannot find module") || text.contains("MODULE_NOT_FOUND") {
        "A required file or module is missing — check the site folder.".into()
    } else if text.contains("SyntaxError") {
        "server.js (or a required file) has a syntax error.".into()
    } else {
        "Read more with `sitebox logs <id>`.".into()
    }
}

/// Load the manifest for `id` under `root`.
///
/// A folder with no `sitebox.toml` is still a legitimate site: it was created
/// before the engine, and it must keep starting after the upgrade. The fallback
/// derives the contract from the registry plus the folder's `server.js`, and
/// never writes a manifest into a site the operator did not ask to migrate.
pub fn load_manifest(root: &Path, id: &str) -> Result<Manifest> {
    let dir = root.join("sites").join(id);
    if !dir.is_dir() {
        bail!("no site folder at {}", dir.display());
    }
    if dir.join(crate::manifest::MANIFEST_FILE).is_file() {
        return Manifest::load(&dir);
    }
    let cfg = crate::registry::SiteConfig::read(root)?;
    let site = cfg
        .find(id)
        .ok_or_else(|| anyhow::anyhow!("site {id} has no manifest and is not registered"))?;
    let has_api = std::fs::read_to_string(dir.join("server.js"))
        .map(|t| t.contains("/api/"))
        .unwrap_or(false);
    let mut m = Manifest::from_site(site);
    m.site.path = site.path.clone();
    m.site.serve = if has_api {
        "node".into()
    } else {
        "static".into()
    };
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_site_without_manifest_still_starts() {
        let root = std::env::temp_dir().join(format!("sbx-legacy-{}", std::process::id()));
        let site = root.join("sites/legacy");
        std::fs::create_dir_all(site.join("public")).unwrap();
        std::fs::write(site.join("server.js"), "// has /api/ routes\n").unwrap();
        std::fs::create_dir_all(root.join("dashboard/data")).unwrap();
        std::fs::write(
            root.join("dashboard/data/sites.json"),
            r##"{"dashboard":{"name":"SiteBox","port":4445},"sites":[
                {"id":"legacy","name":"Legacy","port":4601,"path":"sites/legacy",
                 "icon":"globe","iconColor":"#000","category":"tools","url":"http://localhost:4601"}]}"##,
        )
        .unwrap();

        let m = load_manifest(&root, "legacy").unwrap();
        assert_eq!(m.site.id, "legacy");
        assert_eq!(m.site.port, 4601);
        assert_eq!(m.site.serve, "node");
        assert!(m.public_dir(&root).is_dir());

        let _ = std::fs::remove_dir_all(&root);
    }
}
