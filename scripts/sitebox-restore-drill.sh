#!/usr/bin/env bash
# SiteBox — restore drill. Proves a backup restores, without touching production.
#
# A backup that has never been restored is a hypothesis. This runs the whole claim on a
# throwaway copy: a second dashboard on its own port, reading the snapshot's data dir,
# serving a site folder taken straight out of the snapshot.
#
# What it asserts, in the order that matters:
#   1. the restored token store still authenticates the key the client already has
#      (401 without the key, 200 with it — a store whose hashes are intact)
#   2. the registry survived with the same number of sites
#   3. a site folder deleted from the copy can be brought back from the snapshot and
#      answered 200 by its own server.js
#
# Production is never stopped, never read for state other than the snapshot itself, and
# never written to: SITEBOX_DATA points into a temp dir and the dashboard port is
# rewritten in the copy.
#
# Usage: sitebox-restore-drill.sh [snapshot-dir]
set -euo pipefail
export TZ="Asia/Bangkok"

SNAPSHOT="${1:-$(ls -1d /home/l2s/backups/sitebox/[0-9]* 2>/dev/null | sort | tail -1 || true)}"
SRC_DASH="/home/l2s/sitebox/dashboard"
TOKEN_FILE="${SITEBOX_API_TOKEN_FILE:-/home/l2s/.config/sitebox/mcp.token}"
SCRATCH="${SCRATCH:-/home/l2s/.hermes/profiles/dev-ops/cache/scratch}"
DASH_PORT="${DRILL_DASH_PORT:-4492}"
SITE_PORT="${DRILL_SITE_PORT:-4621}"

[ -n "${SNAPSHOT}" ] && [ -d "${SNAPSHOT}" ] || { echo "FAIL no snapshot found" >&2; exit 1; }
[ -f "${SNAPSHOT}/dashboard-data/tokens.json" ] || { echo "FAIL snapshot has no tokens.json" >&2; exit 1; }
[ -f "${TOKEN_FILE}" ] || { echo "FAIL no client token at ${TOKEN_FILE}" >&2; exit 1; }

WORK="$(mktemp -d "${SCRATCH}/sitebox-drill-XXXXXX")"
DASH_PID=""; SITE_PID=""
cleanup() {
  for pid in "${SITE_PID}" "${DASH_PID}"; do
    [ -n "$pid" ] || continue
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 1 20); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill -9 "$pid" 2>/dev/null || true
  done
  rm -rf "${WORK}"
}
trap cleanup EXIT

fails=0
pass() { printf 'PASS %-46s [%s]\n' "$1" "${2:-}"; }
fail() { printf 'FAIL %-46s [%s]\n' "$1" "${2:-}"; fails=$((fails+1)); }

port_busy() { ss -ltn 2>/dev/null | awk '{print $4}' | grep -q ":$1$"; }
for p in "${DASH_PORT}" "${SITE_PORT}"; do
  if port_busy "$p"; then
    echo "FAIL port ${p} is already in use — a leftover drill process would be tested instead of this snapshot" >&2
    echo "     kill it first: ss -ltnp | grep :${p}" >&2
    exit 1
  fi
done

echo "snapshot: ${SNAPSHOT##*/}"

# ── 1. a throwaway copy of the snapshot + a dashboard to read it ────────────────
mkdir -p "${WORK}/app/dashboard"
cp -a "${SNAPSHOT}/sites" "${WORK}/app/sites"
cp -a "${SNAPSHOT}/dashboard-data" "${WORK}/app/dashboard/data"
cp -a "${SRC_DASH}/server.js" "${SRC_DASH}/auth.js" "${SRC_DASH}/tokenstore.js" \
      "${SRC_DASH}/token.js" "${SRC_DASH}/audit.js" "${SRC_DASH}/public" "${WORK}/app/dashboard/"
sed -i "s/^const PORT = 4445;/const PORT = ${DASH_PORT};/" "${WORK}/app/dashboard/server.js"
grep -q "const PORT = ${DASH_PORT};" "${WORK}/app/dashboard/server.js" \
  || { echo "FAIL could not rewrite the dashboard port in the copy" >&2; exit 1; }

( cd "${WORK}/app" && exec env SITEBOX_DATA="${WORK}/app/dashboard/data" node dashboard/server.js > "${WORK}/dash.log" 2>&1 ) &
DASH_PID=$!
for _ in $(seq 1 30); do
  [ "$(curl -s -o /dev/null -m 2 -w '%{http_code}' "http://127.0.0.1:${DASH_PORT}/api/health" || true)" = "200" ] && break
  sleep 0.3
done

# ── 2. the token store restored intact: 401 without, 200 with ───────────────────
code="$(curl -s -o /dev/null -m 3 -w '%{http_code}' "http://127.0.0.1:${DASH_PORT}/api/sites" || true)"
[ "${code}" = "401" ] && pass "restored dashboard refuses anon" "401" || fail "restored dashboard refuses anon" "got ${code}"

TOK="$(cat "${TOKEN_FILE}")"
code="$(curl -s -o /dev/null -m 3 -H "Authorization: Bearer ${TOK}" -w '%{http_code}' "http://127.0.0.1:${DASH_PORT}/api/sites" || true)"
[ "${code}" = "200" ] && pass "restored token store accepts the client key" "200" || fail "restored token store accepts the client key" "got ${code}"

got="$(curl -s -m 3 -H "Authorization: Bearer ${TOK}" "http://127.0.0.1:${DASH_PORT}/api/sites" \
  | python3 -c 'import json,sys
d = json.load(sys.stdin)
print(len(d) if isinstance(d, list) else -1)' 2>/dev/null || echo -1)"
want="$(python3 -c 'import json,sys;print(len(json.load(open(sys.argv[1]))["sites"]))' "${SNAPSHOT}/dashboard-data/sites.json" 2>/dev/null || echo 0)"
[ "${got}" = "${want}" ] && pass "registry restored with every entry" "${got} sites" || fail "registry restored with every entry" "${got} != ${want}"

# ── 3. a deleted site folder comes back from the snapshot ───────────────────────
VICTIM=""
while read -r d; do
  if [ -f "${d}/server.js" ] && [ -f "${d}/public/index.html" ]; then VICTIM="$(basename "${d}")"; break; fi
done < <(find "${WORK}/app/sites" -mindepth 1 -maxdepth 1 -type d | sort)
[ -n "${VICTIM}" ] || { echo "FAIL no restorable site folder in the snapshot" >&2; exit 1; }
echo "victim: ${VICTIM} (deleted from the copy, then restored from the snapshot)"

rm -rf "${WORK}/app/sites/${VICTIM}"
[ -d "${WORK}/app/sites/${VICTIM}" ] && fail "site folder deleted from the copy" "still there" || pass "site folder deleted from the copy" "${VICTIM} gone"

cp -a "${SNAPSHOT}/sites/${VICTIM}" "${WORK}/app/sites/${VICTIM}"
[ -f "${WORK}/app/sites/${VICTIM}/public/index.html" ] \
  && pass "folder restored from the snapshot" "$(du -sh "${WORK}/app/sites/${VICTIM}" | cut -f1)" \
  || fail "folder restored from the snapshot" "missing index.html"

( cd "${WORK}/app/sites/${VICTIM}" && exec env PORT="${SITE_PORT}" HOST=127.0.0.1 node server.js > "${WORK}/site.log" 2>&1 ) &
SITE_PID=$!
site_code=""
for _ in $(seq 1 20); do
  site_code="$(curl -s -o /dev/null -m 2 -w '%{http_code}' "http://127.0.0.1:${SITE_PORT}/" || true)"
  [ "${site_code}" = "200" ] && break
  sleep 0.3
done
[ "${site_code}" = "200" ] && pass "restored site serves its page" "GET / = 200" || fail "restored site serves its page" "got ${site_code:-none}"

echo
if [ "${fails}" -gt 0 ]; then
  echo "DRILL FAILED (${fails} check(s))"
  echo "--- dashboard log ---"; tail -5 "${WORK}/dash.log" || true
  echo "--- site log ---"; tail -5 "${WORK}/site.log" || true
  exit 1
fi
echo "DRILL PASSED — the snapshot restores to a working dashboard and a working site"
