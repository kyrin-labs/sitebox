---
name: sitebox-create
description: Build, verify, redesign, and debug static sites for SiteBox. Use when creating a new site under sites/, scaffolding a site's server.js and public/ files, redesigning or improving an existing SiteBox site, registering a site in the dashboard, or when a SiteBox site fails to start, shows a blank page, renders half-styled, 404s, or needs a quality pass (performance, accessibility, copy). Covers the full loop: brief, design, build, register, start, verify, debug.
metadata:
  author: sitebox
  version: "2.1.0"
  updated: "2026-09-22"
---

# Create / Redesign a Site (SiteBox)

Build sites that feel like real products — not demos, not templates. Every site ships verified: registered, started, health-checked, and audited before you call it done.

## Companion skills — load them, don't duplicate them

| Skill | What it owns | When to load |
|-------|--------------|--------------|
| `sitebox-design` | Design tokens, typography, color, layout, components, visual anti-patterns, accessibility/UX audit, copy passes (humanizer + deslop) | Before any visual work or redesign (step 3), and for the quality gate (step 9) |
| `sitebox-config` | Dashboard API, port selection, register/start/stop/restart, logs, health, stale entries, delete/purge | Steps 4 and 8, and any lifecycle/debug work |
| `sitebox-skill-maintainer` | The SiteBox skills themselves | Only when editing skills, not when building sites |

The design system lives in `sitebox-design`. Never copy its token tables into a site's `<style>` as a substitute for reading it, and never duplicate its content into other skills — drift between skill copies is how quality dies.

## Workflow

### 1. Pick the mode

- **Create** — new folder under `sites/`. Continue below.
- **Redesign** — an existing site (folder or running URL). Jump to [Redesign mode](#redesign-mode).

### 2. Understand the brief

Answer before writing code:

- What is this site? Who visits it?
- What content does it need — pages, sections, real data?
- What feeling should it evoke — professional, warm, minimal, bold, editorial?
- Which category and port range does it belong to?

If the brief is silent on something, propose a concrete choice and say why. Do not fall back to a generic default (see `sitebox-design` for the AI-tells list).

### 3. Design pass (required)

Load `sitebox-design` and produce a short design plan there:
1. Token system: color (4–6 named hex), type (max 2 families), spacing scale.
2. Layout concept in one sentence, left-aligned unless the brief says otherwise.
3. Mark: the logo/brand-mark idea in one sentence, plus how it becomes `favicon.svg` (see `sitebox-design` → `references/brand-mark.md`).
4. Review the plan against the brief; revise anything that reads like the default you'd produce for any similar page. Say what you changed and why.

Only then write code. The plan is the contract; deviations during build should be deliberate. If you explore alternatives in a scratch page, delete that page before shipping — concept previews are never part of the site.

### 4. Plan the id and port

Use `sitebox-config` for the full procedure. Short version:

```bash
# kebab-case id derived from the name, e.g. "Lumen Press" -> lumen-press
curl -s "http://localhost:4445/api/ports/check?port=4500"
curl -s http://localhost:4445/api/sites
```

Rules: id is kebab-case (`^[a-z0-9]+(-[a-z0-9]+)*$`), port must be unique in `sites.json`, and must fit the range table in `sitebox-config`. The dashboard API rejects duplicate ports with HTTP 409 — don't guess, check.

### 5. Scaffold

```bash
mkdir -p sites/<id>/public
```

Every site = `sites/<id>/server.js` + `sites/<id>/public/`.

### 6. Write server.js (canonical template)

Copy this exactly for multi-page sites. Single-page sites may copy the simpler `sites/example-notes/server.js` instead. Do not invent a third variant.

```js
const http = require('http');
const fs = require('fs');
const path = require('path');
const PORT = process.env.PORT || PORT_NUMBER;
const PUBLIC = path.join(__dirname, 'public');
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png', '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml', '.ico': 'image/x-icon',
};

http.createServer((req, res) => {
  let file = req.url.split('?')[0];
  if (file === '/') file = '/index.html';
  const fp = path.join(PUBLIC, file);
  if (!fp.startsWith(PUBLIC)) { res.writeHead(403); res.end('Forbidden'); return; }
  if (fs.existsSync(fp) && fs.statSync(fp).isFile()) {
    res.writeHead(200, { 'Content-Type': MIME[path.extname(fp)] || 'text/plain' });
    fs.createReadStream(fp).pipe(res);
  } else if (!path.extname(file)) {
    const html = path.join(PUBLIC, file + '.html');
    if (fs.existsSync(html)) { res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }); fs.createReadStream(html).pipe(res); }
    else { const idx = path.join(PUBLIC, file, 'index.html'); if (fs.existsSync(idx)) { res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }); fs.createReadStream(idx).pipe(res); } else { res.writeHead(404); res.end('Not Found'); } }
  } else { res.writeHead(404); res.end('Not Found'); }
}).listen(PORT, '0.0.0.0', () => console.log(`Running at http://localhost:${PORT}`));
```

Replace `PORT_NUMBER` with the planned port. Keep `process.env.PORT` — the dashboard passes the port from `sites.json`, and a mismatch here is the most common cause of "started but port never listening".

### 7. Build the site

- Single page: one `index.html` with embedded CSS. Multi-page: `index.html`, `<page>.html` (clean URL works via the template), or `<page>/index.html`.
- Required head (adapt `lang`):

```html
<!DOCTYPE html>
<html lang="th">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Page Title — Site Name</title>
  <meta name="description" content="One real sentence about this page.">
  <meta property="og:title" content="...">
  <meta property="og:description" content="...">
  <meta property="og:type" content="website">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=..." rel="stylesheet">
  <link rel="icon" type="image/svg+xml" href="/favicon.svg">
  <style>/* all CSS here */</style>
</head>
<body>
  <main><!-- content --></main>
</body>
</html>
```

- **Always create `public/favicon.svg`.** The head references it and a missing favicon is a guaranteed 404.
- **Icons are inline SVG from one set.** Put the `<path>` in the markup (Lucide geometry is the house style). Never load an icon webfont or icon-set CSS from a CDN — it is render-blocking, breaks offline, and violates the zero-dependency rule.
- **Never hotlink images.** Download external images into `public/` and serve them from there; third-party hosts block referrer/localhost requests, so the image 404s on the running site even when it loads in your editor preview.
- **Balance your braces.** `<style>` needs a `{` for every `}`. One missing `}` silently discards every rule after it and the page ships half-styled with no error. Check before verifying:

```bash
node -e "const s=require('fs').readFileSync(process.argv[1],'utf8');const o=(s.match(/{/g)||[]).length,c=(s.match(/}/g)||[]).length;console.log(o===c?'CSS braces balanced':'UNBALANCED: '+o+' { vs '+c+' }')" sites/<id>/public/index.html
```

- Fonts: if the site must work offline, self-host `.woff2` files in `public/fonts/` — Google Fonts requires network. See `references/performance.md`.
- Follow the performance budgets in `references/performance.md` (font count, image dimensions, lazy loading, no render-blocking JS).
- Content and visual quality: follow `sitebox-design` and its `references/audit.md`.

### 8. Copy pass (humanizer + deslop)

Before registering, run the two copy passes from `sitebox-design` → `references/writing.md`:

1. **Humanizer** — specific verbs, active voice, plain language, consistent naming through the flow (the button that says "Publish" produces "Published").
2. **Deslop** — remove AI tells: opener clichés, buzzwords, rule-of-three filler, fake testimonials, generic headlines ("About Us", "Features"), decorative em-dash labels.

Real content only — no lorem ipsum, no placeholder numbers.

### 9. Register, start, verify

Use `sitebox-config`. The minimum loop:

```bash
curl -X POST http://localhost:4445/api/sites -H 'Content-Type: application/json' \
  -d '{"id":"<id>","name":"<Name>","description":"<real one-liner>","icon":"globe","iconColor":"#<subject-specific>","port":<port>,"category":"<category>","url":"http://localhost:<port>","path":"sites/<id>"}'

curl -X POST http://localhost:4445/api/sites/<id>/start
curl -s http://localhost:4445/api/sites/<id>/health
curl -s http://localhost:4445/api/sites/<id>/logs?lines=50
```

`start` now verifies the process: you get `ok:true` only when the port is actually listening. If `ok:false`, the response includes `hint` and the last log lines — read them before retrying. Never report a site as working without `health.online: true` and one real request returning 200 (fetch `/` and one linked asset such as `/favicon.svg`).

### 10. Quality gate

Run in this order, fix, then re-verify:

1. `sitebox-design` → `references/audit.md` — accessibility/UX checklist.
2. `references/performance.md` — budgets and checks.
3. `sitebox-design` → `references/writing.md` — copy passes (again, after edits).
4. Final `start` → `health` → logs clean (no stack traces).
5. Static self-check: CSS braces balanced (command in step 7), no icon webfont/CDN `<link>`, no `src="https://…"` images.

A site is done when all five pass, not when it renders once.

---

## Redesign mode

Redesigning means improving, not replacing. The original has content and structure worth preserving until proven otherwise.

1. **Audit first.** Load the current page/folder. Run `sitebox-design` → `references/audit.md` and `references/performance.md`. Note what already works — those are constraints, not casualties.
2. **Preserve URLs.** Static sites break when files are renamed. Keep old paths working: leave files in place, or keep `<page>.html` / `<page>/index.html` structure so clean URLs still resolve.
3. **Preserve content, restructure presentation.** Real copy, data, and assets survive; layout, type, color, and spacing are fair game.
4. **Re-plan tokens** with `sitebox-design` — do not iterate on the old palette one hex at a time.
5. **Diff deliberately.** Before replacing a section, say (to yourself or in the commit) what improves and what is removed. Removals of content need a reason.
6. **Re-verify.** Redesigns are where 404s and dark-mode breakage hide: run the full quality gate and click every nav link.

For "make it match this reference image/site", extract the reference's principles (type scale, spacing rhythm, structure) and rebuild in the site's own voice. Do not copy pixel-for-pixel and do not paste reference CSS wholesale.

---

## Debugging

SiteBox is local and zero-dependency; almost every failure is one of a handful. Quick triage (full playbook: `sitebox-config` → `references/troubleshooting.md`):

| Symptom | First thing to check |
|---------|---------------------|
| `start` → `ok:false, process exited (EADDRINUSE)` | Port taken. `GET /api/ports/check?port=N`, then update the site with a free port and start again |
| `start` → `ok:false, server.js not found` | Folder moved/deleted or `path` wrong. `GET /api/sites` shows `_stale: true`; fix path or delete the entry |
| `start` → `ok:true` but health offline | `PORT` env mismatch — site listens on a hardcoded different port, or `process.env.PORT` ignored |
| Blank page after 200 | JS error or content in the wrong file; open `/` and read the response body, check `<script>` paths |
| CSS/asset 404 | Path case sensitivity and missing files; every referenced asset must exist in `public/` |
| Fonts missing / layout shifts offline | Google Fonts unreachable — self-host woff2, see `references/performance.md` |
| Logs are empty | Only processes started by the dashboard capture logs; start via API first |
| Page partly unstyled — styles stop after a point | Unclosed `}` in `<style>`; every rule after it is discarded. Count braces (step 7), don't eyeball |
| Columns squeezed, or the page scrolls sideways | Container `max-width` is smaller than the column sum, or a flex column lacks `min-width: 0` |
| Icons render as blank boxes or missing glyphs | An icon webfont or CDN icon set failed to load — inline the SVG instead |
| Images work in the editor but 404 on the running site | The image is hotlinked; download it into `public/` and reference the local path |

Read `GET /api/sites/:id/logs?lines=200` before guessing. The last `[err]` line is usually the answer.

---

## Rules

- Each site = its own directory: `sites/<id>/server.js` + `sites/<id>/public/`.
- Self-contained: no shared deps, no build step, vanilla HTML/CSS/JS — and no external runtime dependencies (no CDN scripts, no icon webfonts, no hotlinked images). Vendor everything into `public/`.
- Every `<style>` block is syntactically valid (balanced braces) before you verify.
- `process.env.PORT` with fallback, `0.0.0.0` binding.
- Never commit new site folders (`.gitignore` blocks `sites/*` except the examples).
- Never commit changes to `dashboard/data/sites.json` — it is runtime state and must keep only `example-notes` and `example-clock` in git. See `sitebox-config`.
- Register via API (`POST /api/sites`), not by hand-editing JSON, so validation and `created` date stay consistent.

## Reference files

- `references/performance.md` — budgets, font/image strategy, verification commands
- `sitebox-design` → `references/brand-mark.md` — logo/brand-mark workflow, SVG rules, favicon
- `sitebox-config` → `references/troubleshooting.md` — full failure playbook and log interpretation
- `sitebox-design` → `references/audit.md`, `references/writing.md` — quality gate details
