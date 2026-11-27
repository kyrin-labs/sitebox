# 04 — Prompt & Skills

This doc covers the biggest conceptual change: **the router skill is dissolved**.
Its content becomes the chat's always-on doctrine, and the six working skills remain
loadable `SKILL.md` files.

## 1. Why the router disappears

Today the router (`~/.pi/agent/skills/sitebox/SKILL.md`) exists because a
general-purpose pi agent might be asked to do anything, so something must decide
*which* SiteBox skill applies and in what order. It is a skill because the agent
must opt into SiteBox mode.

A **dedicated SiteBox chat is always in SiteBox mode.** There is no "am I doing
SiteBox work?" question. Therefore:

- The routing doctrine belongs in the **system prompt**, always present, not in a
  skill the model must remember to load.
- The six working skills stay as skills, because they are large and only one or two
  apply per job; loading them on demand is correct.
- Keeping the router as a skill would mean either two homes for the same facts
  (repo + home server) or a skill that is always loaded anyway.

**Decision:** delete the router skill; move its content into `agent/system-prompt.md`
(doctrine) and `agent/guide.md` (human guide).

## 2. What survives from the router

The router's sections map as follows:

| Router section | Destination | Why |
|---|---|---|
| "Where the skills live" | system prompt (with rendered paths) | The agent must know the six skills |
| "Route the request" table | system prompt | Core orchestration logic |
| "The order, and why" | system prompt | The non-negotiable sequence |
| "Decisions that must be settled" | system prompt | Cross-skill defaults |
| "When recorded experience conflicts" | system prompt | Relay/Folio/Nearly priority |
| "Anti-patterns that span skills" | system prompt | Hard rules |
| "Definition of done" (8 gates) | system prompt | Acceptance the agent must self-check |
| "Reporting" | system prompt | How to phrase completion |
| Narrative/history ("Nearly's layout…") | guide.md | Humans like the stories; the model needs the rules |
| Path-resolution notes ("reach repo at …") | dropped | Paths are rendered, not guessed |

## 3. `agent/system-prompt.md` (source, tracked)

### 3.1 Structure

```markdown
# SiteBox Agent — operating doctrine

You are the SiteBox agent. You turn a request into a real, verified site that runs
inside this SiteBox instance. You are not a general assistant.

## Your environment
- Repo root: {{REPO_ROOT}}
- Dashboard API: {{DASHBOARD_URL}}
- Sites live in: {{SITES_DIR}}/<id>/
- Skills live in: {{SKILLS_DIR}}/

## The six skills
{{SKILL_LIST}}

Load a skill by reading its `SKILL.md` before doing that phase's work. The skill's
description tells you when it applies.

## Route the request
| The request sounds like | Load, in this order |
| ... (router table, verbatim) ... |

## The order, and why each step is where it is
1 design → 2 data → 3 create → 4 verify → 5 config
... (router text, trimmed) ...

## Decisions you must settle before code exists
... (router decisions table) ...

## Anti-patterns (never do these)
1..8 (router list)

## Definition of done
1..8 (router gates)

## Reporting
State numbers, not verdicts. Say which skills you used, which defaults you took,
what you deviated from, and what you could not verify.

## SiteBox specifics
- Ports: 4445 dashboard, 4500–4599 tools, 4600–4699 publishing, 4700–4799 widgets,
  4800–4899 experimental. Never 4444/4445 for a site.
- Register a site with `curl -X POST {{DASHBOARD_URL}}/api/sites -d '{...}'`.
- Start it with `curl -X POST {{DASHBOARD_URL}}/api/sites/<id>/start`.
- Prove it with `curl {{DASHBOARD_URL}}/api/sites/<id>/health`.
- `dashboard/data/sites.json` is runtime state. Never commit it.
```

### 3.2 Placeholders

| Token | Rendered from |
|---|---|
| `{{REPO_ROOT}}` | constant `/app` (or `process.cwd()` on bare metal) |
| `{{DASHBOARD_URL}}` | env `SITEBOX_DASHBOARD_URL` or `http://localhost:4445` |
| `{{SITES_DIR}}` | `{{REPO_ROOT}}/sites` |
| `{{SKILLS_DIR}}` | `{{REPO_ROOT}}/skills` |
| `{{SKILL_LIST}}` | generated bullet list (name — description) from discovered skills |
| `{{PORT_RANGES}}` | from the README table |
| `{{DATE}}` | render date |

Rendering is strict: a leftover `{{...}}` is a startup error. This prevents a
half-rendered doctrine reaching the model.

### 3.3 Length budget

Target **≤ 12 KB**. The router is ~11 KB, and the doctrine is always in context, so
trim:

- Drop the historical anecdotes (they live in `guide.md`).
- Keep tables and imperative rules.
- Do not duplicate the six skills' content — the doctrine points at them.

If the rendered prompt exceeds the budget, the build logs a warning (not an error)
so we notice drift.

## 4. `agent/guide.md` (source, tracked)

Human-facing. Shown by a "Guide / Help" button in the chat. Contents:

- What the chat can do (with example prompts).
- How settings work (the three fields).
- The order the agent follows, with the *why* stories.
- What "done" means (the eight gates) in plain language.
- Troubleshooting: agent won't start, wrong model, blank reply, site won't start.
- Security note: the agent can run shell commands; scope and limits.

This is documentation, not model input. It may be longer and friendlier.

## 5. Skills remain skills

`skills/` keeps exactly six directories:

```
skills/
├── sitebox-create/SKILL.md
├── sitebox-design/SKILL.md
├── sitebox-data/SKILL.md
├── sitebox-verify/SKILL.md
├── sitebox-config/SKILL.md
└── sitebox-skill-maintainer/SKILL.md
```

Discovery: `--skill /app/skills` (pi discovers directories containing `SKILL.md`
recursively). pi injects each skill's **name, description, and path** into the
system prompt automatically; the model reads the file when the task matches.

### 5.1 What changes in the skills

| Skill | Change |
|---|---|
| `sitebox-skill-maintainer` | Remove the "do not add a copy to `skills/`" rule for the router (the router is gone). Add a check that `agent/system-prompt.md` exists and contains the required sections. |
| `sitebox-verify/references/browser.md` | Replace the hard-coded Playwright path with env-driven discovery (`CHROME`, `PLAYWRIGHT`) so it works in the container. |
| Others | Unchanged. |

### 5.2 Drift checker

`skills/sitebox-skill-maintainer/scripts/check-skills.mjs` gains checks:

1. `skills/` contains exactly the six expected skill names (no router).
2. `agent/system-prompt.md` exists and contains the required headings
   (`Route the request`, `Anti-patterns`, `Definition of done`, …).
3. Every `{{TOKEN}}` used in `agent/system-prompt.md` has a renderer.
4. `agent/guide.md` exists.

## 6. Prompt assembly (what the model actually receives)

```
pi built-in system prompt
  + AGENTS.md/CLAUDE.md (if any; likely none)
  + skill index (name/description/path of the six)      ← pi injects
  + append-system-prompt: agent/system-prompt.md (rendered)  ← we inject
```

Because `--append-system-prompt` appends, we keep pi's tool-use and safety guidance
intact. We deliberately do **not** use `--system-prompt` (replace), which would
throw away pi's own operational instructions.

## 7. First-message behavior

On a brand-new session the chat SHOULD NOT auto-prompt. But it MAY show a **starter
card** (client-side only) with example requests:

- "สร้างเว็บรวมลิงก์เครื่องมือของฉัน"
- "โคลนหน้า YouTube ของช่อง X"
- "แก้ดีไซน์เว็บ <id> ให้ดูพรีเมียมขึ้น"
- "เว็บ <id> หน้าเว็บขาว ช่วยแก้"

These are UI affordances, not model turns. They map to the doctrine's routing table
so the first interaction is likely to route correctly.

## 8. Testing the doctrine

See [`10-testing.md`](./10-testing.md#3-doctrine--prompt-tests). Key cases:

- **Rendering**: no leftover tokens; paths correct; skill list complete.
- **Routing**: given the four example prompts, the agent loads the expected skills
  (assert by scanning the transcript for `read skills/sitebox-*/SKILL.md`).
- **Order**: a "create a site with real data" prompt shows `sitebox-data` loaded
  **before** any site file is written.
- **Done gate**: the agent's final report names the eight gates or states which it
  could not run.
