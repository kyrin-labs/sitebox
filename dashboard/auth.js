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
    const t = store.findByLabel(s.label);
    if (!t) return { ok: false, reason: 'revoked' };
    return { ok: true, via: 'cookie', label: t.label, scope: t.scope };
  }
  return { ok: false, reason: 'none' };
}

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

function parseBody(req) {
  return new Promise((resolve) => {
    let d = '';
    req.on('data', (c) => (d += c));
    req.on('end', () => { try { resolve(JSON.parse(d)); } catch { resolve(null); } });
  });
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
      send(res, 429, { error: 'too many attempts', hint: 'wait a minute' });
      return true;
    }
    const body = await parseBody(req);
    const t = body && body.token ? store.verify(String(body.token)) : null;
    if (!t) {
      send(res, 401, { error: 'unauthorized' });
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
    send(res, 401, { error: 'unauthorized' });
    return true;
  }

  const unsafe = !['GET', 'HEAD', 'OPTIONS'].includes(method);
  if (who.via === 'cookie' && unsafe && !sameOrigin(req)) {
    send(res, 403, { error: 'origin not allowed' });
    return true;
  }
  if (req.headers['sec-fetch-site'] === 'cross-site') {
    send(res, 403, { error: 'cross-site request refused' });
    return true;
  }

  // token administration is a write surface of its own
  if (pathname === '/api/tokens' || pathname.startsWith('/api/tokens/')) {
    if (who.scope !== 'write') {
      send(res, 403, { error: 'this token is read-only' });
      return true;
    }
    if (pathname === '/api/tokens' && method === 'GET') {
      send(res, 200, { tokens: store.summary() || [] });
      return true;
    }
    if (pathname === '/api/tokens' && method === 'POST') {
      const body = await parseBody(req);
      try {
        const raw = store.createToken(body && body.label, (body && body.scope) || 'write');
        // The key is printed exactly once; only its hash is stored.
        send(res, 200, { ok: true, label: body.label, scope: body.scope || 'write', key: raw, note: 'copy it now — it is not stored and cannot be shown again' });
      } catch (e) {
        send(res, 400, { error: e.message });
      }
      return true;
    }
    const m2 = pathname.match(/^\/api\/tokens\/([^/]+)$/);
    if (m2 && method === 'DELETE') {
      const r = store.revokeToken(decodeURIComponent(m2[1]));
      send(res, r.removed ? 200 : 404, { ok: Boolean(r.removed), label: decodeURIComponent(m2[1]), sessionsDropped: r.sessions });
      return true;
    }
    send(res, 404, { error: 'not found' });
    return true;
  }

  if (unsafe && who.scope !== 'write') {
    send(res, 403, { error: 'this token is read-only', hint: 'mint a write-scoped token for this action' });
    return true;
  }
  return false;
}

module.exports = { guard, authenticate, sameOrigin };
