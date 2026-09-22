# The Pipeline

How the tool chain is shaped, why each step exists, and the traps that are specific to running it.

## The shape

```
tools/lib.js         shared: HTTP with retry, the platform's JSON extraction, any token building
tools/fetch.js       network → synd/raw/ or synd/pages/       raw, kept as evidence
tools/build.js       raw → synd/<site>-data.json              THE SOURCE OF TRUTH + coverage floors
tools/download.js    data → assets-raw/                       validates each asset before accepting it
tools/resize.py      assets-raw → public/img/ + dims.json     needs Pillow; dev machine
tools/buildjs.js     data + dims → synd/data.block.js         the payload the page uses
tools/patchdata.py   block → public/index.html                splices between two markers
tools/htmldata.py    src/app.js → <script id="app">           only when the app source is separate
tools/verify.js      page vs source                           sitebox-verify owns the checks
tools/audit.js       CSS / a11y / contrast / tokens           sitebox-verify owns the checks
tools/sweep.js       real HTTP                                sitebox-verify owns the checks
tools/deploy.sh      mirror + checksum                        sitebox-create step 9
```

**Every tool self-locates.** The first lines of each are equivalent to:

```js
const SITE = path.join(__dirname, '..');
process.chdir(SITE);
```

so `node tools/fetch.js` works from any working directory, from a script, and from `ssh`. Without this,
half the pipeline only runs from one folder and someone eventually runs it from the wrong one.

## Why a source-of-truth file and not the page

`synd/<site>-data.json` is the single artifact everything else derives from:

- `buildjs.js` turns it into the payload the page loads.
- `verify.js` compares the rendered page against it, field by field.
- A human can read it to answer "where did this number come from?".

If the page were the source, "did the data drift?" would be unanswerable. Because the file is the source,
drift is a test failure.

## The splice pattern

The page carries two markers:

```html
<script id="app">
/*__DATA_START__*/
const DATA = { … };
/*__DATA_END__*/
```

`patchdata.py` replaces everything between them and reports the size delta:

```
spliced: 247275 -> 247395 bytes (+0 KB)
markers now: 1 start / 1 end
```

Two properties worth keeping:

- **Assert the markers exist, exactly once each.** A second `__DATA_START__` (from a botched edit) makes
  the splice non-deterministic. Print the counts.
- **Print the size delta.** It is how you notice a run that replaced 300 KB with 30 KB — a thin scrape —
  before the floors catch it.

Keep the app source in `src/app.js` and splice it too (`htmldata.py`) when the app is large. Nearly's rule
is the one to copy: **never hand-edit `public/index.html`.** Edit `src/app.js`, run the splice. A verify
step that compares the page against the source makes the mistake visible immediately instead of silently.

## Idempotency

Running the whole chain twice must produce byte-identical output. If it does not, the pipeline is not
reproducible and nothing downstream can be trusted.

Things that break it:

- A timestamp written into an output file.
- A `Set` or `Object.keys` iteration order that depends on insertion from a network response.
- A downloader that re-fetches everything every run (fine, but then say so and keep the cache key honest).
- A resize step that writes `dims.json` in whatever order the filesystem returned files.

`resize.py` deliberately **does not delete** files in `public/img/` that it did not produce — it reports
them, and a human decides. A tool that silently deletes is a tool that silently deletes the wrong thing.

## Order, and why it is load-bearing

```
fetch → build → download → resize → buildjs → patchdata → verify
```

- `build.js` wants every asset present in `dims.json`, but `dims.json` is written by `resize.py`, which
  runs *after* it. Nearly's build step initially hard-failed on every new asset for this reason. The fix
  is for `build.js` to accept "not resized yet, but the raw file exists" as **pass**, and to leave the
  final word to `verify.js`, which checks the files on disk.
- `buildjs.js` declares each asset's `width`/`height` from `dims.json`. Run it before `resize.py` and the
  page ships the *previous* geometry while the files are new — which is exactly what happened, and the
  page declared 405×720 for a 1080×1920 frame until someone re-ran the right command.
- `verify.js` runs last and must not modify anything. If a verify step "fixes" state on its way through,
  the run after it is not testing what you think.

If a step can be run out of order without complaining, add a check that compares what it *declared*
against what is on disk.

## Dev / server split

| Step | Where | Why |
|---|---|---|
| `fetch`, `build`, `download` | either | plain Node + network |
| `resize.py`, anything needing Pillow | **dev machine** | the home server has no pip and no sudo |
| `buildjs`, `patchdata`, `verify`, `audit`, `sweep` | server | Node only |
| `deploy.sh` | dev machine → server | mirror + checksum |
| browser layout check | either | headless Chromium exists on both |

Do not add Python to the server to avoid a copy. The copy is one `scp` and the deploy already mirrors
everything.

## The downloader

Three jobs, in this order:

1. **Build the candidate list per asset.** Most platforms offer several sizes. Record the list, not just
   the winner — that list is what lets a later run notice the source changed.
2. **Validate before accepting.** Read the image header (not the file extension, not the URL) and reject
   a variant whose aspect ratio is off by more than the tolerance, then try the next candidate. Nearly's
   order for a 16:9 thumbnail is `hq720` → `maxresdefault` → `mqdefault` → whatever the page offered,
   with a ±8 % gate, because `maxresdefault` on an old 4:3 upload is 960×720 and `hq720` is a pillarboxed
   1280×720.
3. **Cache on the source URL.** Write `assets-raw/.sources.json` mapping each destination file to the
   candidate list it was fetched from, and treat "the list changed" as "re-fetch". Keying on the
   destination filename alone means a corrected handle silently serves the previous entity's picture.

The header reader must handle what you actually download: WebP (VP8 / VP8L / VP8X) and PNG, not just
JPEG. A reader that silently fails on WebP turns the aspect gate into a no-op.

## Provenance in practice

Keep a provenance table in the site's development doc — *shown → source*. Three examples from the repo,
each of which cost time to discover:

| Shown | Source | The lesson |
|---|---|---|
| total post count | `<meta property="og:description">` | Not in the JSON at all. `media_count` was `undefined`, `JSON.stringify` dropped it, the page rendered `NaN` |
| total channel views | `aboutChannelViewModel` | Exact (`132,885,388`), where the videos page only shows abbreviated values |
| like count | `microformat.playerMicroformatRenderer.likeCount` | Already present in a request the pipeline was making anyway |

When you cannot find a value, **fail or omit**. Do not derive it, do not round it, do not put `0`.

## Floors and fixtures

- **Floors** live in `build.js` and exit non-zero: `if (shorts.length < 45) TOO THIN`. Set them just under
  the real counts. Their job is to catch a broken run, not a change on the platform.
- **Fixtures** live in `verify.js` and compare every displayed field against the source JSON. Their job is
  to catch drift between what was fetched and what the page says.
- **Neither is worth anything without a negative control.** Break the thing, watch the check fail, restore
  it. `sitebox-verify` owns that discipline; a floor that has never fired is a comment with syntax.

## A full run, end to end

```bash
cd ~/sitebox/sites/<id>

# 0) re-fetch (network; write raw evidence)
node tools/fetch.js
node tools/build.js             # → synd/<id>-data.json  (+ floors)

# 1) assets
node tools/download.js          # → assets-raw/          (aspect-validated, source-keyed cache)
python3 tools/resize.py         # → public/img/ + synd/dims.json      [dev machine, needs Pillow]

# 2) payload
node tools/buildjs.js           # → synd/data.block.js + any on-demand JSON
python3 tools/htmldata.py       # src/app.js → <script id="app">      (if the app is separate)
python3 tools/patchdata.py      # splice into public/index.html

# 3) prove it
node tools/verify.js            # page vs source, crops, a11y, i18n
node tools/audit.js             # CSS / themes / contrast
node tools/controls.js          # every guard must be able to fail
node tools/sweep.js http://localhost:<port>
bash tools/deploy.sh            # mirror + checksum, exit 1 on mismatch
```

Copy resized images to the server as part of the deploy, not as a separate step someone will forget.
