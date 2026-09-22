# The Render Harness

A zero-dependency way to run a site's real render pipeline in Node and assert against it. No browser, no
test framework, a few seconds for every route.

## Why not a real browser for this

A browser is the right tool for layout and the wrong tool for logic. It is slower, it needs a display
stack, it makes failures read as screenshots instead of stack traces, and it cannot easily assert "every
displayed value matches the source file". Split the job: the harness proves logic, the browser proves
layout.

## The shape

```js
const fs = require('fs'), vm = require('vm'), path = require('path');
process.chdir(path.join(__dirname, '..'));

const html = fs.readFileSync('public/index.html', 'utf8');
const app  = html.match(/<script id="app">([\s\S]*?)<\/script>/)[1];   // the shipped script
const DATA = JSON.parse(fs.readFileSync('synd/<site>-data.json', 'utf8'));  // the source of truth

// 1. a DOM stub just good enough for the app's own render functions
const el = (tag = 'div') => ({
  tagName: tag.toUpperCase(), children: [], attrs: {}, style: {}, dataset: {}, classList: {
    add(){}, remove(){}, toggle(){}, contains(){ return false; } },
  innerHTML: '', textContent: '', value: '',
  setAttribute(k, v) { this.attrs[k] = String(v); },
  getAttribute(k) { return this.attrs[k] ?? null; },
  appendChild(c) { this.children.push(c); return c; },
  addEventListener() {}, removeEventListener() {},
  querySelector() { return null; }, querySelectorAll() { return []; },
});

const doc = {
  createElement: el,
  getElementById: () => el(),
  querySelector: () => el(), querySelectorAll: () => [],
  addEventListener() {}, documentElement: el('html'), body: el('body'),
  location: { hash: '#/', pathname: '/', search: '' },
};

// 2. run the shipped script against it
const ctx = vm.createContext({
  document: doc, window: { addEventListener(){}, location: doc.location, matchMedia: () => ({ matches:false, addEventListener(){} }) },
  localStorage: { getItem: () => null, setItem(){}, removeItem(){} },
  console, Intl, Date, Math, JSON, Set, Map, Array, Object, String, Number, Boolean, RegExp, Error,
});
vm.runInContext(app, ctx);

// 3. walk the app's own routes and read the markup it produces
const out = ctx.App.render('#/watch/' + DATA.videos[0].id);
```

## What the stub must be able to do

Only what the app actually touches. A stub that pretends to be more than it is hides bugs; a stub that is
missing a method the app calls produces a confusing `TypeError` instead of a real finding. When you hit one,
add the method — do not mock the app.

The one place a stub is genuinely wrong: **it is not a parser.** `<button>` inside `<button>` builds a tree
here and is silently restructured by a browser. Tokenise the markup separately for nesting.

## What to assert

### Render coverage

Every route, and every variant of a route that can produce a different screen: each channel profile × each
tab, a watch page for each comment thread, an empty library, a search with no results. Empty states are
where render code breaks, because the happy path is what got tested by hand.

```js
for (const r of ['#/', '#/explore', '#/shorts', '#/library', '#/notifications', '#/studio'])
  check(renders(r), r + ' renders');
for (const c of DATA.channels) for (const t of ['', 'videos', 'shorts', 'about'])
  check(renders('#/channel/' + c.handle + t), 'channel tab renders');
```

### Content fixtures

Iterate the source of truth and compare every field the page displays. This is the assertion that makes the
data trustworthy.

```js
for (const v of DATA.videos) {
  const got = ctx.App.VIDEO(v.id);
  if (!got) { fail(v.id + ' missing'); continue; }
  if (got.title !== v.title)          fail(v.id + ' title drifted');
  if (got.views !== v.views)          fail(v.id + ' views: ' + got.views + ' vs ' + v.views);
  if (got.seconds !== v.seconds)      fail(v.id + ' length drifted');
  if (got.published !== v.published)  fail(v.id + ' publish time drifted');
}
```

Print the count of things compared, so the size of the claim is visible: `all 83 videos match the source`.

### Structural and hygiene checks

| Check | Why |
|---|---|
| Exactly one `<h1>` per route | Two headings means two pages got concatenated |
| No `undefined` / `NaN` / `[object Object]` in output | Found `NaN โพสต์` and a null subscriber count |
| No nested interactive elements | `<button>` in `<button>` / `<a>` — tokenise, the stub cannot see it |
| Every `<img>` has `width`, `height` and an asset version | A page that does not know an image's height will get a fixed height, and that is where cropping starts |
| Every referenced file exists on disk | A 404 that never reaches the browser |
| No `Math.random` in the shipped script | Fabricated engagement |
| No placeholder service URL (`picsum`, `pravatar`, `unsplash`, `ui-avatars`) | Fabricated content |
| Every channel/entity has a real, non-derived figure | A number without a source |

**Scope the string checks.** A search for `placehold` matches `placeholder=`; a case-insensitive `todo`
matches a Portuguese comment; `lucide` matches your own comment explaining why you did not use it. Prefer
matching a URL host or an exact token, and scan **UI copy** rather than the whole file.

### Interactions

- **Reversible toggles.** Like → unlike returns the original count and state. A count that drifts by one on
  every click is the classic bug.
- **Hostile input.** Submit `<img src=x onerror=alert(1)>` as a comment and assert it is escaped in the
  output, not present as markup.
- **Empty submissions.** Do not crash, do not create a blank item.
- **Create / rename / delete** a playlist, then assert the collection is back where it started.

### i18n

```js
const th = Object.keys(ctx.App.TH), en = Object.keys(ctx.App.EN);
check(th.length === en.length && th.every(k => en.includes(k)),
      'every TH key has an EN counterpart');
```

Also assert the *values* differ where they should (a dictionary that is half-copied from the other language
is a real bug that key-count equality does not catch), and that dates/numbers go through `Intl`.

## Reporting

```
✓ all 83 videos match the source (title, exact views, length, publish time)
✓ all 7 channels match (subs, exact total views, video count, joined, country)
✓ 853 comments across 143 threads match the source text and usernames
✓ no Math.random anywhere in the shipped script
✓ exactly one <h1> on 12 routes
ALL CHECKS PASSED · 7 channels · 83 videos · 60 shorts · 853 comments · 191 i18n keys
```

The final line should state **what was verified**, with counts. "ALL CHECKS PASSED" alone tells a reader
nothing about how much was actually checked.

## Failure handling

- **Exit non-zero** on any failure, and print `file:line`-style context where possible.
- **Never `continue` past a missing thing.** A missing entity is a failure, not a skip. The one exception is
  a check whose *tool* is absent — and that must be reported as skipped, not passed.
- **Floors, not exact counts, for anything the platform can change.** `comments >= 800`, not `=== 853`; the
  exact-number guarantee belongs to the per-field fixtures.
