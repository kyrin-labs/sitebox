# Contributing to SiteBox

## Adding a New Site

1. Create `sites/your-site-name/` with `server.js` + `public/` (see `sites/example-notes/` or the `sitebox-create` skill).
2. Register it through the API — not by hand-editing the committed JSON:

   ```bash
   curl -X POST http://localhost:4445/api/sites \
     -H 'Content-Type: application/json' \
     -d '{"id":"your-site-name","name":"Your Site","port":4500,"path":"sites/your-site-name"}'
   ```

3. Start and verify: `POST /api/sites/your-site-name/start` — it returns `ok:true` only when the port is confirmed listening. Follow with `GET /api/sites/your-site-name/health`.
4. If it fails, read `GET /api/sites/your-site-name/logs` before guessing. Full playbook: `skills/sitebox-config/references/troubleshooting.md`.
5. Quality gate before calling it done: `skills/sitebox-design/references/audit.md`, the checks in `skills/sitebox-create/references/performance.md`, and the copy passes in `skills/sitebox-design/references/writing.md`.
6. **Prove it, don't eyeball it.** `skills/sitebox-verify/SKILL.md` — a render harness that compares every displayed value against the source, a negative control for every guard, a real-browser layout check, and a checksum-verified deploy.
7. **If the content is real**, build the pipeline first: `skills/sitebox-data/SKILL.md`.

## Conventions

- Directory names: `kebab-case` (validated by the API).
- Each site is fully self-contained; no shared dependencies between sites.
- Prefer vanilla HTML/CSS/JS — no build step.
- Use `process.env.PORT` with a fallback; the dashboard passes the port from `sites.json`.
- Ports must be unique and inside the range table in `README.md`.

## Never commit `dashboard/data/sites.json`

This file is runtime state. The dashboard rewrites it on add/edit/delete and
auto-detect adds entries. The committed copy must contain **only**
`example-notes` and `example-clock`.

- Disable the change with: `git restore --staged dashboard/data/sites.json`
- Restore the tracked baseline with: `git checkout -- dashboard/data/sites.json`
- The `.githooks/pre-commit` hook blocks it. Enable once per clone:

  ```bash
  git config core.hooksPath .githooks
  chmod +x .githooks/pre-commit   # macOS / Linux
  ```

Details: `dashboard/data/README.md`.

## Dashboard Changes

The dashboard is at `dashboard/` — vanilla HTML/CSS/JS served by `dashboard/server.js`.

- `dashboard/public/` — frontend files
- `dashboard/data/sites.json` — runtime site config (see the policy above)
- `dashboard/server.js` — backend (API + static serving + process management)

After changing API endpoints, icons, or ports, update the mirrored docs and run:

```bash
node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
```

## Skills

Skills live in `skills/` and are project-global (they are not agent-specific config).
Conventions and maintenance workflow: `skills/sitebox-skill-maintainer/SKILL.md`.

When two recorded lessons conflict, the earlier experience wins: **Relay + Folio** first,
then Nearly. The resolutions already made this way are listed in
`sitebox-data` and `sitebox-design` — do not re-litigate them without changing both sides.

## Running Locally

```bash
node dashboard/server.js
# http://localhost:4445
```
