# Negative Controls

The discipline that makes every other check mean something. For each guard: break the thing it protects,
run the guard, confirm it fails, restore, confirm the tree is green again.

## Why it is not optional

Every guard in this repo that was ever wrong was wrong in the direction of passing. None of them was
obviously broken — they printed a green tick, which is exactly what a working guard prints.

The clearest case: Folio's stray-glyph check used `/const TH=\{[\s\S]*?\n\};/` against real code that says
`const TH = {`. It never matched, `uiCopy` was empty, and the check printed `✓ no stray CJK` for the entire
life of the site. It was found only by injecting `图像` into a Thai label and watching the audit still pass.

**A `✓` that has never had a chance to be a `✗` proves nothing.**

## The shape of a control

```js
{ name:  'caught rounded subscriber count',
  tool:  'verify.js',
  why:   'the 125K that was invented for a 72K channel',
  break() { edit('public/index.html', '"subs":"72K"', '"subs":"125K"'); } },
```

- **`name`** reads as a sentence: `caught <what you broke>`. The runner prints it either way, so it has to
  make sense in both `caught …` and `MISSED …` form.
- **`tool`** names the guard that is supposed to catch it, so a MISSED line tells you which check failed to
  exist.
- **`why`** records the real bug it stands for. A control without a real bug behind it is a control for an
  imaginary problem, and it will not catch the next real one.
- **`break()`** edits the shipped artifact, not a copy the guard does not read.

The runner:

1. Read the target file into memory.
2. Apply the break.
3. Run **only** the named guard, capture pass/fail.
4. Restore in a `finally` — including on throw.
5. Print `caught <name> by <tool>` or `MISSED <name>`.
6. **Skip explicitly** when the tool is unavailable (no browser, no Pillow). A skip and a pass must be
   distinguishable in the output.
7. Re-run the whole suite at the end and print that the tree is green again. If it is not, the runner left
   damage behind and the entire run is void.

## The taxonomy of fake guards

Check for each of these deliberately. Every one has happened here.

### 1. The regex that never matches

```js
html.match(/const TH=\{[\s\S]*?\n\};/)   // real code: const TH = {
```

Nothing downstream can detect this, because the *symptom* of a non-matching regex is an empty sample, and
an empty sample passes every `for` loop.

**Fix:** match with `\s*` and assert the sample size.

### 2. The guard over an empty sample

The same bug's other face. Any check of the form "nothing in this collection is bad" is vacuously true for
an empty collection.

**Fix:** a floor. `if (uiCopy.length < 4000) fail('the UI-copy blocks could not be located — this check
would be vacuous');`

### 3. The wrong-level check

Normalising CSS by stripping all whitespace turned the descendant selector `.card .thumb` into `.card.thumb`,
so a check that should have passed failed. The dangerous direction is the other one: a check written the same
way that should have failed and passed instead.

**Fix:** operate on the real text with `\s*` rather than on a mutated copy, and prove both directions.

### 4. The inverted helper

```js
const add = (cond, msg) => { if (!cond) ok.push(msg); else fail.push(msg); };   // backwards
```

Every result is inverted, which is loud — unless the suite is usually green, in which case it looks like a
catastrophe and you fix the *site*.

**Fix:** one `check(cond, msg)` helper, used everywhere, with a control that proves it fails on `false`.

### 5. The broken utility

```js
function lum(hex) { /* handles #rrggbb only */ }
lum('#fff')   // NaN
```

`NaN` in a comparison is falsy, so "is the contrast at least 4.5?" answered `false`… or, written the other
way, `true`. Either way the check is decorative.

**Fix:** expand 3-digit hex, and add a control that feeds it one.

### 6. The check that skips the failing case

```js
if (!img.naturalWidth) continue;   // broken images silently skipped
```

A render check that skips images which failed to load reports a green page full of 404s. This shipped: only
`index.html` was deployed and every image was missing.

**Fix:** `if (img.complete && !img.naturalWidth) fail(...)`.

### 7. The stale control

A control for a hero `max-height` bug stopped biting once the layout changed to be narrower than the cap, so
it could never fail again. The control was fine when written and meaningless afterwards.

**Fix:** when the design changes, re-derive the control from what could go wrong *now*. A control that
reproduces a bug you already fixed by restructuring is theatre.

### 8. The check dropped in a rewrite

A refactor removed per-thread comment verification — 853 comments compared field by field became zero — and
nothing complained, because a missing check is indistinguishable from a passing one.

**Fix:** floors on coverage (`comments >= 800`, `threads >= 140`) so a shrinking suite is visible. Keep
floors *below* the real numbers; they are a tripwire, not a change detector.

### 9. The guard covering one of two siblings

`width`/`height` was verified for videos but not for Shorts, so the page declared 405×720 for a file that
was 1080×1920 and every check stayed green.

**Fix:** when you write a guard for a collection, look for its siblings — every other collection shaped the
same way — and cover them in the same pass.

### 10. The false positive from real content

- `placehold` matched `placeholder=` on an input.
- A case-insensitive `todo` matched a Portuguese comment: `"transformar todo esse lucro"`.
- `lucide` matched your own comment explaining why you did not use Lucide.

All three were "fixed" by making the check narrower, not by changing the data.

**Fix:** match a URL host or an exact token, scope to UI copy, and prefer a structural signal (a
`@font-face` rule) over a library's name.

## Reporting

```
  caught video title drift                  by verify.js  ✗ title drifted: "EDITED BY HAND TITLE…"
  caught rounded subscriber count           by verify.js  ✗ app "125K" vs the source "72K"
  caught fabricated engagement              by verify.js  ✗ no Math.random anywhere in the shipped script
  caught thumbnail baked to a cropped ratio by verify.js  ✗ was cropped: 640x640 vs source 1.777778
  caught theme token missing                by audit.js   FINDINGS (1)
  caught stray CJK in the UI copy           by audit.js   FINDINGS (1)
  skipped avatar file holds another channel  (faces.py needs Pillow — not on this machine)

after restoring:
  green verify.js   ALL CHECKS PASSED
  green audit.js    no unexpected findings

all 37 controls fire, and the tree is green again
```

Print the guard's own failure message. It is the evidence that the guard tested the right thing, and it is
how you notice a guard that "caught" the break for an unrelated reason.

## The two rules about controls

- **A MISSED control means the control is wrong first.** Both controls that missed on their first run in
  Nearly were faulty controls: one tested a rule the layout no longer had, the other ran at a viewport
  narrower than the thing it was checking. Fix the control before touching the site.
- **A control must reproduce the CURRENT risk.** See #7.
