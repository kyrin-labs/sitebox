'use strict';
/**
 * http-transport — the streamable-HTTP side of the MCP server (spec 2025-06-18).
 *
 * One endpoint, `POST /mcp`, answering `application/json`. The transport never pushes a
 * server-initiated message (this server has no subscriptions), so an SSE stream would be
 * a keep-alive with nothing to send; the spec allows a JSON response for every request.
 *
 * Sessions are required after `initialize`, exactly as the spec says. That is not
 * ceremony: without it, any request that got past the network could spend a dashboard
 * call, and a shared URL would look like a shared identity.
 *
 * The credential is the same one the stdio server already uses (SITEBOX_API_TOKEN or
 * ~/.config/sitebox/mcp.token): one process, one identity. A request that does not
 * present it is refused here; a revoked key still fails on the next tool call because
 * the dashboard stays the authority.
 */
const http = require('http');
const crypto = require('crypto');

const MAX_BODY = 256 * 1024;
const SESSION_TTL_MS = 30 * 60 * 1000;
const MAX_SESSIONS = 64;

const sha256 = (s) => crypto.createHash('sha256').update(String(s), 'utf8').digest();

/** Constant-time, length-safe: comparing digests keeps the sizes equal. */
function tokenMatches(presented, expected) {
  if (typeof presented !== 'string' || !presented || !expected) return false;
  return crypto.timingSafeEqual(sha256(presented), sha256(expected));
}

function bearer(req) {
  const m = /^Bearer\s+(.+)$/i.exec(req.headers.authorization || '');
  return m ? m[1].trim() : null;
}

/**
 * A bearer header cannot be attached by a cross-origin page without a preflight we never
 * answer, so this is defence in depth against DNS rebinding rather than CSRF. A client
 * that sends no Origin (every CLI and MCP client) is fine; a browser that does must name
 * the host it actually reached.
 */
function originMatchesHost(req) {
  const origin = req.headers.origin;
  if (!origin) return true;
  try {
    return new URL(origin).host === (req.headers.host || '');
  } catch {
    return false;
  }
}

function readBody(req) {
  return new Promise((resolve) => {
    let size = 0;
    const chunks = [];
    let settled = false;
    const finish = (value) => { if (!settled) { settled = true; resolve(value); } };
    req.on('data', (c) => {
      size += c.length;
      if (size > MAX_BODY) {
        finish({ tooLarge: true });
        req.pause(); // let the handler answer 413 before the socket closes
        return;
      }
      chunks.push(c);
    });
    req.on('end', () => finish({ body: Buffer.concat(chunks).toString('utf8') }));
    req.on('error', () => finish({ body: null }));
  });
}

function json(res, status, body, headers) {
  res.writeHead(status, Object.assign({
    'content-type': 'application/json; charset=utf-8',
    'cache-control': 'no-store',
  }, headers || {}));
  res.end(body === undefined ? '' : JSON.stringify(body));
}

const rpcError = (res, status, code, message, fix, headers) => json(res, status, {
  jsonrpc: '2.0',
  id: null,
  error: { code, message, ...(fix ? { data: { fix } } : {}) },
}, headers);

/**
 * @param {object} opts
 * @param {(msg: object) => Promise<object|null>} opts.dispatch  one shared dispatcher
 * @param {string} opts.token     the credential this process acts with (never logged)
 * @param {string[]} opts.protocolVersions
 * @param {(...a: any[]) => void} opts.log
 */
function createHttpTransport(opts) {
  const dispatch = opts.dispatch;
  const token = opts.token;
  const supported = opts.protocolVersions || [];
  const log = opts.log || (() => {});

  /** @type {Map<string, {lastSeen: number}>} */
  const sessions = new Map();

  function sweep() {
    const now = Date.now();
    for (const [id, s] of sessions) if (now - s.lastSeen > SESSION_TTL_MS) sessions.delete(id);
  }

  const server = http.createServer(async (req, res) => {
    const pathname = String(req.url || '').split('?')[0];

    // Probes must not need a key: this route answers no site data at all.
    if (pathname === '/health' && req.method === 'GET') {
      return json(res, 200, { ok: true, server: 'sitebox-mcp', transport: 'http' });
    }
    if (pathname !== '/mcp') {
      return rpcError(res, 404, -32000, 'not found', 'the MCP endpoint is /mcp');
    }
    if (!originMatchesHost(req)) {
      return rpcError(res, 403, -32000, 'origin not allowed', 'send no Origin header, or one that matches the Host you reached');
    }
    if (req.method === 'OPTIONS') {
      res.writeHead(204, { allow: 'POST, DELETE', 'cache-control': 'no-store' });
      return res.end();
    }
    if (req.method === 'GET') {
      return rpcError(res, 405, -32000, 'no server-initiated stream', 'this server answers every request with application/json; use POST /mcp', { allow: 'POST, DELETE' });
    }
    if (!tokenMatches(bearer(req), token)) {
      return rpcError(res, 401, -32001, 'unauthorized', 'send Authorization: Bearer <sbx_…> — the same key the stdio server uses', { 'www-authenticate': 'Bearer realm="sitebox-mcp"' });
    }

    sweep();

    if (req.method === 'DELETE') {
      const sid = String(req.headers['mcp-session-id'] || '');
      if (!sid || !sessions.has(sid)) {
        return rpcError(res, 404, -32000, 'unknown or missing Mcp-Session-Id', 'initialize first; the session id is returned in the initialize response');
      }
      sessions.delete(sid);
      res.writeHead(204, { 'cache-control': 'no-store' });
      return res.end();
    }
    if (req.method !== 'POST') {
      return rpcError(res, 405, -32000, 'method not allowed', 'use POST /mcp', { allow: 'POST, DELETE' });
    }

    const ctype = String(req.headers['content-type'] || '').split(';')[0].trim().toLowerCase();
    if (ctype !== 'application/json') {
      return rpcError(res, 415, -32000, 'content-type must be application/json', 'one JSON-RPC object per request');
    }
    const declared = Number(req.headers['content-length']);
    if (Number.isFinite(declared) && declared > MAX_BODY) {
      req.resume(); // discard, never buffer: the cap protects memory, not bandwidth
      return rpcError(res, 413, -32000, 'request body too large', 'at most ' + MAX_BODY + ' bytes');
    }

    const read = await readBody(req);
    if (read.tooLarge) {
      req.resume();
      return rpcError(res, 413, -32000, 'request body too large', 'at most ' + MAX_BODY + ' bytes');
    }
    if (read.body == null) return rpcError(res, 400, -32700, 'unreadable body', 'send one JSON-RPC object');

    let msg;
    try {
      msg = JSON.parse(read.body);
    } catch {
      return rpcError(res, 400, -32700, 'parse error', 'the body must be one JSON-RPC object');
    }
    if (Array.isArray(msg)) {
      return rpcError(res, 400, -32600, 'batch requests are not supported', 'send one JSON-RPC object per request');
    }
    if (!msg || typeof msg !== 'object' || msg.jsonrpc !== '2.0' || typeof msg.method !== 'string') {
      return rpcError(res, 400, -32600, 'invalid JSON-RPC request', 'jsonrpc must be "2.0" and method a string');
    }

    const askedVersion = req.headers['mcp-protocol-version'];
    if (askedVersion && !supported.includes(String(askedVersion))) {
      return rpcError(res, 400, -32000, 'unsupported MCP-Protocol-Version: ' + askedVersion,
        'supported: ' + supported.join(', '));
    }

    // initialize is the one request that may arrive without a session; it creates one.
    if (msg.method === 'initialize') {
      if (sessions.size >= MAX_SESSIONS) {
        const oldest = [...sessions.entries()].sort((a, b) => a[1].lastSeen - b[1].lastSeen)[0];
        if (oldest) sessions.delete(oldest[0]);
      }
      const sid = crypto.randomUUID();
      sessions.set(sid, { lastSeen: Date.now() });
      const response = await dispatch(msg);
      log('http session opened');
      return json(res, 200, response || null, { 'mcp-session-id': sid });
    }

    const sid = String(req.headers['mcp-session-id'] || '');
    const session = sid ? sessions.get(sid) : null;
    if (!session) {
      return rpcError(res, 404, -32000, 'unknown or missing Mcp-Session-Id',
        'initialize first; the session id is returned in the initialize response and required on every later request');
    }
    session.lastSeen = Date.now();

    const response = await dispatch(msg);
    if (!response) {
      res.writeHead(202, { 'cache-control': 'no-store' });
      return res.end();
    }
    return json(res, 200, response);
  });

  // Nothing here streams, so a slow client must not hold a socket open forever.
  server.headersTimeout = 15 * 1000;
  server.requestTimeout = 120 * 1000;
  return server;
}

module.exports = { createHttpTransport, MAX_BODY, SESSION_TTL_MS, MAX_SESSIONS, tokenMatches, originMatchesHost };
