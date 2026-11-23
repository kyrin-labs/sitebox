//! `sitebox.toml` — the per-site contract.
//!
//! Everything the engine needs to build, check, serve and deploy a site is
//! declared here. Prose in the skills explains *why*; this file says *what*.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::registry::Site;

pub const MANIFEST_FILE: &str = "sitebox.toml";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub schema: u32,
    pub site: SiteMeta,
    #[serde(default)]
    pub pipeline: Vec<PipelineStep>,
    #[serde(default)]
    pub checks: Vec<CheckCfg>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SiteMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub port: u16,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub icon_color: String,
    /// Folder relative to the SiteBox root, e.g. `sites/folio`.
    #[serde(default)]
    pub path: String,
    /// Folder served to the browser, relative to `path`.
    #[serde(default = "default_public")]
    pub public_dir: String,
    /// `node` keeps spawning the site's own server.js; `static` is served by
    /// this engine. `proxy` serves static assets and forwards `/api/*` to
    /// `api_upstream`.
    #[serde(default = "default_serve")]
    pub serve: String,
    /// Only for `serve = "proxy"`, e.g. `http://127.0.0.1:4510`.
    #[serde(default)]
    pub api_upstream: Option<String>,
    #[serde(default)]
    pub created: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineStep {
    pub name: String,
    /// Built-in stage: `images`. Omit for a `run` step.
    #[serde(default)]
    pub kind: Option<String>,
    /// Shell command, executed with cwd = site folder.
    #[serde(default)]
    pub run: Option<String>,
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub rules: Option<toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckCfg {
    pub id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub args: Option<toml::Value>,
}

fn yes() -> bool {
    true
}
fn default_category() -> String {
    "tools".into()
}
fn default_public() -> String {
    "public".into()
}
fn default_serve() -> String {
    "static".into()
}

impl Manifest {
    pub fn load(site_dir: &Path) -> Result<Self> {
        let p = site_dir.join(MANIFEST_FILE);
        let raw = std::fs::read_to_string(&p)
            .with_context(|| format!("read manifest {}", p.display()))?;
        let m: Manifest =
            toml::from_str(&raw).with_context(|| format!("parse manifest {}", p.display()))?;
        if m.schema != SCHEMA_VERSION {
            anyhow::bail!(
                "{}: schema {} is not supported (engine expects {})",
                p.display(),
                m.schema,
                SCHEMA_VERSION
            );
        }
        Ok(m)
    }

    pub fn save(&self, site_dir: &Path) -> Result<PathBuf> {
        let p = site_dir.join(MANIFEST_FILE);
        let body = toml::to_string_pretty(self)?;
        let tmp = p.with_extension("toml.tmp");
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &p)?;
        Ok(p)
    }

    pub fn site_dir(&self, root: &Path) -> PathBuf {
        root.join(&self.site.path)
    }

    pub fn public_dir(&self, root: &Path) -> PathBuf {
        self.site_dir(root).join(&self.site.public_dir)
    }

    pub fn from_site(site: &Site) -> Self {
        let serve = if site.path.is_empty() {
            "static"
        } else {
            "node"
        };
        Manifest {
            schema: SCHEMA_VERSION,
            site: SiteMeta {
                id: site.id.clone(),
                name: site.name.clone(),
                description: site.description.clone(),
                port: site.port,
                category: site.category.clone(),
                icon: site.icon.clone(),
                icon_color: site.icon_color.clone(),
                path: site.path.clone(),
                public_dir: default_public(),
                serve: serve.into(),
                api_upstream: None,
                created: site.created.clone(),
            },
            pipeline: Vec::new(),
            checks: Vec::new(),
        }
    }

    /// JSON Schema for the manifest — the agent-facing contract. Hand-written
    /// so it stays stable and dependency-free.
    pub fn json_schema() -> serde_json::Value {
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "sitebox.toml",
            "type": "object",
            "required": ["schema", "site"],
            "properties": {
                "schema": { "const": SCHEMA_VERSION },
                "site": {
                    "type": "object",
                    "required": ["id", "name", "port", "path"],
                    "properties": {
                        "id": { "type": "string", "pattern": "^[a-z0-9]+(?:-[a-z0-9]+)*$" },
                        "name": { "type": "string", "minLength": 1 },
                        "description": { "type": "string" },
                        "port": { "type": "integer", "minimum": 1024, "maximum": 65535 },
                        "category": { "type": "string" },
                        "icon": { "type": "string" },
                        "icon_color": { "type": "string" },
                        "path": { "type": "string" },
                        "public_dir": { "type": "string", "default": "public" },
                        "serve": { "enum": ["static", "proxy", "node"], "default": "static" },
                        "api_upstream": { "type": ["string", "null"] },
                        "created": { "type": "string" }
                    }
                },
                "pipeline": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["name"],
                        "properties": {
                            "name": { "type": "string" },
                            "kind": { "enum": ["images", null] },
                            "run": { "type": ["string", "null"] },
                            "inputs": { "type": "array", "items": { "type": "string" } },
                            "outputs": { "type": "array", "items": { "type": "string" } },
                            "rules": { "type": ["object", "null"] }
                        }
                    }
                },
                "checks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["id"],
                        "properties": {
                            "id": { "type": "string" },
                            "enabled": { "type": "boolean", "default": true },
                            "args": { "type": ["object", "null"] }
                        }
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_manifest() {
        let m = Manifest {
            schema: SCHEMA_VERSION,
            site: SiteMeta {
                id: "demo".into(),
                name: "Demo".into(),
                description: "d".into(),
                port: 4500,
                category: "tools".into(),
                icon: "globe".into(),
                icon_color: "#fff".into(),
                path: "sites/demo".into(),
                public_dir: "public".into(),
                serve: "static".into(),
                api_upstream: None,
                created: "2026-01-01".into(),
            },
            pipeline: vec![PipelineStep {
                name: "assets".into(),
                kind: Some("images".into()),
                run: None,
                inputs: vec!["synd/data.json".into()],
                outputs: vec!["public/img".into()],
                rules: None,
            }],
            checks: vec![CheckCfg {
                id: "SB010".into(),
                enabled: true,
                args: None,
            }],
        };
        let dir = std::env::temp_dir().join(format!("sbx-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        m.save(&dir).unwrap();
        let back = Manifest::load(&dir).unwrap();
        assert_eq!(m, back);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
