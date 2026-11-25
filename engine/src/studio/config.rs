//! Studio settings: base URL, model, key.
//!
//! The key is write-only from the browser's point of view: every response goes
//! through [`Settings::public_json`], which never contains it. The file is
//! `0600` so a stray backup or a shared volume does not leak it either.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Tool rounds per turn before the loop stops itself.
    #[serde(default)]
    pub max_rounds: Option<u32>,
    /// Extra request headers for gateways that need them (e.g. HTTP-Referer).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_headers: BTreeMap<String, String>,
}

impl Settings {
    pub fn path(root: &Path) -> PathBuf {
        super::dir(root).join("settings.json")
    }

    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(Self::path(root))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let p = Self::path(root);
        super::ensure_dir(p.parent().unwrap_or(Path::new(".")))?;
        super::atomic_write(&p, serde_json::to_string_pretty(self)?.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("chmod 600 {}", p.display()))?;
        }
        Ok(())
    }

    pub fn configured(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.model.trim().is_empty()
    }

    /// Accepts `https://host`, `https://host/v1`, or a full
    /// `.../chat/completions` URL. Anything else gets `/v1/chat/completions`.
    pub fn endpoint(&self) -> String {
        let base = self.base_url.trim().trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            base.to_string()
        } else if base.ends_with("/v1") || base.ends_with("/v1beta") {
            format!("{base}/chat/completions")
        } else {
            format!("{base}/v1/chat/completions")
        }
    }

    pub fn key(&self) -> Option<&str> {
        self.api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
    }

    pub fn public_json(&self) -> Value {
        json!({
            "base_url": self.base_url,
            "model": self.model,
            "has_key": self.key().is_some(),
            "configured": self.configured(),
            "endpoint": if self.base_url.trim().is_empty() { String::new() } else { self.endpoint() },
            "max_rounds": self.max_rounds.unwrap_or(12),
            "extra_headers": self.extra_headers,
        })
    }

    /// Merge a browser update. `None` keeps the stored value; `Some("")` clears it.
    pub fn merge(&mut self, body: &Value) {
        if let Some(v) = body.get("base_url").and_then(|v| v.as_str()) {
            self.base_url = v.trim().to_string();
        }
        if let Some(v) = body.get("model").and_then(|v| v.as_str()) {
            self.model = v.trim().to_string();
        }
        if let Some(v) = body.get("max_rounds").and_then(|v| v.as_u64()) {
            self.max_rounds = Some(v.clamp(1, 12) as u32);
        }
        // `api_key` is only touched when the field is present at all: an absent
        // field means "the UI did not change it", not "erase the key".
        if let Some(v) = body.get("api_key").and_then(|v| v.as_str()) {
            let v = v.trim();
            self.api_key = if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            };
        }
        if let Some(map) = body.get("extra_headers").and_then(|v| v.as_object()) {
            self.extra_headers = map
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect();
        }
    }
}
