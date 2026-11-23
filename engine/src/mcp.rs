//! MCP server (stdio, JSON-RPC 2.0).
//!
//! This is the agent-facing surface. An agent should never have to guess a
//! shell command: every capability is a typed tool with a JSON schema and a
//! structured result.

use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::checks;
use crate::deploy;
use crate::manifest::Manifest;
use crate::pipeline;
use crate::supervisor::Supervisor;
use crate::util;

pub async fn serve_stdio(root: PathBuf) -> Result<()> {
    let sup = Arc::new(Supervisor::new(root.clone()));
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse error: {e}")}});
                stdout.write_all(format!("{err}\n").as_bytes()).await?;
                stdout.flush().await?;
                continue;
            }
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        // Notifications have no id and must not be answered.
        if id.is_none() {
            continue;
        }
        let id = id.unwrap();

        let resp = match method {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "sitebox", "version": env!("CARGO_PKG_VERSION") }
            }),
            "ping" => json!({}),
            "tools/list" => json!({ "tools": tools() }),
            "tools/call" => {
                let params = req.get("params").cloned().unwrap_or(json!({}));
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match call(&root, &sup, name, args).await {
                    Ok(v) => json!({
                        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }],
                        "isError": false
                    }),
                    Err(e) => json!({
                        "content": [{ "type": "text", "text": format!("{e:#}") }],
                        "isError": true
                    }),
                }
            }
            other => {
                let err = json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":format!("unknown method {other}")}});
                stdout.write_all(format!("{err}\n").as_bytes()).await?;
                stdout.flush().await?;
                continue;
            }
        };
        let out = json!({ "jsonrpc": "2.0", "id": id, "result": resp });
        stdout.write_all(format!("{out}\n").as_bytes()).await?;
        stdout.flush().await?;
    }
    Ok(())
}

fn tools() -> Vec<Value> {
    let root_note = "SiteBox root (defaults to SITEBOX_ROOT or ~/sitebox)";
    vec![
        tool("sitebox_list", "List every registered site with its manifest status.", json!({})),
        tool("sitebox_get", "Read one site manifest as JSON.", json!({"id": {"type":"string", "description": "site id"}})),
        tool("sitebox_check", "Run the static check catalog (SB0xx) against one site.", json!({"id": {"type":"string"}})),
        tool("sitebox_verify", "Run HTTP checks (SB1xx) against a running site, starting it if needed.", json!({"id": {"type":"string"}, "auto_start": {"type":"boolean", "default": true}})),
        tool("sitebox_plan", "Show the pipeline and which steps would run.", json!({"id": {"type":"string"}})),
        tool("sitebox_build", "Run the declared pipeline (fetch → build → assets → splice).", json!({"id": {"type":"string"}, "step": {"type":"string"}, "force": {"type":"boolean"}})),
        tool("sitebox_up", "Start a site (static served in-process; node spawned).", json!({"id": {"type":"string"}})),
        tool("sitebox_down", "Stop a site.", json!({"id": {"type":"string"}})),
        tool("sitebox_restart", "Restart a site.", json!({"id": {"type":"string"}})),
        tool("sitebox_logs", "Read the last N log lines of a site.", json!({"id": {"type":"string"}, "lines": {"type":"integer", "default": 50}})),
        tool("sitebox_doctor", "Report drift: registry ↔ folders ↔ manifests ↔ ports.", json!({})),
        tool("sitebox_deploy", "Mirror a site's public folder to a local path and verify sha256.", json!({"id": {"type":"string"}, "to": {"type":"string"}, "dry_run": {"type":"boolean"}})),
        tool("sitebox_new", "Scaffold a new static site with a manifest and register it.", json!({"id": {"type":"string"}, "name": {"type":"string"}, "port": {"type":"integer"}, "description": {"type":"string"}})),
        tool("sitebox_ports_check", "Check whether a port is free.", json!({"port": {"type":"integer"}})),
        tool("sitebox_self_test", "Run the negative controls that prove the checks can fail.", json!({})),
    ]
    .into_iter()
    .map(|mut t| {
        t["description"] = json!(format!("{} Root: {root_note}.", t["description"].as_str().unwrap_or("")));
        t
    })
    .collect()
}

fn tool(name: &str, desc: &str, props: Value) -> Value {
    json!({
        "name": name,
        "description": desc,
        "inputSchema": { "type": "object", "properties": props, "additionalProperties": false }
    })
}

async fn call(root: &PathBuf, sup: &Arc<Supervisor>, name: &str, args: Value) -> Result<Value> {
    let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let need_manifest = |id: &str| -> Result<Manifest> {
        if id.is_empty() {
            anyhow::bail!("`id` is required");
        }
        crate::supervisor::load_manifest(root, id)
    };

    match name {
        "sitebox_list" => {
            let mut out = Vec::new();
            for p in crate::registry::manifests(root)? {
                let site_dir = p.parent().unwrap_or(&p).to_path_buf();
                match Manifest::load(&site_dir) {
                    Ok(m) => out.push(json!({
                        "id": m.site.id, "name": m.site.name, "port": m.site.port,
                        "serve": m.site.serve, "path": m.site.path,
                        "manifest": true,
                        "running": sup.is_running(&m.site.id).await,
                        "pipeline_steps": m.pipeline.len(),
                    })),
                    Err(e) => out.push(json!({"manifest": false, "error": format!("{e:#}"), "file": p.display().to_string()})),
                }
            }
            Ok(json!({ "root": root.display().to_string(), "sites": out, "count": out.len() }))
        }
        "sitebox_get" => Ok(serde_json::to_value(need_manifest(id)?)?),
        "sitebox_check" => {
            let m = need_manifest(id)?;
            let rep = checks::run_static(root, &m)?;
            Ok(serde_json::to_value(rep)?)
        }
        "sitebox_verify" => {
            let m = need_manifest(id)?;
            let auto = args
                .get("auto_start")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let started = if auto && !util::port_bound(m.site.port) {
                Some(sup.start(&m).await?)
            } else {
                None
            };
            let base = format!("http://127.0.0.1:{}", m.site.port);
            let rep = checks::run_http(&base, &sup_http_client()).await;
            if let Some(s) = started {
                if s.ok {
                    sup.stop(&m.site.id).await.ok();
                }
            }
            Ok(serde_json::to_value(rep)?)
        }
        "sitebox_plan" => {
            let m = need_manifest(id)?;
            Ok(serde_json::to_value(pipeline::plan(&m, root)?)?)
        }
        "sitebox_build" => {
            let m = need_manifest(id)?;
            let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
            let step = args.get("step").and_then(|v| v.as_str());
            Ok(serde_json::to_value(pipeline::apply(
                &m, root, force, step,
            )?)?)
        }
        "sitebox_up" => {
            let m = need_manifest(id)?;
            Ok(serde_json::to_value(sup.start(&m).await?)?)
        }
        "sitebox_down" => {
            sup.stop(id).await?;
            Ok(json!({"ok": true, "id": id, "message": "stopped"}))
        }
        "sitebox_restart" => {
            let m = need_manifest(id)?;
            Ok(serde_json::to_value(sup.restart(&m).await?)?)
        }
        "sitebox_logs" => {
            let n = args.get("lines").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            Ok(json!({ "id": id, "lines": sup.logs(id, n.min(500)).await }))
        }
        "sitebox_doctor" => Ok(doctor(root, sup).await),
        "sitebox_deploy" => {
            let m = need_manifest(id)?;
            let to = args.get("to").and_then(|v| v.as_str()).unwrap_or("");
            if to.is_empty() {
                anyhow::bail!("`to` is required (local folder or user@host:/path)");
            }
            let dry = args
                .get("dry_run")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let src = m.public_dir(root);
            let rep = if to.contains(':') {
                deploy::deploy_ssh(&src, to, dry)?
            } else {
                deploy::deploy_local(&src, std::path::Path::new(to), dry)?
            };
            Ok(serde_json::to_value(rep)?)
        }
        "sitebox_new" => {
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or(id);
            let port = args.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let desc = args
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let dir = pipeline::scaffold(root, id, name, port, desc)?;
            Ok(json!({"ok": true, "id": id, "dir": dir.display().to_string()}))
        }
        "sitebox_ports_check" => {
            let port = args.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let used = util::port_bound(port);
            Ok(json!({"port": port, "used": used, "available": !used}))
        }
        "sitebox_self_test" => Ok(serde_json::to_value(checks::self_test())?),
        other => anyhow::bail!("unknown tool `{other}`"),
    }
}

fn sup_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_gzip()
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .unwrap_or_default()
}

pub async fn doctor(root: &PathBuf, sup: &Supervisor) -> Value {
    let mut problems = Vec::new();
    let cfg = crate::registry::SiteConfig::read(root);
    let cfg = match cfg {
        Ok(c) => c,
        Err(e) => {
            return json!({"ok": false, "problems": [format!("registry unreadable: {e:#}")]});
        }
    };
    let manifests = crate::registry::manifests(root).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    for p in &manifests {
        let dir = p.parent().unwrap_or(p);
        match Manifest::load(dir) {
            Ok(m) => {
                seen.insert(m.site.id.clone());
                if !m.public_dir(root).is_dir() {
                    problems.push(format!(
                        "{}: public dir missing ({})",
                        m.site.id,
                        m.public_dir(root).display()
                    ));
                }
                if let Some(reg) = cfg.find(&m.site.id) {
                    if reg.port != m.site.port {
                        problems.push(format!(
                            "{}: port {} in manifest vs {} in sites.json",
                            m.site.id, m.site.port, reg.port
                        ));
                    }
                } else {
                    problems.push(format!(
                        "{}: manifest exists but site is not registered",
                        m.site.id
                    ));
                }
            }
            Err(e) => problems.push(format!("{}: {e:#}", p.display())),
        }
    }
    for s in &cfg.sites {
        if !seen.contains(&s.id) {
            // Legacy sites are supported: the registry entry plus server.js is
            // enough to start them. Not a problem, just worth knowing.
            let legacy = root.join(&s.path).join("server.js").is_file();
            if !legacy {
                problems.push(format!(
                    "{}: registered but has no sitebox.toml or server.js",
                    s.id
                ));
            }
        }
        if !root.join(&s.path).is_dir() {
            problems.push(format!("{}: folder missing ({})", s.id, s.path));
        }
    }
    json!({
        "ok": problems.is_empty(),
        "root": root.display().to_string(),
        "registered": cfg.sites.len(),
        "manifests": manifests.len(),
        "running": sup.running_ids().await,
        "problems": problems,
    })
}
