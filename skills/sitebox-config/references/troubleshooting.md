# Troubleshooting Playbook

Systematic debugging for SiteBox. Rule: read before guessing — the API returns hints and the logs usually contain the exact error. After any fix, re-run start → health → a real request; don't stop at the first green signal.

## Decision tree

```
Site won't start?
├─ "server.js not found"        → stale path (folder moved/deleted)
├─ "port already in use"        → orphan process or another app
├─ "exited ... EADDRINUSE"      → same, discovered at spawn
├─ "never listened within 6s"   → server ignores process.env.PORT
└─ spawn error (node missing)   → environment problem

Started but health offline?
├─ _running true, health false  → wrong port in config vs server.js, or server crashed after listen
└─ _running false               → process died; read logs (kept after exit)

Page loads (200) but broken?
├─ blank                      → JS error, wrong file, or content never written
├─ styles missing             → wrong path/case, MIME mismatch, file not in public/
├─ styles stop after a point  → unclosed `}` in <style>; every rule after it is discarded
├─ icons are blank boxes      → CDN icon set / webfont failed → inline the SVG or vendor it
├─ images 404 on the site     → hotlinked third-party URL → download into public/
├─ MANY images 404 at once    → the deploy was partial; mirror the tree and compare checksums
├─ an image is cropped/squashed → box geometry, not the file → sitebox-design, Image geometry
├─ a fixed image still looks old → cache: bump the asset version, send no-cache + ETag
├─ fonts missing / FOUT       → Google Fonts unreachable (offline) → self-host
└─ favicon 404                → public/favicon.svg not created

Everything stopped at once (dashboard AND every site)?
└─ it was running as a bare node process → see "Keeping it alive" in SKILL.md

Dashboard itself unreachable?
└─ port 4445 occupied / dashboard process not running
```

## Start failures

### `server.js not found at sites/<id>/server.js`

The config points at a path without a server file. Confirm:

```bash
curl -s http://localhost:4445/api/sites   # look for "_stale": true
```

Fixes: restore/rename the folder back, POST the corrected `path`, or delete the entry. Auto-detect will not repair the path for you. Delete with `?purge=1` only if the files are truly disposable.

### `port N is already in use before starting` / `EADDRINUSE`

Something owns the port — often a site process started by a previous dashboard run that is no longer tracked (dashboard restart leaves children running; they are detached on purpose).

1. Identify: `GET /api/ports/check?port=N` → `used: true`.
2. Find the owner (OS-dependent):
   - Windows: `netstat -ano | findstr :N` then `tasklist /FI "PID eq <pid>"`; stop with `taskkill /PID <pid> /F`.
   - macOS/Linux: `lsof -i :N` then `kill <pid>`.
3. Or simply move the site to a free port in the correct range:
   `POST /api/sites` with the same `id` and a new `port`, then `POST /api/sites/<id>/start`.
4. Restarting the dashboard is safe but does not kill orphan site processes — the dashboard only tracks what it spawned this run.

Do not "fix" this by hardcoding another port in `server.js`; the dashboard passes `PORT` and the config must match.

### Process alive but port never listened

The site is running but bound somewhere else. Common causes:

- `server.js` hardcodes a port instead of `process.env.PORT || fallback`.
- The site listens on a different interface or a Unix socket.
- Startup code threw after listen (check logs).

Fix `server.js` (see the canonical template in `sitebox-create`), then `restart`.

### `process exited before it started listening`

Read the `logs` array in the start response, or `GET /api/sites/<id>/logs?lines=200`:

- `Cannot find module './x'` → missing file in the site folder.
- `SyntaxError` → broken JS; run `node --check sites/<id>/server.js` locally.
- Any stack trace → fix the top frame first; the exit code in the log line tells you success (0) vs error (non-zero).

## Runtime failures

### Health offline while `_running: true`

The process exists but HTTP fails: wrong `url`/`port` in config, server crashed between polls (log will show `process exited`), or the server rejects the host header. Compare `port` in `sites.json` with the port in the startup log line ("Running at http://localhost:...").

### Blank page after HTTP 200

1. Fetch the body: `curl -s http://localhost:<port>/` — is the expected HTML actually there?
2. Check every referenced path (`<script src>`, `<link href>`) returns 200. Case sensitivity matters even on localhost when deployed to Linux later.
3. If content is rendered by JS, read the browser console (or Playwright) — a single error stops everything after it.
4. Multi-page sites: open the specific page path; clean URLs resolve via `<page>.html` or `<page>/index.html` in the template.

### Styles or assets missing

- Verify the file exists at the exact casing used in HTML.
- Confirm `server.js` MIME map includes the extension; unknown types are served as `text/plain`, which browsers refuse for CSS/JS.
- Hard-refresh: browsers cache aggressively on localhost too.

### Partially unstyled page (some rules work, everything after a point does not)

This is a CSS syntax error, not a serving problem. A single unclosed `}` — most often inside a `@media` block — makes the browser discard every rule after it, so the server still returns 200 while the page looks half-styled.

```bash
node -e "const s=require('fs').readFileSync(process.argv[1],'utf8');const o=(s.match(/{/g)||[]).length,c=(s.match(/}/g)||[]).length;console.log(o===c?'balanced':'UNBALANCED '+o+'/'+c)" sites/<id>/public/index.html
```

Also look for an orphan declaration left by a previous fix (a property list ending in `}` with no selector) — it is still a parse error. Fix the site's `<style>`; `sitebox-create` owns this, and its quality gate re-checks it.

### Icons missing or images 404 while the file exists

- Blank boxes where icons should be: a CDN icon set or webfont did not load. Fix by inlining the SVG,
  or vendoring the library into `public/` — **not** by adding another CDN link (`sitebox-design` → Icons).
- An image that renders in an editor preview but 404s on the running site is hotlinked. Download it into
  `public/` and reference the local path (`sitebox-create` → `references/performance.md`).
- **Every image on the page 404s at once, but the page itself returns 200:** the deploy was partial.
  This happened for real — only `index.html` was copied, and 22 new images plus 180 commenter avatars
  were missing while every local check stayed green. Confirm with a real request, then mirror the whole
  tree and compare checksums (`sitebox-verify`). `health.online: true` cannot catch this: the page is
  served, it is just empty of pictures.
- **An image is the wrong shape (cropped, squashed, stretched) while the file is correct:** that is a
  layout bug, not a serving bug. Measure the drawn ratio against the natural ratio in a browser —
  `sitebox-design` → Image geometry.
- **A replaced image keeps showing the old version:** the browser has a `max-age` copy, or the asset
  version was not bumped. Send `Cache-Control: no-cache` + `ETag`, and bump `?v=N`.

### Fonts missing / text shifts on load

Google Fonts needs network. On an offline machine the site falls back. Fix by self-hosting `.woff2` in `public/fonts/` with `font-display: swap` and a metric-compatible fallback. See `sitebox-create` → `references/performance.md`.

## Stale entries and delete surprises

- `_stale: true` → path has no `server.js`. Fix path or delete.
- Deleted a site but it came back with a new port → default DELETE keeps the folder; auto-detect re-adds it. Use `?purge=1`.
- Deleted with `purge=1` but the port stays occupied → the process was stopped first, but an untracked orphan may still hold the port (see EADDRINUSE above).

## Logs

- Only processes started via the API have captured logs.
- The buffer keeps the last 500 lines and survives process exit — that's where you see why it died.
- `DELETE /api/sites/<id>/logs` clears before a clean test run so old errors don't confuse the diagnosis.
- Log lines are timestamped ISO strings with a stream tag (`out`, `err`, `sys`).

## Dashboard problems

- **4445 unreachable**: is `node dashboard/server.js` running (or the container up)? Check for another process on 4445. In Docker, `network_mode: host` means ports are shared with the host.
- **Everything stopped at once — dashboard *and* every site**: the dashboard was almost certainly running as a bare `node` process and exited, taking its detached children with it. This is not a per-site failure; do not debug a site. See [Keeping it alive](../SKILL.md#keeping-it-alive--a-bare-node-process-will-die-silently).
- **Unexpected auto-detected entries**: any folder in `sites/` with `server.js` gets picked up. If it shouldn't be a site, move it out of `sites/` or remove its `server.js`.
- **Unsaved config edits**: edit through the API or dashboard; the server rewrites the JSON (atomic write via `.tmp` + rename), so hand edits made while the server runs can be overwritten.

## Docker

- `docker compose up -d` runs the dashboard; site processes are spawned inside the container.
- After `docker restart`/reboot, the dashboard returns but all sites are **stopped** (the running map is in memory). Start them again via API. Sites do not have a restart policy of their own.
- Volumes: `./dashboard/data` and `./sites` are mounted, so edits and runtime config persist across container rebuilds.

## After any fix

```bash
curl -X POST http://localhost:4445/api/sites/<id>/restart
curl -s http://localhost:4445/api/sites/<id>/health          # expect online: true
curl -s http://localhost:<port>/                             # expect the real page
curl -s "http://localhost:4445/api/sites/<id>/logs?lines=20" # expect no new [err]
```

Then, if the change touched files: **re-run the deploy mirror and its checksum**, and re-run the
browser layout check. A fix that is green locally but was not deployed is the most common form of
"I already fixed that".

If the same symptom returns, escalate to the `sitebox-create` quality gate and `sitebox-verify` —
recurring 404s and layout breakage are build problems, not lifecycle problems.

Reminder: never commit changes to `dashboard/data/sites.json`.
