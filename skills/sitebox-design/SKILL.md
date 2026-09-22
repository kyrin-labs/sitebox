---
name: sitebox-design
description: Design system and visual quality guide for SiteBox sites. Use when planning or reviewing the look of a SiteBox site — choosing colors, typography, layout, or spacing; designing or reviewing a logo/brand mark and favicon; auditing accessibility and UX; rewriting or humanizing site copy (humanizer/deslop passes); improving an existing design during a redesign; or when a page renders partially unstyled after a CSS syntax error. Defines the quality floor every SiteBox site must meet.
metadata:
  author: sitebox
  version: "2.1.0"
  updated: "2026-09-22"
---

# SiteBox Design System

Every site built for SiteBox meets this quality floor. This is not optional styling advice — it is the standard.

**Companions:** `sitebox-create` runs this skill as its design pass and quality gate; `sitebox-config` handles the dashboard lifecycle. This skill owns the *what it should look like*; do not duplicate its tables elsewhere.

## Core principle

> Make it look like a real product, not a generated demo.

## Design process

### Before coding

1. **Understand the subject** — what is this site about? Who visits it?
2. **Pick a mood** — professional, warm, minimal, bold, editorial, playful.
3. **Define tokens** — colors, typography, spacing. Write them down as CSS custom properties.
4. **Plan layout** — one sentence: "left-aligned editorial with hero + 3 sections".
5. **Plan the mark** — one sentence for the logo/brand-mark idea. See [Logo & brand mark](#logo--brand-mark).
6. **Review** — would this plan look like something on a real website? If it could be any site, change something before coding.

When you explore alternatives (mark concepts, layout variants), build them in a scratch page, pick one, then **delete the scratch page before shipping**. `*-concepts.html` previews are never part of the site.

### While coding

- Build section by section. After each, ask: does it look intentional?
- Test at 375 px width.
- Check contrast as you pick colors, not after.

### After coding

- Re-read the anti-pattern list below and fix every match.
- Verify no placeholder text remains, all links/assets resolve, focus states are visible.
- Run the full executable checklist: `references/audit.md`.
- Run the copy passes: `references/writing.md`.

---

## Typography

### Font selection

| Site type | Headlines | Body |
|-----------|-----------|------|
| Editorial / literary | Playfair Display + Noto Serif Thai | Inter + Noto Sans Thai |
| Modern / tech | Inter + Noto Sans Thai | Inter + Noto Sans Thai |
| Corporate | Space Grotesk | Inter |
| Creative | Sora or DM Serif Display | Inter |

These are starting points, not defaults. Pick a family because it fits the subject, and give a reason if you deviate.

### Type scale

```
h1: 2.5rem  (40px) — page title, hero
h2: 1.8rem  (29px) — section headings
h3: 1.3rem  (21px) — card titles, subsections
body: 1rem  (16px) — paragraphs
small: 0.85rem (14px) — captions, metadata
```

### Rules

- Line length: 60–70 characters (never above 80).
- Line height: 1.6 for body, 1.2–1.3 for headings; serif body gets slightly more.
- Letter spacing: -0.01em for large headings only.
- Font weight: 400 body, 600–700 headings.
- Max 2 families, max ~4 files total. Self-host for offline sites (see `sitebox-create` → `references/performance.md`).
- Do not accent a single word in a headline with bold/italic/color; do not use all-caps labels.

---

## Color

### How to choose

1. Start from the subject's world — what colors does this subject actually use?
2. Pick 1 accent that could only belong to this site.
3. Build neutrals around it.
4. Verify contrast ratios (table below).

### AI color tells (avoid these)

| Color | Hex | Why it's a tell |
|-------|-----|-----------------|
| Purple / violet | #7c3aed, #6c3ce0, #8b5cf6 | The #1 AI accent. Every generated page picks purple. |
| Indigo | #4f46e5, #6366f1 | Close second. |
| Teal / cyan | #06b6d4, #0891b2 | Default "tech dashboard" accent. |
| Blue→purple gradient | any | The default AI gradient. |

Pick from the real world instead: deep green (#0e7c6b, #166534), warm amber (#92400e, #b45309), deep ocean (#1e3a5f, #1e40af), earth red (#9a3412, #b91c1c), olive (#4d7c0f, #3f6212), slate (#475569, #334155).

**Test:** if you can swap your accent with another site's accent and nobody notices, it's too generic.

### Contrast requirements (WCAG AA)

| Element | Minimum ratio |
|---------|---------------|
| Body text on background | 4.5:1 |
| Large text (18 px+) on background | 3:1 |
| Interactive elements (borders, icons) | 3:1 |

### Palette template

```css
:root {
  --ink:       #1a1a2e;  /* primary text */
  --ink-soft:  #4a4a5a;  /* secondary text */
  --ink-muted: #8a8a9a;  /* captions, metadata */
  --bg:        #fafaf9;  /* page background */
  --surface:   #ffffff;  /* cards, elevated areas */
  --accent:    #2563eb;  /* links, buttons, highlights */
  --border:    #e5e5e5;  /* dividers, borders */
}
```

Use custom properties, not repeated hex values, so a theme change is one edit.

**A theme is a complete token set, not a new background.** Redefine *every* token — background, surface, text, muted text, border, hover, input, overlay — for each theme you offer. A token you forget falls back to the other theme and ships a half-styled page (a white card on a dark background). When you add a theme, check its token list against the list you already have; nothing may be missing.

---

## Layout

### Principles

- **Left-aligned** for editorial content (reads naturally). Center only short hero text (3–5 words).
- **Generous whitespace** — 4–6 rem between sections.
- **Max-width**: 1100–1200 px for content; text blocks 60–70 ch.
- **Budget multi-column width before you set `max-width`.** Add the columns plus gaps first — e.g. 260 + 600 + 350 = 1210 px needs a container of ~1280 px or more. A container narrower than the sum squeezes the columns or forces a horizontal scrollbar.
- **`min-width: 0` on flex children** that hold text or media, or the column refuses to shrink below its content and overflows the parent. Pair it with `flex: 1` for the column that should absorb the extra width.
- **Anchor an overlap to its own container, not to a magic offset from a sibling.** `top: -68px` breaks the moment the sibling's height changes; a `bottom`/`inset` value measured from the container keeps working.
- Single column on mobile, 2–3 columns on desktop.
- Structural devices (borders, numbering, eyebrows) must encode real information. Numbered steps are only for actual sequences.

### Spacing scale

```
0.25rem — tiny gap        1.5rem — card padding
0.5rem  — small gap       2rem   — section padding
1rem    — element spacing 3rem   — large gaps
                          4–6rem — section separators
```

Only multiples of 0.25 rem. If it isn't on the scale, it's a mistake.

---

## Components

### Navigation
- Sticky header, clean background, 64 px height.
- Brand left, links right. Mobile: hamburger menu.
- Current page indicated (underline, weight, or color).

### Cards
- Minimal borders (1 px) or none; all cards one radius and one shadow, or none.
- Consistent padding (1.5 rem) and hierarchy: title > description > metadata.
- Hover: subtle border or shadow change, nothing bouncy.

### Buttons
- Primary filled and high contrast; secondary outline/ghost.
- 44 px minimum touch target. Label says what happens ("Save changes", not "Submit").
- Visible `:focus-visible` ring. Same name through the whole flow (button "Publish" → toast "Published").

### Icons
- One set, one style. Use inline SVG (Lucide geometry is the house style) and copy the `<path>` into the markup.
- Never load an icon webfont or icon-set CSS from a CDN (`lucide-static`, Font Awesome, etc.). It is render-blocking, breaks offline, and violates the zero-dependency rule — the failure mode is blank boxes where icons should be.
- Decorative icons are `aria-hidden="true"`; icon-only buttons carry an `aria-label`.

### Interactive states
- Actions that mean different things must look different. If reply, repost, and like all render in the same color, the row reads as one blob — give each action its own token and reuse it everywhere.
- A toggled action must visibly differ from its untoggled state (outline → filled, muted → accent) and be reversible. "On" and "off" that look alike is a bug.
- Status marks (badges, counters, dots) stay consistent across the whole product; don't restyle the same signal per page.

### Footer
- Simple, consistent with the header. Links + copyright. No sitemap dumps.

---

## Logo & brand mark

Every site ships one mark that survives at 16 px and reads as *this* subject. Plan it with the tokens, not after them. Full workflow, SVG rules, and pass criteria: `references/brand-mark.md`.

- **Derive it from the subject.** A bicycle shop, a research lab, and a bakery must not produce the same mark. A circle, rounded square, or monogram-in-a-badge is what you draw when you haven't decided.
- **One idea, one accent.** Wordmark plus a single accented character or dot beats wordmark + badge + tagline. Weight contrast (a heavy initial, a lighter remainder) is already the design — don't stack color on top of it.
- **Test the hard cases before shipping:** legible at 16 px, works on light *and* dark (use `currentColor`), renders without a webfont.
- **Ship it as inline SVG**, with any type outlined to paths unless it is a metric-safe system stack, and derive `public/favicon.svg` from the same geometry.

---

## Performance as a design constraint

Design choices decide performance before any code is optimized:

- Max 2 font families / 4 files; a display font for one word is a cost with no benefit.
- Every image has an aspect ratio; no stock hero photos (also an anti-pattern).
- One bold moment per page, then restraint (Chanel rule: before shipping, remove one accessory). Motion is a design element, not decoration.
- No gradient washes, no glassmorphism layers, no icon fonts.

Full budgets and checks: `sitebox-create` → `references/performance.md`.

---

## Copy and voice

Words are design material — same care as spacing and color. Before the design is "done", run both passes in `references/writing.md`:

- **Humanizer** — specific, active, plain language; consistent vocabulary; Thai that reads Thai, not translated English.
- **Deslop** — remove AI tells: opener clichés ("ในยุคที่...", "in today's fast-paced world"), buzzwords (seamless, elevate, ปลดล็อก, ยกระดับ), rule-of-three filler, generic headlines, fake numbers, decorative em-dash labels.

Short version: every written element does exactly one job; errors explain what happened and how to fix it; empty states invite action.

---

## Accessibility / UX

The checklist below is the summary. The executable audit — with checks that can be run by an agent without a browser, report format, and fix patterns — is `references/audit.md`.

- [ ] Semantic HTML: `header`, `main`, `nav`, `footer`, `article`, `section`
- [ ] Heading hierarchy h1 → h2 → h3, no skips, exactly one h1
- [ ] `lang` attribute matches the content language (`lang="th"` for Thai sites)
- [ ] Alt text on all meaningful images; empty `alt=""` for decorative
- [ ] Form labels associated with inputs; errors next to the field
- [ ] Visible focus states; full keyboard navigation; skip-to-content link
- [ ] Contrast passes WCAG AA (table above)
- [ ] `prefers-reduced-motion` respected — and its block is syntactically closed (see below)
- [ ] Stylesheet parses cleanly: every `{` has a `}`. One unclosed brace silently discards every rule after it and the page ships half-styled
- [ ] Icons are inline SVG from one set and images are local files in `public/` — no icon webfont, no CDN, no hotlinked `src="https://…"`
- [ ] Every declared theme defines the full token set (no token left to fall back)
- [ ] Touch targets ≥ 44 px; 16 px minimum body text; no horizontal scroll at 375 px
- [ ] Page works at 200% zoom

---

## Anti-patterns (never)

1. Gradient backgrounds without a functional purpose
2. Glassmorphism (blur + transparency) as a default
3. Centered everything — left-align content
4. ALL-CAPS labels everywhere
5. Feature grids (icon + title + sentence × 3, equal weight)
6. Unnecessary animations — fade-in on every section, hover transitions on every card
7. Stock photos as hero images
8. Template layouts that could be any site
9. Placeholder text or fake data
10. Generic AI colors — purple (#7c3aed), indigo (#4f46e5), blue→purple gradients
11. Emoji as icons (use inline SVG, Lucide style)
12. Borders/shadows on every element
13. Single-word accent in headlines (one word bold/colored)
14. Numbered markers (01 / 02 / 03) when content isn't sequential
15. Copy AI tells — see `references/writing.md`
16. Icon webfonts or CDN icon sets (Lucide/Font Awesome CSS) — inline the SVG instead
17. Hotlinked third-party images — download them into `public/`
18. A circular monogram as the default logo, with no reason from the subject
19. Magic-number offsets copied from another element's size
20. A theme that redefines only background and text, leaving surfaces, borders, and hovers to fall back

---

## Responsive breakpoints

```
Mobile:  0–640px    — single column, stacked
Tablet:  641–1024px — 2 columns where appropriate
Desktop: 1025px+    — full layout
```

### Mobile requirements
- Touch targets ≥ 44 px, body text ≥ 16 px
- Hamburger or bottom nav; no horizontal scroll
- Hero must not push all content below the fold on a 375 px screen

---

## Redesign principles

When improving an existing site (see `sitebox-create` → Redesign mode):

1. Audit before touching pixels — note what already works.
2. Keep URLs and content structure stable; rename nothing that is linked.
3. Re-plan tokens as a system; don't tweak hex-by-hex.
4. Spend boldness in one place; the rest stays disciplined.
5. Re-run the audit and copy passes after every structural change.
6. Re-check that the stylesheet still parses (balanced braces) and that the mark still works at 16 px on both themes — breakage hides in the surrounding CSS you didn't touch.

---

## Reference files

- `references/audit.md` — executable accessibility/UX audit with report format
- `references/writing.md` — humanizer and deslop passes with before/after examples
- `references/brand-mark.md` — logo/brand-mark workflow, SVG rules, favicon, pass criteria
