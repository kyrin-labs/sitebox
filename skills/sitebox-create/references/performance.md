# Performance Requirements (SiteBox sites)

SiteBox sites are local-first static sites. Local means latency is tiny, but the budgets still matter: they keep the site usable on weak hardware, offline, and when deployed behind a real network later.

## Budgets

Measure per page. These are ceilings, not targets.

| Metric | Budget | Why |
|--------|--------|-----|
| HTML (single page) | < 60 KB uncompressed | Everything above is content users wait for |
| CSS | < 30 KB, inline for single-page | Avoids a render-blocking request |
| JS | Vanilla only; < 20 KB, `defer` or end of body | Local tool pages rarely need more |
| Font files | Max 4 files, max 2 families, ≤ 2 weights each | Each file blocks text rendering |
| Total transfer, first load | < 500 KB excluding hero imagery | Comfortable on 3G-class links |
| Images | Every `<img>` has width/height or aspect-ratio | Prevents layout shift (CLS) |
| Time to first render (localhost) | < 300 ms | If it's slower, something is blocking |

No third-party CDN libraries (jQuery, icon fonts, CSS frameworks). Zero dependencies is a SiteBox rule, not a preference.

## Fonts

Google Fonts is allowed only when the site can require network access. The decision tree:

1. **System stack fits the design?** Use it. Fastest and never breaks offline:
   `font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", "Noto Sans Thai", sans-serif;`
2. **Offline needed or Thai text?** Self-host: put `.woff2` files in `public/fonts/`, declare `@font-face` with `font-display: swap`, and subset to the glyphs actually used when possible.
3. **Google Fonts URL?** Then: max 2 families, ≤ 2 weights each, `display=swap`, and keep the preconnect lines. Add a fallback stack with correct metrics (e.g. `font-family: 'Inter', system-ui, sans-serif;`) so offline visits don't collapse.

Anti-patterns: 5+ weights, italics you never use, a display font loaded for one word, icon fonts and CDN icon sets (inline the SVG instead — see the Icons rules in `sitebox-design`).

## Images

- SVG for icons and diagrams; `<img>` for raster.
- Always set dimensions: `width`/`height` attributes, or `aspect-ratio` in CSS. This is the single biggest CLS fix.
- Below-the-fold images: `loading="lazy" decoding="async"`.
- Hero/LCP image only: `fetchpriority="high"` and no lazy.
- No stock photos (also a design rule). If a raster image is needed, compress it — a 2 MB JPEG is never acceptable for a static site.
- Never base64-encode large images into CSS; it blocks parsing and can't be cached separately.
- **Vendor images into `public/`; never hotlink.** External hosts block referrer/localhost requests, so a hotlinked image 404s on the running site even when it loads in an editor preview. Download the file, then reference the local path.

## CSS

- Single-page sites: inline `<style>` in `<head>` — zero extra requests.
- Multi-page sites: one shared `public/style.css`; no per-page duplicates.
- Use CSS custom properties for tokens (`:root { --ink: ... }`) so dark mode and theme changes are one block.
- Animate only `transform` and `opacity`; wrap non-essential motion in `@media (prefers-reduced-motion: no-preference)`.
- **Close every block.** An unclosed `}` — most often a `@media` (breakpoints, `prefers-reduced-motion`) — silently discards every rule after it, and the page ships half-styled with no error. A fix that leaves an orphan declaration (e.g. `-webkit-font-smoothing:…}` with no selector) is still a parse error; remove it. Verify balance before starting the site:
  `node -e "const s=require('fs').readFileSync(process.argv[1],'utf8');const o=(s.match(/{/g)||[]).length,c=(s.match(/}/g)||[]).length;console.log(o===c?'balanced':'UNBALANCED '+o+'/'+c)" sites/<id>/public/index.html`
- No `transition: all` — it animates layout properties and causes jank. Name the properties.

## JavaScript

- Vanilla, no bundler. Put scripts at the end of `<body>` or use `defer`.
- No layout thrashing: batch reads (`getBoundingClientRect`) before writes (`style.x =`).
- Event delegation for lists instead of a listener per item.
- Guard optional APIs (`Intl`, `IntersectionObserver`) or provide a fallback — the site may be opened in an old browser.

## Server (server.js)

The canonical template in `sitebox-create` is intentionally minimal. Two cheap wins when a site grows:

- **Caching for static assets** — add `Cache-Control: public, max-age=300` for CSS/JS/images; keep HTML uncached (local edits should show immediately).
- **gzip for text** — Node's built-in `zlib` via `zlib.createGzip()`, content-type gated. Only bother if a page exceeds ~100 KB.

Do not add frameworks "for performance" — on localhost, fewer moving parts is faster.

## Verification

After starting the site, confirm:

1. Every page returns 200: fetch `/`, then each nav link.
2. Every referenced asset returns 200: favicon, CSS, JS, fonts, images. A 404 here is a failed quality gate.
3. No console errors. Without a browser: read the logs (`GET /api/sites/:id/logs`) and the served HTML/JS for obvious errors; with a browser or Playwright, capture console output and screenshots at 375 px and 1280 px.
4. Measure transfer size of the HTML file (any HTTP client; e.g. `curl -s -o /dev/null -w "%{size_download}"` on macOS/Linux, `curl.exe -s -o NUL -w "%{size_download}"` on Windows). Compare against the budget table.
5. Check the page with images disabled or slow-network throttling if the browser is available: text must be readable immediately.
6. Static sanity: CSS braces balance; no icon webfont/CDN `<link>`; no `src="https://…"` image (icons inline, images local in `public/`).

If a budget is exceeded, find what dominates before optimizing: usually fonts (too many files) or images (wrong format/size).
