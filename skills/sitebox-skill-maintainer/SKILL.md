---
name: sitebox-skill-maintainer
description: Create, audit, and update the SiteBox skills themselves. Use when editing any skill under skills/, adding a new SiteBox skill, checking skills against the real dashboard API and icons, resolving drift between skills/README/server behavior, wiring a new skill into the others, or bumping skill versions after a feature change. Not for building sites — use sitebox-create, sitebox-design, sitebox-data, sitebox-verify, and sitebox-config for that.
metadata:
  author: sitebox
  version: "2.0.0"
  updated: "2026-09-22"
---

# SiteBox Skill Maintainer (meta)

The SiteBox skills are the operating manual for building and running sites. This skill keeps them true. It governs:

| Skill | Owns | Source of truth for its facts |
|-------|------|-------------------------------|
| `sitebox-create` | Build/redesign/deploy/debug workflow, server template, serving additions | `dashboard/server.js`, `sites/example-*/server.js` |
| `sitebox-design` | Visual system, image geometry, audit, copy passes (humanizer/deslop), accepted deviations | design decisions; no code dependency |
| `sitebox-data` | Real-data pipelines, provenance, refreshability, fixtures, coverage floors, asset integrity | the platforms themselves; the site's `tools/` |
| `sitebox-verify` | Render harness, negative controls, real-browser layout proof, HTTP sweep, deploy mirror | the site's `tools/`; the running server |
| `sitebox-config` | Dashboard API, ports, lifecycle, logs, git policy | `dashboard/server.js`, `dashboard/public/js/main.js` |
| `sitebox-skill-maintainer` | This file — conventions and drift checks | `skills/` itself |

There is also a `sitebox` **router** skill that lives with the pi agent, not in this repo. It decides
which of the above apply to a whole job and in what order. It is not checked by this script and is not
tracked in git here — do not add a copy to `skills/`, because the same facts would then have two homes.

## Sources of truth (never duplicate, always check)

| Fact | Lives in | Mirrored in |
|------|----------|-------------|
| API endpoints + response shapes | `dashboard/server.js` | `sitebox-config` SKILL.md |
| Icon names | `dashboard/public/js/main.js` (`ICONS`) | `sitebox-config` "Icons" section |
| Port ranges | `README.md` | `sitebox-config`, `sitebox-create` |
| `server.js` template | `sitebox-create` SKILL.md | `sites/example-notes/server.js` (simpler variant) |
| Git policy for `sites.json` | `dashboard/data/README.md` + `.githooks/pre-commit` | `README.md`, `CONTRIBUTING.md`, `sitebox-config` |
| Which skill owns what | `sitebox-skill-maintainer` (the table above) | each SKILL.md's "Related skills" section |

If a fact needs changing, change every mirror in the same edit. The checker below finds most drift automatically.

## When lessons conflict, earlier experience wins

This repo has two tiers of recorded experience, and the owner set their authority explicitly:

```
1  Relay + Folio      the doctrine — these were the two sites whose lessons were distilled on purpose
2  Nearly             newest, most specific, most likely to over-fit one site
```

When a rule in one conflicts with a rule in another, the lower number wins, and the loser is recorded
(not deleted). The conflicts that have already been resolved this way are listed in `sitebox-data` and
`sitebox-design`. **Do not re-litigate a resolved conflict** — if you believe the resolution is wrong, change
it in one place and update the loser's entry in the same edit.

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
| New skill | New folder + SKILL.md → add a row to the ownership table above → add to every sibling's "Related skills" → `README.md` tree + table → checker |
| A lesson learned on a real site | Put it in the skill that owns that phase, **with the failure it prevents** — not in a new section of a skill that happens to mention it. If two skills could own it, the ownership table decides |
| A conflict between skills | Resolve by the tier order above, then record the loser in the same edit |

## Rules

- **No duplication.** One home per fact; other skills link to it. Duplicated tables are how the skills drift apart.
- **Never invent behavior.** If the API doesn't do it, the skill doesn't say it. The checker verifies endpoints exist; it cannot verify claims — read the code.
- **Skills are project-global.** They live in `skills/` for SiteBox's own agents. Do not move them into agent-specific folders (`.opencode/`, `.claude/`, `.agents/`) or add agent config files.
- **`dashboard/data/sites.json` is runtime state.** Never commit changes to it; the repo copy keeps only `example-notes` and `example-clock`. The checker warns when runtime entries are present.
- **Keep each `SKILL.md` under 500 lines.** Detail goes in `references/`. The description field is the trigger — it must say *when* to use the skill, not just what it is.
- **Explain the why.** Instructions that carry their reasoning survive updates better than bare MUSTs. Match the tone of the existing skills.
- **Verify the artifact, not the report.** A change is done when you have re-read the file and re-run the checker — not when the changelog says "fixed". Repo experience: a CSS fix was declared complete while an orphan declaration was still sitting in the file, and a summary table listed files that did not match the folder. Residual damage is the most common form of a "finished" edit, so look for the stray leftover (an orphan rule, a stale row, a missing file), not only the intended result.
- **Verify with a real prompt.** After a substantive change, run a realistic task ("add a small site…") and watch whether the skill triggers and the steps hold; fix what the run reveals.

## Reference files

- `references/authoring.md` — frontmatter spec, description recipe, progressive disclosure, review loop, lineage
- `scripts/check-skills.mjs` — automated drift checker (see below)

## Checker

```bash
node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
```

Checks: frontmatter validity, name/folder match, description length, trigger phrasing **and anti-trigger phrasing**, `metadata.version`, SKILL.md length, broken relative reference links (in `SKILL.md` **and** in every `references/*.md`), cross-skill name references resolving to a real skill, referenced `/api/...` endpoints existing in `dashboard/server.js`, icon lists matching `main.js`, `sites.json` containing only the two examples, README mentioning every skill, and the pre-commit hook existing. Exit code 1 on errors; warnings are printed but do not fail.

**The checker is a guard, so it needs a negative control too.** Before trusting a new check, break the
thing it looks for and confirm it reports an error. A checker that has only ever printed `0 error(s)` is in
the same category as every other guard that never fired.
