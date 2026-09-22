# Performance Requirements (SiteBox sites)

SiteBox sites are local-first static sites. Local means latency is tiny, but the budgets still matter: they keep the site usable on weak hardware, offline, and when deployed behind a real network later.

## There is no size budget

SiteBox sites are local-first and carry real content in Thai, which is 3 bytes per character. Relay is
124 KB, Folio 185 KB, Nearly 308 KB raw — all correct, all deliberate. A ceiling that every finished
site fails only teaches an agent to ignore ceilings, so there isn't one.

What replaced it — rules that are about **correctness**, not bytes:

| Rule | Why it is not negotiable |
|------|--------------------------|
| **Measure and report the byte count** (`gzip -c public/index.html \| wc -c`) | You should know the number even though nothing fails on it |
| Serve text gzipped with `ETag`/304 and `Cache-Control: no-cache` | Turns 308 KB into 66 KB and makes edits show immediately. This is a serving rule |
| Every `<img>` has real `width`/`height` or `aspect-ratio` | Prevents layout shift, and a page that does not know an image's height will eventually be fixed with a fixed height — which is how cropping starts |
| Below-the-fold images: `loading="lazy" decoding="async"` | Local latency is tiny; the browser's decode work is not |
| Hero/LCP image only: `fetchpriority="high"`, no lazy | The one image that is allowed to be eager |
| Font files: max 4 files, max 2 families, ≤ 2 weights each | A *design* rule — a display font for one word is a cost with no benefit |
| One `main`-thread-blocking script maximum, at the end of `<body>` or `defer` | Time to first render on localhost should be under 300 ms; if it is slower something is blocking |
| No third-party CDN libraries (jQuery, CSS frameworks, icon webfonts) | Zero dependencies is a SiteBox rule, not a preference |

**When the page is large, the question is not "how do I shrink it" but "does any screen need this
before the user acts?"** Nearly moved 346 KB of comments into `public/data/comments.json`, fetched only
when a video is opened — the home page never waits for them. Relay kept everything inline on purpose,
because a failed fetch there would have meant a blank page. Both are correct **if you say which you
chose and why**.

## Fonts

Google Fonts is allowed only when the site can require network access. The decision tree:

1. **System stack fits the design?** Use it. Fastest and never breaks offline:
   `font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", "Noto Sans Thai", sans-serif;`
2. **Offline needed or Thai text?** Self-host: put `.woff2` files in `public/fonts/`, declare `@font-face` with `font-display: swap`, and subset to the glyphs actually used when possible.
3. **Google Fonts URL?** Then: max 2 families, ≤ 2 weights each, `display=swap`, and keep the preconnect lines. Add a fallback stack with correct metrics (e.g. `font-family: 'Inter', system-ui, sans-serif;`) so offline visits don't collapse.

Anti-patterns: 5+ weights, italics you never use, a display font loaded for one word, CDN icon sets (inline the SVG, or vendor the library into `public/` — see the Icons rules in `sitebox-design`).

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

- **Caching for static assets** — `Cache-Control: no-cache` plus `ETag`/304. "No-cache" does not mean
  "do not cache"; it means "revalidate first". On localhost that gives you both: an edit shows
  immediately and an unchanged file still answers 304 instead of re-downloading. A plain `max-age`
  is how a fixed image keeps rendering cropped for an hour.
- **gzip for text** — Node's built-in `zlib` via `zlib.gzipSync()`, content-type gated. Do this as soon
  as the page has content at all; it is four lines and it is the single biggest win.
- **A real 404** for a missing file. Never fall back to `index.html` for an asset path: a silent 200
  hides a broken image until a person looks at the page.
- **Path traversal guard** — resolve the path and check it still starts with `PUBLIC`.

Do not add frameworks "for performance" — on localhost, fewer moving parts is faster.

## Verification

After starting the site, confirm:

1. Every page returns 200: fetch `/`, then each nav link.
2. Every referenced asset returns 200: favicon, CSS, JS, fonts, images. A 404 here is a failed quality gate.
3. No console errors. Without a browser: read the logs (`GET /api/sites/:id/logs`) and the served HTML/JS for obvious errors; with a browser or Playwright, capture console output and screenshots at 1440 px and 375 px.
4. **Report the transfer size** of the HTML (`gzip -c public/index.html | wc -c`). Nothing fails on it — you just have to know it.
5. **In a real browser:** every sized image draws at its own ratio (no crop), no horizontal overflow at
   1440 px and 375 px, and no image fails to load. `sitebox-verify` → `render-check.js` owns this; a
   text-level check cannot see it.
6. Static sanity: CSS braces balance; no CDN `<link>`; no `src="https://…"` image; every `<img>` has
   real `width`/`height` and an asset version.

If the page is large, find what dominates before optimising: usually it is content that no screen
needs early (split it) rather than something that needs to be smaller.
