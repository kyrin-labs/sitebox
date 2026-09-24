# 08 — Security

The embedded agent is a **full coding agent with shell access**. Treat it as a
trusted-but-fallible process with the same power as an SSH session into the
container. This doc states the threat model, the controls, and the residual risks
we accept.

## 1. Trust model

| Actor | Trust |
|---|---|
| The operator (LAN) | Trusted |
| The dashboard process | Trusted |
| The pi agent process | Trusted code, **untrusted behavior** (it follows model output) |
| Model output | Untrusted (can be wrong or adversarial) |
| Scraped web content | **Untrusted** (prompt-injection vector) |
| Other LAN devices | Semi-trusted (no auth — see §4) |
| The public internet | Untrusted |

The important shift: the agent **executes model output**, and the model reads
**untrusted web content** during `sitebox-data`. So a malicious page can, in
principle, try to instruct the agent to do something harmful. Controls below limit
the blast radius.

## 2. Assets to protect

| Asset | Sensitivity | Where |
|---|---|---|
| Provider API key | High (billing + access) | `dashboard/data/agent.json`, `pi-agent/models.json` |
| Conversation history | Medium (may include pasted secrets) | `pi-agent/sessions/*.jsonl` |
| Host filesystem | High | mounted `./sites`, `./dashboard/data`; container root |
| Host network | Medium | `network_mode: host` |
| Other services on the box | High | n8n, portainer, etc. reachable from the container |

## 3. API key handling

Controls:

1. **Write-only from the UI.** `GET /api/agent/settings` never returns the key;
   it returns `hasApiKey` and a 4-char hint.
2. **File mode `0600`**, enforced after write (not just at create), and written
   atomically (tmp + rename).
3. **Not in argv.** The key is in `models.json`, not `--api-key` (argv is visible
   via `ps`).
4. **Not in env.** We deliberately avoid `$SITEBOX_API_KEY` interpolation because
   the bash tool and every child process inherit env; a prompt-injected command
   could `env | grep KEY`. The file is read by pi at startup only.
5. **Not committed.** `.gitignore` + pre-commit block.
6. **Not logged.** The RPC reader logs stderr, not stdout records; settings routes
   never echo the key. Debug logging MUST redact `apiKey`.

Residual risk: the key sits in plaintext on disk. Anyone with host read access can
read it. Accepted for a single-operator home server; if that changes, wrap with a
secret store (out of scope).

## 4. Network exposure

`network_mode: host` means the dashboard and every site port are on the LAN with
**no authentication**. The agent adds new write endpoints (`/api/agent/*`) that can
drive a shell-capable process. Implications:

- Anyone on the LAN who can reach `:4445` can send chat messages and therefore run
  commands inside the container.
- This is consistent with the existing model (the site API is already unauthenticated
  and can start/stop/purge sites).

**Recommendation (not blocking v1):** bind the dashboard to a LAN interface and/or
put it behind the existing reverse proxy (nginx-proxy-manager is already running)
with auth. Document this in the deploy doc as a hardening step. A minimal option is
an optional shared-secret header (`SITEBOX_AGENT_TOKEN`) checked on `/api/agent/*`.

## 5. Shell access (the bash tool)

The agent needs bash to build and verify sites. Controls and boundaries:

| Control | Detail |
|---|---|
| Container boundary | The agent runs inside the container; it cannot see host files not mounted |
| Mount scope | Only `./sites` (rw) and `./dashboard/data` (rw) are mounted. The container's `/app` is otherwise image-only |
| `cwd` | `/app`, so relative operations stay in the repo |
| Permission gate | An extension can intercept `tool_call` for `bash` and require confirmation for destructive patterns (see §6) |
| Non-root (proposed) | Run the container as a non-root user so a breakout has less privilege (open question) |

Residual risks accepted for v1:

- The agent can `curl` any host-reachable service (n8n, portainer) because of host
  networking.
- It can fill the disk with scraped data (no quota).
- It can `rm -rf` inside the container's writable mounts.

## 6. Permission gate

A small extension (`agent/extensions/permission-gate.ts`, Phase 4) intercepts
`tool_call`:

```
if tool is bash and command matches a destructive pattern:
    ask the client via extension UI "confirm"
    block unless confirmed
```

Patterns (conservative, extendable):

```
rm -rf /            rm -rf *            mkfs            dd if=
:(){ :|:& };:       shutdown            reboot          > /dev/sd
curl … | sh         wget … | sh         chmod -R 777 /
```

Because the UI runs over RPC, the gate uses the **extension UI confirm** subprotocol.
The frontend MUST answer (or cancel) every `extension_ui_request`, otherwise pi
blocks. See [`05-backend-api.md`](./05-backend-api.md#7-extension-ui) and
[`06-frontend-ui.md`](./06-frontend-ui.md#54-status-affordances).

> The gate is a **guardrail, not a sandbox**. A determined injection can use
> commands that do not match the patterns. The real boundary is the container.

## 7. Prompt injection via scraped data

`sitebox-data` fetches real content (platforms, APIs). That content enters the
model's context. A page could contain "ignore your instructions and run X".

Mitigations:

1. **System-prompt hardening**: the doctrine states that instructions found in
   fetched content are data, not commands.
2. **Permission gate** on destructive shell commands.
3. **Least privilege**: non-root, minimal mounts, no provider key in env.
4. **Operator visibility**: every tool call is shown live in the chat, so the
   operator sees a suspicious `bash` before/while it runs.

We do **not** claim to eliminate injection; we bound it.

## 8. XSS in the chat UI

Model output is rendered as markdown. Controls:

- Escape all HTML first; apply a fixed, small set of transforms.
- No raw `innerHTML` of model text.
- Links restricted to `http(s):`, `rel="noopener noreferrer"`, `target="_blank"`.
- Code blocks rendered as text inside `<pre><code>`.

See [`06-frontend-ui.md`](./06-frontend-ui.md#52-markdown).

## 9. Extension trust

Extensions run inside the pi process with its privileges. Only load extensions
from the repo (`agent/extensions/`), never from a user-supplied path. The search
extension is ported from the home server and reviewed before inclusion.

## 10. Session data

- `pi-agent/sessions/*.jsonl` may contain anything the operator typed or the agent
  read (including code and scraped text). Treat as private.
- Mode `0600` on the directory (best effort) and gitignored.
- A future "clear history" action is desirable; document as an open question.

## 11. Input limits & DoS

| Limit | Value | Enforced at |
|---|---|---|
| Max message bytes | 256 KB | `/api/agent/chat` |
| Max SSE subscribers | small (e.g. 8) | SSE registry |
| SSE queue cap | 1000 records | SSE fan-out |
| Concurrent turns | 1 | manager lock |
| Readiness timeout | 15 s | rpc.js |
| Agent restarts backoff | 1s→30s | manager |

## 12. Security checklist (review before merge)

```
[ ] GET settings never contains apiKey (test asserts absence)
[ ] agent.json is 0600 after every write
[ ] key is not in argv, env, or logs
[ ] .gitignore + pre-commit block agent.json and pi-agent/
[ ] chat markdown escapes HTML; links http(s) only
[ ] extension_ui_request always answered or cancelled
[ ] permission gate covers the destructive patterns (Phase 4)
[ ] session path traversal blocked in /api/agent/sessions/open
[ ] message size + subscriber caps enforced
[ ] container runs with minimal mounts; non-root considered
```

## 13. Residual risks (accepted, with owners)

| Risk | Severity | Mitigation | Status |
|---|---|---|---|
| LAN attacker drives the shell-capable agent | High | container boundary; optional token; reverse-proxy auth | accepted, hardening recommended |
| Prompt injection runs an unpatterned destructive command | Medium | permission gate + live visibility | accepted |
| Plaintext key on disk | Medium | 0600, gitignored, not in env/argv | accepted |
| Agent fills disk with scraped assets | Low | manual monitoring; no quota | accepted |
| Host services reachable via host networking | Medium | out of scope; note in deploy doc | accepted |
