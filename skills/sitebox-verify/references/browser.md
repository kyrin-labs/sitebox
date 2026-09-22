# The Real-Browser Layout Check

Headless Chromium exists on both machines, so there is no situation in this repo where "a browser is not
available" is a true statement. This layer catches what no text-level check can.

## Why reading CSS is not enough

Nearly's suite read the CSS, saw `aspect-ratio: 16 / 9` on the thumbnail box, and passed. The browser
ignored the declaration entirely because the box was a `<span>` — a non-replaced **inline** element — so the
box took its height from the line box and `object-fit: cover` then ate **26.8 % of the width of every
thumbnail on every card**. A full session of "the banner looks weird" while the checks were green.

Three wrong hypotheses were tested before the real cause was found (black bars in the files, insufficient
resolution, a bad placeholder colour). Two of them cost real work — 11 MB of extra image resolution that
changed nothing. A single screenshot at the start would have shown the cropped text on the thumbnail.

## How to drive it

```bash
chrome --headless=new --no-sandbox --virtual-time-budget=12000 \
       --screenshot=/tmp/out.png "http://localhost:4504/#/explore"
```

For measurement rather than a picture, use the DevTools protocol through Playwright:

```js
const { chromium } = require(process.env.PLAYWRIGHT ||
  '/home/l2s/.npm/_npx/e41f203b7505f1fb/node_modules/playwright');
const browser = await chromium.launch({ args: ['--no-sandbox'] });
for (const [name, route, width] of ROUTES) {
  const page = await browser.newPage({ viewport: { width, height: 900 } });
  await page.goto(BASE + route, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1000);        // let lazy images settle
  const m = await page.evaluate(measure);
  await page.screenshot({ path: `synd/shots/${name}.png` });
  await page.close();
}
```

## What to measure

### Cropping — the assertion that matters most

For every `<img>` whose box has a constrained size, compare the ratio it is **drawn** at with its own
natural ratio.

```js
const drawn   = r.width / r.height;
const natural = img.naturalWidth / img.naturalHeight;
if (Math.abs(drawn / natural - 1) > 0.01) fail(   // 1 %
  `${src}: drawn ${r.width.toFixed(0)}x${r.height.toFixed(0)} (${drawn.toFixed(3)}) ` +
  `vs natural ${img.naturalWidth}x${img.naturalHeight} (${natural.toFixed(3)}) ` +
  `= ${((1 - Math.min(drawn, natural) / Math.max(drawn, natural)) * 100).toFixed(1)}% cut`);
```

Restrict it to images inside a sized box. An image left at its natural size cannot be cropped, and including
it only adds noise.

This one assertion catches all six symptoms in `sitebox-design` → Image geometry: the inline box, the
`aspect-ratio` + `max-height` combination, the stretched grid parent, the squashed viewer, and (via a
sibling check on disk) a crop baked into the file.

### Load failures

```js
if (img.complete && !img.naturalWidth) fail('image did not load: ' + src);
```

Never `continue`. Skipping broken images is how a page full of 404s reports green.

### Inline geometry

```js
const cs = getComputedStyle(el);
if (cs.aspectRatio !== 'auto' && cs.display === 'inline')
  fail(el.className + ' sets aspect-ratio but is display:inline — the declaration has no effect');
```

### Overflow and scrolling

```js
if (document.documentElement.scrollWidth > innerWidth + 1) fail('horizontal overflow');
if (document.documentElement.scrollHeight > innerHeight + 2 && routeShouldFitOneScreen)
  fail('the page scrolls on top of its own inner scroll container');
```

The second one caught a Shorts page with two scrollbars: the player height was computed as
`100vh - 64 - 40` while the container still had `padding-bottom: 96px`. The fix was to let flex fill the
space instead of adding viewport units up by hand.

### Edge alignment

Bands stacked on top of each other must report the same left and right edge.

```js
const band = document.querySelector('.hero').getBoundingClientRect();
const rail = document.querySelector('.rail').getBoundingClientRect();
if (Math.abs(band.left - rail.left) > 1 || Math.abs(band.right - rail.right) > 1)
  fail(`hero ${band.left}-${band.right} vs rail ${rail.left}-${rail.right}`);
```

### Content reaching the edge

At a viewport wider than any `max-width` in the CSS, measure the gap between the content's right edge and
the viewport. A gap of 680 px at 2560 px is a cap nobody noticed, because at 1440 px it was invisible.

## Run the viewport matrix, not one width

A check at one width is a check of one width. Nearly's alignment assertions were blind at 1440 px because
the shell's cap was 1600 px — the cap only became visible at 1920 px, and the dead space only at 2560 px.

Minimum matrix:

| Pass | Width | Catches |
|---|---|---|
| Desktop | 1440 | ordinary layout |
| **Wide** | 1920 | a centred shell or a cap narrower than the screen |
| **Ultra** | 2560 | dead space on the right, over-stretched bands |
| Mobile | 390 | overflow, collapsed search, stacked nav |

Plus one route per distinct layout, not one route total. A home page and a watch page share almost no
geometry.

## Screenshots are evidence

Save one per route and viewport. They are how you notice what you did not think to assert — the cropped
thumbnail text, the second scrollbar, a nav item wrapping to two lines. Assertions only find what you
already suspected; a picture finds the rest.

Keep them out of the shipped tree (`synd/shots/`, gitignored).

## When a browser check is genuinely unavailable

Report it as **skipped**, loudly, next to the control that depends on it — never as passed. Then say in the
final report which layer did not run, so nobody reads "all checks passed" as more than it is.
