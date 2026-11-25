//! Apply and undo. This is the only code path in Studio that writes a site file.
//!
//! Order is not negotiable: snapshot → write → build → check → (restore on red).
//! A check that fails after the build restores the sources *and* rebuilds, so the
//! served page and the source tree never disagree.

use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::io::Write;

use super::{patch, site_path, site_ref, snapshot, stage, store, SiteRef, StudioState};
use crate::{checks, pipeline, util};

struct Written {
    snapshot: snapshot::SnapMeta,
    static_report: checks::CheckReport,
    /// Absolute check result — reported, never the rollback trigger.
    static_ok: bool,
    baseline_ok: bool,
    /// Checks that became red because of this patch.
    introduced: Vec<String>,
    /// Checks that were already red before it.
    pre_existing: Vec<String>,
    build: Vec<Value>,
    restored: bool,
}

pub async fn apply(st: &StudioState, patch_id: &str) -> Result<Value> {
    let rec = store::get_patch(st, patch_id)?;
    if rec.status != "pending" {
        bail!("patch {} is {}, not pending", rec.id, rec.status);
    }
    let site_id = rec.site_id.clone();
    {
        let mut b = st.busy.lock().await;
        if !b.insert(site_id.clone()) {
            bail!("another edit is running on site {site_id}");
        }
    }

    let st2 = st.clone();
    let rec2 = rec.clone();
    // HTTP baseline before the write, so a pre-existing failure (a site that
    // never sent gzip, say) cannot be blamed on this patch.
    let port = load_port(st, &site_id);
    let baseline_http = run_http_if_running(st, &site_id, port).await;
    let baseline_http_fails = baseline_http
        .as_ref()
        .map(stage::fail_ids)
        .unwrap_or_default();
    let written = tokio::task::spawn_blocking(move || write_and_check(&st2, &rec2)).await;

    let outcome: Result<Value> = match written {
        Ok(Ok(w)) => {
            // The HTTP checks need the site running, so they happen outside the
            // blocking section, after the files are already in place.
            let http = if w.introduced.is_empty() {
                run_http_if_running(st, &site_id, port).await
            } else {
                None
            };
            let http_introduced: Vec<String> = http
                .as_ref()
                .map(|r| {
                    stage::fail_ids(r)
                        .difference(&baseline_http_fails)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            let http_pre_existing: Vec<String> = baseline_http_fails.iter().cloned().collect();
            let http_ok = http_introduced.is_empty();
            let ok = w.introduced.is_empty() && http_ok;
            let mut restored = w.restored;
            if !ok && !restored {
                let st3 = st.clone();
                let site3 = site_id.clone();
                let snap3 = w.snapshot.clone();
                restored = tokio::task::spawn_blocking(move || rollback(&st3, &site3, &snap3))
                    .await
                    .unwrap_or(false);
            }
            let mut rec = store::get_patch(st, patch_id)?;
            rec.status = if ok {
                "applied".into()
            } else {
                "rolled-back".into()
            };
            rec.applied = Some(util::now_iso());
            store::put_patch(st, &rec)?;
            audit(
                st,
                json!({
                    "action": "apply",
                    "patch": rec.id,
                    "site": rec.site_id,
                    "path": rec.path,
                    "ok": ok,
                    "restored": restored,
                    "snapshot": w.snapshot.id,
                }),
            );
            Ok(json!({
                "ok": ok,
                "restored": restored,
                "patch_id": rec.id,
                "snapshot": w.snapshot.id,
                "build": w.build,
                "static": {
                    "ok": w.static_ok,
                    "baseline_ok": w.baseline_ok,
                    "introduced": w.introduced,
                    "pre_existing": w.pre_existing,
                    "findings": w.static_report.findings
                },
                "introduced": w.introduced,
                "pre_existing": w.pre_existing,
                "http": http.map(|r| json!({
                    "ok": r.ok(),
                    "introduced": http_introduced,
                    "pre_existing": http_pre_existing,
                    "findings": r.findings
                })),
                "note": if restored {
                    "The patch introduced failing checks: the site was restored to the snapshot. Nothing was left half-applied."
                } else if w.pre_existing.is_empty() && http_pre_existing.is_empty() {
                    "Applied. The checks passed; the site is serving the new files."
                } else if w.introduced.is_empty() {
                    "Applied. No new failures; the site already had failing checks before this patch."
                } else {
                    "Applied, but a check failed outside the rollback path. Read the findings."
                },
            }))
        }
        Ok(Err(e)) => {
            // The write path restored the snapshot; the queue must say so too,
            // or the same broken patch is offered again.
            if let Ok(mut rec) = store::get_patch(st, patch_id) {
                rec.status = "rolled-back".into();
                rec.applied = Some(util::now_iso());
                let _ = store::put_patch(st, &rec);
            }
            Err(e)
        }
        Err(e) => Err(anyhow::anyhow!("apply worker failed: {e}")),
    };

    st.busy.lock().await.remove(&site_id);
    outcome
}

fn load_port(st: &StudioState, site: &str) -> u16 {
    site_ref(&st.root, site)
        .map(|s| s.manifest.site.port)
        .unwrap_or(0)
}

fn write_and_check(st: &StudioState, rec: &store::PatchRec) -> Result<Written> {
    let site = site_ref(&st.root, &rec.site_id)?;
    let target = site_path(&site.dir, &rec.path)?;
    let before = std::fs::read_to_string(&target)
        .map_err(|_| anyhow::anyhow!("{} is no longer readable text", rec.path))?;
    // Re-assert the anchor: the file may have changed since the patch was queued.
    let after = patch::apply_text(&before, &rec.old, &rec.new, rec.count)
        .map_err(|e| anyhow::anyhow!("patch is stale: {e}"))?;

    // Baseline before the write: a site can already be red for reasons that
    // have nothing to do with this patch (a CDN link, an old missing dimension).
    let baseline = checks::run_static(&st.root, &site.manifest)?;
    let baseline_fails = stage::fail_ids(&baseline);

    let snap = snapshot::create(
        st,
        &rec.site_id,
        &site.dir,
        &format!("before patch: {}", rec.reason),
    )?;
    super::atomic_write(&target, after.as_bytes())?;

    let mut build: Vec<Value> = Vec::new();
    // From here on the file is changed: any error (a failing build, an I/O
    // failure during checks) must put the snapshot back. A half-applied edit is
    // worse than a refused one.
    let outcome = (|| -> Result<(Vec<Value>, checks::CheckReport)> {
        if !site.manifest.pipeline.is_empty() {
            let steps = pipeline::apply(&site.manifest, &st.root, false, None)?;
            let failed: Vec<String> = steps
                .iter()
                .filter(|s| s.status == "fail" || s.status == "blocked")
                .map(|s| format!("{} ({})", s.name, s.status))
                .collect();
            build = steps
                .into_iter()
                .map(
                    |s| json!({"name": s.name, "status": s.status, "ms": s.ms, "detail": s.detail}),
                )
                .collect();
            // `pipeline::apply` reports a failed step in its result rather than
            // returning Err. Treating that as success would leave the site
            // serving the previous build while the source says otherwise.
            if !failed.is_empty() {
                bail!("build failed: {}", failed.join(", "));
            }
        }
        let report = checks::run_static(&st.root, &site.manifest)?;
        Ok((build.clone(), report))
    })();

    let (build, static_report) = match outcome {
        Ok(v) => v,
        Err(e) => {
            let _ = snapshot::restore(st, &site.dir, &snap);
            if !site.manifest.pipeline.is_empty() {
                let _ = pipeline::apply(&site.manifest, &st.root, false, None);
            }
            return Err(e.context("edit failed after the file was written; snapshot restored"));
        }
    };

    let static_ok = static_report.ok();
    let after_fails = stage::fail_ids(&static_report);
    let introduced: Vec<String> = after_fails.difference(&baseline_fails).cloned().collect();
    let pre_existing: Vec<String> = baseline_fails.iter().cloned().collect();
    let mut restored = false;
    if !introduced.is_empty() {
        snapshot::restore(st, &site.dir, &snap)?;
        if !site.manifest.pipeline.is_empty() {
            let _ = pipeline::apply(&site.manifest, &st.root, false, None);
        }
        restored = true;
    }

    Ok(Written {
        snapshot: snap,
        static_report,
        static_ok,
        baseline_ok: baseline.ok(),
        introduced,
        pre_existing,
        build,
        restored,
    })
}

fn rollback(st: &StudioState, site_id: &str, snap: &snapshot::SnapMeta) -> bool {
    let Ok(site) = site_ref(&st.root, site_id) else {
        return false;
    };
    if snapshot::restore(st, &site.dir, snap).is_err() {
        return false;
    }
    if !site.manifest.pipeline.is_empty() {
        let _ = pipeline::apply(&site.manifest, &st.root, false, None);
    }
    true
}

fn undo_inner(st: &StudioState, site: &SiteRef, snap: &snapshot::SnapMeta) -> Result<Value> {
    // Snapshot the current state first so the undo itself is reversible.
    let current = snapshot::create(st, &site.id, &site.dir, "before undo")?;
    let changed = snapshot::restore(st, &site.dir, snap)?;
    let mut build = Vec::new();
    let mut build_ok = true;
    if !site.manifest.pipeline.is_empty() {
        let steps = pipeline::apply(&site.manifest, &st.root, false, None)?;
        build_ok = steps
            .iter()
            .all(|s| s.status != "fail" && s.status != "blocked");
        build = steps
            .into_iter()
            .map(|s| json!({"name": s.name, "status": s.status, "ms": s.ms}))
            .collect();
    }
    let report = checks::run_static(&st.root, &site.manifest)?;
    audit(
        st,
        json!({
            "action": "undo",
            "site": site.id,
            "snapshot": snap.id,
            "changed": changed,
            "ok": build_ok,
            "checks_ok": report.ok(),
        }),
    );
    Ok(json!({
        "ok": build_ok,
        "checks_ok": report.ok(),
        "restored_snapshot": snap.id,
        "redo_snapshot": current.id,
        "changed": changed,
        "build": build,
        "findings": report.findings,
    }))
}

pub async fn undo(st: &StudioState, site_id: &str) -> Result<Value> {
    let site = site_ref(&st.root, site_id)?;
    let Some(snap) = snapshot::latest(st, site_id) else {
        bail!("no snapshot for {site_id}: nothing to undo");
    };
    {
        let mut b = st.busy.lock().await;
        if !b.insert(site_id.to_string()) {
            bail!("another edit is running on site {site_id}");
        }
    }
    let st2 = st.clone();
    let site2 = site.clone();
    let out = tokio::task::spawn_blocking(move || undo_inner(&st2, &site2, &snap)).await;
    st.busy.lock().await.remove(site_id);
    match out {
        Ok(v) => v,
        Err(e) => Err(anyhow::anyhow!("undo worker failed: {e}")),
    }
}

async fn run_http_if_running(
    st: &StudioState,
    site: &str,
    port: u16,
) -> Option<checks::CheckReport> {
    if port == 0 || !st.sup.is_running(site).await {
        return None;
    }
    let base = format!("http://127.0.0.1:{port}");
    Some(checks::run_http(&base, &st.check_client).await)
}

pub fn audit(st: &StudioState, entry: Value) {
    let p = st.data.join("audit.jsonl");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
    {
        let _ = writeln!(f, "{entry}");
    }
}
