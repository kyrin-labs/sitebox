# 00 — Overview: Goals, Scope, Glossary

## 1. Why this exists

SiteBox already has everything a site needs except a brain. The six skills
(`sitebox-create`, `sitebox-design`, `sitebox-data`, `sitebox-verify`,
`sitebox-config`, `sitebox-skill-maintainer`) describe *how* to build a
production-quality site, but they only run when a human starts a pi agent in a
terminal, points it at the repo, and drives it by hand. The dashboard — the thing
the operator actually opens — knows nothing about the agent.

The result is friction at exactly the moment that matters: the operator sees the
dashboard, decides "I want a site for X", and then has to leave the dashboard,
open a terminal, find the repo, remember the router, and type a brief. The product
boundary and the work boundary are in different places.

This feature closes that gap. The agent becomes a first-class part of SiteBox:

- Configure it once in the dashboard (base URL, model, key).
- Talk to it from the dashboard (a chat widget).
- Watch it build a site and register it — in the same place the site then lives.

## 2. Goals

| # | Goal | How we know it is met |
|---|---|---|
| G1 | One place to configure the model | Settings exposes base URL + model + key; saving produces a working agent with no terminal step |
| G2 | Chat inside the dashboard | A mini-chat is visible on the dashboard and expands to a full chat page and back |
| G3 | Chat → running site | A single message ("create a site that …") results in a registered, started, health-checked site under `sites/` |
| G4 | The six skills drive quality | The agent loads the skills and follows the design → data → create → verify → config order |
| G5 | The dashboard survives the agent | Killing/hanging the agent process leaves the dashboard and every site running |
| G6 | Zero new runtime dependencies in the dashboard | `dashboard/` still runs on plain Node with no `node_modules` requirement |
| G7 | Secrets never reach the browser | The API key is write-only from the UI and masked in every response |

## 3. Non-goals (explicitly out of scope for the first delivery)

- **Multi-user / auth.** SiteBox is a single-operator tool on a trusted LAN. No
  login, no per-user sessions.
- **Remote/multi-tenant agents.** One agent, one operator, one box.
- **A model catalog / model picker.** The operator types a model name; we do not
  fetch `pi.dev` catalogs. `PI_OFFLINE=1`.
- **Fine-grained cost controls / quotas.** Usage is *shown*, not *enforced*.
- **Subagents in v1.** Deferred (ADR-005). The extension may be present but the
  single-agent loop ships first.
- **Rewriting the six skills.** They stay as they are; only the router is
  dissolved (ADR-003).
- **A general-purpose coding agent.** The chat is scoped to SiteBox work by its
  system prompt, but it is technically a full pi agent with bash — see
  [`08-security.md`](./08-security.md).

## 4. Scope boundary

```
IN SCOPE (this feature)                    OUT OF SCOPE
────────────────────────────────────────   ──────────────────────────────────────
dashboard/agent/          (new code)       six skills' content (unchanged)
dashboard/server.js       (new routes)     sites.json format (unchanged)
dashboard/public/*        (chat UI)        dashboard REST API for sites (unchanged)
agent/system-prompt.md    (new)            pi's own source
agent/guide.md            (new)
agent/extensions/*        (new, optional)
Dockerfile / compose      (changed)
.gitignore / pre-commit   (changed)
```

## 5. Personas and the primary flow

**Persona: the operator (L2S).** Runs SiteBox on the home server via Docker,
reaches the dashboard over the LAN, wants a site built without leaving the
browser.

**Primary flow (the one that must be perfect):**

```
1. Operator opens http://<host>:4445            → dashboard loads
2. Operator opens Settings, enters:
       Base URL  https://api.example.com/v1
       Model     some-model-name
       API Key   sk-...
   and clicks Save                              → agent process (re)starts
3. Operator clicks the mini-chat, types:
       "สร้างเว็บรวมลิงก์เครื่องมือของฉัน เป็นการ์ดสวยๆ"
4. Agent: loads sitebox (system prompt) → sitebox-design → sitebox-create
   → writes sites/<id>/… → curl POST /api/sites → curl POST /api/sites/<id>/start
   → sitebox-verify → reports done
5. The site card appears in the dashboard grid, running.
6. Operator closes the mini-chat; it disappears. Reopening restores the session.
```

Every design decision in these docs is judged against this flow.

## 6. Success criteria (acceptance)

The feature is **done** when all of the following hold, each demonstrated:

1. **S1 — Configure & chat.** With only the three settings filled, the operator
   sends a message and receives a streamed reply with live tool activity.
2. **S2 — Build a site.** A natural-language request produces a new folder under
   `sites/`, an entry in `sites.json`, a started process, and `health.online: true`.
3. **S3 — Quality gates ran.** The agent's transcript shows it loaded and followed
   `sitebox-design` and `sitebox-verify` (not just `sitebox-create`).
4. **S4 — Dashboard resilience.** `kill -9` the pi child; the dashboard keeps
   serving, sites keep running, and the next message transparently respawns it.
5. **S5 — Secret hygiene.** `GET /api/agent/settings` never contains the key; the
   key file is `0600` and cannot be committed.
6. **S6 — Zero-dep dashboard.** `dashboard/server.js` still starts with no
   `node_modules` present (the agent needs only the `pi` binary on `PATH`).
7. **S7 — UI continuity.** The mini-chat survives navigation to the full page and
   back, and an in-flight stream is not interrupted by the switch.

## 7. Glossary

| Term | Meaning |
|---|---|
| **Agent** | The embedded pi process running in RPC mode, controlled by the dashboard |
| **RPC mode** | `pi --mode rpc`: a long-lived child speaking JSONL commands on stdin, responses + events on stdout |
| **Session** | A persisted pi conversation (JSONL file) under `pi-agent/sessions/` |
| **Turn** | One assistant response plus its tool calls and results |
| **Settle** | `agent_settled`: pi has no more automatic work (retries/compaction/queue done) |
| **Steer / follow-up** | Messages injected into a running turn (steer) or after it (follow-up) |
| **Mini-chat** | The floating chat widget in the dashboard corner |
| **Full chat** | The expanded, page-filling chat view |
| **Provider** | The OpenAI-compatible endpoint, registered in pi as `sitebox` |
| **Agent dir** | pi's config directory (`PI_CODING_AGENT_DIR`), holding `models.json`, `settings.json`, `sessions/` |
| **Skill** | A `SKILL.md` directory pi advertises and loads on demand (the six sitebox-*) |
| **Doctrine** | The dissolved router's content, shipped as the chat system prompt |

## 8. Constraints inherited from SiteBox

These are not negotiable; the design works within them.

| Constraint | Source | Consequence |
|---|---|---|
| Dashboard is zero-dependency, pure Node | `README.md` | No SDK import; hand-rolled RPC client |
| `dashboard/data/sites.json` is runtime state, never committed | `.githooks/pre-commit` | Agent data must be excluded the same way |
| Docker uses `network_mode: host` | `docker-compose.yml` | `localhost:4445` works from inside the agent; site ports are host ports |
| Sites live at `sites/<id>/`, ports 4500–4899 | `README.md`, skills | The doctrine and prompt must state these |
| The dashboard spawns site processes with `child_process.spawn` | `dashboard/server.js` | Spawning pi is the same pattern; reuse log capture |
| Skills are plain `SKILL.md` dirs under `skills/` | repo layout | `--skill /app/skills` discovers them recursively |

## 9. What changes for the operator (before/after)

**Before**

```bash
ssh l2s@192.168.1.102
cd ~/sitebox
pi                       # interactive TUI
> สร้างเว็บ...
```

**After**

```
open http://192.168.1.102:4445
settings → base URL, model, key
chat → "สร้างเว็บ..."
```

The terminal path keeps working for power users (the repo skills are unchanged),
but it is no longer required.
