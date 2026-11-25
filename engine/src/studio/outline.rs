//! A site's shape, cheap enough to put in a prompt.
//!
//! The model never gets a 300 KB file. It gets a map: which blocks exist, where
//! they start and end, and how many bytes each one holds. Then it asks for the
//! one region the user pointed at.

use anyhow::{bail, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub kind: String,
    pub name: String,
    pub start: usize,
    pub end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub const MAX_ITEMS: usize = 400;

const SOURCE_EXT: &[&str] = &[
    "html", "htm", "css", "js", "mjs", "cjs", "ts", "tsx", "jsx", "json", "toml", "md", "txt",
    "sh", "py", "svg",
];

pub fn is_source(rel: &str) -> bool {
    let ext = Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    SOURCE_EXT.contains(&ext.to_ascii_lowercase().as_str())
}

/// Source files under the site, largest first, skipping build output folders and
/// anything that is not text we would ever patch.
pub fn source_files(site_dir: &Path, limit: usize) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for e in walkdir::WalkDir::new(site_dir)
        .max_depth(5)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(e.file_type().is_dir()
                && (n == "node_modules"
                    || n == ".git"
                    || n == ".sitebox"
                    || n == ".snapshots"
                    || n == "img"
                    || n == "images"
                    || n == "fonts"
                    || n == "media"))
        })
        .flatten()
    {
        if !e.file_type().is_file() {
            continue;
        }
        let Ok(rel) = e.path().strip_prefix(site_dir) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        if !is_source(&rel) {
            continue;
        }
        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
        if size > super::patch::MAX_FILE_BYTES {
            continue;
        }
        out.push((rel, size));
    }
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out.truncate(limit);
    out
}

pub fn items(site_dir: &Path, rel: &str) -> Result<Vec<Item>> {
    let path = super::site_path(site_dir, rel)?;
    if !path.is_file() {
        bail!("no such file: {rel}");
    }
    let size = path.metadata()?.len();
    if size > super::patch::MAX_FILE_BYTES {
        bail!(
            "{rel} is {size} bytes; outline is limited to files under {}",
            super::patch::MAX_FILE_BYTES
        );
    }
    let text =
        std::fs::read_to_string(&path).map_err(|_| anyhow::anyhow!("{rel} is not UTF-8 text"))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut items = match ext.as_str() {
        "html" | "htm" => html(&text),
        "css" => css(&text),
        "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" => js(&text),
        "json" => json(&text),
        "toml" => toml(&text),
        "md" => md(&text),
        _ => Vec::new(),
    };
    if items.is_empty() {
        let lines = text.lines().count().max(1);
        items.push(Item {
            kind: "file".into(),
            name: rel.into(),
            start: 1,
            end: lines,
            note: None,
        });
    }
    items.truncate(MAX_ITEMS);
    Ok(items)
}

fn attr(line: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let i = line.find(&needle)?;
    let rest = &line[i + needle.len()..];
    Some(rest.split('"').next()?.to_string())
}

fn html(text: &str) -> Vec<Item> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<Item> = Vec::new();
    let mut open: Vec<(String, String, usize, usize)> = Vec::new(); // kind, name, start, line index
    let mut style_start: Option<usize> = None;
    let mut script_start: Option<(usize, String)> = None;

    for (i, raw) in lines.iter().enumerate() {
        let n = i + 1;
        let line = raw.trim();

        if let Some(start) = style_start {
            if line.contains("</style>") {
                out.push(Item {
                    kind: "style".into(),
                    name: "inline <style>".into(),
                    start,
                    end: n,
                    note: Some(format!("{} lines", n - start + 1)),
                });
                style_start = None;
            }
        } else if line.starts_with("<style") {
            style_start = Some(n);
        }

        if let Some((start, _)) = &script_start {
            if line.contains("</script>") {
                out.push(Item {
                    kind: "script".into(),
                    name: "inline <script>".into(),
                    start: *start,
                    end: n,
                    note: Some(format!("{} lines", n - start + 1)),
                });
                script_start = None;
            }
        } else if line.starts_with("<script") {
            if line.contains("src=") {
                let src = attr(line, "src").unwrap_or_default();
                out.push(Item {
                    kind: "script-src".into(),
                    name: src,
                    start: n,
                    end: n,
                    note: None,
                });
            } else {
                script_start = Some((n, String::new()));
            }
        }

        for tag in [
            "section", "header", "main", "footer", "nav", "article", "aside",
        ] {
            let open_tag = format!("<{tag}");
            let close_tag = format!("</{tag}");
            if line.contains(&close_tag) {
                if let Some(pos) = open.iter().rposition(|(k, _, _, _)| k == tag) {
                    let (_, name, start, _) = open.remove(pos);
                    out.push(Item {
                        kind: tag.into(),
                        name,
                        start,
                        end: n,
                        note: None,
                    });
                }
            } else if line.contains(&open_tag) {
                let name = attr(line, "id")
                    .or_else(|| attr(line, "aria-label"))
                    .or_else(|| attr(line, "class"))
                    .unwrap_or_else(|| format!("<{tag}>"));
                open.push((tag.into(), name, n, i));
            }
        }

        if let Some(rest) = line.strip_prefix("<!--") {
            let label = rest
                .trim_start_matches(['=', '-', ' '])
                .trim_end_matches(['=', '-', ' ', '>']);
            if !label.is_empty() && label.len() < 90 {
                out.push(Item {
                    kind: "comment".into(),
                    name: label.to_string(),
                    start: n,
                    end: n,
                    note: None,
                });
            }
        }
    }

    // Unclosed blocks run to the end of the file; saying so is better than lying.
    let last = lines.len();
    for (kind, name, start, _) in open {
        out.push(Item {
            kind,
            name,
            start,
            end: last,
            note: Some("unclosed — runs to EOF".into()),
        });
    }
    if let Some(s) = style_start {
        out.push(Item {
            kind: "style".into(),
            name: "inline <style>".into(),
            start: s,
            end: last,
            note: Some("unclosed".into()),
        });
    }
    if let Some((s, _)) = script_start {
        out.push(Item {
            kind: "script".into(),
            name: "inline <script>".into(),
            start: s,
            end: last,
            note: Some("unclosed".into()),
        });
    }

    out.sort_by_key(|i| i.start);
    out
}

/// Brace matching that ignores strings and line comments just enough to be
/// useful on real CSS/JS.
fn block_end(lines: &[&str], start_idx: usize) -> usize {
    let mut depth = 0i32;
    let mut seen = false;
    for (i, raw) in lines.iter().enumerate().skip(start_idx) {
        let mut in_str: Option<char> = None;
        let mut escaped = false;
        let chars: Vec<char> = raw.chars().collect();
        let mut j = 0;
        while j < chars.len() {
            let c = chars[j];
            if let Some(q) = in_str {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    in_str = None;
                }
            } else if c == '"' || c == '\'' || c == '`' {
                in_str = Some(c);
            } else if c == '/' && j + 1 < chars.len() && chars[j + 1] == '/' {
                break;
            } else if c == '{' {
                depth += 1;
                seen = true;
            } else if c == '}' {
                depth -= 1;
                if seen && depth <= 0 {
                    return i + 1;
                }
            }
            j += 1;
        }
    }
    lines.len()
}

fn css(text: &str) -> Vec<Item> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.ends_with('{') && !line.starts_with('@') || line.starts_with("@media") {
            let end = block_end(&lines, i);
            out.push(Item {
                kind: if line.starts_with('@') {
                    "at-rule".into()
                } else {
                    "rule".into()
                },
                name: line.trim_end_matches('{').trim().to_string(),
                start: i + 1,
                end,
                note: Some(format!("{} lines", end - i)),
            });
            i = end;
            continue;
        }
        i += 1;
    }
    out
}

fn js(text: &str) -> Vec<Item> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_start();
        let is_fn = line.starts_with("function ")
            || line.starts_with("async function ")
            || line.starts_with("export function ")
            || line.starts_with("export async function ")
            || line.starts_with("export default function ");
        let is_arrow =
            (line.starts_with("const ") || line.starts_with("let ") || line.starts_with("var "))
                && line.contains('=')
                && (line.contains("=>") || line.contains("= function") || line.contains("= async"));
        if is_fn || is_arrow {
            let name = line
                .split(['(', '='])
                .nth(if is_fn { 1 } else { 0 })
                .map(|s| {
                    s.trim()
                        .trim_start_matches("export ")
                        .trim_start_matches("default ")
                        .trim()
                        .to_string()
                })
                .unwrap_or_else(|| "anonymous".into());
            if name.is_empty() || name.len() > 60 {
                continue;
            }
            let end = if line.contains('{') {
                block_end(&lines, i)
            } else {
                i + 1
            };
            out.push(Item {
                kind: if is_fn {
                    "function".into()
                } else {
                    "const".into()
                },
                name,
                start: i + 1,
                end,
                note: Some(format!("{} lines", end - i)),
            });
        }
    }
    out
}

fn json(text: &str) -> Vec<Item> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        // A top-level key is the only thing worth mapping in a data file: the
        // value can be a thousand records long, and the model addresses those
        // by key, not by line.
        if depth == 1 {
            if let Some(rest) = line.strip_prefix('"') {
                if let Some((name, _)) = rest.split_once("\":") {
                    if !name.is_empty() && name.len() < 80 {
                        out.push(Item {
                            kind: "key".into(),
                            name: name.to_string(),
                            start: i + 1,
                            end: i + 1,
                            note: None,
                        });
                    }
                }
            }
        }
        for c in line.chars() {
            match c {
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                _ => {}
            }
        }
    }
    out
}

fn toml(text: &str) -> Vec<Item> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if (line.starts_with('[') && line.ends_with(']'))
            || (line.contains('=') && !line.starts_with('#'))
        {
            out.push(Item {
                kind: if line.starts_with('[') {
                    "table".into()
                } else {
                    "key".into()
                },
                name: line
                    .trim_matches(['[', ']'])
                    .split('=')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                start: i + 1,
                end: i + 1,
                note: None,
            });
        }
    }
    out
}

fn md(text: &str) -> Vec<Item> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        if let Some(h) = raw.strip_prefix('#') {
            let level = raw.len() - raw.trim_start_matches('#').len();
            out.push(Item {
                kind: format!("h{level}"),
                name: h.trim().to_string(),
                start: i + 1,
                end: i + 1,
                note: None,
            });
        }
    }
    out
}
