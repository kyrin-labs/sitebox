'use strict';
/**
 * The HTTP transport is a second way in, so every gate the stdio path relies on has to
 * exist here too: a credential, one session per initialize, a body cap, an origin rule,
 * and not one byte on stdout. This file drives a real process over real HTTP against the
 * stub dashboard — a transport tested only in-process would not prove the wire format.
 */
const { test, before, after } = require('node:test');
const assert = require('node:assert');
const { spawn } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const ROOT = path.join(__dirname, '..');
const STUB = path.join(__dirname, 'stub-dashboard.js');
const MCP = path.join(ROOT, 'sitebox-mcp.js');
const TOKEN = 'sbx_http_test_token';

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'sitebox-http-test-'));
const SITES = path.join(tmp, 'sites');
fs.mkdirSync(SITES, { recursive: true });

function startStub() {
  const child = spawn('node', [STUB], {
    env: { ...process.env, STUB_SITES: SITES, STUB_FEATURES: 'health,port0,host,auth' },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  return new Promise((resolve, reject) => {
    let buf = '';
    child.stdout.on('data', (c) => {
      buf += c;
      const nl = buf.indexOf('\n');
      if (nl < 0) return;
      const info = JSON.parse(buf.slice(0, nl));
      resolve({ api: 'http://127.0.0.1:' + info.port, close: () => child.kill('SIGTERM') });
    });
    child.on('error', reject);
    setTimeout(() => reject(new Error('stub did not start')), 5000);
  });
}

function startMcp(apiBase) {
  const child = spawn('node', [MCP, '--http'], {
    env: {
      ...process.env,
      SITEBOX_API: apiBase,
      SITEBOX_API_TOKEN: TOKEN,
      SITEBOX_MCP_HTTP_PORT: '0',
      SITEBOX_MCP_HTTP_HOST: '127.0.0.1',
      SITEBOX_SITES: SITES,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const out = [];
  let stdout = '';
  child.stdout.on('data', (c) => { stdout += c; });
  child.stderr.on('data', (c) => out.push(String(c)));
  return new Promise((resolve, reject) => {
    const deadline = setTimeout(() => reject(new Error('mcp http did not listen: ' + out.join(''))), 8000);
    child.stderr.on('data', () => {
      const m = /http listening on (http:\/\/[^\s]+)/.exec(out.join(''));
      if (!m) return;
      clearTimeout(deadline);
      resolve({
        base: m[1].replace(/\/mcp$/, ''),
        stdout: () => stdout,
        stderr: () => out.join(''),
        close: () => child.kill('SIGTERM'),
      });
    });
    child.on('error', reject);
  });
}

async function post(base, body, opts) {
  const o = opts || {};
  const headers = { 'content-type': 'application/json', accept: 'application/json, text/event-stream' };
  if (o.token !== null) headers.authorization = 'Bearer ' + (o.token || TOKEN);
  if (o.session) headers['mcp-session-id'] = o.session;
  Object.assign(headers, o.headers || {});
  const res = await fetch(base + (o.path || '/mcp'), {
    method: o.method || 'POST',
    headers,
    body: o.method === 'DELETE' || o.method === 'GET' ? undefined
      : (typeof body === 'string' ? body : JSON.stringify(body)),
  });
  const text = await res.text();
  let json = null;
  try { json = text ? JSON.parse(text) : null; } catch { json = null; }
  return { status: res.status, headers: res.headers, json, text };
}

const initMsg = (id) => ({
  jsonrpc: '2.0', id: id || 1, method: 'initialize',
  params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'http-test', version: '1' } },
});

let stub;
let mcp;
let session;

before(async () => {
  stub = await startStub();
  mcp = await startMcp(stub.api);
  const init = await post(mcp.base, initMsg());
  assert.equal(init.status, 200);
  session = init.headers.get('mcp-session-id');
  assert.match(session, /^[0-9a-f-]{36}$/);
});

after(() => {
  if (mcp) mcp.close();
  if (stub) stub.close();
  fs.rmSync(tmp, { recursive: true, force: true });
});

test('health answers without a key and carries no site data', async () => {
  const res = await fetch(mcp.base + '/health');
  assert.equal(res.status, 200);
  const body = await res.json();
  assert.equal(body.ok, true);
  assert.equal(body.server, 'sitebox-mcp');
  assert.ok(!('sites' in body) && !('token' in body));
});

test('a request without a bearer token is refused before any tool runs', async () => {
  const r = await post(mcp.base, { jsonrpc: '2.0', id: 2, method: 'tools/list' }, { token: null, session });
  assert.equal(r.status, 401);
  assert.equal(r.json.error.code, -32001);
  assert.match(r.json.error.data.fix, /Authorization: Bearer/);
});

test('a wrong token is refused too', async () => {
  const r = await post(mcp.base, { jsonrpc: '2.0', id: 3, method: 'tools/list' }, { token: 'sbx_wrong', session });
  assert.equal(r.status, 401);
});

test('initialize opens a session and says who it is', async () => {
  const r = await post(mcp.base, initMsg(4));
  assert.equal(r.status, 200);
  assert.equal(r.json.result.serverInfo.name, 'sitebox');
  assert.equal(r.json.result.protocolVersion, '2025-06-18');
  assert.ok(r.headers.get('mcp-session-id'), 'the session id must be on the initialize response');
});

test('every later request needs the session id', async () => {
  const none = await post(mcp.base, { jsonrpc: '2.0', id: 5, method: 'tools/list' });
  assert.equal(none.status, 404);
  assert.equal(none.json.error.code, -32000);
  const bad = await post(mcp.base, { jsonrpc: '2.0', id: 6, method: 'tools/list' }, { session: 'not-a-session' });
  assert.equal(bad.status, 404);
});

test('tools/list over HTTP exposes the same eight tools as stdio', async () => {
  const r = await post(mcp.base, { jsonrpc: '2.0', id: 7, method: 'tools/list' }, { session });
  assert.equal(r.status, 200);
  const names = r.json.result.tools.map((t) => t.name).sort();
  assert.deepEqual(names, ['check_site', 'create_site', 'delete_site', 'doctor', 'list_sites', 'site_ctl', 'site_logs', 'update_site']);
});

test('ping and a notification answer 200 and 202 respectively', async () => {
  const ping = await post(mcp.base, { jsonrpc: '2.0', id: 8, method: 'ping' }, { session });
  assert.equal(ping.status, 200);
  assert.deepEqual(ping.json.result, {});
  const note = await post(mcp.base, { jsonrpc: '2.0', method: 'notifications/initialized' }, { session });
  assert.equal(note.status, 202);
  assert.equal(note.text, '');
});

test('a real tool call passes through to the dashboard', async () => {
  const r = await post(mcp.base, { jsonrpc: '2.0', id: 9, method: 'tools/call', params: { name: 'doctor', arguments: {} } }, { session });
  assert.equal(r.status, 200);
  const body = JSON.parse(r.json.result.content[0].text);
  assert.equal(body.ok, true, JSON.stringify(body.errors || body));
  assert.deepEqual(body.token_access, { scope: 'write', canRead: true, canWrite: true });
});

test('an unsupported MCP-Protocol-Version is refused, not ignored', async () => {
  const r = await post(mcp.base, { jsonrpc: '2.0', id: 10, method: 'ping' }, { session, headers: { 'mcp-protocol-version': '1999-01-01' } });
  assert.equal(r.status, 400);
  assert.match(r.json.error.message, /MCP-Protocol-Version/);
});

test('the body rules hold: parse error, batch, content type, size', async () => {
  const bad = await post(mcp.base, '{ nope', { session });
  assert.equal(bad.status, 400);
  assert.equal(bad.json.error.code, -32700);

  const batch = await post(mcp.base, [{ jsonrpc: '2.0', id: 11, method: 'ping' }], { session });
  assert.equal(batch.status, 400);
  assert.equal(batch.json.error.code, -32600);

  const ctype = await post(mcp.base, { jsonrpc: '2.0', id: 12, method: 'ping' }, { session, headers: { 'content-type': 'text/plain' } });
  assert.equal(ctype.status, 415);

  const big = await post(mcp.base, JSON.stringify({ jsonrpc: '2.0', id: 13, method: 'ping', params: { pad: 'a'.repeat(300 * 1024) } }), { session });
  assert.equal(big.status, 413);
});

test('a foreign Origin is refused; no Origin is fine', async () => {
  const evil = await post(mcp.base, { jsonrpc: '2.0', id: 14, method: 'ping' }, { session, headers: { origin: 'http://evil.example' } });
  assert.equal(evil.status, 403);
  assert.match(evil.json.error.message, /origin/i);

  const same = await post(mcp.base, { jsonrpc: '2.0', id: 15, method: 'ping' }, { session, headers: { origin: mcp.base } });
  assert.equal(same.status, 200);
});

test('GET has no stream, unknown paths are 404', async () => {
  const get = await fetch(mcp.base + '/mcp', { headers: { authorization: 'Bearer ' + TOKEN } });
  assert.equal(get.status, 405);
  assert.match(get.headers.get('allow') || '', /POST/);
  const other = await post(mcp.base, { jsonrpc: '2.0', id: 16, method: 'ping' }, { session, path: '/nope' });
  assert.equal(other.status, 404);
});

test('DELETE ends the session and the id stops working', async () => {
  const init = await post(mcp.base, initMsg(17));
  const sid = init.headers.get('mcp-session-id');
  const del = await fetch(mcp.base + '/mcp', {
    method: 'DELETE',
    headers: { authorization: 'Bearer ' + TOKEN, 'mcp-session-id': sid },
  });
  assert.equal(del.status, 204);
  const after = await post(mcp.base, { jsonrpc: '2.0', id: 18, method: 'ping' }, { session: sid });
  assert.equal(after.status, 404);
});

test('stdout stays empty: HTTP mode must never write protocol frames there', () => {
  assert.equal(mcp.stdout(), '');
});
