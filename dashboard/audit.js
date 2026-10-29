'use strict';
/**
 * audit — one JSONL line per authorised state change.
 *
 * There is no per-machine authorisation any more: any device that can reach the
 * dashboard may act on it. The network boundary is the gate, and this file is the
 * only record of what crossed it — so the line has to answer "who, what, when"
 * without containing anything secret itself.
 *
 * Rules, each one learned the hard way elsewhere in this codebase:
 *   * append-only. One `appendFileSync` per line (O_APPEND is atomic enough below
 *     the 4KB line we emit). Never read-modify-write JSON.
 *   * no secrets. No Authorization header, no cookie value, no query string. The
 *     cookie is reduced to six hex characters of its sha256 — enough to correlate
 *     lines from one browser, useless as a credential.
 *   * bounded. 1MB × 5 files, rotated on write, so `dashboard/data/` and the nightly
 *     backup do not grow without limit.
 *   * the actor names the credential when there is one: `token:<label>`. A browser
 *     session has no name, so it gets a fingerprint plus IP and a short UA, which is
 *     the honest answer ("same device") rather than a flattering one ("a user").
 */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const DATA = process.env.SITEBOX_DATA
  ? path.resolve(process.env.SITEBOX_DATA)
  : path.join(__dirname, 'data');
const LOG = path.join(DATA, 'audit.log');
const MAX_BYTES = 1024 * 1024;
const limit = () => Number(process.env.SITEBOX_AUDIT_MAX_BYTES) || MAX_BYTES;
const KEEP = 5;

function sessionId(req) {
  const m = /(?:^|;\s*)sbx_session=([^;]+)/.exec(String(req.headers.cookie || ''));
  if (!m) return 'anon';
  let raw = m[1];
  try { raw = decodeURIComponent(raw); } catch { /* keep raw */ }
  return crypto.createHash('sha256').update(raw).digest('hex').slice(0, 6);
}

function actor(who, req) {
  if (who && who.via === 'bearer' && who.label) return 'token:' + who.label;
  const ip = String((req.socket && req.socket.remoteAddress) || '-');
  const ua = String(req.headers['user-agent'] || '-').replace(/\s+/g, ' ').slice(0, 40);
  return `session:${sessionId(req)}|${ip}|${ua}`;
}

function rotate() {
  let size = 0;
  try { size = fs.statSync(LOG).size; } catch { return; } // no file yet: nothing to rotate
  if (size < limit()) return;
  for (let i = KEEP - 1; i >= 1; i--) {
    const from = `${LOG}.${i}`;
    const to = `${LOG}.${i + 1}`;
    if (fs.existsSync(from)) {
      try { fs.renameSync(from, to); } catch { /* best effort */ }
    }
  }
  try { fs.renameSync(LOG, `${LOG}.1`); } catch { /* best effort */ }
}

/**
 * Append one line. Throws when the line could not be written, so the caller can
 * decide whether that is fatal (it is for delete, which cannot be undone).
 */
function record({ who, req, method, path: target }) {
  const line = JSON.stringify({
    time: new Date().toISOString(),
    actor: actor(who, req),
    method,
    path: target,
    ip: String((req.socket && req.socket.remoteAddress) || '-'),
  });
  rotate();
  fs.appendFileSync(LOG, line + '\n', { mode: 0o600 });
  return line;
}

module.exports = { record, actor, LOG };
