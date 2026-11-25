//! Studio — an editing agent that lives in the dashboard.
//!
//! Three invariants hold this module together:
//!
//! 1. **The model cannot write a file.** It has one mutating verb,
//!    `studio_propose_patch`, and that only queues a patch for a human to apply.
//! 2. **Every apply is reversible.** A snapshot is taken before the first byte
//!    is written, and a failing check restores it automatically.
//! 3. **Every path is resolved against exactly one site folder.** There is no
//!    tool that accepts a root-relative or absolute path.

pub mod apply;
pub mod config;
pub mod llm;
pub mod outline;
pub mod patch;
pub mod shot;
pub mod snapshot;
pub mod stage;
pub mod store;
pub mod tools;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

use crate::manifest::Manifest;
use crate::supervisor::Supervisor;
use crate::util;

static SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct StudioState {
    pub root: PathBuf,
    pub data: PathBuf,
    pub sup: Arc<Supervisor>,
    pub client: reqwest::Client,
    /// HTTP checks need a client that does not decompress on our behalf: with
    /// gzip enabled, reqwest strips `Content-Encoding` and SB104 fails on a
    /// correctly compressed response.
    pub check_client: reqwest::Client,
    /// Per-process session id: the cookie the dashboard hands its own browser.
    pub sid: String,
    /// Optional fixed token for non-browser clients (`STUDIO_TOKEN`).
    pub token: Option<String>,
    /// One turn per site. Two concurrent edits on one folder is how work is lost.
    pub busy: Arc<Mutex<HashSet<String>>>,
    pub turns: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    /// Turn start times per site, for the hourly rate limit.
    pub turn_log: Arc<Mutex<HashMap<String, Vec<std::time::Instant>>>>,
    /// Recently seen client nonces, so a double submit cannot open two turns.
    pub nonces: Arc<Mutex<HashMap<String, std::time::Instant>>>,
    pub events: broadcast::Sender<Value>,
}

impl StudioState {
    pub fn new(root: PathBuf, sup: Arc<Supervisor>, client: reqwest::Client) -> Self {
        let data = dir(&root);
        let _ = ensure_dir(&data);
        for sub in ["threads", "patches", "snapshots/blobs", "shots", "stage"] {
            let _ = ensure_dir(&data.join(sub));
        }
        let seed = format!(
            "{}:{}:{}",
            util::now_iso(),
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        );
        let sid = util::sha256_bytes(seed.as_bytes())[..32].to_string();
        let token = std::env::var("STUDIO_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        let (events, _) = broadcast::channel(1024);
        let check_client = reqwest::Client::builder()
            .no_gzip()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            root,
            data,
            sup,
            client,
            check_client,
            sid,
            token,
            busy: Default::default(),
            turns: Default::default(),
            turn_log: Default::default(),
            nonces: Default::default(),
            events,
        }
    }
}

pub fn dir(root: &Path) -> PathBuf {
    root.join("dashboard/data/studio")
}

pub fn ensure_dir(p: &Path) -> Result<()> {
    std::fs::create_dir_all(p).with_context(|| format!("create {}", p.display()))
}

/// Write through a temp file + rename so a crash cannot leave half a patch on disk.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp.{}",
        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
        std::process::id()
    ));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn new_id(prefix: &str) -> String {
    let seed = format!(
        "{}:{}:{}",
        util::now_iso(),
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    );
    format!("{prefix}-{}", &util::sha256_bytes(seed.as_bytes())[..12])
}

pub fn emit(tx: &broadcast::Sender<Value>, thread: &str, mut ev: Value) {
    if let Some(o) = ev.as_object_mut() {
        o.insert("thread".into(), json!(thread));
        o.insert("ts".into(), json!(util::now_iso()));
    }
    let _ = tx.send(ev);
}

// ── sites ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SiteRef {
    pub id: String,
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// `hand-authored` | `generated` | `node`
    pub kind: String,
    pub kind_reason: String,
}

/// Load a site and classify how its files are produced. The classification is a
/// hard guard, not a hint: patching `public/` of a generated site is how an edit
/// silently disappears on the next build.
pub fn site_ref(root: &Path, id: &str) -> Result<SiteRef> {
    let manifest =
        crate::supervisor::load_manifest(root, id).with_context(|| format!("load site {id}"))?;
    let dir = manifest.site_dir(root);
    if !dir.is_dir() {
        bail!("site folder missing: {}", dir.display());
    }
    let has_pipeline = !manifest.pipeline.is_empty();
    let has_build = dir.join("tools/build.js").is_file();
    let node_api = std::fs::read_to_string(dir.join("server.js"))
        .map(|t| t.contains("/api/"))
        .unwrap_or(false);
    let (kind, kind_reason) = if has_pipeline {
        ("generated", "sitebox.toml declares a pipeline".to_string())
    } else if has_build {
        ("generated", "tools/build.js generates the site".to_string())
    } else if node_api || manifest.site.serve == "node" {
        ("node", "server.js owns /api".to_string())
    } else {
        (
            "hand-authored",
            "public/ is the source of truth".to_string(),
        )
    };
    Ok(SiteRef {
        id: id.to_string(),
        dir,
        manifest,
        kind: kind.into(),
        kind_reason,
    })
}

/// Resolve `rel` under `site_dir`, refusing anything that escapes it.
///
/// The check is on the resolved path, not the string: `a/../../etc/passwd`,
/// percent-encoded dots, and symlinked directories all have to fail here.
pub fn site_path(site_dir: &Path, rel: &str) -> Result<PathBuf> {
    let rel = rel.trim();
    if rel.is_empty() {
        bail!("path is required");
    }
    if rel.starts_with('/') || rel.starts_with('\\') {
        bail!("absolute paths are not allowed");
    }
    if rel.contains('\0') {
        bail!("invalid path");
    }
    let candidate = Path::new(rel);
    for c in candidate.components() {
        match c {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("path may not escape the site: {rel}")
            }
            _ => {}
        }
    }
    let joined = site_dir.join(candidate);
    let site_canon = site_dir
        .canonicalize()
        .unwrap_or_else(|_| site_dir.to_path_buf());
    let check = if joined.exists() {
        joined.canonicalize().unwrap_or_else(|_| joined.clone())
    } else {
        joined
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .unwrap_or_else(|| site_canon.clone())
    };
    if !check.starts_with(&site_canon) {
        bail!("path escapes the site: {rel}");
    }
    Ok(joined)
}

// ── turn ─────────────────────────────────────────────────────────────────────

/// Run one assistant turn: model → tools → model, until it stops calling tools.
pub async fn run_turn(st: StudioState, thread_id: String, user_msg: String) {
    let site_id = match store::get(&st, &thread_id) {
        Ok(t) => t.site_id,
        Err(e) => {
            emit(
                &st.events,
                &thread_id,
                json!({"type":"error","message":format!("{e:#}")}),
            );
            return;
        }
    };
    if let Err(e) = run_turn_inner(&st, &thread_id, &site_id, &user_msg).await {
        emit(
            &st.events,
            &thread_id,
            json!({"type":"error","message":format!("{e:#}")}),
        );
    }
    st.busy.lock().await.remove(&site_id);
    st.turns.lock().await.remove(&thread_id);
}

async fn run_turn_inner(
    st: &StudioState,
    thread_id: &str,
    site_id: &str,
    user_msg: &str,
) -> Result<()> {
    let site = site_ref(&st.root, site_id)?;
    store::append_msg(st, thread_id, &store::Message::new("user", user_msg))?;
    if store::get(st, thread_id)?.title.is_empty() {
        store::set_title(st, thread_id, user_msg)?;
    }
    emit(&st.events, thread_id, json!({"type":"turn_start"}));

    let settings = config::Settings::load(&st.root);
    if !settings.configured() {
        bail!("agent is not configured: set base URL and model in Studio settings");
    }
    let max_rounds = settings.max_rounds.unwrap_or(6).clamp(1, 12);
    let tools = tools::definitions();
    let mut tokens_in: u64 = 0;
    let mut tokens_out: u64 = 0;

    for round in 0..max_rounds {
        let messages = build_messages(st, thread_id, &site)?;
        let out = llm::stream_chat(st, &settings, messages, tools.clone(), thread_id).await?;
        tokens_in += out.tokens_in;
        tokens_out += out.tokens_out;

        let has_calls = !out.tool_calls.is_empty();
        store::append_msg(
            st,
            thread_id,
            &store::Message {
                role: "assistant".into(),
                content: out.text.clone(),
                tool_calls: if has_calls {
                    Some(out.tool_calls.clone())
                } else {
                    None
                },
                tool_call_id: None,
                name: None,
                ts: util::now_iso(),
            },
        )?;

        if !has_calls {
            emit(
                &st.events,
                thread_id,
                json!({"type":"done","rounds":round+1,"tokens_in":tokens_in,"tokens_out":tokens_out}),
            );
            return Ok(());
        }

        for call in &out.tool_calls {
            let name = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let raw_args = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let call_id = call.get("id").and_then(|v| v.as_str()).unwrap_or("call");
            let args: Value = serde_json::from_str(raw_args).unwrap_or_else(|_| json!({}));
            emit(
                &st.events,
                thread_id,
                json!({"type":"tool_start","name":name,"args":args,"call_id":call_id}),
            );
            let result = match tools::execute(st, &site, thread_id, name, &args).await {
                Ok(v) => v,
                Err(e) => json!({"ok": false, "error": format!("{e:#}")}),
            };
            emit(
                &st.events,
                thread_id,
                json!({"type":"tool_end","name":name,"call_id":call_id,"ok":result.get("ok").and_then(|v| v.as_bool()).unwrap_or(true),"result":short(&result)}),
            );
            store::append_msg(
                st,
                thread_id,
                &store::Message {
                    role: "tool".into(),
                    content: clip(&result.to_string(), 6000),
                    tool_calls: None,
                    tool_call_id: Some(call_id.to_string()),
                    name: Some(name.to_string()),
                    ts: util::now_iso(),
                },
            )?;
        }
    }

    emit(
        &st.events,
        thread_id,
        json!({"type":"done","rounds":max_rounds,"tokens_in":tokens_in,"tokens_out":tokens_out,"note":"tool round limit reached"}),
    );
    Ok(())
}

fn short(v: &Value) -> Value {
    // Tool results can carry a whole file outline; the UI only needs a summary.
    let s = v.to_string();
    if s.len() <= 700 {
        v.clone()
    } else {
        json!({"ok": v.get("ok"), "summary": clip(&s, 600)})
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push_str("… [truncated]");
        out
    }
}

fn build_messages(st: &StudioState, thread_id: &str, site: &SiteRef) -> Result<Vec<Value>> {
    let mut out = vec![json!({"role":"system","content": tools::system_prompt(site)})];
    let hist = store::messages(st, thread_id)?;
    let start = hist.len().saturating_sub(24);
    for m in &hist[start..] {
        let mut obj = json!({"role": m.role, "content": clip(&m.content, 4000)});
        if let Some(calls) = &m.tool_calls {
            obj["tool_calls"] = json!(calls);
            if obj["content"].as_str().unwrap_or("").is_empty() {
                obj["content"] = json!("");
            }
        }
        if let Some(id) = &m.tool_call_id {
            obj["tool_call_id"] = json!(id);
        }
        if let Some(n) = &m.name {
            obj["name"] = json!(n);
        }
        out.push(obj);
    }
    Ok(out)
}
