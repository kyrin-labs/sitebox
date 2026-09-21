---
name: sitebox-design
description: Design system and visual quality guide for SiteBox sites.
---

# SiteBox Design System

Every site built for SiteBox should meet this quality floor. This is not optional styling advice — it's the standard.

## Core Principle

> Make it look like a real product, not a generated demo.

## Design Process

### Before coding

1. **Understand the subject** — what is this site about? Who visits it?
2. **Pick a mood** — professional, warm, minimal, bold, editorial, playful
3. **Define tokens** — colors, typography, spacing (write them down)
4. **Plan layout** — one sentence: "left-aligned editorial with hero + 3 sections"
5. **Review** — does this plan look like something you'd see on a real website? If it looks generic, change something.

### While coding

- Build section by section
- Check each section: does it look intentional?
- Test on mobile (375px width)
- Check color contrast

### After coding

- Read the Anti-Patterns list and fix anything that matches
- Verify no placeholder text remains
- Ensure all links work

---

## Typography

### Font Selection

| Site Type | Headlines | Body |
|-----------|-----------|------|
| Editorial / literary | Playfair Display + Noto Serif Thai | Inter + Noto Sans Thai |
| Modern / tech | Inter + Noto Sans Thai | Inter + Noto Sans Thai |
| Corporate | Space Grotesk | Inter |
| Creative | Sora or DM Serif Display | Inter |

### Type Scale

```
h1: 2.5rem  (40px) — page title, hero
h2: 1.8rem  (29px) — section headings
h3: 1.3rem  (21px) — card titles, subsections
body: 1rem  (16px) — paragraphs
small: 0.85rem (14px) — captions, metadata
```

### Rules

- Line length: 60-70 characters max
- Line height: 1.6 for body, 1.2-1.3 for headings
- Letter spacing: -0.01em for large headings
- Font weight: 400 for body, 600-700 for headings
- Never use more than 2 font families

---

## Color

### How to Choose

1. Start with the subject matter — what colors does this world use?
2. Pick 1 accent color that feels unique to this site
3. Define neutrals around it
4. Verify contrast ratios

### Contrast Requirements (WCAG AA)

| Element | Minimum Ratio |
|---------|---------------|
| Body text on background | 4.5:1 |
| Large text (18px+) on background | 3:1 |
| Interactive elements | 3:1 |
| Decorative text | No requirement |

### Palette Template

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

---

## Layout

### Principles

- **Left-aligned** for editorial content (reads naturally)
- **Center-aligned** only for short hero text (3-5 words)
- **Generous whitespace** — sections breathe with 4-6rem gaps
- **Max-width**: 1100-1200px for content
- **Single column** on mobile, 2-3 columns on desktop

### Spacing Scale

```
0.25rem  — tiny gap
0.5rem   — small gap
1rem     — element spacing
1.5rem   — card padding
2rem     — section padding
3rem     — large gaps
4-6rem   — section separators
```

---

## Components

### Navigation
- Sticky header, clean background
- Logo/brand left, links right
- Mobile: hamburger menu
- 64px height standard

### Cards
- Minimal borders (1px) or no borders
- Hover: subtle shadow or border change
- Consistent padding (1.5rem)
- Clear hierarchy: title > description > metadata

### Buttons
- Primary: filled, high contrast
- Secondary: outline or ghost
- 44px minimum touch target
- Clear label (what happens when clicked)

### Footer
- Simple, minimal
- Links + copyright
- Consistent with header

---

## Anti-Patterns

These are tells of a generated/generic page. Avoid all of them:

1. **Gradient backgrounds** without functional purpose
2. **Glassmorphism** (blur + transparency) as default
3. **Centered everything** — use left-alignment for content
4. **ALL-CAPS labels** everywhere
5. **Feature grids** (icon + title + sentence × 3, all equal weight)
6. **Unnecessary animations** (fade-in on every section)
7. **Stock photos** as hero images
8. **Template layouts** that look like every other site
9. **Placeholder text** that looks fake
10. **Generic colors** (the same blue/purple gradient)
11. **Emoji as icons** (use inline SVG instead)
12. **Borders/shadows** on every element
13. **Single-word accent** in headlines (one word in bold/color)
14. **Numbered markers** (01 / 02 / 03) when content isn't sequential

---

## Content Quality

- **Real content only** — no lorem ipsum, no fake data
- **Specific headlines** — "Why we built this" not "About Us"
- **Active voice** — "Start building" not "Get started"
- **Thai primary** for user's sites, English secondary
- **Every section earns its place** — if you can remove it and the page still works, remove it

---

## Responsive Breakpoints

```
Mobile:  0-640px    — single column, stacked layout
Tablet:  641-1024px — 2 columns where appropriate
Desktop: 1025px+    — full layout
```

### Mobile Requirements
- Touch targets: 44px minimum
- Font size: 16px minimum (no pinch-to-zoom needed)
- Navigation: hamburger or bottom nav
- Readable without horizontal scroll

---

## Accessibility Checklist

- [ ] Semantic HTML elements
- [ ] Heading hierarchy (h1 → h2 → h3, no skipping)
- [ ] Alt text on all images
- [ ] Focus states visible
- [ ] Keyboard navigable
- [ ] Color contrast passes WCAG AA
- [ ] prefers-reduced-motion respected
- [ ] Form labels associated with inputs
- [ ] Skip-to-content link (optional but good)
