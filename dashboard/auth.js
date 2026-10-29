'use strict';
/**
 * auth — mechanism only, no policy. `guard()` answers one question: may this request
 * reach a route that reads or writes state? Scope checks per route live in server.js.
 *
 * Accepted credentials: `Authorization: Bearer sbx_…` (clients) or the `sbx_session`
 * cookie minted by POST /api/session (browser UI, because <img> cannot send headers).
 *
 * Origin rules differ by mechanism on purpose:
 *   - Bearer: Origin is irrelevant — a header is never attached by a browser form.
 *   - Cookie + unsafe method: Origin is required and must match the request host.
 *   - Token-less requests are 401 before any of this matters.
 */
const store = require('./tokenstore');
const audit = require('./audit');

const LIMIT = { windowMs: 60 * 1000, max: 10 };
let attempts = { start: Date.now(), count: 0 };

function bearer(req) {
  const m = /^Bearer\s+(.+)$/i.exec(req.headers.authorization || '');
  return m ? m[1].trim() : null;
}

function cookie(req, name) {
  for (const part of String(req.headers.cookie || '').split(';')) {
    const i = part.indexOf('=');
    if (i < 0) continue;
    if (part.slice(0, i).trim() === name) {
      try { return decodeURIComponent(part.slice(i + 1).trim()); } catch { return null; }
    }
  }
  return null;
}

function authenticate(req) {
  const raw = bearer(req);
  if (raw) {
    const t = store.verify(raw);
    return t ? { ok: true, via: 'bearer', label: t.label, scope: t.scope } : { ok: false, reason: 'bad_token' };
  }
  const secret = cookie(req, store.COOKIE);
  if (secret) {
    const s = store.getSession(secret);
    if (!s) return { ok: false, reason: 'bad_session' };
    // Browser UI sessions are not tied to a token: the dashboard is the same origin
    // and the operator is a person, not a client. Revoking an MCP key must never log
    // the person out of their own dashboard.
    // A UI session is not a bearer token and must not claim a token scope: it can
    // only do what the loopback rule allows, which is not always 'write'.
    if (s.ui) return { ok: true, via: 'cookie', label: 'ui', scope: 'session', ui: true };
    const t = store.findByLabel(s.label);
    if (!t) return { ok: false, reason: 'revoked' };
    return { ok: true, via: 'cookie', label: t.label, scope: t.scope };
  }
  return { ok: false, reason: 'none' };
}

/**
 * Hand the dashboard page a session on first load. No token, no prompt: the UI is the
 * thing the operator already has access to. Only programmatic callers (MCP, curl) have
 * to present a bearer token, which is the whole point of separating the two.
 */
function issueUiSession(req, res) {
  if (cookie(req, store.COOKIE)) return false;
  const secret = store.createUiSession();
  res.setHeader('Set-Cookie', store.COOKIE + '=' + encodeURIComponent(secret)
    + '; HttpOnly; SameSite=Strict; Path=/; Max-Age=' + Math.floor(store.SESSION_TTL_MS / 1000));
  return true;
}

// Never read X-Forwarded-For: any client can write "127.0.0.1" into it. The socket
// address is the only one the kernel wrote. Node reports IPv4 peers of a 0.0.0.0
// listener as "::ffff:127.0.0.1", so the v4-mapped form has to be normalised or the
// operator cannot create a site from a browser on the machine itself.
function isLoopback(req) {
  const raw = String((req.socket && req.socket.remoteAddress) || '');
  const ip = raw.startsWith('::ffff:') ? raw.slice(7) : raw;
  return ip === '127.0.0.1' || ip === '::1';
}

// The only writes a LAN visitor may perform: start and stop. Everything that creates,
// deletes or reorders is local-only (or needs a write-scoped Bearer token).

function sameOrigin(req) {
  const origin = req.headers.origin;
  if (!origin) return false; // cookie + write + no Origin is refused, not trusted
  const host = req.headers.host || '';
  try {
    const u = new URL(origin);
    return u.host === host;
  } catch {
    return false;
  }
}

const MAX_BODY = 64 * 1024;
function parseBody(req, res, sendFn) {
  return new Promise((resolve) => {
    let d = '';
    let size = 0;
    let settled = false;
    const finish = (v) => { if (!settled) { settled = true; resolve(v); } };
    req.on('data', (c) => {
      size += c.length;
      if (size > MAX_BODY) {
        if (res) deny(sendFn, res, req, req.method, String(req.url || '').split('?')[0], 413, { error: 'request body too large', hint: `at most ${MAX_BODY} bytes` }, 'anon');
        req.destroy();
        return finish(null);
      }
      d += c;
    });
    req.on('end', () => { try { finish(JSON.parse(d)); } catch { finish(null); } });
    req.on('error', () => finish(null));
  });
}

/**
 * Refuse, and leave a trace. A failure to write the audit line must never mask the
 * refusal itself, so the log attempt is best-effort here and the refusal always wins.
 */
function deny(send, res, req, method, pathname, status, payload, actor) {
  try { audit.denied({ req, method, path: pathname, status, actor }); } catch { /* never mask the refusal */ }
  send(res, status, payload);
}

/**
 * Returns true when the request was fully handled here (allowed-through endpoints or
 * a rejection), false when the caller should continue routing.
 */
async function guard(req, res, send, url, method, pathname) {
  // login: the token is in the body, so this is the one route that cannot require a cookie
  if (pathname === '/api/session' && method === 'POST') {
    const now = Date.now();
    if (now - attempts.start > LIMIT.windowMs) attempts = { start: now, count: 0 };
    if (++attempts.count > LIMIT.max) {
      deny(send, res, req, method, pathname, 429, { error: 'too many attempts', hint: 'wait a minute' }, 'denied:rate');
      return true;
    }
    const body = await parseBody(req, res, send);
    const t = body && body.token ? store.verify(String(body.token)) : null;
    if (!t) {
      deny(send, res, req, method, pathname, 401, { error: 'unauthorized' }, 'anon');
      return true;
    }
    const secret = store.createSession(t.label);
    const maxAge = Math.floor(store.SESSION_TTL_MS / 1000);
    res.setHeader('Set-Cookie', store.COOKIE + '=' + encodeURIComponent(secret)
      + '; HttpOnly; SameSite=Strict; Path=/; Max-Age=' + maxAge);
    send(res, 200, { ok: true, label: t.label, scope: t.scope, expiresIn: maxAge });
    return true;
  }

  if (pathname === '/api/session' && method === 'DELETE') {
    // explicit logout: drop the cookie. Session rows expire on their own.
    res.setHeader('Set-Cookie', store.COOKIE + '=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0');
    send(res, 200, { ok: true });
    return true;
  }

  const who = authenticate(req);
  if (!who.ok) {
    deny(send, res, req, method, pathname, 401, { error: 'unauthorized' }, 'anon');
    return true;
  }

  const unsafe = !['GET', 'HEAD', 'OPTIONS'].includes(method);
  if (who.via === 'cookie' && unsafe && !sameOrigin(req)) {
    deny(send, res, req, method, pathname, 403, { error: 'origin not allowed' }, 'denied:origin');
    return true;
  }
  if (req.headers['sec-fetch-site'] === 'cross-site') {
    deny(send, res, req, method, pathname, 403, { error: 'cross-site request refused' }, 'denied:cross-site');
    return true;
  }

  // token administration is a write surface of its own — and the one that would undo
  // every other guard: a cookie minted by anyone on the LAN must not be able to issue
  // itself a write token.
  if (pathname === '/api/tokens' || pathname.startsWith('/api/tokens/')) {
    // The keys pane is part of the dashboard, and the dashboard is open to the LAN by
    // decision: any device that can load it may manage keys. What this no longer does
    // is gate LAN writes — see the note in the route table below.
    const mayAdmin = Boolean(who.ui) || isLoopback(req) || (who.via === 'bearer' && who.scope === 'write');
    if (!mayAdmin) {
      deny(send, res, req, method, pathname, 403, {
        error: 'token administration needs a dashboard session or a write-scoped token',
        hint: 'open the dashboard and use Keys, run node dashboard/token.js … on the host, or present a write-scoped Bearer token',
      }, 'denied:admin');
      return true;
    }
    // Only a bearer token can be read-only; a dashboard session is not a token.
    if (who.scope !== 'write' && !who.ui) {
      deny(send, res, req, method, pathname, 403, { error: 'this token is read-only', hint: 'manage keys with a write-scoped token — write includes read' }, 'denied:scope');
      return true;
    }
    if (pathname === '/api/tokens' && method === 'GET') {
      send(res, 200, { tokens: store.summary() || [] });
      return true;
    }
    if (pathname === '/api/tokens' && method === 'POST') {
      const body = await parseBody(req, res, send);
      audit.recordOnFinish(res, { who, req, method, path: pathname });
      try {
        // One scope string, validated by the store: a write key is read + write, so
        // there is no second value to accept. Echo the scope that was stored, not the
        // one the request happened to send.
        const scope = (body && body.scope) || 'write';
        const raw = store.createToken(body && body.label, scope);
        // The key is printed exactly once; only its hash is stored.
        send(res, 200, { ok: true, label: body.label, scope, key: raw, note: 'copy it now — it is not stored and cannot be shown again' });
      } catch (e) {
        send(res, 400, { error: e.message });
      }
      return true;
    }
    const m2 = pathname.match(/^\/api\/tokens\/([^/]+)$/);
    if (m2 && method === 'DELETE') {
      audit.recordOnFinish(res, { who, req, method, path: pathname });
      const r = store.revokeToken(decodeURIComponent(m2[1]));
      send(res, r.removed ? 200 : 404, { ok: Boolean(r.removed), label: decodeURIComponent(m2[1]), sessionsDropped: r.sessions });
      return true;
    }
    send(res, 404, { error: 'not found' });
    return true;
  }

  // Read-only window on the audit log: the answer to "who touched this" without SSH.
  // Any authenticated caller may read it — it is the same information they can cause.
  if (pathname === '/api/audit' && method === 'GET') {
    const asked = Number(new URLSearchParams(url.search).get('lines'));
    send(res, 200, { lines: audit.tail(asked || 5) });
    return true;
  }

  if (unsafe && who.scope !== 'write' && !who.ui) {
    deny(send, res, req, method, pathname, 403, { error: 'this token is read-only', hint: 'mint a write-scoped token for this action — write includes read, so one key is enough for an agent' }, 'denied:scope');
    return true;
  }

  // The network boundary is the gate now: any authenticated dashboard request may
  // write. What is left is what actually protects us — Origin on cookie writes (CSRF,
  // above) and the record this branch guarantees.
  if (unsafe) {
    try {
      // Delete cannot be undone, so the log must be provably writable before it happens.
      // For everything else the line is written with the real status once the route answers.
      if (method === 'DELETE') audit.assertWritable();
      audit.recordOnFinish(res, { who, req, method, path: pathname });
    } catch (e) {
      if (method === 'DELETE') {
        send(res, 503, {
          error: 'audit log is not writable',
          hint: 'refusing to delete without a record: fix dashboard/data ownership (chown 1000:1000) and retry',
        });
        return true;
      }
    }
  }
  return false;
}

module.exports = { guard, authenticate, sameOrigin, isLoopback, issueUiSession };
