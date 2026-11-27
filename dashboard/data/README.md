# dashboard/data

Runtime data for the SiteBox dashboard.

## Never commit `sites.json`

`dashboard/data/sites.json` is **runtime state**:

- the engine rewrites it whenever a site is added, edited, or deleted
- auto-detect appends entries for folders with a `sitebox.toml` (or a legacy
  `server.js`) found in `sites/`
- a deployed server will always have a different `sites.json` than the repo

The copy committed to git must contain **only the two example sites**:
`example-notes` and `example-clock`. That is the baseline every clone gets.

When your local/deployed dashboard updates the file, leave those changes
uncommitted. If you staged them by accident:

```bash
git restore --staged dashboard/data/sites.json
git checkout -- dashboard/data/sites.json
```

## Personal sites are also private

The repository ships two example sites. Everything else under `sites/` is
personal: its content, `sitebox.toml`, data and assets must not be committed.
`.gitignore` excludes them and the pre-commit hook refuses them even when they
are force-added.

## Enforcement

The pre-commit hook blocks staged changes to this file and any personal site
path. Enable it once per clone:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit   # macOS / Linux (git needs the exec bit)
```

## Why

Keeping runtime state out of commits keeps the example baseline clean and makes
fresh clones deterministic: `siteboxd --root .` always starts with exactly the
two examples, and auto-detect picks up any other site folders on that machine.
