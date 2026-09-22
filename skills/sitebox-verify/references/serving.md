# Serving and Deploying

The last layer: proving that what the server sends is what you verified. A page can pass every other check
and still be broken on the running site, because the bytes never got there.

## The HTTP sweep

Walk every path the page references and request it for real.

| What | Expect |
|---|---|
| HTML, favicon, data JSON | 200 |
| Every image, font, stylesheet, script | 200, and a `content-type` matching the extension |
| HTML | `content-encoding: gzip` and an `ETag` |
| Same request with `If-None-Match: <etag>` | **304**, same ETag |
| A path that does not exist | **404** — not a 200 with the index page |
| `/../../etc/passwd` | **404** |

Report the count: `995 served, 0 problem(s)`. The number matters — a sweep that only checked the home page
would also print `0 problem(s)`.

**A silent 200 for a missing asset is the worst outcome.** If the server falls back to `index.html` for
anything it cannot find, a broken image path returns a valid HTML document with a 200 and the browser
quietly renders nothing. Return a real 404 (`sitebox-create`, step 6).

## gzip, ETag and `no-cache`

```js
const body = fs.readFileSync(fp);
const etag = '"' + crypto.createHash('sha1').update(body).digest('hex').slice(0, 20) + '"';
if (req.headers['if-none-match'] === etag) { res.writeHead(304); res.end(); return; }
const headers = { 'Content-Type': MIME[path.extname(fp)] || 'text/plain',
                  ETag: etag, 'Cache-Control': 'no-cache' };
if (/\.(html|css|js|json|svg)$/.test(fp)) {
  headers['Content-Encoding'] = 'gzip';
  res.writeHead(200, headers); res.end(zlib.gzipSync(body));
} else {
  res.writeHead(200, headers); fs.createReadStream(fp).pipe(res);
}
```

`no-cache` does **not** mean "do not cache". It means "revalidate before using". On localhost that is
exactly right: an edit shows immediately, and an unchanged file still answers 304 instead of re-downloading.

A plain `Cache-Control: public, max-age=3600` is how a fixed image keeps rendering cropped for an hour
while you debug the wrong thing. That happened, and it cost a whole round.

## Deploy as a mirror

The single quietest failure in this repo: Nearly's `index.html` was copied and nothing else. The page
returned 200, `health.online` was `true`, the log was clean — and 22 new images plus 180 commenter avatars
were 404. Every local check was green because every local file existed.

A deploy script must do three things and fail loudly:

1. **Mirror** everything the page can reference: `index.html`, `favicon.svg`, `data/*.json`, `img/**`,
   `fonts/**`.
2. **Delete** files on the server that no longer exist locally. Without this the remote tree is not a
   mirror, and stale files accumulate invisibly.
3. **Compare checksums** on both sides and `exit 1` on any mismatch.

```bash
# local
( find public/img -type f | sort | xargs sha256sum
  sha256sum public/index.html public/favicon.svg public/data/*.json ) \
  | sed 's/ \*/  /' | LC_ALL=C sort -k2,2 > /tmp/local.sums

# remote
ssh home-server 'cd ~/sitebox/sites/<id> && ( find public/img -type f | sort | xargs sha256sum; \
  sha256sum public/index.html public/favicon.svg public/data/*.json ) \
  | sed "s/ \*/  /" | LC_ALL=C sort -k2,2' > /tmp/remote.sums

diff -q /tmp/local.sums /tmp/remote.sums && echo "byte-identical: $(wc -l < /tmp/local.sums) files"
```

Two traps, both paid for:

- **`sha256sum` marks binary files with `*`** (`<hash> *path` vs `<hash>  path`). Normalise with
  `sed 's/ \*/  /'` or every binary file reports as different.
- **Sort order is locale-dependent.** `LC_ALL=C sort -k2,2` on both sides, or the two lists never compare
  equal even when the files are identical.

### Do the remote delete in one call

```bash
# WRONG — ssh consumes stdin, so the loop deletes one file per run
while read -r f; do ssh home-server "rm -f '$f'"; done < stale.txt

# RIGHT — one ssh, one xargs
ssh home-server "cd ~/sitebox/sites/<id> && xargs -a - rm -f" < stale.txt
```

This is not a style preference: the wrong version deletes exactly one file and reports success, so the next
run deletes another, and the tree is never a mirror.

## Order of operations

```
build → verify (harness, controls) → browser check → deploy mirror + checksum
      → restart → health → sweep the served bytes → read the logs
```

The last three are not ceremony:

- **`restart`, not `start`.** A running site keeps serving the old file. `POST :id/restart` frees the port,
  waits, and starts again.
- **`health` after the deploy**, not before. A pre-deploy health check proves the previous deploy worked.
- **Sweep the served bytes after the restart**, because the checksum compared the disk and the sweep compares
  what the server actually returns. They are different claims.

## Reporting

```
sweep.js     995 served, 0 problem(s)
             html: text/html; charset=utf-8, gzip, etag "…"
             revalidation: 304 with the same ETag
             missing file -> 404, traversal attempt -> 404
checksum     990/990 files byte-identical, local == deployed
dashboard    online:true · port 4504 · 0 errors in the log
```

If the checksum line does not say `byte-identical`, the deploy is not done — regardless of what the page
looks like in a browser, which may be serving a cached copy of the previous deploy.
