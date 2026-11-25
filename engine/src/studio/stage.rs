//! Stage a patch in a throwaway copy and run the static checks there.
//!
//! Staging uses hard links for the untouched files, so a site with 354 MB of
//! audio costs one directory walk, not a 354 MB copy. The one file being edited
//! is unlinked and rewritten, which is why the real folder is never touched.
//!
//! A site can be red before the edit — a CDN link, a missing width/height that
//! predates the patch. Judging the patch by the absolute check result would make
//! every edit to such a site roll back, so the gate compares the *set* of failing
//! checks before and after and only the new ones count.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

use super::patch::PatchSpec;
use super::{new_id, site_path, SiteRef, StudioState};
use crate::checks;

#[derive(Debug, Serialize)]
pub struct StageReport {
    /// True when the patch introduced no new failing check.
    pub ok: bool,
    pub mode: String,
    pub findings: Vec<checks::Finding>,
    pub baseline_ok: bool,
    pub pre_existing: Vec<String>,
    pub introduced: Vec<String>,
    pub added: usize,
    pub removed: usize,
}

pub fn fail_ids(report: &checks::CheckReport) -> BTreeSet<String> {
    report
        .findings
        .iter()
        .filter(|f| f.status == checks::Status::Fail)
        .map(|f| f.check.clone())
        .collect()
}

pub fn verify(st: &StudioState, site: &SiteRef, spec: &PatchSpec) -> Result<(StageReport, String)> {
    let id = new_id("st");
    let root = st.data.join("stage").join(&id);
    let stage_site = root.join("site");
    let out = (|| -> Result<(StageReport, String)> {
        // Baseline first: the live site is only read, never written.
        let baseline = checks::run_static(&st.root, &site.manifest)?;
        let baseline_fails = fail_ids(&baseline);

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
        let staged_fails = fail_ids(&report);

        let introduced: Vec<String> = staged_fails.difference(&baseline_fails).cloned().collect();
        let pre_existing: Vec<String> = baseline_fails.iter().cloned().collect();
        let mode = if site.manifest.pipeline.is_empty() {
            "static".to_string()
        } else {
            "static-only (pipeline steps were not run in staging)".to_string()
        };
        let out = StageReport {
            ok: introduced.is_empty(),
            mode,
            findings: report.findings.clone(),
            baseline_ok: baseline.ok(),
            pre_existing,
            introduced,
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

/// Summarise a finding list the way the dashboard shows it: what this patch
/// introduced first, then what was already red before it.
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
                failures.push(json!({
                    "check": f.check,
                    "title": f.title,
                    "evidence": f.evidence,
                    "fix": f.fix,
                    "introduced": report.introduced.contains(&f.check),
                }));
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
        "baseline_ok": report.baseline_ok,
        "introduced": report.introduced,
        "pre_existing": report.pre_existing,
        "failures": failures,
    })
}
