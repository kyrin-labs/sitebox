//! Apply and undo. This is the only code path in Studio that writes a site file.
//!
//! Order is not negotiable: snapshot → write → build → check → (restore on red).
//! A check that fails after the build restores the sources *and* rebuilds, so the
//! served page and the source tree never disagree.

use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::io::Write;

use super::{patch, site_path, site_ref, snapshot, store, SiteRef, StudioState};
use crate::{checks, pipeline, util};

struct Written {
    snapshot: snapshot::SnapMeta,
    static_report: checks::CheckReport,
    static_ok: bool,
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
    let written = tokio::task::spawn_blocking(move || write_and_check(&st2, &rec2)).await;

    let outcome: Result<Value> = match written {
        Ok(Ok(w)) => {
            // The HTTP checks need the site running, so they happen outside the
            // blocking section, after the files are already in place.
            let http = if w.static_ok {
                run_http_if_running(st, &site_id, load_port(st, &site_id)).await
            } else {
                None
            };
            let http_ok = http.as_ref().map(|r| r.ok()).unwrap_or(true);
            let ok = w.static_ok && http_ok;
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
                "static": { "ok": w.static_ok, "findings": w.static_report.findings },
                "http": http.map(|r| json!({"ok": r.ok(), "findings": r.findings})),
                "note": if restored {
                    "Checks failed after the edit: the site was restored to the snapshot. Nothing was left half-applied."
                } else if ok {
                    "Applied. The checks passed; the site is serving the new files."
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
    let mut restored = false;
    if !static_ok {
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
            "ok": report.ok() && build_ok,
        }),
    );
    Ok(json!({
        "ok": report.ok() && build_ok,
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
    Some(checks::run_http(&base, &st.client).await)
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
