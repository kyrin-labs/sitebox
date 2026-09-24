# Accessibility & UX Audit

Run this before declaring any SiteBox site done, and after every redesign. The checklist is executable — do the checks, don't just read them.

Lineage: the audit style and many rules follow Vercel's Web Interface Guidelines (100+ rules, `web-design-guidelines` skill) and Anthropic's `frontend-design`. When network is available, re-fetch the living guideline list (`https://raw.githubusercontent.com/vercel-labs/web-interface-guidelines/main/command.md`) and apply it too; the table below is the offline-safe subset tuned for SiteBox.

## How to run

1. Read the site's HTML/CSS/JS files (`sites/<id>/public/`). Most findings come from a careful read.
2. Fetch the running page (`curl -s http://localhost:<port>/`) and each linked asset; a 404 is a finding.
3. **Run the browser pass — do not skip it when a browser exists.** Reading CSS cannot tell you how a page *looks*; a text check sees `aspect-ratio: 16/9` and passes while the browser ignores it. With headless Chromium available on both machines, the browser pass is part of the audit, not an extra. `sitebox-verify` owns the tooling (`render-check.js`); at minimum: measure every sized image's drawn ratio against its natural ratio, tab through the page, screenshot at 1440 px and 375 px, capture console errors, force reduced motion.
4. Report findings as `file:line — [severity] finding — suggested fix`. Severity: **blocker** (unusable/inaccessible), **major** (quality gate fails), **minor** (polish).

Example finding:

```
public/index.html:42 — [major] nav links have no focus style — add :focus-visible outline using --accent
```

## CSS & build health (check first — a parse error invalidates everything below)

A CSS parse error makes the later checks lie: the markup is right, the rules are right, and the page
still renders half-styled.

- [ ] **Braces balance.** Count `{` and `}` in every `<style>` block; they must be equal. One missing
  `}` discards *every* rule after it, and the browser reports nothing.
  `node -e "const s=require('fs').readFileSync(process.argv[1],'utf8');const o=(s.match(/{/g)||[]).length,c=(s.match(/}/g)||[]).length;console.log(o===c?'balanced':'UNBALANCED '+o+'/'+c)" sites/<id>/public/index.html`
- [ ] **No orphan declarations.** A stray property list ending in `}` at the top level (e.g.
  `-webkit-font-smoothing:antialiased}` with no selector) is a leftover from a botched fix — still a
  parse error, still remove it.
- [ ] Media queries (`prefers-reduced-motion`, breakpoints) are individually opened and closed.
- [ ] **No external runtime dependencies.** No `<script src="https://…">`, no icon-webfont or
  `lucide-static` CSS link, no `src="https://…"` image. Icons are inline SVG; images live in
  `public/`. Hotlinked images 404 at runtime because the host blocks referrers.

## Image geometry (check second — a crop invalidates every visual judgement after it)

A cropped image makes the rest of this audit lie: the layout is "correct", the tokens are "correct",
and the picture is still wrong. Nearly shipped 27 %-cropped thumbnails on every card for a full
session while the text checks reported success.

- [ ] **Every box that shows an image gets its height from that image's ratio.** No fixed `height`,
  no `max-height` on the same element as `aspect-ratio`, no `align-items: stretch` parent.
- [ ] **No `aspect-ratio` on a non-replaced inline box.** `<span class="thumb">` with
  `aspect-ratio: 16/9` and no `display` is a silent no-op — the box takes its height from the line
  box and `object-fit: cover` then eats ~27 % of the width.
- [ ] **`aspect-ratio` and `max-height` are never on the same box.** Together they make the browser
  derive the *width* from the clamped height, so the box is the wrong shape at every viewport, not
  just small ones.
- [ ] **Measured in a browser, not read from CSS.** For every sized `<img>`: `drawn ratio` vs
  `naturalWidth/naturalHeight` must agree within **1 %**. This is the check that catches all of the
  above at once.
- [ ] **Every image actually loads.** `img.complete && !img.naturalWidth` is a **failure** — do not
  `continue` past it. A render check that skips broken images reports a green page full of 404s.
- [ ] **Files keep their source ratio.** Compare each served file against the ratio recorded when it
  was downloaded; a mismatch means a crop was baked in and no CSS can undo it.
- [ ] **No horizontal overflow** at 1440 px and at 375 px.

## Accessibility checks

### Structure

- [ ] One `<h1>` per page; no skipped heading levels.
- [ ] Landmarks present: `header`, `nav`, `main`, `footer`; only one `main`.
- [ ] `<html lang="...">` matches the content language (`th` for Thai sites, `en` for English).
- [ ] Page `<title>` is unique and descriptive (not "Home").
- [ ] Skip-to-content link first in the tab order (or a single nav that never traps focus).

### Text & images

- [ ] Body text ≥ 16 px; line length 60–70 ch; line-height ≥ 1.6.
- [ ] Contrast: body text ≥ 4.5:1, large text ≥ 3:1, interactive borders/icons ≥ 3:1. Compute, don't eyeball. If a color fails, change the token, not one element.
- [ ] Every meaningful `<img>` has specific alt text (what it conveys, not "image"); decorative images use `alt=""`.
- [ ] Inline SVG icons are `aria-hidden="true"` when decorative; icon-only buttons have `aria-label`.
- [ ] No text baked into images.

### Interaction

- [ ] Every interactive element is reachable and operable by keyboard (Tab, Enter/Space, Escape for overlays).
- [ ] `:focus-visible` styling exists, is high contrast, and is never removed (`outline: none` without replacement is a blocker).
- [ ] Buttons are `<button>`, links are `<a href>`; no clickable `<div>`.
- [ ] **No interactive element nested inside another.** `<button>` inside `<button>`, or `<button>` inside `<a>`, is invalid: the browser silently closes the outer element and the layout breaks. A DOM stub will not catch this — it is not a parser. Walk the markup with a tokenizer, or check it in a real browser.
- [ ] Touch targets ≥ 44×44 px with spacing.
- [ ] Forms: every input has an associated `<label>`; errors are text next to the field, not color-only; `autocomplete` set on personal fields.
- [ ] `target="_blank"` links carry `rel="noopener"`.
- [ ] Modals/dialogs: focus moves in, Escape closes, focus returns.

### Motion & media

- [ ] `@media (prefers-reduced-motion: reduce)` disables non-essential motion.
- [ ] No autoplaying audio/video with sound; any autoplay is muted and pauses on interaction.
- [ ] No flashing content; animations don't block reading.

### Zoom & viewport

- [ ] No `user-scalable=no` / `maximum-scale=1`.
- [ ] At 200% zoom and at 320 px width: no horizontal scroll, no clipped content.
- [ ] Text remains readable if custom fonts fail (fallback stack present).

## UX checks

- [ ] Navigation: current page is indicated; every link goes somewhere real (no dead `href="#"`).
- [ ] **No stray glyphs in your own UI copy.** Thai and English copy must not contain Chinese characters (`\u4e00-\u9fff`) or Cyrillic that nobody typed on purpose — a model borrows them in through transliterated words and the owner notices immediately. **Scope the scan to the UI copy only:** real scraped content legitimately contains Japanese, Korean, Chinese and Cyrillic, and a whole-file scan flags it as a false positive. Never scan `-` and conclude clean if the copy block was not located — a guard over an empty sample is not a pass.
- [ ] **Real content is verbatim.** Scraped titles, captions and comments are never translated, shortened, or "improved". If a title is in Japanese, it stays in Japanese.
- [ ] One primary action per screen; the label states the outcome ("Save changes").
- [ ] The same action keeps the same name through the flow.
- [ ] Empty states invite action (what to do next), error states say what happened and how to fix it.
- [ ] Loading states exist for anything async (even on localhost, be honest).
- [ ] Forms validate inline; submitting invalid data never loses the user's input.
- [ ] Dark mode (if offered): **every token** is defined in every theme — background, surface, text, muted, border, hover, input, overlay; a missing token falls back and ships a half-styled page. Both themes pass contrast; the toggle state persists; no hardcoded colors outside tokens.
- [ ] Dates/numbers use the user's locale (`Intl.DateTimeFormat`, `Intl.NumberFormat`).
- [ ] 404 behavior is deliberate: the server returns 404 for missing assets, and multi-page sites link back home.
- [ ] No layout shift on load: images have dimensions, fonts have fallbacks, no late-injected banners.

## Brand mark

- [ ] The mark is derived from the subject, not a default circle/monogram. If the mark is the hard part, follow the workflow in [brand-mark.md](brand-mark.md): explore 3–5 concepts, render them at 16/32/64/256 px on light and dark, and look before choosing.
- [ ] Legible at 16 px (favicon size) and works on light *and* dark backgrounds (`currentColor`).
- [ ] Inline SVG; no webfont-dependent `<text>` (outline the type, or use a metric-safe system stack).
- [ ] `public/favicon.svg` exists, is referenced from `<head>`, and returns 200.
- [ ] Concept/scratch preview pages (`*-concepts.html`) are deleted, not shipped.
- [ ] No indigo `#4f46e5` / violet `#7c3aed` / blue→purple gradient anywhere in the mark — these are the default AI palette and they show up in generated marks most often.

## Performance cross-check

Run `sitebox-create` → `references/performance.md` budgets in the same pass. Common audit findings that are also performance bugs: oversized images without dimensions, 4+ font files, render-blocking scripts, `transition: all`.

## Fix patterns

| Finding | Fix |
|---------|-----|
| Removed focus outline | `:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }` |
| Low contrast secondary text | Darken `--ink-soft` until 4.5:1; don't shrink text instead |
| Icon-only button | `<button aria-label="Close">` + `aria-hidden="true"` on the SVG |
| Clickable div | Replace with `<button>` or `<a href>`; keep styles in a class |
| Motion ignoring preference | Wrap in `@media (prefers-reduced-motion: no-preference)` |
| Form error as red border only | Add text: "อีเมลไม่ถูกต้อง" / "Enter a valid email" next to the field |
| Missing image size | `width`/`height` attributes or `aspect-ratio` in CSS |
| Image cropped ~27 % | The box is a non-replaced inline element — add `display: block` |
| Image cropped ~24 % | `aspect-ratio` and `max-height` are on the same box — remove one |
| Image cropped ~39 % | A grid/flex parent is stretching the box — `align-items: flex-start` |
| Image squashed, not cropped | `width: 100%` + `max-height` — use `width: auto; height: auto; max-width/max-height` |
| The served file is cropped | The crop happened at resize time — re-run the pipeline without the crop; it cannot be undone in CSS |
| The wrong picture is served | The download cache is keyed on the filename — key it on the source URL |
| Content stops short of the right edge | A `max-width` on a wrapper that is narrower than the screen — measure the gap and remove the ceiling |
| Two bands don't share an edge | Give them one parent with one width instead of two matching numbers |
| Interactive element nested in another | Make the inner one a sibling that is positioned over the outer, not a child |

## Pass criteria

The audit passes when there are zero **blocker** findings and zero **major** findings. Minor findings may ship only if listed explicitly with a reason. Re-run after fixes — a fix that breaks another check is not a fix.

Two things do **not** count as findings when they are written down properly:

- **An accepted deviation** — a platform behaviour copied on purpose. It needs all four parts: the
  measured number, why it is worth copying, the restriction that makes it safe, and where it is
  recorded. Reported as `ACCEPTED DEVIATION`, not as a failure. See `sitebox-design` →
  [Accepted deviations](../SKILL.md#accepted-deviations).
- **A size over some number** — there is no page-size budget. Report the byte count; do not fail on it.

## The audit must be able to fail

Every check above is worthless until you have watched it go red. `sitebox-verify` owns this: for each
guard, break the thing it protects, confirm the guard fails, then restore. A `✓` that has never had a
chance to be a `✗` proves nothing — Folio had a stray-glyph check whose regex never matched and it
printed `✓ no stray CJK` for as long as the site existed.
