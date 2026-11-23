---
name: sitebox-engine
description: "Use when operating a SiteBox site — scaffold, build, check, verify, deploy, start/stop with the Rust engine. Do not use for design or data-source judgment (sitebox-design, sitebox-data)."
metadata:
  author: sitebox
  version: "1.0.0"
  updated: "2026-11-23"
---

# SiteBox engine — the executable contract

Use this instead of re-reading the long SiteBox skills when the task is
*operating* a site: scaffold, build, check, verify, deploy, start/stop. The
design/copy/data skills still own judgment (what the page should look like and
what the numbers mean). This skill owns the mechanical contract.

Every command takes `--json`; exit codes are `0` ok, `1` operational failure,
`2` the work is not done (checks ran and failed).

## 1. Start from the contract

A site is defined by `sites/<id>/sitebox.toml`:

```toml
schema = 1

[site]
id = "folio"
name = "Folio"
port = 4502
path = "sites/folio"
public_dir = "public"     # default
serve = "static"          # static | node | proxy
# api_upstream = "http://127.0.0.1:4510"   # only for serve = "proxy"

[[pipeline]]
name   = "assets"
kind   = "images"         # built-in: downscale-only WebP + dims.json
inputs = ["assets-raw"]
outputs = ["public/img"]
rules  = { max_edge = 1080, quality = 82, square_avatars = true }

[[pipeline]]
name   = "data"
run    = "node tools/build.js"   # bespoke parser stays bespoke
inputs = ["synd/raw"]
outputs = ["synd/folio-data.json"]
```

`sitebox schema` prints the JSON Schema; validate before writing.

## 2. The workflow

```bash
sitebox plan  <id>              # what would run, inputs ready, fingerprint changed
sitebox build <id>              # ordered; refuses a step whose inputs are missing
sitebox check <id>              # static catalog (fast, no browser)
sitebox verify <id> [--url]     # HTTP catalog; starts the site if needed
sitebox deploy <id> --to <path|user@host:/path> [--dry-run]
sitebox up <id> / down <id> / restart <id>
sitebox logs <id> -n 50
sitebox doctor                  # registry ↔ folders ↔ manifests ↔ ports
```

New site: `sitebox new <id> --name "<Name>" --port <port> --description "<one line>"`
then edit `public/index.html` and re-run `check` → `verify` → `deploy`.

## 3. Check catalog

Static (`SB0xx`):
- `SB001` index.html exists
- `SB002` no third-party assets (a CDN `<link rel=stylesheet>` is a failure)
- `SB003` every `<img>` with a `src` has width and height (runtime-filled images are skipped and counted)
- `SB004` splice markers balanced when the site uses them
- `SB005` every referenced local asset exists
- `SB010` declared geometry (`synd/dims.json`) equals the files on disk
- `SB020` no placeholder or invented data (picsum/unsplash/pravatar/`Math.random`)
- `SB021` no `NaN`/`undefined` leaked into the page
- `SB030` every `synd/*.json` parses

HTTP (`SB1xx`, needs a listener):
- `SB101` `GET /` → 200 `text/html`
- `SB102` missing asset → 404 (never 200 index.html)
- `SB103` ETag → 304
- `SB104` gzip on HTML
- `SB105` Range → 206 when the site ships audio/pdf
- `SB106` `X-Content-Type-Options: nosniff`
- `SB107` raw `GET /../../etc/passwd` refused

`sitebox self-test` proves each detector fires on a broken fixture. A guard that
has never failed is not a guard.

## 4. Rules the engine enforces (do not work around them)

- Never crop a media asset. `kind = "images"` can only downscale; `av-*` is the
  one square exception.
- `dims.json` is generated from the encoded files — never hand-edited.
- A missing asset is a 404. Extension-less routes may fall back to the SPA.
- A step's inputs must exist before it runs; the fingerprint decides skip vs run.
- Deploy is a mirror: extra files on the destination are deleted, then every
  hash is compared. A partial deploy is a failure, not a warning.

## 5. Agent surface

Prefer the MCP tools over shell when available (`sitebox-mcp`): `sitebox_list`,
`sitebox_get`, `sitebox_plan`, `sitebox_build`, `sitebox_check`,
`sitebox_verify`, `sitebox_up/down/restart`, `sitebox_logs`, `sitebox_doctor`,
`sitebox_deploy`, `sitebox_new`, `sitebox_ports_check`, `sitebox_self_test`.

When a check fails, the finding carries `file`, `evidence` and `fix`. Apply the
fix, re-run only that command, and report the check id + evidence. Do not claim
a site works without a green `check` **and** `verify`.
