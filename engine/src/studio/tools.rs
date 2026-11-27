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
        json!({
            "type": "function",
            "function": {
                "name": "sols_rename_site",
                "description": "Rename the site everywhere SiteBox shows it: the dashboard card (registry) and the site manifest. Use whenever the operator asks to change the app or site name — editing only the page <title> is not a rename.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "description": "New display name"},
                        "description": {"type": "string", "description": "Optional new one-line description"}
                    },
                    "required": ["name"]
                }
            }
        }),
        {
            "type": "function",
            "function": {
                "name": "sols_youtube",
                "description": "Read a real YouTube channel: title, avatar, subscriber count and the most recent videos (id, title, thumbnail, views, published). Use it before building any page about a channel, then download the thumbnails with sols_asset.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "channel": {"type": "string", "description": "handle or URL, e.g. 9arm, @9arm, https://www.youtube.com/@9arm"},
                        "limit": {"type": "integer", "description": "videos to return, 1-12, default 8"}
                    },
                    "required": ["channel"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_steam",
                "description": "Read the real Steam store front (Thai region): top sellers, new releases and current specials, with game id, name, original/final price in baht, discount percent and header image URL. Use it instead of scraping the store pages, then download the images with sols_asset.",
                "parameters": {"type": "object", "properties": {
                    "limit": {"type": "integer", "description": "items per category, 4-10, default 8"}
                }}
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
                        "max_chars": {"type": "integer", "description": "500-20000, default 6000"},
                        "render": {"type": "boolean", "description": "run the page in headless Chromium first (for JS apps)"}
                    },
                    "required": ["url"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_import",
                "description": "Copy a file the operator dropped into `<root>/imports/` into this site (images, data, logos). Use `sitebox_ls` after to confirm. Cannot read anywhere else on the machine.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "from": {"type": "string", "description": "path under imports/, e.g. field-notes/photo.jpg"},
                        "to": {"type": "string", "description": "site-relative destination; default assets-raw/<file name>"}
                    },
                    "required": ["from"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_build",
                "description": "Run the site's pipeline (image resize, data generation) now and return each step's status. A build writes into public/, so run it before checking or verifying. Pass force when you changed the build script, which is not tracked as an input.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "force": {"type": "boolean", "description": "rerun steps whose inputs look unchanged"}
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_ls",
                "description": "List files in the site (text and binary) with sizes. `sitebox_outline` only knows source files, so use this to see images under public/assets or anything else you or a tool wrote.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "dir": {"type": "string", "description": "site-relative folder, default the whole site"},
                        "max": {"type": "integer", "description": "default 200"}
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_http",
                "description": "Call a route on this site (localhost only) and return the status and body. Use it to test /api/ endpoints you just wrote — a 200 with the wrong JSON is still a bug.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "method": {"type": "string", "description": "GET or POST, default GET"},
                        "path": {"type": "string", "description": "path on the site, e.g. /api/chat"},
                        "body": {"type": "object", "description": "JSON body for POST"}
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_asset",
                "description": "Download public photos (jpeg/png/webp/gif) into public/assets and return paths and pixel sizes. Pass one `url`, or `items` for a batch (up to 12). Only adds new files, never overwrites.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string"},
                        "name": {"type": "string", "description": "file name without extension, e.g. feed-01"},
                        "items": {
                            "type": "array",
                            "description": "batch form: [{url, name}]",
                            "items": {"type": "object", "properties": {"url": {"type": "string"}, "name": {"type": "string"}}}
                        }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_probe",
                "description": "Render pages at a given width and report layout numbers: elements wider than the viewport, clipped scroll containers, and text below 12px. Pass every route you changed in one call with `routes` — one round instead of one per page.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "route": {"type": "string", "description": "one path on the site, default /"},
                        "routes": {"type": "array", "items": {"type": "string"}, "description": "up to 6 paths, probed in parallel, e.g. [\"/\", \"/registry/\", \"/cases/a.html\"]"},
                        "width": {"type": "integer", "description": "default 500 (the headless browser minimum), max 1440"},
                        "height": {"type": "integer", "description": "default 900"}
                    }
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
                "name": "sitebox_new_page",
                "description": "Create a new HTML page by copying the shell (head, header/nav, footer, stylesheet links) of an existing page in the same site, then filling it with the sections you supply. Use this instead of writing a whole page in one patch: the call carries only the content, and the page already matches the site. Afterwards patch the details (classes, imagery).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "new file path, e.g. public/cases/ubong-2531.html"},
                        "title": {"type": "string", "description": "page title and h1"},
                        "description": {"type": "string", "description": "meta description, one sentence"},
                        "template": {"type": "string", "description": "existing page to copy the shell from; default: index.html in the same folder, else public/index.html"},
                        "back": {"type": "object", "properties": {"href": {"type": "string"}, "text": {"type": "string"}}, "description": "optional link before the footer, e.g. {href: '../index.html', text: '← กลับสารบัญ'}"},
                        "sections": {
                            "type": "array",
                            "description": "page content in order; each becomes a section with an h2",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "heading": {"type": "string"},
                                    "body": {"type": "string", "description": "one paragraph of real copy"},
                                    "items": {"type": "array", "items": {"type": "string"}, "description": "optional bullet list"}
                                },
                                "required": ["heading", "body"]
                            }
                        }
                    },
                    "required": ["path", "title", "sections"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sitebox_propose_patch",
                "description": "Propose exact-substring edits. Each `old` must occur exactly `count` times in its file or the server refuses. Pass `path/old/new` for one, or `patches` for up to 12 in one call. The server stages each patch in a copy, runs the static checks, and queues it for the operator (or applies it during site creation).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "site-relative file path"},
                        "old": {"type": "string", "description": "exact existing text to replace, including indentation"},
                        "new": {"type": "string", "description": "replacement text"},
                        "count": {"type": "integer", "description": "exact number of occurrences of `old` (default 1)"},
                        "reason": {"type": "string", "description": "one sentence: what this changes and why"},
                        "create": {"type": "boolean", "description": "write a new file (send the whole content in `new`; no `old`)"},
                        "patches": {
                            "type": "array",
                            "description": "batch form: [{path, old, new, count?, reason}]",
                            "items": {"type": "object", "properties": {
                                "path": {"type": "string"}, "old": {"type": "string"},
                                "new": {"type": "string"}, "count": {"type": "integer"},
                                "reason": {"type": "string"}, "create": {"type": "boolean"}
                            }, "required": ["path", "new", "reason"]}
                        }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_design_index",
                "description": "List the design combinations recent sites used (archetype, palette, type pairing, layout, signature). Call this before choosing a look, so a new site does not repeat the last one.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_design",
                "description": "Record the five design axes you chose for this site. Do it before building. If another site already uses the same combination, vary at least one axis unless the brief asks for a repeat.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "site": {"type": "string", "description": "site id this design belongs to"},
                        "archetype": {"type": "string", "description": "e.g. editorial index, dense product feed, institutional archive, docs shell, full-screen app"},
                        "palette": {"type": "string", "description": "the concrete family, e.g. paper + ink + rust, chalk + forest + brass"},
                        "type_pair": {"type": "string", "description": "display + body pairing, e.g. serif Thai display + sans body at 17px"},
                        "layout": {"type": "string", "description": "skeleton, e.g. sticky rail + 3-col grid, centred 68ch column, split hero + table"},
                        "signature": {"type": "string", "description": "one element unique to this site, e.g. numbered margin rules, gauge ladder, monospace stamp header"}
                    },
                    "required": ["site", "archetype", "palette", "type_pair", "layout", "signature"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_bee",
                "description": "Send worker bees (1-4) to do independent work in parallel, then get their reports. A bee reads, greps, searches and fetches; it cannot edit files, apply patches or restart the site, and it does not see this conversation. Use for big jobs: research across several sources, a multi-page build, a clone that needs structure plus copy plus facts. Before a site exists it can still research the web and the skills (no site files). After the reports come back, make the edits yourself.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tasks": {
                            "type": "array",
                            "description": "One self-contained assignment per bee. Max 4.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "title": {"type": "string", "description": "3-6 words, shown in the UI"},
                                    "brief": {"type": "string", "description": "The full assignment: what to find or write, the shape of the answer, the language, and what counts as done. The bee cannot see this chat."},
                                    "effort": {"type": "string", "enum": ["quick", "deep"], "description": "quick = 4 rounds / 8 calls for a mechanical sweep or a narrow audit; deep (default) = 8 rounds / 14 calls for research or design."}
                                },
                                "required": ["title", "brief"]
                            }
                        }
                    },
                    "required": ["tasks"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "sols_board",
                "description": "Read the scratchpad the bees wrote during this turn. Use it when a report is thin or when you want their raw findings without another swarm.",
                "parameters": {"type": "object", "properties": {}}
            }
        }
    ])
}

/// Rename is an identity change, not a copy edit: the dashboard reads the
/// registry, so a page `<title>` alone leaves the card showing the old name.
async fn rename_site(st: &StudioState, site: &SiteRef, args: &Value) -> Result<Value> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        bail!("`name` is required");
    }
    if name.chars().count() > 60 {
        bail!("`name` is longer than 60 characters");
    }
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());

    let mut manifest = crate::manifest::Manifest::load(&site.dir)?;
    let old = manifest.site.name.clone();
    manifest.site.name = name.clone();
    if let Some(d) = &description {
        manifest.site.description = d.clone();
    }
    manifest.save(&site.dir)?;

    let mut cfg = crate::registry::SiteConfig::read(&st.root)?;
    let mut found = false;
    for entry in cfg.sites.iter_mut().filter(|s| s.id == site.id) {
        entry.name = name.clone();
        if let Some(d) = &description {
            entry.description = d.clone();
        }
        found = true;
    }
    if found {
        cfg.write(&st.root)?;
    }

    super::emit(
        &st.events,
        "",
        json!({"type":"site_renamed","site":site.id,"name":name,"old":old,"dashboard":found}),
    );
    Ok(json!({
        "ok": true,
        "site": site.id,
        "old_name": old,
        "name": name,
        "dashboard_updated": found,
        "note": if found { "The dashboard card reads the new name on its next load." } else { "No registry entry matched this site; only the manifest changed." }
    }))
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

pub fn tool_skills() -> Result<Value> {
    Ok(json!({"ok": true, "skills": skills::list()}))
}

pub fn tool_skill(args: &Value) -> Result<Value> {
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

/// Capability that does not need a site folder: doctrine, design memory,
/// search, real-data readers, the board and bees. Both the unbound turn and the
/// site-bound turn cross this seam, so a tool can no longer exist in one
/// dispatcher and be missing from the other — that drift made `sols_steam`
/// answer "unknown tool" on a site conversation while its schema was advertised.
pub async fn shared_tool(
    st: &StudioState,
    thread_id: &str,
    site: Option<&SiteRef>,
    name: &str,
    args: &Value,
) -> Option<Result<Value>> {
    Some(match name {
        "sols_design_index" => Ok(super::design::list(st)),
        "sols_design" => super::design::record(st, args),
        "sols_skills" => tool_skills(),
        "sols_skill" => tool_skill(args),
        "sols_plan" => tool_plan(st, thread_id, args),
        "sols_search" => web_search(st, args).await,
        "sols_fetch" => web_fetch(st, args).await,
        "sols_youtube" => youtube(args).await,
        "sols_steam" => steam(args).await,
        "sols_board" => {
            let notes = super::bee::notes(st, thread_id).await;
            Ok(json!({
                "ok": true,
                "notes": notes,
                "note": "Findings the bees left for each other this turn."
            }))
        }
        "sols_asset" => match site {
            Some(site) => add_asset(st, site, args).await,
            None => return None,
        },
        "sols_create_site" => match site {
            None => create_site(st, thread_id, args).await,
            Some(_) => return None,
        },
        "sols_bee" => {
            let raw = args
                .get("tasks")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let briefs: Vec<super::bee::Brief> = raw
                .iter()
                .take(super::bee::MAX_TASKS)
                .filter_map(|t| {
                    let title = t.get("title").and_then(|v| v.as_str())?.trim().to_string();
                    let brief = t.get("brief").and_then(|v| v.as_str())?.trim().to_string();
                    if title.is_empty() || brief.chars().count() < 10 {
                        return None;
                    }
                    let effort = match t.get("effort").and_then(|v| v.as_str()) {
                        Some("quick") => "quick",
                        _ => "deep",
                    };
                    Some(super::bee::Brief {
                        title,
                        brief,
                        effort: effort.to_string(),
                    })
                })
                .collect();
            if briefs.is_empty() {
                return Some(Err(anyhow::anyhow!(
                    "sols_bee needs at least one task with a title and a brief of 10+ characters"
                )));
            }
            let reports = super::bee::run(st, site, thread_id, briefs).await;
            Ok(super::bee::as_json(&reports))
        }
        _ => return None,
    })
}

pub async fn execute_unbound(
    st: &StudioState,
    thread_id: &str,
    name: &str,
    args: &Value,
) -> Result<Value> {
    if let Some(r) = shared_tool(st, thread_id, None, name, args).await {
        return r;
    }
    bail!(
        "`{name}` needs a site, and this conversation has none yet. \
         Call sols_create_site first."
    )
}





/// A real channel sheet: identity from the rendered page, uploads from the
/// channel's RSS feed (the page itself is a JS shell that no longer ships a
/// stable video list).
pub async fn youtube(args: &Value) -> Result<Value> {
    let raw = args.get("channel").and_then(|v| v.as_str()).unwrap_or("").trim();
    if raw.is_empty() {
        bail!("channel is required, e.g. 9arm or a channel URL");
    }
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(8)
        .clamp(1, 15) as usize;
    let url = if raw.starts_with("http") {
        raw.to_string()
    } else {
        let h = raw.trim_start_matches('@');
        format!("https://www.youtube.com/@{h}/videos")
    };
    let dom = shot::dump_dom_at(&url, 1280, 1200).await?;

    let external = dom
        .split("\"externalId\":\"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .filter(|s| s.starts_with("UC"))
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("could not resolve a channel id for {url}"))?;

    let title = dom
        .split("\"channelMetadataRenderer\":{\"title\":\"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .unwrap_or("")
        .to_string();
    let avatar = dom
        .split("\"avatar\":{\"thumbnails\":[{\"url\":\"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .unwrap_or("")
        .to_string();
    let subs = dom
        .split('>')
        .zip(dom.split('>').skip(1))
        .map(|(a, _)| a.replace('<', "").trim().to_string())
        .find(|t| t.contains("subscriber") && t.chars().any(|c| c.is_ascii_digit()) && t.len() < 40)
        .map(|t| t.trim_end_matches("/span").trim().to_string())
        .unwrap_or_default();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("SiteBox-Sols/1.0")
        .build()?;
    let xml = client
        .get(format!(
            "https://www.youtube.com/feeds/videos.xml?channel_id={external}"
        ))
        .send()
        .await?
        .text()
        .await?;
    let mut videos = Vec::new();
    for entry in xml.split("<entry>").skip(1).take(limit) {
        let id = entry
            .split("<yt:videoId>")
            .nth(1)
            .and_then(|r| r.split("</yt:videoId>").next())
            .unwrap_or("");
        let t = entry
            .split("<title>")
            .nth(1)
            .and_then(|r| r.split("</title>").next())
            .unwrap_or("")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#39;", "'");
        let published = entry
            .split("<published>")
            .nth(1)
            .and_then(|r| r.split("</published>").next())
            .unwrap_or("");
        let views = entry
            .split("<media:statistics views=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap_or("");
        if id.is_empty() {
            continue;
        }
        videos.push(json!({
            "id": id,
            "title": t,
            "published": published,
            "views": views,
            "thumbnail": format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"),
            "url": format!("https://www.youtube.com/watch?v={id}")
        }));
    }
    if videos.is_empty() {
        bail!("the channel feed for {external} returned no videos");
    }
    Ok(json!({
        "ok": true,
        "channel": {"id": external, "title": title, "avatar": avatar, "subscribers": subs, "url": url},
        "videos": videos,
        "note": "Real channel data. Download the avatar and thumbnails with sols_asset, keep titles and view counts as written, and do not invent uploads."
    }))
}


/// The store front as data: prices arrive in satang, so they are converted once
/// here instead of by a model that may drop a zero.
pub async fn steam(args: &Value) -> Result<Value> {
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(8)
        .clamp(4, 10) as usize;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .user_agent("SiteBox-Sols/1.0")
        .build()?;
    let body: Value = client
        .get("https://store.steampowered.com/api/featuredcategories?l=thai&cc=TH")
        .send()
        .await?
        .json()
        .await?;
    let money = |c: &Value| -> String {
        let n = (c.as_i64().unwrap_or(0) as f64 / 100.0).round() as i64;
        let digits = n.to_string();
        let mut grouped = String::new();
        for (i, ch) in digits.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(ch);
        }
        let pretty: String = grouped.chars().rev().collect();
        format!("฿{pretty}")
    };
    let mut out = serde_json::Map::new();
    for (key, label) in [
        ("top_sellers", "ขายดี"),
        ("new_releases", "มาใหม่"),
        ("specials", "ลดราคา"),
    ] {
        let items: Vec<Value> = body
            .get(key)
            .and_then(|v| v.get("items"))
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .take(limit)
                    .filter_map(|g| {
                        let id = g.get("id").and_then(|v| v.as_u64())?;
                        let name = g.get("name").and_then(|v| v.as_str())?.to_string();
                        let img = g
                            .get("header_image")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let discount = g
                            .get("discount_percent")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let final_price = g.get("final_price").cloned().unwrap_or(json!(0));
                        let original = g.get("original_price").cloned().unwrap_or(json!(0));
                        Some(json!({
                            "id": id,
                            "name": name,
                            "discount_percent": discount,
                            "price": money(&final_price),
                            "original_price": money(&original),
                            "header_image": img,
                            "url": format!("https://store.steampowered.com/app/{id}/")
                        }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.insert(label.to_string(), json!(items));
    }
    Ok(json!({
        "ok": true,
        "categories": out,
        "note": "Real store data in THB. Download each header_image with sols_asset and keep names and prices as written."
    }))
}

async fn add_asset(st: &StudioState, site: &SiteRef, args: &Value) -> Result<Value> {
    // One call can carry a whole feed: a photo grid is ten files, and a tool
    // that needs ten rounds to do it never finishes inside a turn.
    let mut items: Vec<(String, String)> = Vec::new();
    if let Some(list) = args.get("items").and_then(|v| v.as_array()) {
        for it in list.iter().take(12) {
            let url = it.get("url").and_then(|v| v.as_str()).unwrap_or("").trim();
            let name = it.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if !url.is_empty() {
                items.push((url.to_string(), name.to_string()));
            }
        }
    }
    if items.is_empty() {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if url.is_empty() {
            bail!("`url` or `items` is required");
        }
        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
        items.push((url.to_string(), name.to_string()));
    }

    let mut assets = Vec::new();
    let mut errors = Vec::new();
    for (url, name) in &items {
        match fetch_one_asset(&st.root, site, url, name).await {
            Ok(v) => assets.push(v),
            Err(e) => errors.push(json!({"url": url, "error": format!("{e:#}")})),
        }
    }
    Ok(json!({
        "ok": errors.is_empty(),
        "assets": assets,
        "errors": errors,
        "next": "Reference each with its relative `url` and set width/height on the <img>.",
    }))
}

async fn fetch_one_asset(root: &std::path::Path, site: &SiteRef, raw: &str, wanted: &str) -> Result<Value> {
    let url = safe_url(root, raw)?;
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
    let ext = match ctype.split(';').next().unwrap_or("").trim() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        other => bail!("{url} is `{other}`, not a photo (jpeg/png/webp/gif)"),
    };
    let bytes = resp.bytes().await?;
    if bytes.len() > 3 * 1024 * 1024 {
        bail!("{url} is {} bytes; the asset limit is 3 MB", bytes.len());
    }

    let dir = site.dir.join("public/assets");
    std::fs::create_dir_all(&dir)?;
    let stem = asset_slug(wanted, raw);
    let mut path = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
        if n > 40 {
            bail!("too many files with that name");
        }
    }
    super::atomic_write(&path, &bytes)?;
    let (w, h) = image::image_dimensions(&path).unwrap_or((0, 0));
    let file = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    Ok(json!({
        "path": format!("public/assets/{file}"),
        "url": format!("/assets/{file}"),
        "bytes": bytes.len(),
        "width": w,
        "height": h,
        "content_type": ctype,
    }))
}

fn asset_slug(wanted: &str, url: &str) -> String {
    let source = if wanted.trim().is_empty() {
        url.rsplit('/')
            .next()
            .unwrap_or("image")
            .split('.')
            .next()
            .unwrap_or("image")
    } else {
        wanted
    };
    let mut s: String = source
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    while s.starts_with('-') || s.starts_with('_') {
        s.remove(0);
    }
    if s.is_empty() {
        s = format!("img-{}", &util::sha256_bytes(url.as_bytes())[..6]);
    }
    s.chars().take(48).collect()
}

pub async fn web_search(st: &StudioState, args: &Value) -> Result<Value> {
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
/// Internal hosts the operator explicitly allows, e.g. a reference site on the
/// LAN for a study-and-rebuild job. `SOLS_FETCH_ALLOW=127.0.0.1:4610,192.168.1.5`
/// keeps the SSRF guard intact while unblocking the one address the job needs.
fn fetch_allowed(root: &std::path::Path, u: &reqwest::Url) -> bool {
    let raw = config::env_or_file(root, "SOLS_FETCH_ALLOW").unwrap_or_default();
    let host = u.host_str().unwrap_or("");
    let port = u.port_or_known_default().unwrap_or(80);
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .any(|entry| {
            let (h, p) = match entry.rsplit_once(':') {
                Some((h, p)) => (h, p.parse::<u16>().ok()),
                None => (entry, None),
            };
            h.eq_ignore_ascii_case(host) && (p.is_none() || p == Some(port))
        })
}

fn safe_url(root: &std::path::Path, raw: &str) -> Result<reqwest::Url> {
    let u = reqwest::Url::parse(raw).with_context(|| format!("not a URL: {raw}"))?;
    if !matches!(u.scheme(), "http" | "https") {
        bail!("only http(s) URLs can be fetched");
    }
    if fetch_allowed(root, &u) {
        return Ok(u);
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

pub async fn web_fetch(st: &StudioState, args: &Value) -> Result<Value> {
    let raw = args
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if raw.is_empty() {
        bail!("url is required");
    }
    let url = safe_url(&st.root, raw)?;
    let max = args
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .unwrap_or(6000)
        .clamp(500, 20000) as usize;
    let want_render = args
        .get("render")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
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
    let is_html = ctype.contains("html");
    // Studying a design means reading its markup and its stylesheet, not the
    // prose between the tags. `raw` returns the source.
    let want_raw = args
        .get("raw")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut text = if is_html && !want_raw {
        html_to_text(&body)
    } else {
        body.to_string()
    };
    let mut rendered = false;

    // A JS app ships an empty shell; the static text is a few words of title.
    // Render it once instead of fetching the same page again and hoping.
    if is_html && (want_render || text.chars().count() < 400) {
        match shot::dump_dom(url.as_str()).await {
            Ok(dom) => {
                let page = html_to_text(&dom);
                if page.chars().count() > text.chars().count() {
                    text = page;
                    rendered = true;
                }
            }
            Err(e) if want_render => {
                return Ok(json!({
                    "ok": false,
                    "url": url.as_str(),
                    "error": format!("{e:#}"),
                    "note": "The static text is below; rendering needs Chromium on the host.",
                    "text": text.chars().take(max).collect::<String>(),
                }));
            }
            Err(_) => {}
        }
    }

    let total = text.chars().count();
    let clipped: String = text.chars().take(max).collect();
    Ok(json!({
        "ok": true,
        "url": url.as_str(),
        "chars": total,
        "truncated": total > max,
        "rendered": rendered,
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

/// Pull the probe's JSON out of the rendered DOM and undo the HTML escaping
/// the serializer applied to its text node.
fn extract_probe(dom: &str) -> Option<Value> {
    let start = dom.find("id=\"__sols_probe\"")?;
    let open = dom[start..].find('>')? + start + 1;
    let close = dom[open..].find("</pre>")? + open;
    let raw = &dom[open..close];
    let decoded = raw
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    serde_json::from_str(&decoded).ok()
}


/// Read-only site surface. A bee reaches the site through this seam and never
/// through the writer dispatcher, so a worker cannot patch, build or spawn.
/// Paths a tool result touched. The convention lives here, next to the tools
/// that produce it, so the turn loop does not walk JSON it does not own.
pub fn touched_paths(v: &Value, out: &mut std::collections::HashSet<String>) {
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                if k == "path" {
                    if let Some(p) = val.as_str() {
                        out.insert(p.trim_start_matches('/').to_string());
                    }
                } else {
                    touched_paths(val, out);
                }
            }
        }
        Value::Array(items) => {
            for it in items {
                touched_paths(it, out);
            }
        }
        _ => {}
    }
}

pub fn is_read_only(name: &str) -> bool {
    matches!(
        name,
        "sitebox_read"
            | "sitebox_grep"
            | "sitebox_ls"
            | "sitebox_outline"
            | "sitebox_check"
            | "sitebox_verify"
            | "sitebox_http"
            | "sitebox_probe"
            | "sitebox_shot"
    )
}

pub async fn read_only_site(
    st: &StudioState,
    site: &SiteRef,
    thread_id: &str,
    name: &str,
    args: &Value,
) -> Result<Value> {
    match name {
"sitebox_read" => {
            {
                let path_key = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let key = format!("{thread_id}:{path_key}");
                let mut reads = st.reads.lock().await;
                let n = reads.entry(key).or_insert(0);
                *n += 1;
                if *n > 2 {
                    bail!(
                        "you have already read `{path_key}` twice this turn. Use sitebox_grep to                          find the exact anchor and send a patch instead of reading it again."
                    );
                }
            }
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
"sitebox_grep" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if query.chars().count() < 3 {
                bail!("query must be at least 3 characters");
            }
            // Searching is cheap for the model and expensive for the turn: one
            // measured edit ran ten greps and then asked a question instead of
            // writing the patch.
            {
                let key = format!("{thread_id}:__grep");
                let mut r = st.reads.lock().await;
                let n = r.entry(key).or_insert(0);
                *n += 1;
                if *n > 10 {
                    bail!(
                        "you have run 10 searches this turn. Read the file you already \n                         found and write the patch; another search will not decide it."
                    );
                }
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
"sitebox_ls" => {
            let dir = args
                .get("dir")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim_matches('/');
            let max = args
                .get("max")
                .and_then(|v| v.as_u64())
                .unwrap_or(200)
                .clamp(1, 500) as usize;
            let base = if dir.is_empty() {
                site.dir.clone()
            } else {
                site_path(&site.dir, dir)?
            };
            if !base.is_dir() {
                bail!("{dir} is not a folder in this site");
            }
            let mut out: Vec<Value> = Vec::new();
            let mut stack = vec![(base, String::from(dir))];
            while let Some((folder, prefix)) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&folder) else {
                    continue;
                };
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name == "node_modules" || name == ".git" || name == "stage" {
                        continue;
                    }
                    let rel = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{prefix}/{name}")
                    };
                    let md = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    if md.is_dir() {
                        if out.len() < max {
                            stack.push((entry.path(), rel.clone()));
                            out.push(json!({"path": rel, "kind": "dir"}));
                        }
                    } else if out.len() < max {
                        out.push(json!({"path": rel, "kind": "file", "bytes": md.len()}));
                    }
                    if out.len() >= max {
                        break;
                    }
                }
                if out.len() >= max {
                    break;
                }
            }
            out.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
            Ok(
                json!({"ok": true, "dir": if dir.is_empty() { "." } else { dir }, "entries": out, "truncated": out.len() >= max}),
            )
        }
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
"sitebox_http" => {
            if !st.sup.is_running(&site.id).await {
                bail!(
                    "site {} is not running; start it before testing routes",
                    site.id
                );
            }
            let route = args.get("path").and_then(|v| v.as_str()).unwrap_or("/");
            if !route.starts_with('/') {
                bail!("path must start with /");
            }
            let method = args
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("GET")
                .to_uppercase();
            let url = format!("http://127.0.0.1:{}{route}", site.manifest.site.port);
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(90))
                .no_gzip()
                .build()?;
            let mut req = match method.as_str() {
                "GET" => client.get(&url),
                "POST" => client.post(&url),
                other => bail!("method {other} is not supported"),
            };
            if let Some(body) = args.get("body") {
                req = req.json(body);
            }
            let resp = req
                .send()
                .await
                .with_context(|| format!("{method} {route} failed"))?;
            let status = resp.status().as_u16();
            let ctype = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let text = resp.text().await.unwrap_or_default();
            let body: String = text.chars().take(4000).collect();
            Ok(json!({
                "ok": (200..400).contains(&status),
                "status": status,
                "content_type": ctype,
                "body": body,
                "truncated": text.chars().count() > 4000,
            }))
        }
"sitebox_probe" => {
            if !st.sup.is_running(&site.id).await {
                bail!(
                    "site {} is not running; start it before probing layout",
                    site.id
                );
            }
            // A build touches several pages; probing them one call at a time
            // costs a model round each, which is the expensive part.
            let mut routes: Vec<String> = args
                .get("routes")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .take(6)
                        .map(|r| r.trim().to_string())
                        .collect()
                })
                .unwrap_or_default();
            if routes.is_empty() {
                routes.push(
                    args.get("route")
                        .and_then(|v| v.as_str())
                        .unwrap_or("/")
                        .to_string(),
                );
            }
            let routes: Vec<String> = routes
                .into_iter()
                .map(|r| {
                    if r.starts_with('/') {
                        r
                    } else {
                        format!("/{r}")
                    }
                })
                .collect();
            let width = args
                .get("width")
                .and_then(|v| v.as_u64())
                .unwrap_or(375)
                .clamp(320, 1440) as u32;
            let height = args
                .get("height")
                .and_then(|v| v.as_u64())
                .unwrap_or(900)
                .clamp(480, 2000) as u32;
            let port = site.manifest.site.port;
            let urls: Vec<String> = routes
                .iter()
                .map(|r| {
                    let join = if r.contains('?') { '&' } else { '?' };
                    format!("http://127.0.0.1:{port}{r}{join}sols_probe=1")
                })
                .collect();
            let doms = futures_util::future::join_all(
                urls.iter().map(|u| shot::dump_dom_at(u, width, height)),
            )
            .await;
            let mut pages = Vec::new();
            let mut first: Option<Value> = None;
            for (i, dom) in doms.into_iter().enumerate() {
                let found = match dom {
                    Ok(d) => match extract_probe(&d) {
                        Some(f) => f,
                        None => {
                            pages.push(json!({
                                "route": routes[i],
                                "url": urls[i],
                                "error": "the probe script did not run; is this page served by SiteBox?"
                            }));
                            continue;
                        }
                    },
                    Err(e) => {
                        pages.push(json!({
                            "route": routes[i],
                            "url": urls[i],
                            "error": format!("{e:#}")
                        }));
                        continue;
                    }
                };
                let page = json!({
                    "route": routes[i],
                    "url": urls[i],
                    "viewport": found.get("viewport"),
                    "doc_width": found.get("doc_width"),
                    "overflow": found.get("overflow").cloned().unwrap_or(json!([])),
                    "clipped": found.get("clipped").cloned().unwrap_or(json!([])),
                    "small_text": found.get("small_text").cloned().unwrap_or(json!([])),
                });
                if first.is_none() {
                    first = Some(page.clone());
                }
                pages.push(page);
            }
            let clean = pages.iter().all(|p| {
                p.get("error").is_none()
                    && p.get("overflow").and_then(|v| v.as_array()).map(|a| a.is_empty()).unwrap_or(true)
                    && p.get("clipped").and_then(|v| v.as_array()).map(|a| a.is_empty()).unwrap_or(true)
                    && p.get("small_text").and_then(|v| v.as_array()).map(|a| a.is_empty()).unwrap_or(true)
            });
            let mut out = first.unwrap_or_else(|| json!({}));
            if let Some(o) = out.as_object_mut() {
                o.insert("ok".into(), json!(clean));
                o.insert("pages".into(), json!(pages));
                o.insert("note".into(), json!("One call probed every route. Anything in `overflow` or `clipped` is a visible bug at this width; `small_text` lists text under 12px."));
            }
            Ok(out)
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
        other => bail!("`{other}` is not a read-only site tool"),
    }
}

pub async fn execute(
    st: &StudioState,
    site: &SiteRef,
    thread_id: &str,
    name: &str,
    args: &Value,
    auto_apply: bool,
) -> Result<Value> {
    if let Some(r) = shared_tool(st, thread_id, Some(site), name, args).await {
        return r;
    }
    if is_read_only(name) {
        return read_only_site(st, site, thread_id, name, args).await;
    }
    match name {
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
                        "sols_import" => {
            let from = args
                .get("from")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim_start_matches('/');
            if from.is_empty() || from.contains("..") {
                bail!("`from` must be a path inside imports/");
            }
            let src = crate::util::safe_join(&st.root.join("imports"), from)
                .ok_or_else(|| anyhow::anyhow!("bad path: {from}"))?;
            if !src.is_file() {
                bail!("imports/{from} does not exist; ask the operator to drop it there");
            }
            let size = std::fs::metadata(&src)?.len();
            if size > 20 * 1024 * 1024 {
                bail!("imports/{from} is {size} bytes; the limit is 20 MB");
            }
            let name = src
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let to = args
                .get("to")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("assets-raw/{name}"));
            let dest = site_path(&site.dir, &to)?;
            if let Some(parent) = dest.parent() {
                super::ensure_dir(parent)?;
            }
            std::fs::copy(&src, &dest)?;
            Ok(json!({
                "ok": true,
                "path": to,
                "bytes": size,
                "next": "If this is a source image, add an images pipeline step in sitebox.toml and run sitebox_build.",
            }))
        }
        "sitebox_build" => {
            let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
            let steps = crate::pipeline::apply(&site.manifest, &st.root, force, None)?;
            let failed: Vec<String> = steps
                .iter()
                .filter(|s| s.status == "fail" || s.status == "blocked")
                .map(|s| format!("{} ({})", s.name, s.status))
                .collect();
            Ok(json!({
                "ok": failed.is_empty(),
                "steps": steps.iter().map(|s| json!({"name": s.name, "status": s.status, "ms": s.ms, "detail": s.detail})).collect::<Vec<_>>(),
                "failed": failed,
            }))
        }
                                                        "sols_rename_site" => rename_site(st, site, args).await,
        "sitebox_new_page" => new_page(st, site, thread_id, args, auto_apply).await,
        "sitebox_propose_patch" => propose(st, site, thread_id, args, auto_apply).await,
        other => bail!("unknown tool `{other}`"),
    }
}


/// HTML-escape model text before it becomes markup.
fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn replace_between(text: &str, open: &str, close: &str, with: &str) -> String {
    match (text.find(open), text.find(close)) {
        (Some(a), Some(b)) if b > a => format!(
            "{}{}{}",
            &text[..a + open.len()],
            with,
            &text[b..]
        ),
        _ => text.to_string(),
    }
}

/// Build a page from an existing page's shell and the content the model sent.
/// Pure so it can be tested without a site.
fn render_page(
    shell: &str,
    title: &str,
    description: &str,
    sections: &[(String, String, Vec<String>)],
    back: Option<(String, String)>,
) -> Result<String> {
    let body_at = shell
        .find("<body")
        .ok_or_else(|| anyhow::anyhow!("the template has no <body>"))?;
    let header_end = shell[body_at..]
        .find("</header>")
        .map(|i| body_at + i + "</header>".len())
        .or_else(|| shell[body_at..].find('>').map(|i| body_at + i + 1))
        .unwrap_or(body_at);
    let mut prefix = shell[..header_end].to_string();
    if !title.trim().is_empty() {
        prefix = replace_between(
            &prefix,
            "<title>",
            "</title>",
            &esc_html(title),
        );
    }
    if !description.trim().is_empty()
        && prefix.contains("name=\"description\"")
    {
        // Cheap attribute replace: the generator only needs the new sentence.
        if let Some(start) = prefix.find("name=\"description\"") {
            if let Some(cstart) = prefix[start..].find("content=\"") {
                let abs = start + cstart + "content=\"".len();
                if let Some(cend) = prefix[abs..].find('"') {
                    prefix = format!(
                        "{}{}{}",
                        &prefix[..abs],
                        esc_html(description),
                        &prefix[abs + cend..]
                    );
                }
            }
        }
    }

    let main_at = shell[header_end..].find("<main").map(|i| header_end + i);
    let main_open = main_at
        .and_then(|a| shell[a..].find('>').map(|j| shell[a..a + j + 1].to_string()))
        .unwrap_or_else(|| "<main>".to_string());

    let footer_at = shell.rfind("<footer").or_else(|| shell.rfind("</main>"));
    let suffix = footer_at
        .map(|i| shell[i..].to_string())
        .unwrap_or_else(|| "</main>\n</body>\n</html>\n".to_string());
    let suffix = suffix
        .strip_prefix("</main>")
        .map(|s| s.to_string())
        .unwrap_or(suffix);

    let mut out = String::with_capacity(shell.len());
    out.push_str(&prefix);
    out.push('\n');
    out.push_str(&main_open);
    out.push('\n');
    out.push_str(&format!("<h1>{}</h1>\n", esc_html(title)));
    for (heading, body, items) in sections {
        out.push_str("<section class=\"page-section\">\n");
        out.push_str(&format!("  <h2>{}</h2>\n", esc_html(heading)));
        out.push_str(&format!("  <p>{}</p>\n", esc_html(body)));
        if !items.is_empty() {
            out.push_str("  <ul>\n");
            for it in items {
                out.push_str(&format!("    <li>{}</li>\n", esc_html(it)));
            }
            out.push_str("  </ul>\n");
        }
        out.push_str("</section>\n");
    }
    if let Some((href, text)) = back {
        out.push_str(&format!(
            "<p class=\"back\"><a href=\"{}\">{}</a></p>\n",
            esc_html(&href),
            esc_html(&text)
        ));
    }
    out.push_str("</main>\n");
    out.push_str(&suffix);
    Ok(out)
}

async fn new_page(
    st: &StudioState,
    site: &SiteRef,
    thread_id: &str,
    args: &Value,
    auto_apply: bool,
) -> Result<Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if !path.ends_with(".html") {
        bail!("path must end in .html");
    }
    let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("").trim();
    if title.is_empty() {
        bail!("title is required");
    }
    if site_path(&site.dir, path)?.exists() {
        bail!("{path} already exists; use sitebox_propose_patch to edit it");
    }
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let sections: Vec<(String, String, Vec<String>)> = args
        .get("sections")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .take(12)
                .filter_map(|s| {
                    let heading = s.get("heading").and_then(|v| v.as_str())?.trim().to_string();
                    let body = s.get("body").and_then(|v| v.as_str())?.trim().to_string();
                    if heading.is_empty() || body.is_empty() {
                        return None;
                    }
                    let items = s
                        .get("items")
                        .and_then(|v| v.as_array())
                        .map(|l| {
                            l.iter()
                                .take(12)
                                .filter_map(|i| i.as_str().map(|x| x.trim().to_string()))
                                .filter(|x| !x.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();
                    Some((heading, body, items))
                })
                .collect()
        })
        .unwrap_or_default();
    if sections.is_empty() {
        bail!("sections need at least one heading + body");
    }
    let back = args.get("back").and_then(|b| {
        let href = b.get("href").and_then(|v| v.as_str())?.trim();
        let text = b.get("text").and_then(|v| v.as_str())?.trim();
        if href.is_empty() || text.is_empty() {
            None
        } else {
            Some((href.to_string(), text.to_string()))
        }
    });

    // Template: explicit, else index.html beside the target, else public/index.html.
    let target_dir = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut candidates: Vec<String> = Vec::new();
    if let Some(t) = args.get("template").and_then(|v| v.as_str()) {
        candidates.push(t.trim().to_string());
    }
    if !target_dir.is_empty() {
        candidates.push(format!("{target_dir}/index.html"));
    }
    candidates.push("public/index.html".to_string());
    let mut shell = String::new();
    let mut template = String::new();
    for c in &candidates {
        if c.is_empty() || c == path {
            continue;
        }
        if let Ok(p) = site_path(&site.dir, c) {
            if p.is_file() {
                if let Ok(t) = std::fs::read_to_string(&p) {
                    if t.contains("<body") {
                        shell = t;
                        template = c.clone();
                        break;
                    }
                }
            }
        }
    }
    if shell.is_empty() {
        bail!("no template page found; create one first or pass `template`");
    }

    let html = render_page(&shell, title, description, &sections, back)?;
    if html.len() > 120 * 1024 {
        bail!("the generated page is {} bytes; keep it under 120 KB", html.len());
    }
    let reason = format!("new page from template {template}");
    let res = propose(
        st,
        site,
        thread_id,
        &json!({"path": path, "create": true, "old": "", "new": html, "reason": reason}),
        auto_apply,
    )
    .await?;
    Ok(json!({
        "ok": res.get("ok").cloned().unwrap_or(json!(true)),
        "template": template,
        "bytes": html.len(),
        "result": res,
        "next": "The page exists with the site shell. Patch its classes, imagery and copy details.",
    }))
}

async fn propose(
    st: &StudioState,
    site: &SiteRef,
    thread_id: &str,
    args: &Value,
    auto_apply: bool,
) -> Result<Value> {
    // A whole-page edit is eight small replacements; one call carries them all
    // so the round budget goes to thinking, not to bookkeeping.
    if let Some(list) = args.get("patches").and_then(|v| v.as_array()) {
        if list.is_empty() {
            bail!("`patches` is empty");
        }
        let mut done = Vec::new();
        let mut errors = Vec::new();
        for (i, item) in list.iter().take(12).enumerate() {
            match propose_one(st, site, thread_id, item, auto_apply).await {
                Ok(v) => done.push(v),
                Err(e) => errors.push(json!({"index": i, "error": format!("{e:#}")})),
            }
        }
        return Ok(json!({
            "ok": errors.is_empty(),
            "patches": done,
            "errors": errors,
            "next": if errors.is_empty() { "All patches are in place." } else { "Fix the failed entries and send only those again." },
        }));
    }
    propose_one(st, site, thread_id, args, auto_apply).await
}

async fn propose_one(
    st: &StudioState,
    site: &SiteRef,
    thread_id: &str,
    args: &Value,
    auto_apply: bool,
) -> Result<Value> {
    let wants_path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let target_exists = site_path(&site.dir, wants_path)
        .map(|p| p.exists())
        .unwrap_or(false);
    let explicit_create = args
        .get("create")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let old_text = args.get("old").and_then(|v| v.as_str()).unwrap_or("");
    let spec = PatchSpec {
        create: explicit_create || (!target_exists && old_text.is_empty()),
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
        count: if args.get("count").is_some() {
            args.get("count").and_then(|v| v.as_u64()).unwrap_or(1) as usize
        } else if !target_exists && old_text.is_empty() {
            0
        } else {
            1
        },
        reason: args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };
    super::patch::validate(&spec, &site.kind)?;

    let target = site_path(&site.dir, &spec.path)?;
    let before = if spec.create {
        if target.exists() {
            bail!("{} already exists; patch it instead", spec.path);
        }
        String::new()
    } else {
        let size = std::fs::metadata(&target)
            .with_context(|| {
                format!(
                    "{} does not exist (pass `create` to make a new file)",
                    spec.path
                )
            })?
            .len();
        if size > super::patch::MAX_FILE_BYTES {
            bail!(
                "{} is {size} bytes; the patch limit is {}",
                spec.path,
                super::patch::MAX_FILE_BYTES
            );
        }
        std::fs::read_to_string(&target)
            .map_err(|_| anyhow::anyhow!("{} is not UTF-8 text", spec.path))?
    };
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
        create: spec.create,
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
2. Read `sitebox-playbook` first — it is the operator's taste and the defects that keep coming back. Then `sitebox-design` before you choose anything visual, and `sitebox-create` when the site is more than one page. Three skills at most; never read the same one twice.
3. Before you choose a look, call `sols_design_index`. Pick five axes (archetype, palette, type pairing, layout, signature element) that no recent site used, and record them with `sols_design`. Two sites that look alike is a defect unless the brief asks for it.
4. Call `sols_plan` with 3-5 steps before the first action when the job has several parts, then update it as steps move between pending, doing and done. Every clause in the request is a deliverable: finish them all, and do not end with a question the site can answer.
4. Ask only what you cannot infer: what the site is for, who reads it, and what content is real. Do not interview the operator when the request is already clear: make the reasonable choice, say it in one line, and build. Choose `id` and `name` yourself and say what you chose. When the operator does not name it: short, international, brandable — Latin letters, one or two words that sound like a product (Drift, Halcyon, Relay, Northwind). Never a Thai common noun as the display name, even when the copy is Thai; `id` is lowercase latin with hyphens.
5. `sols_create_site` writes the folder, manifest, starter page and registry entry, picks a free port, starts the site and binds this conversation to it. It already starts the site: do not call `sitebox_site_start` afterwards. A static site serves files from disk, so a patch needs no restart either.
6. The starter is a blank canvas, not a finished site: read it, then improve it with `sitebox_propose_patch`. For each additional page use `sitebox_new_page` (title + sections) — it copies the starter's shell and writes the page, so you only send content. Photography: `sols_asset` downloads a photo into `public/assets` while you are creating (prefer `https://picsum.photos/seed/<word>/800/800` for placeholder shots). Never hotlink an image; always set width/height on `<img>`. While you are creating the site, your patches are applied immediately (snapshot, build and checks still run); after this turn they will wait for the operator's Apply.

## Skills (bundled, read on demand)
{skill_index}

Routing: taste, copy rules, recurring defects -> sitebox-playbook (new site: start here); look, layout, tokens -> sitebox-design; real content, numbers, sources -> sitebox-data; build or change a page -> sitebox-create; prove it -> sitebox-verify; register, port, start/stop, logs -> sitebox-config; manifest, pipeline, engine commands -> sitebox-engine. `sols_skill` reads a file; `sols_skills` lists every file.

## Rules that do not change
You cannot write files except through `sols_create_site` (once, before a site exists) and `sitebox_propose_patch`. During creation a patch is applied as soon as the checks pass, so you may say it is live — only after the tool says `applied`. After the creation turn, patches are queued and you must not call them live. Verify with `sitebox_check` and `sitebox_verify` before you claim anything.

## Big jobs: send bees
You are the only writer; bees are your readers. `sols_bee` runs 1-4 workers at once and returns when they all finish.
- Send bees when a job needs three or more independent lookups, or covers several pages at once (a clone, a multi-page build, a fact sweep). A single edit does not need one.
- Each brief must stand alone: a bee cannot see this conversation. Say what to find or write, the shape of the answer, the language, and what counts as done.
- Good splits: research the reference product and return its screens, flows and copy tone with URLs; draft the copy for sections A/B/C; propose the page structure and tokens from the skills.
- The reports arrive as one tool result. Fold them into your own plan and patches; never paste a bee report to the operator as your answer.
- **Two waves beat one.** Wave 1 researches (facts, structure, copy). Then send wave 2 with the wave-1 findings quoted in the briefs and an explicit job: find contradictions, check every number against its source, or attack the copy. Wave 2 is how bees disagree without talking to each other — put the criticism in the brief, not in a request to "discuss".
- Mark mechanical work `effort: "quick"` (scanning files, checking numbers, listing tokens); keep research and design `deep`.
- Bees in the same wave share a scratchpad (`bee_note` / `bee_notes`); `sols_board` reads it afterwards. It is a place to avoid duplicate work, not a conversation: a bee cannot ask another a question.

## Big jobs: send bees
You are the only writer; bees are your readers. `sols_bee` runs 1-4 workers at once and returns when they all finish.
- Send bees when a job needs three or more independent lookups, or covers several pages at once (a clone, a multi-page build, a fact sweep). A single edit does not need one.
- Each brief must stand alone: a bee cannot see this conversation. Say what to find or write, the shape of the answer, the language, and what counts as done.
- Good splits: research the reference product and return its screens, flows and copy tone with URLs; draft the copy for sections A/B/C; propose the page structure and tokens from the skills.
- The reports arrive as one tool result. Fold them into your own plan and patches; never paste a bee report to the operator as your answer.

## Looking things up
- `sols_search` is a real web search through SearXNG. Use it for facts you cannot read from the site: a company, product, place, person, price, domain, or a current event.
- Cite the result URL you used. Snippets are search results, not verified facts; if the search returns nothing, say so in one line instead of guessing.
- `sols_fetch` opens one result when its snippet is not enough. Only fetch a URL the request is about or a search returned.
- You do have search. Never tell the operator you have no search tool.
- Before you call a page finished, run `sitebox_probe` at width 500 (the browser minimum) with every route you changed in `routes` — one call, not one per page. Anything it reports as wider than the viewport, clipped, or under 12px is a bug: fix it and probe again.
- Every tool call costs a model round. Batch reads, grep and probes; probe once after a batch of patches, not after each patch.
- Research budget: at most 3 searches and 4 fetches before you act. A JS app returns a shell; fetch it once with \`render\`: true instead of touring ten URLs.
- When the operator asks for a site that emulates a product, build the product's own surface (the chat panel, the character grid, the flow a user would touch), never an article about it. Use a new brand name and your own copy; never reuse the reference's name, logo or screenshots.


## Language
Answer in the language the operator used, and only that language. Never mix scripts: a Chinese or Japanese character inside a Thai or English sentence is a bug, not a style.

## Editing
Send a whole change as one call with `patches` (up to 12 exact replacements) instead of one call per line. Prefer a few larger anchors over many tiny ones.

## Photography
A photo-sharing page, portfolio or product page needs photographs, not gradients. Use `sols_asset` to download one (picsum.photos seeds are fine for placeholders), reference it with the returned `/assets/...` url, and set width/height on every `<img>`. Never hotlink an image from another site.

## Real functionality
A site that has to think, remember or call an API is a node site, not a mockup.
- Write the UI under `public/`, then `server.js` at the site root and patch `sitebox.toml` to `serve = "node"`. Node's built-in modules only: no `npm install` at runtime.
- The gateway comes from the environment: `AI_BASE_URL`, `AI_MODEL`, `AI_API_KEY`. Read them in `server.js` with `process.env`. Never put a key in `public/` or in the page; the browser only talks to your `/api/` routes.
- Listen on `0.0.0.0`, not `127.0.0.1`: the checks only see localhost, but the operator opens the site from another device on the LAN.
- Keep state (threads, profiles, memory) in a JSON file under the site folder such as `data/store.json`, not under `public/`.
- Some gateways need extra request headers. SiteBox passes them to node servers as `AI_HEADERS`, a JSON object; send them on every provider request.
- Files the operator drops in `<root>/imports/` come in with `sols_import`; then a pipeline step (`sitebox_build`) can resize or convert them. After you edit the build script itself, call `sitebox_build` with `force: true` — the script is not one of the step's declared inputs. Learn the manifest shape from the `sitebox-engine` skill.
- `sitebox_outline` lists source files only; use `sitebox_ls` to see written assets (public/assets) or anything else on disk.
- After a restart, call the route with `sitebox_http` and read the body. A 200 with `{{"error":...}}` or `live:false` is a bug, not success.
- If the provider call fails, log the status and body and return the real error (a `live: false` with a canned reply hides the bug). Never fall back silently.
- A file that does not exist yet is a creating patch: pass `create: true` and the whole content in `new`.

## Deciding versus asking
- When the operator says "do it", "all of it", "go ahead" or repeats a request, act. Do not ask again and do not restate the question.
- Ask at most one question per turn, and only when a wrong guess would waste the work. Anything else: choose the sensible default, say it in one line, and build.
- Never answer with only a question when part of the request is already actionable.
- Do not re-read a region you read earlier in the same turn; the transcript already holds it.

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
   A request with several clauses is several deliverables: put each in `sols_plan`, finish all of them this turn, and do not end with a question the site can answer. Skip the plan for a one-line edit.
   Fix only what the request names. A defect you notice elsewhere is one sentence at the end, not a patch; if the request names a path, that file must exist when you finish.
   A message with numbered items is a checklist: put each item in `sols_plan` and close the turn only when every item is done or you have said why it cannot be. Do not answer a checklist with a health report.
5. If the operator renames the app, call `sols_rename_site`: it updates the manifest and the dashboard card. A `<title>` edit alone leaves SiteBox showing the old name.
6. Verify with `sitebox_check`, and `sitebox_verify` when the site is running, before you claim anything.
7. Close with what changed, what the checks said, and one next step.

## Attached images
A message can carry up to four images (a screenshot of the target, a reference design, a photo). Look at them before planning: they are evidence of what the operator wants, not decoration. If a screenshot shows a layout, match its structure and spacing; if it shows content, use that content. Say in one line what you saw, then work.

## Where the operator is
{where_}

## Skills (bundled, read on demand)
{skill_index}

Routing: taste, copy rules, recurring defects → sitebox-playbook (read this first on a new site) · look, layout, tokens → sitebox-design · real content, numbers, sources → sitebox-data · build or change a page → sitebox-create · prove it → sitebox-verify · register, port, start/stop, logs → sitebox-config · manifest, pipeline, engine commands → sitebox-engine · the skills themselves → sitebox-skill-maintainer. `sols_skill` reads the file; `sols_skills` lists every file including references.

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
0. A **new page** starts with `sitebox_new_page`: pass the path, title and sections with real copy. It copies the site's head, header/nav and footer, writes the file, and runs the checks; then patch the classes and imagery. Do not write a whole page as a create patch.
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

## Real functionality
A site that has to think, remember or call an API is a node site, not a mockup.
- Write the UI under `public/`, then `server.js` at the site root and patch `sitebox.toml` to `serve = "node"`. Node's built-in modules only: no `npm install` at runtime.
- The gateway comes from the environment: `AI_BASE_URL`, `AI_MODEL`, `AI_API_KEY`. Read them in `server.js` with `process.env`. Never put a key in `public/` or in the page; the browser only talks to your `/api/` routes.
- Keep state (threads, profiles, memory) in a JSON file under the site folder such as `data/store.json`, not under `public/`.
- A file that does not exist yet is a creating patch: pass `create: true` and the whole content in `new`.

## Deciding versus asking
- When the operator says "do it", "all of it", "go ahead" or repeats a request, act. Do not ask again and do not restate the question.
- Ask at most one question per turn, and only when a wrong guess would waste the work. Anything else: choose the sensible default, say it in one line, and build.
- Never answer with only a question when part of the request is already actionable.
- Do not re-read a region you read earlier in the same turn; the transcript already holds it.

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

#[cfg(test)]
mod asset_tests {
    use super::asset_slug;

    #[test]
    fn makes_safe_file_stems() {
        assert_eq!(asset_slug("feed-01", "https://x/i.jpg"), "feed-01");
        assert_eq!(
            asset_slug("", "https://cdn.example/photo%20one.JPG"),
            "photo20one"
        );
        assert_eq!(asset_slug("../../etc/passwd", "https://x/"), "etcpasswd");
        assert!(asset_slug("", "https://x/").starts_with("img-"));
    }
}

#[cfg(test)]
mod probe_tests {
    use super::extract_probe;

    #[test]
    fn reads_the_probe_payload_out_of_a_dom() {
        let dom = r#"<html><body><p>x</p><pre id="__sols_probe">{"viewport":500,"clipped":[],"small_text":[{"el":"p","px":11.5}]}</pre></body></html>"#;
        let v = extract_probe(dom).expect("payload");
        assert_eq!(v["viewport"], 500);
        assert_eq!(v["small_text"][0]["px"], 11.5);

        // The serializer escapes quotes inside the text node.
        let escaped = r#"<pre id="__sols_probe">{&quot;viewport&quot;:375}</pre>"#;
        assert_eq!(extract_probe(escaped).unwrap()["viewport"], 375);
        assert!(extract_probe("<html></html>").is_none());
    }
}


#[cfg(test)]
mod page_tests {
    use super::*;

    const SHELL: &str = r#"<!doctype html><html lang="th"><head><title>old</title>
<meta name="description" content="old desc"><link rel="stylesheet" href="style.css"></head>
<body><header><nav>menu</nav></header><main class="wrap"><p>body</p></main>
<footer>foot</footer></body></html>"#;

    #[test]
    fn page_inherits_the_shell_and_fills_sections() {
        let sections = vec![(
            "หัวข้อหนึ่ง".to_string(),
            "เนื้อความ".to_string(),
            vec!["ข้อ ก".to_string()],
        )];
        let out = render_page(SHELL, "หน้าใหม่", "คำอธิบายใหม่", &sections, None).unwrap();
        assert!(out.contains("<title>หน้าใหม่</title>"), "{out}");
        assert!(out.contains("content=\"คำอธิบายใหม่\""), "{out}");
        assert!(out.contains("<header><nav>menu</nav></header>"), "{out}");
        assert!(out.contains("<h1>หน้าใหม่</h1>"), "{out}");
        assert!(out.contains("<h2>หัวข้อหนึ่ง</h2>"), "{out}");
        assert!(out.contains("<li>ข้อ ก</li>"), "{out}");
        assert!(out.contains("<footer>foot</footer>"), "{out}");
        assert_eq!(out.matches("<main").count(), 1, "{out}");
        assert_eq!(out.matches("</main>").count(), 1, "{out}");
    }

    #[test]
    fn page_escapes_model_text() {
        let sections = vec![("a<b".to_string(), "x & y".to_string(), vec![])];
        let out = render_page(SHELL, "t", "", &sections, Some(("<x>".into(), "กลับ".into()))).unwrap();
        assert!(out.contains("a&lt;b"), "{out}");
        assert!(out.contains("x &amp; y"), "{out}");
        assert!(out.contains("href=\"&lt;x&gt;\""), "{out}");
    }
}
