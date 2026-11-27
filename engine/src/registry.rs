//! Compatibility with the existing `dashboard/data/sites.json` registry.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Site {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default, rename = "iconColor")]
    pub icon_color: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub created: String,
    /// Set by auto-detect, never written by the dashboard originally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _auto: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SiteConfig {
    #[serde(default)]
    pub dashboard: DashboardMeta,
    #[serde(default)]
    pub sites: Vec<Site>,
}

pub const REGISTRY_REL: &str = "dashboard/data/sites.json";

impl SiteConfig {
    pub fn read(root: &Path) -> Result<Self> {
        let p = root.join(REGISTRY_REL);
        let raw = std::fs::read_to_string(&p)
            .with_context(|| format!("read registry {}", p.display()))?;
        let cfg: SiteConfig = serde_json::from_str(&raw)
            .with_context(|| format!("parse registry {}", p.display()))?;
        Ok(cfg)
    }

    pub fn write(&self, root: &Path) -> Result<()> {
        let p = root.join(REGISTRY_REL);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)? + "\n")?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    pub fn find(&self, id: &str) -> Option<&Site> {
        self.sites.iter().find(|s| s.id == id)
    }

    /// Mirror the original auto-detect: any folder under `sites/` with a
    /// server.js that is not registered yet gets an entry on a free port.
    pub fn auto_detect(&mut self, root: &Path) -> Result<usize> {
        let dir = root.join("sites");
        if !dir.is_dir() {
            return Ok(0);
        }
        let mut used: Vec<u16> = self.sites.iter().map(|s| s.port).collect();
        let mut added = 0;
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            if self.sites.iter().any(|s| s.id == id) {
                continue;
            }
            let site_dir = entry.path();
            // A manifest is the source of truth: its port and name win. A legacy
            // folder with only server.js is still detected, on the next free port.
            let manifest = crate::manifest::Manifest::load(&site_dir).ok();
            if manifest.is_none() && !site_dir.join("server.js").is_file() {
                continue;
            }
            let (name, description, port, category, icon, icon_color) = match &manifest {
                Some(m) => (
                    m.site.name.clone(),
                    m.site.description.clone(),
                    m.site.port,
                    m.site.category.clone(),
                    m.site.icon.clone(),
                    m.site.icon_color.clone(),
                ),
                None => {
                    let name = id
                        .split('-')
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                                None => String::new(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    let mut port = 4800u16;
                    while used.contains(&port) && port < 65535 {
                        port += 1;
                    }
                    (
                        name,
                        "Auto-detected".to_string(),
                        port,
                        "detected".to_string(),
                        "globe".to_string(),
                        "#8b949e".to_string(),
                    )
                }
            };
            // Never silently move a manifest site to another port: a conflict is
            // a configuration error the operator must see, not something to hide.
            if used.contains(&port) {
                continue;
            }
            if port == 65535 {
                break;
            }
            used.push(port);
            self.sites.push(Site {
                id: id.clone(),
                name,
                description,
                icon,
                icon_color,
                port,
                category,
                url: format!("http://localhost:{port}"),
                path: format!("sites/{id}"),
                created: chrono::Local::now().format("%Y-%m-%d").to_string(),
                _auto: Some(true),
            });
            added += 1;
        }
        Ok(added)
    }

    pub fn validate(&self) -> Result<()> {
        for s in &self.sites {
            validate_id(&s.id)?;
            if s.name.trim().is_empty() {
                bail!("site {}: name required", s.id);
            }
            if s.port < 1024 {
                bail!("site {}: port {} is below 1024", s.id, s.port);
            }
            if self
                .sites
                .iter()
                .filter(|o| o.id != s.id && o.port == s.port)
                .count()
                > 0
            {
                bail!("site {}: port {} is already assigned", s.id, s.port);
            }
        }
        Ok(())
    }
}

pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || id.starts_with('-')
        || id.ends_with('-')
        || id.contains("--")
    {
        bail!("id must be kebab-case — lowercase letters, numbers, single hyphens (e.g. my-site)");
    }
    Ok(())
}

/// All manifests under `root/sites/*/sitebox.toml`.
pub fn manifests(root: &Path) -> Result<Vec<PathBuf>> {
    let dir = root.join("sites");
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path().join(crate::manifest::MANIFEST_FILE);
        if p.is_file() {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_rules() {
        assert!(validate_id("folio").is_ok());
        assert!(validate_id("eow-audio").is_ok());
        assert!(validate_id("Folio").is_err());
        assert!(validate_id("a--b").is_err());
        assert!(validate_id("-a").is_err());
    }

    #[test]
    fn auto_detect_reads_the_manifest() {
        let root = std::env::temp_dir().join(format!("sbx-registry-{}", std::process::id()));
        let site = root.join("sites/demo");
        std::fs::create_dir_all(site.join("public")).unwrap();
        std::fs::write(
            site.join("sitebox.toml"),
            r#"schema = 1
[site]
id = "demo"
name = "Demo Site"
description = "from manifest"
port = 4599
path = "sites/demo"
"#,
        )
        .unwrap();
        let mut cfg = SiteConfig::default();
        let added = cfg.auto_detect(&root).unwrap();
        assert_eq!(added, 1);
        let s = &cfg.sites[0];
        assert_eq!(s.id, "demo");
        assert_eq!(s.name, "Demo Site");
        assert_eq!(s.port, 4599);
        assert_eq!(s.description, "from manifest");
        let _ = std::fs::remove_dir_all(&root);
    }
}
