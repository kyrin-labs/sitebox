# Native Agent for SiteBox — Design Documentation

This folder is the **single home for the design of the native pi agent inside SiteBox**.
It documents *what we will build, why, and exactly how* — before any code is written.

> **Status: DESIGN ONLY.** Nothing in this folder is implemented yet.
> The implementation branch is `feat/native-agent-rpc`. See
> [`09-implementation-plan.md`](./09-implementation-plan.md) for the phased build order.

---

## What this feature is

Today SiteBox is a zero-dependency dashboard that manages static sites, and the
SiteBox skills are driven by a **separate** pi agent that the operator runs by hand
(on the home server, or on `L2S-Shell`). The agent and the product are two worlds.

This feature makes the agent **native to SiteBox**:

- The operator configures **one OpenAI-compatible endpoint** (base URL, model name,
  API key) in the dashboard settings.
- A **chat interface** lives inside the dashboard — a small floating widget in the
  corner that expands into a full-page chat.
- From that chat the operator can say *"create a site that does X"* and the agent
  builds it: it designs, gathers real data, scaffolds, deploys, verifies, and
  registers the site in the dashboard — using the six SiteBox skills as its manual.
- The agent runs as a **pi RPC subprocess**, so the dashboard stays zero-dependency
  and never dies with the agent.

In one sentence: **SiteBox gains an embedded pi agent that turns a chat message into
a running, verified site.**

---

## Locked decisions (do not re-litigate without an ADR update)

| # | Decision | Where |
|---|---|---|
| D1 | Embed pi as an **RPC subprocess** (`pi --mode rpc`), hand-rolled JSONL client, no SDK dependency | [ADR-001](./11-risks-and-decisions.md#adr-001) |
| D2 | Docker base moves to **`node:22-bookworm-slim` + chromium** | [ADR-002](./11-risks-and-decisions.md#adr-002) |
| D3 | **No router skill.** The router becomes the chat's default system prompt + a UI guide | [ADR-003](./11-risks-and-decisions.md#adr-003) |
| D4 | Chat UX = **floating mini-chat** that expands to a **full-page chat** and back | [ADR-004](./11-risks-and-decisions.md#adr-004) |
| D5 | **Single-agent loop first**; subagents deferred to a later phase | [ADR-005](./11-risks-and-decisions.md#adr-005) |
| D6 | Settings page exposes **only base URL, model name, API key** (advanced optional) | [ADR-006](./11-risks-and-decisions.md#adr-006) |
| D7 | Secrets live in `dashboard/data/agent.json` (gitignored, `0600`), never sent to the browser | [ADR-007](./11-risks-and-decisions.md#adr-007) |
| D8 | pi version is **pinned** in the image | [ADR-008](./11-risks-and-decisions.md#adr-008) |

---

## Document map

Read in order for the full picture, or jump to the one you need.

| Doc | Contents |
|---|---|
| [`00-overview.md`](./00-overview.md) | Goals, non-goals, scope, glossary, success criteria |
| [`01-architecture.md`](./01-architecture.md) | Components, process model, data flow, sequence diagrams |
| [`02-rpc-protocol.md`](./02-rpc-protocol.md) | JSONL framing, commands used, events consumed, lifecycle, error handling |
| [`03-config-and-models.md`](./03-config-and-models.md) | `agent.json`, generated `models.json`, provider config, settings API, secrets |
| [`04-prompt-and-skills.md`](./04-prompt-and-skills.md) | Router removal, `system-prompt.md`, `guide.md`, skill discovery |
| [`05-backend-api.md`](./05-backend-api.md) | Every new HTTP route, request/response schemas, SSE format |
| [`06-frontend-ui.md`](./06-frontend-ui.md) | Mini-chat + full page, state machine, components, accessibility |
| [`07-docker-and-deploy.md`](./07-docker-and-deploy.md) | Dockerfile, env vars, volumes, gitignore, pre-commit guard |
| [`08-security.md`](./08-security.md) | Threat model, API-key handling, bash tool, permission gate, hardening |
| [`09-implementation-plan.md`](./09-implementation-plan.md) | Phases, tasks, acceptance criteria, file-by-file map |
| [`10-testing.md`](./10-testing.md) | Test plan, mock provider, negative controls, E2E scenarios |
| [`11-risks-and-decisions.md`](./11-risks-and-decisions.md) | ADR log, risks, open questions |

---

## The 30-second architecture

```
Browser (dashboard UI: mini-chat + full chat)
   │  fetch  +  EventSource (SSE)
   ▼
dashboard/server.js  (zero-dep, port 4445)   ← existing + new agent routes
   ├─ /api/sites/*                (unchanged)
   ├─ /api/agent/settings         GET/PUT  (baseUrl, model, apiKey)
   ├─ /api/agent/chat             POST → writes JSONL to pi stdin
   ├─ /api/agent/events           GET  ← reads pi stdout → SSE
   └─ /api/agent/{abort,status,sessions,restart}
        │
        ├── child: `pi --mode rpc`   (cwd=/app)
        │      ├─ --skill /app/skills                (the 6 skills)
        │      ├─ --append-system-prompt <generated> (router doctrine)
        │      ├─ extensions/searxng-search.ts       (optional)
        │      └─ tools: read/write/edit/bash/grep/find/ls
        │
        └── child: `node sites/<id>/server.js`       (unchanged)
```

The agent reaches SiteBox through the **existing dashboard HTTP API** at
`http://localhost:4445`, exactly as the six skills already document. No skill needs
to change for Phase 1–3.

---

## Conventions used in these docs

- **MUST / SHOULD / MAY** follow RFC 2119.
- Code blocks marked `text` are illustrative; blocks marked `json` / `js` / `sh`
  are intended to be copied.
- Paths are given **as seen inside the container** (`/app/...`) unless stated
  otherwise; host paths appear in the deploy doc.
- Version-specific pi facts refer to **pi 0.87.1** (the version pinned by ADR-008).
  `pi --help` is authoritative for any installed version.

---

## How to use this folder

1. **Reviewing the design?** Start at [`00-overview.md`](./00-overview.md) and
   [`01-architecture.md`](./01-architecture.md), then read the ADR log.
2. **Implementing?** Read [`09-implementation-plan.md`](./09-implementation-plan.md)
   top to bottom; it links to the detail docs per task.
3. **Debugging later?** [`02-rpc-protocol.md`](./02-rpc-protocol.md) and
   [`10-testing.md`](./10-testing.md) are the references.
4. **Changing a locked decision?** Add a new ADR in
   [`11-risks-and-decisions.md`](./11-risks-and-decisions.md); do not silently edit
   the table above.
