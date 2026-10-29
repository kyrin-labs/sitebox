'use strict';
/**
 * api.js — the only place the dashboard HTTP contract lives.
 * Status codes become actionable codes here, so no tool handler ever inspects a number.
 */
const fs = require('fs');
const os = require('os');
const path = require('path');
const http = require('http');

const BASE = (process.env.SITEBOX_API || 'http://127.0.0.1:4445').replace(/\/+$/, '');

function readToken() {
  if (process.env.SITEBOX_API_TOKEN) return process.env.SITEBOX_API_TOKEN.trim();
  const fallback = path.join(os.homedir(), '.config', 'sitebox', 'mcp.token');
  try {
    const t = fs.readFileSync(fallback, 'utf8').trim();
    return t || null;
  } catch {
    return null;
  }
}
const TOKEN = readToken();

class ApiFailure extends Error {
  constructor(code, message, extra = {}) {
    super(message);
    this.code = code;
    Object.assign(this, extra);
  }
}

function raw(method, pathname, body, timeout = 8000) {
  const url = new URL(BASE + pathname);
  const payload = body == null ? null : JSON.stringify(body);
  const headers = {};
  if (payload) {
    headers['content-type'] = 'application/json';
    headers['content-length'] = Buffer.byteLength(payload);
  }
  if (TOKEN) headers.authorization = 'Bearer ' + TOKEN;
  return new Promise((resolve, reject) => {
    const req = http.request(url, { method, headers, timeout }, (res) => {
      let data = '';
      res.on('data', (c) => (data += c));
      res.on('end', () => {
        let json = null;
        try { json = data ? JSON.parse(data) : null; } catch { json = { raw: data }; }
        resolve({ status: res.statusCode, json });
      });
    });
    req.on('timeout', () => { req.destroy(); reject(new ApiFailure('E_UNREACHABLE', 'dashboard did not answer within ' + timeout + 'ms')); });
    req.on('error', (e) => reject(new ApiFailure('E_UNREACHABLE', 'dashboard unreachable at ' + BASE + ' (' + e.code + ')', { cause: e.code })));
    if (payload) req.write(payload);
    req.end();
  });
}

function map(status, json, pathname) {
  if (status >= 200 && status < 300) return json;
  const msg = (json && (json.error || json.message)) || ('HTTP ' + status);
  if (status === 401) throw new ApiFailure('E_AUTH', 'dashboard rejected the token', { fix: 'a client needs a Bearer token: mint one with `node dashboard/token.js add mcp --scope write --print` and put it in SITEBOX_API_TOKEN or ~/.config/sitebox/mcp.token (600). The dashboard UI opens without a token by design — that browser cookie does not authenticate an agent.' });
  if (status === 507) throw new ApiFailure('E_BAND_FULL', msg, { fix: 'the port band is full: this is an ops verdict, not something to retry — run doctor, then move or delete a site' });
  if (status === 409) throw new ApiFailure('E_PORT_TAKEN', msg, { retryable: true, fix: 'register again with port 0' });
  if (status === 404) throw new ApiFailure('E_NOT_FOUND', msg, { fix: 'the site id is not registered on this dashboard — list_sites to see what exists' });
  if (status === 400) throw new ApiFailure('E_BAD_INPUT', msg + ' (' + pathname + ')', { fix: 'the dashboard rejected the payload; the message above is the server-side rule' });
  throw new ApiFailure('E_SERVER', msg, { status });
}

async function call(method, pathname, body) {
  const { status, json } = await raw(method, pathname, body);
  return map(status, json, pathname);
}

const enc = encodeURIComponent;

module.exports = {
  BASE,
  hasToken: () => Boolean(TOKEN),
  token: () => TOKEN,
  health: () => call('GET', '/api/health'),
  me: () => call('GET', '/api/me'),
  sites: () => call('GET', '/api/sites'),
  ports: (port) => call('GET', '/api/ports/check?port=' + port),
  register: (payload) => call('POST', '/api/sites', payload),
  start: (id) => call('POST', '/api/sites/' + enc(id) + '/start'),
  stop: (id) => call('POST', '/api/sites/' + enc(id) + '/stop'),
  restart: (id) => call('POST', '/api/sites/' + enc(id) + '/restart'),
  logs: (id, lines) => call('GET', '/api/sites/' + enc(id) + '/logs?lines=' + (lines || 80)),
  remove: (id) => call('DELETE', '/api/sites/' + enc(id)),
  get: (url, timeout = 2500) => new Promise((resolve) => {
    const req = http.get(url, { timeout }, (res) => {
      let body = '';
      res.on('data', (c) => { if (body.length < 8192) body += c; });
      res.on('end', () => resolve({ status: res.statusCode, body }));
    });
    req.on('error', () => resolve({ status: 0, body: '' }));
    req.on('timeout', () => { req.destroy(); resolve({ status: 0, body: '' }); });
  }),
  ApiFailure,
};
