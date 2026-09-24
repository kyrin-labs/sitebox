# 07 — Docker, Deploy, and Repo Hygiene

## 1. Why the base image changes

`node:22-alpine` cannot run the agent well:

| Need | Alpine | Debian slim |
|---|---|---|
| `bash` (pi's bash tool) | not installed by default | `bash` |
| `ripgrep` (grep tool speed) | separate pkg | `ripgrep` |
| Chromium (verify skill) | awkward, extra libs | `chromium` |
| pi's `jiti`/TS loading | fine | fine |

**Decision (ADR-002):** switch to `node:22-bookworm-slim`.

## 2. Dockerfile (target)

```dockerfile
FROM node:22-bookworm-slim

# System tools the agent needs:
#  - bash       : pi's bash tool
#  - git        : the agent may inspect/clone
#  - ripgrep    : fast grep/find
#  - chromium   : headless browser for sitebox-verify
#  - ca-certs   : TLS to the model endpoint and scraped sources
#  - curl       : the skills' documented API calls
#  - procps     : ps/kill for the agent's own debugging
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      bash git ripgrep chromium ca-certificates curl procps \
 && rm -rf /var/lib/apt/lists/*

# Pin the agent (ADR-008). Bump deliberately, then re-verify docs/native-agent/02.
ARG PI_VERSION=0.87.1
RUN npm install -g --ignore-scripts @earendil-works/pi-coding-agent@${PI_VERSION}

WORKDIR /app

# Code-only image; runtime data is mounted (see compose).
COPY dashboard/ ./dashboard/
COPY sites/ ./sites/
COPY skills/ ./skills/
COPY agent/ ./agent/

ENV PI_CODING_AGENT_DIR=/app/dashboard/data/pi-agent \
    PI_OFFLINE=1 \
    PI_SKIP_VERSION_CHECK=1 \
    SITEBOX_DASHBOARD_URL=http://localhost:4445 \
    SEARXNG_URL=http://localhost:8080 \
    CHROME=/usr/bin/chromium

EXPOSE 4445
CMD ["node", "dashboard/server.js"]
```

Notes:

- `COPY skills/` and `COPY agent/` are **new** — previously only `dashboard/` and
  `sites/` were copied. The agent needs both.
- `--ignore-scripts` avoids surprise postinstall network access.
- Chromium on Debian slim works headless with `--no-sandbox` inside a container
  (already the flag used by `sitebox-verify`).

## 3. docker-compose.yml (target)

```yaml
services:
  sitebox:
    build: .
    container_name: sitebox
    network_mode: host
    init: true
    volumes:
      - ./dashboard/data:/app/dashboard/data   # sites.json + agent.json + pi-agent/
      - ./sites:/app/sites
    environment:
      # Provider credentials are NOT here; they live in agent.json (0600).
      PI_CODING_AGENT_DIR: /app/dashboard/data/pi-agent
      PI_OFFLINE: "1"
      PI_SKIP_VERSION_CHECK: "1"
      SITEBOX_DASHBOARD_URL: http://localhost:4445
      SEARXNG_URL: http://localhost:8080
      CHROME: /usr/bin/chromium
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "node", "-e", "require('http').get('http://127.0.0.1:4445/api/sites',r=>process.exit(r.statusCode<500?0:1)).on('error',()=>process.exit(1))"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 5s
```

`init: true` already reaps orphaned site processes; it also reaps a stray pi child.

## 4. Volumes and what persists

| Host path | Container | Persists | Contains secrets |
|---|---|---|---|
| `./dashboard/data` | `/app/dashboard/data` | yes | **yes** (`agent.json`, `pi-agent/models.json`, sessions) |
| `./sites` | `/app/sites` | yes | no (site content) |
| image layers | `/app/dashboard`, `/app/skills`, `/app/agent` | rebuilt | no |

Everything the agent generates that must survive a restart lives under
`dashboard/data/pi-agent/`, which is inside the existing mounted volume — so **no
compose change is needed for the agent's state**.

## 5. Repo hygiene (git)

### 5.1 `.gitignore` additions

```gitignore
# Native agent — runtime state and secrets
dashboard/data/agent.json
dashboard/data/agent.json.tmp
dashboard/data/pi-agent/
```

`pi-agent/` covers `models.json` (key), `settings.json`, `system-prompt.md`, and
`sessions/` (private conversations).

### 5.2 Pre-commit guard (`.githooks/pre-commit`)

Extend the existing script so a staged secret is blocked even if `.gitignore` is
bypassed (`git add -f`):

```sh
#!/bin/sh
# existing sites.json check ...
if git diff --cached --name-only --diff-filter=ACMR | grep -qx 'dashboard/data/sites.json'; then
  # ... existing message ...
  exit 1
fi

# Native agent secrets/runtime must never be committed.
if git diff --cached --name-only --diff-filter=ACMR \
   | grep -Eq '^dashboard/data/(agent\.json|pi-agent/)'; then
  echo ""
  echo "COMMIT BLOCKED: agent runtime data / secrets."
  echo ""
  echo "dashboard/data/agent.json and dashboard/data/pi-agent/ contain the API"
  echo "key and private conversations. They are runtime state and must not be"
  echo "committed. Unstage them:"
  echo ""
  echo "  git restore --staged dashboard/data/agent.json dashboard/data/pi-agent/"
  echo ""
  exit 1
fi
exit 0
```

## 6. Bare-metal (no Docker) parity

The dashboard must also run from a plain `node dashboard/server.js`. In that case:

- `PI_CODING_AGENT_DIR` defaults to `<repo>/dashboard/data/pi-agent`.
- `CHROME` is discovered from `PATH` (`chromium`, `chromium-browser`,
  `google-chrome`), or left unset (verify skill degrades with a clear message).
- `SITEBOX_DASHBOARD_URL` defaults to `http://localhost:4445`.
- `pi` must be on `PATH`; if not, status is `unavailable` with a setup hint.

A `docs/native-agent/README` note and the status endpoint's hint cover this.

## 7. Upgrade procedure

1. Change `ARG PI_VERSION` in the Dockerfile.
2. Re-read upstream `docs/rpc.md`, `docs/rpc-commands.md`, `docs/json.md`; update
   [`02-rpc-protocol.md`](./02-rpc-protocol.md) and the ADR log if anything changed.
3. `docker compose up -d --build`.
4. Run the protocol tests ([`10-testing.md`](./10-testing.md)).
5. Verify an end-to-end site build.

**Never** upgrade the pinned version without re-reading the protocol docs; the
JSONL framing and event shapes are the contract.

## 8. First-run checklist (operator)

```
[ ] docker compose up -d --build
[ ] open http://<host>:4445
[ ] Agent → Settings → base URL, model, key → Test connection → Save
[ ] status dot turns green; model chip shows the model
[ ] chat → "สร้างเว็บทดสอบง่ายๆ"
[ ] site card appears and is running
[ ] close/reopen the mini-chat: transcript intact
[ ] docker compose restart: agent state survives (settings + sessions)
```

## 9. Troubleshooting matrix

| Symptom | Likely cause | Check |
|---|---|---|
| status `unavailable` | `pi` not on PATH in the image | `docker exec sitebox pi --version` |
| status `unavailable` | spawn ENOENT | agent log (stderr) via `/api/agent/events` `agent_log` |
| chat error 401/404 | wrong base URL/key/model | Settings → Test connection |
| blank reply | model returned empty / wrong API type | try `openai-responses` in Advanced; check endpoint |
| agent restarts in a loop | invalid `models.json` | `docker exec sitebox cat .../models.json` |
| verify phase says no browser | `CHROME` unset / chromium missing | `docker exec sitebox chromium --version` |
| port 4445 busy | another process / old container | `docker ps`, `ss -ltnp` |
| sites.json unreadable | hand-edited | restore from git (existing behavior) |

## 10. Image size & build notes

- Chromium adds ~250–350 MB. Accept it: the verify skill is a core quality gate
  ([ADR-002](./11-risks-and-decisions.md#adr-002)).
- Build cache: put `apt-get` before `COPY` so code changes do not re-install
  packages.
- If size becomes painful later, a multi-stage build that copies only `pi` + a
  chromium runtime could be explored (out of scope now).
