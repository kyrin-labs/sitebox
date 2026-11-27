//! The SiteBox skills, embedded at build time.
//!
//! Sols does not get the whole doctrine in its prompt — that would be a hundred
//! kilobytes on every turn. It gets the index and one tool to read the skill the
//! task belongs to, which is how the terminal agent works too.

use anyhow::{bail, Result};
use serde::Serialize;

include!(concat!(env!("OUT_DIR"), "/skills_embedded.rs"));

#[derive(Debug, Clone, Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub files: Vec<String>,
}

fn description_of(text: &str) -> String {
    for line in text.lines().take(30) {
        if let Some(rest) = line.strip_prefix("description:") {
            let d = rest.trim().trim_matches('"').trim();
            if !d.is_empty() {
                return d.to_string();
            }
        }
    }
    String::new()
}

/// One entry per skill directory, ordered by name.
pub fn list() -> Vec<SkillInfo> {
    let mut out: Vec<SkillInfo> = Vec::new();
    for (skill, rel, text) in SKILL_FILES {
        if *rel != "SKILL.md" {
            continue;
        }
        out.push(SkillInfo {
            name: (*skill).to_string(),
            description: description_of(text),
            files: SKILL_FILES
                .iter()
                .filter(|(s, _, _)| s == skill)
                .map(|(_, r, _)| (*r).to_string())
                .collect(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Read one markdown file of one skill. Defaults to `SKILL.md`.
pub fn read(name: &str, file: Option<&str>) -> Result<(String, String)> {
    let rel = file.unwrap_or("SKILL.md");
    if rel.contains("..") || rel.starts_with('/') {
        bail!("invalid skill file path");
    }
    for (skill, path, text) in SKILL_FILES {
        if *skill == name && *path == rel {
            return Ok(((*path).to_string(), (*text).to_string()));
        }
    }
    let known: Vec<&str> = SKILL_FILES
        .iter()
        .filter(|(s, _, _)| *s == name)
        .map(|(_, p, _)| *p)
        .collect();
    if known.is_empty() {
        bail!("no skill named `{name}`; call sols_skills for the list");
    }
    bail!(
        "`{rel}` is not part of {name}; available: {}",
        known.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_skills_have_descriptions() {
        let all = list();
        assert!(
            all.len() >= 5,
            "expected the SiteBox skills, got {}",
            all.len()
        );
        let design = all
            .iter()
            .find(|s| s.name == "sitebox-design")
            .expect("sitebox-design is bundled");
        assert!(!design.description.is_empty());
        assert!(design.files.iter().any(|f| f == "SKILL.md"));
    }

    #[test]
    fn reading_a_skill_returns_its_body() {
        let (path, text) = read("sitebox-design", None).unwrap();
        assert_eq!(path, "SKILL.md");
        assert!(text.contains("#"));
        assert!(read("sitebox-design", Some("../etc/passwd")).is_err());
        assert!(read("nope", None).is_err());
    }
}
