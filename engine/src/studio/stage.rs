//! Stage a patch in a throwaway copy and run the static checks there.
//!
//! Staging uses hard links for the untouched files, so a site with 354 MB of
//! audio costs one directory walk, not a 354 MB copy. The one file being edited
//! is unlinked and rewritten, which is why the real folder is never touched.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

use super::patch::PatchSpec;
use super::{new_id, site_path, SiteRef, StudioState};
use crate::checks;

#[derive(Debug, Serialize)]
pub struct StageReport {
    pub ok: bool,
    pub mode: String,
    pub findings: Vec<checks::Finding>,
    pub added: usize,
    pub removed: usize,
}

pub fn verify(st: &StudioState, site: &SiteRef, spec: &PatchSpec) -> Result<(StageReport, String)> {
    let id = new_id("st");
    let root = st.data.join("stage").join(&id);
    let stage_site = root.join("site");
    let out = (|| -> Result<(StageReport, String)> {
        super::ensure_dir(&stage_site)?;

        copy_links(&site.dir, &stage_site)?;

        let target =
            site_path(&stage_site, &spec.path).with_context(|| format!("stage {}", spec.path))?;
        let text = std::fs::read_to_string(&target)
            .map_err(|_| anyhow::anyhow!("{} is not UTF-8 text", spec.path))?;
        let next = super::patch::apply_text(&text, &spec.old, &spec.new, spec.count)?;
        // Break the hard link before writing: writing in place would edit the live file.
        std::fs::remove_file(&target)?;
        std::fs::write(&target, next)?;

        let mut m = site.manifest.clone();
        m.site.path = "site".into();
        let report = checks::run_static(&root, &m)?;

        let mode = if site.manifest.pipeline.is_empty() {
            "static".to_string()
        } else {
            "static-only (pipeline steps were not run in staging)".to_string()
        };
        let out = StageReport {
            ok: report.ok(),
            mode,
            findings: report.findings.clone(),
            added: spec.new.lines().count(),
            removed: spec.old.lines().count(),
        };
        Ok((out, id.clone()))
    })();
    let _ = std::fs::remove_dir_all(&root);
    out
}

fn copy_links(from: &Path, to: &Path) -> Result<()> {
    for e in walkdir::WalkDir::new(from)
        .max_depth(8)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(e.file_type().is_dir()
                && (n == ".git" || n == "node_modules" || n == ".sitebox" || n == ".snapshots"))
        })
        .flatten()
    {
        let rel = match e.path().strip_prefix(from) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let dest = to.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else if e.file_type().is_file() {
            if let Some(p) = dest.parent() {
                std::fs::create_dir_all(p)?;
            }
            // Hard link, fall back to a copy across filesystems.
            if std::fs::hard_link(e.path(), &dest).is_err() {
                std::fs::copy(e.path(), &dest)?;
            }
        }
    }
    Ok(())
}

/// Summarise a finding list the way the dashboard shows it: counts first, then
/// only the failures, because a chat message is not a check log.
pub fn summarise(report: &StageReport) -> Value {
    let mut pass = 0;
    let mut fail = 0;
    let mut skip = 0;
    let mut failures = Vec::new();
    for f in &report.findings {
        match f.status {
            checks::Status::Pass => pass += 1,
            checks::Status::Fail => {
                fail += 1;
                failures.push(json!({"check": f.check, "title": f.title, "evidence": f.evidence, "fix": f.fix}));
            }
            checks::Status::Skip => skip += 1,
            checks::Status::Info => {}
        }
    }
    json!({
        "ok": report.ok,
        "mode": report.mode,
        "pass": pass,
        "fail": fail,
        "skip": skip,
        "failures": failures,
    })
}
