# Logo & Brand Mark

A site needs one mark that survives at 16 px and reads as *this* subject. The mark is design
material, not an afterthought bolted on at the end — plan it with the tokens and the layout.

Lineage: distilled from the `SVG Logo Designer` skill (rknall/claude-skills) and repo experience;
the workflow below is the offline-safe subset tuned for SiteBox.

## Workflow

1. **Derive from the subject, not from a shape.** Write one line: what does this site actually do,
   and what object, letter, or sound could stand for it? A bicycle shop, a research lab, and a
   bakery must not produce the same mark.
2. **Explore 3–5 concepts in one scratch page.** Build them side by side (`*-concepts.html`) so they
   are compared, not imagined. Vary the *idea*, not just the color.
3. **Pick one deliberately.** State the reason in one sentence ("the wordmark, because the name is
   the brand; the dot marks 'live'"). If two concepts are equally good, the simpler one wins.
4. **Refine the winner** — spacing, weight, the single accent. Then run the hard cases below.
5. **Export for the site**: the mark as inline SVG, and a `public/favicon.svg` derived from the same
   geometry.
6. **Delete the scratch page.** Concept / `*-concepts` previews are never shipped and never linked.

## What makes a mark work

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

## Pass criteria

The mark passes when all of these hold:

- It is derived from the subject — you can say why in one sentence.
- It is legible at 16 px and works on light and dark.
- It uses at most one accent.
- It ships as inline SVG with no webfont dependency, plus a matching `public/favicon.svg`.
- The scratch concept page has been deleted.
