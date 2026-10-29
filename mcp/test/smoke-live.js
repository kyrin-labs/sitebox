#!/usr/bin/env node
'use strict';
/**
 * smoke-live — the gate that runs against a REAL dashboard (default 4445).
 *
 *   SITEBOX_API_TOKEN=… node mcp/test/smoke-live.js [id]
 *
 * Proves in one run, on production:
 *   - a prototype can be created, served, verified, restarted, checked, logged and purged
 *   - a loopback-bound site is NOT reachable from the LAN, before and after a restart
 *   - revoking a token makes the very next request 401 (no in-memory cache window)
 *   - the token store holds hashes only
 *
 * Exit 0 only when every gate passes. A gate that needs something this host lacks
 * (the token CLI, a non-loopback interface) reports SKIP with the reason; it never
 * silently passes.
 */
const { spawn, execFileSync } = require('child_process');
const http = require('http');
const fs = require('fs');
const os = require('os');
const path = require('path');

const API = process.env.SITEBOX_API || 'http://127.0.0.1:4445';
const SITES = process.env.SITEBOX_SITES || path.join(process.env.HOME || '', 'sitebox', 'sites');
const DATA = process.env.SITEBOX_DATA || path.join(process.env.HOME || '', 'sitebox', 'dashboard', 'data');
const CLI = process.env.SITEBOX_TOKEN_CLI || path.join(process.env.HOME || '', 'sitebox', 'dashboard', 'token.js');
const MCP = path.join(__dirname, '..', 'sitebox-mcp.js');
const ID = process.argv[2] || 'smoke-mcp';

const results = [];
let failed = 0;
const gate = (name, pass, detail) => {
  results.push((pass ? 'PASS ' : 'FAIL ') + name + (detail ? '  [' + detail + ']' : ''));
  if (!pass) failed++;
};
const skip = (name, why) => results.push('SKIP ' + name + '  [' + why + ']');

const lanAddress = () => {
  for (const list of Object.values(os.networkInterfaces())) {
    for (const a of list || []) {
      if (a.family === 'IPv4' && !a.internal) return a.address;
    }
  }
  return null;
};

const status = (host, port) => new Promise((resolve) => {
  const req = http.get({ host, port, path: '/', timeout: 1500 }, (res) => { res.resume(); resolve(res.statusCode); });
  req.on('error', () => resolve(0));
  req.on('timeout', () => { req.destroy(); resolve(0); });
});

const api = (method, pathname, token) => new Promise((resolve) => {
  const headers = {};
  if (token) headers.authorization = 'Bearer ' + token;
  const req = http.request(new URL(API + pathname), { method, headers, timeout: 4000 }, (res) => {
    let d = '';
    res.on('data', (c) => (d += c));
    res.on('end', () => { let j = null; try { j = JSON.parse(d); } catch { j = null; } resolve({ status: res.statusCode, json: j }); });
  });
  req.on('error', () => resolve({ status: 0, json: null }));
  req.on('timeout', () => { req.destroy(); resolve({ status: 0, json: null }); });
  req.end();
});

/* Raw request with optional cookie/Origin, addressed at an explicit base URL so the LAN
 * gates really come from the network and not from loopback. */
const raw = (method, pathname, opts = {}) => new Promise((resolve) => {
  const payload = opts.body ? JSON.stringify(opts.body) : null;
  const headers = {};
  if (payload) { headers['content-type'] = 'application/json'; headers['content-length'] = Buffer.byteLength(payload); }
  if (opts.cookie) headers.cookie = opts.cookie;
  if (opts.origin) headers.origin = opts.origin;
  const req = http.request(new URL(pathname, opts.base || API), { method, headers, timeout: 5000 }, (res) => {
    let d = '';
    res.on('data', (c) => (d += c));
    res.on('end', () => {
      let j = null;
      try { j = JSON.parse(d); } catch { j = null; }
      resolve({ status: res.statusCode, json: j, setCookie: res.headers['set-cookie'] || [] });
    });
  });
  req.on('error', () => resolve({ status: 0, json: null, setCookie: [] }));
  req.on('timeout', () => { req.destroy(); resolve({ status: 0, json: null, setCookie: [] }); });
  if (payload) req.write(payload);
  req.end();
});

/** Open the dashboard page as a network visitor would and keep the session it hands out. */
async function uiCookieFrom(lan) {
  const res = await raw('GET', '/', { base: 'http://' + lan + ':4445' });
  const first = (res.setCookie || [])[0];
  return first ? first.split(';')[0] : null;
}

function connect() {
  const child = spawn('node', [MCP], {
    env: { ...process.env, SITEBOX_API: API, SITEBOX_SITES: SITES },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  let buf = '', id = 1, stderr = '';
  const pending = new Map();
  child.stderr.on('data', (c) => (stderr += c));
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', (c) => {
    buf += c;
    const lines = buf.split('\n');
    buf = lines.pop();
    for (const line of lines) {
      if (!line.trim()) continue;
      const msg = JSON.parse(line);
      const p = pending.get(msg.id);
      if (p) { pending.delete(msg.id); p(msg); }
    }
  });
  const rpc = (method, params) => new Promise((resolve, reject) => {
    const n = id++;
    pending.set(n, resolve);
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: n, method, params }) + '\n');
    setTimeout(() => { if (pending.has(n)) { pending.delete(n); reject(new Error('timeout ' + method)); } }, 60000);
  });
  return {
    rpc,
    call: async (name, args) => {
      const r = await rpc('tools/call', { name, arguments: args || {} });
      return { isError: Boolean(r.result && r.result.isError), body: JSON.parse(r.result.content[0].text) };
    },
    stderr: () => stderr,
    close: () => child.kill(),
  };
}

(async () => {
  const c = connect();
  await c.rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'smoke-live', version: '1' } });
  const lan = lanAddress();

  // an id must never be reused, or "create" silently measures an old site
  const before = await c.call('list_sites', { id: ID });
  gate('id is free: ' + ID, before.isError === true, before.isError ? 'not registered' : 'already exists — pick another id');

  const payload = {
    id: ID, name: 'Smoke MCP', layout: 'docs', accent: 'navy',
    title: 'ตรวจการเชื่อมต่อ MCP กับ sitebox',
    tagline: 'สร้าง อ่าน ลบ จบในรอบเดียว',
    sections: [{ type: 'cta', heading: 'ผ่านแล้ว', body: 'ลบอัตโนมัติหลังตรวจ' }],
  };

  const created = await c.call('create_site', payload);
  gate('create_site', !created.isError, created.isError ? JSON.stringify(created.body.errors) : 'port ' + created.body.port + ' in ' + created.body.elapsedMs + 'ms');
  const port = created.body.port;

  if (!created.isError) {
    gate('serves 200 + title', created.body.verified.status === 200 && created.body.verified.titleMatch === true,
      'status=' + created.body.verified.status + ' titleMatch=' + created.body.verified.titleMatch);
    gate('root responds on loopback', (await status('127.0.0.1', port)) === 200, 'port ' + port);

    // The bind rule is the one thing a stub cannot prove: the same site must answer on
    // loopback and refuse the network, before AND after a restart (HOST survives).
    if (!lan) {
      skip('LAN refused (before restart)', 'no non-loopback IPv4 on this host');
    } else {
      const before_lan = await status(lan, port);
      gate('LAN refused (before restart)', before_lan === 0, lan + ':' + port + ' -> ' + before_lan);
    }

    const restarted = await c.call('site_ctl', { id: ID, action: 'restart' });
    gate('restart keeps the site up', !restarted.isError && restarted.body.live && restarted.body.live.status === 200,
      JSON.stringify(restarted.body.live || restarted.body.errors));
    if (lan) {
      const after_lan = await status(lan, port);
      gate('LAN refused (after restart)', after_lan === 0, lan + ':' + port + ' -> ' + after_lan);
    } else {
      skip('LAN refused (after restart)', 'no non-loopback IPv4 on this host');
    }

    const check = await c.call('check_site', { id: ID });
    gate('check_site clean', !check.isError, JSON.stringify(check.body.errors || []));

    const logs = await c.call('site_logs', { id: ID, lines: 20 });
    gate('site_logs readable', !logs.isError, 'errLines=' + logs.body.errorLines);

    const del = await c.call('delete_site', { id: ID, confirm: ID });
    gate('delete_site purged folder', !del.isError && del.body.purged === true, String(del.body.removed));
    gate('folder gone on disk', !fs.existsSync(path.join(SITES, ID)), path.join(SITES, ID));
    await new Promise((r) => setTimeout(r, 300));
    gate('port freed', (await status('127.0.0.1', port)) === 0, 'port ' + port);
    const after = await c.call('list_sites', { id: ID });
    gate('no ghost entry left', after.isError === true, after.isError ? 'gone from config' : 'still registered');
  }

  // Revocation must land on the next request: the store is read through a stat cache,
  // not held in memory for the life of the process.
  if (!fs.existsSync(CLI)) {
    skip('revoke -> 401 immediately', 'no token CLI at ' + CLI);
  } else {
    const label = 'smoke-revoke-' + Date.now().toString(36);
    let key = null;
    try {
      const out = execFileSync('node', [CLI, 'add', label, '--scope', 'read', '--print'], {
        env: { ...process.env, SITEBOX_DATA: DATA }, encoding: 'utf8',
      });
      key = out.trim().split('\n').pop().trim();
    } catch (e) {
      skip('revoke -> 401 immediately', 'could not mint a throwaway token: ' + String(e.message).split('\n')[0]);
    }
    if (key && key.startsWith('sbx_')) {
      const live = await api('GET', '/api/sites', key);
      gate('throwaway token works before revoke', live.status === 200, 'status=' + live.status);
      execFileSync('node', [CLI, 'revoke', label], { env: { ...process.env, SITEBOX_DATA: DATA }, encoding: 'utf8' });
      const dead = await api('GET', '/api/sites', key);
      gate('revoke -> 401 immediately', dead.status === 401, 'status=' + dead.status);
    }
  }

  // Hashes only: the store must never contain a usable key.
  try {
    const raw = fs.readFileSync(path.join(DATA, 'tokens.json'), 'utf8');
    gate('token store has no plaintext key', !/sbx_[A-Za-z0-9_-]{20,}/.test(raw), path.join(DATA, 'tokens.json'));
  } catch {
    skip('token store has no plaintext key', 'cannot read ' + path.join(DATA, 'tokens.json'));
  }

  // The bypass test: the dashboard page hands a session to anyone who opens it, so the
  // LAN guard is only real if that session cannot mint a token or create/delete a site.
  if (!lan) {
    skip('LAN cookie cannot mint a token', 'no non-loopback IPv4 on this host');
  } else {
    const cookie = await uiCookieFrom(lan);
    if (!cookie) {
      skip('LAN cookie cannot mint a token', 'GET / from ' + lan + ' returned no session cookie');
    } else {
      const origin = 'http://' + lan + ':4445';
      const base = origin;
      const mint = await raw('POST', '/api/tokens', { base, cookie, origin, body: { label: 'bypass-' + Date.now().toString(36), scope: 'write' } });
      gate('LAN cookie cannot mint a token', mint.status === 403, 'status=' + mint.status);

      const create = await raw('POST', '/api/sites', { base, cookie, origin, body: { id: 'lan-bypass', name: 'LAN', port: 0, host: '127.0.0.1' } });
      gate('LAN cookie cannot create a site', create.status === 403, 'status=' + create.status);

      const del = await raw('DELETE', '/api/sites/lan-bypass', { base, cookie, origin });
      gate('LAN cookie cannot delete a site', del.status === 403, 'status=' + del.status);

      const start = await raw('POST', '/api/sites/relay/start', { base, cookie, origin });
      gate('LAN cookie can still start/stop', start.status !== 403, 'status=' + start.status);
    }
  }

  c.close();
  console.log(results.join('\n'));
  const err = c.stderr().trim();
  if (err) console.log('\n--- mcp stderr ---\n' + err);
  process.exit(failed ? 1 : 0);
})().catch((e) => {
  console.log('SMOKE CRASH ' + (e && e.stack));
  process.exit(1);
});
