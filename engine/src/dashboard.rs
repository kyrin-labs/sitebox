//! The dashboard API — parity with `dashboard/server.js`, on the Rust engine.
//!
//! The UI is unchanged: it still talks to the same endpoints. What changed is
//! who owns the behaviour: manifests, the shared server, the supervisor and the
//! check catalog live in the engine, and this module is only HTTP glue.

use anyhow::Result;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use crate::manifest::Manifest;
use crate::registry::{validate_id, Site, SiteConfig};
use crate::studio::{self, StudioState};
use crate::supervisor::Supervisor;
use crate::util;

#[derive(Clone)]
pub struct DashState {
    pub root: PathBuf,
    pub ui: PathBuf,
    pub sup: Arc<Supervisor>,
    pub client: reqwest::Client,
    pub registry_error: Arc<tokio::sync::Mutex<Option<String>>>,
    pub studio: StudioState,
}

pub fn router(state: DashState) -> Router {
    Router::new()
        .route("/api/sites", get(list_sites).post(upsert_site))
        .route("/api/sites/reorder", post(reorder))
        .route("/api/sites/{id}/start", post(start))
        .route("/api/sites/{id}/stop", post(stop))
        .route("/api/sites/{id}/restart", post(restart))
        .route("/api/sites/{id}/logs", get(logs).delete(clear_logs))
        .route("/api/sites/{id}/health", get(health))
        .route("/api/sites/{id}/icon", get(icon))
        .route("/api/sites/{id}", delete(remove))
        .route("/api/ports/check", get(port_check))
        .route("/api/doctor", get(doctor_route))
        .route("/api/studio/status", get(studio_status))
        .route("/api/studio/config", get(studio_config_get).put(studio_config_put))
        .route("/api/studio/threads", get(studio_threads).post(studio_thread_create))
        .route("/api/studio/threads/{id}/messages", get(studio_thread_messages))
        .route("/api/studio/events", get(studio_events))
        .route("/api/studio/chat", post(studio_chat))
        .route("/api/studio/abort", post(studio_abort))
        .route("/api/studio/sites/{id}/outline", get(studio_outline))
        .route("/api/studio/sites/{id}/history", get(studio_history))
        .route("/api/studio/sites/{id}/undo", post(studio_undo))
        .route("/api/studio/patches/{id}/apply", post(studio_apply))
        .route("/api/studio/shots/{site}/{file}", get(studio_shot_file))
        .fallback(static_ui)
        .layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(middleware::from_fn(cors))
        .layer(middleware::from_fn_with_state(state.clone(), studio_guard))
        .with_state(state)
}

async fn cors(req: Request, next: Next) -> Response {
    let is_api = req.uri().path().starts_with("/api/");
    let mut res = next.run(req).await;
    if is_api {
        let h = res.headers_mut();
        h.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
        h.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET,POST,PUT,DELETE,OPTIONS"),
        );
        h.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Content-Type"),
        );
    }
    res
}

fn manifest_for(root: &FsPath, id: &str, registry_path: &str) -> Option<Manifest> {
    Manifest::load(&root.join("sites").join(id))
        .ok()
        .or_else(|| Manifest::load(&root.join(registry_path)).ok())
}

fn cfg(state: &DashState) -> Result<SiteConfig, String> {
    SiteConfig::read(&state.root).map_err(|e| format!("{e:#}"))
}

async fn list_sites(State(st): State<DashState>) -> Response {
    let Ok(mut c) = cfg(&st) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"sites.json unreadable"})),
        )
            .into_response();
    };
    if let Ok(n) = c.auto_detect(&st.root) {
        if n > 0 {
            let _ = c.write(&st.root);
        }
    }
    let mut out = Vec::new();
    for s in &c.sites {
        let dir = st.root.join(&s.path);
        let manifest = manifest_for(&st.root, &s.id, &s.path);
        let public_dir = manifest
            .as_ref()
            .map(|m| m.public_dir(&st.root))
            .unwrap_or_else(|| dir.join("public"));
        let stale = manifest
            .as_ref()
            .map(|m| !m.public_dir(&st.root).is_dir() && !dir.join("server.js").is_file())
            .unwrap_or_else(|| !dir.is_dir());
        let fav = detect_favicon(&public_dir);
        out.push(json!({
            "id": s.id, "name": s.name, "description": s.description,
            "icon": s.icon, "iconColor": s.icon_color, "port": s.port,
            "category": s.category, "url": s.url, "path": s.path, "created": s.created,
            "_running": st.sup.is_running(&s.id).await,
            "_stale": stale,
            "_logLines": st.sup.log_count(&s.id).await,
            "_manifest": manifest.is_some(),
            "_serve": manifest.as_ref().map(|m| m.site.serve.clone()),
            "_favicon": fav.as_ref().map(|_| format!("/api/sites/{}/icon", s.id)),
            "_faviconSource": fav.as_ref().map(|p| p.strip_prefix(&st.root).unwrap_or(p).to_string_lossy().to_string()),
        }));
    }
    // Manifest-only sites (scaffolded directly, not registered yet) are still
    // real sites; report them instead of hiding them.
    let known: Vec<String> = c.sites.iter().map(|s| s.id.clone()).collect();
    for p in crate::registry::manifests(&st.root).unwrap_or_default() {
        let Some(m) = Manifest::load(p.parent().unwrap_or(&p)).ok() else {
            continue;
        };
        if known.contains(&m.site.id) {
            continue;
        }
        let public_dir = m.public_dir(&st.root);
        let fav = detect_favicon(&public_dir);
        out.push(json!({
            "id": m.site.id, "name": m.site.name, "description": m.site.description,
            "icon": m.site.icon, "iconColor": m.site.icon_color, "port": m.site.port,
            "category": m.site.category, "url": format!("http://localhost:{}", m.site.port),
            "path": m.site.path, "created": m.site.created,
            "_running": st.sup.is_running(&m.site.id).await,
            "_stale": !public_dir.is_dir(),
            "_logLines": st.sup.log_count(&m.site.id).await,
            "_manifest": true,
            "_unregistered": true,
            "_serve": m.site.serve,
            "_favicon": fav.as_ref().map(|_| format!("/api/sites/{}/icon", m.site.id)),
            "_faviconSource": fav.as_ref().map(|p| p.strip_prefix(&st.root).unwrap_or(p).to_string_lossy().to_string()),
        }));
    }
    Json(Value::Array(out)).into_response()
}

async fn upsert_site(
    State(st): State<DashState>,
    body: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Ok(Json(body)) = body else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid JSON body"})),
        )
            .into_response();
    };
    let Some(id) = body.get("id").and_then(|v| v.as_str()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"id required"})),
        )
            .into_response();
    };
    if let Err(e) = validate_id(id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        )
            .into_response();
    }
    let Ok(mut c) = cfg(&st) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"sites.json unreadable"})),
        )
            .into_response();
    };
    let port = body.get("port").and_then(|v| v.as_u64()).unwrap_or(0);
    if !(1024..=65535).contains(&port) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"port must be an integer between 1024 and 65535"})),
        )
            .into_response();
    }
    let port = port as u16;
    if let Some(clash) = c.sites.iter().find(|s| s.id != id && s.port == port) {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": format!("port {port} is already assigned to \"{}\"", clash.id)})),
        )
            .into_response();
    }
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"name required"})),
        )
            .into_response();
    }
    let existing = c.sites.iter().position(|s| s.id == id);
    let mut site = existing.map(|i| c.sites[i].clone()).unwrap_or_default();
    site.id = id.into();
    site.name = name;
    site.port = port;
    for (k, field) in [
        ("description", &mut site.description),
        ("icon", &mut site.icon),
        ("iconColor", &mut site.icon_color),
        ("category", &mut site.category),
        ("url", &mut site.url),
        ("path", &mut site.path),
    ] {
        if let Some(v) = body.get(k).and_then(|v| v.as_str()) {
            *field = v.to_string();
        }
    }
    if site.path.is_empty() {
        site.path = format!("sites/{id}");
    }
    if site.url.is_empty() {
        site.url = format!("http://localhost:{port}");
    }
    if site.created.is_empty() {
        site.created = util::today();
    }
    match existing {
        Some(i) => c.sites[i] = site.clone(),
        None => c.sites.push(site.clone()),
    }
    if let Err(e) = c.write(&st.root) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("{e:#}")})),
        )
            .into_response();
    }
    // Keep the contract under root/sites/<id>/ — never inside the registry
    // path, which may point at a production folder imported read-only.
    let dir = st.root.join("sites").join(&site.id);
    let _ = std::fs::create_dir_all(&dir);
    let mut m = Manifest::load(&dir).unwrap_or_else(|_| Manifest::from_site(&site));
    m.site.port = site.port;
    m.site.name = site.name.clone();
    m.site.path = site.path.clone();
    let _ = m.save(&dir);
    Json(json!({"ok": true, "site": site})).into_response()
}

async fn reorder(
    State(st): State<DashState>,
    body: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Ok(Json(body)) = body else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid JSON body"})),
        )
            .into_response();
    };
    let Some(order) = body.get("order").and_then(|v| v.as_array()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"order array required"})),
        )
            .into_response();
    };
    let ids: Vec<String> = order
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    let Ok(mut c) = cfg(&st) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"sites.json unreadable"})),
        )
            .into_response();
    };
    let mut next: Vec<Site> = ids
        .iter()
        .filter_map(|id| c.sites.iter().find(|s| &s.id == id).cloned())
        .collect();
    for s in &c.sites {
        if !ids.contains(&s.id) {
            next.push(s.clone());
        }
    }
    c.sites = next;
    let _ = c.write(&st.root);
    Json(json!({"ok": true})).into_response()
}

fn load(st: &DashState, id: &str) -> Result<Manifest, Response> {
    crate::supervisor::load_manifest(&st.root, id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("{e:#}")})),
        )
            .into_response()
    })
}

async fn start(State(st): State<DashState>, Path(id): Path<String>) -> Response {
    let m = match load(&st, &id) {
        Ok(m) => m,
        Err(r) => return r,
    };
    match st.sup.start(&m).await {
        Ok(r) => Json(serde_json::to_value(r).unwrap_or_default()).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("{e:#}")})),
        )
            .into_response(),
    }
}

async fn stop(State(st): State<DashState>, Path(id): Path<String>) -> Response {
    match st.sup.stop(&id).await {
        Ok(()) => Json(json!({"ok": true, "message": "stopped"})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("{e:#}")})),
        )
            .into_response(),
    }
}

async fn restart(State(st): State<DashState>, Path(id): Path<String>) -> Response {
    let m = match load(&st, &id) {
        Ok(m) => m,
        Err(r) => return r,
    };
    match st.sup.restart(&m).await {
        Ok(r) => Json(serde_json::to_value(r).unwrap_or_default()).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("{e:#}")})),
        )
            .into_response(),
    }
}

async fn logs(
    State(st): State<DashState>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let limit = q
        .get("lines")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(200)
        .clamp(1, 500);
    let lines = st.sup.logs(&id, limit).await;
    Json(json!({
        "id": id,
        "running": st.sup.is_running(&id).await,
        "count": st.sup.log_count(&id).await,
        "lines": lines,
    }))
    .into_response()
}

async fn clear_logs(State(st): State<DashState>, Path(id): Path<String>) -> Response {
    st.sup.clear_logs(&id).await;
    Json(json!({"ok": true, "cleared": true})).into_response()
}

async fn health(State(st): State<DashState>, Path(id): Path<String>) -> Response {
    let Ok(c) = cfg(&st) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"sites.json unreadable"})),
        )
            .into_response();
    };
    let (url, _port) = if let Some(site) = c.find(&id) {
        (
            if site.url.is_empty() {
                format!("http://127.0.0.1:{}", site.port)
            } else {
                site.url.clone()
            },
            site.port,
        )
    } else if let Some(m) = manifest_for(&st.root, &id, "") {
        (format!("http://127.0.0.1:{}", m.site.port), m.site.port)
    } else {
        return (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))).into_response();
    };
    match st
        .client
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(r) => Json(json!({"online": r.status().as_u16() < 500, "status": r.status().as_u16()}))
            .into_response(),
        Err(_) => Json(json!({"online": false, "status": 0})).into_response(),
    }
}

async fn icon(State(st): State<DashState>, Path(id): Path<String>) -> Response {
    let Ok(c) = cfg(&st) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"sites.json unreadable"})),
        )
            .into_response();
    };
    let public = if let Some(site) = c.find(&id) {
        st.root.join(&site.path).join("public")
    } else if let Some(m) = manifest_for(&st.root, &id, "") {
        m.public_dir(&st.root)
    } else {
        return (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))).into_response();
    };
    let Some(fp) = detect_favicon(&public) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"no favicon detected"})),
        )
            .into_response();
    };
    let Ok(md) = std::fs::metadata(&fp) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"no favicon detected"})),
        )
            .into_response();
    };
    let mime = mime_guess::from_path(&fp)
        .first_or_octet_stream()
        .to_string();
    let body = std::fs::read(&fp).unwrap_or_default();
    let etag = format!(
        "\"{:x}-{:x}\"",
        md.len(),
        md.modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::ETAG, etag),
            (header::CACHE_CONTROL, "no-cache".into()),
        ],
        body,
    )
        .into_response()
}

async fn remove(
    State(st): State<DashState>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Ok(mut c) = cfg(&st) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"sites.json unreadable"})),
        )
            .into_response();
    };
    let Some(site) = c.find(&id).cloned() else {
        return (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))).into_response();
    };
    let purge = q.get("keep").map(|v| v != "1").unwrap_or(true);
    let mut abs: Option<PathBuf> = None;
    if purge {
        let p = st.root.join(&site.path);
        let sites_dir = st.root.join("sites");
        let canon_parent = p.parent().map(|x| x.to_path_buf()).unwrap_or_default();
        if !p.starts_with(&sites_dir) || p == sites_dir || canon_parent != sites_dir {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"refusing to purge: path is outside sites/", "hint": format!("entry path is \"{}\"", site.path)})),
            )
                .into_response();
        }
        abs = Some(p);
    }
    st.sup.stop(&id).await.ok();
    if let Some(p) = abs {
        let _ = std::fs::remove_dir_all(p);
    }
    c.sites.retain(|s| s.id != id);
    let _ = c.write(&st.root);
    st.sup.clear_logs(&id).await;
    Json(json!({
        "ok": true,
        "purged": purge,
        "note": if purge { "site stopped, folder deleted, entry removed" } else { "config removed only — the folder is still on disk" }
    }))
    .into_response()
}

async fn port_check(Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(port) = q.get("port").and_then(|v| v.parse::<u32>().ok()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid port"})),
        )
            .into_response();
    };
    if !(1..=65535).contains(&port) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid port"})),
        )
            .into_response();
    }
    let used = util::port_bound(port as u16);
    Json(json!({"port": port, "used": used, "available": !used})).into_response()
}

async fn doctor_route(State(st): State<DashState>) -> Response {
    Json(crate::mcp::doctor(&st.root, &st.sup).await).into_response()
}

fn detect_favicon(public: &FsPath) -> Option<PathBuf> {
    let index = public.join("index.html");
    if let Ok(html) = std::fs::read_to_string(&index) {
        for tag in html.split('<') {
            let lower = tag.to_ascii_lowercase();
            if !lower.starts_with("link") || !lower.contains("icon") {
                continue;
            }
            if let Some(href) = attr(tag, "href") {
                if href.starts_with("http") || href.starts_with("data:") || href.starts_with("//") {
                    continue;
                }
                let clean = href.split(['?', '#']).next().unwrap_or("");
                let p = public.join(clean.trim_start_matches('/'));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    for name in [
        "favicon.svg",
        "favicon.png",
        "favicon.ico",
        "favicon.webp",
        "icon.svg",
        "icon.png",
    ] {
        let p = public.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{name}=");
    let i = lower.find(&needle)?;
    let rest = &tag[i + needle.len()..];
    let v = if let Some(s) = rest.strip_prefix('"') {
        s.split('"').next()
    } else if let Some(s) = rest.strip_prefix('\'') {
        s.split('\'').next()
    } else {
        rest.split([' ', '>', '\n', '\t']).next()
    };
    v.map(|s| s.trim().to_string())
}

// ── Studio ──────────────────────────────────────────────────────────────────

/// Studio has its own door: a per-process cookie for the dashboard UI, or an
/// `x-studio-token` for scripts when `STUDIO_TOKEN` is set. The rest of the
/// dashboard keeps its existing (unauthenticated) LAN contract, but nothing that
/// can write a site answers without one of these.
async fn studio_guard(State(st): State<DashState>, req: Request, next: Next) -> Response {
    if !req.uri().path().starts_with("/api/studio/") {
        return next.run(req).await;
    }
    let header_ok = st
        .studio
        .token
        .as_deref()
        .map(|t| req.headers().get("x-studio-token").and_then(|v| v.to_str().ok()) == Some(t))
        .unwrap_or(false);
    let want = format!("studio_sid={}", st.studio.sid);
    let cookie_ok = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|c| c.split(';').any(|p| p.trim() == want))
        .unwrap_or(false);
    if header_ok || cookie_ok {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "studio session required"})),
        )
            .into_response()
    }
}

async fn studio_status(State(st): State<DashState>) -> Response {
    let cfg = studio::config::Settings::load(&st.root);
    let sites: Vec<Value> = match SiteConfig::read(&st.root) {
        Ok(c) => c
            .sites
            .iter()
            .filter(|s| st.root.join(&s.path).is_dir())
            .map(|s| json!({"id": s.id, "name": s.name, "port": s.port}))
            .collect(),
        Err(_) => Vec::new(),
    };
    Json(json!({
        "settings": cfg.public_json(),
        "browser": studio::shot::browser().map(|p| p.display().to_string()),
        "sites": sites,
        "data_dir": st.studio.data,
    }))
    .into_response()
}

async fn studio_config_get(State(st): State<DashState>) -> Response {
    let cfg = studio::config::Settings::load(&st.root);
    Json(cfg.public_json()).into_response()
}

async fn studio_config_put(State(st): State<DashState>, Json(body): Json<Value>) -> Response {
    let mut cfg = studio::config::Settings::load(&st.root);
    cfg.merge(&body);
    match cfg.save(&st.root) {
        Ok(()) => Json(cfg.public_json()).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e:#}")}))).into_response(),
    }
}

async fn studio_threads(
    State(st): State<DashState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let list = studio::store::list(&st.studio, q.get("site").map(|s| s.as_str()));
    Json(list).into_response()
}

async fn studio_thread_create(State(st): State<DashState>, Json(body): Json<Value>) -> Response {
    let site_id = body.get("site_id").and_then(|v| v.as_str()).unwrap_or("");
    if let Err(e) = studio::site_ref(&st.root, site_id) {
        return (StatusCode::NOT_FOUND, Json(json!({"error": format!("{e:#}")}))).into_response();
    }
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("");
    match studio::store::create(&st.studio, site_id, title) {
        Ok(t) => Json(t).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("{e:#}")}))).into_response(),
    }
}

async fn studio_thread_messages(State(st): State<DashState>, Path(id): Path<String>) -> Response {
    match studio::store::get(&st.studio, &id) {
        Ok(t) => {
            let msgs = studio::store::messages(&st.studio, &id).unwrap_or_default();
            Json(json!({"thread": t, "messages": msgs})).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "no such thread"}))).into_response(),
    }
}

async fn studio_events(
    State(st): State<DashState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let want = q.get("thread").cloned().unwrap_or_default();
    let mut rx = st.studio.events.subscribe();
    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().data(json!({"type":"hello"}).to_string()));
        loop {
            match rx.recv().await {
                Ok(v) => {
                    let tid = v.get("thread").and_then(|t| t.as_str()).unwrap_or("");
                    if want.is_empty() || tid == want {
                        yield Ok(Event::default().data(v.to_string()));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

async fn studio_chat(State(st): State<DashState>, Json(body): Json<Value>) -> Response {
    let thread_id = body.get("thread_id").and_then(|v| v.as_str()).unwrap_or("");
    let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("").trim();
    if message.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "message is empty"}))).into_response();
    }
    if message.chars().count() > 8000 {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "message is too long (8000 characters)"}))).into_response();
    }
    let thread = match studio::store::get(&st.studio, thread_id) {
        Ok(t) => t,
        Err(_) => return (StatusCode::NOT_FOUND, Json(json!({"error": "no such thread"}))).into_response(),
    };
    if !studio::config::Settings::load(&st.root).configured() {
        return (StatusCode::CONFLICT, Json(json!({"error": "agent is not configured"}))).into_response();
    }
    {
        let mut busy = st.studio.busy.lock().await;
        if !busy.insert(thread.site_id.clone()) {
            return (StatusCode::CONFLICT, Json(json!({"error": format!("a turn is already running on {}", thread.site_id)}))).into_response();
        }
    }
    {
        // 60 turns per site per hour. A loop that retries on failure is the
        // normal way to burn a budget; this is the cheap guard, not a quota.
        let mut log = st.studio.turn_log.lock().await;
        let now = std::time::Instant::now();
        let entry = log.entry(thread.site_id.clone()).or_default();
        entry.retain(|t| now.duration_since(*t).as_secs() < 3600);
        if entry.len() >= 60 {
            st.studio.busy.lock().await.remove(&thread.site_id);
            return (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error": "60 turns per hour on this site; wait before sending more"}))).into_response();
        }
        entry.push(now);
    }
    let stc = st.studio.clone();
    let tid = thread_id.to_string();
    let msg = message.to_string();
    let handle = tokio::spawn(async move { studio::run_turn(stc, tid, msg).await; });
    st.studio.turns.lock().await.insert(thread_id.to_string(), handle);
    (StatusCode::ACCEPTED, Json(json!({"ok": true, "thread_id": thread_id}))).into_response()
}

async fn studio_abort(State(st): State<DashState>, Json(body): Json<Value>) -> Response {
    let thread_id = body.get("thread_id").and_then(|v| v.as_str()).unwrap_or("");
    let site = studio::store::get(&st.studio, thread_id).map(|t| t.site_id).ok();
    if let Some(h) = st.studio.turns.lock().await.remove(thread_id) {
        h.abort();
    }
    if let Some(s) = site {
        st.studio.busy.lock().await.remove(&s);
    }
    studio::emit(&st.studio.events, thread_id, json!({"type":"aborted"}));
    Json(json!({"ok": true})).into_response()
}

async fn studio_outline(
    State(st): State<DashState>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let site = match studio::site_ref(&st.root, &id) {
        Ok(s) => s,
        Err(e) => return (StatusCode::NOT_FOUND, Json(json!({"error": format!("{e:#}")}))).into_response(),
    };
    if let Some(path) = q.get("path").filter(|p| !p.is_empty()) {
        return match studio::outline::items(&site.dir, path) {
            Ok(items) => Json(json!({"path": path, "items": items})).into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error": format!("{e:#}")}))).into_response(),
        };
    }
    let files = studio::outline::source_files(&site.dir, 40);
    let main = ["public/index.html", "index.html"]
        .iter()
        .find(|p| site.dir.join(p).is_file())
        .map(|p| p.to_string());
    let items = main
        .as_ref()
        .and_then(|p| studio::outline::items(&site.dir, p).ok())
        .unwrap_or_default();
    Json(json!({
        "site": site.id,
        "kind": site.kind,
        "files": files,
        "main": main,
        "items": items,
    }))
    .into_response()
}

async fn studio_history(State(st): State<DashState>, Path(id): Path<String>) -> Response {
    let snaps = studio::snapshot::list(&st.studio, &id);
    let patches = studio::store::list_patches(&st.studio, &id, 30);
    Json(json!({"snapshots": snaps, "patches": patches})).into_response()
}

async fn studio_apply(State(st): State<DashState>, Path(id): Path<String>) -> Response {
    match studio::apply::apply(&st.studio, &id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": format!("{e:#}")}))).into_response(),
    }
}

async fn studio_undo(State(st): State<DashState>, Path(id): Path<String>) -> Response {
    match studio::apply::undo(&st.studio, &id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": format!("{e:#}")}))).into_response(),
    }
}

async fn studio_shot_file(
    State(st): State<DashState>,
    Path((site, file)): Path<(String, String)>,
) -> Response {
    let Some(p) = util::safe_join(&st.studio.data.join("shots").join(&site), &file) else {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    };
    match tokio::fs::read(&p).await {
        Ok(b) => ([(header::CONTENT_TYPE, "image/png")], b).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

async fn static_ui(State(st): State<DashState>, req: Request) -> Response {
    static_ui_file(&st, req).await
}

async fn static_ui_file(st: &DashState, req: Request) -> Response {
    let path = req.uri().path();
    let mut rel = path.trim_start_matches('/').to_string();
    if rel.is_empty() {
        rel = "index.html".into();
    }
    let Some(fp) = crate::util::safe_join(&st.ui, &rel) else {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    };
    let body = match tokio::fs::read(&fp).await {
        Ok(b) => b,
        Err(_) => {
            if FsPath::new(path).extension().is_none() {
                match tokio::fs::read(st.ui.join("index.html")).await {
                    Ok(b) => b,
                    Err(_) => return (StatusCode::NOT_FOUND, "Not Found").into_response(),
                }
            } else {
                return (StatusCode::NOT_FOUND, "Not Found").into_response();
            }
        }
    };
    let mime = mime_guess::from_path(&fp)
        .first_or_octet_stream()
        .to_string();
    // Every UI response carries the Studio session cookie: the fallback serves
    // index.html for extensionless paths too, and a panel that answers 401
    // because the user opened /sites instead of / is a bug, not a security win.
    let cookie = format!(
        "studio_sid={}; HttpOnly; SameSite=Strict; Path=/",
        st.studio.sid
    );
    (
        [(header::CONTENT_TYPE, mime), (header::SET_COOKIE, cookie)],
        body,
    )
        .into_response()
}

/// Run the dashboard listener.
pub async fn serve(port: u16, state: DashState) -> Result<()> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    eprintln!("\n  SiteBox Dashboard (rust)  →  http://localhost:{port}\n");
    axum::serve(listener, app).await?;
    Ok(())
}
