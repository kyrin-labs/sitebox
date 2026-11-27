//! `sitebox` — the agent-facing CLI.
//!
//! Every command answers JSON with `--json`, and exit codes are stable:
//!   0  ok
//!   1  operational failure (missing site, build failed, deploy mismatch)
//!   2  checks ran and failed (the work is not done)
//!   64 bad usage

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sitebox_rs::checks;
use sitebox_rs::dashboard::{self, DashState};
use sitebox_rs::manifest::Manifest;
use sitebox_rs::registry::SiteConfig;
use sitebox_rs::serve::{self, ServeState};
use sitebox_rs::supervisor::{self, Supervisor};
use sitebox_rs::{deploy, pipeline, util};

#[derive(Parser, Debug)]
#[command(name = "sitebox", version, about = "Agent-native engine for SiteBox")]
struct Cli {
    /// SiteBox root (defaults to $SITEBOX_ROOT or ~/sitebox)
    #[arg(long, global = true, env = "SITEBOX_ROOT")]
    root: Option<PathBuf>,
    /// Machine-readable output
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Import the Node registry into manifests, without touching the source.
    Import {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        work: PathBuf,
    },
    /// Snapshot hashes and sizes for the site set (P0 golden fixtures).
    Golden {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Comma-separated ids (default: every registered site)
        #[arg(long)]
        ids: Option<String>,
    },
    /// Print the JSON Schema of sitebox.toml.
    Schema,
    /// List sites with manifest/running status.
    List,
    /// Print one manifest.
    Get {
        id: String,
    },
    /// Run the static check catalog.
    Check {
        id: String,
        /// Also run HTTP checks against the site's port.
        #[arg(long)]
        http: bool,
    },
    /// Run HTTP checks, starting the site for the duration if needed.
    Verify {
        id: String,
        #[arg(long, default_value_t = true)]
        auto_start: bool,
        /// Check this base URL instead of the manifest port (e.g. a test listener)
        #[arg(long)]
        url: Option<String>,
    },
    /// Show the pipeline and which steps would run.
    Plan {
        id: String,
    },
    /// Run the pipeline.
    Build {
        id: String,
        #[arg(long)]
        step: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Serve one site's public folder.
    Serve {
        id: String,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Start / stop / restart through the in-process supervisor.
    Up {
        id: String,
    },
    Down {
        id: String,
    },
    Restart {
        id: String,
    },
    /// Read the last log lines.
    Logs {
        id: String,
        #[arg(short = 'n', long, default_value_t = 50)]
        lines: usize,
    },
    /// Mirror a site's public folder and verify hashes.
    Deploy {
        id: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Registry ↔ folders ↔ manifests ↔ ports.
    Doctor,
    /// Scaffold and register a new static site.
    New {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        port: u16,
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Port availability.
    Ports {
        #[command(subcommand)]
        cmd: PortsCmd,
    },
    /// Run the negative controls that prove each check can fail.
    SelfTest,
    /// Run the Rust dashboard API + the existing UI.
    Dashboard {
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        ui: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum PortsCmd {
    Check { port: u16 },
}

fn home_root() -> PathBuf {
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join("sitebox"))
        .unwrap_or_else(|_| PathBuf::from("/home/l2s/sitebox"))
}

fn main() {
    let cli = Cli::parse();
    let root = cli.root.clone().unwrap_or_else(home_root);
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let code = match rt.block_on(run(cli, root)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

async fn run(cli: Cli, root: PathBuf) -> Result<i32> {
    let json_out = cli.json;
    match cli.cmd {
        Cmd::Import { source, work } => cmd_import(&source, &work, json_out),
        Cmd::Golden { source, out, ids } => cmd_golden(&source, &out, ids.as_deref(), json_out),
        Cmd::Schema => {
            let s = Manifest::json_schema();
            println!("{}", serde_json::to_string_pretty(&s)?);
            Ok(0)
        }
        Cmd::List => cmd_list(&root, json_out),
        Cmd::Get { id } => {
            let m = load(&root, &id)?;
            print_value(&serde_json::to_value(m)?, json_out);
            Ok(0)
        }
        Cmd::Check { id, http } => {
            let m = load(&root, &id)?;
            let mut rep = checks::run_static(&root, &m)?;
            if http {
                let base = format!("http://127.0.0.1:{}", m.site.port);
                let client = reqwest::Client::builder()
                    .no_gzip()
                    .timeout(std::time::Duration::from_secs(4))
                    .build()?;
                let h = checks::run_http(&base, &client).await;
                rep.findings.extend(h.findings);
                rep.passed += h.passed;
                rep.failed += h.failed;
                rep.skipped += h.skipped;
            }
            print_report(&rep, json_out)?;
            Ok(if rep.ok() { 0 } else { 2 })
        }
        Cmd::Verify {
            id,
            auto_start,
            url,
        } => cmd_verify(&root, &id, auto_start, url.as_deref(), json_out).await,
        Cmd::Plan { id } => {
            let m = load(&root, &id)?;
            print_value(&serde_json::to_value(pipeline::plan(&m, &root)?)?, json_out);
            Ok(0)
        }
        Cmd::Build { id, step, force } => {
            let m = load(&root, &id)?;
            let res = pipeline::apply(&m, &root, force, step.as_deref())?;
            let failed = res
                .iter()
                .any(|r| r.status == "fail" || r.status == "blocked");
            print_value(&serde_json::to_value(&res)?, json_out);
            Ok(if failed { 1 } else { 0 })
        }
        Cmd::Serve { id, port } => {
            let m = load(&root, &id)?;
            let state = ServeState {
                root: m.public_dir(&root),
                site_id: m.site.id.clone(),
                api_upstream: m.site.api_upstream.clone(),
                client: reqwest::Client::new(),
                // `sitebox serve` is a standalone site process with no
                // dashboard to call home, so no launcher is injected.
                sols: None,
            };
            let port = port.unwrap_or(m.site.port);
            serve::serve(port, state).await?;
            Ok(0)
        }
        Cmd::Up { id } => {
            let m = load(&root, &id)?;
            let r = Supervisor::new(root.clone()).start(&m).await?;
            print_value(&serde_json::to_value(&r)?, json_out);
            Ok(if r.ok { 0 } else { 1 })
        }
        Cmd::Down { id } => {
            Supervisor::new(root.clone()).stop(&id).await?;
            println!(
                "{id}: stopped (note: a cold CLI start cannot stop a process owned by siteboxd)"
            );
            Ok(0)
        }
        Cmd::Restart { id } => {
            let m = load(&root, &id)?;
            let r = Supervisor::new(root.clone()).restart(&m).await?;
            print_value(&serde_json::to_value(&r)?, json_out);
            Ok(if r.ok { 0 } else { 1 })
        }
        Cmd::Logs { id, lines } => cmd_logs(&root, &id, lines, json_out),
        Cmd::Deploy { id, to, dry_run } => {
            let m = load(&root, &id)?;
            let src = m.public_dir(&root);
            let rep = if to.contains(':') {
                deploy::deploy_ssh(&src, &to, dry_run)?
            } else {
                deploy::deploy_local(&src, Path::new(&to), dry_run)?
            };
            print_value(&serde_json::to_value(rep)?, json_out);
            Ok(0)
        }
        Cmd::Doctor => {
            let sup = Supervisor::new(root.clone());
            let v = sitebox_rs::mcp::doctor(&root, &sup).await;
            print_value(&v, json_out);
            Ok(if v["ok"].as_bool().unwrap_or(false) {
                0
            } else {
                2
            })
        }
        Cmd::New {
            id,
            name,
            port,
            description,
        } => {
            let name = name.unwrap_or_else(|| id.clone());
            let dir = pipeline::scaffold(&root, &id, &name, port, &description)?;
            print_value(
                &json!({"ok": true, "id": id, "dir": dir.display().to_string()}),
                json_out,
            );
            Ok(0)
        }
        Cmd::Ports {
            cmd: PortsCmd::Check { port },
        } => {
            let used = util::port_bound(port);
            print_value(
                &json!({"port": port, "used": used, "available": !used}),
                json_out,
            );
            Ok(0)
        }
        Cmd::SelfTest => {
            let r = checks::self_test();
            let failed = r
                .iter()
                .filter(|f| f.status == checks::Status::Fail)
                .count();
            print_value(&serde_json::to_value(&r)?, json_out);
            Ok(if failed == 0 { 0 } else { 2 })
        }
        Cmd::Dashboard { port, ui } => {
            let port = port.unwrap_or(4445);
            let ui = ui.unwrap_or_else(|| root.join("dashboard/public"));
            let sup = Arc::new(Supervisor::new(root.clone()));
            let client = reqwest::Client::new();
            let studio = sitebox_rs::studio::StudioState::new(root.clone(), sup.clone(), client.clone());
            let state = DashState {
                root: root.clone(),
                ui,
                sup,
                client,
                registry_error: Default::default(),
                studio,
            };
            dashboard::serve(port, state).await?;
            Ok(0)
        }
    }
}

fn load(root: &Path, id: &str) -> Result<Manifest> {
    supervisor::load_manifest(root, id)
}

fn print_value(v: &Value, _json_out: bool) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn print_report(rep: &checks::CheckReport, json_out: bool) -> Result<()> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(rep)?);
        return Ok(());
    }
    for f in &rep.findings {
        let mark = match f.status {
            checks::Status::Pass => "PASS",
            checks::Status::Fail => "FAIL",
            checks::Status::Skip => "SKIP",
            checks::Status::Info => "INFO",
        };
        println!("{mark}  {:<6} {}", f.check, f.title);
        if let Some(e) = &f.evidence {
            if f.status != checks::Status::Pass {
                println!("        evidence: {e}");
            }
        }
        if let Some(fix) = &f.fix {
            if f.status == checks::Status::Fail {
                println!("        fix: {fix}");
            }
        }
    }
    println!(
        "\n{}  passed={} failed={} skipped={}",
        rep.site, rep.passed, rep.failed, rep.skipped
    );
    Ok(())
}

fn cmd_list(root: &Path, json_out: bool) -> Result<i32> {
    let mut out = Vec::new();
    for p in sitebox_rs::registry::manifests(root)? {
        let dir = p.parent().unwrap_or(&p);
        match Manifest::load(dir) {
            Ok(m) => out.push(json!({
                "id": m.site.id, "name": m.site.name, "port": m.site.port,
                "serve": m.site.serve, "path": m.site.path,
                "public": m.public_dir(root).display().to_string(),
                "public_exists": m.public_dir(root).is_dir(),
                "pipeline_steps": m.pipeline.len(),
            })),
            Err(e) => out.push(json!({"file": p.display().to_string(), "error": format!("{e:#}")})),
        }
    }
    let v = json!({"root": root.display().to_string(), "count": out.len(), "sites": out});
    if json_out {
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        for s in v["sites"].as_array().cloned().unwrap_or_default() {
            println!(
                "{:<18} {:<22} :{:<5} {:<7} {}",
                s["id"].as_str().unwrap_or("?"),
                s["name"].as_str().unwrap_or(""),
                s["port"],
                s["serve"].as_str().unwrap_or(""),
                if s["public_exists"].as_bool().unwrap_or(false) {
                    ""
                } else {
                    "PUBLIC MISSING"
                }
            );
        }
        println!("\n{} site(s) under {}", v["count"], root.display());
    }
    Ok(0)
}

async fn cmd_verify(
    root: &Path,
    id: &str,
    auto_start: bool,
    url: Option<&str>,
    json_out: bool,
) -> Result<i32> {
    let m = load(root, id)?;
    let sup = Supervisor::new(root.to_path_buf());
    let started = if url.is_none() && auto_start && !util::port_bound(m.site.port) {
        Some(sup.start(&m).await?)
    } else {
        None
    };
    let base = url
        .map(|u| u.to_string())
        .unwrap_or_else(|| format!("http://127.0.0.1:{}", m.site.port));
    let client = reqwest::Client::builder()
        .no_gzip()
        .timeout(std::time::Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let rep = checks::run_http(&base, &client).await;
    if let Some(s) = &started {
        if s.ok {
            sup.stop(&m.site.id).await.ok();
        }
    }
    print_report(&rep, json_out)?;
    Ok(if rep.ok() { 0 } else { 2 })
}

fn cmd_logs(root: &Path, id: &str, lines: usize, json_out: bool) -> Result<i32> {
    let path = root.join("logs").join(format!("{id}.jsonl"));
    if !path.is_file() {
        bail!("no logs at {}", path.display());
    }
    let body = std::fs::read_to_string(&path)?;
    let all: Vec<Value> = body
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let take: Vec<Value> = all.iter().rev().take(lines).rev().cloned().collect();
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"id": id, "lines": take}))?
        );
    } else {
        for l in &take {
            println!(
                "{} [{}] {}",
                l["t"].as_str().unwrap_or(""),
                l["stream"].as_str().unwrap_or(""),
                l["text"].as_str().unwrap_or("")
            );
        }
    }
    Ok(0)
}

fn cmd_import(source: &Path, work: &Path, json_out: bool) -> Result<i32> {
    let mut cfg = SiteConfig::read(source).with_context(|| {
        format!(
            "read {}",
            source.join(sitebox_rs::registry::REGISTRY_REL).display()
        )
    })?;
    std::fs::create_dir_all(work.join("dashboard/data"))?;
    std::fs::create_dir_all(work.join("sites"))?;

    let mut imported = Vec::new();
    for site in &mut cfg.sites {
        let abs = source.join(&site.path);
        let server = abs.join("server.js");
        let has_api = std::fs::read_to_string(&server)
            .map(|t| t.contains("/api/"))
            .unwrap_or(false);
        let mut m = Manifest::from_site(site);
        m.site.path = abs.to_string_lossy().to_string();
        m.site.serve = if has_api {
            "node".into()
        } else {
            "static".into()
        };
        let dir = work.join("sites").join(&site.id);
        std::fs::create_dir_all(&dir)?;
        m.save(&dir)?;
        imported.push(json!({
            "id": site.id, "serve": m.site.serve,
            "public_exists": m.public_dir(work).is_dir(),
            "has_server_js": server.is_file(),
        }));
        site.path = abs.to_string_lossy().to_string();
    }
    // The work registry points at the original folders; nothing under `source`
    // is written by this command.
    cfg.dashboard.name = cfg.dashboard.name.clone();
    cfg.write(work)?;

    let v = json!({
        "source": source.display().to_string(),
        "work": work.display().to_string(),
        "sites": imported.len(),
        "static": imported.iter().filter(|s| s["serve"] == "static").count(),
        "node": imported.iter().filter(|s| s["serve"] == "node").count(),
        "details": imported,
    });
    print_value(&v, json_out);
    Ok(0)
}

fn cmd_golden(source: &Path, out: &Path, ids: Option<&str>, json_out: bool) -> Result<i32> {
    let cfg = SiteConfig::read(source)?;
    let wanted: Option<Vec<&str>> = ids.map(|s| s.split(',').map(|x| x.trim()).collect());
    let mut rows = Vec::new();
    for site in &cfg.sites {
        if let Some(w) = &wanted {
            if !w.contains(&site.id.as_str()) {
                continue;
            }
        }
        let dir = source.join(&site.path);
        let public = dir.join("public");
        let mut files = 0usize;
        let mut bytes = 0u64;
        for e in walkdir::WalkDir::new(&public) {
            let Ok(e) = e else { continue };
            if e.file_type().is_file() {
                files += 1;
                bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        let index = public.join("index.html");
        let dims = dir.join("synd/dims.json");
        let data = dir.join(format!("synd/{}-data.json", site.id));
        rows.push(json!({
            "id": site.id,
            "port": site.port,
            "path": site.path,
            "files": files,
            "bytes": bytes,
            "index_sha256": if index.is_file() { util::sha256_file(&index).ok() } else { None },
            "dims_sha256": if dims.is_file() { util::sha256_file(&dims).ok() } else { None },
            "data_json": data.file_name().map(|n| n.to_string_lossy().to_string()),
            "data_sha256": if data.is_file() { util::sha256_file(&data).ok() } else { None },
        }));
    }
    std::fs::create_dir_all(out)?;
    let payload = json!({
        "captured_at": util::now_iso(),
        "source": source.display().to_string(),
        "sites": rows,
    });
    std::fs::write(
        out.join("golden.json"),
        serde_json::to_string_pretty(&payload)? + "\n",
    )?;
    print_value(&payload, json_out);
    Ok(0)
}
