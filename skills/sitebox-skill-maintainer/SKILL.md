---
name: sitebox-skill-maintainer
description: Create, audit, and update the SiteBox skills themselves. Use when editing any skill under skills/, adding a new SiteBox skill, checking skills against the real dashboard API and icons, resolving drift between skills/README/server behavior, or bumping skill versions after a feature change. Not for building sites — use sitebox-create, sitebox-design, and sitebox-config for that.
metadata:
  author: sitebox
  version: "1.0.0"
  updated: "2026-09-22"
---

# SiteBox Skill Maintainer (meta)

The SiteBox skills are the operating manual for building and running sites. This skill keeps them true. It governs:

| Skill | Owns | Source of truth for its facts |
|-------|------|-------------------------------|
| `sitebox-create` | Build/redesign/debug workflow, server template, performance requirements | `dashboard/server.js`, `sites/example-*/server.js` |
| `sitebox-design` | Visual system, audit, copy passes (humanizer/deslop) | design decisions; no code dependency |
| `sitebox-config` | Dashboard API, ports, lifecycle, logs, git policy | `dashboard/server.js`, `dashboard/public/js/main.js` |
| `sitebox-skill-maintainer` | This file — conventions and drift checks | `skills/` itself |

## Sources of truth (never duplicate, always check)

| Fact | Lives in | Mirrored in |
|------|----------|-------------|
| API endpoints + response shapes | `dashboard/server.js` | `sitebox-config` SKILL.md |
| Icon names | `dashboard/public/js/main.js` (`ICONS`) | `sitebox-config` "Icons" section |
| Port ranges | `README.md` | `sitebox-config`, `sitebox-create` |
| `server.js` template | `sitebox-create` SKILL.md | `sites/example-notes/server.js` (simpler variant) |
| Git policy for `sites.json` | `dashboard/data/README.md` + `.githooks/pre-commit` | `README.md`, `CONTRIBUTING.md`, `sitebox-config` |

If a fact needs changing, change every mirror in the same edit. The checker below finds most drift automatically.

## Maintain workflow

1. **Read before editing.** Open the target skill and the source of truth it mirrors. If the change is driven by a server/API change, read the diff in `dashboard/server.js` first.
2. **Run the checker** to see current drift:
   ```bash
   node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
   ```
3. **Make the smallest correct change.** Prefer editing an existing section over adding a new one; delete content that no longer earns its place (stale endpoints, invented features, duplicated tables).
4. **Keep the skills wired together.** When behavior moves between skills, update the cross-references. Skills point at each other by name (`sitebox-config` → `references/troubleshooting.md`); every pointer must resolve.
5. **Bump `metadata.version` and `metadata.updated`** in every skill whose content changed.
6. **Re-run the checker** until there are no errors. Warnings are allowed only when intentional — say why in the change.
7. **Update `README.md`** if a skill was added, renamed, or removed.

## Common tasks

| Task | Touch |
|------|-------|
| New API endpoint | `dashboard/server.js` → `sitebox-config` API table + relevant section → `README.md` table → checker leaf-token list if the verb is new |
| Endpoint removed/renamed | Same, plus grep all skills for the old path |
| New dashboard icon | `dashboard/public/js/main.js` → `sitebox-config` Icons section |
| Port range change | `README.md`, `sitebox-config`, `sitebox-create` (all three tables) |
| Server template change | `sitebox-create` template, then decide whether examples follow |
| git policy change | `dashboard/data/README.md`, `.githooks/pre-commit`, `README.md`, `CONTRIBUTING.md`, `sitebox-config` |

## Rules

- **No duplication.** One home per fact; other skills link to it. Duplicated tables are how the skills drift apart.
- **Never invent behavior.** If the API doesn't do it, the skill doesn't say it. The checker verifies endpoints exist; it cannot verify claims — read the code.
- **Skills are project-global.** They live in `skills/` for SiteBox's own agents. Do not move them into agent-specific folders (`.opencode/`, `.claude/`, `.agents/`) or add agent config files.
- **`dashboard/data/sites.json` is runtime state.** Never commit changes to it; the repo copy keeps only `example-notes` and `example-clock`. The checker warns when runtime entries are present.
- **Keep each `SKILL.md` under 500 lines.** Detail goes in `references/`. The description field is the trigger — it must say *when* to use the skill, not just what it is.
- **Explain the why.** Instructions that carry their reasoning survive updates better than bare MUSTs. Match the tone of the existing skills.
- **Verify with a real prompt.** After a substantive change, run a realistic task ("add a small site…") and watch whether the skill triggers and the steps hold; fix what the run reveals.

## Reference files

- `references/authoring.md` — frontmatter spec, description recipe, progressive disclosure, review loop, lineage
- `scripts/check-skills.mjs` — automated drift checker (see below)

## Checker

```bash
node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
```

Checks: frontmatter validity, name/folder match, description length and trigger phrasing, `metadata.version`, SKILL.md length, broken relative reference links, referenced `/api/...` endpoints existing in `dashboard/server.js`, icon lists matching `main.js`, `sites.json` containing only the two examples, README mentioning every skill, and the pre-commit hook existing. Exit code 1 on errors; warnings are printed but do not fail.
