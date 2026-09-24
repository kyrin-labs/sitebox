# 02 — RPC Protocol Integration

This is the reference for how the dashboard talks to `pi --mode rpc`. It is
deliberately low-level: the framing rules here are the ones that break silently if
ignored.

> Version basis: **pi 0.87.1**. If the pinned version changes, re-verify against
> `pi --help` and the upstream `docs/rpc.md` / `docs/rpc-commands.md` / `docs/json.md`.

## 1. The process

```bash
pi --mode rpc \
  --provider sitebox \
  --model <model-id> \
  --skill /app/skills \
  --append-system-prompt /app/dashboard/data/pi-agent/system-prompt.md \
  --session-dir /app/dashboard/data/pi-agent/sessions \
  --name "SiteBox Chat" \
  --tools read,write,edit,bash,grep,find,ls
```

Environment (set by `rpc.js`, merged with the dashboard's env):

```text
PI_CODING_AGENT_DIR=/app/dashboard/data/pi-agent
PI_OFFLINE=1                 # no model-catalog / version network chatter
PI_SKIP_VERSION_CHECK=1
SEARXNG_URL=http://localhost:8080   # only if the search extension is enabled
```

| Flag | Why |
|---|---|
| `--mode rpc` | Long-lived bidirectional JSONL |
| `--provider sitebox` | The provider id we write into `models.json` |
| `--model <id>` | Selects the model; required for `--api-key`, and pins the default |
| `--skill /app/skills` | Recursively discovers the six `SKILL.md` dirs |
| `--append-system-prompt <path>` | Adds the doctrine without replacing pi's built-in prompt |
| `--session-dir <path>` | Keeps sessions inside our mounted data dir |
| `--name` | Human label on the session |
| `--tools` | Explicit allowlist; prevents surprises from defaults |

> **Do not** use `--no-session`: persistent sessions are required for continuity
> across restarts and for the session list in the UI.

### 1.1 `cwd`

Spawn with `cwd: "/app"` (the repo root). This makes:

- project resource discovery anchor at the repo,
- relative paths in skills (`skills/sitebox-create/SKILL.md`) resolve,
- the agent's `write`/`edit`/`bash` default to the repo,
- sites land at `/app/sites/<id>` = the mounted host folder.

### 1.2 stdio

| Stream | Use |
|---|---|
| stdin | Commands (one JSON object + `\n`) |
| stdout | Responses + events (one JSON object + `\n`) — **protocol only** |
| stderr | Diagnostics; captured into the agent log ring buffer, never parsed |

## 2. Framing rules (the part that bites)

1. **One JSON object per line, terminated by `\n`.**
2. **Split only on LF (`\n`).** Strip one optional preceding `\r` (CRLF).
3. **Never use `readline`.** Node's `readline` also splits on `U+2028` (LINE
   SEPARATOR) and `U+2029` (PARAGRAPH SEPARATOR), which are legal inside JSON
   strings. A model reply or a file path containing one would be cut in half.
4. **Decode as UTF-8 across chunk boundaries.** A multi-byte character can be split
   between two `data` chunks. Use `StringDecoder("utf8")` (or a `TextDecoder` with
   `{ stream: true }`), not `chunk.toString()`.
5. **Keep reading stdout.** If the reader stops, the OS pipe buffer fills and pi
   stalls. Never `pause()` indefinitely.
6. **Honor stdin backpressure.** Use `write()`'s return value / `drain` when sending
   large commands (e.g. a long prompt with images).

### 2.1 Reference line splitter

```js
// dashboard/agent/rpc.js (excerpt) — the canonical splitter
const { StringDecoder } = require("string_decoder");

function createLineReader(onLine) {
  const decoder = new StringDecoder("utf8");
  let buf = "";
  return (chunk) => {
    buf += decoder.write(chunk);
    let nl;
    while ((nl = buf.indexOf("\n")) !== -1) {
      let line = buf.slice(0, nl);
      buf = buf.slice(nl + 1);
      if (line.endsWith("\r")) line = line.slice(0, -1);
      if (line.length) onLine(line);
    }
    // Guard against a pathological unbounded line (never expected, but cheap).
    if (buf.length > 16 * 1024 * 1024) {
      onLine(buf);
      buf = "";
    }
  };
}
```

### 2.2 Reference line parser

```js
function parseLine(line, { onRecord, onParseError }) {
  let rec;
  try {
    rec = JSON.parse(line);
  } catch (e) {
    // A malformed line must never kill the reader.
    onParseError(line, e);
    return;
  }
  if (rec && typeof rec === "object") onRecord(rec);
}
```

## 3. Command envelope

Every command is one JSON object. `id` is optional but **always send one**; command
handling is asynchronous and responses can arrive out of order.

```json
{"id":"c-7","type":"get_state"}
```

Response:

```json
{"id":"c-7","type":"response","command":"get_state","success":true,"data":{...}}
```

Failure:

```json
{"id":"c-8","type":"response","command":"set_model","success":false,"error":"Model not found: bad/model"}
```

Correlation: match `response.id` to the pending command id. Keep a `Map<id,{resolve,reject,timer}>`.
A response with no `id` (e.g. a parse error) is logged, not correlated.

## 4. Commands the dashboard uses

| Command | Payload | When | Notes |
|---|---|---|---|
| `get_state` | `{}` | readiness probe, status polling | Returns model, `isStreaming`, `sessionId`, `sessionFile`, `messageCount` |
| `prompt` | `{message, images?, streamingBehavior?}` | user sends a message | `streamingBehavior: "steer"` / `"follow_up"` when busy |
| `steer` | `{message, images?}` | explicit steer command | Alternative to `prompt` + behavior |
| `follow_up` | `{message, images?}` | explicit queue | Alternative to `prompt` + behavior |
| `abort` | `{}` | Stop button | Keeps queued messages unless cleared |
| `clear_queue` | `{}` | Stop button (full) | Returns the cleared text so the UI can restore it |
| `new_session` | `{}` or `{parentSession?}` | New chat | |
| `switch_session` | `{sessionPath}` | Resume a session | May be cancelled by an extension |
| `get_messages` | `{}` | Load transcript for the UI | Authoritative message list |
| `get_session_stats` | `{}` | Usage footer | tokens, cost, context % |
| `set_model` | `{provider, modelId}` | (future) model switch | |
| `get_available_models` | `{}` | (future) picker | |
| `set_thinking_level` | `{level}` | (future) advanced setting | |
| `get_commands` | `{}` | discover skill commands | optional |
| `bash` | `{command}` | (only if we add a shell box) | streams `bash_execution_update` |

### 4.1 `prompt` semantics

- A `success: true` response means the prompt was **accepted/queued/handled** — not
  that the run finished. Completion is signalled by `agent_settled`.
- Sending a prompt while streaming **without** `streamingBehavior` is rejected.
  The manager must decide (see [`01-architecture.md`](./01-architecture.md#6-concurrency-and-the-turn-lock)).
- `images` use `{type:"image", data:"<base64>", mimeType:"image/png"}`. v1 may omit
  image input; the schema should still allow it.

### 4.2 `abort` vs `clear_queue`

| Want | Sequence |
|---|---|
| Stop the current turn, keep queued messages | `abort` |
| Stop everything and recover the typed-but-unset text | `clear_queue` then `abort` |

`clear_queue`'s response returns the cleared queue text; the UI can restore it into
the input box (matches the interactive Esc behavior).

## 5. Events the dashboard consumes

All events share the JSON-mode shapes (RPC has no session header). The important
ones, in order of how much the UI depends on them:

### 5.1 Streaming text

```json
{"type":"message_update","usage":{...},"assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hello "}}
```

`message_update` is **delta-only** on the wire. Reconstruct live text by appending
`delta`. Replace the block's content with `text_end.content` when it arrives, and
replace the whole message with `message_end.message`.

Nested `assistantMessageEvent.type` values: `start`, `text_start`, `text_delta`,
`text_end`, `thinking_start`, `thinking_delta`, `thinking_end`, `toolcall_start`,
`toolcall_delta`, `toolcall_end`, `done`, `error`.

| Type | Extra fields | UI action |
|---|---|---|
| `text_delta` | `contentIndex`, `delta` | append to live text |
| `text_end` | `contentIndex`, `content` | set authoritative text |
| `thinking_delta` | `contentIndex`, `delta` | append to collapsed thinking block |
| `toolcall_start` | `contentIndex`, `id`, `toolName` | open a tool card (pending) |
| `toolcall_delta` | `contentIndex`, `delta` | append to card's args preview |
| `toolcall_end` | `contentIndex`, `toolCall` | set card args (parsed) |

### 5.2 Message lifecycle

| Event | Meaning | UI action |
|---|---|---|
| `message_start` | A message began | create bubble if needed |
| `message_end` | Authoritative final message | finalize bubble (role-dependent) |

`message_end.message.stopReason` may be `"error"` or `"aborted"`; surface
`errorMessage` in the chat when present.

### 5.3 Tool execution

| Event | Fields | UI action |
|---|---|---|
| `tool_execution_start` | `toolCallId`, `toolName`, `args` | show running card |
| `tool_execution_update` | `..., partialResult` | update card output |
| `tool_execution_end` | `..., result`, `isError` | finalize card, mark error |

`toolCallId` correlates the three. The card should render a **one-line summary**
(`$ ls -la`, `write sites/x/index.html`, `read skills/…`) and expand for detail.

### 5.4 Run lifecycle

| Event | Meaning | UI action |
|---|---|---|
| `agent_start` | low-level run started | spinner on |
| `agent_end` | run ended (`willRetry`) | keep spinner if `willRetry` |
| `agent_settled` | **no more automatic work** | spinner off, enable send |
| `turn_start` / `turn_end` | one assistant turn | optional step marker |

**The UI's "done" signal is `agent_settled`.** `agent_end` alone is not enough
(retries/compaction/queue can follow).

### 5.5 Queue and misc

| Event | Fields | UI action |
|---|---|---|
| `queue_update` | `steering`, `followUp` | show queued chips |
| `thinking_level_changed` | `level` | update status |
| `session_info_changed` | `name` | update title |
| `compaction_start` / `compaction_end` | reason/result | show "compacting…" notice |
| `auto_retry_start` / `auto_retry_end` | attempt/error | show retry notice |
| `extension_error` | `extensionPath`, `event`, `error` | log + subtle notice |
| `bash_execution_update` | `id`, `delta` | only if we expose a shell box |

### 5.6 Extension UI records (request/response)

Extensions may ask the client for input (`select`, `confirm`, `input`, `editor`) and
send notifications (`notify`, `setStatus`, `setWidget`, `setTitle`,
`set_editor_text`). These are **not** `AgentSessionEvent`s; they are a subprotocol.

For v1 the dashboard MUST handle at least:

- `confirm` → render a small yes/no in the chat (used by permission gates).
- `notify` → toast.
- `select` / `input` / `editor` → a modal prompt (can be deferred; reply "cancel"
  so pi never hangs waiting).

Every `extension_ui_request` carries an `id`; the reply is
`{"type":"extension_ui_response","id":...,"value"|"confirmed"|"cancelled":...}`.

> **Critical:** if an extension request is ignored, pi may block. Always answer or
> cancel. See [`08-security.md`](./08-security.md#permission-gate).

## 6. Lifecycle & recovery

### 6.1 Readiness

After spawn, send `get_state`. The process is **ready** when a `success: true`
response arrives. Timeout: **15 s** (covers model metadata load). On timeout, kill
and mark `unavailable` with the captured stderr.

### 6.2 Turn completion

Track `isStreaming` from `get_state` and events. A turn is complete on
`agent_settled`. Watchdog: if no event of any kind arrives for **180 s** while
streaming (configurable), surface a "still working…" notice (do **not** auto-kill;
long scrapes are legitimate). A hard cap may be offered as a setting later.

### 6.3 Exit handling

```js
child.on("exit", (code, signal) => {
  running.delete();
  const wasStreaming = state.isStreaming;
  state.isStreaming = false;
  emit({ type: "agent_status", status: "stopped", code, signal, wasStreaming });
  // Do NOT auto-respawn here. Respawn lazily on the next prompt,
  // or immediately if a turn was in flight (see below).
  if (wasStreaming) emit({ type: "agent_error", message: "agent exited mid-turn" });
});
```

- **Idle exit** → status `stopped`; next message triggers `ensureRunning()`.
- **Mid-turn exit** → emit `agent_error`; next message respawns and reopens the last
  session file (`switch_session`), so the transcript is intact.
- **Repeated crashes** → exponential backoff (1s→30s) before the next spawn; a
  manual "Restart" always bypasses the backoff.

### 6.4 Restart on settings change

```
abort (if streaming) → SIGTERM → wait 3 s → SIGKILL → respawn with new config
```

If `sessionFile` is known, pass `--session <path>`? **No** — we use
`--session-dir` + `switch_session` after ready, to keep session discovery uniform.
(Alternative documented in the ADR log.)

## 7. Error taxonomy

| Class | Source | Handling |
|---|---|---|
| Spawn failure | `child.on("error")` (ENOENT) | status `unavailable`; UI hint "pi not installed" |
| Protocol parse error | malformed line | log + skip; never fatal |
| Command rejection | `response.success:false` | reject the pending promise; surface `error` |
| Provider auth error | `message_end.stopReason:"error"` | show `errorMessage` in chat |
| Provider rate limit | `auto_retry_*` then error | show retry notices; final error in chat |
| Context overflow | `compaction_*` then continue/retry | show "compacting…"; pi recovers |
| Agent crash | `exit` | `agent_error` + respawn policy |
| Readiness timeout | no `get_state` response | kill + `unavailable` |

## 8. Test hooks (protocol-level)

`rpc.js` SHOULD accept injected `spawn` and clock so tests can drive it with a fake
child:

```js
createRpcClient({ spawn: fakeSpawn, now: fakeNow, readyTimeoutMs: 50 })
```

See [`10-testing.md`](./10-testing.md#1-rpc-layer-unit-tests) for the fixtures,
including the `U+2028` splitter case and out-of-order response correlation.

## 9. Quick reference card

```text
SPAWN   pi --mode rpc --provider sitebox --model M --skill /app/skills
           --append-system-prompt <gen> --session-dir <dir> --name "SiteBox Chat"
           --tools read,write,edit,bash,grep,find,ls
CWD     /app
ENV     PI_CODING_AGENT_DIR, PI_OFFLINE=1, PI_SKIP_VERSION_CHECK=1
READ    stdout: split on \n only, UTF-8 stream decoder, never readline
WRITE   stdin: one JSON + \n; send an id; await matching response
READY   get_state success (15 s)
DONE    agent_settled
STOP    abort  (or clear_queue then abort)
RESET   new_session | switch_session {sessionPath}
STATE   get_state | get_messages | get_session_stats
```
