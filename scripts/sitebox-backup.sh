#!/usr/bin/env bash
# SiteBox — nightly backup of the two things that cannot be regenerated.
#
#   sites/            every site folder (426MB today; uploads and media live here)
#   dashboard/data/   sites.json (the registry) and studio/ (conversations,
#                     patches, snapshots, uploads, shots, settings)
#
# `--rollback` protects the *code*. This protects the *data*: delete_site removes a
# folder for real, and nothing else on this host can bring it back.
#
# Shape: a dated snapshot directory, with sites/ hardlinked against the previous
# snapshot (rsync --link-dest). A night where nothing changed costs kilobytes, not
# 426MB, and every snapshot still looks like a plain full tree to a restore.
#
# Usage: sitebox-backup.sh [--no-prune]
set -euo pipefail
export TZ="Asia/Bangkok"

SRC_ROOT="/home/l2s/sitebox"
DEST_ROOT="${DEST_ROOT:-/home/l2s/backups/sitebox}"
KEEP_DAILY=14
KEEP_WEEKLY=8

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
DEST="${DEST_ROOT}/${STAMP}"
LOG="${DEST_ROOT}/backup.log"
ERRORS=0

mkdir -p "${DEST_ROOT}"
log() { printf '%s %s\n' "$(date -Is)" "$*" | tee -a "${LOG}"; }

log "=== sitebox backup ${STAMP} start ==="

# --- 0. refuse to snapshot a live-mutating tree without saying so ---------------
# The dashboard writes sites.json and the studio transcript continuously. That is fine
# for a file-level snapshot: each file is replaced atomically by the app.
if ! curl -sf -m 3 http://127.0.0.1:4445/api/sites >/dev/null; then
  log "WARN dashboard is not answering /api/sites — snapshotting anyway"
fi

# --- 1. previous snapshot to hardlink against -----------------------------------
PREV="$(ls -1d "${DEST_ROOT}"/[0-9]* 2>/dev/null | sort | tail -1 || true)"
LINK_ARGS=()
if [ -n "${PREV}" ] && [ -d "${PREV}/sites" ]; then
  LINK_ARGS=(--link-dest="${PREV}/sites")
  log "hardlinking against ${PREV##*/}"
fi

# --- 2. sites/ ----------------------------------------------------------------
mkdir -p "${DEST}/sites"
if rsync -a --delete "${LINK_ARGS[@]+"${LINK_ARGS[@]}"}" "${SRC_ROOT}/sites/" "${DEST}/sites/" 2>>"${LOG}"; then
  log "sites: $(find "${DEST}/sites" -type f | wc -l) files, $(du -sh "${DEST}/sites" | cut -f1) on disk ($(du -sh --apparent-size "${DEST}/sites" | cut -f1) apparent)"
else
  log "FAIL rsync sites/"; ERRORS=$((ERRORS+1))
fi

# --- 3. dashboard/data ---------------------------------------------------------
mkdir -p "${DEST}/dashboard-data"
if cp -a "${SRC_ROOT}/dashboard/data/." "${DEST}/dashboard-data/" 2>>"${LOG}"; then
  log "data: $(ls -1 "${DEST}/dashboard-data" | wc -l) files ($(du -sh "${DEST}/dashboard-data" | cut -f1))"
else
  log "FAIL copy dashboard/data"; ERRORS=$((ERRORS+1))
fi

# --- 4. verify against the source ---------------------------------------------
# diff -rq names the one file that is missing or different instead of leaving two
# people quoting totals at each other.
VERIFY_OUT="$(diff -rq "${SRC_ROOT}/sites" "${DEST}/sites" 2>&1 | head -20 || true)"
if [ -n "${VERIFY_OUT}" ]; then
  # A live site writing during the copy produces a benign difference; say so, loudly.
  log "DIFF sites/ (files changed while copying?):"
  printf '%s\n' "${VERIFY_OUT}" | tee -a "${LOG}"
else
  log "verify sites/: byte-identical to source"
fi
DATA_DIFF="$(diff -rq "${SRC_ROOT}/dashboard/data" "${DEST}/dashboard-data" 2>&1 | head -10 || true)"
if [ -n "${DATA_DIFF}" ]; then
  log "NOTE data/ changed during copy (expected on a busy dashboard):"
  printf '%s\n' "${DATA_DIFF}" | tee -a "${LOG}"
else
  log "verify data/: byte-identical to source"
fi

# The registry is the thing a restore is judged by: count it, do not just hash it.
SITES_JSON="${DEST}/dashboard-data/sites.json"
if [ -f "${SITES_JSON}" ]; then
  if COUNT="$(python3 -c 'import json,sys;print(len(json.load(open(sys.argv[1]))["sites"]))' "${SITES_JSON}" 2>/dev/null)"; then
    FOLDERS="$(find "${DEST}/sites" -mindepth 1 -maxdepth 1 -type d | wc -l)"
    log "registry: ${COUNT} site entries · ${FOLDERS} folders on disk"
    [ "${COUNT}" -le "${FOLDERS}" ] || log "WARN registry has entries with no folder (stale entries are normal)"
  else
    log "FAIL sites.json in the snapshot does not parse"; ERRORS=$((ERRORS+1))
  fi
fi
if [ -d "${DEST}/dashboard-data/studio" ]; then
  log "studio: $(find "${DEST}/dashboard-data/studio" -type f | wc -l) files"
fi

# --- 5. manifest + total ------------------------------------------------------
( cd "${DEST}" && find sites dashboard-data -type f -print0 | sort -z | xargs -0 sha256sum > MANIFEST.sha256 2>/dev/null ) || true
log "manifest: $(wc -l < "${DEST}/MANIFEST.sha256") files hashed"
log "snapshot size on disk: $(du -sh "${DEST}" | cut -f1) · free space: $(df -h "${DEST_ROOT}" | awk 'NR==2{print $4}')"

# --- 6. retention: 14 daily, 8 weekly (Sunday snapshots) ----------------------
if [ "${2:-}" != "--no-prune" ] && [ "${1:-}" != "--no-prune" ]; then
  ALL="$(ls -1d "${DEST_ROOT}"/[0-9]* 2>/dev/null | sort || true)"
  TOTAL="$(printf '%s\n' "${ALL}" | grep -c . || true)"
  if [ "${TOTAL}" -gt "${KEEP_DAILY}" ]; then
    # keep the newest KEEP_DAILY, plus anything dated Sunday up to KEEP_WEEKLY weeks
    WEEKLY=0
    for d in $(printf '%s\n' "${ALL}" | head -n "$((TOTAL - KEEP_DAILY))"); do
      DOW="$(date -u -d "$(basename "$d" | cut -c1-8)" +%u 2>/dev/null || echo 0)"
      if [ "${DOW}" = "7" ] && [ "${WEEKLY}" -lt "${KEEP_WEEKLY}" ]; then
        WEEKLY=$((WEEKLY+1)); continue
      fi
      rm -rf "$d" && log "pruned ${d##*/}"
    done
  fi
fi

# --- 7. off-box copy ------------------------------------------------------------
# Backups on the same filesystem as the data protect against mistakes, not against the
# disk. Set OFFBOX_TARGET (an rsync destination: ssh peer, mounted disk) to keep a second
# copy; unset means the run says so out loud rather than implying coverage it lacks.
if [ -n "${OFFBOX_TARGET:-}" ]; then
  if rsync -a --delete "${DEST_ROOT}/" "${OFFBOX_TARGET}/" 2>>"${LOG}"; then
    log "offbox: synced to ${OFFBOX_TARGET}"
  else
    log "FAIL offbox rsync to ${OFFBOX_TARGET}"; ERRORS=$((ERRORS+1))
  fi
else
  log "offbox: NOT configured — this backup lives on the same filesystem as the data"
fi

if [ "${ERRORS}" -gt 0 ]; then
  log "=== backup ${STAMP} FAILED (${ERRORS} error(s)) ==="
  exit 1
fi
log "=== backup ${STAMP} ok ==="
