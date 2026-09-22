# SiteBox

A lightweight, config-driven dashboard for managing static sites on your local server.

**Zero dependencies. Pure Node.js. One JSON file.**

## Quick Start

```bash
# Start the dashboard
cd ~/sitebox/dashboard
node server.js

# Open http://localhost:4445
```

### Docker

```bash
docker compose up -d
# Open http://localhost:4445
```

Uses `network_mode: host` — all site ports are directly accessible on the host. Add new sites to `sites/` and they appear in the dashboard automatically (auto-detect). No rebuild needed.

## What's Inside

```
sitebox/
├── dashboard/              ← Dashboard (port 4445)
│   ├── server.js           ← Backend: config, process mgmt, verified starts, logs
│   ├── data/sites.json     ← Site registry (runtime state — never commit changes)
│   └── public/             ← Dashboard UI (light/dark theme)
│
├── sites/                  ← Static sites live here
│   ├── example-notes/      ← Notes app (port 4451)
│   └── example-clock/      ← World clock (port 4452)
│
├── skills/                 ← Project skills for SiteBox agents
│   ├── sitebox-create/     ← Build/redesign/deploy/debug sites (+ performance reference)
│   ├── sitebox-design/     ← Design system, image geometry, audit, copy passes (+ references)
│   ├── sitebox-data/       ← Real-data pipelines: provenance, refreshability, fixtures (+ references)
│   ├── sitebox-verify/     ← Harness, negative controls, browser layout proof, deploy mirror (+ references)
│   ├── sitebox-config/     ← Dashboard API, lifecycle, logs, troubleshooting
│   └── sitebox-skill-maintainer/  ← Keeps the skills in sync (meta)
│
├── .githooks/pre-commit    ← Blocks commits of sites.json
├── Dockerfile
├── docker-compose.yml
└── README.md
```

## Features

- **Start/Stop sites** from the dashboard
- **Verified starts** — start fails loudly (with logs + hint) if the port never listens
- **Live logs** — captured stdout/stderr per site, viewable in the dashboard
- **Health checks** — see which sites are online
- **Port availability** — check before assigning ports (duplicate ports are rejected)
- **Stale detection** — flags entries whose folder/server.js is missing
- **Auto-detect** — drop a site folder in `sites/` and it appears
- **Light/Dark theme** — toggle in header
- **Custom icon colors** — per-site icon color
- **Search & filter** — by name, description, or category

## Adding a Site

### Option 1: Dashboard UI

Click **Add Site** in the dashboard, fill in the form.

### Option 2: API

```bash
curl -X POST http://localhost:4445/api/sites \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "my-site",
    "name": "My Site",
    "description": "A cool site",
    "icon": "globe",
    "iconColor": "#58a6ff",
    "port": 4500,
    "category": "tools",
    "url": "http://localhost:4500",
    "path": "sites/my-site"
  }'
```

### Option 3: Edit sites.json directly (discouraged)

`dashboard/data/sites.json` is runtime state. Editing it by hand skips
validation and will be overwritten by the dashboard. Use the API or the UI
instead, and **never commit your runtime changes** — see
[Site data & Git policy](#site-data--git-policy).

### Option 4: Auto-detect

Just create a folder in `sites/` with a `server.js` and `public/index.html`. The dashboard will detect it automatically.

### Option 5: Skills

If your agent loads the project skills from `skills/`, the entry point is a
**`sitebox`** router skill that lives with the agent (not in this repo): it decides
which of the six apply to a whole job and in what order. The six themselves:
`sitebox-create` (build + deploy), `sitebox-design` (quality gate),
`sitebox-data` (real content), `sitebox-verify` (proof), `sitebox-config`
(lifecycle), and `sitebox-skill-maintainer` (keeps them in sync with the dashboard).

## Skills & Interop

The six working skills are designed to run together:

| Skill | Role |
|-------|------|
| `sitebox-create` | Builds: brief → design pass → build → deploy → register → start → verify → debug. Owns the server template and the deploy mirror |
| `sitebox-design` | Design tokens, image geometry, and the quality gate (accessibility/UX audit, humanizer + deslop copy passes, accepted deviations) |
| `sitebox-data` | Real, traceable, refreshable content: source discovery, provenance, coverage floors, fixtures, uncropped assets |
| `sitebox-verify` | Proof: render harness, negative controls, real-browser layout check, HTTP sweep, checksum-verified deploy |
| `sitebox-config` | Dashboard API, ports, lifecycle, logs, stale entries, git policy |
| `sitebox-skill-maintainer` | Meta: conventions and a drift checker for the other five |

Order for a new site: **design → data → create → verify → config**.

Run the drift checker after any API or icon change:

```bash
node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
```

## Site Structure

Each site follows this pattern:

```
sites/my-site/
├── server.js       ← Node.js static file server
└── public/
    ├── index.html  ← Entry point
    ├── style.css   ← (optional)
    └── app.js      ← (optional)
```

### Minimal server.js

Simplest possible version (always serves `index.html` as a fallback). For
multi-page sites use the canonical template with proper 404s and clean URLs in
`skills/sitebox-create/SKILL.md`.

```js
const http = require('http');
const fs = require('fs');
const path = require('path');
const PORT = process.env.PORT || 4500;
const PUBLIC = path.join(__dirname, 'public');
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
};

http.createServer((req, res) => {
  let file = req.url.split('?')[0];
  if (file === '/') file = '/index.html';
  const fp = path.join(PUBLIC, file);
  if (fs.existsSync(fp) && fs.statSync(fp).isFile()) {
    res.writeHead(200, { 'Content-Type': MIME[path.extname(fp)] || 'text/plain' });
    fs.createReadStream(fp).pipe(res);
  } else {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    fs.createReadStream(path.join(PUBLIC, 'index.html')).pipe(res);
  }
}).listen(PORT, '0.0.0.0', () => console.log(`Running at http://localhost:${PORT}`));
```

## Dashboard API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/sites` | List all sites with status (`_running`, `_stale`, `_logLines`) |
| `POST` | `/api/sites` | Add or update a site (validates id, name, unique port) |
| `DELETE` | `/api/sites/:id` | Remove a site (`?purge=1` also deletes the folder) |
| `POST` | `/api/sites/:id/start` | Start a site and verify the port is listening |
| `POST` | `/api/sites/:id/stop` | Stop a site process |
| `POST` | `/api/sites/:id/restart` | Stop + start |
| `GET` | `/api/sites/:id/health` | Check if site is reachable |
| `GET` | `/api/sites/:id/logs` | Captured output (`?lines=1..500`) |
| `DELETE` | `/api/sites/:id/logs` | Clear captured output |
| `GET` | `/api/ports/check` | Port availability (`?port=4500`) |

## Site data & Git policy

`dashboard/data/sites.json` is **runtime state** — the dashboard rewrites it
when sites are added, edited, or deleted, and auto-detect appends folders it
finds in `sites/`. Deployed servers will always have a different file than the
repository.

**The committed copy must only ever contain `example-notes` and
`example-clock`. Never commit updates to this file.**

A pre-commit hook enforces it. Enable once per clone:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit   # macOS / Linux (git needs the exec bit)
```

If you staged runtime changes by accident:

```bash
git restore --staged dashboard/data/sites.json
git checkout -- dashboard/data/sites.json
```

## Port Ranges

| Range | Purpose |
|-------|---------|
| 4444 | Reserved |
| 4445 | Dashboard |
| 4450-4499 | Examples / tests |
| 4500-4599 | Tools |
| 4600-4699 | Publishing |
| 4700-4799 | Widgets |
| 4800-4899 | Experimental / auto-detected |

## Icon Names

The dashboard renders these icons; any other name falls back to `globe`
(source of truth: `dashboard/public/js/main.js`):

`globe` · `book` · `clock` · `code` · `camera` · `gamepad` · `music` · `settings` · `notebook` · `notebook-pen` · `database` · `star`

## Design

- **Light/Dark theme** — toggle via header button
- **WCAG AA compliant** — 4.5:1+ contrast ratios
- **Custom icon colors** — per-site hex color

## License

MIT
