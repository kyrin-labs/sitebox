# 06 — Frontend: Mini-Chat & Full Chat

The chat has two presentations of **one component**: a floating mini-chat on the
dashboard, and a full-page chat. Switching between them must not interrupt a live
stream.

## 1. UX contract (what the operator gets)

```
Dashboard                                   Full chat
┌─────────────────────────────┐             ┌─────────────────────────────┐
│ SiteBox            [+] [🌙] │             │ ← กลับ    SiteBox Chat  ⋯ ✕ │
│ ┌────┐ ┌────┐ ┌────┐        │             │                             │
│ │site│ │site│ │site│        │   ขยาย ⤢    │  [transcript, full width]   │
│ └────┘ └────┘ └────┘        │ ──────────▶ │                             │
│                             │             │                             │
│                    ┌───────┐│             │                             │
│                    │ mini  ││ ◀────────── │  [input]                    │
│                    │ chat  ││   กลับ      │                             │
│                    │[input]││             │                             │
│                    └───────┘│             │                             │
│                       ✕     │             │                             │
└─────────────────────────────┘             └─────────────────────────────┘
```

Rules:

1. **Mini-chat is closed by default** on first visit. Opening it is one click.
2. **Open/closed persists** (`localStorage`). If open and the operator leaves and
   returns, it is still open with the same transcript and scroll.
3. **Expand** (⤢) turns the mini-chat into the full-page view. **Back** (←) returns
   to the dashboard with the mini-chat still open.
4. **Close** (✕) hides the mini-chat. It does not delete the session.
5. **A live stream is never interrupted by switching views.** One `EventSource`, one
   in-memory message model, two renders.
6. **The agent keeps working when the mini-chat is closed.** Closing is a UI action
   only; pi keeps running. Reopening shows the current state (via `get_messages`).
7. **Status is always visible** while open: a dot (idle / streaming / error) and the
   model name.

## 2. Component model

```
dashboard/public/js/chat.js
  createChat({ root, mode: "mini" | "full" }) → controller
    ├─ store      (messages, status, queue, sessionId)
    ├─ transport  (EventSource + fetch to /api/agent/*)
    ├─ render     (mini | full layout of the same store)
    └─ actions    (send, steer, abort, newSession, openSession, expand, close)
```

- **One `store` and one `transport` per page load.** They are module-level singletons.
- **`render` is a pure function of `store`** plus the current `mode`.
- Switching mode calls `render()` again against the same DOM container (or moves the
  container node). No stream teardown.

### 2.1 Mount points (`index.html`)

```html
<!-- floating widget, sibling of .app -->
<div id="chat-mini" class="chat chat--mini" hidden></div>

<!-- full page, sibling of .app -->
<div id="chat-full" class="chat chat--full" hidden></div>
```

`chat.js` owns both containers and toggles `hidden`. The dashboard's `main.js`
calls `chat.init()` once.

## 3. State machine

### 3.1 Visibility

```
            ┌──────── closed ────────┐
   ✕ close  │                        │ open (click launcher)
   ┌────────┘                        ▼
   │                            ┌── mini ──┐
   └────────────────────────────│          │
                                └────┬─────┘
                         expand ⤢    │    ▲ back ←
                                     ▼    │
                                ┌── full ──┐
                                └──────────┘
```

Persisted: `{ open: bool, mode: "mini"|"full", sessionId: string|null }`.

### 3.2 Run status (drives the send button and dot)

```
idle ──send──▶ streaming ──agent_settled──▶ idle
                   │
                   ├─ abort ──▶ aborting ──▶ idle
                   └─ error ──▶ error ──▶ idle (on next send)
```

While `streaming`, the input offers **Steer** (default) and **Queue** (follow-up);
plain Send is disabled unless the manager says idle. If the server returns 409
`agent_busy`, the UI opens a small chooser.

## 4. Message model (client)

```js
// store.messages: ChatMessage[]
{
  id,                      // stable key
  role: "user" | "assistant" | "system" | "tool",
  blocks: Block[],
  ts,
  stopReason?: "stop" | "error" | "aborted",
  errorMessage?: string,
}

// Block
{ type: "text", text, done }
{ type: "thinking", text, done }
{ type: "tool", toolCallId, toolName, args, status: "running"|"done"|"error",
  output?, isError? }
```

Reconstruction from events:

- `message_start` → new message.
- `message_update.text_delta` → append to the trailing text block.
- `text_end` → set block authoritative text, `done = true`.
- `toolcall_*` → create/update a tool block inside the assistant message.
- `tool_execution_*` → update that block's `status`/`output`.
- `message_end` → replace the message with the authoritative one; set `stopReason`.

On reconnect or first open: fetch `GET /api/agent/messages`, map to the model,
then apply live events.

## 5. Rendering

### 5.1 Layout differences

| Aspect | Mini | Full |
|---|---|---|
| Size | ~360×480 px, bottom-right, resizable later | fills the viewport |
| Header | title, dot, expand ⤢, close ✕ | back ←, title, session menu ⋯, close ✕ |
| Transcript | compact, same renderer | same renderer, wider |
| Tool cards | collapsed by default | collapsed, easier to expand |
| Footer | model + token mini-stat | model, context %, cost, session name |
| Input | one line grows to ~5 | grows to ~12 |

Both use the **same** `renderTranscript()`, `renderToolCard()`, `renderInput()`.

### 5.2 Markdown

Assistant text is markdown. SiteBox has no markdown dependency, so ship a **small,
safe renderer** (`chat-md.js`) that supports:

- fenced code blocks (``` with optional language),
- inline code,
- bold / italic,
- links (rendered as text + href; `target="_blank" rel="noopener noreferrer"`),
- unordered/ordered lists,
- headings (h1–h3).

**Safety:** escape all HTML first, then apply the limited transforms. Never
`innerHTML` raw model output. Links are restricted to `http(s):`. This is both an
XSS guard and a guard against model output containing HTML that the browser would
execute.

### 5.3 Tool cards

One line summary, then expandable detail:

```
▶ ⚙ bash        $ curl -s localhost:4445/api/sites        ✓ 12ms
▶ ✎ write       sites/my-site/public/index.html            ✓
▶ ▶ read        skills/sitebox-design/SKILL.md             ✓
```

Summarizers (per tool):

| Tool | Summary |
|---|---|
| `bash` | `$ <first line of command>` |
| `read` | `<path>:<range?>` |
| `write` | `<path> (+N bytes)` |
| `edit` | `<path>` |
| `grep` | `/<pattern>/ in <path>` |
| `find` | `<pattern> in <path>` |
| `ls` | `<path>` |
| unknown | tool name + first arg |

Detail shows full args (pretty JSON) and truncated output (cap ~10 KB, with a
"show more" that reveals the rest from the event payload).

### 5.4 Status affordances

- **Dot**: grey idle, pulsing blue streaming, red error.
- **Model chip**: `qwen3-coder · 12k ctx`.
- **Queue chips**: when `queue_update` has pending steer/follow-up, show them above
  the input with an ✕ to clear.
- **Compaction/retry notices**: inline system lines ("compacting…", "retrying (2/3)").
- **Extension dialogs**: a modal for `confirm`/`select`/`input`; `notify` → toast.

## 6. Transport

### 6.1 EventSource

```js
const es = new EventSource("/api/agent/events");
es.addEventListener("pi", (e) => store.applyPiEvent(JSON.parse(e.data)));
es.addEventListener("dashboard", (e) => store.applyDashboardEvent(JSON.parse(e.data)));
es.onerror = () => { /* browser auto-reconnects; update dot to "reconnecting" */ };
```

Reconnect: rely on the browser's built-in backoff, but on `open` re-fetch
`/api/agent/messages` to resync (no delta replay, per
[`05-backend-api.md`](./05-backend-api.md#55-replay-policy)).

### 6.2 Actions

| Action | Call |
|---|---|
| send | `POST /api/agent/chat {message}` |
| steer | `POST /api/agent/chat {message, streamingBehavior:"steer"}` |
| queue | `POST /api/agent/chat {message, streamingBehavior:"followUp"}` |
| abort | `POST /api/agent/abort` |
| new session | `POST /api/agent/sessions {}` |
| open session | `POST /api/agent/sessions/open {sessionPath}` |
| list sessions | `GET /api/agent/sessions` |
| load transcript | `GET /api/agent/messages` |
| answer dialog | `POST /api/agent/extension-ui` |

## 7. Persistence (localStorage)

| Key | Value |
|---|---|
| `sitebox-chat-open` | `"1"` / `"0"` |
| `sitebox-chat-mode` | `"mini"` / `"full"` |
| `sitebox-chat-session` | session file path or id |
| `sitebox-chat-scroll` | last scrollTop (per mode) |

Session content is **not** stored in `localStorage`; it lives on the server. The
client rehydrates from `/api/agent/messages`.

## 8. Settings UI

Add an **Agent** section (a new modal or a section in the existing Add/Edit modal
pattern). Primary fields exactly as requested:

```
Agent settings
  Base URL   [ https://api.example.com/v1 ]
  Model      [ some-model-name           ]
  API Key    [ ••••••••••••••  (Replace) ]     ← write-only

  ▸ Advanced (collapsed)
      API type        [ openai-completions ▾ ]
      Thinking level  [ off ▾ ]
      Context window  [          ]  (optional)
      Max tokens      [          ]  (optional)
      Vision input    [ ] text+image
      Enable search   [ ]  SearXNG URL [ http://localhost:8080 ]
      Allowed tools   [x] read [x] write [x] edit [x] bash [x] grep [x] find [x] ls

  [ Test connection ]        [ Save ]
  Status: ● running · qwen3-coder · session abc123
```

Behaviors:

- On open, `GET /api/agent/settings`. If `hasApiKey`, the field shows a masked
  placeholder and a "Replace" affordance; leaving it untouched preserves the key.
- "Test connection" → `POST /api/agent/settings/test`; shows latency or the error.
- "Save" → `PUT`; on success show the new status; if `restarted`, note it.
- If `configured` is false, the mini-chat shows a "Set up the agent" CTA that opens
  this modal.

## 9. Accessibility

- Transcript container: `role="log"` `aria-live="polite"` (streaming text announces
  in chunks; avoid `assertive`).
- Input: `<textarea>` with `aria-label="Message"`; `Enter` sends, `Shift+Enter`
  newline (documented in a hint). `Ctrl/Cmd+Enter` also sends.
- Tool cards: `<button aria-expanded>` toggles; content is a `<pre>`.
- Modals: focus trap; `Esc` closes (dialogs answer "cancel").
- Mini-chat launcher: a real `<button>` with `aria-label="Open chat"`.
- Color is never the only status signal (dot has a text label nearby).
- Keyboard shortcuts (full view): `Esc` → back to dashboard; `Ctrl+K` → focus chat
  input (avoid clashing with the dashboard search).

## 10. Responsive behavior

- **< 720 px**: mini-chat becomes near-full-width; "expand" goes straight to full.
- **Full view**: transcript max-width ~860 px centered on wide screens; input same.
- The dashboard grid reflows independently (unchanged).

## 11. Empty & error states

| State | Mini | Full |
|---|---|---|
| Not configured | CTA card → settings | CTA card → settings |
| Configured, no messages | starter prompts | starter prompts + guide link |
| Agent stopped | "Agent stopped" + Start | same + log excerpt |
| Provider error | inline red system line with `errorMessage` | same |
| Reconnecting | dot amber + "reconnecting…" | banner |

## 12. Visual style

Reuse the dashboard's tokens from `css/style.css` (light/dark via `data-theme`).
No new fonts. The chat is a card with the dashboard's radius/border/background
variables; tool cards use the mono font already loaded (JetBrains Mono). This keeps
the chat feeling native to SiteBox rather than bolted on.

## 13. File-level frontend map

| File | Change |
|---|---|
| `public/index.html` | settings fields; `#chat-mini`, `#chat-full`, launcher button; guide modal |
| `public/js/main.js` | wire settings form; `chat.init()` |
| `public/js/chat.js` | new: store, transport, render, actions, mode switching |
| `public/js/chat-md.js` | new: safe minimal markdown renderer |
| `public/css/style.css` | chat + settings + dialog styles, responsive rules |
