# SiteBox

A self-hosted dashboard and engine for fast, verifiable static sites.

**One Rust binary. One manifest per site. One shared server. No per-site
`node_modules`.**

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
└── src/bin/          ← sitebox (CLI), siteboxd (dashboard), sitebox-mcp (MCP)

dashboard/public/     ← the unchanged dashboard UI
dashboard/data/       ← runtime registry (never committed)
```

Static sites are served in-process: one listener task per site, no child Node.
A site that genuinely owns an HTTP API declares `serve = "node"` and keeps its
own `server.js`; the supervisor spawns it, verifies the port and persists its
logs. `serve = "proxy"` serves static files and forwards `/api/*` to an
upstream.

## Features

- **One shared server** — traversal guard, ETag/304, gzip, Range, security
  headers and a real 404 for every site, identical everywhere
- **Manifests, not prose** — `sitebox.toml` is machine-readable and has a JSON
  Schema (`sitebox schema`)
- **Provable "done"** — a check catalog with stable ids (`SB001`–`SB107`) and a
  `self-test` that proves each guard can fail
- **Verified starts** — `ok:true` only when the port is actually listening
- **Persisted logs** — 500-line ring in memory plus `logs/<id>.jsonl` on disk
- **Pipelines with fingerprints** — steps run in order; an unchanged step is
  skipped, a changed command or input re-runs it
- **Assets that cannot drift** — downscale-only WebP, `dims.json` written from
  the encoded files, never cropped
- **Deploy as a mirror** — copy, delete extraneous files, then compare sha256
- **MCP server** — 15 typed tools for agents (`sitebox_list`, `check`, `verify`,
  `build`, `up`, …)
- **Auto-detect** — drop a folder with a `sitebox.toml` (or legacy `server.js`)
  into `sites/`
- Light/dark theme, custom icon colours, auto-detected favicons, search/filter

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

The folder is only removed when the resolved path is inside `sites/`.

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
| `sitebox-skill-maintainer` | Meta: conventions and the drift checker |

```bash
node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
```

## Site data & Git policy

Two kinds of state never enter the repository:

1. `dashboard/data/sites.json` — rewritten at runtime by add/delete and
   auto-detect. The committed copy describes only the examples.
2. **Personal sites** — anything under `sites/` except `example-notes` and
   `example-clock`. Their content, manifests, data and assets are private.

`.gitignore` excludes both, and `.githooks/pre-commit` blocks them even if
forced. Enable the hook once per clone:

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

The dashboard prefers a favicon the site actually ships (`<link rel="icon">`,
then `favicon.svg|png|ico`). Local files only; hotlinked and `data:` icons are
ignored. Otherwise it renders a Lucide icon — `globe`, `book`, `clock`, `code`,
`camera`, `gamepad`, `music`, `settings`, `notebook`, `notebook-pen`,
`database`, `star` — with `globe` as the fallback (source:
`dashboard/public/js/main.js`).

## Testing

```bash
cd engine
cargo test                 # 13 tests, no network or model required
sitebox self-test          # negative controls: every check fails on a broken fixture
```

## License

MIT. Built-in icons are Lucide SVG geometry (ISC License).
