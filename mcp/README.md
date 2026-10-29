# sitebox-mcp

A stdio MCP server that lets an agent build a small site on SiteBox in about 450 ms,
without learning the SiteBox skills.

Zero dependencies, no build step: `node mcp/sitebox-mcp.js` speaks line-delimited
JSON-RPC 2.0 on stdout. Every log line goes to stderr — stdout is the protocol channel
and one stray `console.log` corrupts it.

The server holds no business logic of its own. It validates and renders files locally,
then calls the dashboard API for everything that touches state, so there is exactly one
implementation of ports, lifecycle and config.

## Register it with a client

```yaml
mcp_servers:
  sitebox:
    command: node
    args: ["/home/l2s/sitebox/mcp/sitebox-mcp.js"]
    enabled: true
    env:
      SITEBOX_API: http://127.0.0.1:4445
      SITEBOX_SITES: /home/l2s/sitebox/sites
```

Use the **deployed** tree (`~/sitebox/mcp/...`), not the working clone: sites live under
`~/sitebox/sites` and that is where files must be written.

| Variable | Default | Meaning |
|---|---|---|
| `SITEBOX_API` | `http://127.0.0.1:4445` | dashboard base URL |
| `SITEBOX_SITES` | `~/sitebox/sites` | where site folders live (must match the volume) |
| `SITEBOX_API_TOKEN` | — | Bearer token; falls back to `~/.config/sitebox/mcp.token` (600) |
| `SITEBOX_DATA_HOST` | `~/sitebox/dashboard/data` | used by `doctor` to check the host can mint |
| `SITEBOX_BASELINE_DIR` | `~/backups/sitebox-baseline` | `doctor` reads the last pre-deploy baseline |

Keep the key **out of the client config** when you can: a config file gets copied and
shared, a `0600` file in `~/.config` does not. Mint one per client so `lastUsed` and the
audit `actor` say which agent acted:

```bash
node ~/sitebox/dashboard/token.js add mcp-hermes --scope write --print | tail -1 \
  > ~/.config/sitebox/mcp-hermes.token && chmod 600 ~/.config/sitebox/mcp-hermes.token
```

`write` includes `read`; there is no combined value to pass. A `read` key can list and
check, but `create_site`/`update_site`/`site_ctl`/`delete_site` answer 403 — `doctor`
reports that as `E_SCOPE` instead of letting the first write discover it.

The dashboard UI needs no token — but that browser session authenticates **people**, not
clients. A cookie never authorises an agent, and `401` means exactly that.

## The eight tools

| Tool | What it does |
|---|---|
| `doctor` | preflight: capabilities, token (and what it may do: `token_access.canWrite`), stale listeners, root-owned files, disk, host mint path, plus a fill-in `create_site` skeleton and the layout catalog |
| `list_sites({id?})` | registered sites with real reachability (HTTP GET on the port, not the `_running` flag) |
| `create_site` | validate → render → register on a self-allocated port → start → verify `GET /` = 200 with a matching `<title>` |
| `update_site` | patch `sitebox.json`, re-render, restart, verify — one call for a colour or a sentence |
| `check_site` | validate + re-render and diff against disk; reports drift from hand edits |
| `site_ctl({id, action})` | start / stop / restart (restart is also how a stale PID map is cleaned) |
| `site_logs({id, lines?})` | tail of the dashboard ring buffer |
| `delete_site({id, confirm})` | deletes entry **and folder**; `confirm` must equal the id |

A prototype costs **two calls**: `doctor` once per session, then `create_site`.

Every tool that names a site (`create_site`, `update_site`, `site_ctl`, `list_sites`)
returns `url`, `localOnly` and `note`. A site registered on `127.0.0.1` has no LAN URL:
its `url` is `http://localhost:<port>` and it opens **only on the host running SiteBox**.
Hand the operator the URL together with that note; do not replace `localhost` with a
guessed hostname, and do not tell them to open it from a phone.

## What `create_site` accepts

Content, never structure. Do not send HTML; `server.js` is a hash-pinned template the
agent must never write.

```
{ id, name, layout, accent, title, tagline, sections[], lang?, density?,
  radius?, typeScale?, hero?, rules?, texture? }
```

- `id` kebab-case, ≤32 chars (the dashboard rejects anything else) · `name` ≤40 ·
  `title` ≤64 · `tagline` ≤90
- `layout` ∈ `docs | dashboard | showcase | list | form` (see `doctor.layouts` for when)
- `accent` ∈ pre-measured palette: `ink-green | navy | terracotta | firefly | deep-teal |
  charcoal` — every value is AA against the paper, so contrast cannot fail
- knobs (all optional, enum): `density` `radius` `typeScale` `hero` `rules` `texture`.
  Omitted knobs get a deterministic cosmetic seed from the id, so two prototypes never
  look identical by accident.
- `sections` ≤5, `type` ∈ `hero | features | steps | table | faq | cta | footer-note`,
  `heading` ≤56, `body` ≤240, `items` ≤6, table needs `columns: [left, right]`

Hard rules, enforced at write time rather than at the end: no placeholder markers
(`«…»`, "lorem", "TODO"), no Han characters, no external `src`/`href`/`@import`/CDN, at
most 5 served files and 48 KB, and the generated server binds `127.0.0.1` unless
`sites.json` says otherwise.

## Error codes

Every failure carries `code`, `path` and a mechanical `fix`; all failures arrive in one
response so one round of repair is enough.

`E_CAP` `E_ENUM` `E_SHAPE` `E_EMPTY` `E_PLACEHOLDER` `E_ID_FORMAT` `E_CJK` `E_EXTERNAL`
`E_BUDGET` `E_FILES` `E_BIND` `E_DRIFT` `E_ID_TAKEN` `E_NOT_PROTOTYPE` `E_AUTH`
`E_TOO_OLD` `E_PORT_TAKEN` `E_BAND_FULL` `E_START_FAILED` `E_VERIFY_FAILED` `E_CONFIRM`

`doctor` also reports conditions of the host rather than of the payload:
`E_ROOT_OWNED` (files owned by root under `sites/`), `E_STALE_LISTENER` (a port answers
but the dashboard has no process for it), `E_NO_MINT_PATH` (the operator could not mint a
key — the recovery path is broken), `E_SCOPE` (the configured key is read-only, so every
write tool answers 403), `E_UNREACHABLE`, and two notes that are verdicts
rather than failures: `E_BASELINE_GAP` and `E_ALL_STOPPED`.

- `E_TOO_OLD` = the dashboard is older than this server: rebuild it, do not retry.
- `E_PORT_TAKEN` = the only retryable one (re-register with `port: 0`, max 2 attempts).
- `E_BAND_FULL` (507) = an ops verdict, point the operator at `doctor`.
- `E_ID_TAKEN` is decided by schema hash: same payload is `{noop:true}`, a folder that
  is missing is `resume`, different content is a real conflict → `update_site`.

## Tests

```bash
node --test mcp/test/mcp.test.js       # 17 gates, stub dashboard, no Docker
node mcp/test/smoke-live.js            # live round trip against production (needs a token)
```

`node mcp/test/doc-check.js` fails when a tool exists but this file does not document it.

`smoke-live.js` also asserts things a stub cannot: the site is refused from the LAN
before and after a restart, a revoked token answers 401 on the next request, the audit
log grows by exactly one line per write with no secret material, and a 70 KB body is
rejected with 413 while the dashboard keeps answering. It runs inside the deploy script
(phase 6b) and writes a few audit lines each run — that is expected, not a leak.

## When a prototype is not enough

`update_site` and the schema stop at one page. If the work needs several routes, real
data pipelines, or a visual audit, it is no longer a prototype: use the full
`sitebox-create` pipeline. Do not add a write tool here to get around that.
