#!/usr/bin/env bash
# SiteBox — restore drill. Proves a backup restores, without touching production.
#
# A backup that has never been restored is a hypothesis. This runs the claim on a
# throwaway copy: the real engine binary, its own port, reading the snapshot's data dir.
#
# What it asserts:
#   1. the engine starts against the snapshot and lists the registry
#   2. the registry has the same number of sites as sites.json says
#   3. a site folder deleted from the copy can be brought back from the snapshot
#
# Production is never stopped and never written to.
#
# Usage: sitebox-restore-drill.sh [snapshot-dir]
set -euo pipefail
export TZ="Asia/Bangkok"

SNAPSHOT="${1:-$(ls -1d /home/l2s/backups/sitebox/[0-9]* 2>/dev/null | sort | tail -1 || true)}"
BIN="${SITEBOX_BIN:-/home/l2s/gh-project-ywei/sitebox/engine/target/release/siteboxd}"
UI="${SITEBOX_UI:-/home/l2s/gh-project-ywei/sitebox/dashboard/public}"
SCRATCH="${SCRATCH:-/home/l2s/.hermes/cache/scratch}"
DASH_PORT="${DRILL_DASH_PORT:-4492}"

[ -n "${SNAPSHOT}" ] && [ -d "${SNAPSHOT}" ] || { echo "FAIL no snapshot found" >&2; exit 1; }
[ -x "${BIN}" ] || { echo "FAIL no engine binary at ${BIN}" >&2; exit 1; }

WORK="$(mktemp -d "${SCRATCH}/sitebox-drill-XXXXXX")"
DASH_PID=""
cleanup() {
  [ -n "${DASH_PID}" ] || true
  if [ -n "${DASH_PID}" ]; then
    kill "${DASH_PID}" 2>/dev/null || true
    for _ in $(seq 1 20); do kill -0 "${DASH_PID}" 2>/dev/null || break; sleep 0.1; done
    kill -9 "${DASH_PID}" 2>/dev/null || true
  fi
  rm -rf "${WORK}"
}
trap cleanup EXIT

fails=0
pass() { printf 'PASS %-46s [%s]\n' "$1" "${2:-}"; }
fail() { printf 'FAIL %-46s [%s]\n' "$1" "${2:-}"; fails=$((fails+1)); }

if ss -ltn 2>/dev/null | awk '{print $4}' | grep -q ":${DASH_PORT}$"; then
  echo "FAIL port ${DASH_PORT} is already in use" >&2; exit 1
fi

echo "snapshot: ${SNAPSHOT##*/}"
mkdir -p "${WORK}/app/dashboard"
cp -a "${SNAPSHOT}/sites" "${WORK}/app/sites"
cp -a "${SNAPSHOT}/dashboard-data" "${WORK}/app/dashboard/data"

( exec "${BIN}" --root "${WORK}/app" --port "${DASH_PORT}" --ui "${UI}" > "${WORK}/dash.log" 2>&1 ) &
DASH_PID=$!
code=""
for _ in $(seq 1 40); do
  code="$(curl -s -o /dev/null -m 2 -w '%{http_code}' "http://127.0.0.1:${DASH_PORT}/api/sites" || true)"
  [ "${code}" = "200" ] && break
  sleep 0.25
done
[ "${code}" = "200" ] && pass "engine starts against the snapshot" "200" || fail "engine starts against the snapshot" "got ${code:-none}"

got="$(curl -s -m 3 "http://127.0.0.1:${DASH_PORT}/api/sites" \
  | python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d) if isinstance(d,list) else -1)' 2>/dev/null || echo -1)"
want="$(python3 -c 'import json,sys;print(len(json.load(open(sys.argv[1]))["sites"]))' "${SNAPSHOT}/dashboard-data/sites.json" 2>/dev/null || echo 0)"
[ "${got}" = "${want}" ] && pass "registry restored with every entry" "${got} sites" || fail "registry restored with every entry" "${got} != ${want}"

VICTIM="$(find "${WORK}/app/sites" -mindepth 1 -maxdepth 1 -type d | sort | head -1)"
[ -n "${VICTIM}" ] || { echo "FAIL no site folder in the snapshot" >&2; exit 1; }
VICTIM="$(basename "${VICTIM}")"
echo "victim: ${VICTIM} (deleted from the copy, then restored from the snapshot)"

rm -rf "${WORK}/app/sites/${VICTIM}"
[ -d "${WORK}/app/sites/${VICTIM}" ] && fail "site folder deleted from the copy" "still there" || pass "site folder deleted from the copy" "${VICTIM} gone"

cp -a "${SNAPSHOT}/sites/${VICTIM}" "${WORK}/app/sites/${VICTIM}"
[ -d "${WORK}/app/sites/${VICTIM}" ] \
  && pass "folder restored from the snapshot" "$(du -sh "${WORK}/app/sites/${VICTIM}" | cut -f1)" \
  || fail "folder restored from the snapshot" "missing"

echo
if [ "${fails}" -gt 0 ]; then
  echo "DRILL FAILED (${fails} check(s))"
  echo "--- dashboard log ---"; tail -5 "${WORK}/dash.log" || true
  exit 1
fi
echo "DRILL PASSED — the snapshot restores to a working dashboard"
