# 10 — Testing

A check that has never failed is not a check (this is the same doctrine
`sitebox-verify` applies to sites). Every guard here ships with a **negative
control** that proves it can fail.

## 0. Test layers

| Layer | What | Tooling |
|---|---|---|
| L1 | Pure functions: splitter, parser, config rendering, redaction, markdown | `node --test` (built-in) |
| L2 | RPC client against a **fake child** | `node --test` + injected spawn |
| L3 | HTTP routes against a running dashboard with a **mock provider** | `node --test` + `fetch` |
| L4 | End-to-end: chat → site built → running | scripted against the container |
| L5 | Manual UX checks (mini/full, a11y) | human |

No new test dependencies: use Node's built-in `node:test` and `node:assert`. This
preserves the zero-dependency spirit for the dashboard; tests may live under
`dashboard/agent/__tests__/`.

## 1. RPC layer unit tests (L1/L2)

### 1.1 Line splitter

| Case | Input | Expected |
|---|---|---|
| basic | `a\nb\n` | two lines `a`, `b` |
| CRLF | `a\r\nb\r\n` | `a`, `b` (CR stripped) |
| split chunk | `{"a":` + `1}\n` | one line `{"a":1}` |
| **U+2028 in string** | `{"s":"x\u2028y"}\n` | **one** line (readline would split) |
| multi-byte split | `"é"` bytes split across chunks | correct UTF-8 |
| no trailing newline | `{"a":1}` | buffered, not emitted until `\n` |

**Negative control:** run the same U+2028 case through `readline` and assert it
**fails** (proves the test detects the bug class).

### 1.2 Command correlation

- Send `get_state` with `id: c1`; fake replies out of order (`c2` then `c1`);
  assert each promise resolves with its own data.
- Fake replies `success:false`; assert the promise rejects with `error`.
- No reply; assert timeout rejects.

### 1.3 Parse errors are non-fatal

- Feed `not json\n{"type":"agent_start"}\n`; assert one parse error recorded and the
  valid event still emitted; the reader continues.

### 1.4 Exit handling

- Fake `exit` while idle → status `stopped`, no auto-respawn.
- Fake `exit` while streaming → `agent_error` emitted, `isStreaming=false`.
- Repeated spawn failures → backoff sequence `1000,2000,4000,…` (fake clock).

## 2. Config tests (L1)

| Case | Assert |
|---|---|
| defaults when file missing | agent not `configured` |
| write is atomic + 0600 | mode bits equal `0o600` |
| corrupt JSON | dashboard serves defaults, refuses to overwrite, surfaces `configError` |
| key preservation | `PUT` without `apiKey` keeps the old key |
| **redaction** | serialized `GET` payload contains no substring of the key |
| models.json render | provider/baseUrl/api/models match settings |
| optional metadata merge | `contextWindow`/`maxTokens`/`reasoning`/`input` appear when set |
| system prompt render | no `{{TOKEN}}` remains; paths correct; skill list complete |
| unknown placeholder | render throws (fail-loud) |

**Negative control (redaction):** temporarily inject the key into the response
builder and assert the redaction test **fails**. Restore.

## 3. Doctrine & prompt tests (L1/L3)

| Case | Method |
|---|---|
| all six skills present | scan `skills/` dirs |
| no router | assert `skills/sitebox/` absent |
| required headings in doctrine | string match (`Route the request`, `Anti-patterns`, `Definition of done`, …) |
| placeholder renderers exist | parse `{{...}}` tokens, assert each is in the renderer map |
| **routing** | send the four starter prompts against the mock provider; assert the transcript reads the expected `skills/sitebox-*/SKILL.md` |
| **order** | for "create with real data", assert `sitebox-data` is read before any `write` to `sites/` |
| done report | final assistant text names the gates or says which could not run |

The mock provider (§4) can be scripted to emit deterministic tool calls so routing
can be asserted without a real model.

## 4. Mock provider (the key enabler)

A tiny OpenAI-compatible server that returns scripted SSE responses, so L3/L4 run
offline and deterministically.

```
dashboard/agent/__tests__/mock-provider.js
  POST /v1/chat/completions
    - reads a scenario name from the request (system or first user message)
    - streams text deltas and tool_calls per the scenario
```

Scenarios:

| Scenario | Emits |
|---|---|
| `echo` | plain text reply |
| `tool-bash` | a `bash` tool call, then a final text |
| `build-site` | read skill → write files → bash curl register/start → text |
| `error-auth` | a 401 response |
| `slow` | deltas with delays (tests streaming + abort) |
| `big` | large output (tests truncation + SSE backpressure) |

Point `agent.json` at `http://127.0.0.1:<port>/v1` with a dummy key for tests.

## 5. HTTP route tests (L3)

| Route | Cases |
|---|---|
| `GET /settings` | no key; `hasApiKey` correctness; `configured` |
| `PUT /settings` | validation errors; key preservation; restart triggered |
| `POST /settings/test` | ok; auth error surfaced |
| `POST /chat` | empty→400; too large→400; not configured→503; busy without behavior→409; steer→202 |
| `GET /events` | status snapshot first; receives pi records; keep-alive; subscriber removal on close |
| `POST /abort` | clears queue returns text |
| `GET /sessions` | lists files; active flag |
| `POST /sessions/open` | path traversal `../../etc/passwd` → 400 |
| `POST /extension-ui` | unknown id → 404; confirm → writes correct record |

**Negative controls:**

- Path traversal: assert a legitimate path **succeeds** and the traversal **fails**.
- Busy: assert idle prompt **succeeds** and busy-without-behavior **fails**.

## 6. SSE backpressure tests (L2)

- Flood 5000 droppable deltas to a slow subscriber; assert:
  - droppable records dropped,
  - `message_end`/`agent_settled` **still delivered**,
  - memory bounded (queue length ≤ cap).
- **Negative control:** make `agent_settled` droppable in a temporary build and
  assert the test fails.

## 7. End-to-end (L4)

Run inside the container (or bare metal with `pi` on PATH):

```
E2E-1  configure via PUT with the mock provider
E2E-2  open SSE
E2E-3  POST chat "create a simple one-page site"
E2E-4  wait for agent_settled
E2E-5  assert sites.json contains the new id
E2E-6  assert GET /api/sites/<id>/health → online
E2E-7  assert the site returns 200 on / and on one asset
E2E-8  kill -9 the pi child; assert /api/sites still 200; send another message; assert respawn
```

Assertions are artifacts (JSON, HTTP status), not eyeballing — same standard as
`sitebox-verify`.

## 8. Frontend tests (L1 + L5)

L1 (pure, no browser):

- `chat-md.js`: escapes `<script>`; renders fenced code; restricts link schemes.
- store reducers: apply a sequence of pi events → expected message model.
- reconnect merge: `get_messages` + live events → no duplicates.

L5 (manual checklist):

```
[ ] mini-chat opens/closes; state persists across reload
[ ] expand → full → back keeps transcript and scroll
[ ] switching mid-stream loses no text (compare to /api/agent/messages)
[ ] abort stops the turn
[ ] steer during a run is delivered
[ ] tool cards expand and show output
[ ] confirm dialog (permission gate) can be answered and cancelled
[ ] dark and light themes both legible
[ ] keyboard: Enter sends, Shift+Enter newline, Esc returns from full view
[ ] screen reader announces streaming text (aria-live)
```

## 9. Security tests

| Test | Assert |
|---|---|
| settings redaction | no key substring in any response (also `settings/test`) |
| file mode | `agent.json` and `models.json` are `0600` |
| git guard | `git add -f agent.json` + commit is blocked by the hook |
| key not in argv | capture the spawn argv; assert it contains no key |
| key not in env | assert `child.env` has no key variable |
| XSS | model text with HTML/onerror renders inert |
| traversal | sessions/open rejects `..` |

## 10. Regression gates (run before merge)

```sh
node --test dashboard/agent/__tests__/          # L1/L2
node --test dashboard/agent/__tests__/http/     # L3 (mock provider)
node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
node dashboard/agent/__tests__/e2e.js           # L4 (container)
```

## 11. What each gate proves (and cannot)

| Gate | Proves | Cannot prove |
|---|---|---|
| L1/L2 | framing, correlation, config correctness | real model behavior |
| L3 | route contracts, SSE semantics | UX |
| L4 | a site can be built and served | design quality |
| L5 | the operator's experience | model quality |
| Routing tests | the doctrine routes correctly *under the mock* | real-model obedience |

State this limitation in the final report — the same way `sitebox-verify` says
which layer could not run.
