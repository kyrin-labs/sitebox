---
name: sitebox-config
description: Manage the SiteBox lifecycle — add, update, start, stop, restart, verify, debug, and remove sites via the dashboard API. Use when registering or deleting a site, choosing or validating a port, starting a site and checking it actually listens, reading a site's logs, investigating a site that won't start or shows a blank page, handling stale entries after moving folders, or when someone asks about editing dashboard/data/sites.json (never commit it).
metadata:
  author: sitebox
  version: "2.0.0"
  updated: "2026-09-22"
---

# SiteBox Config & Lifecycle

One API, port 4445, zero dependencies. This skill owns everything that happens to a site after the files exist. Building the site itself is `sitebox-create`'s job (load it for quality rules and the server template).

Base URL: `http://localhost:4445` (replace host if remote).

## Lifecycle at a glance

```
create files  →  POST /api/sites       (register; validates id, port, name)
              →  POST :id/start        (spawns node, VERIFIES the port listens)
              →  GET  :id/health       (HTTP reachability)
              →  GET  :id/logs         (captured stdout/stderr)
              →  POST :id/stop         (SIGTERM, then SIGKILL after 2.5s)
              →  POST :id/restart      (stop + start)
              →  DELETE :id            (removes config; folder stays)
              →  DELETE :id?purge=1    (removes config AND folder)
```

## Port selection

Never guess a port. Check the API and the range table:

```bash
curl -s "http://localhost:4445/api/ports/check?port=4500"
# { "port": 4500, "used": false, "available": true }

curl -s http://localhost:4445/api/sites
```

| Range | Category |
|-------|----------|
| 4444 | Reserved |
| 4445 | Dashboard itself |
| 4450–4499 | Examples / tests |
| 4500–4599 | Tools |
| 4600–4699 | Publishing |
| 4700–4799 | Widgets |
| 4800–4899 | Experimental / auto-detected |

The API rejects duplicate ports assigned to other sites with HTTP 409, and rejects ports outside 1024–65535. `ports/check` answers "is something listening there right now" — useful but not sufficient: a port can be free now and taken after boot, so keep the config unique too.

## Register / update

```bash
curl -X POST http://localhost:4445/api/sites \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "my-site",
    "name": "My Site",
    "description": "What it actually does",
    "icon": "globe",
    "iconColor": "#8B5E3C",
    "port": 4500,
    "category": "tools",
    "url": "http://localhost:4500",
    "path": "sites/my-site"
  }'
```

- `id` must be kebab-case: `^[a-z0-9]+(-[a-z0-9]+)*$`. This is validated (400).
- `name` and a valid `port` are required. `path` defaults to `sites/<id>`; `url` defaults to `http://localhost:<port>`.
- Updates are partial: POST the same `id` with only the fields to change. `created` is set automatically on first registration.
- Changing `port` or `path` on a running site requires a restart to take effect (`POST :id/restart`).

## Start = spawn + verify

```bash
curl -X POST http://localhost:4445/api/sites/my-site/start
```

The response is a contract, not a fire-and-forget:

```jsonc
{ "ok": true,  "message": "started and verified", "pid": 12345, "port": 4500, "verified": true }
{ "ok": false, "message": "process exited before it started listening (code=1)",
  "hint": "Port is already in use...", "logs": [{ "t": "...", "stream": "err", "text": "..." }] }
```

`ok:true` means the port was confirmed listening (checked by a real bind test, then a short settle). Failure modes, all with `hint` and recent `logs`:

- **server.js not found** — folder missing/moved or `path` wrong (`_stale` on GET /api/sites).
- **port already in use before starting** — an orphan process from a previous dashboard run, or another app.
- **process exited (EADDRINUSE)** — same, but discovered during startup.
- **alive but port never listened within 6s** — usually `PORT` env ignored (hardcoded port in server.js).

Always follow a start with:

```bash
curl -s http://localhost:4445/api/sites/my-site/health
# { "online": true, "status": 200 }
curl -s "http://localhost:4445/api/sites/my-site/logs?lines=50"
```

Never report "site is up" from `ok:true` alone — confirm `health.online` and that `curl http://localhost:4500/` returns 200.

## Logs

```bash
curl -s "http://localhost:4445/api/sites/my-site/logs?lines=200"
# { "id": "my-site", "running": true, "count": 42,
#   "lines": [ { "t": "2026-09-22T...", "stream": "out", "text": "Running at ..." } ] }

curl -X DELETE http://localhost:4445/api/sites/my-site/logs
```

Notes:

- Logs are captured only for processes **started via the dashboard API** — a site started by hand from a terminal has no captured output.
- Ring buffer: last 500 lines per site, cleared on `DELETE …/logs`, kept after the process exits (so you can read why it died).
- Interpret errors: the last `[err]` line is the cause; `EADDRINUSE` → port, `Cannot find module` → missing file, `SyntaxError` → code.

## Stop / restart

```bash
curl -X POST http://localhost:4445/api/sites/my-site/stop
curl -X POST http://localhost:4445/api/sites/my-site/restart
```

Stop sends SIGTERM and escalates to SIGKILL if the process hasn't exited after ~2.5s. Restart waits briefly for the port to free before starting, so it's the right tool after config or code changes.

## Delete and purge

```bash
# Remove the dashboard entry, keep the folder (it will be auto-detected again!)
curl -X DELETE http://localhost:4445/api/sites/my-site

# Remove the entry AND the files (safest way to truly delete a site)
curl -X DELETE "http://localhost:4445/api/sites/my-site?purge=1"
```

- Default delete removes the config only. Because auto-detect scans `sites/`, the folder reappears on the next request with a fresh port — if that surprises you, use `purge`.
- `purge=1` refuses to delete anything outside `sites/` and deletes the folder only after stopping the process.

## Stale entries

`GET /api/sites` marks each entry:

- `_running` — a live process tracked by the dashboard.
- `_stale: true` — `server.js` doesn't exist at `path`. The site can't start.
- `_logLines` — captured log lines available.

Stale usually means a folder was moved/renamed outside the dashboard. Fix the `path` (POST), or delete the entry (`?purge=1` for cleanup). The dashboard shows a "stale" badge and disables Start/Open.

Auto-detect never removes entries — it only adds missing folders. The file is only rewritten when something is added; just browsing the dashboard does not modify it.

## Icons

Available Lucide-style icons in the dashboard: `globe` `book` `clock` `code` `camera` `gamepad` `music` `settings` `notebook` `notebook-pen` `database` `star`. Unknown names fall back to `globe` silently, so check the list. Source of truth: `dashboard/public/js/main.js` (`ICONS`).

## Site object fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique kebab-case identifier |
| `name` | string | yes | Display name |
| `description` | string | no | One real line (no filler) |
| `icon` | string | no | Icon name (default `globe`) |
| `iconColor` | string | no | Hex color (default `#8b949e`) |
| `port` | number | yes | Unique, 1024–65535, within range table |
| `category` | string | no | Used for dashboard filtering |
| `url` | string | no | Full URL; defaults to `http://localhost:<port>` |
| `path` | string | no | Relative to SiteBox root; defaults to `sites/<id>` |
| `created` | string | no | ISO date, auto-set |

## Never do this: commit `dashboard/data/sites.json`

- The file is **runtime state**. The dashboard rewrites it on add/edit/delete and auto-detect.
- The git copy must contain **only** `example-notes` and `example-clock`. Compare: `git show HEAD:dashboard/data/sites.json`.
- On a deployed server the file will differ from git — that is expected. Never commit those differences. A pre-commit hook (`.githooks/pre-commit`) blocks it; enable with `git config core.hooksPath .githooks`.
- To change sites, use the API, not hand edits to the committed file.

## API reference

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/sites` | List sites with `_running`, `_stale`, `_logLines` |
| `POST` | `/api/sites` | Add or partially update a site (validated) |
| `DELETE` | `/api/sites/:id` | Remove config (`?purge=1` also deletes files) |
| `POST` | `/api/sites/:id/start` | Start + verify port |
| `POST` | `/api/sites/:id/stop` | Stop process |
| `POST` | `/api/sites/:id/restart` | Stop + start |
| `GET` | `/api/sites/:id/health` | HTTP reachability |
| `GET` | `/api/sites/:id/logs` | Captured output (`?lines=1..500`) |
| `DELETE` | `/api/sites/:id/logs` | Clear captured output |
| `GET` | `/api/ports/check` | Port availability (`?port=N`) |

## Notes

- New folders in `sites/` with a `server.js` are auto-detected on the next API request and get a port from 4800 upward.
- The `url` field stores `localhost`; the dashboard replaces the host for "Open" links.
- Sites do **not** auto-start after a machine or Docker restart. After reboot, start them via `POST :id/start` (or the dashboard UI). Dashboard itself restarts with `restart: unless-stopped` in `docker-compose.yml`.
- Full failure playbook: `references/troubleshooting.md`.
- Build quality rules: `sitebox-create` (+ its performance reference) and `sitebox-design`.
