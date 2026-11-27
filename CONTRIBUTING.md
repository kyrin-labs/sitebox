# Contributing to SiteBox

## Adding a New Site

1. Scaffold it with the engine — it writes the manifest and registers the site:

   ```bash
   sitebox --root ~/sitebox new your-site-name \
     --name "Your Site" --port 4500 --description "One real line"
   ```

   That creates `sites/your-site-name/sitebox.toml` and `public/index.html`.
   You do **not** write a server: the engine serves every static site from one
   process (traversal guard, ETag/304, gzip, Range, real 404). A site that owns
   an HTTP API declares `serve = "node"` and keeps its own `server.js`.

2. Build and check:

   ```bash
   sitebox --root ~/sitebox check your-site-name
   sitebox --root ~/sitebox verify your-site-name      # starts it, runs HTTP checks, stops it
   ```

3. Start it for real through the dashboard API (`POST /api/sites/<id>/start`)
   or the UI; `ok:true` means the port was confirmed listening.

4. If it fails, `sitebox logs <id> -n 100` (persisted in `logs/<id>.jsonl`) and
   `skills/sitebox-config/references/troubleshooting.md`.

5. Quality gate: `skills/sitebox-design/references/audit.md`, the checks in
   `skills/sitebox-create/references/performance.md`, and the copy passes in
   `skills/sitebox-design/references/writing.md`.

6. **Prove it, don't eyeball it.** `skills/sitebox-engine/SKILL.md` lists the
   check catalog (`SB0xx` static, `SB1xx` HTTP). A site is done when `check`
   and `verify` are green, not when it renders once.

7. **If the content is real**, build the pipeline first:
   `skills/sitebox-data/SKILL.md`.

## Conventions

- Directory names: `kebab-case` (validated by the engine).
- Each site is self-contained; no shared dependencies between sites.
- Prefer vanilla HTML/CSS/JS — no build step.
- The port lives in `sitebox.toml`; the engine passes `PORT` to `serve="node"`
  children. Never hardcode a second port.
- Ports must be unique and inside the range table in `README.md`.

## Personal sites never enter git

The repository ships exactly two example sites. Everything else under `sites/`
is personal and is excluded by `.gitignore`, and the pre-commit hook blocks it
even if someone forces an add. Do not commit a site's content, its
`sitebox.toml`, its data, or its manifests.

`dashboard/data/sites.json` is runtime state and is blocked separately: the
committed copy only ever describes the two examples.

Enable the hooks once per clone:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit
```

## Engine changes

The Rust engine lives in `engine/`:

- `engine/src/serve.rs` — the shared static server
- `engine/src/supervisor.rs` — listeners, node children, persisted logs
- `engine/src/checks.rs` — the check catalog and negative controls
- `engine/src/manifest.rs` — `sitebox.toml` and its JSON Schema
- `engine/src/dashboard.rs` — the API the UI talks to
- `engine/src/bin/` — `sitebox`, `siteboxd`, `sitebox-mcp`

```bash
cd engine
cargo test          # unit tests + negative controls, no network/model needed
cargo build --release
```

`sitebox self-test` must stay green: every check needs a fixture it fails on.

After changing API endpoints, icons or ports, update the mirrored docs and run:

```bash
node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
```

## Dashboard UI changes

`dashboard/public/` is unchanged vanilla HTML/CSS/JS; it talks to the engine's
API on port 4445. `dashboard/server.js` no longer exists — the backend is
`siteboxd`.

## Skills

Skills live in `skills/` and are project-global. Conventions and maintenance:
`skills/sitebox-skill-maintainer/SKILL.md`.

## Running Locally

```bash
cd engine && cargo run --release --bin siteboxd -- \
  --root .. --port 4445 --ui ../dashboard/public
# http://localhost:4445
```

Or with Docker: `docker compose up -d --build`.
