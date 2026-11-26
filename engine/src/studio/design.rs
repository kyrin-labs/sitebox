//! A small registry of design decisions, so the next site does not become a
//! copy of the last one.
//!
//! The playbook and the page scaffold make builds fast; speed without a memory
//! converges on one look. Every site records the five axes it chose, and the
//! agent reads the list before it chooses. This is advice, not a gate: a brief
//! can deliberately repeat a look, but it has to do so on purpose.

use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;

use super::StudioState;

const AXES: &[&str] = &["archetype", "palette", "type_pair", "layout", "signature"];

fn path(st: &StudioState) -> PathBuf {
    st.data.join("design.json")
}

pub fn load(st: &StudioState) -> Value {
    std::fs::read_to_string(path(st))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({"sites": {}}))
}

pub fn list(st: &StudioState) -> Value {
    let doc = load(st);
    let sites = doc.get("sites").cloned().unwrap_or_else(|| json!({}));
    let mut rows: Vec<Value> = sites
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(id, v)| {
                    let mut row = v.clone();
                    if let Some(o) = row.as_object_mut() {
                        o.insert("site".into(), json!(id));
                    }
                    row
                })
                .collect()
        })
        .unwrap_or_default();
    rows.sort_by(|a, b| {
        a.get("updated")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .cmp(b.get("updated").and_then(|v| v.as_str()).unwrap_or(""))
    });
    rows.reverse();
    json!({
        "ok": true,
        "count": rows.len(),
        "recent": rows.iter().take(8).collect::<Vec<_>>(),
        "axes": AXES,
        "note": "Choose a combination no recent site used. Repeating a look is allowed only when the brief asks for it."
    })
}

pub fn record(st: &StudioState, args: &Value) -> Result<Value> {
    let site = args
        .get("site")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if site.is_empty() {
        anyhow::bail!("site is required (the site id this design belongs to)");
    }
    let mut entry = serde_json::Map::new();
    for axis in AXES {
        let v = args
            .get(*axis)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if v.is_empty() {
            anyhow::bail!("`{axis}` is required; pick one before building");
        }
        entry.insert((*axis).to_string(), json!(v));
    }
    entry.insert("updated".into(), json!(crate::util::now_iso()));

    let mut doc = load(st);
    let combo = |v: &Value| -> (String, String, String, String) {
        (
            v.get("archetype").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            v.get("palette").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            v.get("type_pair").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            v.get("layout").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        )
    };
    let mine = combo(&Value::Object(entry.clone()));
    let mut same = Vec::new();
    if let Some(sites) = doc.get("sites").and_then(|v| v.as_object()) {
        for (id, v) in sites {
            if id != &site && combo(v) == mine {
                same.push(id.clone());
            }
        }
    }
    if let Some(sites) = doc
        .as_object_mut()
        .map(|o| o.entry("sites").or_insert_with(|| json!({})))
        .and_then(|v| v.as_object_mut())
    {
        sites.insert(site.clone(), Value::Object(entry.clone()));
    }
    super::atomic_write(&path(st), serde_json::to_string_pretty(&doc)?.as_bytes())?;
    Ok(json!({
        "ok": true,
        "site": site,
        "recorded": entry,
        "same_combination": same,
        "note": if same.is_empty() {
            "This combination is new among the recorded sites."
        } else {
            "Another site already uses this combination. Vary at least one axis unless the brief asks for a repeat."
        }
    }))
}
