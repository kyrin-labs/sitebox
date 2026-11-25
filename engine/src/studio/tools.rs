//! The entire capability surface the model has.
//!
//! Note what is absent: no write tool and no shell. The model can look, search
//! the public web, measure, and *propose*. A human applies. That asymmetry is
//! the feature; the one HTTP surface it has is read-only and SSRF-guarded.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::net::{IpAddr, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

use super::patch::PatchSpec;
use super::{config, outline, shot, site_path, skills, stage, store, SiteRef, StudioState};
use crate::checks;
use crate::util;

const MAX_READ_LINES: usize = 400;

pub fn definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "sitebox_grep",
                "description": "Find a literal string across the site's source files in one call. Use this to answer 'where does this text live' or 'how many pages have it' instead of reading files one by one.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "literal text to find (at least 3 characters)"},
                        "glob": {"type": "string", "description": "optional file pattern, e.g. public/*.html"},
                        "ignore_case": {"type": "boolean"},
                        "max": {"type": "integer", "description": "max matches, default 40"}
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_create_site",
                "description": "Create a new SiteBox site (folder, manifest, starter page, registry entry, a free port), start it, and bind this conversation to it. Only available before a site is bound. After it returns, improve the starter with patches.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "description": "slug: lowercase letters, digits and dashes"},
                        "name": {"type": "string"},
                        "description": {"type": "string"},
                        "port": {"type": "integer", "description": "optional; a free port in 4500-4899 is chosen otherwise"}
                    },
                    "required": ["id", "name"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_search",
                "description": "Search the web through the configured SearXNG instance. Use for facts outside the site (companies, places, people, prices, domains, news). Returns titles, URLs and snippets.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "count": {"type": "integer", "description": "1-10, default 5"},
                        "language": {"type": "string", "description": "ISO code such as th or en; omit for auto"}
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_fetch",
                "description": "Fetch one public http(s) page and return its readable text. Refuses private/local addresses. Use after a search when the snippet is not enough.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string"},
                        "max_chars": {"type": "integer", "description": "500-20000, default 6000"}
                    },
                    "required": ["url"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_skills",
                "description": "List the SiteBox skills bundled with you and the one-line rule for when each applies.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_skill",
                "description": "Read a bundled SiteBox skill before acting. The skill is the doctrine for that kind of work (design, data, create, verify, config, engine). Read it instead of guessing a workflow.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "description": "skill name, e.g. sitebox-design"},
                        "file": {"type": "string", "description": "optional markdown file inside the skill; default SKILL.md"}
                    },
                    "required": ["name"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_plan",
                "description": "Show or update the operator-visible plan. Call it before the first action with 2-5 steps, then call it again as steps move between pending, doing and done.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "steps": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "text": {"type": "string"},
                                    "status": {"type": "string", "enum": ["pending", "doing", "done"]}
                                },
                                "required": ["text"]
                            }
                        }
                    },
                    "required": ["steps"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_site_kind",
                "description": "Report how this site is produced (hand-authored, generated by a pipeline/build script, or node-backed) and which files are safe to edit.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_outline",
                "description": "Map a site's files. With no path: list source files and the outline of the main HTML page. With a path: sections, style/script blocks, functions, CSS rules, data keys with line ranges.",
                "parameters": {
                    "type": "object",
                    "properties": {"path": {"type": "string", "description": "site-relative file path, e.g. public/index.html"}}
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_read",
                "description": "Read a line range of a site file. Returns numbered lines. Read the exact region before proposing a patch.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "from": {"type": "integer", "description": "1-based first line (default 1)"},
                        "to": {"type": "integer", "description": "last line, inclusive (default from+199, max 400 lines)"}
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_site_start",
                "description": "Start the site this conversation is attached to. Use it when the site is stopped and you need a live page, HTTP checks or a screenshot, or when the user asks.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_site_stop",
                "description": "Stop the site this conversation is attached to. Only do this when the user asks: stopping a site takes it off the air until someone starts it again.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_site_restart",
                "description": "Restart the site this conversation is attached to, picking up a change to server.js or a manifest.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_check",
                "description": "Run the static check catalog (SB0xx) on the current site. Use before proposing when you need a baseline.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_verify",
                "description": "Run the HTTP checks (SB1xx) against the running site. Fails if the site is stopped.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_shot",
                "description": "Screenshot the running site with headless Chromium, if a browser is installed. Returns a URL the operator can open.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "width": {"type": "integer", "description": "default 1440"},
                        "height": {"type": "integer", "description": "default 900"},
                        "route": {"type": "string", "description": "path on the site, default /"}
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_propose_patch",
                "description": "Propose ONE exact-substring edit. `old` must occur exactly `count` times in the file or the server refuses. The server stages the patch in a copy, runs the static checks, and queues it for the operator to apply. You cannot write the file yourself.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "site-relative file path"},
                        "old": {"type": "string", "description": "exact existing text to replace, including indentation"},
                        "new": {"type": "string", "description": "replacement text"},
                        "count": {"type": "integer", "description": "exact number of occurrences of `old` (default 1)"},
                        "reason": {"type": "string", "description": "one sentence: what this changes and why"}
                    },
                    "required": ["path", "old", "new", "reason"]
                }
            }
        }
    ])
}

async fn create_site(st: &StudioState, thread_id: &str, args: &Value) -> Result<Value> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if id.is_empty() || name.is_empty() {
        bail!("`id` and `name` are required");
    }
    let thread = store::get(st, thread_id)?;
    if !thread.site_id.trim().is_empty() {
        bail!(
            "this conversation is already bound to `{}`; new sites need a new conversation",
            thread.site_id
        );
    }

    let port = match args.get("port").and_then(|v| v.as_u64()) {
        Some(p) => {
            let p = p as u16;
            if !(1024..=65535).contains(&p) {
                bail!("port must be between 1024 and 65535");
            }
            if util::port_bound(p) {
                bail!("port {p} is already in use");
            }
            p
        }
        None => free_port(&st.root).ok_or_else(|| anyhow::anyhow!("no free port in 4500-4899"))?,
    };

    let dir = crate::pipeline::scaffold(&st.root, &id, &name, port, &description)?;
    let manifest = crate::manifest::Manifest::load(&dir)?;
    let report = checks::run_static(&st.root, &manifest)?;
    if !report.ok() {
        // A scaffold that cannot pass its own starter checks is a bug; do not
        // leave a broken site behind for the operator to find.
        let _ = std::fs::remove_dir_all(&dir);
        if let Ok(mut cfg) = crate::registry::SiteConfig::read(&st.root) {
            cfg.sites.retain(|s| s.id != id);
            let p = st.root.join(crate::registry::REGISTRY_REL);
            let _ = super::atomic_write(&p, serde_json::to_string_pretty(&cfg)?.as_bytes());
        }
        bail!("the scaffolded site failed its own checks; nothing was kept");
    }

    // Bind the conversation to the new site so the rest of the turn (and every
    // later turn) can read, patch and run it.
    store::set_site(st, thread_id, &id)?;
    let start = st.sup.start(&manifest).await;
    super::emit(
        &st.events,
        thread_id,
        json!({"type":"site_created","site":id,"name":name,"port":port,
               "running":start.as_ref().map(|r| r.ok).unwrap_or(false)}),
    );

    Ok(json!({
        "ok": true,
        "site": id,
        "name": name,
        "port": port,
        "dir": dir.display().to_string(),
        "running": start.as_ref().map(|r| r.ok).unwrap_or(false),
        "verified": start.as_ref().map(|r| r.verified).unwrap_or(false),
        "start_error": start.err().map(|e| format!("{e:#}")),
        "files": ["sitebox.toml", "public/index.html", "public/favicon.svg"],
        "checks": { "ok": report.ok(), "findings": report.findings },
        "next": "The conversation is now bound to this site. Read sitebox-design, then improve the starter page with patches.",
    }))
}

fn free_port(root: &Path) -> Option<u16> {
    let used: std::collections::HashSet<u16> = crate::registry::SiteConfig::read(root)
        .map(|c| c.sites.iter().map(|s| s.port).collect())
        .unwrap_or_default();
    (4500..4900).find(|p| !used.contains(p) && !util::port_bound(*p))
}

fn tool_skills() -> Result<Value> {
    Ok(json!({"ok": true, "skills": skills::list()}))
}

fn tool_skill(args: &Value) -> Result<Value> {
    let skill = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let file = args.get("file").and_then(|v| v.as_str());
    let (path, text) = skills::read(skill, file)?;
    let total = text.chars().count();
    let clipped: String = text.chars().take(80_000).collect();
    Ok(json!({
        "ok": true,
        "name": skill,
        "file": path,
        "chars": total,
        "text": clipped,
        "note": "Follow this skill; do not restate it to the operator."
    }))
}

fn tool_plan(st: &StudioState, thread_id: &str, args: &Value) -> Result<Value> {
    let raw = args
        .get("steps")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut steps: Vec<store::PlanStep> = raw
        .iter()
        .filter_map(|s| {
            let text = s.get("text").and_then(|v| v.as_str())?.trim().to_string();
            if text.is_empty() {
                return None;
            }
            let status = match s.get("status").and_then(|v| v.as_str()) {
                Some("doing") => "doing",
                Some("done") => "done",
                _ => "pending",
            };
            Some(store::PlanStep {
                text,
                status: status.to_string(),
            })
        })
        .collect();
    if steps.is_empty() {
        bail!("plan needs at least one step");
    }
    steps.truncate(8);
    store::set_plan(st, thread_id, steps.clone())?;
    super::emit(&st.events, thread_id, json!({"type":"plan","steps":steps}));
    Ok(json!({"ok": true, "steps": steps}))
}

/// Tools that work before a site exists.
pub async fn execute_unbound(
    st: &StudioState,
    thread_id: &str,
    name: &str,
    args: &Value,
) -> Result<Value> {
    match name {
        "sols_skills" => tool_skills(),
        "sols_skill" => tool_skill(args),
        "sols_plan" => tool_plan(st, thread_id, args),
        "sols_create_site" => create_site(st, thread_id, args).await,
        "sols_search" => web_search(st, args).await,
        "sols_fetch" => web_fetch(args).await,
        other => bail!(
            "`{other}` needs a site, and this conversation has none yet. \
             Call sols_create_site first."
        ),
    }
}

/// Web search through the operator's SearXNG. Configured with `SEARXNG_URL`
/// (environment first, then `.env` at the data root).
async fn web_search(st: &StudioState, args: &Value) -> Result<Value> {
    let base = config::env_or_file(&st.root, "SEARXNG_URL").ok_or_else(|| {
        anyhow::anyhow!(
            "web search is not configured: set SEARXNG_URL (for example http://127.0.0.1:8899)"
        )
    })?;
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if query.chars().count() < 2 {
        bail!("query is too short");
    }
    let count = args
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .clamp(1, 10) as usize;
    let mut params: Vec<(&str, String)> = vec![("q", query.clone()), ("format", "json".into())];
    if let Some(lang) = args
        .get("language")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        params.push(("language", lang.to_string()));
    }
    if let Some(engines) = config::env_or_file(&st.root, "SEARXNG_ENGINES") {
        params.push(("engines", engines));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("SiteBox-Sols/1.0")
        .build()?;
    let url = format!("{}/search", base.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .query(&params)
        .send()
        .await
        .with_context(|| format!("search request to {url} failed"))?;
    let status = resp.status();
    if !status.is_success() {
        bail!("search answered {status}; check SEARXNG_URL and that JSON is enabled in SearXNG");
    }
    let body: Value = resp.json().await.context("search returned invalid JSON")?;

    let mut results = Vec::new();
    for r in body
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
    {
        if results.len() >= count {
            break;
        }
        let link = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if link.is_empty() {
            continue;
        }
        results.push(json!({
            "title": r.get("title").and_then(|v| v.as_str()).unwrap_or(""),
            "url": link,
            "snippet": r.get("content").and_then(|v| v.as_str()).unwrap_or(""),
            "engine": r.get("engine").and_then(|v| v.as_str()).unwrap_or(""),
        }));
    }
    let answers: Vec<String> = body
        .get("answers")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    Ok(json!({
        "ok": true,
        "query": query,
        "results": results,
        "answers": answers,
        "note": if results.is_empty() {
            "No results. Say so in one line; do not fill the gap from memory."
        } else {
            "Snippets are unverified search results. Cite the URL you used; fetch it if the detail matters."
        },
    }))
}

fn host_is_public(host: &str) -> bool {
    let h = host.trim().to_ascii_lowercase();
    !(h.is_empty()
        || h == "localhost"
        || h.ends_with(".local")
        || h.ends_with(".internal")
        || h.ends_with(".localhost"))
}

fn ip_is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_multicast())
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80)
        }
    }
}

/// Parse and vet a URL before any request leaves the box. Name lookups happen
/// here so a DNS name that resolves to the LAN is refused too.
fn safe_url(raw: &str) -> Result<reqwest::Url> {
    let u = reqwest::Url::parse(raw).with_context(|| format!("not a URL: {raw}"))?;
    if !matches!(u.scheme(), "http" | "https") {
        bail!("only http(s) URLs can be fetched");
    }
    let host = u.host_str().unwrap_or("");
    if !host_is_public(host) {
        bail!("refusing to fetch a private or local address");
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !ip_is_public(ip) {
            bail!("refusing to fetch {ip}");
        }
    }
    if let Ok(addrs) = (host, u.port_or_known_default().unwrap_or(80)).to_socket_addrs() {
        for a in addrs {
            if !ip_is_public(a.ip()) {
                bail!("{host} resolves to a private address ({})", a.ip());
            }
        }
    }
    Ok(u)
}

async fn web_fetch(args: &Value) -> Result<Value> {
    let raw = args
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if raw.is_empty() {
        bail!("url is required");
    }
    let url = safe_url(raw)?;
    let max = args
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .unwrap_or(6000)
        .clamp(500, 20000) as usize;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("SiteBox-Sols/1.0")
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 4
                || !host_is_public(attempt.url().host_str().unwrap_or(""))
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()?;
    let resp = client
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("fetching {url} failed"))?;
    let status = resp.status();
    if !status.is_success() {
        bail!("{url} answered {status}");
    }
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !(ctype.contains("text/")
        || ctype.contains("json")
        || ctype.contains("xml")
        || ctype.is_empty())
    {
        bail!("{url} is {ctype}; only text pages can be read");
    }
    let bytes = resp.bytes().await?;
    if bytes.len() > 2 * 1024 * 1024 {
        bail!("{url} is {} bytes; the limit is 2 MB", bytes.len());
    }
    let body = String::from_utf8_lossy(&bytes);
    let text = if ctype.contains("html") {
        html_to_text(&body)
    } else {
        body.to_string()
    };
    let total = text.chars().count();
    let clipped: String = text.chars().take(max).collect();
    Ok(json!({
        "ok": true,
        "url": url.as_str(),
        "chars": total,
        "truncated": total > max,
        "text": clipped,
    }))
}

/// Enough HTML understanding for reading a page: drop script/style, turn block
/// boundaries into newlines, decode the entities that appear in prose.
fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut tag = String::new();
    let mut in_tag = false;
    let mut skip: Option<&str> = None;

    for c in html.chars() {
        if !in_tag {
            if c == '<' {
                in_tag = true;
                tag.clear();
            } else if skip.is_none() {
                out.push(c);
            }
            continue;
        }
        if c == '>' {
            in_tag = false;
            let t = tag.trim().to_ascii_lowercase();
            let name = t
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("");
            if t.starts_with("script") || t.starts_with("style") {
                skip = Some(if t.starts_with("script") {
                    "script"
                } else {
                    "style"
                });
            } else if t.starts_with("/script") || t.starts_with("/style") {
                skip = None;
            } else if matches!(
                name,
                "p" | "br"
                    | "div"
                    | "section"
                    | "article"
                    | "li"
                    | "tr"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "blockquote"
                    | "pre"
                    | "td"
            ) {
                out.push('\n');
            }
            continue;
        }
        tag.push(c);
    }

    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&rsquo;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–");

    let mut compact = String::with_capacity(decoded.len());
    let mut blank = 0;
    for line in decoded.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        compact.push_str(&line);
        compact.push('\n');
    }
    compact.trim().to_string()
}

pub async fn execute(
    st: &StudioState,
    site: &SiteRef,
    thread_id: &str,
    name: &str,
    args: &Value,
    auto_apply: bool,
) -> Result<Value> {
    match name {
        "sols_search" => web_search(st, args).await,
        "sols_fetch" => web_fetch(args).await,
        "sitebox_grep" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if query.chars().count() < 3 {
                bail!("query must be at least 3 characters");
            }
            let ignore_case = args
                .get("ignore_case")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let glob = args.get("glob").and_then(|v| v.as_str());
            let max = args
                .get("max")
                .and_then(|v| v.as_u64())
                .unwrap_or(40)
                .clamp(1, 200) as usize;
            let needle = if ignore_case {
                query.to_lowercase()
            } else {
                query.to_string()
            };

            let mut matches = Vec::new();
            let mut files_hit = 0usize;
            let mut truncated = false;
            for (rel, _) in outline::source_files(&site.dir, 2000) {
                if let Some(g) = glob {
                    if !glob_match(g, &rel) {
                        continue;
                    }
                }
                let Ok(p) = site_path(&site.dir, &rel) else {
                    continue;
                };
                let Ok(text) = std::fs::read_to_string(&p) else {
                    continue;
                };
                let mut hit = false;
                for (i, line) in text.lines().enumerate() {
                    let hay = if ignore_case {
                        line.to_lowercase()
                    } else {
                        line.to_string()
                    };
                    if hay.contains(&needle) {
                        hit = true;
                        if matches.len() < max {
                            let trimmed: String = line.trim().chars().take(180).collect();
                            matches.push(json!({"path": rel, "line": i + 1, "text": trimmed}));
                        } else {
                            truncated = true;
                        }
                    }
                }
                if hit {
                    files_hit += 1;
                }
            }
            Ok(json!({
                "ok": true,
                "query": query,
                "files": files_hit,
                "matches": matches,
                "truncated": truncated,
            }))
        }
        "sols_skills" => tool_skills(),
        "sols_skill" => tool_skill(args),
        "sols_plan" => tool_plan(st, thread_id, args),
        "sitebox_site_kind" => Ok(json!({
            "ok": true,
            "site": site.id,
            "kind": site.kind,
            "why": site.kind_reason,
            "editable": if site.kind == "generated" {
                json!(["tools/", "synd/", "data files", "sitebox.toml (only with a reason)"])
            } else {
                json!(["public/index.html", "public/*.css", "public/*.js", "server.js"])
            },
            "port": site.manifest.site.port,
            "serve": site.manifest.site.serve,
            "running": st.sup.is_running(&site.id).await,
            "pipeline_steps": site.manifest.pipeline.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        })),
        "sitebox_outline" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.trim().is_empty() {
                let files = outline::source_files(&site.dir, 40)
                    .into_iter()
                    .map(|(p, s)| json!({"path": p, "bytes": s}))
                    .collect::<Vec<_>>();
                let main = ["public/index.html", "index.html"]
                    .iter()
                    .find(|p| site.dir.join(p).is_file())
                    .map(|p| p.to_string());
                let mut items = match &main {
                    Some(p) => outline::items(&site.dir, p).unwrap_or_default(),
                    None => Vec::new(),
                };
                // The map is for the model's orientation, not for the reply. A
                // full outline invites the model to read it back to the user.
                let total = items.len();
                items.truncate(24);
                Ok(json!({
                    "ok": true,
                    "kind": site.kind,
                    "files": files,
                    "main": main,
                    "items": items,
                    "items_total": total,
                    "hint": "For orientation only: do not enumerate these back. Read a region with sitebox_read when you are about to patch it."
                }))
            } else {
                let items = outline::items(&site.dir, path)?;
                Ok(json!({"ok": true, "path": path, "count": items.len(), "items": items}))
            }
        }
        "sitebox_read" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let p = site_path(&site.dir, path)?;
            let text = std::fs::read_to_string(&p)
                .map_err(|_| anyhow::anyhow!("{path} is missing or not UTF-8 text"))?;
            let lines: Vec<&str> = text.lines().collect();
            let total = lines.len();
            let from = args
                .get("from")
                .and_then(|v| v.as_u64())
                .unwrap_or(1)
                .max(1) as usize;
            let want_to = args
                .get("to")
                .and_then(|v| v.as_u64())
                .unwrap_or((from + 199) as u64) as usize;
            let to = want_to
                .min(from.saturating_add(MAX_READ_LINES - 1))
                .min(total);
            if from > total {
                bail!("from {from} is past the end ({total} lines)");
            }
            let body = lines[from - 1..to]
                .iter()
                .enumerate()
                .map(|(i, l)| format!("{:>6}| {}", from + i, l))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(json!({
                "ok": true,
                "path": path,
                "from": from,
                "to": to,
                "total_lines": total,
                "truncated": to < total,
                "text": body,
            }))
        }
        "sitebox_site_start" => {
            let r = st.sup.start(&site.manifest).await?;
            Ok(json!({
                "ok": r.ok,
                "running": st.sup.is_running(&site.id).await,
                "port": r.port,
                "verified": r.verified,
                "message": r.message,
            }))
        }
        "sitebox_site_stop" => {
            st.sup.stop(&site.id).await?;
            Ok(json!({
                "ok": true,
                "running": st.sup.is_running(&site.id).await,
                "port": site.manifest.site.port,
                "message": format!("{} stopped", site.id),
            }))
        }
        "sitebox_site_restart" => {
            let r = st.sup.restart(&site.manifest).await?;
            Ok(json!({
                "ok": r.ok,
                "running": st.sup.is_running(&site.id).await,
                "port": r.port,
                "verified": r.verified,
                "message": r.message,
            }))
        }
        "sitebox_check" => {
            let report = checks::run_static(&st.root, &site.manifest)?;
            Ok(json!({
                "ok": report.ok(),
                "mode": "static",
                "findings": report.findings,
            }))
        }
        "sitebox_verify" => {
            if !st.sup.is_running(&site.id).await {
                bail!(
                    "site {} is not running; start it in the dashboard first",
                    site.id
                );
            }
            let base = format!("http://127.0.0.1:{}", site.manifest.site.port);
            // The no-gzip client is deliberate: reqwest strips Content-Encoding
            // after auto-decompressing, so SB104 would fail on a correct server.
            let report = checks::run_http(&base, &st.check_client).await;
            Ok(json!({"ok": report.ok(), "base": base, "findings": report.findings}))
        }
        "sitebox_shot" => {
            let width = args.get("width").and_then(|v| v.as_u64()).unwrap_or(1440) as u32;
            let height = args.get("height").and_then(|v| v.as_u64()).unwrap_or(900) as u32;
            let route = args.get("route").and_then(|v| v.as_str()).unwrap_or("/");
            if !st.sup.is_running(&site.id).await {
                bail!(
                    "site {} is not running; start it before asking for a screenshot",
                    site.id
                );
            }
            shot::capture(st, &site.id, site.manifest.site.port, width, height, route).await
        }
        "sitebox_propose_patch" => propose(st, site, thread_id, args, auto_apply).await,
        other => bail!("unknown tool `{other}`"),
    }
}

async fn propose(
    st: &StudioState,
    site: &SiteRef,
    thread_id: &str,
    args: &Value,
    auto_apply: bool,
) -> Result<Value> {
    let spec = PatchSpec {
        path: args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        old: args
            .get("old")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        new: args
            .get("new")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        count: args.get("count").and_then(|v| v.as_u64()).unwrap_or(1) as usize,
        reason: args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };
    super::patch::validate(&spec, &site.kind)?;

    let target = site_path(&site.dir, &spec.path)?;
    let size = std::fs::metadata(&target)
        .with_context(|| format!("{} does not exist", spec.path))?
        .len();
    if size > super::patch::MAX_FILE_BYTES {
        bail!(
            "{} is {size} bytes; the patch limit is {}",
            spec.path,
            super::patch::MAX_FILE_BYTES
        );
    }
    let before = std::fs::read_to_string(&target)
        .map_err(|_| anyhow::anyhow!("{} is not UTF-8 text", spec.path))?;
    let after = super::patch::apply_text(&before, &spec.old, &spec.new, spec.count)?;
    super::patch::html_sanity(&spec.path, &after)?;
    let (diff, stat) = super::patch::diff_hunk(&before, &after);

    let (report, _stage_id) = stage::verify(st, site, &spec)?;
    let verify = stage::summarise(&report);

    let rec = store::PatchRec {
        id: super::new_id("pt"),
        site_id: site.id.clone(),
        thread_id: thread_id.to_string(),
        path: spec.path.clone(),
        old: spec.old.clone(),
        new: spec.new.clone(),
        count: spec.count,
        reason: spec.reason.clone(),
        diff: diff.clone(),
        status: "pending".into(),
        created: util::now_iso(),
        applied: None,
        verify: verify.clone(),
    };
    store::put_patch(st, &rec)?;
    super::emit(
        &st.events,
        thread_id,
        json!({"type":"patch","patch":{
            "id": rec.id, "path": rec.path, "reason": rec.reason,
            "diff": rec.diff, "added": stat.added, "removed": stat.removed,
            "verify": rec.verify,
        }}),
    );

    let checks_ok = verify.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);

    // A conversation that is still creating its site applies as it goes: the
    // operator asked for a site, not for a pile of pending cards. Snapshot,
    // build and checks are the same as a manual Apply, and Undo still works.
    if auto_apply {
        let applied = super::apply::apply_locked(st, &rec.id).await;
        let ok = applied
            .as_ref()
            .map(|r| r.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
            .unwrap_or(false);
        let status = if ok { "applied" } else { "rolled-back" };
        super::emit(
            &st.events,
            thread_id,
            json!({"type":"patch_applied","patch_id":rec.id,"ok":ok,"status":status}),
        );
        match applied {
            Ok(res) => Ok(json!({
                "ok": res.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
                "queued": false,
                "applied": true,
                "patch_id": rec.id,
                "path": rec.path,
                "added": stat.added,
                "removed": stat.removed,
                "apply": res,
                "next": "Applied to the new site. If the checks are red it was rolled back; read `apply.introduced`.",
            })),
            Err(e) => Ok(json!({
                "ok": false,
                "queued": true,
                "patch_id": rec.id,
                "path": rec.path,
                "error": format!("{e:#}"),
                "next": "Applying failed. Read the error; if the patch is stale, propose it again.",
            })),
        }
    } else {
        Ok(json!({
            "ok": true,
            "queued": true,
            "patch_id": rec.id,
            "path": rec.path,
            "added": stat.added,
            "removed": stat.removed,
            "staged_checks_ok": checks_ok,
            "verify": verify,
            "next": "The operator sees this patch in the panel and clicks Apply. Do not claim the site is changed yet.",
        }))
    }
}

pub fn system_prompt(site: Option<&SiteRef>, page: Option<&str>) -> String {
    let skill_index = skills::list()
        .iter()
        .map(|s| {
            let d: String = s.description.chars().take(150).collect();
            format!("- {}: {}", s.name, d)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let Some(site) = site else {
        return format!(
            r#"You are Sols, the editing agent inside SiteBox. This conversation is not attached to a site yet — your job is to create one with `sols_create_site` and then build it.

Work the way SiteBox's own agent does: read the skill that owns the task, plan in visible steps, execute with tools, verify with checks, report in a few lines.

## How you work
1. Say what the site is in one sentence.
2. Read `sitebox-design` before you choose anything visual, and `sitebox-create` when the site is more than one page.
3. Call `sols_plan` with 3-5 steps before the first action, then update it as steps move between pending, doing and done.
4. Ask only what you cannot infer: what the site is for, who reads it, and what content is real. Choose `id` and `name` yourself and say what you chose.
5. `sols_create_site` writes the folder, manifest, starter page and registry entry, picks a free port, starts the site and binds this conversation to it. It already starts the site: do not call `sitebox_site_start` afterwards. A static site serves files from disk, so a patch needs no restart either.
6. The starter is a blank canvas, not a finished site: read it, then improve it with `sitebox_propose_patch`. While you are creating the site, your patches are applied immediately (snapshot, build and checks still run); after this turn they will wait for the operator's Apply.

## Skills (bundled, read on demand)
{skill_index}

Routing: look, layout, copy, tokens -> sitebox-design; real content, numbers, sources -> sitebox-data; build or change a page -> sitebox-create; prove it -> sitebox-verify; register, port, start/stop, logs -> sitebox-config; manifest, pipeline, engine commands -> sitebox-engine. `sols_skill` reads a file; `sols_skills` lists every file.

## Rules that do not change
You cannot write files except through `sols_create_site` (once, before a site exists) and `sitebox_propose_patch`. During creation a patch is applied as soon as the checks pass, so you may say it is live — only after the tool says `applied`. After the creation turn, patches are queued and you must not call them live. Verify with `sitebox_check` and `sitebox_verify` before you claim anything.

## Looking things up
- `sols_search` is a real web search through SearXNG. Use it for facts you cannot read from the site: a company, product, place, person, price, domain, or a current event.
- Cite the result URL you used. Snippets are search results, not verified facts; if the search returns nothing, say so in one line instead of guessing.
- `sols_fetch` opens one result when its snippet is not enough. Only fetch a URL the request is about or a search returned.
- You do have search. Never tell the operator you have no search tool.

## Language
Answer in the language the operator used, and only that language. Never mix scripts: a Chinese or Japanese character inside a Thai or English sentence is a bug, not a style.

## How to answer
Three to six short lines, at most three bullets, plain words for checks, `path:line` only for the line you change, one next step. Thai when the operator writes Thai."#
        );
    };

    let files = outline::source_files(&site.dir, 20)
        .into_iter()
        .map(|(p, s)| format!("  - {p} ({} KB)", s / 1024))
        .collect::<Vec<_>>()
        .join("\n");
    let where_ = match page {
        Some(p) => format!(
            "The operator opened this conversation while viewing `{p}`. Start there. \
             If the text is not on that page, say so before reading other pages."
        ),
        None => "The operator's current page is unknown. Ask if the page matters.".to_string(),
    };
    format!(
        r#"You are Sols, the editing agent inside SiteBox. You work on exactly one site: `{id}` ({name}).

You work the way SiteBox's own agent does when it builds a site: read the skill that owns the task, plan in visible steps, execute with tools, verify with checks, report in a few lines.

## How you work
1. Say the goal in one sentence.
2. Load the owning skill with `sols_skill` before acting. The skills are the doctrine; do not invent a workflow they already define.
3. Call `sols_plan` with 2-5 steps before the first action, then again as steps move between pending, doing and done.
4. Batch the work: one outline, then read only the region you will change. Never re-read what you already have.
5. Verify with `sitebox_check`, and `sitebox_verify` when the site is running, before you claim anything.
6. Close with what changed, what the checks said, and one next step.

## Where the operator is
{where_}

## Skills (bundled, read on demand)
{skill_index}

Routing: look, layout, copy, tokens → sitebox-design · real content, numbers, sources → sitebox-data · build or change a page → sitebox-create · prove it → sitebox-verify · register, port, start/stop, logs → sitebox-config · manifest, pipeline, engine commands → sitebox-engine · the skills themselves → sitebox-skill-maintainer. `sols_skill` reads the file; `sols_skills` lists every file including references.

## How this site is produced
kind: {kind} — {why}
port: {port}, serve: {serve}
public dir: {public}
pipeline steps: {steps}

## Files (largest first)
{files}

## The one rule that matters
You cannot write files. You have `sitebox_propose_patch`, which stages an exact-substring edit in a copy, runs the static checks, and queues it for the operator. The operator clicks Apply; the server snapshots first and rolls back automatically if a check fails. Never say a change is live, applied, or done — say it is proposed or queued.

## Finding what they mean
- Use `sitebox_grep` for "where does this text live": one call returns `path:line` for every page that contains it. Do not read files one at a time to count occurrences.
- If the text they quote appears in several files (a footer, a tagline, a nav), say that plainly and offer to change every page in one pass: one patch per file. Never silently change one page of ten.
- A phrase repeated across pages is a multi-file edit. Propose at most three patches in one turn, then stop and tell the operator to apply them and say "continue". Do not try to finish ten files in one turn.
- Read at most three regions before you either propose a patch or ask one question. If which page or which wording is unclear, ask — do not tour the site.

## How to edit
1. `sitebox_outline` (no path) to see the shape, then `sitebox_read` the exact region.
2. Quote `old` exactly as it appears, including indentation. State `count` as the exact number of occurrences.
3. Keep one patch to one coherent change. Split unrelated edits.
4. If `count` does not match, the server refuses and tells you the real count: read again, do not guess.
5. Generated site? Edit the source (`tools/`, `synd/`, data), not `public/`. The build overwrites `public/`.

## Checks
Static: SB001 index exists, SB002 no third-party asset, SB003 every <img> has width/height, SB004 splice markers, SB005 referenced assets exist, SB010 image dims match, SB020/021 no placeholders or NaN, SB030 JSON parses. HTTP: SB101-SB107.
A red check means the patch is wrong. Fix it before proposing again; do not argue with the check.

## Site lifecycle
You may start, stop and restart the site this conversation is attached to: `sitebox_site_start`, `sitebox_site_stop`, `sitebox_site_restart`.
- Start it when you need a live page, HTTP checks or a screenshot, or when the user asks.
- Stop it only when the user asks. Never stop it to "clean up" or as a side effect of another task.
- Restart after changing `server.js` or `sitebox.toml`; a static site reads files from disk and needs no restart.
- The tool result carries `running` and `verified`. Report those, not your intention.

## Design
Reuse the site's existing CSS custom properties. Do not introduce a new colour, font, or spacing scale unless the user asks. Purple/violet/indigo gradients are the default AI look and are not a design choice — avoid them unless the site already uses them.

## Looking things up
- `sols_search` is a real web search through SearXNG. Use it for facts you cannot read from the site: a company, product, place, person, price, domain, or a current event.
- Cite the result URL you used. Snippets are search results, not verified facts; if the search returns nothing, say so in one line instead of guessing.
- `sols_fetch` opens one result when its snippet is not enough. Only fetch a URL the request is about or a search returned.
- You do have search. Never tell the operator you have no search tool.

## Language
Answer in the language the operator used, and only that language. Never mix scripts: a Chinese or Japanese character inside a Thai or English sentence is a bug, not a style.

## How to answer
Write for a busy operator reading on a phone, not for a log file.

- Start with the answer in one or two sentences: what you found, or what you want to change.
- Default length is three to six short lines. A full inventory of sections, files or checks is not an answer.
- Use at most three bullets, and only when they are choices. Do not enumerate everything the tools returned.
- Talk about checks in plain words: "one check fails — an external font" rather than "SB002 fail". Give a check id only when the user asks about checks or a patch is blocked by that check.
- Cite `path:line` only for the line you are changing. Never paste a path:line range for every block you saw.
- Do not restate the request, the tool output, or your own conclusion.
- End with one concrete next step or one question. No filler and no "let me know if…".
- Markdown: short paragraphs, **bold** for the single thing that matters, bullets for options, fenced code only for code the user will read.
- Answer in the user's language (Thai when they write Thai) and use their words for their own content.

A good answer to "how is this page built?" is:
"markup เป็นหน้าเดียวจบ (53 KB): เมนูบน ฟีดกลาง และ sidebar สองข้าง
เช็คผ่าน 5 ข้อ ไม่ผ่าน 1 — ฟอนต์ Inter/Noto โหลดจาก Google ต้อง vendor เข้ามา
ให้ข้าเสนอ patch ย้ายฟอนต์ไหม?"

Not a list of every section with line ranges."#,
        id = site.id,
        name = site.manifest.site.name,
        kind = site.kind,
        why = site.kind_reason,
        port = site.manifest.site.port,
        serve = site.manifest.site.serve,
        public = site.manifest.site.public_dir,
        steps = if site.manifest.pipeline.is_empty() {
            "none".to_string()
        } else {
            site.manifest
                .pipeline
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        },
        files = if files.is_empty() {
            "  (no source files found)".into()
        } else {
            files
        },
    )
}

/// Tiny glob: `*` matches anything (including `/`), `?` matches one character.
/// Enough for `public/*.html`, which is how the model asks for a file set.
fn glob_match(pat: &str, s: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = s.chars().collect();
    let (n, m) = (p.len(), t.len());
    let mut dp = vec![vec![false; m + 1]; n + 1];
    dp[0][0] = true;
    for i in 1..=n {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=n {
        for j in 1..=m {
            dp[i][j] = match p[i - 1] {
                '*' => dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i - 1][j - 1],
                c => dp[i - 1][j - 1] && c == t[j - 1],
            };
        }
    }
    dp[n][m]
}
