# SiteBox

[อ่านภาษาไทย](README.th.md) · English below

A dashboard and a Rust engine that serve your static sites from one process —
and tell you when one is broken.

One binary, one `sitebox.toml` per site, no per-site `node_modules`.

## Quick start

```bash
# Docker (recommended — survives reboots)
docker compose up -d
# http://localhost:4445

# Or from source
cd engine && cargo run --release --bin siteboxd -- \
  --root .. --port 4445 --ui ../dashboard/public
```

The two example sites are auto-detected from their manifests on the first API
request. Start them from the dashboard, or:

```bash
curl -X POST http://localhost:4445/api/sites/example-notes/start
curl -X POST http://localhost:4445/api/sites/example-clock/start
```

## Architecture

```
sites/<id>/
├── sitebox.toml      ← the contract: port, name, serve mode, pipeline, checks
└── public/           ← what the browser gets

engine/               ← one process owns serving, supervision and checks
├── src/serve.rs      ← shared static server (guard, ETag/304, gzip, Range, 404)
├── src/supervisor.rs ← in-process listeners + node children + JSONL logs
├── src/checks.rs     ← SB0xx static / SB1xx HTTP catalog + negative controls
├── src/manifest.rs   ← sitebox.toml + JSON Schema
├── src/assets.rs     ← downscale-only WebP pipeline + dims.json
├── src/pipeline.rs   ← ordered steps, fingerprints, scaffold
├── src/deploy.rs     ← mirror + sha256 verification
├── src/dashboard.rs  ← the API the UI talks to
├── src/studio/       ← Sols: agent loop, tools, patches, snapshot, bees, skills
├── assets/sols-widget.js ← launcher injected into every HTML page the engine serves
└── src/bin/          ← sitebox (CLI), siteboxd (dashboard), sitebox-mcp (MCP)

dashboard/public/     ← the dashboard UI, Sols panel included
dashboard/data/       ← runtime registry, site metadata and Sols state
```

Static sites are served in-process: one listener task per site, no child Node.
A site that genuinely owns an HTTP API declares `serve = "node"` and keeps its
own `server.js`; the supervisor spawns it, verifies the port and persists its
logs. `serve = "proxy"` serves static files and forwards `/api/*` to an
upstream.

## Features

- **One shared server** — traversal guard, ETag/304, gzip, Range and security
  headers for every engine-served site, with a real 404
- **The manifest is the config** — `sitebox.toml` is machine-readable and has a
  JSON Schema (`sitebox schema`)
- **Done means an id passed** — a check catalog with stable ids (`SB001`–`SB107`)
  and a `self-test` that proves each guard can fail
- **Verified starts** — `ok:true` only when the port is actually listening
- **Persisted logs** — 500-line ring in memory plus `logs/<id>.jsonl` on disk
- **Pipelines with fingerprints** — steps run in order; an unchanged step is
  skipped, a changed command or input re-runs it
- **Images are encoded once** — downscale-only WebP; `dims.json` is written from
  the files that shipped, and nothing is cropped
- **Deploy by mirroring** — copy, delete extraneous files, then compare sha256
- **MCP server** — 15 typed tools for agents (`sitebox_list`, `check`, `verify`,
  `build`, `up`, …)
- **Sols, the editing agent** — a docked panel in the dashboard and on every
  engine-served page. It proposes anchored patches, stages them in a copy, runs
  the checks, applies with a snapshot and undoes. Bundled SiteBox skills,
  read-only Bee workers, web search/fetch, screenshots, site creation and
  image attachments.
- **Auto-detect** — drop a folder with a `sitebox.toml` (or legacy `server.js`)
  into `sites/`
- Light/dark theme, custom icon colours, auto-detected favicons, search/filter

## Sols — the editing agent

Sols edits the sites this dashboard serves, from a chat panel. Open it from a
site card, or from the **Sols** launcher on an engine-served page. You describe
the change; it reads the site's skills, proposes patches, runs the checks and
shows a screenshot.

### Enabling it

```bash
export STUDIO_TOKEN=$(openssl rand -hex 16)   # optional: lets scripts send x-studio-token
export CHROME=/path/to/chrome                # optional: screenshots and probes
export SEARXNG_URL=http://127.0.0.1:8899     # optional: web search
export SOLS_FETCH_ALLOW=127.0.0.1:4610       # optional: fetch a LAN host
export SOLS_WIDGET=0                          # optional: hide the site launcher
```

The model endpoint, model name and API key live in **Studio settings** in the
panel. The key is write-only from the browser: the API reports `has_key`, never
the value, and the file is written `0600`.

### What it can do

| Tool | Purpose |
|------|---------|
| `sitebox_propose_patch` | Exact-substring edit (or `create`) with `count`; staged, checked, queued or applied |
| `sitebox_new_page` | New page from the site's shell + sections |
| `sitebox_probe` | Render up to 6 routes at one width (default 375, max 1440) and report overflow/clipped/small text |
| `sitebox_shot` · `sitebox_check` · `sitebox_verify` | Screenshot, static checks, HTTP checks |
| `sols_create_site` · `sols_rename_site` | Create a site from the conversation; rename manifest + dashboard card |
| `sols_bee` | 1–4 read-only workers with their own briefs and a shared board |
| `sols_search` · `sols_fetch` · `sols_youtube` · `sols_steam` · `sols_asset` | Real data and images |
| `sols_plan` · `sols_design` · `sols_import` | Plan a build, per-site design memory, bring in an existing page |
| `sols_skill` · `sols_skills` | Bundled SiteBox doctrine (including `sitebox-playbook`), read on demand |

### Images

A message can carry up to four images. The paperclip opens the file picker;
pasting from the clipboard attaches directly. Each image may be 8 MB and must be
png, jpeg, webp or gif — the extension is checked against the magic bytes.

Images are stored under `dashboard/data/studio/uploads/<thread>/` and sent to the
model as image parts next to the text. If the configured model cannot read
images, the turn is refused before anything is written: `400 vision_unsupported`
with the model name, or `503 vision_unknown` when the endpoint cannot be probed.
The answer is cached per endpoint + model in `vision.json`.

### Safety invariants

- The model cannot write files, run commands or read the filesystem directly.
  A patch is an anchored `old`/`new` pair (or a create) with an expected
  occurrence count; every path resolves inside one site folder.
- Every patch is staged in a hard-linked copy and the static checks run there.
  Only checks that *the patch introduced* count, so a site that was already red
  does not block edits.
- Apply snapshots first (content-addressed, 20 kept) and rolls back when a new
  check fails. Undo restores the last snapshot.
- HTTP is read-only search/fetch behind an SSRF guard; `SOLS_FETCH_ALLOW` opens
  named hosts. There is no shell tool.
- A bee sees a leaf toolbox (read/grep/outline/check/http/probe/shot) and cannot
  patch, build or spawn.

### Turn model and limits

- Editing turns **propose** patches for the operator to apply, unless the thread
  is switched to **All-in** (`POST /api/studio/threads/:id/mode`), which applies
  each patch as it passes. A turn that creates a site always applies its own.
- One turn per **site**; different sites run in parallel. Conversations on the
  same site serialize because they write the same files.
- 16 rounds per edit (`max_rounds` in settings), 26 while creating, capped at 32.
- Patch payload 200 KB, file ceiling 2 MB, 12 patches per call.
- Message 8,000 characters; 60 turns per site per hour; a generated page is
  capped at 120 KB.
- A turn keeps working when the browser closes; it stops only on Stop, a model
  error or an engine restart. Reopening the panel shows the transcript (and the
  Stop button if the turn is still running).

### Studio API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/studio/status` | Public probe: model, sites, search, browser |
| `GET`/`PUT` | `/api/studio/config` | Settings (`has_key`, never the key) |
| `GET`/`POST` | `/api/studio/threads` | List / create conversations (`?site=`) |
| `GET` | `/api/studio/threads/:id/messages` | Transcript + `running` |
| `POST` | `/api/studio/chat` | Start a turn (202; one per site) |
| `POST` | `/api/studio/abort` | Stop the turn |
| `GET` | `/api/studio/events?thread=` | SSE: text deltas, tool start/end, plans, bees |
| `POST` | `/api/studio/threads/:id/mode` | Switch a thread between ask and all-in |
| `GET` | `/api/studio/threads/:id/timings` | Per-round timing for the thread |
| `GET` | `/api/studio/uploads/:thread/:file` | Serve an attachment back to the panel |
| `POST` | `/api/studio/patches/:id/apply` · `/api/studio/sites/:id/undo` | Apply / restore |

Runtime state lives in `dashboard/data/studio/`: `threads/`, `patches/`,
`snapshots/`, `shots/`, `stage/`, `uploads/`, plus `settings.json`, `vision.json`,
`design.json` and `audit.jsonl`.

## Adding a site

### CLI (what agents should use)

```bash
sitebox --root ~/sitebox new my-site \
  --name "My Site" --port 4500 --description "One real line"
sitebox --root ~/sitebox build my-site
sitebox --root ~/sitebox check my-site
sitebox --root ~/sitebox verify my-site     # starts, checks HTTP, stops
```

### API / UI

The dashboard's **Add Site** button and `POST /api/sites` both work; the engine
creates or refreshes the manifest beside the site.

### Manifest

```toml
schema = 1

[site]
id = "my-site"
name = "My Site"
description = "One real line"
port = 4500
category = "tools"
icon = "globe"
icon_color = "#58a6ff"
path = "sites/my-site"
public_dir = "public"
serve = "static"

[[pipeline]]
name = "assets"
kind = "images"
inputs = ["assets-raw"]
outputs = ["public/img"]
rules = { max_edge = 1080, quality = 82 }
```

## Deleting a site

Delete is a real delete: stop, remove `sites/<id>/`, drop the entry. Auto-detect
would otherwise put a config-only delete straight back.

```bash
curl -X DELETE http://localhost:4445/api/sites/my-site
curl -X DELETE "http://localhost:4445/api/sites/my-site?keep=1"   # entry only
```

The folder is only removed when the resolved path is inside `sites/`. A purge
also removes that site's Sols conversations, queued patches, snapshots and
screenshots; `?keep=1` keeps the folder and its conversations.

## Dashboard API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/sites` | List sites (`_running`, `_stale`, `_manifest`, `_logLines`, `_favicon`) |
| `POST` | `/api/sites` | Add or update (validates id, name, unique port) |
| `POST` | `/api/sites/reorder` | Persist card order |
| `DELETE` | `/api/sites/:id` | Stop, delete the folder, remove the entry (`?keep=1` keeps files) |
| `POST` | `/api/sites/:id/start` | Start and verify the port listens |
| `POST` | `/api/sites/:id/stop` | Stop |
| `POST` | `/api/sites/:id/restart` | Stop + start |
| `GET` | `/api/sites/:id/health` | HTTP reachability |
| `GET` | `/api/sites/:id/icon` | Auto-detected favicon (404 when none) |
| `GET` | `/api/sites/:id/logs` | Ring + persisted output (`?lines=1..500`) |
| `DELETE` | `/api/sites/:id/logs` | Clear |
| `GET` | `/api/ports/check` | Port availability (`?port=4500`) |
| `GET` | `/api/doctor` | Registry ↔ folders ↔ manifests ↔ ports |

## Checks

`sitebox check <id>` runs the static catalog; `sitebox verify <id>` runs the
HTTP catalog against a live site.

- `SB001` entry point · `SB002` no third-party assets · `SB003` image geometry
  · `SB004` splice markers · `SB005` referenced assets exist
- `SB010` `dims.json` matches files · `SB020` no placeholder data · `SB021` no
  `NaN`/`undefined` · `SB030` data parses
- `SB101` 200 HTML · `SB102` 404 · `SB103` ETag/304 · `SB104` gzip · `SB105`
  Range · `SB106` nosniff · `SB107` traversal refused

Full contract: `skills/sitebox-engine/SKILL.md`. Exit codes: `0` ok, `1`
operational failure, `2` checks failed.

## Skills

| Skill | Role |
|-------|------|
| `sitebox-engine` | The executable contract: commands, check ids, pipeline rules, MCP tools |
| `sitebox-create` | Brief → design → build → register → verify → debug |
| `sitebox-design` | Tokens, image geometry, accessibility/UX audit, copy passes |
| `sitebox-data` | Real, traceable, refreshable content: provenance, coverage, fixtures |
| `sitebox-verify` | Browser layout proof, negative controls, deploy mirror |
| `sitebox-config` | Lifecycle API, ports, logs, troubleshooting, git policy |
| `sitebox-playbook` | Operator taste and the defects that keep coming back (read first on a new site) |
| `sitebox-skill-maintainer` | Meta: conventions and the drift checker |

```bash
node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
```

## Site data & Git policy

`dashboard/data/sites.json` is tracked and describes only the examples; the
engine rewrites it at runtime, so expect a local diff after adding a site.
Personal sites — anything under `sites/` except `example-notes` and
`example-clock` — never enter the repository; `.githooks/pre-commit` blocks them
even if forced.

Enable the hook once per clone:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit
```

## Port ranges

| Range | Purpose |
|-------|---------|
| 4445 | Dashboard |
| 4450–4499 | Examples / tests |
| 4500–4599 | Tools |
| 4600–4699 | Publishing |
| 4700–4799 | Widgets |
| 4800–4899 | Experimental / auto-detected |

## Icons

Dashboard and panel buttons all use the same Lucide line set.

The dashboard prefers a favicon the site actually ships (`<link rel="icon">`,
then `favicon.svg|png|ico`). Local files only; hotlinked and `data:` icons are
ignored. Otherwise it renders a Lucide icon — `globe`, `book`, `clock`, `code`,
`camera`, `gamepad`, `music`, `settings`, `notebook`, `notebook-pen`,
`database`, `star` — with `globe` as the fallback (source:
`dashboard/public/js/main.js`).

## Testing

```bash
cd engine
cargo test                 # 41 unit tests, no network or model required
python3 ../scripts/studio-e2e.py   # 90 checks against a mock model (port 4811)
sitebox self-test          # negative controls: every check fails on a broken fixture
```

## License

MIT. Built-in icons are Lucide SVG geometry (ISC License).

## Security model

There is no login and no per-device permission. Anything that can load the
dashboard can create, delete and reorder sites. That is a deliberate choice: the
gate is the network, not a login page.

| Layer | What it does |
|---|---|
| Network boundary | The dashboard listens on `0.0.0.0:4445`. There is no bind switch; firewall it, or keep it behind tailscale or an SSH tunnel. |
| Studio token | `/api/studio/*` accepts `x-studio-token` (scripts) or the `studio_sid` cookie the browser is given on load. Site CRUD is unauthenticated. |
| Origin | CORS reflects the caller's origin for the Sols launcher. It is not a write check: writes carry no credentials, so there is nothing to protect with it. |
| Apply audit | `dashboard/data/studio/audit.jsonl` — one line per applied or undone patch. Dashboard writes are not logged. |
| Delete brake | Deleting asks you to type the site id. It stops a misclick, not a script on the same network. |

What follows from that:

* A LAN visitor can do anything you can do — plan the network accordingly.
* `serve = "node"` children bind `127.0.0.1` and are reached through the engine's
  proxy; `static` and `proxy` sites bind `0.0.0.0` on their manifest port.
* Request bodies are capped at 4 MB, except `/api/studio/chat` at 48 MB so four
  8 MB images fit after base64 encoding. Without a cap, one oversized POST would
  take down the dashboard and every site in the same process.
