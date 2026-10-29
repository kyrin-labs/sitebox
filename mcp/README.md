# sitebox-mcp

An MCP server that lets an agent build a small site on SiteBox in about 450 ms, without
learning the SiteBox skills. Two transports, one dispatcher: **stdio by default**, and an
opt-in **streamable HTTP** endpoint for clients that cannot spawn a process.

Zero dependencies, no build step. In stdio mode `node mcp/sitebox-mcp.js` speaks
line-delimited JSON-RPC 2.0 on stdout. Every log line goes to stderr — stdout is the
protocol channel and one stray `console.log` corrupts it. In HTTP mode stdout stays
empty entirely.

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
| `SITEBOX_MCP_TRANSPORT` | `stdio` | `http` serves `/mcp` instead of reading stdin |
| `SITEBOX_MCP_HTTP_PORT` | `4444` | HTTP listen port; `0` picks a free one and logs it |
| `SITEBOX_MCP_HTTP_HOST` | `127.0.0.1` | `0.0.0.0` exposes the endpoint on the LAN |

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

## Over HTTP (opt-in)

`node mcp/sitebox-mcp.js --http` (or `SITEBOX_MCP_TRANSPORT=http`) serves the same eight
tools at `http://127.0.0.1:4444/mcp` using the streamable-HTTP transport. stdio stays the
default: a client that can spawn a process does not need a port.

```bash
SITEBOX_API_TOKEN=$(cat ~/.config/sitebox/mcp.token) node mcp/sitebox-mcp.js --http
```

Rules this endpoint enforces, so it is never a second, weaker door to `delete_site`:

- **Bearer on every request**, the same key the stdio server uses. It refuses to start
  with no key configured. One process is one identity; the dashboard stays the authority,
  so a revoked key fails on the next tool call.
- **A session after `initialize`.** The response carries `Mcp-Session-Id`; every later
  request must send it, and `DELETE /mcp` ends it. Unknown or missing → `404`.
- **`POST /mcp` only** with `content-type: application/json`, 256 KB cap, one JSON-RPC
  object per request (no batching). Responses are `application/json`; `GET /mcp` is `405`
  because this server never pushes a message.
- **Origin must match Host** when an `Origin` header is present (DNS-rebinding defence).
  Clients that send no Origin are unaffected. `GET /health` answers without a key and
  carries no site data.

```bash
BASE=http://127.0.0.1:4444
SID=$(curl -si -X POST "$BASE/mcp" \
  -H "Authorization: Bearer $(cat ~/.config/sitebox/mcp.token)" \
  -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"curl","version":"1"}}}' \
  | sed -n 's/^[Mm]cp-[Ss]ession-[Ii]d: //p' | tr -d '\r')

curl -s -X POST "$BASE/mcp" \
  -H "Authorization: Bearer $(cat ~/.config/sitebox/mcp.token)" \
  -H "mcp-session-id: $SID" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_sites","arguments":{}}}'
```

An MCP client only needs the URL, the bearer header, and `accept: application/json,
text/event-stream` — for example `pi mcp add sitebox --url http://127.0.0.1:4444/mcp
--bearer-token-env-var SITEBOX_KEY`.

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
node --test mcp/test/mcp.test.js        # stdio gates, stub dashboard, no Docker
node --test mcp/test/http.test.js       # HTTP transport: auth, sessions, body cap, origin
node mcp/test/smoke-live.js             # live round trip against production (needs a token)
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
