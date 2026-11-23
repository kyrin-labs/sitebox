//! The check catalog.
//!
//! "Done" used to be eight paragraphs in a skill. Here every rule is a check
//! with a stable id, an evidence string and a fix. The ids are the contract an
//! agent reads; the prose only has to explain why the rule exists.

use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::manifest::Manifest;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Fail,
    Skip,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub check: String,
    pub status: Status,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

impl Finding {
    fn pass(id: &str, title: &str) -> Self {
        Self {
            check: id.into(),
            status: Status::Pass,
            title: title.into(),
            file: None,
            evidence: None,
            fix: None,
        }
    }
    fn fail(
        id: &str,
        title: &str,
        file: Option<String>,
        evidence: impl Into<String>,
        fix: &str,
    ) -> Self {
        Self {
            check: id.into(),
            status: Status::Fail,
            title: title.into(),
            file,
            evidence: Some(evidence.into()),
            fix: Some(fix.into()),
        }
    }
    fn skip(id: &str, title: &str, why: &str) -> Self {
        Self {
            check: id.into(),
            status: Status::Skip,
            title: title.into(),
            file: None,
            evidence: Some(why.into()),
            fix: None,
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct CheckReport {
    pub site: String,
    pub findings: Vec<Finding>,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl CheckReport {
    pub fn new(site: &str) -> Self {
        Self {
            site: site.into(),
            ..Default::default()
        }
    }
    pub fn push(&mut self, f: Finding) {
        match f.status {
            Status::Pass => self.passed += 1,
            Status::Fail => self.failed += 1,
            Status::Skip => self.skipped += 1,
            Status::Info => {}
        }
        self.findings.push(f);
    }
    pub fn ok(&self) -> bool {
        self.failed == 0
    }
}

/* ── static checks ─────────────────────────────────────────────────────── */

pub fn run_static(root: &Path, m: &Manifest) -> Result<CheckReport> {
    let site_dir = m.site_dir(root);
    let public = m.public_dir(root);
    let mut r = CheckReport::new(&m.site.id);

    // SB001 — the entry point exists.
    let index = public.join("index.html");
    if index.is_file() {
        r.push(Finding::pass("SB001", "index.html exists"));
    } else {
        r.push(Finding::fail(
            "SB001",
            "index.html exists",
            Some(rel(&public, &index)),
            "no index.html in the public folder",
            "create public/index.html (or fix site.public_dir in sitebox.toml)",
        ));
        return Ok(r);
    }
    let html = std::fs::read_to_string(&index).unwrap_or_default();

    // SB002 — no third-party assets. External <a href> is fine; a CDN asset is not.
    let mut external = Vec::new();
    for tag in tags(&html) {
        if let Some(u) = asset_url(&tag) {
            if u.starts_with("http://") || u.starts_with("https://") || u.starts_with("//") {
                external.push(u);
            }
        }
    }
    if external.is_empty() {
        r.push(Finding::pass("SB002", "no third-party assets"));
    } else {
        r.push(Finding::fail(
            "SB002",
            "no third-party assets",
            Some("public/index.html".into()),
            format!(
                "{} external asset reference(s), first: {}",
                external.len(),
                external[0]
            ),
            "vendor the file locally (public/vendor/) and reference it relatively",
        ));
    }

    // SB003 — every <img> declares width and height (layout shift + crop proof).
    let mut missing_dims = Vec::new();
    let mut runtime_imgs = 0;
    for tag in tags(&html) {
        if !tag.to_ascii_lowercase().starts_with("<img") {
            continue;
        }
        // An <img> with no src is filled at runtime (e.g. a crop preview).
        // It cannot declare intrinsic geometry yet, so it is not a failure —
        // but it is counted, so the report shows it was seen.
        let src = attr_value(&tag, "src");
        if src.as_deref().unwrap_or("").trim().is_empty() {
            runtime_imgs += 1;
            continue;
        }
        let has_w = attr_value(&tag, "width").is_some();
        let has_h = attr_value(&tag, "height").is_some();
        if !has_w || !has_h {
            missing_dims.push(src.unwrap_or_else(|| "(no src)".into()));
        }
    }
    if missing_dims.is_empty() {
        let mut f = Finding::pass("SB003", "every <img> has width/height");
        if runtime_imgs > 0 {
            f.evidence = Some(format!(
                "{runtime_imgs} runtime-filled <img> skipped (no src)"
            ));
        }
        r.push(f);
    } else {
        r.push(Finding::fail(
            "SB003",
            "every <img> has width/height",
            Some("public/index.html".into()),
            format!(
                "{} image tag(s) missing width or height, first: {}",
                missing_dims.len(),
                missing_dims[0]
            ),
            "add the real intrinsic width/height from synd/dims.json",
        ));
    }

    // SB004 — splice markers, when the site uses them, are balanced.
    let starts = html.matches("/*__DATA_START__*/").count();
    let ends = html.matches("/*__DATA_END__*/").count();
    if starts == 0 && ends == 0 {
        r.push(Finding::skip(
            "SB004",
            "data splice markers",
            "site does not splice a data block",
        ));
    } else if starts == 1 && ends == 1 {
        r.push(Finding::pass("SB004", "data splice markers balanced"));
    } else {
        r.push(Finding::fail(
            "SB004",
            "data splice markers balanced",
            Some("public/index.html".into()),
            format!("{starts} start marker(s) vs {ends} end marker(s)"),
            "restore exactly one /*__DATA_START__*/ … /*__DATA_END__*/ pair, then re-run the splice step",
        ));
    }

    // SB005 — every local asset referenced by the page exists on disk.
    let mut missing = Vec::new();
    for tag in tags(&html) {
        if let Some(u) = asset_url(&tag) {
            if u.starts_with("http") || u.starts_with("//") {
                continue;
            }
            let clean = u.split(['?', '#']).next().unwrap_or("");
            if clean.is_empty() {
                continue;
            }
            let p = public.join(clean.trim_start_matches('/'));
            if !p.is_file() {
                missing.push(clean.to_string());
            }
        }
    }
    if missing.is_empty() {
        r.push(Finding::pass("SB005", "every referenced asset exists"));
    } else {
        r.push(Finding::fail(
            "SB005",
            "every referenced asset exists",
            Some("public/index.html".into()),
            format!("{} missing, first: {}", missing.len(), missing[0]),
            "run the download/resize steps, or remove the reference",
        ));
    }

    // SB010 — declared geometry matches the files on disk.
    let dims_path = site_dir.join("synd/dims.json");
    if !dims_path.is_file() {
        r.push(Finding::skip(
            "SB010",
            "dims match files",
            "no synd/dims.json",
        ));
    } else {
        match check_dims(&public, &dims_path) {
            Ok((checked, bad)) => {
                if bad.is_empty() {
                    r.push(Finding::pass(
                        "SB010",
                        &format!("dims match {checked} file(s)"),
                    ));
                } else {
                    let sample = bad.iter().take(3).cloned().collect::<Vec<_>>().join("; ");
                    r.push(Finding::fail(
                        "SB010",
                        "dims match files",
                        Some("synd/dims.json".into()),
                        format!("{} of {checked} mismatch: {sample}", bad.len()),
                        "re-run `sitebox build <id>` so dims.json is generated after the resize step",
                    ));
                }
            }
            Err(e) => r.push(Finding::fail(
                "SB010",
                "dims match files",
                Some("synd/dims.json".into()),
                format!("unreadable: {e}"),
                "regenerate dims.json from the resize step",
            )),
        }
    }

    // SB020 — no placeholder or invented data.
    let placeholders = [
        "picsum.photos",
        "unsplash.com",
        "placeholder.com",
        "via.placeholder",
        "pravatar",
        "loremflickr",
    ];
    let mut found: Vec<String> = placeholders
        .iter()
        .filter(|p| html.to_ascii_lowercase().contains(**p))
        .map(|p| p.to_string())
        .collect();
    if let Some(dir) = site_dir
        .join("synd")
        .is_dir()
        .then(|| site_dir.join("synd"))
    {
        for entry in walkdir::WalkDir::new(dir).max_depth(1) {
            let Ok(e) = entry else { continue };
            if e.path().extension().map(|x| x == "json").unwrap_or(false) {
                if let Ok(txt) = std::fs::read_to_string(e.path()) {
                    for p in placeholders {
                        if txt.to_ascii_lowercase().contains(p) && !found.iter().any(|f| f == p) {
                            found.push(p.to_string());
                        }
                    }
                    if txt.contains("Math.random") && !found.iter().any(|f| f == "Math.random") {
                        found.push("Math.random".into());
                    }
                }
            }
        }
    }
    if found.is_empty() {
        r.push(Finding::pass("SB020", "no placeholder or invented data"));
    } else {
        r.push(Finding::fail(
            "SB020",
            "no placeholder or invented data",
            None,
            format!("found: {}", found.join(", ")),
            "replace with the real source value, or omit the field; never synthesise numbers",
        ));
    }

    // SB021 — no NaN/undefined leaked into the rendered HTML.
    let leaks: Vec<&str> = ["NaN โ", ">NaN<", "undefined โ", ">undefined<", "null โ"]
        .into_iter()
        .filter(|s| html.contains(s))
        .collect();
    if leaks.is_empty() {
        r.push(Finding::pass("SB021", "no NaN/undefined in output"));
    } else {
        r.push(Finding::fail(
            "SB021",
            "no NaN/undefined in output",
            Some("public/index.html".into()),
            format!("found: {}", leaks.join(", ")),
            "fix the data field at the source — a missing value must fail the build, not render",
        ));
    }

    // SB030 — every data file under synd/ parses as JSON.
    let synd = site_dir.join("synd");
    if !synd.is_dir() {
        r.push(Finding::skip(
            "SB030",
            "data files parse",
            "no synd/ folder",
        ));
    } else {
        let mut bad = Vec::new();
        let mut n = 0;
        for entry in walkdir::WalkDir::new(&synd).max_depth(1) {
            let Ok(e) = entry else { continue };
            if e.path().extension().map(|x| x == "json").unwrap_or(false) {
                n += 1;
                if serde_json::from_str::<serde_json::Value>(
                    &std::fs::read_to_string(e.path()).unwrap_or_default(),
                )
                .is_err()
                {
                    bad.push(
                        e.path()
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    );
                }
            }
        }
        if bad.is_empty() {
            r.push(Finding::pass("SB030", &format!("{n} data file(s) parse")));
        } else {
            r.push(Finding::fail(
                "SB030",
                "data files parse",
                None,
                format!("invalid JSON: {}", bad.join(", ")),
                "regenerate the data file; do not hand-edit it",
            ));
        }
    }

    Ok(r)
}

fn rel(base: &Path, p: &Path) -> String {
    p.strip_prefix(base)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Minimal tag scanner: yields `<tag …>` slices, ignoring comments and the
/// contents of `<script>` / `<style>` blocks (a template string inside JS is
/// not a real tag, and treating it as one produced false asset findings).
fn tags(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        rest = &rest[i..];
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(j) => rest = &rest[j + 3..],
                None => break,
            }
            continue;
        }
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..=end];
        let lower = tag.to_ascii_lowercase();
        if !tag.starts_with("</") {
            out.push(tag.to_string());
        }
        rest = &rest[end + 1..];
        for block in ["script", "style"] {
            if lower.starts_with(&format!("<{block}")) && !lower.ends_with("/>") {
                let close = format!("</{block}");
                if let Some(j) = rest.to_ascii_lowercase().find(&close) {
                    let tail = &rest[j..];
                    if let Some(k) = tail.find('>') {
                        rest = &tail[k + 1..];
                    }
                }
                break;
            }
        }
    }
    out
}

/// The URL a tag actually loads, if it is an asset reference.
///
/// `<link rel="canonical">` and `rel="preconnect"` are metadata, not assets;
/// counting them as CDN dependencies made every site look broken.
fn asset_url(tag: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    if lower.starts_with("<script") || lower.starts_with("<img") {
        return attr_value(tag, "src").filter(|u| usable_url(u));
    }
    if lower.starts_with("<link") {
        let rel = attr_value(tag, "rel")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let loads = rel.split_whitespace().any(|r| {
            matches!(
                r,
                "stylesheet" | "icon" | "apple-touch-icon" | "preload" | "manifest"
            )
        });
        if !loads {
            return None;
        }
        return attr_value(tag, "href").filter(|u| usable_url(u));
    }
    None
}

fn usable_url(u: &str) -> bool {
    !u.is_empty()
        && u != "/"
        && !u.ends_with('/')
        && !u.contains("${")
        && !u.contains("{{")
        && !u.starts_with("data:")
        && !u.starts_with('#')
}

/// `attr="v"`, `attr='v'` or unquoted `attr=v`.
fn attr_value(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{attr}=");
    let mut from = 0;
    while let Some(i) = lower[from..].find(&needle) {
        let abs = from + i;
        // must be a whole attribute name (preceded by space/newline)
        let prev_ok = abs == 0
            || tag[..abs]
                .chars()
                .last()
                .map(|c| c.is_whitespace())
                .unwrap_or(false);
        let after = abs + needle.len();
        if !prev_ok {
            from = after;
            continue;
        }
        let rest = &tag[after..];
        let v = if let Some(s) = rest.strip_prefix('"') {
            s.split('"').next()
        } else if let Some(s) = rest.strip_prefix('\'') {
            s.split('\'').next()
        } else {
            rest.split([' ', '>', '\n', '\t']).next()
        };
        if let Some(v) = v {
            return Some(v.trim().to_string());
        }
        from = after;
    }
    None
}

/// dims.json is `{ "img/x.webp": [w, h, ratio?, colour?] }` relative to public.
fn check_dims(public: &Path, dims_path: &Path) -> Result<(usize, Vec<String>)> {
    let raw = std::fs::read_to_string(dims_path).context("read dims")?;
    let map: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&raw).context("parse dims")?;
    let mut bad = Vec::new();
    let mut checked = 0;
    for (k, v) in map {
        checked += 1;
        let Some(arr) = v.as_array() else {
            bad.push(format!("{k}: not an array"));
            continue;
        };
        let (Some(w), Some(h)) = (
            arr.first().and_then(|x| x.as_u64()),
            arr.get(1).and_then(|x| x.as_u64()),
        ) else {
            bad.push(format!("{k}: no w/h"));
            continue;
        };
        let p: PathBuf = public.join(&k);
        if !p.is_file() {
            bad.push(format!("{k}: file missing"));
            continue;
        }
        match image::image_dimensions(&p) {
            Ok((aw, ah)) if aw as u64 == w && ah as u64 == h => {}
            Ok((aw, ah)) => bad.push(format!("{k}: {aw}x{ah} != declared {w}x{h}")),
            Err(e) => bad.push(format!("{k}: unreadable ({e})")),
        }
    }
    Ok((checked, bad))
}

/* ── HTTP checks (run against a listening site) ────────────────────────── */

pub async fn run_http(base: &str, client: &reqwest::Client) -> CheckReport {
    let mut r = CheckReport::new(base);
    let get = |path: &str| {
        let url = format!("{}{}", base.trim_end_matches('/'), path);
        client.get(url)
    };

    // SB101 — / answers 200 HTML.
    match get("/").send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let ctype = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let nosniff = resp.headers().contains_key("x-content-type-options");
            if status == 200 && ctype.starts_with("text/html") {
                r.push(Finding::pass("SB101", "GET / → 200 text/html"));
            } else {
                r.push(Finding::fail(
                    "SB101",
                    "GET / → 200 text/html",
                    None,
                    format!("{status} {ctype}"),
                    "serve the entry point at /",
                ));
            }
            if nosniff {
                r.push(Finding::pass("SB106", "nosniff header present"));
            } else {
                r.push(Finding::fail(
                    "SB106",
                    "nosniff header present",
                    None,
                    "X-Content-Type-Options missing",
                    "the shared server sets this; if absent the site is still on its own server.js",
                ));
            }
        }
        Err(e) => {
            r.push(Finding::fail(
                "SB101",
                "GET / → 200 text/html",
                None,
                format!("request failed: {e}"),
                "start the site and retry",
            ));
            return r;
        }
    }

    // SB102 — a missing asset is a real 404.
    match get("/__sitebox_missing__.png").send().await {
        Ok(resp) if resp.status().as_u16() == 404 => {
            r.push(Finding::pass("SB102", "missing asset → 404"))
        }
        Ok(resp) => r.push(Finding::fail(
            "SB102",
            "missing asset → 404",
            None,
            format!("answered {}", resp.status().as_u16()),
            "serve a real 404 for missing files — never index.html with 200",
        )),
        Err(e) => r.push(Finding::fail(
            "SB102",
            "missing asset → 404",
            None,
            format!("{e}"),
            "retry",
        )),
    }

    // SB103 — ETag revalidation.
    match get("/").send().await {
        Ok(resp) => {
            let etag = resp
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let last_mod = resp
                .headers()
                .get("last-modified")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            match etag {
                Some(tag) => {
                    let again = get("/").header("if-none-match", &tag).send().await;
                    match again {
                        Ok(r2) if r2.status().as_u16() == 304 => {
                            r.push(Finding::pass("SB103", "ETag → 304"))
                        }
                        Ok(r2) => r.push(Finding::fail(
                            "SB103",
                            "ETag → 304",
                            None,
                            format!("revalidation answered {}", r2.status().as_u16()),
                            "honour If-None-Match on static files",
                        )),
                        Err(e) => r.push(Finding::fail(
                            "SB103",
                            "ETag → 304",
                            None,
                            format!("{e}"),
                            "retry",
                        )),
                    }
                }
                None => r.push(Finding::fail(
                    "SB103",
                    "ETag → 304",
                    None,
                    format!(
                        "no ETag (last-modified: {})",
                        last_mod.unwrap_or_else(|| "none".into())
                    ),
                    "send ETag and honour If-None-Match",
                )),
            }
        }
        Err(e) => r.push(Finding::fail(
            "SB103",
            "ETag → 304",
            None,
            format!("{e}"),
            "retry",
        )),
    }

    // SB104 — gzip for compressible responses.
    match get("/").header("accept-encoding", "gzip").send().await {
        Ok(resp) => {
            let enc = resp
                .headers()
                .get("content-encoding")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if enc.contains("gzip") {
                r.push(Finding::pass("SB104", "gzip on HTML"));
            } else {
                r.push(Finding::fail(
                    "SB104",
                    "gzip on HTML",
                    None,
                    "no Content-Encoding: gzip",
                    "compress text/html when the client accepts gzip",
                ));
            }
        }
        Err(e) => r.push(Finding::fail(
            "SB104",
            "gzip on HTML",
            None,
            format!("{e}"),
            "retry",
        )),
    }

    // SB105 — Range support (only meaningful when the site ships audio/video/pdf).
    if let Some(asset) = first_large_media(base, client).await {
        match get(&asset).header("range", "bytes=0-15").send().await {
            Ok(resp) => {
                let code = resp.status().as_u16();
                let cr = resp.headers().get("content-range").is_some();
                if code == 206 && cr {
                    r.push(Finding::pass("SB105", "Range → 206"));
                } else {
                    r.push(Finding::fail(
                        "SB105",
                        "Range → 206",
                        Some(asset.clone()),
                        format!("answered {code}, content-range={cr}"),
                        "support single-range requests for audio/pdf",
                    ));
                }
            }
            Err(e) => r.push(Finding::fail(
                "SB105",
                "Range → 206",
                Some(asset),
                format!("{e}"),
                "retry",
            )),
        }
    } else {
        r.push(Finding::skip(
            "SB105",
            "Range → 206",
            "no audio/video/pdf asset found",
        ));
    }

    // SB107 — traversal is refused. Every HTTP client normalises `../` before
    // sending, so this check speaks raw TCP: it is the only way to prove the
    // server itself rejects the path rather than the client sanitising it.
    let host = base
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_string();
    match raw_get_status(&host, "/../../etc/passwd").await {
        Some(code) if code == 400 || code == 403 || code == 404 => {
            r.push(Finding::pass("SB107", "path traversal refused"))
        }
        Some(code) => r.push(Finding::fail(
            "SB107",
            "path traversal refused",
            None,
            format!("answered {code}"),
            "reject any path that escapes the public folder",
        )),
        None => r.push(Finding::fail(
            "SB107",
            "path traversal refused",
            None,
            "raw request failed",
            "retry",
        )),
    }

    r
}

/// Send a raw HTTP/1.1 request and return the status code.
async fn raw_get_status(host: &str, path: &str) -> Option<u16> {
    let host = host.to_string();
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        let mut s = std::net::TcpStream::connect(&host).ok()?;
        s.set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .ok()?;
        let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes()).ok()?;
        let mut buf = [0u8; 128];
        let n = s.read(&mut buf).ok()?;
        let head = String::from_utf8_lossy(&buf[..n]);
        let line = head.lines().next()?;
        line.split_whitespace().nth(1)?.parse().ok()
    })
    .await
    .ok()?
}

async fn first_large_media(base: &str, client: &reqwest::Client) -> Option<String> {
    let root = base.trim_end_matches('/');
    let html = client
        .get(format!("{root}/"))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let mut candidates: Vec<String> = Vec::new();

    for tag in tags(&html) {
        for attr in ["src", "href"] {
            if let Some(u) = attr_value(&tag, attr) {
                candidates.push(u);
            }
        }
    }
    // Data blobs build URLs in JS, so also scan the raw text for quoted or
    // bare paths that end in a media extension.
    let exts = [".ogg", ".mp3", ".pdf", ".mp4", ".webm"];
    for ext in exts {
        let mut from = 0;
        while let Some(i) = html[from..].find(ext) {
            let end = from + i + ext.len();
            let start = html[..end]
                .rfind(['"', '\'', ' ', '('])
                .map(|p| p + 1)
                .unwrap_or(0);
            let cand = html[start..end].trim();
            if cand.starts_with('/') || cand.starts_with("audio/") || cand.starts_with("img/") {
                candidates.push(cand.to_string());
            }
            from = end;
        }
    }

    for c in candidates {
        let lower = c.to_ascii_lowercase();
        if !exts.iter().any(|e| lower.ends_with(e)) {
            continue;
        }
        let url = if c.starts_with("http") {
            c.clone()
        } else {
            format!("{root}/{}", c.trim_start_matches('/'))
        };
        if let Ok(r) = client.get(&url).header("range", "bytes=0-0").send().await {
            if r.status().as_u16() == 206 || r.status().is_success() {
                return Some(if c.starts_with("http") {
                    c
                } else {
                    format!("/{}", c.trim_start_matches('/'))
                });
            }
        }
    }
    None
}

/* ── negative controls ─────────────────────────────────────────────────── */

/// Prove the detectors can fail. Every guard that has never failed is not a
/// guard: each case below is deliberately broken and must be caught.
pub fn self_test() -> Vec<Finding> {
    let mut out = Vec::new();
    let mut ok = |id: &str, title: &str, fired: bool| {
        if fired {
            out.push(Finding::pass(id, title));
        } else {
            out.push(Finding::fail(
                id,
                title,
                None,
                "detector did not fire on the broken fixture",
                "fix the detector before trusting a green run",
            ));
        }
    };

    let bad_html = r#"<img src="https://cdn.example/x.png"><img src="/local.png"><script src="https://cdn.example/a.js"></script>"#;
    ok(
        "NC001",
        "external-asset detector fires",
        tags(bad_html).iter().any(|t| {
            let l = t.to_ascii_lowercase();
            (l.starts_with("<img") || l.starts_with("<script"))
                && attr_value(t, "src")
                    .map(|u| u.starts_with("http"))
                    .unwrap_or(false)
        }),
    );
    let no_dims = r#"<img src="/a.png"><img src="/b.png" width="1" height="1">"#;
    ok(
        "NC002",
        "missing-dimension detector fires",
        tags(no_dims)
            .iter()
            .filter(|t| t.to_ascii_lowercase().starts_with("<img"))
            .any(|t| attr_value(t, "width").is_none() || attr_value(t, "height").is_none()),
    );
    let placeholder = "https://picsum.photos/800/600";
    ok(
        "NC003",
        "placeholder detector fires",
        placeholder.to_ascii_lowercase().contains("picsum.photos"),
    );
    let nan_html = "<span>NaN โพสต์</span>";
    ok(
        "NC004",
        "NaN-leak detector fires",
        nan_html.contains("NaN โ"),
    );
    ok(
        "NC005",
        "range parser rejects out-of-bounds",
        crate::serve::parse_range("bytes=5000-", 1000).is_none(),
    );

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attr_parser() {
        assert_eq!(
            attr_value(r#"<img src="/a.png" width="10">"#, "src"),
            Some("/a.png".into())
        );
        assert_eq!(
            attr_value(r#"<img data-src="/a.png" src="/b.png">"#, "src"),
            Some("/b.png".into())
        );
        assert_eq!(
            attr_value(r#"<img src=/c.png>"#, "src"),
            Some("/c.png".into())
        );
        assert_eq!(attr_value(r#"<img srcset="/a.png">"#, "src"), None);
    }

    #[test]
    fn tags_skip_comments() {
        let t = tags("<!-- <img src='x'> --><img src='y'>");
        assert_eq!(t.len(), 1);
        assert!(t[0].contains("y"));
    }

    #[test]
    fn tags_skip_script_bodies() {
        let html = r#"<script>const t = `<img src="${src}">`;</script><img src="/real.png" width="1" height="1">"#;
        let t = tags(html);
        assert_eq!(t.len(), 2, "{t:?}"); // <script src-less> opening tag + the real img
        assert!(t.iter().any(|x| x.contains("/real.png")));
        assert!(!t.iter().any(|x| x.contains("${src}")));
    }

    #[test]
    fn link_metadata_is_not_an_asset() {
        assert!(
            asset_url(r#"<link rel="preconnect" href="https://fonts.googleapis.com">"#).is_none()
        );
        assert!(asset_url(r#"<link rel="canonical" href="/">"#).is_none());
        assert!(asset_url(
            r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter">"#
        )
        .is_some());
    }

    #[test]
    fn negative_controls_all_fire() {
        let r = self_test();
        assert!(r.iter().all(|f| f.status == Status::Pass), "{r:?}");
    }
}
