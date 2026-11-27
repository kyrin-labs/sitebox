# 11 — Risks, Decisions (ADR), and Open Questions

## Part A — Architecture Decision Records

Format: **Context → Decision → Consequences**. A locked decision is changed only by
adding a superseding ADR, never by editing the original.

---

### ADR-001 — Embed pi as an RPC subprocess (not the SDK)

**Context.** We need a long-lived, streamable chat agent inside a dashboard that
markets itself as "zero dependencies, pure Node.js". Options: in-process SDK,
RPC subprocess, JSON-per-message, or the SDK's `RpcClient`.

**Decision.** Spawn `pi --mode rpc` as a child process and speak JSONL by hand.
Do not import the SDK.

**Consequences.**

- ➕ Dashboard keeps zero runtime dependencies (needs only the `pi` binary).
- ➕ Agent crash/hang cannot take down the dashboard or the sites.
- ➕ Matches the existing `child_process.spawn` pattern for sites; reuses log capture.
- ➕ Process boundary enables future privilege separation (non-root, resource limits).
- ➖ We own JSONL framing and correlation (documented in `02-rpc-protocol.md`).
- ➖ No compile-time types for the protocol.
- ➖ A stray child must be reaped (compose already sets `init: true`).

**Rejected.** In-process SDK (breaks zero-dep; shared failure domain; CJS→ESM).
`RpcClient` (same dependency problem). JSON-per-message (cold start per turn).

---

### ADR-002 — Docker base moves to `node:22-bookworm-slim` + chromium

**Context.** The verify skill requires a real headless browser, and pi's bash tool
requires `bash`. Alpine lacks both cleanly.

**Decision.** Switch the base image to `node:22-bookworm-slim`; install `bash`,
`git`, `ripgrep`, `chromium`, `ca-certificates`, `curl`, `procps`.

**Consequences.**

- ➕ Verify phase works end-to-end in the container.
- ➕ Standard glibc environment (fewer native-module surprises).
- ➖ Image grows by ~250–350 MB (chromium).
- ➖ Slightly slower cold builds.

**Rejected.** Staying on Alpine (browser pain); skipping the browser check (would
violate the router's definition of done).

---

### ADR-003 — Dissolve the router skill into the system prompt

**Context.** The router exists to decide *which* SiteBox skill applies. A dedicated
SiteBox chat is always in SiteBox mode, so the routing question is answered by
context, not by a skill.

**Decision.** Remove the router skill. Move its doctrine (routing table, order,
decisions, anti-patterns, definition of done) into `agent/system-prompt.md`,
injected via `--append-system-prompt`. Keep the historical narratives in
`agent/guide.md` for humans. Keep the six working skills as loadable skills.

**Consequences.**

- ➕ One home for the doctrine (the repo), no repo/home-server duplication.
- ➕ Always in context — the model cannot forget to load it.
- ➕ The six large skills still load on demand (context efficiency preserved).
- ➖ The system prompt grows (~≤12 KB), costing tokens every turn.
- ➖ The home-server router copy must be removed/symlinked (migration task).

**Rejected.** Keeping the router as a skill (redundant, two homes). Folding the
router *and* the six skills into one giant prompt (context blow-up).

---

### ADR-004 — Chat as a mini-chat that expands to a full page

**Context.** The operator wants the agent reachable from the dashboard without
losing the dashboard.

**Decision.** A floating mini-chat (bottom-right) that expands into a full-page
view and returns. Open/closed/expanded and session persist in `localStorage`. One
component, one `EventSource`, two layouts.

**Consequences.**

- ➕ The agent is always one click away; the dashboard is never blocked.
- ➕ A live stream survives view switches.
- ➖ Two layout code paths to keep consistent (mitigated by one renderer).
- ➖ `localStorage` state can desync across tabs (acceptable; single operator).

**Rejected.** A separate `/chat` app (context switch, two shells). A full-page-only
chat (loses the dashboard). A modal (blocks the grid).

---

### ADR-005 — Single-agent loop first; subagents later

**Context.** Subagents add context isolation and parallelism but also complexity
(process fan-out, cost, hand-off formats).

**Decision.** Ship a single agent that loads the six skills on demand. Defer
subagents to Phase 4+.

**Consequences.**

- ➕ Smaller surface to make correct; easier to test and debug.
- ➕ The doctrine's ordering already provides structure without subagents.
- ➖ Long jobs share one context window (compaction handles it).
- ➖ No parallel phase execution in v1.

**Rejected.** Subagents first (premature; harder to verify).

---

### ADR-006 — Settings expose base URL, model, API key (advanced optional)

**Context.** The operator asked for exactly three fields.

**Decision.** The primary form has three fields. Everything else (`api` type,
thinking level, context window, max tokens, vision, search, tool allowlist) lives
under a collapsed "Advanced" section with safe defaults.

**Consequences.**

- ➕ Fast onboarding; the common case is three fields.
- ➕ Power users can still tune without editing files.
- ➖ Defaults may be wrong for exotic endpoints (mitigated by Test connection + the
  `api` type field).

**Rejected.** Only three fields ever (blocks vision/limits). A full pi settings UI
(overwhelming).

---

### ADR-007 — Secrets in `agent.json` (0600), not env or argv

**Context.** The key must reach pi without leaking to the browser, `ps`, or the
bash tool's environment.

**Decision.** Store the key in `dashboard/data/agent.json` (0600, gitignored) and
render it into `pi-agent/models.json` (0600). Never pass it via `--api-key`, never
via an env var the bash tool inherits.

**Consequences.**

- ➕ Narrowest practical channel; not visible in `ps` or `env`.
- ➕ The browser only ever sees `hasApiKey` + a 4-char hint.
- ➖ Plaintext at rest (accepted for a single-operator box).
- ➖ A container exec with read access can read it (accepted).

**Rejected.** `--api-key` (argv). `$SITEBOX_API_KEY` interpolation (env inherited by
bash). A separate secret manager (out of scope).

---

### ADR-008 — Pin the pi version

**Context.** The RPC protocol and event shapes are a contract that can change
between versions.

**Decision.** Pin `@earendil-works/pi-coding-agent` via `ARG PI_VERSION` in the
Dockerfile (currently `0.87.1`). Upgrades re-read the protocol docs and run the
test suite.

**Consequences.**

- ➕ Reproducible builds; no surprise protocol drift.
- ➖ Manual upgrade cadence.
- ➖ A security fix requires a deliberate bump.

**Rejected.** Floating `latest` (non-reproducible; silent breakage).

---

## Part B — Risks (beyond the accepted security residuals)

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Real-model routing is worse than the mock tests | Medium | Medium | Keep the doctrine explicit; assert in E2E; tune the prompt |
| R2 | Long builds exhaust context → heavy compaction | Medium | Low | pi compacts; show compaction notices; consider subagents later |
| R3 | Provider-specific quirks (tool calls, streaming) | Medium | Medium | Test connection; `api` type field; document known endpoints |
| R4 | Chromium verify is slow/flaky in CI-like runs | Medium | Low | Timeouts; the verify skill already tolerates degraded layers |
| R5 | SSE fan-out memory growth under a runaway agent | Low | Medium | Bounded queues + drop policy + subscriber cap |
| R6 | Session files grow unbounded | Low | Low | Manual cleanup; open question for a prune action |
| R7 | Host-networking exposure of a shell-capable agent | Medium | High | Container boundary; recommend reverse-proxy auth; optional token |
| R8 | Prompt injection from scraped data | Medium | Medium | Doctrine hardening + permission gate + live visibility |
| R9 | Port collisions between agent-chosen and existing sites | Low | Low | Skills already mandate `/api/ports/check`; API rejects duplicates |
| R10 | Drift between the doctrine and the six skills | Medium | Medium | `check-skills.mjs` extended to check doctrine headings |

## Part C — Open questions (decide before/at the relevant phase)

| # | Question | Options | Leaning | Phase |
|---|---|---|---|---|
| Q1 | Run the container as non-root? | yes / no | yes if mounts allow | 4 |
| Q2 | Add a shared-secret token on `/api/agent/*`? | none / header token | header token, opt-in | 4 |
| Q3 | Session pruning / "clear history" action? | manual / button / retention | a manual button | 4 |
| Q4 | One agent process or a small pool? | one / pool | one | — |
| Q5 | Expose a raw shell box in the UI? | no / yes | no (the agent runs bash) | — |
| Q6 | Persist `get_messages` to `localStorage`? | no / yes | no (server is the source) | 2 |
| Q7 | Auto-prompt on first session? | no / starter card | starter card only | 2 |
| Q8 | Multi-modal (paste an image)? | no / yes | schema-ready, UI later | 4 |
| Q9 | Cost/usage enforcement? | none / display / cap | display only | 4 |
| Q10 | Rename the "sitebox" provider id? | fixed / configurable | fixed | — |

## Part D — What would make us revisit ADR-001

If any of these become true, reconsider the in-process SDK:

- We need **multiple concurrent agents** with shared in-memory state.
- We need **native custom tools** that call dashboard internals directly (no HTTP).
- The hand-rolled protocol becomes a maintenance burden across pi upgrades.
- The dashboard drops the zero-dependency claim for other reasons.

Until then, RPC is the better fit for SiteBox's constraints and its failure model.
