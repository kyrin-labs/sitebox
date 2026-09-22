# Finding a Source

Detail behind the ladder in `SKILL.md`. Read this when the platform you need is not one of the three
below, or when a route that used to work stopped.

## The ladder, with the evidence

### 1. The platform's own JSON endpoint

Always try this first. It is the only route that is stable, fast, and honest about its own data.

**X / Twitter** — one request gives a whole profile and ~100 posts:

```bash
curl -sL -A 'Mozilla/5.0' \
  "https://syndication.twitter.com/srv/timeline-profile/screen-name/{handle}" \
  -o "synd/$HANDLE.html"
```

The response is ~500 KB of HTML containing `<script id="__NEXT_DATA__">`. From that one JSON you get
`name`, `description`, `location`, `url`, `followers_count`, `friends_count`, `statuses_count`,
`created_at`, `verified`, `is_blue_verified`, `profile_image_url_https`, `profile_banner_url`, and for
each tweet `full_text`, `favorite_count`, `retweet_count`, `reply_count`, `created_at` and
`extended_entities.media[]`.

Keep the raw HTML. It is the evidence that the numbers on the page were real on the day they were read.

**Filtering to original posts** — `retweeted_status` absent and `conversation_id_str === id_str`. Without
that you get retweets and replies mixed into a timeline that claims to be the account's own posts.

### 2. The same page with a crawler user-agent

When a page is client-rendered, the data is usually still reachable — just not by the route a browser
takes.

Measured on one Instagram post:

| User-Agent | Bytes | Contents |
|---|---|---|
| Chrome | 630 KB | login shell, rendered client-side — **not one `og:` tag** |
| `facebookexternalhit` | 694 KB | `og:` tags only |
| **`Googlebot`** | **1,060 KB** | **fully server-rendered, all data** |

```bash
curl -A 'Mozilla/5.0 (compatible; Googlebot/2.1; http://www.google.com/bot.html)' \
     "https://www.instagram.com/p/{shortcode}/"
```

The Googlebot page contains 42 `<script type="application/json">` blocks; block 23 (207 KB) holds the
post:

```json
{ "__isXIGPolarisMedia": "XIGPolarisCarouselMedia",
  "code": "DZ7FzoNmqX7", "taken_at": 1782205427, "media_type": 8,
  "product_type": "carousel_container",
  "like_count": 579442, "comment_count": 2014,
  "caption": { "text": "…" }, "carousel_media": [ … ] }
```

`like_count: 579442`, not `579.4K`. Exact. Plus real comments (`XIGComment`: `text`, `created_at`,
`comment_like_count`, `user.username`, `user.profile_pic_url`).

Two normalisation traps in this page:

- Slashes in the JSON are escaped as `\/`. A regex looking for `instagram.com/{handle}/p/` finds
  **nothing** until you normalise.
- Profile links are **relative** and say `/reel/` even for still photos:
  `href="/illit_official/reel/Ddi8v9RzRtP/"`. Accept both `/p/` and `/reel/`, and let the post page
  decide the real media type.

### 3. A smaller endpoint instead of the big page

The page you *can* fetch is often not the one you should.

YouTube's watch page is 1.6 MB per video. `POST youtubei/v1/player` is 12 KB and returns more of what
you need:

```json
{ "videoDetails": { "title": "…", "viewCount": "89381", "lengthSeconds": "1221",
                    "shortDescription": "…" },
  "microformat": { "playerMicroformatRenderer": { "publishDate": "2026-09-21T06:30:39-07:00",
                                                   "category": "Entertainment",
                                                   "likeCount": "1111" } } }
```

Eight requests in parallel: **0.3 s**. 130 videos: under 10 s. And `likeCount` was already there inside
`microformat.playerMicroformatRenderer` — the exact figure the site had been *deriving* from the view
count, wrong by 3.3×, without a single extra request.

YouTube needs five sources assembled, because no single one has everything:

| Need | Source |
|---|---|
| `channelId`, name, avatar, banner | `youtube.com/@{handle}` → `ytInitialData` (900 KB – 2.6 MB) |
| subscribers, **exact total views**, video count, country, joined | `/{handle}/about` → `aboutChannelViewModel` |
| 30 latest videos | `/{handle}/videos` → `lockupViewModel[]` |
| real Shorts | `/{handle}/shorts` → `shortsLockupViewModel[]` |
| exact views, length, publish time, description, **likes** | `POST youtubei/v1/player` |
| real comments | `POST youtubei/v1/next` (see below) |

### 4. Reconstructing a request

When an endpoint needs a token you can only obtain from a huge page, decode one real token and compare
two of them before giving up.

YouTube's comment continuation token, after base64-decoding, is 56 bytes:

```
120d120b <videoId, 11 bytes> 1806 3225 2211 220b <videoId, 11 bytes> 3000 7802 4210 "comments-section"
```

Two different videos' tokens differed in **exactly two places — byte offsets 4 and 23** — both the video
id. So the token is a template: substitute the id and post it.

**Prove it before trusting it.** The fetcher builds the token for a second video, calls the endpoint, and
throws if no comments come back. A reconstructed token that is never validated is a guess.

> **Trap:** the real token is **URL-encoded**, so it contains `%3D` for the base64 padding. Decoding it
> directly gives you one extra byte and `0` comments. `decodeURIComponent` first, always.

The result: 130 videos' comments with **zero** extra page fetches, instead of 130 × 1.6 MB.

### 5. A third-party renderer — last resort

`r.jina.ai/https://www.instagram.com/p/{code}/` returned real captions, real counts (`474.7K 1.7K`),
real comments and real image URLs — and then began returning **403 after roughly seven requests** on the
free tier. It nearly worked, which is worse than not working: a pipeline built on it fails a week later.

Use it to *learn* what a page contains, then go find the owner's own endpoint.

### What did not work (Folio's record)

Keep a list like this for every platform. It is worth more than the route that worked, because it stops
the next person from spending the same hour.

| Route | Result |
|---|---|
| `instagram.com/api/v1/users/web_profile_info/` + `x-ig-app-id` | **401** `{"require_login":true}` |
| the same with a `csrf` cookie from the front page | 401 |
| `i.instagram.com` with an Android UA | 401 |
| `?__a=1&__d=dis` | empty body |
| `/api/v1/feed/user/{u}/username/{u}/` | "Page Not Found" |
| mirrors: imginn / picuki / pixwox / picnob | 403 / 301 |
| `/explore/tags/…`, `/{u}/tagged/` | 302 redirect |
| `twstalker.com` (for X) | Cloudflare 403 |
| `cdn.syndication.twimg.com/widgets/followbutton/info.json` | empty |
| `curl https://x.com/{user}` | JS shell, no data |
| **a search engine's snippet of the profile page** | **worked** — the meta description carries `Joined … · N Following · N Followers` |

## When the platform's own numbers are wrong or missing

The spec is not the data. Nearly's spec asserted a 9arm video published in June 2026; the channel the
spec named has eight videos, all from 2013, and 2.8K subscribers. The real channel differs by a period
(`@9arm.`) and has 1.61M subscribers, 1,255 videos and 450,289,376 views.

**Trust what you fetched, not what the document says about it** — and when the document turns out to
have been right all along (it had warned about the missing period), say so. The original
`DEVELOPMENT-EXPERIENCE.md` recorded that warning, someone "fixed" it away, and the wrong channel shipped.

## Rate limits and manners

- Sleep between requests. One second is enough for most platforms.
- Retry with backoff, and treat a *payload* miss as a failure, not just a network error.
- Parallelise where the endpoint is cheap (`/player` tolerated 8 at once) and serialise where it is not.
- A single full run of Nearly's pipeline is ~190 requests. That is fine once; it is not fine on a cron.

## Verifying a source is the right one

Before building a pipeline on a source, answer these four:

1. **Is it the owner's own endpoint?** (If not, it will break.)
2. **Does it give exact values or pre-rounded ones?** (`579442` vs `579.4K` changes what the page can say.)
3. **What identifies the entity?** (A handle with a period is a different channel. A slug is not an id.)
4. **Does the same request twice give the same answer?** (`yt3.googleusercontent.com` re-encodes avatars —
   the same URL returned 49,965 and then 49,944 bytes. Compare *pixels*, not bytes.)
