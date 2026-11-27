# 09 — Implementation Plan

Phased, with acceptance criteria and a file-by-file map. Each phase ends with
something demonstrable. Do not start a phase before the previous one's acceptance
criteria pass.

> **This branch is documentation-only.** Code starts after the design is reviewed.

## Phase 0 — Repo & prompt groundwork

**Goal:** the doctrine exists, the router is gone, secrets cannot be committed.

| Task | Output |
|---|---|
| 0.1 Move router content into `agent/system-prompt.md` (doctrine) + `agent/guide.md` (guide) | new files |
| 0.2 Delete the router from the agent's skill set (and plan the home-server symlink/removal) | skills/ has six dirs |
| 0.3 Update `sitebox-skill-maintainer` (drop the "no router copy" rule; add doctrine checks) | edited skill |
| 0.4 Extend `check-skills.mjs`: six skills, doctrine headings, placeholder renderers, guide exists | edited script |
| 0.5 `.gitignore` + `.githooks/pre-commit` for `agent.json` and `pi-agent/` | edited files |
| 0.6 Make `sitebox-verify/references/browser.md` portable (`CHROME`/`PLAYWRIGHT` env) | edited reference |

**Acceptance**

- `node skills/sitebox-skill-maintainer/scripts/check-skills.mjs` passes.
- `skills/` contains exactly six skill dirs; no router.
- `git add -f dashboard/data/agent.json` is blocked by the hook.
- `grep '{{'` of a rendered prompt finds no leftovers (renderer exists in Phase 1;
  here, assert the tokens are all known).

## Phase 1 — Agent core (headless)

**Goal:** talk to the agent with curl; no UI yet.

| Task | Output |
|---|---|
| 1.1 `dashboard/agent/config.js`: load/save/migrate `agent.json`; render `models.json`, `settings.json`, `system-prompt.md`; strict placeholder rendering | new module |
| 1.2 `dashboard/agent/rpc.js`: spawn, UTF-8 LF splitter (no readline), command correlation, event bus, stderr capture, readiness/exit handling | new module |
| 1.3 `dashboard/agent/manager.js`: lifecycle, turn lock, restart/backoff, session list/switch, message fetch | new module |
| 1.4 Routes in `server.js`: settings, status, chat, abort, restart, sessions, messages, guide | edited server |
| 1.5 SSE endpoint `/api/agent/events` with backpressure + status snapshot | edited server |
| 1.6 Unit tests for splitter, correlation, config rendering | tests |

**Acceptance**

- `curl -N /api/agent/events` streams text deltas and tool events for a real prompt.
- `POST /api/agent/chat` returns `202` and the turn completes with `agent_settled`.
- `kill -9` the pi child → dashboard still answers `/api/sites`; next chat respawns.
- Splitter test with a `U+2028` inside a JSON string passes.
- No `readline` import anywhere in `dashboard/agent/`.

## Phase 2 — Settings & chat UI

**Goal:** the primary flow works from the browser.

| Task | Output |
|---|---|
| 2.1 Settings UI (three primary fields + Advanced), Test connection, status line | edited HTML/JS/CSS |
| 2.2 `chat-md.js` safe markdown renderer | new module |
| 2.3 `chat.js`: store, transport, render, actions, mini/full mode switching | new module |
| 2.4 Mini-chat container + launcher + full-page container | edited HTML/CSS |
| 2.5 Tool cards, status dot, queue chips, dialogs (`confirm`/`notify`) | in chat.js |
| 2.6 localStorage persistence (open/mode/session/scroll) | in chat.js |
| 2.7 Accessibility pass (aria-live, focus, keyboard) | in chat.js/CSS |

**Acceptance**

- S1 (configure & chat) and S7 (UI continuity) pass.
- Switching mini↔full during a stream loses no events (verified by transcript).
- Closing the mini-chat mid-turn does not stop the agent; reopening shows progress.
- XSS test: a model message containing `<img src=x onerror=…>` renders as text.

## Phase 3 — Native skills & end-to-end

**Goal:** chat → running, verified site.

| Task | Output |
|---|---|
| 3.1 Launch with `--skill /app/skills`; verify all six are discovered | rpc.js args |
| 3.2 Port `browser.md` invocation; confirm chromium works in the container | edited reference + Dockerfile |
| 3.3 Dockerfile/compose per [`07-docker-and-deploy.md`](./07-docker-and-deploy.md) | edited files |
| 3.4 E2E scenario: "create a simple one-page site" | test |
| 3.5 Routing assertions (skills loaded in order) | test |

**Acceptance**

- S2, S3, S4, S6 pass.
- A site created via chat is registered, started, and `health.online: true`.
- The transcript shows `sitebox-design` and `sitebox-verify` were read.

## Phase 4 — Hardening & optional features

**Goal:** safer and more useful.

| Task | Output |
|---|---|
| 4.1 Permission gate extension + UI confirm wiring | new extension |
| 4.2 Port `searxng-search.ts`; enable via settings | new extension + settings |
| 4.3 Optional shared-secret token for `/api/agent/*` | server + settings |
| 4.4 Session management UI (list/open/new/rename) | chat.js |
| 4.5 Usage footer (tokens, cost, context %) via `get_session_stats` | chat.js |
| 4.6 Non-root container user (evaluate) | Dockerfile |
| 4.7 (Later) subagent support | new extension/design |

**Acceptance**

- S5 passes (secret hygiene).
- Permission gate blocks `rm -rf /` in a test and allows a normal command.
- Search tool returns results from the local SearXNG.

## File-by-file change map

| File | Phase | Change |
|---|---|---|
| `agent/system-prompt.md` | 0 | new (doctrine template) |
| `agent/guide.md` | 0 | new (UI guide) |
| `agent/extensions/searxng-search.ts` | 4 | new (ported) |
| `agent/extensions/permission-gate.ts` | 4 | new |
| `skills/sitebox-skill-maintainer/SKILL.md` | 0 | edited |
| `skills/sitebox-skill-maintainer/scripts/check-skills.mjs` | 0 | edited |
| `skills/sitebox-verify/references/browser.md` | 0/3 | edited (portable browser) |
| `dashboard/agent/config.js` | 1 | new |
| `dashboard/agent/rpc.js` | 1 | new |
| `dashboard/agent/manager.js` | 1 | new |
| `dashboard/agent/sse.js` | 1 | new |
| `dashboard/server.js` | 1 | edited (agent routes) |
| `dashboard/public/index.html` | 2 | edited (settings, chat mounts) |
| `dashboard/public/js/main.js` | 2 | edited (wire settings, chat init) |
| `dashboard/public/js/chat.js` | 2 | new |
| `dashboard/public/js/chat-md.js` | 2 | new |
| `dashboard/public/css/style.css` | 2 | edited |
| `Dockerfile` | 3 | edited (base, packages, pi, COPY skills/agent) |
| `docker-compose.yml` | 3 | edited (env) |
| `.gitignore` | 0 | edited |
| `.githooks/pre-commit` | 0 | edited |
| `README.md` | 3/4 | edited (document the agent) |
| `docs/native-agent/*` | — | this documentation |

## Dependency graph

```
0.1 ─┬─▶ 1.1 ─▶ 1.2 ─▶ 1.3 ─┬─▶ 1.4 ─▶ 1.5 ─▶ 2.1 ─▶ 2.3 ─▶ 2.4 ─▶ 3.4
0.5 ─┘                       │                 └─▶ 2.2 ─┘
0.2 ─────────────────────────┴─▶ 3.1 ─▶ 3.2 ─▶ 3.3
0.6 ─────────────────────────────────▶ 3.2
4.x depends on 3.4 passing
```

## Definition of done (feature)

All of S1–S7 in [`00-overview.md`](./00-overview.md#6-success-criteria-acceptance)
pass, the security checklist in
[`08-security.md`](./08-security.md#12-security-checklist-review-before-merge) is
green, and the drift checker passes.

## Effort estimate (rough, one engineer)

| Phase | Effort |
|---|---|
| 0 | 0.5 day |
| 1 | 1.5–2 days |
| 2 | 2–3 days |
| 3 | 1–1.5 days |
| 4 | 1–2 days (optional items) |

## Rollback

The feature is additive and gated by `configured`. If it misbehaves:

1. Do not open the chat; the dashboard behaves exactly as before.
2. `docker compose up -d` on `main` reverts the image.
3. `agent.json` and `pi-agent/` can be deleted safely (regenerated).

No existing route or site behavior changes.
