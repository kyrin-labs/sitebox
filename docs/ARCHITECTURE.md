# Architecture

SiteBox is two things with one boundary:

1. **The engine** (`engine/`, Rust) — owns serving, supervision, checks,
   pipelines, assets and deployment.
2. **The dashboard UI** (`dashboard/public/`) — a client of the engine's HTTP
   API. It owns no behaviour.

The boundary is deliberately narrow: the UI reads `/api/sites` and calls the
lifecycle verbs. Everything else lives behind the CLI and MCP tools.

## Why the engine exists

Measured on the previous 17-site deployment:

- 17 hand-written `server.js`, 1,479 lines, no two identical
- 5 of 17 had no traversal guard, ETag/304 or gzip; one answered every missing
  path with `index.html` and status 200
- 65 per-site tool files, 4 divergent `build.js`, per-site `verify.js` of
  13–38 KB
- the API had no `build`, `verify` or `plan`, so agents had to shell out
- `sites.json` was runtime-only: no contract in git
- "done" was eight paragraphs of prose

The engine turns those into one server, one manifest, one check catalog.

## Components

```
                ┌──────────────────────────────┐
  browser ─────▶│ listener per site (tokio)    │──▶ site public/ (fs)
                │ serve.rs: guard, ETag, gzip, │
                │ Range, real 404              │
                └──────────────────────────────┘
                              ▲
  dashboard UI ──▶ dashboard.rs (axum) ──▶ supervisor.rs
  CLI/MCP      ──▶ sitebox / sitebox-mcp        │
                                                ├─▶ static listener (in-process)
                                                └─▶ node child (serve="node")
```

### `serve.rs` — the shared static server

One request path for every site. It resolves the URL through a component-wise
guard (`safe_join`), refuses anything that escapes the public folder, computes a
validator ETag from size + mtime, answers `If-None-Match` with 304, negotiates
gzip (only when the client asked), serves single byte ranges with 206, and
returns a real 404 for a missing asset. Extension-less routes may fall back to
the SPA entry point; a missing `.png` never does.

### `supervisor.rs` — one process, many sites

Static sites become a `tokio::spawn` listener in the same process: no child
process, no `node_modules`, one RSS for the whole host. `serve = "node"` sites
are spawned with `PORT` in the environment, their stdout/stderr streamed into a
500-line ring **and** `logs/<id>.jsonl`, so a dashboard restart no longer loses
the reason a site died. Start is verified by polling the port, then settling.

### `checks.rs` — proof instead of prose

`SB0xx` are static checks (entry point, external assets, image geometry, splice
markers, referenced files, `dims.json`, placeholders, `NaN`, JSON parse).
`SB1xx` are HTTP checks (200, 404, 304, gzip, Range, nosniff, traversal). Each
finding carries `file`, `evidence` and `fix`. `self-test` runs the detectors
against deliberately broken fixtures — a guard that cannot fail is treated as a
bug.

### `manifest.rs` — the contract

`sitebox.toml` is the single source of truth for a site: identity, port, serve
mode, pipeline steps and check configuration. `sitebox schema` emits the JSON
Schema, which is what an agent validates against before writing.

### `pipeline.rs` — ordered, fingerprinted work

Steps are declared, not scripted. A step with `kind = "images"` runs the Rust
asset stage; a step with `run = "…"` runs the site's own generator. Inputs are
hashed together with the command, the kind and the rules, so editing a
generator re-runs the step and an unchanged step is skipped. A step whose
declared inputs do not exist is `blocked`, never silently skipped.

### `assets.rs` — geometry that cannot drift

Downscale only: media keeps its aspect ratio; the single crop exception is an
explicitly square avatar (`av-*`). `dims.json` is written from the encoded files,
so the declared width/height cannot describe a previous build.

### `deploy.rs` — mirror, not upload

Copy changed files, delete files that no longer exist, then compare sha256 on
both sides. A partial deploy is a failure, not a warning.

### `dashboard.rs` — API parity, nothing more

Same endpoints and response shapes the UI already used, plus `/api/doctor`. The
manifest is loaded by id from `sites/<id>/sitebox.toml`; the registry stays
compatible with old `sites.json` deployments.

### `mcp.rs` — the agent surface

JSON-RPC over stdio, 15 tools mapping to the same engine functions the CLI
calls. Structured results, stable error text, no shell parsing.

## Invariants

1. A missing asset is a 404; a partial deploy is a failure.
2. Media is never cropped; `dims.json` is generated from bytes, not assumptions.
3. Traversal is refused by the server, not sanitised by the client.
4. A start is `ok` only when the port is listening.
5. Runtime state (`sites.json`, logs, fingerprints) is never committed.
6. Personal sites never enter the repository.

## Deployment shapes

- **Docker (default)** — multi-stage build: Rust on Alpine produces a static
  binary; the runtime is `node:22-alpine` so `serve="node"` sites still work.
  Host networking keeps site ports identical to the manifest. `restart:
  unless-stopped` makes the engine survive reboots; sites are started through
  the API after boot.
- **Bare binary** — `siteboxd --root ~/sitebox --port 4445 --ui …` under a
  systemd unit. No Node required unless a site declares it.

## Non-goals

- Pixel-level browser verification. `verify` covers HTTP and static checks; the
  browser layout pass remains a skill (`sitebox-verify`).
- Rewriting a site's bespoke data parser. Those stay as `run` steps behind the
  manifest.
- Multi-tenant isolation. SiteBox manages one owner's sites on one host.
