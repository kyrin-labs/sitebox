#!/usr/bin/env bash
# SiteBox — one-shot deploy with gates, baseline and a real rollback.
#
# The dashboard keeps its PID map in memory, so every recreate forgets which
# sites were running and kills every site process. That makes a deploy a
# two-part job: ship the image, then put back exactly what was running.
#
# Phases (each one a gate — a failure stops the run, never continues):
#   0 baseline      record config + listeners + HTTP (see sitebox-baseline.sh)
#   1 tag           pin the running image as sitebox:pre-mcp-<ts>  ← rollback anchor
#   2 down          stop the container (this kills all site processes)
#   3 chown sweep   make sites/ and dashboard/data/ owned by uid 1000
#   4 mint          host CLI writes tokens.json (must run AFTER the sweep)
#   5 up --build    rebuild + wait for /api/health to advertise auth
#   6 smoke         auth, port:0, MCP create/delete, permission + git gates
#   7 restore       start back only the sites the baseline saw, then set-diff
#
# Usage:
#   sitebox-deploy.sh --preflight    read-only checks (safe any time)
#   sitebox-deploy.sh --deploy       the full run
#   sitebox-deploy.sh --restore-sites  only phase 7
#   sitebox-deploy.sh --rollback     put the pinned image back, no rebuild
set -euo pipefail
export TZ="Asia/Bangkok"

ROOT="${SITEBOX_ROOT:-/home/l2s/sitebox}"
WORKTREE="${SITEBOX_WORKTREE:-/home/l2s/gh-project-ywei/sitebox}"
API="${SITEBOX_API:-http://127.0.0.1:4445}"
MCP_DIR="${MCP_DIR:-${WORKTREE}/mcp}"
BASELINE_DIR="${BASELINE_DIR:-/home/l2s/backups/sitebox-baseline}"
TOKEN_FILE="${TOKEN_FILE:-/home/l2s/.config/sitebox/mcp.token}"
IMAGE_LOCAL="sitebox-sitebox:latest"
PINNED_TAG="sitebox:pre-mcp-$(date -u +%Y%m%dT%H%M%SZ)"

say()  { printf '\n\033[1m== %s\033[0m\n' "$*"; }
ok()   { printf '   ok   %s\n' "$*"; }
bad()  { printf '   FAIL %s\n' "$*"; }
die()  { bad "$*"; exit 1; }
dk()   { sg docker -c "$*"; }
# compose needs to run from the deploy clone: the build context comes from there.
dcz()  { sg docker -c "cd ${ROOT} && docker compose $*"; }

api_get() { curl -s -m 5 -o /dev/null -w '%{http_code}' "$@"; }

# ── phase 0 ────────────────────────────────────────────────────────────────
phase_baseline() {
  say "0 · baseline (before anything changes)"
  /home/l2s/scripts/sitebox-baseline.sh --label pre-deploy >/dev/null
  PRE_BASELINE="$(ls -1t "${BASELINE_DIR}"/*-pre-deploy.json | head -1)"
  ok "pre-deploy baseline: $(basename "$PRE_BASELINE")"
  # Minting is only needed when the current key does not work; a redeploy that keeps
  # the same store must not rotate the key out from under a running MCP client.
  TOKEN_OK=0
  if [ -f "$TOKEN_FILE" ] && [ "$(api_get -H "Authorization: Bearer $(cat "$TOKEN_FILE")" "${API}/api/sites")" = "200" ]; then
    TOKEN_OK=1
    ok "existing token still authenticates — mint will be skipped"
  fi
  python3 - "$PRE_BASELINE" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print('   ', d['counts'], 'orphans:', [o['port'] for o in d['orphan_listeners']] or 'none')
for r in d['sites']:
    if r['listening']:
        print(f"    was up: {r['id']} :{r['port']} http={r['http']}")
PY
}

# ── phase 1 ────────────────────────────────────────────────────────────────
phase_tag() {
  say "1 · pin the running image as the rollback anchor"
  local id; id="$(dk "docker inspect -f '{{.Image}}' sitebox" 2>/dev/null || true)"
  [ -n "$id" ] || die "container 'sitebox' not found — nothing to pin"
  dk "docker tag ${IMAGE_LOCAL} ${PINNED_TAG}" >/dev/null
  echo "$PINNED_TAG" > "${BASELINE_DIR}/ROLLBACK_TAG"
  echo "$id" > "${BASELINE_DIR}/ROLLBACK_IMAGE_ID"
  ok "pinned ${PINNED_TAG}  (${id:0:19})"
  ok "rollback anchor saved to ${BASELINE_DIR}/ROLLBACK_TAG"
}

# ── phase 3 ────────────────────────────────────────────────────────────────
phase_chown() {
  say "3 · chown sweep (container must be down)"
  dk "docker run --rm -u 0 -v ${ROOT}:/w alpine chown -R 1000:1000 /w/sites /w/dashboard/data" >/dev/null
  local rootowned
  rootowned="$(find "${ROOT}/sites" "${ROOT}/dashboard/data" -user 0 2>/dev/null | wc -l)"
  [ "$rootowned" -eq 0 ] || die "${rootowned} files still owned by root under sites/ or dashboard/data/"
  ok "0 root-owned files under sites/ + dashboard/data/"
}

# ── phase 4 ────────────────────────────────────────────────────────────────
phase_mint() {
  say "4 · mint the MCP token (host CLI — this is also the recovery path)"
  if [ "${TOKEN_OK:-0}" = "1" ]; then
    ok "existing token is still valid — keeping it (no rotation on redeploy)"
    return
  fi
  local cli="${ROOT}/dashboard/token.js"
  [ -f "$cli" ] || die "dashboard/token.js missing — auth has not landed yet; nothing to mint"
  mkdir -p "$(dirname "$TOKEN_FILE")"
  KEY="$(cd "${ROOT}" && node dashboard/token.js add mcp --scope write --print 2>/dev/null | tail -1)"
  case "$KEY" in sbx_*) ;; *) die "token CLI did not return an sbx_-prefixed key" ;; esac
  umask 077; printf '%s\n' "$KEY" > "$TOKEN_FILE"; chmod 600 "$TOKEN_FILE"
  ok "wrote ${TOKEN_FILE} (600)"
  local perms owner
  perms="$(stat -c '%a %u' "${ROOT}/dashboard/data/tokens.json" 2>/dev/null || echo missing)"
  [ "$perms" = "600 1000" ] || [ "${perms%% *}" = "600" ] || die "tokens.json perms are '$perms', expected 600"
  ok "tokens.json: $perms"
}

# ── phase 5 ────────────────────────────────────────────────────────────────
phase_up() {
  say "5 · rebuild + wait for /api/health"
  dcz "up -d --build" >/dev/null
  local i code
  for i in $(seq 1 40); do
    code="$(api_get "${API}/api/health" || true)"
    [ "$code" = "200" ] && break
    sleep 1
  done
  [ "$code" = "200" ] || die "/api/health never returned 200 (last: ${code})"
  local feats; feats="$(curl -s -m 5 "${API}/api/health")"
  ok "health: $feats"
  case "$feats" in *'"auth"'*) ;; *) die "/api/health does not advertise auth — image is stale (did you git pull ${ROOT} before rebuild?)" ;; esac
  case "$feats" in *'"port0"'*) ok "port0 advertised" ;; *) die "port0 missing from features" ;; esac
}

# ── phase 6 ────────────────────────────────────────────────────────────────
phase_smoke() {
  say "6 · smoke gates"
  local tok; tok="$(cat "$TOKEN_FILE")"

  [ "$(api_get "${API}/api/health")" = "200" ] || die "health must stay unauthenticated"
  ok "health without token = 200"
  [ "$(api_get "${API}/api/sites")" = "401" ] || die "/api/sites without token must be 401"
  ok "/api/sites without token = 401"
  [ "$(api_get -H "Authorization: Bearer ${tok}" "${API}/api/sites")" = "200" ] || die "/api/sites with token != 200"
  ok "/api/sites with token = 200"
  [ "$(api_get -H "Authorization: Bearer wrong-${RANDOM}" "${API}/api/sites")" = "401" ] || die "a wrong token was accepted"
  ok "wrong token = 401"

  # cookie session: the UI path, and the only way <img> favicons can authenticate
  local jar; jar="$(mktemp)"
  curl -s -m 5 -c "$jar" -X POST "${API}/api/session" -H 'Content-Type: application/json' \
    -d "{\"token\":\"${tok}\"}" -o /dev/null || true
  [ "$(api_get -b "$jar" "${API}/api/sites")" = "200" ] || die "cookie session cannot read /api/sites"
  ok "cookie session = 200 (favicons will work)"
  local first_icon; first_icon="$(curl -s -m 5 -b "$jar" -H "Authorization: Bearer ${tok}" "${API}/api/sites" \
    | python3 -c 'import json,sys;print(next((s["_favicon"] for s in json.load(sys.stdin) if s.get("_favicon")),""))')"
  if [ -n "$first_icon" ]; then
    local icode; icode="$(api_get -b "$jar" "${API}${first_icon}")"
    case "$icode" in 401) die "favicon path returned 401 — the <img> bug is back" ;; *) ok "favicon path = ${icode} (not 401)" ;; esac
  fi
  rm -f "$jar"

  # The dashboard is open on the LAN by design; destructive writes are loopback-only.
  # Both halves have to be true at once, and a 401 must never be mistaken for a verdict:
  # without the cookie from GET / this would "pass" for exactly the wrong reason.
  local jar2 me_code me_loop
  jar2="$(mktemp)"
  curl -s -c "$jar2" -o /dev/null "${API}/" || true
  me_code="$(curl -s -m 5 -b "$jar2" -o /tmp/sb-me-loop.json -w '%{http_code}' "${API}/api/me")"
  me_loop="$(cat /tmp/sb-me-loop.json)"
  [ "$me_code" = "200" ] || die "/api/me on loopback answered ${me_code}, not 200 — the capability gate is meaningless without the UI cookie: ${me_loop}"
  case "$me_loop" in *'"canManage":true'*) ok "loopback view can manage" ;; *) die "loopback /api/me cannot manage: ${me_loop}" ;; esac
  case "$me_loop" in *'"scope":"session"'*) ok "UI session reports scope=session (no token scope it cannot use)" ;; *) die "UI session claims a token scope: ${me_loop}" ;; esac

  local lan_ip lan_code me_lan jar3 lan_del
  lan_ip="$(hostname -I 2>/dev/null | awk '{print $1}')"
  if [ -n "${lan_ip:-}" ]; then
    jar3="$(mktemp)"
    curl -s -c "$jar3" -o /dev/null "http://${lan_ip}:4445/" || true
    lan_code="$(curl -s -m 5 -b "$jar3" -o /tmp/sb-me-lan.json -w '%{http_code}' "http://${lan_ip}:4445/api/me")"
    me_lan="$(cat /tmp/sb-me-lan.json)"
    [ "$lan_code" = "200" ] || die "/api/me on the LAN answered ${lan_code}, not 200: ${me_lan}"
    # canManage must simply agree with mayWrite; the deploy does not care which way the
    # policy points, only that the build does not contradict itself.
    case "$me_lan" in
      *'"mayWrite":true'*)  case "$me_lan" in *'"canManage":true'*) ok "LAN may manage site writes (policy allows)" ;; *) die "mayWrite:true but canManage:false: ${me_lan}" ;; esac ;;
      *'"mayWrite":false'*) case "$me_lan" in *'"canManage":false'*) ok "LAN view is read-mostly (policy brakes it)" ;; *) die "mayWrite:false but canManage:true: ${me_lan}" ;; esac ;;
      *) die "cannot tell the write policy from /api/me: ${me_lan}" ;;
    esac
    # The policy is whatever the server announces: a build that turns writes off must
    # answer 403, a build that allows them must answer something else AND leave a record.
    local audit_log="${ROOT}/dashboard/data/audit.log"
    local before_lines after_lines
    before_lines="$( [ -f "$audit_log" ] && wc -l < "$audit_log" || echo 0 )"
    lan_del="$(curl -s -o /dev/null -m 5 -b "$jar3" -X DELETE -H "Origin: http://${lan_ip}:4445" -w '%{http_code}' "http://${lan_ip}:4445/api/sites/nonexistent-probe")"
    after_lines="$( [ -f "$audit_log" ] && wc -l < "$audit_log" || echo 0 )"
    case "$me_lan" in
      *'"mayWrite":true'*)
        ok "LAN may write (the network boundary is the gate)"
        [ "$lan_del" != "403" ] || die "LAN delete refused although /api/me says mayWrite:true"
        [ "$((after_lines - before_lines))" -eq 1 ] || die "the LAN delete left $((after_lines - before_lines)) audit line(s), expected exactly 1"
        ok "LAN delete answered ${lan_del} and wrote exactly 1 audit line"
        ;;
      *'"mayWrite":false'*)
        ok "LAN writes are braked by policy"
        [ "$lan_del" = "403" ] || die "LAN delete allowed although /api/me says mayWrite:false (got ${lan_del})"
        ok "LAN delete = 403"
        ;;
      *) die "cannot tell the write policy from /api/me: ${me_lan}" ;;
    esac
    rm -f "$jar3"
  fi
  rm -f "$jar2"

  # tokens.json must never be committable and never root-owned
  [ -z "$(git -C "$ROOT" ls-files dashboard/data/tokens.json)" ] || die "tokens.json is tracked by git"
  git -C "$ROOT" check-ignore -q dashboard/data/tokens.json || die "tokens.json is not gitignored"
  ok "tokens.json is ignored and untracked"
  [ "$(stat -c '%u' "${ROOT}/dashboard/data/tokens.json")" = "1000" ] || die "tokens.json is not owned by uid 1000"
  ok "tokens.json owned by uid 1000"
  dk "docker inspect sitebox --format '{{json .Config.Healthcheck.Test}}'" | grep -q 'api/health' \
    || die "healthcheck still points at a route that needs auth"
  ok "compose healthcheck uses /api/health"
  dk "docker inspect sitebox --format '{{json .Config.Healthcheck.Test}}'" | grep -q 'sbx_' \
    && die "a token is baked into the healthcheck (visible in docker inspect)" || true
  ok "no token inside the healthcheck command"

  # real MCP round trip through the stdio server, over the production API
  if [ -f "${MCP_DIR}/sitebox-mcp.js" ]; then
    say "6b · MCP create_site → check → delete_site (production, throwaway id)"
    SITEBOX_API="$API" SITEBOX_API_TOKEN="$tok" node "${MCP_DIR}/test/smoke-live.js" \
      || die "live MCP smoke failed"
    ok "MCP round trip passed"
  else
    bad "MCP server not found at ${MCP_DIR}/sitebox-mcp.js — skipping 6b"
  fi
}

# ── phase 7 ────────────────────────────────────────────────────────────────
phase_restore() {
  say "7 · start back exactly what the baseline saw"
  PRE_BASELINE="${PRE_BASELINE:-$(ls -1t "${BASELINE_DIR}"/*-pre-deploy.json 2>/dev/null | head -1)}"
  [ -n "${PRE_BASELINE:-}" ] || die "no pre-deploy baseline found in ${BASELINE_DIR}"
  local tok; tok="$(cat "$TOKEN_FILE")"
  python3 - "$PRE_BASELINE" > /tmp/sitebox-restore-ids.txt <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
for r in d['sites']:
    if r['listening']:
        print(r['id'])
PY
  local n=0 id
  while read -r id; do
    [ -n "$id" ] || continue
    curl -s -m 20 -X POST -H "Authorization: Bearer ${tok}" "${API}/api/sites/${id}/start" -o /dev/null || true
    n=$((n+1))
  done < /tmp/sitebox-restore-ids.txt
  ok "start requested for ${n} site(s)"
  sleep 2
  say "7b · set-diff (id, port) against the pre-deploy baseline"
  /home/l2s/scripts/sitebox-baseline.sh --label post-deploy >/dev/null
  local post; post="$(ls -1t "${BASELINE_DIR}"/*-post-deploy.json | head -1)"
  python3 - "$PRE_BASELINE" "$post" <<'PY'
import json, sys
pre, post = (json.load(open(p)) for p in sys.argv[1:3])
key = lambda r: (r['id'], r['port'])
was = {key(r) for r in pre['sites'] if r['listening']}
now = {key(r) for r in post['sites'] if r['listening']}
missing, extra = sorted(was - now), sorted(now - was)
print(f"    was up: {len(was)}   now up: {len(now)}")
if missing: print('   MISSING:', missing)
if extra:   print('   EXTRA  :', extra)
sys.exit(1 if (missing or extra) else 0)
PY
  ok "set-diff empty — the same (id, port) pairs came back"
}

# ── rollback ───────────────────────────────────────────────────────────────
do_rollback() {
  say "ROLLBACK — restore the pinned image (no rebuild, no git)"
  local tag; tag="$(cat "${BASELINE_DIR}/ROLLBACK_TAG" 2>/dev/null || true)"
  [ -n "$tag" ] || die "no pinned tag recorded — cannot roll back automatically"
  dcz "down" >/dev/null
  dk "docker tag ${tag} ${IMAGE_LOCAL}" >/dev/null
  dcz "up -d" >/dev/null
  ok "image ${tag} restored as ${IMAGE_LOCAL} and started without rebuild"
  ok "the old build has no auth, so the dashboard is reachable again as before"
  echo
  echo "   # copy-paste, if you ever need it by hand:"
  echo "   sg docker -c 'docker tag ${tag} ${IMAGE_LOCAL}'"
  echo "   sg docker -c 'cd ${ROOT} && docker compose down && docker compose up -d'"
}

# ── preflight ──────────────────────────────────────────────────────────────
do_preflight() {
  say "PREFLIGHT (read-only)"
  local fails=0
  local code; code="$(api_get "${API}/api/sites")"
  ok "dashboard ${API} responds /api/sites -> ${code}"
  local health; health="$(api_get "${API}/api/health")"
  if [ "$health" = "200" ]; then
    ok "/api/health -> 200 $(curl -s "${API}/api/health")"
  else
    bad "/api/health -> ${health}  (this build predates auth; expected before the first deploy)"; fails=$((fails+1))
  fi
  local img; img="$(dk "docker inspect -f '{{.Image}}' sitebox" 2>/dev/null || true)"
  ok "running image: ${img:0:19}"
  if [ -f "${ROOT}/dashboard/token.js" ]; then ok "token CLI present"; else bad "dashboard/token.js missing — mint/recovery path not available yet"; fails=$((fails+1)); fi
  if [ -f "${MCP_DIR}/sitebox-mcp.js" ]; then ok "MCP server present at ${MCP_DIR}"; else bad "MCP server not found at ${MCP_DIR}"; fails=$((fails+1)); fi
  # the mint precondition: can the host user actually write tokens.json here?
  if touch "${ROOT}/dashboard/data/.mint-probe" 2>/dev/null; then
    rm -f "${ROOT}/dashboard/data/.mint-probe"; ok "host user can create files in dashboard/data"
  else
    bad "host user cannot write dashboard/data — chown sweep has not run"; fails=$((fails+1))
  fi
  local rootowned; rootowned="$(find "${ROOT}/sites" "${ROOT}/dashboard/data" -user 0 2>/dev/null | wc -l)"
  ok "root-owned files under sites/ + dashboard/data: ${rootowned} (expect 0 after the sweep)"
  echo
  echo "   rollback anchor that WOULD be pinned: ${PINNED_TAG}"
  if [ "$fails" -gt 0 ]; then
    echo "   ${fails} blocker(s) — --deploy would stop; that is the point of a gate."
    exit 3
  fi
}

case "${1:---preflight}" in
  --preflight)     do_preflight ;;
  --deploy)        phase_baseline; phase_tag; say "2 · compose down"; dcz "down" >/dev/null; ok "container down"; phase_chown; phase_mint; phase_up; phase_smoke; phase_restore ;;
  --restore-sites) phase_restore ;;
  --rollback)      do_rollback ;;
  *) echo "usage: $0 [--preflight|--deploy|--restore-sites|--rollback]"; exit 2 ;;
esac
