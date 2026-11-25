//! Patches: an exact substring, an expected occurrence count, and a diff.
//!
//! Line numbers were rejected on purpose. A model counting lines in a 300 KB
//! file is wrong often enough to be useless, and its wrongness is silent. An
//! anchored substring with a count assertion fails loudly instead: either the
//! anchor occurs exactly as many times as the model said, or nothing is written.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const MAX_PATCH_BYTES: usize = 200 * 1024;
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchSpec {
    pub path: String,
    pub old: String,
    pub new: String,
    #[serde(default = "one")]
    pub count: usize,
    #[serde(default)]
    pub reason: String,
}

fn one() -> usize {
    1
}

/// Reject a patch before it reaches the filesystem.
pub fn validate(spec: &PatchSpec, site_kind: &str) -> Result<()> {
    if spec.old.is_empty() {
        bail!("`old` must not be empty — a patch replaces an existing anchor");
    }
    if spec.old == spec.new {
        bail!("patch changes nothing: `old` and `new` are identical");
    }
    if spec.count == 0 {
        bail!("`count` must be at least 1");
    }
    if spec.count > 50 {
        bail!("`count` {c} is too many; split the edit", c = spec.count);
    }
    let size = spec.old.len() + spec.new.len();
    if size > MAX_PATCH_BYTES {
        bail!("patch is {size} bytes; the limit is {MAX_PATCH_BYTES}");
    }
    if site_kind == "generated" && (spec.path == "public" || spec.path.starts_with("public/")) {
        bail!(
            "this site is generated: `{}` is build output and the next build would overwrite it. \
             Patch the source instead (`tools/`, `synd/`, or the data file) unless the file is hand-maintained.",
            spec.path
        );
    }
    Ok(())
}

/// Replace exactly `expect` non-overlapping occurrences of `old`.
pub fn apply_text(text: &str, old: &str, new: &str, expect: usize) -> Result<String> {
    let found = text.matches(old).count();
    if found != expect {
        bail!(
            "anchor occurs {found} time(s), patch says {expect}. Nothing was written — \
             read the region again and re-issue the patch."
        );
    }
    Ok(text.replace(old, new))
}

#[derive(Debug, Clone, Serialize)]
pub struct Stat {
    pub added: usize,
    pub removed: usize,
}

/// A bounded hunk: three lines of context around the changed block.
///
/// A full LCS diff of two 90k-line files costs more than the edit is worth; the
/// operator needs to recognise the change, not to apply it with `patch(1)`.
pub fn diff_hunk(before: &str, after: &str) -> (String, Stat) {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();

    let mut prefix = 0;
    while prefix < a.len() && prefix < b.len() && a[prefix] == b[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < a.len().saturating_sub(prefix)
        && suffix < b.len().saturating_sub(prefix)
        && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let a_end = a.len() - suffix;
    let b_end = b.len() - suffix;
    let removed = a_end.saturating_sub(prefix);
    let added = b_end.saturating_sub(prefix);

    let ctx = 3usize;
    let start = prefix.saturating_sub(ctx);
    let a_stop = (a_end + ctx).min(a.len());
    let b_stop = (b_end + ctx).min(b.len());

    let mut out = String::new();
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        start + 1,
        a_stop.saturating_sub(start),
        start + 1,
        b_stop.saturating_sub(start)
    ));

    for line in &a[start..prefix] {
        out.push_str(&format!(" {line}\n"));
    }
    let mut capped = 0usize;
    for line in &a[prefix..a_end] {
        if capped < 80 {
            out.push_str(&format!("-{line}\n"));
        }
        capped += 1;
    }
    if capped > 80 {
        out.push_str(&format!("-… {} more removed line(s)\n", capped - 80));
    }
    capped = 0;
    for line in &b[prefix..b_end] {
        if capped < 80 {
            out.push_str(&format!("+{line}\n"));
        }
        capped += 1;
    }
    if capped > 80 {
        out.push_str(&format!("+… {} more added line(s)\n", capped - 80));
    }
    for line in &a[a_end..a_stop] {
        out.push_str(&format!(" {line}\n"));
    }
    if out.len() > 24_000 {
        out.truncate(24_000);
        out.push_str("\n… diff truncated\n");
    }
    (out, Stat { added, removed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_mismatch_writes_nothing() {
        let err = apply_text("<p>a</p><p>a</p>", "<p>a</p>", "<p>b</p>", 1).unwrap_err();
        assert!(err.to_string().contains("occurs 2 time(s)"));
    }

    #[test]
    fn exact_count_replaces_every_occurrence() {
        let out = apply_text("a a a", "a", "b", 3).unwrap();
        assert_eq!(out, "b b b");
    }

    #[test]
    fn generated_site_refuses_public() {
        let spec = PatchSpec {
            path: "public/index.html".into(),
            old: "x".into(),
            new: "y".into(),
            count: 1,
            reason: String::new(),
        };
        assert!(validate(&spec, "generated").is_err());
        assert!(validate(&spec, "hand-authored").is_ok());
    }

    #[test]
    fn diff_reports_only_the_change() {
        let before = "a\nb\nc\nd\ne\nf\n";
        let after = "a\nb\nC\nd\ne\nf\n";
        let (hunk, stat) = diff_hunk(before, after);
        assert!(hunk.contains("-c"));
        assert!(hunk.contains("+C"));
        assert_eq!((stat.added, stat.removed), (1, 1));
    }
}

/// A patch that duplicates document-level closing tags produces a page that
/// still renders, so nothing complains — but the next edit anchors on the first
/// `</body>` and the file keeps growing. Refuse it at propose time instead.
pub fn html_sanity(path: &str, text: &str) -> Result<()> {
    if !(path.ends_with(".html") || path.ends_with(".htm")) {
        return Ok(());
    }
    for (open, close) in [("<html", "</html>"), ("<body", "</body>")] {
        for tag in [open, close] {
            let n = text.matches(tag).count();
            if n > 1 {
                bail!(
                    "the patch would leave {n} `{tag}` tags in {path}; \
                     anchor `old` on the whole document or on a block inside it, \
                     so the closing tags are not duplicated"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod sanity_tests {
    use super::html_sanity;

    #[test]
    fn refuses_duplicate_closers() {
        assert!(html_sanity("a.html", "<html><body>x</body></html>").is_ok());
        assert!(html_sanity("a.html", "<html><body>x</body></html></body></html>").is_err());
        assert!(html_sanity("a.css", "</html></html>").is_ok());
    }
}
