# Contributing to SiteBox

## Adding a New Site

1. Create directory: `sites/your-site-name/`
2. Add `server.js` (copy from `sites/example-notes/server.js`)
3. Add content in `sites/your-site-name/public/`
4. Register in `dashboard/data/sites.json`
5. Start and test: `curl -X POST http://localhost:4445/api/sites/your-site-name/start`

## Conventions

- Directory names: `kebab-case`
- Each site is fully self-contained
- No shared dependencies between sites
- Prefer vanilla HTML/CSS/JS
- Use `process.env.PORT` with fallback

## Dashboard Changes

The dashboard is at `dashboard/`. It's a vanilla HTML/CSS/JS app served by `dashboard/server.js`.

- `dashboard/public/` — frontend files
- `dashboard/data/sites.json` — site config
- `dashboard/server.js` — backend (API + static serving)

## Running Locally

```bash
node dashboard/server.js
# http://localhost:4445
```
