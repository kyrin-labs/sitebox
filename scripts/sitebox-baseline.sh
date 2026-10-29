#!/usr/bin/env bash
# SiteBox — pre-restart baseline snapshot.
#
# Why this exists: the dashboard keeps its PID map in memory, so every
# `docker compose up -d --build` (recreate) forgets which sites were running.
# Any inference done AFTER the fact is guesswork, so the only trustworthy
# answer to "which sites should come back?" is recorded BEFORE the restart.
#
# For each site in sites.json this records three independent facts:
#   inConfig   — the entry exists
#   listening  — the OS has a listener on that port (ss)
#   http       — GET / on the port answers 2xx/3xx
# The three disagree on purpose: a stale listener (listening but untracked) and
# a half-dead site are exactly the cases that cause port collisions later.
#
# Usage: sitebox-baseline.sh [--label pre-recreate]
set -euo pipefail
export TZ="Asia/Bangkok"

API="${SITEBOX_API:-http://127.0.0.1:4445}"
LABEL="${2:-snapshot}"
TOKEN="${SITEBOX_API_TOKEN:-$(cat "${HOME}/.config/sitebox/mcp.token" 2>/dev/null || true)}"
AUTH=(); [ -n "${TOKEN}" ] && AUTH=(-H "Authorization: Bearer ${TOKEN}")
DEST_ROOT="${DEST_ROOT:-/home/l2s/backups/sitebox-baseline}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="${DEST_ROOT}/${STAMP}-${LABEL}.json"
LOG="${DEST_ROOT}/baseline.log"

mkdir -p "${DEST_ROOT}"
log() { printf '%s %s\n' "$(date -Is)" "$*" | tee -a "${LOG}"; }

if ! curl -sf -m 5 ${AUTH[@]+"${AUTH[@]}"} "${API}/api/sites" -o /tmp/sitebox-baseline-sites.json; then
  log "FAIL cannot read ${API}/api/sites (dashboard down, or the token is missing/stale)"
  exit 1
fi

python3 - "$OUT" "$API" "$STAMP" "$LABEL" <<'PY'
import json, socket, subprocess, sys, urllib.request

out, api, stamp, label = sys.argv[1:5]
sites = json.load(open('/tmp/sitebox-baseline-sites.json'))

def listening(port):
    r = subprocess.run(['ss', '-ltnH', f'sport = :{port}'],
                       capture_output=True, text=True)
    return bool(r.stdout.strip())

def http_code(port):
    try:
        req = urllib.request.Request(f'http://127.0.0.1:{port}/', method='GET')
        with urllib.request.urlopen(req, timeout=3) as r:
            return r.status
    except urllib.error.HTTPError as e:
        return e.code
    except Exception:
        return 0

rows = []
for s in sorted(sites, key=lambda x: x.get('port') or 0):
    port = s.get('port')
    rows.append({
        'id': s.get('id'),
        'port': port,
        'host': s.get('host'),
        'inConfig': True,
        'listening': bool(port) and listening(port),
        'http': http_code(port) if port else 0,
    })

# Listeners on the site band that no config entry claims: these are the ones
# that poison port allocation later.
claimed = {r['port'] for r in rows}
ss_out = subprocess.run(['ss', '-ltnH'], capture_output=True, text=True).stdout
orphans = []
for line in ss_out.splitlines():
    parts = line.split()
    if len(parts) < 4:
        continue
    addr = parts[3]
    try:
        port = int(addr.rsplit(':', 1)[1])
    except (IndexError, ValueError):
        continue
    if 4450 <= port <= 5099 and port not in claimed:
        orphans.append({'port': port, 'bind': addr})

snapshot = {
    'stamp': stamp,
    'label': label,
    'api': api,
    'sites': rows,
    'counts': {
        'inConfig': len(rows),
        'listening': sum(1 for r in rows if r['listening']),
        'http_ok': sum(1 for r in rows if 200 <= r['http'] < 400),
    },
    'orphan_listeners': orphans,
}
json.dump(snapshot, open(out, 'w'), indent=2)
print(json.dumps(snapshot['counts']))
print('orphans:', [o['port'] for o in orphans] or 'none')
print('saved:', out)
PY

log "baseline ${STAMP} label=${LABEL} -> ${OUT}"
