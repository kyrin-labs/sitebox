//! Deploy as a mirror, then prove it.
//!
//! The quietest failure in the old repo: `index.html` was uploaded alone and the
//! running site 404'd on every image while local checks stayed green. A deploy
//! here copies the referenced tree, deletes what no longer exists, and then
//! compares sha256 on both sides.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::util;

#[derive(Debug, Default, Serialize)]
pub struct DeployReport {
    pub mode: String,
    pub files: usize,
    pub copied: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub bytes: u64,
    pub mismatches: Vec<String>,
    pub dry_run: bool,
}

pub fn deploy_local(src: &Path, dst: &Path, dry_run: bool) -> Result<DeployReport> {
    if !src.is_dir() {
        bail!("source is not a folder: {}", src.display());
    }
    std::fs::create_dir_all(dst)?;

    let src_files = walk_rel(src)?;
    let dst_files = walk_rel(dst)?;
    let mut rep = DeployReport {
        mode: "local".into(),
        files: src_files.len(),
        dry_run,
        ..Default::default()
    };

    for (rel, s) in &src_files {
        let target = dst.join(rel);
        let s_hash = util::sha256_file(s)?;
        let d_hash = if target.is_file() {
            Some(util::sha256_file(&target)?)
        } else {
            None
        };
        match d_hash {
            Some(h) if h == s_hash => rep.unchanged += 1,
            Some(_) => {
                rep.updated += 1;
                rep.bytes += std::fs::metadata(s)?.len();
                if !dry_run {
                    copy_atomic(s, &target)?;
                }
            }
            None => {
                rep.copied += 1;
                rep.bytes += std::fs::metadata(s)?.len();
                if !dry_run {
                    copy_atomic(s, &target)?;
                }
            }
        }
    }

    // Mirror means mirror: delete files that no longer exist locally.
    for (rel, d) in &dst_files {
        if !src_files.contains_key(rel) {
            rep.removed += 1;
            if !dry_run {
                let _ = std::fs::remove_file(d);
            }
        }
    }

    if !dry_run {
        // Verify what is actually on disk now.
        for (rel, s) in &src_files {
            let target = dst.join(rel);
            match (util::sha256_file(s), util::sha256_file(&target)) {
                (Ok(a), Ok(b)) if a == b => {}
                _ => rep.mismatches.push(rel.clone()),
            }
        }
        if !rep.mismatches.is_empty() {
            bail!(
                "deploy verification failed: {} file(s) differ, first: {}",
                rep.mismatches.len(),
                rep.mismatches[0]
            );
        }
    }
    Ok(rep)
}

pub fn deploy_ssh(src: &Path, remote: &str, dry_run: bool) -> Result<DeployReport> {
    // remote = user@host:/absolute/path
    if !remote.contains(':') {
        bail!("remote must look like user@host:/path");
    }
    let mut args: Vec<String> = vec!["-az".into(), "--delete".into(), "--itemize-changes".into()];
    if dry_run {
        args.push("--dry-run".into());
    }
    args.push(format!("{}/", src.display()));
    args.push(remote.to_string());
    let out = std::process::Command::new("rsync")
        .args(&args)
        .output()
        .context("run rsync (is it installed?)")?;
    if !out.status.success() {
        bail!("rsync failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let changed = text
        .lines()
        .filter(|l| l.starts_with('>') || l.starts_with('*'))
        .count();
    let removed = text.lines().filter(|l| l.starts_with("*deleting")).count();
    Ok(DeployReport {
        mode: format!("ssh:{remote}"),
        files: walk_rel(src).map(|m| m.len()).unwrap_or(0),
        copied: changed,
        removed,
        dry_run,
        ..Default::default()
    })
}

fn walk_rel(root: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let mut out = BTreeMap::new();
    for e in walkdir::WalkDir::new(root).follow_links(false) {
        let e = e?;
        if !e.file_type().is_file() {
            continue;
        }
        let rel = e
            .path()
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        out.insert(rel, e.path().to_path_buf());
    }
    Ok(out)
}

fn copy_atomic(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dst.with_extension(format!(
        "{}.tmp",
        dst.extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    std::fs::copy(src, &tmp)
        .with_context(|| format!("copy {} → {}", src.display(), tmp.display()))?;
    std::fs::rename(&tmp, dst)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_copies_and_deletes() {
        let dir = std::env::temp_dir().join(format!("sbx-deploy-{}", std::process::id()));
        let src = dir.join("src");
        let dst = dir.join("dst");
        std::fs::create_dir_all(src.join("img")).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("index.html"), "hello").unwrap();
        std::fs::write(src.join("img/a.txt"), "a").unwrap();
        std::fs::write(dst.join("stale.txt"), "old").unwrap();

        let rep = deploy_local(&src, &dst, false).unwrap();
        assert_eq!(rep.files, 2);
        assert_eq!(rep.copied, 2);
        assert_eq!(rep.removed, 1);
        assert!(dst.join("index.html").is_file());
        assert!(!dst.join("stale.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
