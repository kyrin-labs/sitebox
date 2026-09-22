---
name: sitebox-verify
description: Prove a SiteBox site actually works — build a render harness that checks every route and every displayed value against the source data, write a negative control for every guard so you know it can fail, measure layout in a real headless browser (cropped images, overflow, load failures), sweep the served HTTP for 200s and 304s, and deploy as a checksum-verified mirror. Use when a site needs to be proven rather than eyeballed; when a check reports success but the site is still broken; when someone asks "how do you know it works?"; after any redesign; or when images are cropped, missing, or stale on the running site. Not for judging the design (sitebox-design) or for fetching the data (sitebox-data).
metadata:
  author: sitebox
  version: "1.0.0"
  updated: "2026-09-22"
---

# Proving a SiteBox site works

Reading a page carefully is not verification. Every expensive bug in this repo passed a careful reading:
a CSS rule the browser ignored, a guard whose regex never matched, a scrape that returned 41 of 51 items,
a deploy that copied one file out of 991, a control that had stopped testing anything.

Core belief: **a check that has never failed is not a check.** It is a comment with syntax. So every guard
ships with a negative control that breaks the thing it protects and watches it go red.

## When to use

- After building or redesigning anything, before saying it works.
- A check reports success but the page is visibly wrong.
- Someone asks how you know — the answer must be an artifact, not an opinion.
- Images are cropped, missing, stale, or the wrong picture.
- Before a deploy, and again after it.

## When NOT to use

- Nothing was built or changed. There is nothing to prove.
- You are still exploring. Verify the artifact, not the sketch.
- You want the *design* judged. That is `sitebox-design` → `references/audit.md`.

## Four layers, four different questions

| Layer | Runs on | Answers | Cannot answer |
|---|---|---|---|
| **1. Render harness** | Node + a DOM stub + `vm` | Does every route render? Does every displayed value match the source? Do i18n, escaping and interactions behave? | Anything about layout. A DOM stub is not a parser |
| **2. Negative controls** | Node, against the real tree | Can each guard in layers 1, 3 and 4 actually fail? | Nothing on its own — it validates the other layers |
| **3. Real browser** | Headless Chromium | Is anything cropped, overflowing, or failing to load? Do bands share their edges? | Business logic |
| **4. HTTP + checksum** | HTTP, `sha256sum` | Is what is served identical to what was verified? | Anything about rendering |

All four are required when a browser exists. Layers 1 and 4 are cheap; layer 3 is the one that catches
what a text-level check structurally cannot.

## Layer 1 — the render harness

Drive the app's **real** render pipeline in `vm` against a small DOM stub. No browser, no test framework,
no dependencies, a few seconds for every route.

What to assert, in rough order of how often it has found something:

- **Every route renders** — all of them, including the empty ones. Empty states are where rendering code
  breaks: a profile with zero posts, an empty feed, a search with no results, an empty library.
- **Every displayed value matches the source of truth.** Iterate the data file and compare field by field:
  titles, exact counts, lengths, timestamps, usernames, comment text. This is what turns "the data drifted"
  into a failing test (`sitebox-data` owns the fixture idea).
- **No `undefined`, `NaN` or `[object Object]` anywhere** in the rendered output. This single string check
  found `NaN โพสต์` and a null subscriber count.
- **Exactly one `<h1>` per route**, no skipped heading levels.
- **Every `<img>` has real `width`/`height` and an asset version**, and the file exists on disk.
- **No fabricated data markers**: no `Math.random`, no placeholder or stock-photo service, no CDN URL.
- **Interactions**: toggle and untoggle (a like must be reversible and the count must not drift), submit
  with hostile input (XSS escaping), add/remove, empty submissions.
- **i18n**: both dictionaries have the same keys; switching language actually changes the screen; dates and
  numbers use `Intl`.
- **Stray glyphs in your own UI copy** — Chinese or Cyrillic nobody typed on purpose. **Scope it to the copy
  tables**, because scraped content legitimately contains CJK.

### The one harness bug to avoid

**A DOM stub is not a parser.** It will happily build a tree from `<button>` inside `<button>`, which a
real browser silently restructures — the outer button closes early and the layout breaks. A harness cannot
see it. Walk the markup with a tokenizer, or check it in the browser.

## Layer 2 — negative controls

For **every** guard: break the thing it protects, run the guard, confirm it fails, restore, confirm the
tree is green again.

```js
{ name: 'caught rounded subscriber count',
  why: 'the 125K that was invented for a 72K channel',
  break() { edit(HTML, '"subs":"72K"', '"subs":"125K"'); } },
```

The runner should:

- copy the file to memory, apply the break, run the named guard, capture whether it failed, then **always**
  restore — including on throw.
- print `caught <name> by <guard>` or a loud `MISSED <name>`.
- **skip explicitly** a control whose tool is unavailable (no browser, no Pillow) — never let it pass
  silently. A skipped control and a passing control must look different.
- at the end, re-run the full suite and print that the tree is green again. If it is not, the runner left
  damage behind and the whole exercise is void.

### The taxonomy of fake guards

Every one of these has happened here. Check for them deliberately.

| # | Fake guard | How it looks | The real case |
|---|---|---|---|
| 1 | **The regex that never matches** | `✓` forever | `/const TH=\{/` against real code `const TH = {` — the stray-glyph check never ran once |
| 2 | **The guard over an empty sample** | `✓` over zero items | same check: `uiCopy` was empty, so there was nothing to scan |
| 3 | **The wrong-level check** | a check that should pass, fails — or worse, the reverse | normalising CSS by stripping all whitespace turned `.card .thumb` into `.card.thumb`, so a descendant selector stopped matching |
| 4 | **The inverted helper** | every result backwards | `add(cond, msg)` written so the message printed on success |
| 5 | **The broken utility** | `NaN` propagates | a luminance function that returned `NaN` for 3-digit hex, so every contrast check passed |
| 6 | **The check that skips the failing case** | green page full of 404s | `if (!img.naturalWidth) continue` — broken images were skipped instead of failing |
| 7 | **The stale control** | a control that no longer bites | a hero `max-height` control that stopped mattering after the layout changed to be narrower than the cap |
| 8 | **The check dropped in a rewrite** | coverage silently shrinks | per-thread comment verification disappeared during a refactor and nobody noticed |
| 9 | **The guard covering one of two siblings** | half the data unguarded | `width`/`height` was verified for videos but not for Shorts, so the page declared stale geometry for hours |
| 10 | **The false positive from real content** | a guard that fires on good data | `placehold` matched `placeholder=`; `todo` matched a Portuguese comment; `lucide` matched your own explanatory comment |

### Two rules about controls themselves

- **A MISSED control means the control is wrong first.** Both controls that missed on their first run in
  Nearly were faulty controls, not faulty sites: one tested a rule that no longer applied, the other ran at
  a viewport narrower than the thing it was testing. Fix the control before you touch the site.
- **A control must reproduce the CURRENT risk.** A control for a bug you already fixed by changing the
  layout is theatre. When the design changes, re-derive the control from what could go wrong *now*.

## Layer 3 — the real browser

Headless Chromium exists on both machines. A text-level check sees `aspect-ratio: 16/9` and passes while
the browser ignores the declaration entirely — that is how 27 % of every thumbnail was cropped for a full
session with a green suite.

What to measure, per route and per viewport:

| Check | Rule |
|---|---|
| **Cropping** | For every `<img>` inside a box with a constrained size: `drawn ratio` vs `naturalWidth/naturalHeight`. **Fail above 1 %** |
| **Inline geometry** | Flag any element with an `aspect-ratio` whose computed `display` is `inline` — the declaration is a no-op there |
| **Load failures** | `img.complete && !img.naturalWidth` is a **failure**. Never `continue` past it |
| **Horizontal overflow** | `documentElement.scrollWidth > innerWidth + 1` fails, at every viewport |
| **Edge alignment** | Bands stacked on each other report the same left and right edge |
| **Double scroll** | `documentElement.scrollHeight > innerHeight` on a route that should fit one screen |

**Run the viewport matrix, not one width.** A centred shell with a `max-width` is invisible at 1440 px if
the cap is 1600 px. Nearly needed a 1920 pass and then a 2560 pass to see the content stopping 680 px
short of the right edge. At minimum: a desktop width, one *wider* than any cap in the CSS, and 390 px.

**Save screenshots as evidence.** `chrome --headless=new --no-sandbox --virtual-time-budget=12000
--screenshot=out.png URL`. They are how you notice what you did not think to assert — the cropped text on a
thumbnail was first seen in a screenshot, not in a measurement.

## Layer 4 — HTTP and the deploy

### Sweep the served bytes

For every asset the page references: **200** and a content type that matches the extension. Plus:

- HTML is gzipped and carries an `ETag`; `If-None-Match` with that ETag returns **304**.
- A file that does not exist returns **404** — not a 200 with the index page. A silent 200 for a broken
  image path hides the bug until a person looks.
- A path traversal attempt (`/../../etc/passwd`) returns **404**.

### Deploy as a mirror, then prove it

A partial deploy is the quietest failure in this repo. Nearly pushed `index.html` and nothing else: the page
returned 200, the health check said `online: true`, and 22 new images plus 180 commenter avatars were 404.

1. **Mirror** every file the page references — html, favicon, data, every image.
2. **Delete** files on the server that no longer exist locally, so the tree really is a mirror.
3. **Compare `sha256`** on both sides and **`exit 1`** on any mismatch.

Two traps, both already paid for:

- Do the remote delete in **one `ssh` call** (`xargs`), not inside a `while read` loop — `ssh` consumes
  stdin and the loop deletes one file per run.
- **Normalise before comparing hashes.** `sha256sum` marks binary files with `*` and sort order differs by
  locale, so `sed 's/ \*/  /'` and `LC_ALL=C sort -k2,2` — otherwise every file "differs".

## Report format

```
verify.js        ALL CHECKS PASSED · <the counts that matter>
audit.js         no unexpected findings
render-check.js  PASS (n) · <what was measured, with the numbers>
controls.js      all n controls fire, and the tree is green again
sweep.js         n served, 0 problem(s)
checksum         n/n files byte-identical, local == deployed
dashboard        online:true · port <n> · 0 errors in the log
```

Print the **numbers**, not just the verdicts. `hero band = rail edges (280-1408)` is evidence;
"layout ok" is not.

## Failure modes

| Symptom | Cause | Response |
|---|---|---|
| Everything passes and the site is still broken | The guard is fake (taxonomy above) | Write the negative control first; if you cannot make it fail, the guard does not exist |
| A control reports MISSED | The control is wrong | Re-derive it from the current risk, not the old one |
| A guard passes over an empty sample | The thing it scans was not found | Fail when the sample is below a floor |
| A fix looks green locally but not on the site | It was never deployed | Re-run the mirror and the checksum |
| An image is the wrong shape in the browser but correct on disk | Layout, not data | `sitebox-design` → Image geometry |
| A control leaves the tree dirty | The runner does not restore on throw | Always restore in a `finally`, and verify the tree is green at the end |

## Quality bar

**Good:** every route renders and every displayed value is compared against the source; every guard has a
control that provably fires; layout is measured in a browser at three widths; the served bytes are swept
and the deploy is checksum-verified; the report contains numbers.

**Reject:** any guard without a control · a control that skips silently when its tool is missing · a
`✓` over an empty sample · a layout claim made from reading CSS · a deploy reported as done without a
checksum · "it looked fine".

## Reference files

- `references/harness.md` — building the DOM-stub render harness, the fixture pattern, what to assert
- `references/controls.md` — writing negative controls, the fake-guard taxonomy, the runner's rules
- `references/browser.md` — the headless-Chromium layout check: what to measure and at which viewports
- `references/serving.md` — the HTTP sweep, gzip/ETag/304, the deploy mirror and the checksum

## Related skills

- `sitebox-design` → `references/audit.md` — the design and accessibility audit (this skill proves it runs)
- `sitebox-data` — the source of truth and the fixtures this skill compares against
- `sitebox-create` — the build workflow; step 9 is the deploy this skill verifies
- `sitebox-config` — lifecycle, health, logs
