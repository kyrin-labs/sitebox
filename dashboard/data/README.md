# dashboard/data

Runtime data for the SiteBox dashboard.

## Never commit `sites.json`

`dashboard/data/sites.json` is **runtime state**:

- the dashboard rewrites it whenever a site is added, edited, or deleted
- auto-detect appends entries for folders found in `sites/`
- a deployed server will always have a different `sites.json` than the repo

The copy committed to git must contain **only the two example sites**:
`example-notes` and `example-clock`. That is the baseline every clone gets.

When your local/deployed dashboard updates the file, leave those changes
uncommitted. If you staged them by accident:

```bash
git restore --staged dashboard/data/sites.json
git checkout -- dashboard/data/sites.json
```

## Enforcement

A pre-commit hook blocks staged changes to this file. Enable it once per clone:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit   # macOS / Linux (git needs the exec bit)
```

## Why

Keeping runtime state out of commits keeps the example baseline clean and
makes fresh clones deterministic — `node dashboard/server.js` always starts
with exactly the two examples, and auto-detect picks up any other site
folders on that machine.
