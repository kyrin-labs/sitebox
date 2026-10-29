'use strict';
/**
 * stub-dashboard — the smallest dashboard the MCP contract needs, so the test suite
 * can run without Docker, without port 4445, and without touching production.
 * Implements: /api/health, /api/sites (GET/POST), start/stop/restart, logs, DELETE.
 */
const http = require('http');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const ID_RE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const FEATURES = (process.env.STUB_FEATURES || 'health,port0,host,auth').split(',').filter(Boolean);
const SITES = process.env.STUB_SITES || path.join(process.cwd(), 'sites');
const BAND = [4500, 4899];

let sites = [];
const running = new Map();
const logs = new Map();
let cursor = BAND[0];

const net = require('net');
const free = (p) => !sites.some((s) => s.port === p) && !running.has(p);
const isBound = (p) => new Promise((resolve) => {
  const s = net.createServer();
  s.once('error', () => resolve(true));
  s.once('listening', () => { s.close(); resolve(false); });
  s.listen(p, '127.0.0.1');
});
async function allocate() {
  for (let i = 0; i <= BAND[1] - BAND[0]; i++) {
    const p = BAND[0] + ((cursor - BAND[0] + i) % (BAND[1] - BAND[0] + 1));
    // The config is not the world: the host may already listen on this port
    // (production runs in the same 4500-4899 band on this machine).
    if (free(p) && !(await isBound(p))) { cursor = p + 1; return p; }
  }
  return null;
}
const alive = (c) => c && c.exitCode === null;
const siteDir = (s) => path.join(SITES, '..', s.path || 'sites/' + s.id);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const probe = (port) => new Promise((res) => {
  const r = http.get({ host: '127.0.0.1', port, path: '/', timeout: 700 }, (x) => { x.resume(); res(true); });
  r.on('error', () => res(false)); r.on('timeout', () => { r.destroy(); res(false); });
});

function body(req) {
  return new Promise((res) => { let d = ''; req.on('data', (c) => (d += c)); req.on('end', () => { try { res(JSON.parse(d)); } catch { res(null); } }); });
}
const send = (res, code, obj) => { res.writeHead(code, { 'content-type': 'application/json' }); res.end(JSON.stringify(obj)); };

async function start(site) {
  if (alive(running.get(site.id))) return { ok: true, message: 'already running', port: site.port, verified: true };
  if (await probe(site.port)) return { ok: false, message: 'port ' + site.port + ' is already in use before starting', hint: 'move the site', logs: [] };
  const dir = siteDir(site);
  const file = path.join(dir, 'server.js');
  if (!fs.existsSync(file)) return { ok: false, message: 'server.js not found at ' + site.path, hint: 'files missing', logs: [] };
  const child = spawn('node', [file], {
    cwd: dir, stdio: ['ignore', 'pipe', 'pipe'],
    env: { PATH: process.env.PATH, HOME: process.env.HOME, PORT: String(site.port), HOST: site.host || '127.0.0.1' },
  });
  const buf = [];
  child.stdout.on('data', (c) => buf.push({ stream: 'out', text: String(c) }));
  child.stderr.on('data', (c) => buf.push({ stream: 'err', text: String(c) }));
  child.unref();
  child.on('exit', () => running.delete(site.id));
  running.set(site.id, child);
  logs.set(site.id, buf);
  for (let i = 0; i < 40; i++) {
    if (!alive(child)) return { ok: false, message: 'process exited before listening', hint: 'read logs', logs: buf };
    if (await probe(site.port)) return { ok: true, message: 'started and verified', port: site.port, pid: child.pid, verified: true };
    await sleep(100);
  }
  return { ok: false, message: 'never started listening', hint: 'read logs', logs: buf };
}

async function stop(site) {
  const child = running.get(site.id);
  if (!alive(child)) { running.delete(site.id); return { ok: true, message: 'not running' }; }
  child.kill('SIGTERM');
  for (let i = 0; i < 25; i++) { if (!alive(child)) break; await sleep(60); }
  if (alive(child)) child.kill('SIGKILL');
  running.delete(site.id);
  return { ok: true, message: 'stopped' };
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x');
  const p = url.pathname, m = req.method;

  if (p === '/api/health' && m === 'GET') return send(res, 200, { ok: true, features: FEATURES });
  if (p === '/api/sites' && m === 'GET') {
    return send(res, 200, sites.map((s) => ({
      ...s, _running: alive(running.get(s.id)),
      _stale: !fs.existsSync(path.join(siteDir(s), 'server.js')),
      _localOnly: s.host === '127.0.0.1',
    })));
  }
  if (p === '/api/sites' && m === 'POST') {
    const b = await body(req);
    if (!b || !b.id) return send(res, 400, { error: 'id required' });
    if (!ID_RE.test(b.id)) return send(res, 400, { error: 'id must be kebab-case' });
    const idx = sites.findIndex((s) => s.id === b.id);
    const next = idx >= 0 ? { ...sites[idx], ...b } : { ...b };
    if (next.host !== undefined && !['127.0.0.1', '0.0.0.0'].includes(next.host)) return send(res, 400, { error: 'host not allowed' });
    if (next.port === 0) {
      const p2 = await allocate();
      if (p2 == null) return send(res, 507, { error: 'no free port in ' + BAND[0] + '-' + BAND[1] });
      next.port = p2;
    }
    if (!Number.isInteger(Number(next.port))) return send(res, 400, { error: 'port must be an integer' });
    next.port = Number(next.port);
    const clash = sites.find((s) => s.id !== b.id && s.port === next.port);
    if (clash) return send(res, 409, { error: 'port ' + next.port + ' is already assigned to "' + clash.id + '"' });
    next.path = next.path || 'sites/' + next.id;
    next.url = 'http://localhost:' + next.port;
    if (idx >= 0) sites[idx] = next; else sites.push(next);
    return send(res, 200, { ok: true, site: next });
  }
  const ctl = p.match(/^\/api\/sites\/([^/]+)\/(start|stop|restart)$/);
  if (ctl && m === 'POST') {
    const site = sites.find((s) => s.id === ctl[1]);
    if (!site) return send(res, 404, { error: 'not found' });
    if (ctl[2] === 'stop') return send(res, 200, await stop(site));
    if (ctl[2] === 'restart') { await stop(site); await sleep(120); return send(res, 200, await start(site)); }
    return send(res, 200, await start(site));
  }
  const lg = p.match(/^\/api\/sites\/([^/]+)\/logs$/);
  if (lg && m === 'GET') {
    const site = sites.find((s) => s.id === lg[1]);
    if (!site) return send(res, 404, { error: 'not found' });
    const all = logs.get(site.id) || [];
    return send(res, 200, { id: site.id, running: alive(running.get(site.id)), count: all.length, lines: all.slice(-50) });
  }
  const del = p.match(/^\/api\/sites\/([^/]+)$/);
  if (del && m === 'DELETE') {
    const site = sites.find((s) => s.id === del[1]);
    if (!site) return send(res, 404, { error: 'not found' });
    await stop(site);
    const abs = siteDir(site);
    if (abs.startsWith(SITES + path.sep)) fs.rmSync(abs, { recursive: true, force: true });
    sites = sites.filter((s) => s.id !== site.id);
    return send(res, 200, { ok: true, purged: true, removed: site.path });
  }
  return send(res, 404, { error: 'not found' });
});

server.listen(Number(process.env.STUB_PORT) || 0, '127.0.0.1', () => {
  process.stdout.write(JSON.stringify({ port: server.address().port, sites: SITES, features: FEATURES }) + '\n');
});

for (const sig of ['SIGTERM', 'SIGINT']) process.on(sig, async () => { for (const s of sites) await stop(s); process.exit(0); });
