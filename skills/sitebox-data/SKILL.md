---
name: sitebox-data
description: Get real, traceable, refreshable content into a SiteBox site — find a working source for a platform's data, build a pipeline that can be re-run without touching the UI, prove every displayed number came from that source, and keep assets uncropped. Use when a site needs content that exists somewhere real (a social platform, an API, a repo, a dataset), when numbers or copy must be real rather than written, when a scrape silently returned less than it should, when a page shows a stale or wrong image, or when someone asks "where did this number come from?". Do not use for a site whose content is the owner's own words.
metadata:
  author: sitebox
  version: "1.0.0"
  updated: "2026-09-22"
---

# Real Data for SiteBox sites

A clone site is only convincing when its content is real. Fabricated content is not a shortcut — it is
the thing the owner notices first. Relay, Folio and Nearly all ended up replacing invented data with
scraped data, and each time the real data exposed problems the fake data had hidden: portrait images,
signed URLs that expire, numbers that are not where you expect, an entire channel that turned out to be
the wrong one.

Core belief: **the pipeline is the deliverable, not the JSON.** Anyone can paste data into a file. The
job is a file that can be regenerated next month, whose every value can be traced to a URL.

## When to use

- The site shows a platform's content: posts, videos, comments, followers, prices, commits.
- A number or a name on the page has to be *true*, not plausible.
- Someone asks where a value came from, or a value changed on the platform and the site did not.
- A scrape "worked" but returned fewer items than it should have.
- Images are stale, wrong, or the wrong shape.

## When NOT to use

- The site is the owner's own words: a shop, a CV, a landing page, a portfolio. A pipeline there is
  waste — write the copy and move on.
- You are only changing layout or styling. The data is already there.
- The "data" is a small fixed set the owner typed (opening hours, three prices). Hard-code it and put a
  comment saying where it came from.

## The pipeline

Seven steps, in this order. Every one of them is a file in `tools/`, and every one self-locates its
own directory so it runs from anywhere.

```
tools/fetch.js      → synd/raw/…          raw pages or JSON, kept as evidence
tools/build.js      → synd/<site>-data.json   THE SOURCE OF TRUTH
tools/download.js   → assets-raw/         every image, validated before it is accepted
tools/resize.py     → public/img/ + synd/dims.json    (needs Pillow — see Environment)
tools/buildjs.js    → synd/data.block.js  the payload the page actually uses
tools/patchdata.py  → public/index.html   splices the block between two markers
tools/verify.js     → proves the page matches the source   (owned by sitebox-verify)
```

**The order is load-bearing.** `resize.py` is what produces `dims.json`, and the data block declares
each asset's `width`/`height` from it. Run the block generator before the resize and the page ships the
*previous* geometry while the files on disk are new — Nearly did exactly this and declared 405×720 for
a 1080×1920 frame for an hour, with every check green. If a step can be re-run out of order, make the
later step read the earlier step's output rather than recompute it.

**Splice, never hand-edit.** The data lives between two markers in `public/index.html`
(`/*__DATA_START__*/ … /*__DATA_END__*/`). Editing the output by hand means the next pipeline run
erases your work — and a verify step that compares the page against the source will fail, which is the
point. Edit the app source (`src/app.js`) and re-run the splice.

## Finding a source that works

Try these in order. Stop at the first one that returns **data**, not the first one that returns 200.

| # | Route | Why it works / fails |
|---|-------|----------------------|
| 1 | **The platform's own JSON endpoint** | Always first choice. X: `syndication.twitter.com/srv/timeline-profile/screen-name/{handle}` returns the whole profile *and* ~100 posts in one request. YouTube: `POST youtubei/v1/player` returns exact view count, length, publish date, description, keywords **and** like count in 12 KB |
| 2 | **The same page with a crawler user-agent** | When a page is client-rendered, a crawler UA often gets a fully server-rendered one. Instagram with `Googlebot` returns 1,060 KB containing `<script type="application/json">` payloads with exact `like_count`, `taken_at`, captions and real comments — where a normal Chrome UA returns a 630 KB login shell with no data at all |
| 3 | **A smaller endpoint instead of the big page** | The watch page is 1.6 MB and the player endpoint is 12 KB for the same facts. 8 parallel requests took 0.3 s |
| 4 | **A reconstructed request** | If an endpoint needs a token you can only get from a huge page, decode one real token and see what varies. A 56-byte comment continuation token differed from another video's in **two bytes** (the video id) — rebuilding it removed 130 requests × 1.6 MB |
| 5 | **A third-party renderer** (e.g. `r.jina.ai`) | Last resort. It worked for Instagram and then returned 403 after ~7 requests on the free tier. Never build on one |
| — | Mirrors, scrapers, `?__a=1`, `/api/v1/…` | Record what you tried and what it returned. Folio's list of eight dead ends is worth more than the one that worked, because nobody has to retry them |

**Record every route you tried, including the failures**, in the site's development doc. The next
person's first hour is otherwise identical to yours.

### Validate the payload, not the request

A 200 is not data. YouTube intermittently answers with a consent interstitial that parses perfectly and
contains no `ytInitialData`. Instagram returned a 720 KB profile page with `follower_count` but **no
media grid**, so the scrape found zero posts and reported success.

Give the fetcher a `mustHave` — the key(s) that prove the payload is real — and treat a miss as a
failure with retry and backoff. Then check the *counts* at the end (see Coverage floors).

### Two traps that cost real time

- **A `head` in the pipeline kills the process.** `node tools/fetch.js | head -10` closes the pipe,
  raises SIGPIPE, and the script dies before it saves — while the terminal shows correct output, so you
  conclude the code is wrong. Pipe to `tail`, or write to a file.
- **Signed image URLs expire.** Instagram's carry `oe=<hex>` with roughly a three-week life. Run the
  downloader soon after the fetch, or the URLs are dead and you re-fetch everything.

## The six rules that make data trustworthy

### 1. Every displayed value has a provenance

Keep a table in the site's development doc: **what the user sees → where it came from**. Relay, Folio
and Nearly each ship one. It looks like this:

| Shown | Comes from |
|---|---|
| name, handle, bio, verified | profile JSON (`full_name`, `username`, `biography`, `is_verified`) |
| followers / following | profile JSON (`follower_count`, `following_count`) |
| **total posts** | `<meta property="og:description">` — `"10M Followers, 1 Following, 1,509 Posts"` |
| likes / comments | post JSON (`like_count`, `comment_count`) |
| caption | post JSON (`caption.text`) — **verbatim, never translated** |
| alt text | the platform's own `accessibility_caption` |

That third row is the lesson: the post count was **not** in the JSON. Reading `media_count` returned
`undefined`, `JSON.stringify` dropped the key, and the profile rendered `NaN โพสต์`.

### 2. Fail, do not invent

When a value cannot be found, the choices are **omit it** or **fail the build**. Never substitute:

- A derived number labelled as the real one. Nearly computed likes as 4 % of views; the true ratio was
  1.21 %, so every number was **3.3× too high**.
- `0` for "unknown". `0` is a claim. Folio's rule: *if you cannot find it, fail — do not put 0*.
- A round number that reads well. Use what the platform prints, and if the platform only prints `522K`,
  print `522K` — that is the real value, not an abbreviation of a number you guessed.

### 3. Scraped content is verbatim

Titles, captions, comments, usernames: byte-for-byte, in the language they were written in. **Never
translate.** A Japanese video title stays Japanese next to Thai UI chrome. Rewriting a title is the
exact failure this project has already made once ("video titles were changed from real titles") — do not
reintroduce it.

Your *own* UI copy is different: that gets the humanizer/deslop passes (`sitebox-design`).

### 4. Coverage floors

After the build, assert minimum counts and **exit non-zero** below them. A partial scrape once dropped
Shorts from 51 to 41 with nothing complaining, and was only caught because a floor existed.

```js
if (videos.length < 70) { console.error('TOO THIN: ' + videos.length + ' videos'); process.exit(1); }
```

Set floors just under the real numbers, not at them — they are a tripwire for a broken run, not a
change detector. Put the floors where they are cheapest to enforce (the build step), so a bad run cannot
reach the deploy.

### 5. Content fixtures

The verify step compares **every displayed field** against the source-of-truth JSON: titles, exact view
counts, lengths, timestamps, channel stats, comment text and usernames. This turns "the data drifted"
from a complaint the owner makes into a test failure you see first. That is what makes `sitebox-verify`
worth building — data without a fixture is data you are trusting.

### 6. Refreshable independently of the UI

The app must not hard-code anything the pipeline produces. `node tools/fetch.js && node tools/build.js`
must be able to change every number on the page without editing a single line of the app. If a value is
in the app source instead of the data file, that is the bug — the site stops being refreshable the
moment someone types a number into a component.

## Assets: never crop, and check the shape when you download

**Files keep the frame they came with.** Cropping is a presentation decision and presentation is
reversible; a byte-level crop is not. Relay baked `ImageOps.fit(img, (1400, 467))` into a banner and
destroyed the top of every portrait permanently — no lightbox could recover it, because the file was
already cut.

**Validate the aspect ratio at download time, not at resize time.** This is counter-intuitive and it
matters: a resize that preserves aspect ratio *passes* even when the file was the wrong shape to begin
with. Two of Nearly's 79 thumbnails were genuinely 4:3 uploads whose `maxresdefault` is 960×720 while
`hq720` is a pillarboxed 1280×720 — so the downloader reads the image header, rejects a variant whose
ratio is off by more than ~8 %, and tries the next candidate in order.

**Record `[width, height, source_aspect_ratio]` per file** in `dims.json` at resize time. A checker then
computes `w/h` against the recorded source ratio and fails above a small tolerance. That single file is
what makes "nobody cropped anything" a fact rather than a promise.

**Key the download cache on the source URL, not the destination filename.** This is the subtlest bug in
the repo. `av-9arm.webp` is a stable *name*; the picture behind it is not. When a handle was corrected,
the downloader saw the file already existed and skipped it — so the site served a different channel's
face, with nothing complaining. Write a manifest of the source URL list per file and re-fetch when it
changes.

**Store the platform's own image metadata when it gives you some.** Instagram's `accessibility_caption`
is real alt text (`"Photo by ILLIT 아일릿 on June 23, 2026."`). Use it instead of writing alt text by
hand.

## Honest empty states

When the platform does not publish something, the feature does not get invented — it gets an empty
state that says why.

| Feature | No real data exists | Ship |
|---|---|---|
| Stories | temporary, not scrapeable | a row of the accounts' latest real posts instead |
| Direct messages | no real conversation to fetch | an empty state that states there is nothing to show |
| Notifications | no real like/mention feed | real upload events from `publishDate` |
| Playlists | spec asked for three | zero, with an empty state inviting the user to create one |

Nearly's home page looks emptier for having no playlists. That is correct, and it is better than three
playlists that do not exist.

## Environment

| Where | What runs |
|---|---|
| **Dev machine** | `resize.py` and anything else needing **Pillow** — the home server has no pip and no sudo |
| **Either** | `fetch`, `build`, `download`, `buildjs`, `patchdata`, `verify`, `audit`, `sweep` |
| **Both have headless Chromium** | the browser layout check (`sitebox-verify`) |

Copy resized images to the server with the rest of the deploy (`sitebox-create` step 9). Do not add a
Python dependency to the server to avoid a copy.

## Failure modes

| Symptom | Cause | Response |
|---|---|---|
| A field is `null` / `NaN` / `undefined` on the page | The extractor expected one shape and got another (`{content}` vs a plain string), or a key was written over with `null` | Accept both shapes; drop `null` keys instead of assigning them |
| A scrape returns fewer items than last time | A partial render, a rate limit, an interstitial | Coverage floors → hard fail → re-fetch |
| Numbers are right in the terminal but stale in the file | A pipe (`head`) killed the process before it saved | Write to a file; read the file, not the screen |
| The wrong image is served | Cache keyed on the destination filename | Key it on the source URL |
| Images are the wrong shape although the file is fine | A layout bug, not a data bug | `sitebox-design` → Image geometry |
| A thumbnail is 4:3 when everything else is 16:9 | The upload is genuinely 4:3 and the high-res variant is letterboxed | Reject wrong-ratio variants at download; keep the file's real ratio and let the layout handle it |
| The page declares an old size for a new file | The block generator ran before the resize | Re-run in order; a checker compares declared vs on-disk |
| The source blocks you after N requests | Free-tier proxy, or you are being rude | Sleep between requests, retry with backoff, and prefer the platform's own endpoint |

## Quality bar

**Good:** every number traceable to a URL; `node tools/fetch.js && node tools/build.js` regenerates the
whole page without touching the app; floors catch a thin run; a fixture catches drift; `dims.json`
proves no file was cropped; a fresh clone can reproduce the data.

**Reject:** any number with no source · any invented or translated content · `0` used for "unknown" ·
a crop baked into an asset · a cache keyed on the filename · data that cannot be re-fetched · a scrape
that cannot tell you how many items it should have found.

## Reference files

- `references/sources.md` — the source-discovery ladder in detail, with the X / Instagram / YouTube
  recipes and the token reconstruction
- `references/pipeline.md` — the tool chain, the splice pattern, idempotency, and the dev/server split

## Related skills

- `sitebox-verify` — turns the fixtures and floors into checks that can fail
- `sitebox-design` — Image geometry (the crop rule) and the copy passes
- `sitebox-create` — the build workflow and the deploy mirror
- `sitebox-config` — lifecycle, ports, logs
