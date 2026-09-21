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
│   ├── server.js           ← Backend: config, process mgmt, health checks
│   ├── data/sites.json     ← Site registry (edit directly or via API)
│   └── public/             ← Dashboard UI (light/dark theme)
│
├── sites/                  ← Static sites live here
│   ├── example-notes/      ← Notes app (port 4451)
│   └── example-clock/      ← World clock (port 4452)
│
├── skills/                 ← Hermes skills for SiteBox
│   ├── sitebox-create/     ← Create new sites
│   └── sitebox-config/     ← Manage site configs
│
├── Dockerfile
├── docker-compose.yml
└── README.md
```

## Features

- **Start/Stop sites** from the dashboard
- **Health checks** — see which sites are online
- **Port availability** — check before assigning ports
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

### Option 3: Edit sites.json directly

Edit `dashboard/data/sites.json`.

### Option 4: Auto-detect

Just create a folder in `sites/` with a `server.js` and `public/index.html`. The dashboard will detect it automatically.

### Option 5: Hermes Skill

If you have Hermes Agent configured, use the `sitebox-create` skill.

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
| `GET` | `/api/sites` | List all sites with status |
| `POST` | `/api/sites` | Add or update a site |
| `DELETE` | `/api/sites/:id` | Remove a site |
| `POST` | `/api/sites/:id/start` | Start a site process |
| `POST` | `/api/sites/:id/stop` | Stop a site process |
| `GET` | `/api/sites/:id/health` | Check if site is reachable |

## Port Ranges

| Range | Purpose |
|-------|---------|
| 4445 | Dashboard |
| 4450-4499 | Examples / tests |
| 4500-4599 | Tools |
| 4600-4699 | Publishing |
| 4700-4799 | Widgets |
| 4800-4899 | Experimental / auto-detected |

## Icon Names

Use any [Lucide](https://lucide.dev) icon name:

`globe` · `book` · `clock` · `code` · `camera` · `gamepad` · `music` · `settings` · `notebook` · `notebook-pen` · `database` · `star`

## Design

- **Light/Dark theme** — toggle via header button
- **WCAG AA compliant** — 4.5:1+ contrast ratios
- **Custom icon colors** — per-site hex color

## License

MIT
