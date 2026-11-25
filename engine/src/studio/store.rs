//! Threads, messages and the patch queue.
//!
//! Plain files, not a database: one thread is one folder, one message is one
//! line. A half-written line can only ever be the last one, and tail-reading a
//! transcript for debugging is `cat`, not a query tool.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use super::{new_id, StudioState};
use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub text: String,
    #[serde(default = "pending")]
    pub status: String,
}

fn pending() -> String {
    "pending".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Thread {
    pub id: String,
    #[serde(default)]
    pub site_id: String,
    #[serde(default)]
    pub title: String,
    /// Last page the operator was looking at when talking to this thread.
    #[serde(default)]
    pub page: Option<String>,
    /// The visible plan for the current piece of work.
    #[serde(default)]
    pub plan: Vec<PlanStep>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub ts: String,
}

impl Message {
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            ts: util::now_iso(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatchRec {
    pub id: String,
    pub site_id: String,
    #[serde(default)]
    pub thread_id: String,
    pub path: String,
    pub old: String,
    pub new: String,
    #[serde(default = "one")]
    pub count: usize,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub diff: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub applied: Option<String>,
    #[serde(default)]
    pub verify: Value,
}

fn one() -> usize {
    1
}

fn threads_dir(st: &StudioState) -> PathBuf {
    st.data.join("threads")
}

fn thread_dir(st: &StudioState, id: &str) -> PathBuf {
    threads_dir(st).join(id)
}

pub fn create(st: &StudioState, site_id: &str, title: &str) -> Result<Thread> {
    // An empty site id is a conversation that will create its site; the model
    // binds it with sols_create_site before any site tool works.
    let now = util::now_iso();
    let t = Thread {
        id: new_id("th"),
        site_id: site_id.to_string(),
        title: title.trim().to_string(),
        page: None,
        plan: Vec::new(),
        created: now.clone(),
        updated: now,
    };
    let d = thread_dir(st, &t.id);
    super::ensure_dir(&d)?;
    super::atomic_write(
        &d.join("meta.json"),
        serde_json::to_string_pretty(&t)?.as_bytes(),
    )?;
    super::atomic_write(&d.join("messages.jsonl"), b"")?;
    Ok(t)
}

pub fn get(st: &StudioState, id: &str) -> Result<Thread> {
    let p = thread_dir(st, id).join("meta.json");
    let raw = std::fs::read_to_string(&p).with_context(|| format!("read thread {id}"))?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn set_title(st: &StudioState, id: &str, title: &str) -> Result<()> {
    let mut t = get(st, id)?;
    t.title = title.chars().take(80).collect();
    t.updated = util::now_iso();
    let p = thread_dir(st, id).join("meta.json");
    super::atomic_write(&p, serde_json::to_string_pretty(&t)?.as_bytes())
}

/// Remember which page the operator is on. The agent reads this before it
/// starts guessing which of a site's pages contains the text they quoted.
pub fn set_page(st: &StudioState, id: &str, page: &str) -> Result<()> {
    let mut t = get(st, id)?;
    let page = page.trim();
    if page.is_empty() {
        return Ok(());
    }
    t.page = Some(page.chars().take(300).collect());
    t.updated = util::now_iso();
    let p = thread_dir(st, id).join("meta.json");
    super::atomic_write(&p, serde_json::to_string_pretty(&t)?.as_bytes())
}

/// Rebind a conversation to the site it just created.
pub fn set_site(st: &StudioState, id: &str, site_id: &str) -> Result<()> {
    let mut t = get(st, id)?;
    t.site_id = site_id.to_string();
    t.updated = util::now_iso();
    let p = thread_dir(st, id).join("meta.json");
    super::atomic_write(&p, serde_json::to_string_pretty(&t)?.as_bytes())
}

/// Replace the visible plan. Statuses are the model's own: pending, doing, done.
pub fn set_plan(st: &StudioState, id: &str, steps: Vec<PlanStep>) -> Result<()> {
    let mut t = get(st, id)?;
    t.plan = steps.into_iter().take(8).collect();
    t.updated = util::now_iso();
    let p = thread_dir(st, id).join("meta.json");
    super::atomic_write(&p, serde_json::to_string_pretty(&t)?.as_bytes())
}

pub fn list(st: &StudioState, site: Option<&str>) -> Vec<Thread> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(threads_dir(st)) else {
        return out;
    };
    for e in rd.flatten() {
        let meta = e.path().join("meta.json");
        let Ok(raw) = std::fs::read_to_string(&meta) else {
            continue;
        };
        let Ok(t) = serde_json::from_str::<Thread>(&raw) else {
            continue;
        };
        if let Some(s) = site {
            if t.site_id != s {
                continue;
            }
        }
        out.push(t);
    }
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    out
}

pub fn append_msg(st: &StudioState, thread_id: &str, msg: &Message) -> Result<()> {
    let mut t = get(st, thread_id)?;
    let line = serde_json::to_string(msg)?;
    let p = thread_dir(st, thread_id).join("messages.jsonl");
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    t.updated = util::now_iso();
    let meta = thread_dir(st, thread_id).join("meta.json");
    super::atomic_write(&meta, serde_json::to_string_pretty(&t)?.as_bytes())
}

pub fn messages(st: &StudioState, thread_id: &str) -> Result<Vec<Message>> {
    let p = thread_dir(st, thread_id).join("messages.jsonl");
    let raw = std::fs::read_to_string(&p).unwrap_or_default();
    Ok(raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Message>(l).ok())
        .collect())
}

// ── patches ─────────────────────────────────────────────────────────────────

fn patch_path(st: &StudioState, id: &str) -> PathBuf {
    st.data.join("patches").join(format!("{id}.json"))
}

pub fn put_patch(st: &StudioState, p: &PatchRec) -> Result<()> {
    super::atomic_write(
        &patch_path(st, &p.id),
        serde_json::to_string_pretty(p)?.as_bytes(),
    )
}

pub fn get_patch(st: &StudioState, id: &str) -> Result<PatchRec> {
    let raw = std::fs::read_to_string(patch_path(st, id))
        .with_context(|| format!("patch {id} not found"))?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn list_patches(st: &StudioState, site: &str, limit: usize) -> Vec<PatchRec> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(st.data.join("patches")) else {
        return out;
    };
    for e in rd.flatten() {
        if e.path().extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let Ok(p) = serde_json::from_str::<PatchRec>(&raw) else {
            continue;
        };
        if p.site_id == site {
            out.push(p);
        }
    }
    out.sort_by(|a, b| b.created.cmp(&a.created));
    out.truncate(limit);
    out
}
