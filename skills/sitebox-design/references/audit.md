# Accessibility & UX Audit

Run this before declaring any SiteBox site done, and after every redesign. The checklist is executable — do the checks, don't just read them.

Lineage: the audit style and many rules follow Vercel's Web Interface Guidelines (100+ rules, `web-design-guidelines` skill) and Anthropic's `frontend-design`. When network is available, re-fetch the living guideline list (`https://raw.githubusercontent.com/vercel-labs/web-interface-guidelines/main/command.md`) and apply it too; the table below is the offline-safe subset tuned for SiteBox.

## How to run

1. Read the site's HTML/CSS/JS files (`sites/<id>/public/`). Most findings come from a careful read.
2. Fetch the running page (`curl -s http://localhost:<port>/`) and each linked asset; a 404 is a finding.
3. If a browser or Playwright is available, also: keyboard-tab through the page, screenshot at 375 px and 1280 px, capture console errors, check with reduced motion forced.
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

- [ ] The mark is derived from the subject, not a default circle/monogram.
- [ ] Legible at 16 px (favicon size) and works on light *and* dark backgrounds (`currentColor`).
- [ ] Inline SVG; no webfont-dependent `<text>` (outline the type, or use a metric-safe system stack).
- [ ] `public/favicon.svg` exists, is referenced from `<head>`, and returns 200.
- [ ] Concept/scratch preview pages (`*-concepts.html`) are deleted, not shipped.

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

## Pass criteria

The audit passes when there are zero **blocker** findings and zero **major** findings. Minor findings may ship only if listed explicitly with a reason. Re-run after fixes — a fix that breaks another check is not a fix.
