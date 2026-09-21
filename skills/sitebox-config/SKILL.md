---
name: sitebox-config
description: Manage site configurations in SiteBox dashboard.
---

# SiteBox Config

Manage sites via the dashboard API or directly edit `dashboard/data/sites.json`.

## Quick Reference

### List all sites
```bash
curl -s http://localhost:4445/api/sites | python3 -m json.tool
```

### Add a site
```bash
curl -X POST http://localhost:4445/api/sites \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "my-site",
    "name": "My Site",
    "description": "What it does",
    "icon": "globe",
    "iconColor": "#58a6ff",
    "port": 4500,
    "category": "tools",
    "url": "http://localhost:4500",
    "path": "sites/my-site"
  }'
```

### Update a site
```bash
curl -X POST http://localhost:4445/api/sites \
  -H 'Content-Type: application/json' \
  -d '{"id":"my-site","description":"New description"}'
```

### Delete a site
```bash
curl -X DELETE http://localhost:4445/api/sites/my-site
```

### Start / Stop
```bash
curl -X POST http://localhost:4445/api/sites/my-site/start
curl -X POST http://localhost:4445/api/sites/my-site/stop
```

### Check port availability
```bash
curl -s "http://localhost:4445/api/ports/check?port=4500"
```

### Health check
```bash
curl -s http://localhost:4445/api/sites/my-site/health
```

---

## Site Object Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique kebab-case identifier |
| `name` | string | yes | Display name shown in dashboard |
| `description` | string | no | Short description (1 line) |
| `icon` | string | no | Lucide icon name (default: `globe`) |
| `iconColor` | string | no | Hex color for icon (default: `#8b949e`) |
| `port` | number | yes | Port number (must be unique) |
| `category` | string | no | Category for filtering |
| `url` | string | yes | Full URL (`http://host:port`) |
| `path` | string | yes | Relative path from sitebox root |
| `created` | string | no | ISO date (auto-set on creation) |

## Available Icons

`globe` · `book` · `clock` · `code` · `camera` · `gamepad` · `music` · `settings` · `notebook` · `notebook-pen` · `database` · `star` · `package` · `layers` · `zap`

## Port Ranges

| Range | Category |
|-------|----------|
| 4444 | Reserved |
| 4445 | Dashboard |
| 4450-4499 | Examples |
| 4500-4599 | Tools |
| 4600-4699 | Publishing |
| 4700-4799 | Widgets |
| 4800-4899 | Experimental |

## Notes

- Auto-detect: new folders in `sites/` with `server.js` appear automatically
- Dashboard reads `sites.json` on every request (no restart needed)
- The `url` field stores `localhost` — dashboard replaces it with the actual hostname for Open links
