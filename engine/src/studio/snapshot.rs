//! Snapshots: content-addressed copies of a site's *source* files.
//!
//! Full-folder copies were rejected after measuring `sites/`: one site is 354 MB
//! of audio and another 221 MB of images. A snapshot keeps only the files a
//! patch can actually touch, keyed by sha256, so twenty undo points cost a few
//! megabytes instead of a copy of every video.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{atomic_write, ensure_dir, outline, StudioState};
use crate::util;

const KEEP: usize = 20;
const MAX_FILES: usize = 500;

static SNAP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapMeta {
    pub id: String,
    pub site_id: String,
    pub label: String,
    pub created: String,
    /// Tie-breaker for snapshots created inside the same millisecond. Without
    /// it `latest()` can return the snapshot taken *after* the change, and undo
    /// becomes a no-op that reports success.
    #[serde(default)]
    pub seq: u64,
    /// relative path → sha256
    pub files: BTreeMap<String, String>,
}

fn snaps_dir(st: &StudioState, site: &str) -> PathBuf {
    st.data.join("snapshots").join(site)
}

fn blob_path(st: &StudioState, sha: &str) -> PathBuf {
    st.data.join("snapshots/blobs").join(sha)
}

pub fn create(st: &StudioState, site_id: &str, site_dir: &Path, label: &str) -> Result<SnapMeta> {
    let seq = SNAP_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let created = util::now_iso();
    let snap = SnapMeta {
        id: format!(
            "sn-{}-{seq}",
            created.replace([':', '.', '-', 'T', 'Z'], "")
        ),
        site_id: site_id.to_string(),
        label: label.to_string(),
        created,
        seq,
        files: BTreeMap::new(),
    };
    let mut files = outline::source_files(site_dir, MAX_FILES * 4);
    // index.html is the file every edit ends up next to; never let the cap drop it.
    files.sort_by(|a, b| {
        let ai = a.0.ends_with("index.html");
        let bi = b.0.ends_with("index.html");
        bi.cmp(&ai).then(b.1.cmp(&a.1))
    });
    files.truncate(MAX_FILES);

    let mut snap = snap;
    for (rel, _) in files {
        let p = match super::site_path(site_dir, &rel) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        let sha = util::sha256_bytes(&bytes);
        let blob = blob_path(st, &sha);
        if !blob.is_file() {
            atomic_write(&blob, &bytes)?;
        }
        snap.files.insert(rel, sha);
    }
    if snap.files.is_empty() {
        bail!(
            "nothing to snapshot: no source files under {}",
            site_dir.display()
        );
    }
    let d = snaps_dir(st, site_id).join(&snap.id);
    ensure_dir(&d)?;
    atomic_write(
        &d.join("manifest.json"),
        serde_json::to_string_pretty(&snap)?.as_bytes(),
    )?;
    prune(st, site_id);
    Ok(snap)
}

pub fn list(st: &StudioState, site_id: &str) -> Vec<SnapMeta> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(snaps_dir(st, site_id)) else {
        return out;
    };
    for e in rd.flatten() {
        let Ok(raw) = std::fs::read_to_string(e.path().join("manifest.json")) else {
            continue;
        };
        let Ok(m) = serde_json::from_str::<SnapMeta>(&raw) else {
            continue;
        };
        out.push(m);
    }
    out.sort_by(|a, b| b.created.cmp(&a.created).then(b.seq.cmp(&a.seq)));
    out
}

pub fn latest(st: &StudioState, site_id: &str) -> Option<SnapMeta> {
    list(st, site_id).into_iter().next()
}

fn prune(st: &StudioState, site_id: &str) {
    let all = list(st, site_id);
    for old in all.into_iter().skip(KEEP) {
        let _ = std::fs::remove_dir_all(snaps_dir(st, site_id).join(old.id));
    }
}

/// Put every snapshotted file back, and remove source files that did not exist
/// at snapshot time. Returns the paths that changed.
pub fn restore(st: &StudioState, site_dir: &Path, snap: &SnapMeta) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for (rel, sha) in &snap.files {
        let blob = blob_path(st, sha);
        if !blob.is_file() {
            bail!("snapshot {} is incomplete: blob {sha} is missing", snap.id);
        }
        let target = super::site_path(site_dir, rel).with_context(|| format!("restore {rel}"))?;
        let now = util::sha256_file(&target).unwrap_or_default();
        if now != *sha {
            let bytes = std::fs::read(&blob)?;
            atomic_write(&target, &bytes)?;
            changed.push(rel.clone());
        }
    }
    // A file added after the snapshot is part of the bad state too.
    for (rel, _) in outline::source_files(site_dir, MAX_FILES) {
        if !snap.files.contains_key(&rel) {
            if let Ok(p) = super::site_path(site_dir, &rel) {
                if std::fs::remove_file(&p).is_ok() {
                    changed.push(format!("{rel} (removed)"));
                }
            }
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_restores_bytes() {
        let tmp = std::env::temp_dir().join(format!("sb-snap-{}", std::process::id()));
        let site = tmp.join("site");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(site.join("index.html"), "<h1>one</h1>").unwrap();
        let st = StudioState::new(
            tmp.clone(),
            std::sync::Arc::new(crate::supervisor::Supervisor::new(tmp.clone())),
            reqwest::Client::new(),
        );
        let snap = create(&st, "s", &site, "test").unwrap();
        std::fs::write(site.join("index.html"), "<h1>two</h1>").unwrap();
        std::fs::write(site.join("extra.js"), "x").unwrap();
        let changed = restore(&st, &site, &snap).unwrap();
        assert!(changed.iter().any(|p| p.contains("index.html")));
        assert_eq!(
            std::fs::read_to_string(site.join("index.html")).unwrap(),
            "<h1>one</h1>"
        );
        assert!(!site.join("extra.js").exists());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn latest_orders_snapshots_created_in_the_same_millisecond() {
        let tmp = std::env::temp_dir().join(format!("sb-snap-order-{}", std::process::id()));
        let site = tmp.join("site");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(site.join("index.html"), "one").unwrap();
        let st = StudioState::new(
            tmp.clone(),
            std::sync::Arc::new(crate::supervisor::Supervisor::new(tmp.clone())),
            reqwest::Client::new(),
        );
        let a = create(&st, "s", &site, "a").unwrap();
        std::fs::write(site.join("index.html"), "two").unwrap();
        let b = create(&st, "s", &site, "b").unwrap();
        let latest = latest(&st, "s").expect("a snapshot");
        assert_eq!(
            latest.id, b.id,
            "latest must be the most recent snapshot, not a tie"
        );
        let all = list(&st, "s");
        assert_eq!(all[0].id, b.id);
        assert_eq!(all[1].id, a.id);
        std::fs::remove_dir_all(&tmp).ok();
    }
}
