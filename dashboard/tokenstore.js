'use strict';
/**
 * tokenstore — the only code that touches tokens.json / sessions.json.
 * Files are 0600, written atomically (tmp + rename), and read through a stat cache
 * keyed on mtime+size so a mint/revoke from the host takes effect on the very next
 * request without restarting the container.
 *
 * Fail closed: a missing, empty or unparseable store means "no valid token", never
 * "no auth required".
 */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const DATA = process.env.SITEBOX_DATA || path.join(__dirname, 'data');
const TOKENS = path.join(DATA, 'tokens.json');
const SESSIONS = path.join(DATA, 'sessions.json');

const PREFIX = 'sbx_';
const COOKIE = 'sbx_session';
const SESSION_TTL_MS = 12 * 60 * 60 * 1000;
const SESSION_MAX_MS = 7 * 24 * 60 * 60 * 1000;
const TOUCH_MS = 60 * 1000;

const sha256 = (s) => crypto.createHash('sha256').update(String(s), 'utf8').digest();
const hex = (s) => sha256(s).toString('hex');

const cache = new Map();
function readCached(file) {
  let st;
  try { st = fs.statSync(file); } catch { return null; }
  const hit = cache.get(file);
  if (hit && hit.mtimeMs === st.mtimeMs && hit.size === st.size) return hit.value;
  let value = null;
  try {
    value = JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    value = null; // unreadable or broken JSON: fail closed
  }
  cache.set(file, { mtimeMs: st.mtimeMs, size: st.size, value });
  return value;
}

function atomicWrite(file, obj) {
  fs.mkdirSync(DATA, { recursive: true });
  const tmp = file + '.' + process.pid + '.tmp';
  fs.writeFileSync(tmp, JSON.stringify(obj, null, 2) + '\n', { mode: 0o600 });
  fs.chmodSync(tmp, 0o600);
  fs.renameSync(tmp, file);
  cache.delete(file);
}

function listTokens() {
  const parsed = readCached(TOKENS);
  if (!parsed || !Array.isArray(parsed.tokens)) return null;
  return parsed.tokens;
}

function createToken(label, scope) {
  if (!/^[a-z0-9][a-z0-9-]{1,31}$/.test(String(label || ''))) {
    throw new Error('label must be kebab-case, 2-32 chars (e.g. mcp, ui)');
  }
  if (!['read', 'write'].includes(scope)) throw new Error('scope must be read or write');
  const tokens = listTokens() || [];
  if (tokens.some((t) => t.label === label)) {
    throw new Error('a token labelled "' + label + '" already exists — revoke it first');
  }
  const raw = PREFIX + crypto.randomBytes(24).toString('base64url');
  tokens.push({ label, hash: hex(raw), scope, created: new Date().toISOString(), lastUsed: null });
  atomicWrite(TOKENS, { tokens });
  return raw;
}

function revokeToken(label) {
  const tokens = listTokens();
  if (!tokens || !tokens.some((t) => t.label === label)) return { removed: 0, sessions: 0 };
  atomicWrite(TOKENS, { tokens: tokens.filter((t) => t.label !== label) });
  const sessions = revokeSessionsByLabel(label);
  return { removed: 1, sessions };
}

function summary() {
  const tokens = listTokens();
  if (!tokens) return null;
  return tokens.map((t) => ({ label: t.label, scope: t.scope, created: t.created, lastUsed: t.lastUsed }));
}

/** Constant-time verification against every stored hash. */
function verify(raw) {
  const tokens = listTokens();
  if (!tokens || !tokens.length || typeof raw !== 'string' || !raw) return null;
  const digest = sha256(raw);
  let found = null;
  for (const t of tokens) {
    const stored = Buffer.from(String(t.hash || ''), 'hex');
    if (stored.length !== digest.length) continue;
    if (crypto.timingSafeEqual(stored, digest)) found = t;
  }
  if (found) touch(found.label);
  return found ? { label: found.label, scope: found.scope } : null;
}

function touch(label) {
  const tokens = listTokens();
  if (!tokens) return;
  const t = tokens.find((x) => x.label === label);
  if (!t) return;
  const last = t.lastUsed ? Date.parse(t.lastUsed) : 0;
  if (Date.now() - last < TOUCH_MS) return; // do not rewrite the file on every request
  t.lastUsed = new Date().toISOString();
  try { atomicWrite(TOKENS, { tokens }); } catch { /* a read-only moment must not break auth */ }
}

function findByLabel(label) {
  const tokens = listTokens();
  if (!tokens) return null;
  return tokens.find((t) => t.label === label) || null;
}

/* ── sessions ── */
function readSessions() {
  const parsed = readCached(SESSIONS);
  if (!parsed || !Array.isArray(parsed.sessions)) return [];
  return parsed.sessions;
}

function createSession(label) {
  const secret = crypto.randomBytes(32).toString('base64url');
  const now = Date.now();
  const sessions = readSessions().filter((s) => now - Date.parse(s.lastSeen || s.created) < SESSION_TTL_MS);
  sessions.push({ id: hex(secret), label, created: new Date(now).toISOString(), lastSeen: new Date(now).toISOString() });
  atomicWrite(SESSIONS, { sessions });
  return secret;
}

function getSession(secret) {
  if (typeof secret !== 'string' || !secret) return null;
  const id = hex(secret);
  const now = Date.now();
  const s = readSessions().find((x) => x.id === id);
  if (!s) return null;
  const born = Date.parse(s.created);
  const seen = Date.parse(s.lastSeen || s.created);
  if (now - born > SESSION_MAX_MS || now - seen > SESSION_TTL_MS) return null;
  if (now - seen > TOUCH_MS) {
    const all = readSessions();
    const row = all.find((x) => x.id === id);
    if (row) {
      row.lastSeen = new Date(now).toISOString();
      try { atomicWrite(SESSIONS, { sessions: all }); } catch { /* ignore */ }
    }
  }
  return s;
}

function revokeSessionsByLabel(label) {
  const all = readSessions();
  const kept = all.filter((s) => s.label !== label);
  const removed = all.length - kept.length;
  if (removed) atomicWrite(SESSIONS, { sessions: kept });
  return removed;
}

module.exports = {
  DATA, TOKENS, SESSIONS, PREFIX, COOKIE, SESSION_TTL_MS, SESSION_MAX_MS,
  listTokens, summary, createToken, revokeToken, verify, findByLabel,
  createSession, getSession, revokeSessionsByLabel, hex,
};
