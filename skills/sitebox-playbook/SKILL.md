---
name: sitebox-playbook
description: Operator taste and field notes from every SiteBox site built so far. Read this before designing a page or writing its copy.
---

# SiteBox playbook — what the operator actually wants

This is not the mechanical contract (that is `sitebox-engine`) and not the layout
doctrine (that is `sitebox-design`). This is the taste and the scar tissue: what
has been asked for, what got rejected, and what keeps breaking. Read it before
you choose a palette, before you write a headline, and before you say "done".

## 1. The operator, in ten rules

1. **Thai first.** UI labels, copy, and the report are Thai. English only for
   product names, codes, and technical terms. Never mix scripts inside a word.
2. **Light by default.** Content sites, archives, portfolios and landing pages
   are light (paper, cream, off-white). Dark only when the product itself is
   dark: a chat app, a game, a media player. Do not pick dark to look modern.
3. **No purple, violet or indigo.** That gradient is the AI tell and has been
   rejected. Use real-world palettes: paper + ink, forest + brass, clay + sand,
   slate + rust. Pick one accent and keep it.
4. **Real content or none.** No lorem, no "ยินดีต้อนรับสู่เว็บไซต์ของเรา", no
   invented statistics without a source. If the number is not known, write a
   sentence that does not need one.
5. **Copy must read like a person wrote it.** Short sentences, concrete nouns,
   specific dates and names. If a paragraph could appear on any other site on
   the internet, delete it and write the actual thing.
6. **Density, not air.** He notices "ช่องว่างระหว่าง component มันขาดๆเกินๆ" and
   "จัดวางแปลกๆ" before he notices anything else. Space things on a scale, not
   by feel: 4/8/12/16/24/32/48/64. No section floating in 200px of nothing.
7. **Every control must work.** A button that does nothing, a dialog with a
   transparent backdrop, a hover state that turns text white on white — all of
   these have been reported as bugs, because they are bugs.
8. **Clones are clones.** "ให้เหมือน Steam 100%", "notion like มากกว่านะ" —
   when he asks for a clone, rebuild the product's own surface: the header, the
   cards, the density, the interactions, the empty states. A recoloured
   skeleton with different content is a rejection.
9. **New brand, same feel.** For a clone, invent the name and the logo; never
   reuse the reference's mark or copy its paragraphs verbatim.
10. **Act, then report in 3-6 lines.** Do not interview him, do not ask
    permission in `all-in`, do not answer with a question after "ทำเลย".

## 2. What "finished" means

A page is finished when all of this is true, not when the HTML exists:

- `sitebox_check` green (no fail; skips are explained).
- `sitebox_verify` green on a running site.
- `sitebox_probe` at width 500 on **every route you touched, in one call**:
  `overflow` empty, `clipped` empty, `small_text` empty.
- A screenshot at 1440 and one at 500 looked at, not just captured.
- The primary interaction exercised: a filter filters, a tab switches, a form
  submits, a search returns and clears an empty state.
- The report names the port, the pages, the check counts, and the one thing
  that is still weak.

Measured: a five-page build that skipped the probe step came back with 6
corrupted characters, 8 labels at 11.8px and a horizontal overflow. None of it
was visible in the HTML diff.

## 3. Archetypes that have worked

**Editorial / landing** (lumen-press, folio, relay)
Paper background, one serif display face for Thai headings, sans for body,
a 64-72ch measure, 3-5 sections, a stat row, one primary CTA. Hero carries a
real photograph, not a gradient. Footer closes the page.

**Product clone** (skytree = Steam, prakai = Instagram, nuanjai = character chat)
Sticky product header, dense card grid or two-column app shell, real public
data, real photography, working interactions (like, follow, send, tab switch).
The shell is the product: sidebar, feed, composer, bottom nav on mobile.

**Archive / registry** (sealing-registry, seal-incident-archive)
A masthead that looks institutional, a level ladder or timeline, an index page
with filter + search, detail pages per record, monospace codes, hairline rules.
Editorial serif for titles, muted paper palette, no card shadows.

**Docs site** (doku manual)
Left sidebar with sections, right-side on-page nav, code blocks with copy,
search, previous/next. Body text 16px, line-height 1.7, max 72ch.

**App-backed site** (`serve = "node"`)
The page is a client: `/api/chat`, `/api/state`, JSON store under `data/`.
The key stays in the server; the browser never sees it. Listen on `0.0.0.0`.

## 4. Layout defects that keep coming back

| Symptom | What it really is | Fix |
|---|---|---|
| "ช่องว่างขาดๆเกินๆ" | spacing by eye, not by scale | one spacing scale, section padding 48-80px, component gaps 8/12/16 |
| dialog looks transparent / search not centred | overlay is missing or `position:absolute` | fixed inset 0, `display:flex/place-items:center`, scrim `rgba(0,0,0,.45)` |
| "ดูเหลี่ยมๆ" | radius 0-2px, borders too dark, no elevation | 8-12px radius, 1px hairline, one soft shadow for overlays only |
| hover text disappears | white text on white hover | check every `:hover` pair; use the surface colour + accent border |
| icons look broken | emoji or mismatched weights | one icon set (lucide-style), 1.5-2px stroke, 18-20px |
| Thai text feels cramped | line-height 1.4 and 16px | 17-18px body, line-height 1.65-1.8, `word-break: normal` |
| hero is a purple gradient | no real image | real photo via `sols_asset`, or an illustrated rule/diagram |
| content stops at the fold | too few sections | 4+ sections with real substance, not filler |

## 5. Copy that reads human (Thai)

- Lead with the concrete thing: a date, a name, a number, a place.
- One idea per sentence. Cut "ซึ่ง", "อีกทั้ง", "นอกจากนี้" unless spoken.
- No marketing triplets ("รวดเร็ว ปลอดภัย มืออาชีพ").
- No rhetorical questions as headlines.
- Empty states say what to do next, in the operator's voice.
- Numbers stay consistent across pages; if the archive says 1,234 records, the
  filter counts must add up to 1,234.

Rejected line: "ยินดีต้อนรับสู่หอจดหมายเหตุแห่งชาติของเรา ที่ซึ่งเรื่องราว..."
Accepted: "หอรับเรื่องแรกเมื่อปี 2499 ตอนนี้มี 9 เหตุการณ์ และ 3 เคสที่เปิดให้อ่านฉบับเต็ม"

## 6. Reading the feedback

He reports symptoms, not causes. Translate before you patch:

| He says | It means | Look at |
|---|---|---|
| "ขาดๆเกินๆ" | spacing and hierarchy are improvised | one spacing scale, section rhythm, alignment between blocks |
| "เหลี่ยมๆ" | corners/borders too hard, no depth | radius 8-12px, hairline borders, scrim on overlays |
| "แปลกๆ" (about a control) | it is off the grid or misaligned | header cluster, `position`, flex gaps |
| "พัง" | an interaction or state is broken | hover colours, empty state, submit, autoplay, dialog mount |
| "ไม่เหมือน X" | fidelity, not colour | rebuild the shell: header, cards, density, nav, motion |
| "อ่านยาก" | contrast, size, or line-height | 17px+, 1.7 line-height, 4.5:1 contrast |
| "มึน" (about copy) | too abstract or too long | concrete nouns, one idea per sentence |

Autoplay is a defect, not a feature: a media element starts paused and only
plays on an explicit click; a route change stops the previous player.

## 7. Visual defaults

- Type: body 17px/1.7; small labels 12.5-13px (never below 12); h1 `clamp(2rem, 4vw, 3.25rem)`.
- Measure: 64-72ch for prose, 1200-1240px max container, 20-24px page gutter.
- Color: one accent, one neutral scale, one paper tone. Contrast ≥ 4.5:1 for body.
- Radius: 8-12px cards, 999px pills, 4px inputs. Shadows only on overlays.
- Motion: 150-200ms ease, transform/opacity only; no parallax on content.
- Focus: visible 2px outline in the accent; never `outline: none` alone.

## 8. Check catalog traps

- `SB002` fails on any CDN `<link>` — including Google Fonts. Self-host the
  font in `public/assets` or use a system stack.
- `SB003` needs width and height on every `<img>` with a `src`.
- `SB020` rejects placeholder sources (picsum, unsplash, pravatar) and
  `Math.random()` in output — placeholders are for drafts only, never shipped.
- `SB005` fails on a referenced asset that does not exist. Download it with
  `sols_asset`; never hotlink.
- Generated site (`tools/build.js`, pipeline)? Edit the **source**, never
  `public/` — the next build overwrites it.
- A site that was already red before your patch: compare the fail set before and
  after; only introduced failures are yours.

## 9. Working with bees

- **Wave 1** researches: structure, facts with URLs, copy drafts, tokens.
- **Wave 2** reviews: quote wave-1 findings in the brief and ask for
  contradictions, number checks, weak sentences.
- Mechanical sweeps are `effort: "quick"` (scan files, count, list selectors).
- The board (`bee_note` / `bee_notes`) stops duplicate work; it is not a chat.
- Never let a bee edit: it reports, you patch.

## 10. Pre-flight and pre-report checklists

Before the first patch:
- [ ] Read this playbook; read `sitebox-design` for layout, `sitebox-create` for
      a multi-page build.
- [ ] Decide the archetype, the palette (one accent), and the copy angle.
- [ ] One stylesheet; pages link it. No inline duplicate CSS per page.

Before reporting:
- [ ] `check` + `verify` green.
- [ ] probe at 500 for all changed routes in one call: no overflow, no clipped,
      nothing under 12px.
- [ ] Looked at 1440 and 500 screenshots.
- [ ] The report is 3-6 lines: what changed, what the numbers say, one next step.

## 11. Never

- Never say "done" because the file was written.
- Never claim a check passed that you did not run.
- Never ship an emoji as an icon, a lorem paragraph, or a dead link.
- Never use a purple gradient, a glassmorphism card, or a 40px shadow.
- Never edit `public/` of a generated site.
- Never reuse the reference product's name, logo or copy when cloning.
