# Logo & Brand Mark (SiteBox delivery)

A site needs one mark that survives at 16 px and reads as *this* subject. The mark is design
material, not an afterthought bolted on at the end — plan it with the tokens and the layout.

This file is the **delivery** subset: what a mark must look like once it lands in a SiteBox site.
For the *design* work — exploring concepts and proving they survive — use the **`alicia-logo-artist`**
pi-agent skill. It renders candidates at 16/32/64/256 px on light and dark and makes you look before
you choose.

Lineage: the workflow below was distilled from the `SVG Logo Designer` skill (rknall/claude-skills),
which this project then **replaced**. That skill recommended an indigo `#4f46e5` → violet `#7c3aed`
gradient in its own examples, `<text font-family="Helvetica">`, a 100 px minimum size (a favicon is
16 px), 30–75 output files per logo, and never rendered anything to look at it. Every one of those is
an anti-pattern here — it is the clearest example in the repo of a skill that reads well and produces
the wrong thing.

## Workflow

1. **Derive from the subject, not from a shape.** Write one line: what does this site actually do,
   and what object, letter, or sound could stand for it? A bicycle shop, a research lab, and a
   bakery must not produce the same mark.
2. **Explore 3–5 concepts in one scratch page.** Build them side by side (`*-concepts.html`) so they
   are compared, not imagined. Vary the *idea*, not just the color.
3. **Render and look before choosing.** Nearly's mark is a play triangle whose three corners do not
   meet; the gap was chosen by sweeping six values and rendering all of them, not by reasoning about
   it. If you cannot see the candidates at 16 px and at 256 px, on light and on dark, you have not
   chosen yet. `alicia-logo-artist` ships the script that produces this comparison sheet.
4. **Pick one deliberately.** State the reason in one sentence ("the wordmark, because the name is
   the brand; the dot marks 'live'"). If two concepts are equally good, the simpler one wins.
5. **Refine the winner** — spacing, weight, the single accent. Then run the hard cases below.
6. **Export for the site**: the mark as inline SVG, and a `public/favicon.svg` derived from the same
   geometry.
7. **Delete the scratch page.** Concept / `*-concepts` previews are never shipped and never linked.

## No default shapes

A circle, a rounded square, or a monogram-in-a-badge is the mark you produce when you have not
decided. Use one only if the subject genuinely argues for it.

## Anti-patterns

| Anti-pattern | Why |
|---|---|
| Indigo `#4f46e5`, violet `#7c3aed`, any blue→purple gradient | The default palette of generated design. If the mark would not look out of place on an unrelated startup, it is not a mark yet |
| A `linearGradient` used to make a flat shape look designed | Gradients hide a weak silhouette. Fix the silhouette at 16 px, then decide about colour |
| `<text>` in a downloaded font | The mark reflows or falls back when the font fails. Outline the type, or use a metric-safe system stack |
| Hairlines and fine detail | They vanish at 16 px. If the detail is gone at favicon size, it was decoration |
| A mark that only works on white | Use `currentColor` or a token; test on the dark theme before shipping |
| 30–75 output files | A site needs one mark, one lockup, one favicon. Extra files are unmaintained drift |
| Presenting concepts without looking at them | A mark described in prose is not a mark. Render it |

- **One idea, one accent.** A wordmark with a single accented character or dot beats a wordmark plus
  badge plus tagline. Everything past the first accent is noise.
- **Weight contrast carries meaning.** If you split a word (a heavy initial, a lighter remainder),
  the contrast *is* the design — do not stack color on top of it.
- **Legible at favicon size.** Shrink to 16 px before committing: if the detail vanishes, it was
  decoration. Thick strokes survive; hairlines do not.
- **Works on light and dark.** Use `currentColor` for monochrome marks so the mark inherits the text
  color. A mark that only works on white is broken on a dark theme.
- **Optically centered, not mathematically centered.** Round forms want a hair of overshoot; tighten
  display-size wordmarks (`-0.02em`…`-0.04em` letter-spacing).
- **No default shapes.** A circle, a rounded square, or a monogram-in-a-badge is the mark you produce
  when you have not decided. Use one only if the subject genuinely argues for it.

## SVG rules (SiteBox)

- **Inline SVG in the page** — not an `<img>` to a remote file, and never an icon webfont. See the
  Icons rules in `SKILL.md`.
- **Do not let the mark depend on a downloaded font.** `<text>` is acceptable only with a metric-safe
  system stack; otherwise outline the type to paths so it renders identically on every machine. A
  logo that reflows because a webfont failed is a bug.
- **Size it explicitly** (`viewBox` + `width`/`height` or CSS) so it reserves space and causes no
  layout shift.
- **Give it an accessible name.** A mark that is the page's main heading can be a real `<h1>`; a
  decorative repeat in the header gets `aria-hidden="true"`.
- **Theme through tokens.** Fills reference `currentColor` or a CSS variable — never a hardcoded hex
  that ignores dark mode.

## Favicon

Every SiteBox site ships `public/favicon.svg`, referenced from `<head>`. Derive it from the mark:
keep the recognizable silhouette, drop whatever cannot survive 16 px, and check it at 16/32 px on
both light and dark browser chrome. A missing favicon is a guaranteed 404 — see `sitebox-create`.
It is also the site's identity in the dashboard: the dashboard auto-detects `public/favicon.svg`
and shows it on the site card instead of the configured Lucide icon, so a missing favicon also
means a generic icon in the launcher.

## Pass criteria

The mark passes when all of these hold:

- It is derived from the subject — you can say why in one sentence.
- **It has been rendered and looked at** at 16 px and at 256 px, on light and on dark, by you — not
  only described.
- It is legible at 16 px and works on light and dark.
- It uses at most one accent, and no indigo/violet/blue→purple gradient.
- It ships as inline SVG with no webfont dependency, plus a matching `public/favicon.svg`.
- The scratch concept page has been deleted.
