//! Studio settings: base URL, model, key.
//!
//! Values come from two places: `AI_*` variables (environment first, then a
//! `.env` at the data root) and `settings.json`. The environment wins, so a
//! deployment configures the agent without touching the data volume. The key is
//! write-only from the browser's point of view: every response goes through
//! [`Settings::public_json`], which never contains it. `settings.json` is `0600`
//! and a key that came from the environment is never copied into it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const ENV_BASE_URL: &str = "AI_BASE_URL";
pub const ENV_MODEL: &str = "AI_MODEL";
pub const ENV_API_KEY: &str = "AI_API_KEY";

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
        let mut s: Self = std::fs::read_to_string(Self::path(root))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        // Deployment configuration beats the file, so a container can be
        // pointed at another gateway without editing the mounted volume.
        if let Some(v) = env_or_file(root, ENV_BASE_URL) {
            s.base_url = v;
        }
        if let Some(v) = env_or_file(root, ENV_MODEL) {
            s.model = v;
        }
        if let Some(v) = env_or_file(root, ENV_API_KEY) {
            s.api_key = Some(v);
        }
        s
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let mut clean = self.clone();
        if env_or_file(root, ENV_API_KEY).is_some() {
            // The secret lives in .env; never duplicate it into settings.json.
            clean.api_key = None;
        }
        let p = Self::path(root);
        super::ensure_dir(p.parent().unwrap_or(Path::new(".")))?;
        super::atomic_write(&p, serde_json::to_string_pretty(&clean)?.as_bytes())?;
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

    /// Which values came from the environment, so the panel can show them as
    /// read-only instead of pretending an edit will stick.
    pub fn from_env(root: &Path) -> Value {
        json!({
            "base_url": env_or_file(root, ENV_BASE_URL).is_some(),
            "model": env_or_file(root, ENV_MODEL).is_some(),
            "has_key": env_or_file(root, ENV_API_KEY).is_some(),
        })
    }

    pub fn public_json(&self, root: &Path) -> Value {
        json!({
            "base_url": self.base_url,
            "model": self.model,
            "has_key": self.key().is_some(),
            "configured": self.configured(),
            "endpoint": if self.base_url.trim().is_empty() { String::new() } else { self.endpoint() },
            "max_rounds": self.max_rounds.unwrap_or(16),
            "extra_headers": self.extra_headers,
            "from_env": Self::from_env(root),
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
            self.max_rounds = Some(v.clamp(1, 24) as u32);
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

/// Environment first, then a `.env` next to the data root. Docker compose
/// passes the same names through `environment:`, so both installs configure
/// search with one setting.
pub fn env_or_file(root: &Path, key: &str) -> Option<String> {
    if let Ok(v) = std::env::var(key) {
        let v = v.trim().trim_matches('"').to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    let text = std::fs::read_to_string(root.join(".env")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sitebox-cfg-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn env_file_supplies_ai_settings() {
        let dir = scratch("load");
        std::fs::write(
            dir.join(".env"),
            "AI_BASE_URL=https://gateway.example/v1\nAI_MODEL=test-model\nAI_API_KEY=sk-from-env-123\n",
        )
        .unwrap();
        let s = Settings::load(&dir);
        assert_eq!(s.base_url, "https://gateway.example/v1");
        assert_eq!(s.model, "test-model");
        assert_eq!(s.key(), Some("sk-from-env-123"));
        assert!(s.configured());

        // Saving must not copy the environment's secret into the data volume.
        s.save(&dir).unwrap();
        let written = std::fs::read_to_string(Settings::path(&dir)).unwrap();
        assert!(!written.contains("sk-from-env-123"), "{written}");
        assert!(written.contains("test-model"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
