---
name: sitebox-config
description: Manage site configurations in SiteBox dashboard.
---

# Config Site (SiteBox)

Manage site entries in `dashboard/data/sites.json`.

## List all sites

```bash
curl -s http://localhost:4445/api/sites | python3 -m json.tool
```

## Add a site

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

## Update a site

```bash
curl -X POST http://localhost:4445/api/sites \
  -H 'Content-Type: application/json' \
  -d '{"id":"my-site","description":"Updated description"}'
```

## Delete a site

```bash
curl -X DELETE http://localhost:4445/api/sites/my-site
```

## Start/Stop

```bash
curl -X POST http://localhost:4445/api/sites/my-site/start
curl -X POST http://localhost:4445/api/sites/my-site/stop
```

## Check port

```bash
curl -s "http://localhost:4445/api/ports/check?port=4500"
```

## Health check

```bash
curl -s http://localhost:4445/api/sites/my-site/health
```

## Direct config edit

Edit `dashboard/data/sites.json` directly. The dashboard reads this file on every request.

## Site object fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| id | string | yes | Unique kebab-case identifier |
| name | string | yes | Display name |
| description | string | no | Short description |
| icon | string | no | Lucide icon name (default: globe) |
| iconColor | string | no | Hex color for icon (default: #8b949e) |
| port | number | yes | Port number |
| category | string | no | Category for filtering |
| url | string | yes | Full URL (http://host:port) |
| path | string | yes | Relative path from sitebox root |
| created | string | no | ISO date (auto-set) |
