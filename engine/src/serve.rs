//! One static server, used by every site.
//!
//! Differences from the 17 hand-written `server.js` copies it replaces:
//!   * traversal guard on every request (5 sites had none)
//!   * ETag + If-None-Match → 304 (5 sites had none)
//!   * gzip for text/json/svg (5 sites had none)
//!   * Range for audio/PDF (only 2 served it)
//!   * a real 404 — never a 200 with index.html for a missing asset
//!   * identical security headers everywhere

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

/// Port the dashboard listens on, published by `dashboard::serve`. The Sols
/// widget injected into every page needs it to call home.
pub static DASHBOARD_PORT: AtomicU16 = AtomicU16::new(0);

pub fn set_dashboard_port(port: u16) {
    DASHBOARD_PORT.store(port, Ordering::Relaxed);
}

fn sols_enabled() -> bool {
    if std::env::var("SOLS_WIDGET")
        .map(|v| v == "0")
        .unwrap_or(false)
    {
        return false;
    }
    true
}

#[derive(Clone, Debug)]
pub struct SolsCfg {
    pub dashboard_port: u16,
}

pub fn sols_config() -> Option<SolsCfg> {
    let port = DASHBOARD_PORT.load(Ordering::Relaxed);
    if !sols_enabled() || port == 0 {
        None
    } else {
        Some(SolsCfg {
            dashboard_port: port,
        })
    }
}

#[derive(Clone)]
pub struct ServeState {
    pub root: PathBuf,
    pub site_id: String,
    pub api_upstream: Option<String>,
    pub client: reqwest::Client,
    /// When set, HTML documents get the Sols launcher injected on the way out.
    pub sols: Option<SolsCfg>,
}

const SOLS_WIDGET: &str = include_str!("../assets/sols-widget.js");

const COMPRESSIBLE: &[&str] = &[
    "text/html",
    "text/css",
    "text/plain",
    "application/javascript",
    "application/json",
    "image/svg+xml",
    "application/xml",
    "text/xml",
];

pub fn router(state: Arc<ServeState>) -> Router {
    Router::new().fallback(handle).with_state(state)
}

/// Bind and serve until the process is stopped.
pub async fn serve(port: u16, state: ServeState) -> Result<()> {
    let app = router(Arc::new(state.clone()));
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    eprintln!(
        "{} running at http://localhost:{port} (root {})",
        state.site_id,
        state.root.display()
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle(State(st): State<Arc<ServeState>>, req: Request) -> Response {
    let path = req.uri().path().to_string();
    if path == "/__sols/widget.js" {
        return (
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/javascript; charset=utf-8"),
                ),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
                (
                    header::X_CONTENT_TYPE_OPTIONS,
                    HeaderValue::from_static("nosniff"),
                ),
            ],
            SOLS_WIDGET,
        )
            .into_response();
    }
    if path.starts_with("/api/") {
        if let Some(up) = &st.api_upstream {
            return proxy(st.clone(), req, up.clone()).await;
        }
        return not_found(&path);
    }
    serve_static(&st, &path, req.method(), req.headers()).await
}

async fn serve_static(
    st: &ServeState,
    path: &str,
    method: &Method,
    headers: &HeaderMap,
) -> Response {
    let head_only = method == Method::HEAD;
    if method != Method::GET && !head_only {
        return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response();
    }

    let mut rel = path.to_string();
    if rel.ends_with('/') {
        rel.push_str("index.html");
    }
    let Some(mut fp) = crate::util::safe_join(&st.root, &rel) else {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    };

    let mut meta = tokio::fs::metadata(&fp).await.ok();
    if meta.as_ref().map(|m| m.is_dir()).unwrap_or(false) {
        fp = fp.join("index.html");
        meta = tokio::fs::metadata(&fp).await.ok();
    }

    // Missing asset: a real 404. Only extension-less routes fall back to the
    // SPA entry point.
    let Some(md) = meta.filter(|m| m.is_file()) else {
        if Path::new(path).extension().is_none() {
            let idx = st.root.join("index.html");
            if let Ok(m) = tokio::fs::metadata(&idx).await {
                if m.is_file() {
                    return file_response(st, &idx, &m, headers, head_only).await;
                }
            }
        }
        return not_found(path);
    };

    file_response(st, &fp, &md, headers, head_only).await
}

async fn file_response(
    st: &ServeState,
    fp: &Path,
    md: &std::fs::Metadata,
    headers: &HeaderMap,
    head_only: bool,
) -> Response {
    let size = md.len();
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let etag = format!("\"{size:x}-{mtime:x}\"");
    let mime_raw = mime_guess::from_path(fp)
        .first_or_octet_stream()
        .to_string();
    let mime = if mime_raw.starts_with("text/")
        || matches!(
            mime_raw.as_str(),
            "application/javascript" | "application/json" | "application/xml" | "image/svg+xml"
        ) {
        format!("{mime_raw}; charset=utf-8")
    } else {
        mime_raw.clone()
    };
    let is_compressible = COMPRESSIBLE.iter().any(|c| mime_raw.starts_with(c));

    // Documents carry the Sols launcher. The injected bytes change the body,
    // so the ETag gets a suffix and Range is not offered for this response.
    if mime_raw.eq_ignore_ascii_case("text/html") {
        if let Some(cfg) = &st.sols {
            if let Ok(raw) = tokio::fs::read(fp).await {
                return sols_html(st, &raw, md, headers, head_only, cfg);
            }
        }
    }

    let mut base = HeaderMap::new();
    base.insert(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap());
    base.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
    base.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    base.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    base.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    base.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    if is_compressible {
        base.insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    }
    if let Ok(lm) = md.modified() {
        if let Ok(d) = lm.duration_since(UNIX_EPOCH) {
            if let Ok(v) = HeaderValue::from_str(&httpdate(d.as_secs())) {
                base.insert(header::LAST_MODIFIED, v);
            }
        }
    }

    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').any(|t| t.trim() == etag))
        .unwrap_or(false)
    {
        return (StatusCode::NOT_MODIFIED, base).into_response();
    }

    // Range takes precedence over compression.
    if let Some(range) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        if let Some((start, end)) = parse_range(range, size) {
            let len = end - start + 1;
            let mut h = base.clone();
            h.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {start}-{end}/{size}")).unwrap(),
            );
            h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
            let body = if head_only {
                Body::empty()
            } else {
                Body::from_stream(slice_stream(fp.to_path_buf(), start, len))
            };
            return (StatusCode::PARTIAL_CONTENT, h, body).into_response();
        }
        return (
            StatusCode::RANGE_NOT_SATISFIABLE,
            [(header::CONTENT_RANGE, format!("bytes */{size}"))],
            "range not satisfiable",
        )
            .into_response();
    }

    // Small compressible files are gzipped in memory; everything else streams.
    let wants_gzip = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("gzip"))
        .unwrap_or(false);
    if is_compressible && wants_gzip && size <= 8 * 1024 * 1024 {
        match tokio::fs::read(fp).await {
            Ok(raw) => {
                let mut enc = GzEncoder::new(Vec::new(), Compression::new(6));
                if enc.write_all(&raw).is_err() {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "gzip failed").into_response();
                }
                let gz = enc.finish().unwrap_or_default();
                let mut h = base.clone();
                h.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
                h.insert(header::CONTENT_LENGTH, HeaderValue::from(gz.len()));
                let body = if head_only {
                    Body::empty()
                } else {
                    Body::from(gz)
                };
                return (StatusCode::OK, h, body).into_response();
            }
            Err(_) => return (StatusCode::NOT_FOUND, "Not Found").into_response(),
        }
    }

    let mut h = base.clone();
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(size));
    let body = if head_only {
        Body::empty()
    } else {
        Body::from_stream(slice_stream(fp.to_path_buf(), 0, size))
    };
    (StatusCode::OK, h, body).into_response()
}

/// Serve an HTML document with the Sols launcher appended.
///
/// Inline script + a tiny external file keeps the site's own markup untouched:
/// the file on disk stays byte-identical, and only this response is different.
fn sols_html(
    st: &ServeState,
    raw: &[u8],
    md: &std::fs::Metadata,
    headers: &HeaderMap,
    head_only: bool,
    cfg: &SolsCfg,
) -> Response {
    let html = String::from_utf8_lossy(raw);
    let site = serde_json::to_string(&st.site_id).unwrap_or_else(|_| "\"\"".to_string());
    let snippet = format!(
        "<script>window.__SOLS={{\"site\":{site},\"port\":{},\"v\":1}};</script>\
         <script src=\"/__sols/widget.js\" defer></script>",
        cfg.dashboard_port
    );
    // `to_ascii_lowercase` preserves byte offsets for ASCII, so the index is
    // valid in the original UTF-8 string.
    let out = match html.to_ascii_lowercase().rfind("</body>") {
        Some(i) => format!("{}{}{}", &html[..i], snippet, &html[i..]),
        None => format!("{html}{snippet}"),
    };
    let bytes = out.into_bytes();

    let size = md.len();
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let etag = format!("\"{size:x}-{mtime:x}-sols1\"");

    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    h.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));

    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').any(|t| t.trim() == etag))
        .unwrap_or(false)
    {
        return (StatusCode::NOT_MODIFIED, h).into_response();
    }

    let wants_gzip = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("gzip"))
        .unwrap_or(false);
    if wants_gzip {
        let mut enc = GzEncoder::new(Vec::new(), Compression::new(6));
        if enc.write_all(&bytes).is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "gzip failed").into_response();
        }
        let gz = enc.finish().unwrap_or_default();
        h.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        h.insert(header::CONTENT_LENGTH, HeaderValue::from(gz.len()));
        let body = if head_only {
            Body::empty()
        } else {
            Body::from(gz)
        };
        return (StatusCode::OK, h, body).into_response();
    }

    h.insert(header::CONTENT_LENGTH, HeaderValue::from(bytes.len()));
    let body = if head_only {
        Body::empty()
    } else {
        Body::from(bytes)
    };
    (StatusCode::OK, h, body).into_response()
}

fn slice_stream(
    path: PathBuf,
    start: u64,
    len: u64,
) -> impl futures_core::Stream<Item = std::io::Result<bytes::Bytes>> {
    async_stream::stream! {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let mut f = match tokio::fs::File::open(&path).await {
            Ok(f) => f,
            Err(e) => { yield Err(e); return; }
        };
        if start > 0 && f.seek(std::io::SeekFrom::Start(start)).await.is_err() {
            yield Err(std::io::Error::new(std::io::ErrorKind::Other, "seek failed"));
            return;
        }
        let mut remaining = len;
        let mut buf = vec![0u8; 64 * 1024];
        while remaining > 0 {
            let want = (buf.len() as u64).min(remaining) as usize;
            match f.read(&mut buf[..want]).await {
                Ok(0) => break,
                Ok(n) => {
                    remaining -= n as u64;
                    yield Ok(bytes::Bytes::copy_from_slice(&buf[..n]));
                }
                Err(e) => { yield Err(e); return; }
            }
        }
    }
}

fn httpdate(secs: u64) -> String {
    // RFC 7231 IMF-fixdate.
    let dt = chrono::DateTime::from_timestamp(secs as i64, 0).unwrap_or_default();
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

pub fn parse_range(h: &str, size: u64) -> Option<(u64, u64)> {
    let spec = h.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None; // multi-range unsupported, answered as full 206? keep single
    }
    let (a, b) = spec.split_once('-')?;
    if a.is_empty() {
        let suffix: u64 = b.parse().ok()?;
        if suffix == 0 || size == 0 {
            return None;
        }
        let start = size.saturating_sub(suffix);
        return Some((start, size - 1));
    }
    let start: u64 = a.parse().ok()?;
    if start >= size {
        return None;
    }
    let end = if b.is_empty() {
        size - 1
    } else {
        b.parse::<u64>().ok()?.min(size - 1)
    };
    if end < start {
        return None;
    }
    Some((start, end))
}

fn not_found(path: &str) -> Response {
    let safe = path.replace(['<', '>', '&'], "");
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>404</title><div style=\"font:15px system-ui;padding:40px\"><h1>404</h1><p>ไม่พบ {safe}</p></div>"
    );
    (
        StatusCode::NOT_FOUND,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response()
}

async fn proxy(st: Arc<ServeState>, req: Request, upstream: String) -> Response {
    let (parts, body) = req.into_parts();
    let url = format!(
        "{}{}",
        upstream.trim_end_matches('/'),
        parts
            .uri
            .path_and_query()
            .map(|p| p.as_str())
            .unwrap_or("/")
    );
    let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())
        .unwrap_or(reqwest::Method::GET);
    let bytes = match axum::body::to_bytes(body, 32 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "body too large").into_response(),
    };
    let mut rb = st.client.request(method, &url);
    for (k, v) in parts.headers.iter() {
        if k == header::HOST || k == header::CONTENT_LENGTH {
            continue;
        }
        rb = rb.header(k.as_str(), v.as_bytes());
    }
    let resp = match rb.body(bytes).send().await {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")).into_response(),
    };
    let status = resp.status();
    let mut out = HeaderMap::new();
    for (k, v) in resp.headers().iter() {
        if k == header::TRANSFER_ENCODING || k == header::CONNECTION {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    let body = resp.bytes().await.unwrap_or_default();
    (status, out, Body::from(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_parsing() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=0-5000", 1000), Some((0, 999)));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("bytes=10-5", 1000), None);
    }
}
