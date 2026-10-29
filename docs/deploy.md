# Deploying SiteBox

The contract the scripts enforce, so a future session does not have to reconstruct it
from a conversation.

Everything here runs from the **worktree** (`~/gh-project-ywei/sitebox`) and ships through
the **deploy clone** (`~/sitebox`, pull-only). The image is built from the deploy clone, so
a build before `git pull` silently ships the previous commit.

```
commit + push (worktree)  →  git pull (~/sitebox)  →  scripts/sitebox-deploy.sh --deploy
```

## The eight phases

Each phase is a gate: a failure stops the run. Half-applied deploys are worse than
failed ones, so nothing "continues on error".

| # | Phase | What it guarantees |
|---|---|---|
| 0 | baseline | Records config + listeners + HTTP for every site to `~/backups/sitebox-baseline/`. This is the only trustworthy answer to "what was running before", because the dashboard keeps its PID map in memory and forgets everything on recreate. |
| 1 | pin image | Tags the running image `sitebox:pre-mcp-<stamp>` and writes `ROLLBACK_TAG` + `ROLLBACK_IMAGE_ID`. |
| 2 | compose down | Stops the container — which kills every site process. |
| 3 | chown sweep | Container stopped, then `chown -R 1000:1000 sites dashboard/data` via a throwaway container. Gate: **zero** root-owned files. |
| 4 | mint | Host CLI writes `dashboard/data/tokens.json`. Skipped when the existing key still authenticates — a redeploy must not rotate a key out from under a running client. Must run after phase 3, never before: a root-owned store cannot be written by the operator, which is the recovery path. |
| 5 | up --build | Waits for `/api/health`. Gate: `features` must include `auth` and `port0`, otherwise the image is stale and the script says so. |
| 6 | smoke | Auth (401/200/wrong token), cookie session, favicon path not 401, capability split, LAN delete matches `mayWrite`, audit delta, 413 on an oversized body, tokens not tracked, file modes. |
| 6b | MCP round trip | `mcp/test/smoke-live.js` against production: create → check → logs → delete, port freed. |
| 7 | restore sites | Starts back **only** what the baseline saw, then re-baselines and compares `(id, port)` as a **set**. Empty diff or the run fails. |
| 8 | restore drill | Opt-in (`--with-restore-drill`): restores the latest snapshot into a temp dir and serves it on temp ports. |

Why the baseline instead of inference: after a recreate, "all sites should come back" is
wrong by default — on this host 4 of 14 were actually running.

## Rollback

`docker tag` redirects the tag that compose runs (the service has `build:` and no `image:`),
so rollback needs no rebuild:

```
scripts/sitebox-deploy.sh --rollback
# equivalent, by hand:
sg docker -c 'docker tag sitebox:pre-mcp-<stamp> sitebox-sitebox:latest'
sg docker -c 'cd ~/sitebox && docker compose down && docker compose up -d'
```

Rollback restores **code only**. Data is the backup's job.

## Backup

`scripts/sitebox-backup.sh`, nightly at 20:30 UTC (03:30 Asia/Bangkok), after the Outline
job. Also runnable by hand; the second run of a quiet day costs about a megabyte.

- `sites/` — hardlinked against the previous snapshot (`rsync --link-dest`), so unchanged
  files share inodes. A snapshot is still a plain tree: restore is `cp -a`, not a tar dance.
- `dashboard/data/` — `sites.json`, `tokens.json`, `audit.log` (+ rotations), `sessions.json`.
  A restore without `tokens.json` leaves every client unable to authenticate, which reads
  as "the restore failed".
- Verification is `diff -rq` against the source plus the registry count, not a hash of an
  archive. A file that changed mid-copy is reported, not hidden.
- Retention: 14 daily plus Sundays up to 8 weeks.

`scripts/sitebox-restore-drill.sh` proves a snapshot restores, on a throwaway copy: a
second dashboard on port 4492 reading the snapshot's data dir, 401 without a key and 200
with the key the client already holds, the registry count, then a site folder deleted from
the copy and restored from the snapshot, served 200 by its own `server.js`. Sundays 21:00
UTC, and optionally at the end of a deploy.

**Known limit, and the hook that fixes it:** source and backups sit on the same filesystem
here. `OFFBOX_TARGET=<rsync destination>` mirrors the whole backup root after each run
(`rsync -a --delete`), and an unset target logs "NOT configured" instead of implying
coverage. The mirror keeps every snapshot as a full tree — rsync does not carry hardlinks —
so budget about 426MB per retained snapshot on the target, and only deltas cross the wire
after the first copy. Pending: a destination (a tailnet peer needs Remote Login enabled, or
an external disk).

## Tokens

Minted from the host — the only path that works when the dashboard will not let you in:

```
node dashboard/token.js add <label> --scope read|write --print   # key is the last line
node dashboard/token.js list
node dashboard/token.js revoke <label>
```

Current labels: `mcp` (deploy/smoke), `mcp-hermes`, `mcp-opencode` (one per client, so
`lastUsed` and the audit actor mean something). Keys live at `~/.config/sitebox/<label>.token`,
mode 600. The store keeps sha256 hashes only and is re-read per request, so a mint or revoke
takes effect without a restart.

## What a gate failure means

| Message | Meaning |
|---|---|
| `/api/health does not advertise auth` | the image predates the commit — `git pull` in `~/sitebox` |
| `N files still owned by root` | the chown sweep did not cover a path; run phase 3 again with the container down |
| `mint … EACCES` | `dashboard/data` is not writable by uid 1000 — chown before minting |
| `set-diff` not empty | the sites that came back are not the ones that left; do not bulk-start, compare the baseline files |
| `oversized body … expected 413` | the body cap regressed; every site is a child of this process |
| `restore drill failed` | the snapshot does not restore — fix before trusting the next backup |

## Security model

See the README section. Short version: there are no per-device permissions. The gate is the
network (`SITEBOX_BIND`), tokens exist for non-browser clients, `Origin` stops CSRF, and
`dashboard/data/audit.log` records every write attempt — including the refused ones.
