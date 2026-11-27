# Deploying SiteBox

The contract the scripts enforce, so a future session does not have to reconstruct it
from a conversation.

Everything runs from the **worktree** (`~/gh-project-ywei/sitebox`) and ships through the
**deploy clone** (`~/sitebox`, pull-only). The image is built from the deploy clone, so a
build before `git pull` silently ships the previous commit.

```
commit + push (worktree)  →  git pull (~/sitebox)  →  scripts/sitebox-deploy.sh --deploy
```

## The phases

Each phase is a gate: a failure stops the run. Half-applied deploys are worse than failed
ones, so nothing "continues on error".

| # | Phase | What it guarantees |
|---|---|---|
| 0 | baseline | Records the ids that are actually running to `~/backups/sitebox-baseline/`. This is the only trustworthy answer to "what was running before", because the dashboard keeps its PID map in memory and forgets everything on recreate. |
| 1 | pin image | Tags the running image `sitebox:pre-deploy-<stamp>` for rollback. |
| 2 | compose down | Stops the container — which kills every site process. |
| 3 | chown sweep | Container stopped, then `chown -R 1000:1000 sites dashboard/data` via a throwaway container. |
| 4 | up --build | Rebuilds and waits for `/api/sites` to answer 200. |
| 5 | smoke | Dashboard HTML, `/api/sites`, a 7 MB POST must get 413, and one start → HTTP 200 → stop round trip on `example-notes`. |
| 6 | restore sites | Starts back **only** what the baseline saw, then compares the running set as a set. Empty diff or the run fails. |

## Rollback

`docker tag` redirects the tag compose runs (the service has `build:` and no `image:`), so
rollback needs no rebuild:

```
scripts/sitebox-deploy.sh --rollback
# equivalent, by hand:
sg docker -c 'docker tag sitebox:pre-deploy-<stamp> sitebox-sitebox:latest'
sg docker -c 'cd ~/sitebox && docker compose down && docker compose up -d'
```

Rollback restores **code only**. Data is the backup's job.

## Backup

`scripts/sitebox-backup.sh`, nightly at 20:30 UTC (03:30 Asia/Bangkok), after the Outline
job. Also runnable by hand; the second run of a quiet day costs about a megabyte.

- `sites/` — hardlinked against the previous snapshot (`rsync --link-dest`), so unchanged
  files share inodes. A snapshot is still a plain tree: restore is `cp -a`, not a tar dance.
- `dashboard/data/` — `sites.json` and the whole `studio/` tree (threads, patches,
  snapshots, uploads, shots, settings). There is no token store any more.
- Verification is `diff -rq` against the source plus the registry count, not a hash of an
  archive. A file that changed mid-copy is reported, not hidden.
- Retention: 14 daily plus Sundays up to 8 weeks.

`scripts/sitebox-restore-drill.sh` proves a snapshot restores, on a throwaway copy: a
second dashboard on port 4492 reading the snapshot's data dir, the registry count, then a
site folder deleted from the copy and restored from the snapshot, served 200 by its own
`server.js`. Sundays 21:00 UTC, and optionally at the end of a deploy.

**Known limit, and the hook that fixes it:** source and backups sit on the same filesystem
here. `OFFBOX_TARGET=<rsync destination>` mirrors the whole backup root after each run
(`rsync -a --delete`), and an unset target logs "NOT configured" instead of implying
coverage. The mirror keeps every snapshot as a full tree — rsync does not carry hardlinks —
so budget about 426MB per retained snapshot on the target, and only deltas cross the wire
after the first copy. Pending: a destination (a tailnet peer needs Remote Login enabled, or
an external disk).

## What a gate failure means

| Message | Meaning |
|---|---|
| `API did not come up` | the container is unhealthy — `docker logs sitebox` |
| `N files still owned by root` | the chown sweep did not cover a path; run it again with the container down |
| `oversized body … expected 413` | the body cap regressed; every site is a child of this process |
| `running set differs` | the sites that came back are not the ones that left; compare the baseline file, do not bulk-start |
| `restore drill failed` | the snapshot does not restore — fix it before trusting the next backup |

## Security model

See the README. Short version: there is no login and no per-device permission. The gate is
the network — the dashboard listens on `0.0.0.0:4445` with no bind switch. Only
`/api/studio/*` is gated, by `x-studio-token` for scripts or the `studio_sid` cookie the
browser receives on load. There is no bearer-token store, no `/api/me`, and no dashboard
audit log; the only audit is `dashboard/data/studio/audit.jsonl`, written per applied or
undone patch.
