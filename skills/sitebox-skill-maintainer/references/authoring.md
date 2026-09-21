# Authoring SiteBox Skills

Conventions for writing and revising the skills under `skills/`. Lineage: Anthropic's `skill-creator` and the `writing-great-skills` / `write-a-skill` skills from skills.sh.

## Format

Every skill is a folder with a `SKILL.md`:

```
skills/<name>/
├── SKILL.md          (required)
├── references/*.md   (optional — loaded only when needed)
├── scripts/*         (optional — runnable, not loaded)
└── assets/*          (optional — templates, icons, fonts)
```

### Frontmatter

```yaml
---
name: sitebox-<topic>              # required, matches folder, ^[a-z0-9]+(-[a-z0-9]+)*$
description: ...                  # required, 1–1024 chars, says WHEN to use it
metadata:
  author: sitebox
  version: "1.0.0"
  updated: "YYYY-MM-DD"
---
```

Other agents may also read `license` and `compatibility`; they are optional. Unknown fields are ignored everywhere, so keep metadata in `metadata:`.

### The description is the trigger

Agents choose skills by reading only the name and description. A weak description means the skill never loads. Write one long sentence with: what it does, then explicit triggers — including casual phrasings and Thai/English keywords users actually type.

Bad:

> `sitebox-create`: Create professional static sites in SiteBox format.

Good:

> Build, verify, redesign, and debug static sites for SiteBox. Use when creating a new site under sites/, scaffolding server.js and public/, redesigning an existing site, registering a site in the dashboard, or when a site fails to start, shows a blank page, or needs a quality pass (performance, accessibility, copy).

Test the trigger after writing: would an agent pick this skill for "ช่วยทำเว็บนาฬิกาโลกให้หน่อย"? If not, add the phrasing that would.

## Progressive disclosure

Three layers, loaded at different times:

1. **Metadata** — name + description, always in context. Keeps triggering cheap.
2. **SKILL.md body** — loaded when the skill triggers. Keep under 500 lines. Workflows and decision rules only.
3. **references/** — loaded on demand. Deep checklists, playbooks, examples. Link them from SKILL.md with a phrase saying *when* to read them ("full playbook: `references/troubleshooting.md`").

If SKILL.md is growing past ~400 lines, move the detail down a layer instead of trimming meaning.

## Writing style

- Imperative voice: "Check the port", not "the port should be checked".
- Explain the why in one clause when the rule matters. Reasoning survives model changes; bare MUSTs don't.
- Concrete examples beat abstractions: show the command, the response, the before/after.
- One home per fact. If a table belongs to `sitebox-design`, other skills link to it by name.
- Tables for lookups (endpoints, ports, failure modes); prose for procedures.
- Keep the repo's language mix: Thai site content examples, English skill instructions unless the audience requires Thai.

## Review loop

Substantive skill changes deserve the same loop Anthropic's skill-creator uses:

1. Draft the change.
2. Run a realistic prompt with the skill (and mentally, one without) — e.g. "add a small site that shows the tide table for Chonburi".
3. Watch where the run goes wrong: did the skill trigger? Did it skip a step? Did it follow a stale command?
4. Rewrite based on what the run revealed — prefer generalizing a step over adding more rules.
5. Record the version bump + `updated` date when the content is settled.

Overfitting is the common failure: a rule that only fixes the test prompt while making other briefs worse. When a rule multiplies, ask which existing rule it replaces.

## Checklist before finishing a skill edit

- [ ] Frontmatter valid; name matches the folder; version/updated bumped.
- [ ] Description includes "Use when…" triggers in English and Thai where natural.
- [ ] SKILL.md < 500 lines; details pushed to `references/`.
- [ ] All links/pointers resolve; cross-skill references use the skill name.
- [ ] Every command and endpoint has been checked against `dashboard/server.js`.
- [ ] No duplicate tables; no invented behavior.
- [ ] `node skills/sitebox-skill-maintainer/scripts/check-skills.mjs` → no errors.
- [ ] README.md tree still accurate.

## Credits and influences

| Skill / source | Borrowed into |
|----------------|---------------|
| anthropics/skills — `skill-creator` | Progressive disclosure, description-as-trigger, review loop |
| anthropics/skills — `frontend-design` | Design-plan-then-critique, AI-tells discipline (sitebox-design) |
| anthropics/skills — `webapp-testing` | Verify-with-a-real-request approach (troubleshooting) |
| vercel-labs — `web-design-guidelines` | `file:line` audit output, review-style checklists |
| skills.sh — `humanizer`, `deslop` | Copy passes (`sitebox-design/references/writing.md`) |
| skills.sh — `redesign-existing-projects` | Redesign principles (preserve URLs/content) |
| skills.sh — `systematic-debugging`, `verification-before-completion` | Read-before-guess playbook, re-verify after fixes |
