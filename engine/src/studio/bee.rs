//! Bees: short-lived worker agents that Sols sends out for the parts of a job
//! that can be done in parallel.
//!
//! A bee is deliberately less capable than Sols. It has no `propose_patch`, no
//! lifecycle tools and no memory of the conversation: it reads, searches and
//! reports. Everything it learns comes back as text in the tool result, and
//! Sols stays the only thing that edits the site. That keeps one writer on the
//! files, so a swarm cannot race itself.

use anyhow::Result;
use futures_util::future::join_all;
use serde_json::{json, Value};

use super::{agent, config::Settings, emit, tools, SiteRef, StudioState};

/// Enough for one line of inquiry, not enough to tour a whole site.
const MAX_ROUNDS: usize = 8;
/// A bee that spends more calls than this is touring, not working.
const MAX_CALLS: usize = 14;
const QUICK_ROUNDS: usize = 4;
const QUICK_CALLS: usize = 8;
/// A swarm that needs more than four workers needs a better plan.
pub const MAX_TASKS: usize = 4;
const MAX_REPORT: usize = 10_000;

/// What a bee is allowed to touch. Anything that writes is absent on purpose:
/// the compiler cannot catch a bee that edits, so the tool list is the fence.
const ALLOWED: &[&str] = &[
    "sitebox_read",
    "sitebox_ls",
    "sitebox_grep",
    "sitebox_outline",
    "sitebox_check",
    "sitebox_http",
    "sols_search",
    "sols_fetch",
    "sols_skills",
    "sols_skill",
];

/// Before a site exists a bee can still research: web and doctrine only.
const ALLOWED_PRE_SITE: &[&str] = &["sols_search", "sols_fetch", "sols_skills", "sols_skill"];

/// One line on the shared scratchpad. Bees of the same wave can read each
/// other's notes; that is the whole extent of bee-to-bee communication.
#[derive(Clone, serde::Serialize)]
pub struct Note {
    pub bee: usize,
    pub title: String,
    pub text: String,
}

pub const MAX_NOTES: usize = 60;
const MAX_NOTE_CHARS: usize = 1200;

pub async fn add_note(
    st: &StudioState,
    thread: &str,
    bee: usize,
    title: &str,
    text: &str,
) -> usize {
    let text: String = text.trim().chars().take(MAX_NOTE_CHARS).collect();
    if text.is_empty() {
        return 0;
    }
    let mut board = st.board.lock().await;
    let list = board.entry(thread.to_string()).or_default();
    if list.len() < MAX_NOTES {
        list.push(Note {
            bee,
            title: title.to_string(),
            text,
        });
    }
    list.len()
}

pub async fn notes(st: &StudioState, thread: &str) -> Vec<Note> {
    st.board
        .lock()
        .await
        .get(thread)
        .cloned()
        .unwrap_or_default()
}

#[derive(Clone)]
pub struct Brief {
    pub title: String,
    pub brief: String,
    /// "quick" is for a mechanical sweep (4 rounds, 8 calls); "deep" for
    /// research or design (8 rounds, 14 calls). Default: deep.
    pub effort: String,
}

pub struct Report {
    pub title: String,
    pub ok: bool,
    pub report: String,
    pub rounds: usize,
    pub calls: usize,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

fn bee_tools(has_site: bool) -> Value {
    let allowed: &[&str] = if has_site { ALLOWED } else { ALLOWED_PRE_SITE };
    let mut list: Vec<Value> = tools::definitions()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            t.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .map(|n| allowed.contains(&n))
                .unwrap_or(false)
        })
        .collect();
    list.push(json!({
        "type": "function",
        "function": {
            "name": "bee_note",
            "description": "Write a finding on the shared scratchpad so the other bees in this wave can use it. Facts with their URL, a decision, or a gap. Not a chat message.",
            "parameters": {
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "One compact finding (max ~1200 characters)."}
                },
                "required": ["text"]
            }
        }
    }));
    list.push(json!({
        "type": "function",
        "function": {
            "name": "bee_notes",
            "description": "Read the shared scratchpad: what the other bees have written so far in this wave.",
            "parameters": {"type": "object", "properties": {}}
        }
    }));
    list.push(json!({
        "type": "function",
        "function": {
            "name": "bee_report",
            "description": "Finish your assignment. Call this once you have the deliverable the brief asked for.",
            "parameters": {
                "type": "object",
                "properties": {
                    "report": {"type": "string", "description": "The deliverable itself, in the shape the brief asked for. Facts must carry their URLs."}
                },
                "required": ["report"]
            }
        }
    }));
    Value::Array(list)
}

fn prompt(site: Option<&SiteRef>, title: &str, brief: &str, budget: usize) -> String {
    format!(
        r#"You are Bee, a worker sent by Sols, the editing agent inside SiteBox. You have one assignment. Nobody is reading along: your only output is the report you return.

## Assignment
Title: {title}
{brief}

## The board
You share a scratchpad with the other bees sent in the same wave.
- `bee_notes` reads what they have written so far. Check it once before you start and once before you report: another bee may already have the fact you need.
- `bee_note` writes one compact finding (a fact with its URL, a decision, a gap). It is not a conversation: nobody answers it, and the operator never sees it.
- Two bees writing at the same moment may not see each other's note. Your report is what Sols reads; the board is only to avoid duplicate work.

## What you can do
You can read and search, not write. `sitebox_grep` finds text, `sitebox_read` opens a region, `sitebox_outline` shows the file shape, `sols_search` and `sols_fetch` reach the web, `sols_skill` reads SiteBox doctrine. When the deliverable is ready, call `bee_report` with it.

## How to work
- Before you design or write copy, read `sitebox-playbook` with `sols_skill` (operator taste, copy rules, recurring layout defects). Read `sitebox-design` only if the assignment is specifically about layout.
- Work the assignment, not the whole site. At most three reads before you either answer or say what is missing. Report what you have when the budget is spent: you have {budget} tool calls.
- A mechanical sweep is one call: one `sitebox_grep` covers every file. Opening files one by one to count something is how a bee runs out of rounds.
- Facts need their source URL. Never invent a number, a name or a quote; if the web gives you nothing, say so in one line.
- The report is for Sols, not for the operator: dense, structured, no preamble, no "here is what I found".
- Reply in the language of the brief and only that language. A Chinese or Japanese character inside a Thai sentence is a bug.
{where}"#,
        budget = budget,
        where = match site {
            Some(s) => format!(
                "- Write for the site `{}` at port {}.\n\n## The site as it stands\nkind: {}, serve: {}\n{}",
                s.id,
                s.manifest.site.port,
                s.kind,
                s.manifest.site.serve,
                s.manifest
                    .pipeline
                    .iter()
                    .map(|st| format!("- {}", st.name))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            None => "- No site exists yet: this is research before the build. You cannot read a site folder."
                .to_string(),
        },
    )
}

struct BeeHost<'a> {
    st: &'a StudioState,
    site: Option<&'a SiteRef>,
    parent: &'a str,
    brief: &'a Brief,
    id: usize,
    max_rounds: usize,
    report: Option<String>,
}

impl agent::Host for BeeHost<'_> {
    fn tools(&self) -> Value {
        bee_tools(self.site.is_some())
    }

    async fn dispatch(&mut self, _call_id: &str, name: &str, args: Value) -> Result<Value> {
        if name == "bee_note" {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let n = add_note(self.st, self.parent, self.id, &self.brief.title, text).await;
            return Ok(json!({"ok": true, "noted": n}));
        }
        if name == "bee_notes" {
            let list = notes(self.st, self.parent).await;
            let body = if list.is_empty() {
                "the board is empty".to_string()
            } else {
                list.iter()
                    .map(|n| format!("[bee {} · {}] {}", n.bee, n.title, n.text))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            return Ok(json!({"ok": true, "notes": body}));
        }
        if name == "bee_report" {
            let text = args
                .get("report")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            self.report = Some(text.clone());
            return Ok(json!({"ok": true, "accepted": !text.is_empty()}));
        }
        // A bee's toolbox is a leaf set: it never crosses the writer
        // dispatcher, so it cannot patch, build or spawn another bee.
        if tools::is_read_only(name) {
            return match self.site {
                Some(site) => tools::read_only_site(self.st, site, "__bee", name, &args).await,
                None => Ok(json!({"ok": false, "error": "no site is attached to this conversation"})),
            };
        }
        match name {
            "sols_search" => tools::web_search(self.st, &args).await,
            "sols_fetch" => tools::web_fetch(self.st, &args).await,
            "sols_youtube" => tools::youtube(&args).await,
            "sols_steam" => tools::steam(&args).await,
            "sols_design_index" => Ok(super::design::list(self.st)),
            "sols_skills" => tools::tool_skills(),
            "sols_skill" => tools::tool_skill(&args),
            other => Ok(json!({
                "ok": false,
                "error": format!("`{other}` is not available to a bee")
            })),
        }
    }

    async fn before_round(&mut self, round: usize, messages: &mut Vec<Value>) -> Result<()> {
        if round + 1 == self.max_rounds {
            messages.push(json!({
                "role": "user",
                "content": "This is your last round. Call bee_report now with what you have; a partial report beats no report."
            }));
        }
        Ok(())
    }
}

async fn one(
    st: &StudioState,
    site: Option<&SiteRef>,
    parent: &str,
    brief: &Brief,
    id: usize,
) -> Report {
    let mut out = Report {
        title: brief.title.clone(),
        ok: false,
        report: String::new(),
        rounds: 0,
        calls: 0,
        tokens_in: 0,
        tokens_out: 0,
    };
    let quick = brief.effort == "quick";
    let max_rounds = if quick { QUICK_ROUNDS } else { MAX_ROUNDS };
    let max_calls = if quick { QUICK_CALLS } else { MAX_CALLS };
    let settings = Settings::load(&st.root);
    if !settings.configured() {
        out.report = "the agent is not configured".into();
        return out;
    }
    let messages = vec![
        json!({"role": "system", "content": prompt(
            site,
            &brief.title,
            &brief.brief,
            max_calls,
        )}),
        json!({"role": "user", "content": brief.brief}),
    ];

    emit(
        &st.events,
        parent,
        json!({"type":"bee","id":id,"title":brief.title,"status":"start"}),
    );

    let mut host = BeeHost {
        st,
        site,
        parent,
        brief,
        id,
        max_rounds,
        report: None,
    };
    let cfg = agent::Config {
        st,
        settings: &settings,
        thread_id: "__bee",
        strip_cjk: true,
        limits: agent::Limits {
            rounds: max_rounds,
            calls: max_calls,
        },
    };
    let outcome = agent::run(cfg, messages, &mut host).await;
    let outcome = match outcome {
        Ok(v) => v,
        Err(e) => {
            out.report = format!("bee failed: {e:#}");
            emit(
                &st.events,
                parent,
                json!({"type":"bee","id":id,"title":brief.title,"status":"error"}),
            );
            return out;
        }
    };
    out.rounds = outcome.rounds;
    out.calls = outcome.calls;
    out.tokens_in = outcome.tokens_in;
    out.tokens_out = outcome.tokens_out;

    out.report = host.report.clone().unwrap_or_else(|| {
        if outcome.text.trim().is_empty() {
            "the bee ran out of rounds without a report".into()
        } else {
            format!("(no bee_report; last message)\n{}", outcome.text.trim())
        }
    });
    out.ok = !out.report.trim().is_empty();
    out.report = out.report.chars().take(MAX_REPORT).collect();
    emit(
        &st.events,
        parent,
        json!({"type":"bee","id":id,"title":brief.title,"status":if out.ok {"done"} else {"error"}}),
    );
    out
}

/// Run every brief at once and wait for all of them. A bee that fails returns
/// its error in place, so one dead worker does not cancel the swarm.
pub async fn run(
    st: &StudioState,
    site: Option<&SiteRef>,
    parent: &str,
    briefs: Vec<Brief>,
) -> Vec<Report> {
    let mut briefs = briefs;
    briefs.truncate(MAX_TASKS);
    let futures = briefs
        .iter()
        .enumerate()
        .map(|(i, b)| one(st, site, parent, b, i + 1));
    join_all(futures).await
}

/// The tool result Sols sees: the reports, not the bees' transcripts.
pub fn as_json(reports: &[Report]) -> Value {
    let items: Vec<Value> = reports
        .iter()
        .map(|r| {
            json!({
                "title": r.title,
                "ok": r.ok,
                "report": r.report,
                "rounds": r.rounds,
                "tool_calls": r.calls,
            })
        })
        .collect();
    json!({
        "ok": reports.iter().any(|r| r.ok),
        "bees": items,
        "note": "These are worker reports, not edits. Decide what to use and make the changes yourself with sitebox_propose_patch."
    })
}
