#!/usr/bin/env bash
# SiteBox — deploy the image and put the running sites back.
#
# The dashboard keeps its PID map in memory, so recreating the container forgets
# which sites were running and kills every site process. The deploy is therefore
# two jobs: ship the image, then start exactly what was running before.
#
# Phases (a failure stops the run):
#   0 baseline   snapshot which sites are running (ids + ports)
#   1 tag        pin the current image for rollback
#   2 down       stop the container (kills every site process)
#   3 chown      make sites/ and dashboard/data/ owned by uid 1000
#   4 up         rebuild and wait for /api/sites
#   5 smoke      health, sites list, body limit, one start/stop round trip
#   6 restore    start back the sites the baseline saw, then diff
#
# Usage:
#   sitebox-deploy.sh --preflight      read-only checks
#   sitebox-deploy.sh --deploy         the full run
#   sitebox-deploy.sh --restore-sites  only phase 6
#   sitebox-deploy.sh --rollback       put the pinned image back, no rebuild
set -euo pipefail
export TZ="Asia/Bangkok"

ROOT="${SITEBOX_ROOT:-/home/l2s/sitebox}"
API="${SITEBOX_API:-http://127.0.0.1:4445}"
BASELINE_DIR="${BASELINE_DIR:-/home/l2s/backups/sitebox-baseline}"
PINNED_TAG="sitebox:pre-deploy-$(date -u +%Y%m%dT%H%M%SZ)"

say() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
ok()  { printf '   ok   %s\n' "$*"; }
bad() { printf '   FAIL %s\n' "$*"; }
die() { bad "$*"; exit 1; }
dk()  { sg docker -c "$*"; }
dcz() { sg docker -c "cd ${ROOT} && docker compose $*"; }
code(){ curl -s -m 5 -o /dev/null -w '%{http_code}' "$@"; }

running_ids() {
  curl -s -m 10 "${API}/api/sites" \
    | python3 -c 'import json,sys;print("\n".join(s["id"] for s in json.load(sys.stdin) if s.get("_running")))'
}

wait_api() {
  local i c
  for i in $(seq 1 60); do
    c="$(code "${API}/api/sites")"
    [ "$c" = "200" ] && return 0
    sleep 2
  done
  return 1
}

phase_baseline() {
  say "0 · baseline"
  mkdir -p "$BASELINE_DIR"
  PRE_BASELINE="${BASELINE_DIR}/$(date -u +%Y%m%dT%H%M%SZ)-pre-deploy.txt"
  running_ids > "$PRE_BASELINE"
  ok "$(wc -l < "$PRE_BASELINE") running site(s) recorded in $(basename "$PRE_BASELINE")"
}

phase_tag() {
  say "1 · pin the current image"
  dk "docker tag sitebox-sitebox:latest ${PINNED_TAG}" && ok "tagged ${PINNED_TAG}"
}

phase_chown() {
  say "3 · chown sweep (uid 1000)"
  dk "docker run --rm -v ${ROOT}/sites:/sites -v ${ROOT}/dashboard/data:/data alpine:3 sh -c 'chown -R 1000:1000 /sites /data'" \
    && ok "sites/ and dashboard/data/ owned by 1000"
}

phase_smoke() {
  say "5 · smoke"
  [ "$(code "${API}/api/sites")" = "200" ] || die "/api/sites did not answer 200"
  ok "/api/sites = 200"
  [ "$(code "${API}/")" = "200" ] || die "dashboard HTML did not answer 200"
  ok "dashboard HTML = 200"

  # Every site is a child of this process; an unbounded body would take them all down.
  local big
  big="$(python3 -c 'print("{\"x\":\"" + "a"*7000000 + "\"}")' \
    | curl -s -o /dev/null -m 20 -X POST -H 'Content-Type: application/json' --data-binary @- -w '%{http_code}' "${API}/api/sites")"
  [ "$big" = "413" ] || die "a 7 MB POST to /api/sites answered ${big}, expected 413"
  ok "oversized body rejected with 413"

  local port start stop
  port="$(curl -s -m 5 "${API}/api/sites" | python3 -c 'import json,sys;s=json.load(sys.stdin);print(next((x["port"] for x in s if x["id"]=="example-notes"),""))')"
  start="$(code -X POST "${API}/api/sites/example-notes/start")"
  [ "$start" = "200" ] || die "example-notes did not start (${start})"
  [ "$(code "http://127.0.0.1:${port}/")" = "200" ] || die "example-notes port ${port} is not serving"
  ok "example-notes started and served on ${port}"
  stop="$(code -X POST "${API}/api/sites/example-notes/stop")"
  [ "$stop" = "200" ] || die "example-notes did not stop (${stop})"
  ok "example-notes stopped"
}

phase_restore() {
  say "6 · restore the running sites"
  PRE_BASELINE="${PRE_BASELINE:-$(ls -1t "${BASELINE_DIR}"/*-pre-deploy.txt 2>/dev/null | head -1)}"
  [ -n "${PRE_BASELINE:-}" ] || die "no baseline in ${BASELINE_DIR}"
  while IFS= read -r id; do
    [ -n "$id" ] || continue
    local c; c="$(code -X POST "${API}/api/sites/${id}/start")"
    [ "$c" = "200" ] && ok "started ${id}" || bad "${id} answered ${c}"
  done < "$PRE_BASELINE"

  local want got
  want="$(sort "$PRE_BASELINE" | sed '/^$/d')"
  got="$(running_ids | sort)"
  if [ "$want" = "$got" ]; then ok "running set matches the baseline"; else
    bad "running set differs"; diff <(printf '%s\n' "$want") <(printf '%s\n' "$got") || true; exit 1
  fi
}

case "${1:---deploy}" in
  --preflight)
    say "preflight"
    [ -d "${ROOT}/sites" ] || die "no sites/ under ${ROOT}"
    dk "docker image inspect sitebox-sitebox:latest >/dev/null" && ok "image present"
    [ "$(code "${API}/api/sites")" = "200" ] && ok "API answers 200" || bad "API is not answering"
    ;;
  --deploy)
    phase_baseline; phase_tag; say "2 · compose down"; dcz "down" >/dev/null; ok "container down"
    phase_chown
    say "4 · up --build"
    dcz "up -d --build" >/dev/null
    wait_api && ok "API is up" || die "API did not come up"
    phase_smoke; phase_restore ;;
  --restore-sites) phase_restore ;;
  --rollback)
    say "rollback to ${PINNED_TAG}"
    dcz "down" >/dev/null
    dk "docker tag ${PINNED_TAG} sitebox-sitebox:latest"
    dcz "up -d" >/dev/null; wait_api && ok "rolled back" || die "API did not come up"; phase_restore ;;
  *) die "unknown flag $1" ;;
esac
