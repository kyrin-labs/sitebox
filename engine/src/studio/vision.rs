//! Can the configured model look at a picture?
//!
//! The panel accepts attachments, but a text-only model would either fail the
//! turn halfway or silently ignore the image. Ask once, cache the answer, and
//! refuse the attachment before a turn starts when the answer is no.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::time::Duration;

use super::b64;
use super::config::Settings;
use super::StudioState;

/// A valid 1x1 PNG. Small enough that a probe costs almost nothing.
const PIXEL_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

pub async fn supported(st: &StudioState, settings: &Settings) -> Result<bool> {
    let key = format!("{}|{}", settings.endpoint(), settings.model);
    let cache = st.data.join("vision.json");
    if let Ok(raw) = std::fs::read_to_string(&cache) {
        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            if v.get("key").and_then(|k| k.as_str()) == Some(key.as_str()) {
                if let Some(b) = v.get("vision").and_then(|b| b.as_bool()) {
                    return Ok(b);
                }
            }
        }
    }

    let body = json!({
        "model": settings.model,
        "max_tokens": 8,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "ok"},
                {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{PIXEL_PNG}")}}
            ]
        }]
    });
    let mut req = st
        .client
        .post(settings.endpoint())
        .timeout(Duration::from_secs(45))
        .header("content-type", "application/json")
        .json(&body);
    if let Some(k) = settings.key() {
        req = req.bearer_auth(k);
    }
    for (k, v) in &settings.extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = req.send().await.context("probe vision support")?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let vision = if status.is_success() {
        true
    } else if status == reqwest::StatusCode::BAD_REQUEST
        && (text.to_lowercase().contains("image")
            || text.to_lowercase().contains("vision")
            || text.to_lowercase().contains("modal"))
    {
        false
    } else {
        anyhow::bail!("model endpoint answered {status} to the vision probe: {}", text.chars().take(200).collect::<String>());
    };
    let _ = std::fs::write(
        &cache,
        serde_json::to_string(&json!({"key": key, "vision": vision})).unwrap_or_default(),
    );
    Ok(vision)
}

/// Decode a `data:image/...;base64,...` URL into bytes and an extension.
pub fn data_url(raw: &str) -> Option<(Vec<u8>, &'static str)> {
    let rest = raw.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?;
    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => return None,
    };
    Some((b64::decode(payload)?, ext))
}

pub fn data_url_of(path: &std::path::Path, ext: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mime = match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return None,
    };
    Some(format!("data:{mime};base64,{}", b64::encode(&bytes)))
}
