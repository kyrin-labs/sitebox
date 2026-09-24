# 05 — Backend API

All new routes are under `/api/agent/*` and are added to `dashboard/server.js`
(or a router module it delegates to). Existing `/api/sites/*` routes are unchanged.

## 0. Conventions

- JSON in, JSON out (except SSE).
- Errors: `{ "error": "<code>", "message": "<human>", "hint"?: "<next step>" }`
  with an appropriate status code.
- CORS: the agent block reuses the dashboard's existing permissive CORS headers
  (LAN tool). SSE must set the same origin behavior.
- Auth: none (trusted LAN). See [`08-security.md`](./08-security.md#4-network-exposure).
- Every mutating route is idempotent where practical, or documents why not.

## 1. Route summary

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/agent/settings` | Read masked settings + status |
| `PUT` | `/api/agent/settings` | Save settings; restart agent |
| `POST` | `/api/agent/settings/test` | One-shot provider check |
| `GET` | `/api/agent/status` | Process + session status |
| `POST` | `/api/agent/chat` | Send a message (prompt/steer/follow-up) |
| `GET` | `/api/agent/events` | SSE stream of pi records + dashboard events |
| `POST` | `/api/agent/abort` | Abort current turn |
| `POST` | `/api/agent/restart` | Force restart the agent process |
| `GET` | `/api/agent/sessions` | List session files |
| `POST` | `/api/agent/sessions` | `new_session` |
| `POST` | `/api/agent/sessions/open` | `switch_session` |
| `GET` | `/api/agent/messages` | `get_messages` for the active session |
| `GET` | `/api/agent/stats` | `get_session_stats` |
| `POST` | `/api/agent/extension-ui` | Answer an `extension_ui_request` |
| `GET` | `/api/agent/guide` | Serve `agent/guide.md` (markdown) |

## 2. Settings

### 2.1 `GET /api/agent/settings`

Returns the masked settings (see [`03-config-and-models.md`](./03-config-and-models.md#get-apiagentsettings))
plus `configured` (all three required fields present).

### 2.2 `PUT /api/agent/settings`

Request:

```jsonc
{
  "provider": { "baseUrl": "https://llm.local/v1", "api": "openai-completions" },
  "model": { "id": "qwen3-coder", "contextWindow": 131072 },
  "apiKey": "sk-...",           // optional on update; required on first save
  "thinkingLevel": "off",
  "search": { "enabled": true, "url": "http://localhost:8080" },
  "tools": { "allow": ["read","write","edit","bash","grep","find","ls"] }
}
```

Behavior:

1. Validate (returns 400 with `error` + `message`).
2. Merge with stored values; a blank `apiKey` keeps the stored key.
3. Write `agent.json` (0600, atomic).
4. Regenerate `models.json`, `settings.json`, `system-prompt.md`.
5. Restart the agent if it was running or if it is now configured.
6. Return `{ ok, restarted, status }`.

Errors: `base_url_required`, `model_required`, `api_key_required`,
`thinking_level_invalid`, `search_url_invalid`, `tool_unknown`.

### 2.3 `POST /api/agent/settings/test`

Spawns a throwaway `pi --print` (or uses the running agent) with a tiny prompt and a
short timeout. Returns `{ ok, latencyMs, reply, error? }`. Never persists anything.

## 3. Status

### `GET /api/agent/status`

```json
{
  "configured": true,
  "state": "running",              // stopped|starting|running|streaming|crashed|unavailable
  "pid": 1234,
  "model": "qwen3-coder",
  "provider": "sitebox",
  "thinkingLevel": "off",
  "isStreaming": false,
  "isCompacting": false,
  "sessionId": "abc123",
  "sessionName": "SiteBox Chat",
  "sessionFile": "/app/dashboard/data/pi-agent/sessions/....jsonl",
  "messageCount": 12,
  "pendingMessageCount": 0,
  "lastError": null,
  "uptimeMs": 84512,
  "restarts": 1
}
```

`state` is derived, not stored: `streaming` ⊃ `running` ⊃ `starting` ⊃ `stopped`;
`crashed`/`unavailable` are terminal-until-next-action.

## 4. Chat

### 4.1 `POST /api/agent/chat`

Request:

```jsonc
{
  "message": "สร้างเว็บ...",
  "streamingBehavior": "steer" | "followUp",   // optional; required if streaming
  "images": [{ "type": "image", "data": "<base64>", "mimeType": "image/png" }]  // optional
}
```

Behavior:

1. Validate size (`limits.maxMessageBytes`), non-empty.
2. `manager.ensureRunning()`.
3. If idle → `prompt`. If streaming and behavior given → `prompt` with behavior
   (or `steer`/`follow_up` command). If streaming and no behavior → **409**
   `{ error: "agent_busy", hint: "choose steer or followUp" }`.
4. Respond `202 { accepted: true, mode: "prompt"|"steer"|"followUp" }` as soon as pi
   accepts. Progress arrives over SSE.

> The response does **not** wait for the turn. The UI keys off SSE.

Errors: `empty_message`, `message_too_large`, `agent_unavailable`,
`not_configured`, `agent_busy`.

### 4.2 `POST /api/agent/abort`

```jsonc
{ "clearQueue": false }
```

`clearQueue: true` sends `clear_queue` first and returns the cleared text:

```json
{ "ok": true, "cleared": "the text that was queued" }
```

### 4.3 `POST /api/agent/restart`

Kills and respawns. Body `{ "session": "keep" | "new" }` (default `keep`). Returns
the new status.

## 5. SSE stream — `GET /api/agent/events`

### 5.1 Headers

```http
Content-Type: text/event-stream; charset=utf-8
Cache-Control: no-cache, no-transform
Connection: keep-alive
X-Accel-Buffering: no
```

Send a comment `:ok\n\n` immediately to open the stream, then a status snapshot.

### 5.2 Framing

One SSE message per pi record or dashboard event:

```text
id: 42
event: pi
data: {"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"ส"}}

```

- `event: pi` for forwarded pi records (the JSON keeps its own `type`).
- `event: dashboard` for dashboard-generated records.
- `id` is a monotonically increasing per-connection counter (used for
  `Last-Event-ID`, but see replay policy).
- Keep-alive comment every **15 s**: `:ka\n\n`.

### 5.3 Dashboard-generated events

| `type` | Payload | When |
|---|---|---|
| `agent_status` | full status object | on connect, on state change |
| `agent_error` | `{ message, detail? }` | spawn failure, crash, readiness timeout |
| `agent_restart` | `{ reason }` | after a restart |
| `agent_log` | `{ stream, text }` | stderr lines (throttled) |
| `extension_ui_request` | the pi record | forwarded for UI dialogs |

### 5.4 SSE backpressure

Per subscriber, keep a bounded queue:

- **Droppable** (safe to drop when the queue is full): `message_update` with
  `text_delta`/`thinking_delta`/`toolcall_delta`, `tool_execution_update`,
  `bash_execution_update`.
- **Must-not-drop**: `message_start`, `message_end`, `tool_execution_start`,
  `tool_execution_end`, `agent_start`, `agent_end`, `agent_settled`,
  `agent_status`, `agent_error`, `queue_update`, `extension_ui_request`.

When the queue exceeds `N` (e.g. 1000), drop the oldest droppable record and set a
`truncated: true` marker on the next status event so the UI can note it.

### 5.5 Replay policy

No delta replay. On (re)connect:

1. Send `agent_status`.
2. If the client requests it (`GET /api/agent/messages`), send the full transcript.
3. Continue live.

This keeps server memory bounded and avoids reconstructing deltas. The client is
responsible for merging `get_messages` with subsequent live events.

## 6. Sessions

### 6.1 `GET /api/agent/sessions`

Lists `*.jsonl` under `pi-agent/sessions/` (via `fs`, not pi):

```json
{
  "sessions": [
    { "id": "abc123", "file": ".../abc123.jsonl", "name": "SiteBox Chat",
      "mtime": "2026-09-24T12:00:00.000Z", "size": 4096, "active": true }
  ]
}
```

`name` is read from the session header/first entry if cheap; otherwise `null` and
the UI shows the date.

### 6.2 `POST /api/agent/sessions`

Body `{}` (new) or `{ "parentSession": "/path" }`. Sends `new_session`. Returns
`{ ok, cancelled }`.

### 6.3 `POST /api/agent/sessions/open`

Body `{ "sessionPath": "..." }`. Validates the path is inside the sessions dir
(path-traversal guard), then sends `switch_session`. Returns `{ ok, cancelled }`.

### 6.4 `GET /api/agent/messages`

Proxies `get_messages` for the active session. Returns the raw message array plus a
normalized `{ role, text, ts }[]` for the UI (normalization is a convenience; the
raw array is preserved under `raw`).

## 7. Extension UI

### `POST /api/agent/extension-ui`

```jsonc
// confirm
{ "id": "ui-1", "confirmed": true }
// select/input/editor
{ "id": "ui-1", "value": "some text" }
// cancel
{ "id": "ui-1", "cancelled": true }
```

Writes the matching `extension_ui_response` to pi's stdin. Unknown/expired ids are
rejected with `404`. See [`08-security.md`](./08-security.md#permission-gate).

## 8. Guide

`GET /api/agent/guide` returns `{ "markdown": "<contents of agent/guide.md>" }`.
The UI renders it in a modal. If the file is missing, `404` with a hint.

## 9. Status codes

| Code | Meaning |
|---|---|
| 200 | OK |
| 202 | Accepted (chat) |
| 400 | Validation error |
| 404 | Not found (session id, extension-ui id, guide) |
| 409 | `agent_busy` |
| 500 | Internal (config write failed, etc.) |
| 503 | `agent_unavailable` / `not_configured` |

## 10. Module boundaries

```
server.js
  ├─ (existing site routes)
  └─ agentRoutes(req, res, { config, manager })
        ├─ settings  → config.load/save/render
        ├─ status    → manager.status()
        ├─ chat      → manager.sendPrompt/abort
        ├─ events    → sse.subscribe(req, res)   ← manager.on("record")
        ├─ sessions  → manager.newSession/switchSession/listSessions/messages
        └─ extui     → manager.answerExtensionUI(id, response)
```

`server.js` stays a thin dispatcher; all agent logic lives in `dashboard/agent/`.
This keeps the existing file readable and testable.

## 11. Example session (curl)

```bash
# configure
curl -sX PUT localhost:4445/api/agent/settings -H 'Content-Type: application/json' -d '{
  "provider":{"baseUrl":"https://llm.local/v1"},
  "model":{"id":"qwen3-coder"},
  "apiKey":"sk-local-123"
}'

# stream events in one terminal
curl -N localhost:4445/api/agent/events

# send a message in another
curl -sX POST localhost:4445/api/agent/chat -H 'Content-Type: application/json' \
  -d '{"message":"สร้างเว็บทดสอบง่ายๆ หนึ่งหน้า"}'

# inspect
curl -s localhost:4445/api/agent/status | jq
curl -s localhost:4445/api/agent/sessions | jq
```
