---
name: sitebox-create
description: Create a new static site in SiteBox format.
---

# Create Site (SiteBox)

Creates a new static site following SiteBox conventions.

## Steps

1. Pick an ID: `kebab-case`, descriptive (e.g. `my-blog`, `weather-widget`)
2. Find a free port:
   ```bash
   curl -s http://localhost:4445/api/sites | python3 -c "import sys,json; used={s['port'] for s in json.load(sys.stdin)}; print(next(p for p in range(4500,4900) if p not in used))"
   ```
3. Create directory:
   ```bash
   mkdir -p ~/sitebox/sites/{ID}/public
   ```
4. Create `server.js` (copy from any existing site in `sites/`)
5. Create content in `public/`
6. Register via API:
   ```bash
   curl -X POST http://localhost:4445/api/sites \
     -H 'Content-Type: application/json' \
     -d '{"id":"ID","name":"Name","description":"Desc","icon":"globe","iconColor":"#58a6ff","port":PORT,"category":"tools","url":"http://localhost:PORT","path":"sites/ID"}'
   ```
7. Start it:
   ```bash
   curl -X POST http://localhost:4445/api/sites/ID/start
   ```

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

## Icons (Lucide)

`globe` · `book` · `clock` · `code` · `camera` · `gamepad` · `music` · `settings` · `notebook` · `notebook-pen` · `database` · `star`

## Rules

- Each site = its own directory under `sites/`
- Each site has `server.js` + `public/`
- Use `process.env.PORT` with fallback
- Self-contained, no shared deps
- Vanilla HTML/CSS/JS preferred
