# 03 — Configuration, Models, and Secrets

This doc defines every file and value the operator can influence, and how the
dashboard turns three UI fields into a working pi provider.

## 1. Files at a glance

| File | Owner | Git | Mode | Contents |
|---|---|---|---|---|
| `dashboard/data/agent.json` | dashboard | **ignored** | `0600` | Operator settings + API key |
| `dashboard/data/pi-agent/models.json` | dashboard (generated) | ignored | `0600` | pi provider + models (contains key) |
| `dashboard/data/pi-agent/settings.json` | dashboard (generated) | ignored | `0644` | Resource paths (skills/extensions) |
| `dashboard/data/pi-agent/system-prompt.md` | dashboard (generated) | ignored | `0644` | Rendered doctrine |
| `dashboard/data/pi-agent/sessions/*.jsonl` | pi | ignored | `0600` | Conversations |
| `agent/system-prompt.md` | repo | tracked | `0644` | Doctrine template (source) |
| `agent/guide.md` | repo | tracked | `0644` | UI guide (source) |

**Rule:** everything under `dashboard/data/pi-agent/` and `agent.json` is runtime
state that may contain secrets or private conversations. It is gitignored and
blocked by the pre-commit hook (see [`07-docker-and-deploy.md`](./07-docker-and-deploy.md)).

## 2. `agent.json` — the operator's settings

### 2.1 Schema (v1)

```jsonc
{
  "version": 1,
  "provider": {
    "id": "sitebox",                     // fixed id; the pi provider name
    "baseUrl": "https://api.example.com/v1",
    "api": "openai-completions"          // fixed for v1
  },
  "model": {
    "id": "some-model-name",             // the only required model field
    "name": null,                        // optional display name
    "contextWindow": null,               // optional; omit → pi default
    "maxTokens": null,                   // optional
    "reasoning": false,                  // optional
    "input": ["text"]                    // optional; add "image" for vision
  },
  "apiKey": "sk-...",                    // secret, write-only from the UI
  "thinkingLevel": "off",                // off|minimal|low|medium|high|xhigh|max
  "tools": {
    "allow": ["read","write","edit","bash","grep","find","ls"]
  },
  "search": {
    "enabled": false,
    "url": "http://localhost:8080"       // SearXNG
  },
  "limits": {
    "readyTimeoutMs": 15000,
    "idleNoticeMs": 180000,
    "maxMessageBytes": 262144
  },
  "updatedAt": "2026-09-24T12:00:00.000Z"
}
```

### 2.2 Defaults

On first run (no file), the dashboard uses:

```json
{
  "version": 1,
  "provider": { "id": "sitebox", "baseUrl": "", "api": "openai-completions" },
  "model": { "id": "", "input": ["text"] },
  "apiKey": "",
  "thinkingLevel": "off",
  "tools": { "allow": ["read","write","edit","bash","grep","find","ls"] },
  "search": { "enabled": false, "url": "http://localhost:8080" },
  "limits": { "readyTimeoutMs": 15000, "idleNoticeMs": 180000, "maxMessageBytes": 262144 }
}
```

The agent is **not started** until `provider.baseUrl`, `model.id`, and `apiKey` are
all non-empty. Validation returns which field is missing.

### 2.3 Validation rules

| Field | Rule | Error |
|---|---|---|
| `provider.baseUrl` | non-empty; `http(s)://`; no trailing requirement | `base_url_required` / `base_url_invalid` |
| `model.id` | non-empty; no whitespace-only | `model_required` |
| `apiKey` | non-empty on save; may be omitted to keep the existing one | `api_key_required` |
| `thinkingLevel` | one of the enum | `thinking_level_invalid` |
| `search.url` | `http(s)://` when `search.enabled` | `search_url_invalid` |
| `tools.allow` | subset of the known tool names; `bash` warn | `tool_unknown` |
| `limits.*` | integers within sane bounds | `limit_invalid` |

**API key update semantics:** the settings form sends `apiKey` only when the
operator types a new one. If the field is left blank on an update, the server keeps
the stored key. This avoids echoing the secret back into the DOM.

### 2.4 Reading and writing safely

```js
// write: atomic + restrictive mode
function writeSecret(file, obj) {
  const tmp = file + ".tmp";
  fs.writeFileSync(tmp, JSON.stringify(obj, null, 2) + "\n", { mode: 0o600 });
  fs.renameSync(tmp, file);
  fs.chmodSync(file, 0o600); // enforce even if umask widened it
}
```

Reading a corrupt `agent.json` MUST NOT crash the dashboard. Mirror the
`sites.json` behavior: keep the last parse error, serve defaults, refuse to
overwrite until fixed.

## 3. Generated `models.json`

pi reads `models.json` from the agent dir (`PI_CODING_AGENT_DIR`). The dashboard
regenerates it whenever settings change.

### 3.1 Minimal (the three-field case)

```json
{
  "providers": {
    "sitebox": {
      "baseUrl": "https://api.example.com/v1",
      "api": "openai-completions",
      "apiKey": "sk-...",
      "models": [
        { "id": "some-model-name" }
      ]
    }
  }
}
```

### 3.2 With optional metadata

When the operator fills advanced fields, they are merged onto the model entry:

```json
{
  "providers": {
    "sitebox": {
      "baseUrl": "https://api.example.com/v1",
      "api": "openai-completions",
      "apiKey": "sk-...",
      "models": [
        {
          "id": "some-model-name",
          "name": "Some Model",
          "input": ["text", "image"],
          "contextWindow": 131072,
          "maxTokens": 8192,
          "reasoning": true
        }
      ]
    }
  }
}
```

### 3.3 Why `openai-completions`

Most OpenAI-compatible servers (Ollama, LM Studio, vLLM, SGLang, proxies) speak
Chat Completions. It is the safest default for "base URL + model + key". If a
server only speaks the Responses API, the operator can change `api` to
`openai-responses` via an advanced field (documented but not exposed in the
primary form).

> Do **not** enable compatibility flags blindly. Only add them when a real endpoint
> rejects requests; record the change in the ADR log.

### 3.4 Key placement

The key lives in `models.json` (mode `0600`), **not** in argv. Rationale:

- argv is visible to other processes via `ps` inside the container.
- env vars are inherited by the bash tool and any child process the agent spawns —
  a scraped-data prompt injection could read them.
- A file read only by the pi process at startup is the narrowest practical channel.

Alternative considered: `--api-key` (rejected: argv exposure) and env interpolation
`$SITEBOX_API_KEY` (rejected: inherited by bash). See ADR-007.

## 4. Generated `settings.json` (pi)

Regenerated to declare resources without relying on discovery surprises:

```json
{
  "skills": ["/app/skills"],
  "extensions": [],
  "enableSkillCommands": true
}
```

When the search extension is enabled, `extensions` becomes:

```json
["/app/agent/extensions/searxng-search.ts"]
```

> `skills`/`extensions` are **absolute paths** in the agent-dir settings, which pi
> resolves as-is. Keeping them explicit means the agent behaves identically no
> matter what happens to be in `cwd`.

## 5. Generated `system-prompt.md`

`config.renderSystemPrompt()` reads `agent/system-prompt.md` and substitutes a
small set of placeholders so the doctrine always reflects the real deployment:

| Placeholder | Value |
|---|---|
| `{{REPO_ROOT}}` | `/app` |
| `{{DASHBOARD_URL}}` | `http://localhost:4445` |
| `{{SITES_DIR}}` | `/app/sites` |
| `{{SKILLS_DIR}}` | `/app/skills` |
| `{{SKILL_LIST}}` | bullet list of the six skill names |
| `{{PORT_RANGES}}` | the range table from the README |
| `{{DATE}}` | build date (informational) |

Rendering is **fail-loud**: an unknown `{{TOKEN}}` left in the output is an error at
render time (not silently passed to the model). See
[`04-prompt-and-skills.md`](./04-prompt-and-skills.md).

## 6. Settings HTTP surface

Only the shapes are here; full route docs are in
[`05-backend-api.md`](./05-backend-api.md).

### `GET /api/agent/settings`

```json
{
  "version": 1,
  "provider": { "id": "sitebox", "baseUrl": "https://api.example.com/v1", "api": "openai-completions" },
  "model": { "id": "some-model-name", "input": ["text"] },
  "hasApiKey": true,
  "apiKeyHint": "sk-…abcd",
  "thinkingLevel": "off",
  "tools": { "allow": ["read","write","edit","bash","grep","find","ls"] },
  "search": { "enabled": false, "url": "http://localhost:8080" },
  "limits": { "readyTimeoutMs": 15000, "idleNoticeMs": 180000, "maxMessageBytes": 262144 },
  "configured": true
}
```

`apiKey` is **never** returned. `apiKeyHint` is the last four characters only.

### `PUT /api/agent/settings`

Request may include `apiKey` (write-only). Response is the same shape as GET plus:

```json
{ "ok": true, "restarted": true, "status": { "state": "running", "model": "some-model-name" } }
```

### `POST /api/agent/settings/test`

Optional but recommended: after saving, verify the endpoint by sending a tiny
prompt (`"reply with the single word: ok"`) with a short timeout and report the
result. This turns "wrong base URL/key" from a chat-time surprise into a
save-time error.

```json
{ "ok": true, "latencyMs": 842, "reply": "ok" }
```

## 7. Environment variables (container)

| Variable | Value | Why |
|---|---|---|
| `PI_CODING_AGENT_DIR` | `/app/dashboard/data/pi-agent` | Keep pi state in the mounted data dir |
| `PI_OFFLINE` | `1` | No catalog/version network calls |
| `PI_SKIP_VERSION_CHECK` | `1` | No update nag |
| `SEARXNG_URL` | `http://localhost:8080` | Search extension target |
| `SITEBOX_DASHBOARD_URL` | `http://localhost:4445` | Used when rendering the prompt |

These are set in `docker-compose.yml` (and defaulted in `rpc.js` for bare-metal
runs). Provider credentials are **not** env vars.

## 8. Migration & versioning

- `agent.json` carries `version: 1`. On read, an older/unknown version is migrated
  by a small `migrate()` that fills defaults and bumps the version. Never discard
  unknown keys blindly — keep them under a `_legacy` bag until the next save.
- `models.json` / `settings.json` / `system-prompt.md` are **derived artifacts**;
  they can always be deleted and regenerated. If a generation bug appears, delete
  and restart rather than hand-editing.

## 9. Worked example: three fields → running provider

Input (UI):

```
Base URL:  https://llm.local/v1
Model:     qwen3-coder
API Key:   sk-local-123
```

Outputs:

`agent.json` (excerpt)
```json
{ "provider": { "id": "sitebox", "baseUrl": "https://llm.local/v1", "api": "openai-completions" },
  "model": { "id": "qwen3-coder", "input": ["text"] },
  "apiKey": "sk-local-123" }
```

`models.json`
```json
{ "providers": { "sitebox": {
    "baseUrl": "https://llm.local/v1",
    "api": "openai-completions",
    "apiKey": "sk-local-123",
    "models": [{ "id": "qwen3-coder" }]
}}}
```

Launch
```bash
pi --mode rpc --provider sitebox --model qwen3-coder ...
```

The browser only ever saw `hasApiKey: true, apiKeyHint: "…k-123"`.
