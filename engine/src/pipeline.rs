//! Pipeline planning and execution.
//!
//! The order is load-bearing: a step that reads an earlier step's output must
//! run after it. The engine enforces that by refusing to run a step whose
//! declared inputs do not exist (unless the step produces them), and by
//! fingerprinting inputs so a stale output is visible instead of silently
//! reused.

use anyhow::{bail, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::assets::{self, AssetRules};
use crate::manifest::{Manifest, PipelineStep};
use crate::util;

#[derive(Debug, Clone, Serialize)]
pub struct StepPlan {
    pub name: String,
    pub kind: String,
    pub command: Option<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub inputs_ready: bool,
    pub missing_inputs: Vec<String>,
    pub fingerprint: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepResult {
    pub name: String,
    pub status: String,
    pub ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
}

fn fingerprint(site_dir: &Path, step: &PipelineStep) -> String {
    let mut parts: Vec<String> = Vec::new();
    for i in &step.inputs {
        let p = site_dir.join(i);
        if p.is_file() {
            if let Ok(h) = util::sha256_file(&p) {
                parts.push(format!("{i}:{h}"));
            }
        } else if p.is_dir() {
            let mut acc = Vec::new();
            for e in walkdir::WalkDir::new(&p).max_depth(4).sort_by_file_name() {
                let Ok(e) = e else { continue };
                if e.file_type().is_file() {
                    let rel = e.path().strip_prefix(site_dir).unwrap_or(e.path());
                    if let Ok(h) = util::sha256_file(e.path()) {
                        acc.push(format!("{}:{}", rel.display(), h));
                    }
                }
            }
            parts.push(format!(
                "{i}[{h}]",
                h = util::sha256_bytes(acc.join(",").as_bytes())
            ));
        }
    }
    if let Some(r) = &step.rules {
        parts.push(format!("rules:{r}"));
    }
    // The command itself is part of the contract: editing a generator must
    // invalidate the step even when its declared inputs did not change.
    if let Some(run) = &step.run {
        parts.push(format!("run:{run}"));
    }
    if let Some(kind) = &step.kind {
        parts.push(format!("kind:{kind}"));
    }
    util::sha256_bytes(parts.join("|").as_bytes())
}

pub fn plan(m: &Manifest, root: &Path) -> Result<Vec<StepPlan>> {
    let site_dir = m.site_dir(root);
    let fp_path = site_dir.join(".sitebox/fingerprints.json");
    let stored: std::collections::BTreeMap<String, String> = if fp_path.is_file() {
        serde_json::from_str(&std::fs::read_to_string(&fp_path)?).unwrap_or_default()
    } else {
        Default::default()
    };

    let mut out = Vec::new();
    let mut produced: Vec<String> = Vec::new();
    for step in &m.pipeline {
        let mut missing = Vec::new();
        for i in &step.inputs {
            if !site_dir.join(i).exists() && !produced.iter().any(|p| p == i) {
                missing.push(i.clone());
            }
        }
        let fp = fingerprint(&site_dir, step);
        let changed = stored.get(&step.name) != Some(&fp);
        out.push(StepPlan {
            name: step.name.clone(),
            kind: step.kind.clone().unwrap_or_else(|| "run".into()),
            command: step.run.clone(),
            inputs: step.inputs.clone(),
            outputs: step.outputs.clone(),
            inputs_ready: missing.is_empty(),
            missing_inputs: missing,
            fingerprint: fp,
            changed,
        });
        produced.extend(step.outputs.iter().cloned());
    }
    Ok(out)
}

pub fn apply(
    m: &Manifest,
    root: &Path,
    force: bool,
    only: Option<&str>,
) -> Result<Vec<StepResult>> {
    let site_dir = m.site_dir(root);
    let plans = plan(m, root)?;
    let fp_path = site_dir.join(".sitebox/fingerprints.json");
    let mut stored: std::collections::BTreeMap<String, String> = if fp_path.is_file() {
        serde_json::from_str(&std::fs::read_to_string(&fp_path)?).unwrap_or_default()
    } else {
        Default::default()
    };
    let mut results = Vec::new();

    for (step, p) in m.pipeline.iter().zip(plans.iter()) {
        if let Some(want) = only {
            if step.name != want {
                continue;
            }
        }
        if !p.inputs_ready {
            results.push(StepResult {
                name: step.name.clone(),
                status: "blocked".into(),
                ms: 0,
                detail: Some(format!("missing inputs: {}", p.missing_inputs.join(", "))),
                findings: vec![],
            });
            continue;
        }
        if !force && !p.changed && p.outputs.iter().all(|o| site_dir.join(o).exists()) {
            results.push(StepResult {
                name: step.name.clone(),
                status: "skip".into(),
                ms: 0,
                detail: Some("inputs unchanged".into()),
                findings: vec![],
            });
            continue;
        }

        let started = std::time::Instant::now();
        let outcome: Result<Option<String>> = match step.kind.as_deref() {
            Some("images") => run_images(step, &site_dir, root, m, force),
            Some(other) => Err(anyhow::anyhow!("unknown built-in stage `{other}`")),
            None => match &step.run {
                Some(cmd) => {
                    let (code, text) = util::run_shell(&site_dir, cmd)?;
                    if code != 0 {
                        Err(anyhow::anyhow!("exit {code}: {}", tail(&text, 12)))
                    } else {
                        Ok(Some(tail(&text, 6)))
                    }
                }
                None => Err(anyhow::anyhow!("step has neither kind nor run")),
            },
        };

        match outcome {
            Ok(detail) => {
                stored.insert(step.name.clone(), p.fingerprint.clone());
                results.push(StepResult {
                    name: step.name.clone(),
                    status: "ok".into(),
                    ms: started.elapsed().as_millis(),
                    detail,
                    findings: vec![],
                });
            }
            Err(e) => results.push(StepResult {
                name: step.name.clone(),
                status: "fail".into(),
                ms: started.elapsed().as_millis(),
                detail: Some(format!("{e:#}")),
                findings: vec![],
            }),
        }
    }

    if let Some(parent) = fp_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(fp_path, serde_json::to_string_pretty(&stored)?)?;
    Ok(results)
}

fn run_images(
    step: &PipelineStep,
    site_dir: &Path,
    root: &Path,
    m: &Manifest,
    force: bool,
) -> Result<Option<String>> {
    let input = step
        .inputs
        .first()
        .map(|s| site_dir.join(s))
        .unwrap_or_else(|| site_dir.join("assets-raw"));
    let out_rel = step
        .outputs
        .first()
        .cloned()
        .unwrap_or_else(|| format!("{}/img", m.site.public_dir));
    let out_rel = out_rel
        .strip_prefix(&format!("{}/", m.site.public_dir))
        .unwrap_or(&out_rel)
        .to_string();

    let mut rules = AssetRules {
        force,
        ..Default::default()
    };
    if let Some(toml::Value::Table(t)) = &step.rules {
        if let Some(v) = t.get("max_edge").and_then(|v| v.as_integer()) {
            rules.max_edge = v.max(1) as u32;
        }
        if let Some(v) = t.get("quality").and_then(|v| v.as_float()) {
            rules.quality = v as f32;
        }
        if let Some(v) = t.get("square_avatars").and_then(|v| v.as_bool()) {
            rules.square_avatars = v;
        }
    }
    let dims = site_dir.join("synd/dims.json");
    let rep = assets::build_assets(&input, &m.public_dir(root), &out_rel, &dims, &rules)?;
    Ok(Some(format!(
        "{} files → {} written, {} skipped, {} failed, {} → {}",
        rep.inputs,
        rep.written,
        rep.skipped,
        rep.failed,
        util::human_bytes(rep.bytes_in),
        util::human_bytes(rep.bytes_out)
    )))
}

fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Build the ordered steps for a brand-new static site.
pub fn scaffold(
    root: &Path,
    id: &str,
    name: &str,
    port: u16,
    description: &str,
) -> Result<PathBuf> {
    crate::registry::validate_id(id)?;
    let dir = root.join("sites").join(id);
    if dir.exists() {
        bail!("site folder already exists: {}", dir.display());
    }
    std::fs::create_dir_all(dir.join("public"))?;
    let m = Manifest {
        schema: crate::manifest::SCHEMA_VERSION,
        site: crate::manifest::SiteMeta {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            port,
            category: "tools".into(),
            icon: "globe".into(),
            icon_color: "#4E5C6E".into(),
            path: format!("sites/{id}"),
            public_dir: "public".into(),
            serve: "static".into(),
            api_upstream: None,
            created: util::today(),
        },
        pipeline: vec![],
        checks: vec![],
    };
    m.save(&dir)?;
    std::fs::write(
        dir.join("public/index.html"),
        format!(
            "<!doctype html>\n<html lang=\"th\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{name}</title>\n<link rel=\"icon\" href=\"favicon.svg\">\n<style>body{{font:16px/1.6 system-ui;margin:0;padding:48px;max-width:46rem}}</style>\n</head>\n<body>\n<h1>{name}</h1>\n<p>{description}</p>\n</body>\n</html>\n"
        ),
    )?;
    std::fs::write(
        dir.join("public/favicon.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\"><rect width=\"32\" height=\"32\" rx=\"7\" fill=\"#0f1115\"/><circle cx=\"16\" cy=\"16\" r=\"7\" fill=\"#7dd3fc\"/></svg>\n",
    )?;

    // Register it in sites.json so the dashboard sees it immediately.
    let mut cfg = crate::registry::SiteConfig::read(root).unwrap_or_default();
    if cfg.sites.iter().any(|s| s.id == id) {
        bail!("id {id} is already registered");
    }
    if cfg.sites.iter().any(|s| s.port == port) {
        bail!("port {port} is already assigned");
    }
    cfg.sites.push(crate::registry::Site {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        icon: "globe".into(),
        icon_color: "#4E5C6E".into(),
        port,
        category: "tools".into(),
        url: format!("http://localhost:{port}"),
        path: format!("sites/{id}"),
        created: util::today(),
        _auto: None,
    });
    cfg.write(root)?;
    Ok(dir)
}
