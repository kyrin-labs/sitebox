const http = require('http');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const PORT = 4445;
const ROOT = path.join(__dirname, '..');
const DATA = path.join(__dirname, 'data', 'sites.json');
const PUBLIC = path.join(__dirname, 'public');
const SITES_DIR = path.join(ROOT, 'sites');

const running = new Map();

/* ── Helpers ── */
function readConfig() { return JSON.parse(fs.readFileSync(DATA, 'utf-8')); }
function writeConfig(cfg) { fs.writeFileSync(DATA, JSON.stringify(cfg, null, 2) + '\n'); }
function send(res, status, data) {
  res.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8' });
  res.end(typeof data === 'string' ? data : JSON.stringify(data));
}
function parseBody(req) {
  return new Promise((resolve) => {
    let body = '';
    req.on('data', (c) => (body += c));
    req.on('end', () => { try { resolve(JSON.parse(body)); } catch { resolve(null); } });
  });
}

/* ── Port check ── */
function isPortUsed(port) {
  return new Promise((resolve) => {
    const srv = require('net').createServer();
    srv.once('error', () => resolve(true));
    srv.once('listening', () => { srv.close(); resolve(false); });
    srv.listen(port, '127.0.0.1');
  });
}

/* ── Health check ── */
function checkUrl(url, timeout = 3000) {
  return new Promise((resolve) => {
    const req = http.get(url, { timeout }, (res) => {
      res.resume();
      resolve({ online: res.statusCode < 500, status: res.statusCode });
    });
    req.on('error', () => resolve({ online: false, status: 0 }));
    req.on('timeout', () => { req.destroy(); resolve({ online: false, status: 0 }); });
  });
}

/* ── Auto-detect sites from filesystem ── */
function autoDetect() {
  const cfg = readConfig();
  const existing = new Set(cfg.sites.map(s => s.id));
  if (!fs.existsSync(SITES_DIR)) return cfg;

  let nextPort = 4800;
  const usedPorts = new Set(cfg.sites.map(s => s.port));

  for (const d of fs.readdirSync(SITES_DIR, { withFileTypes: true })) {
    if (!d.isDirectory() || existing.has(d.name)) continue;
    if (!fs.existsSync(path.join(SITES_DIR, d.name, 'server.js'))) continue;

    while (usedPorts.has(nextPort) && nextPort < 65535) nextPort++;
    if (nextPort >= 65535) break;

    const name = d.name.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
    cfg.sites.push({
      id: d.name, name, description: 'Auto-detected',
      icon: 'globe', iconColor: '#8b949e', port: nextPort,
      category: 'detected', url: `http://localhost:${nextPort}`,
      path: `sites/${d.name}`, created: new Date().toISOString().slice(0, 10), _auto: true,
    });
    usedPorts.add(nextPort);
    nextPort++;
  }
  writeConfig(cfg);
  return cfg;
}

/* ── Site process management ── */
function startSite(site) {
  if (running.has(site.id)) return { ok: true, message: 'already running' };
  const serverFile = path.join(ROOT, site.path, 'server.js');
  if (!fs.existsSync(serverFile)) return { ok: false, message: 'server.js not found' };
  try {
    const child = spawn('node', [serverFile], {
      cwd: path.join(ROOT, site.path),
      env: { ...process.env, PORT: String(site.port) },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    // Drain stdout/stderr so the buffers never fill up and block the child
    child.stdout?.on('data', () => {});
    child.stderr?.on('data', () => {});
    child.unref();              // don't let the child keep the parent alive
    child.on('error', () => running.delete(site.id));
    child.on('exit', () => running.delete(site.id));
    running.set(site.id, child);
    return { ok: true, message: 'started', pid: child.pid };
  } catch (e) { return { ok: false, message: e.message }; }
}

function stopSite(site) {
  const proc = running.get(site.id);
  if (!proc) return { ok: true, message: 'not running' };
  try { proc.kill('SIGTERM'); } catch {}
  running.delete(site.id);
  return { ok: true, message: 'stopped' };
}

/* ── HTTP ── */
const MIME = {
  '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8', '.json': 'application/json; charset=utf-8',
  '.png': 'image/png', '.svg': 'image/svg+xml',
};

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  const p = url.pathname, m = req.method;

  if (p.startsWith('/api/')) {
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'GET,POST,PUT,DELETE,OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
    if (m === 'OPTIONS') return send(res, 204, '');

    const cfg = autoDetect();

    if (p === '/api/sites' && m === 'GET')
      return send(res, 200, cfg.sites.map(s => ({ ...s, _running: running.has(s.id) })));
    if (p === '/api/sites' && m === 'POST') {
      const body = await parseBody(req);
      if (!body?.id) return send(res, 400, { error: 'id required' });
      const idx = cfg.sites.findIndex(s => s.id === body.id);
      if (idx >= 0) cfg.sites[idx] = { ...cfg.sites[idx], ...body };
      else { body.created = new Date().toISOString().slice(0, 10); cfg.sites.push(body); }
      writeConfig(cfg); return send(res, 200, { ok: true });
    }
    const del = p.match(/^\/api\/sites\/([^/]+)$/);
    if (del && m === 'DELETE') {
      stopSite({ id: del[1] }); cfg.sites = cfg.sites.filter(s => s.id !== del[1]);
      writeConfig(cfg); return send(res, 200, { ok: true });
    }
    const startM = p.match(/^\/api\/sites\/([^/]+)\/start$/);
    if (startM && m === 'POST') {
      const s = cfg.sites.find(x => x.id === startM[1]);
      return s ? send(res, 200, startSite(s)) : send(res, 404, { error: 'not found' });
    }
    const stopM = p.match(/^\/api\/sites\/([^/]+)\/stop$/);
    if (stopM && m === 'POST') {
      const s = cfg.sites.find(x => x.id === stopM[1]);
      return s ? send(res, 200, stopSite(s)) : send(res, 404, { error: 'not found' });
    }
    if (p === '/api/ports/check' && m === 'GET') {
      const port = parseInt(url.searchParams.get('port'));
      if (!port || port < 1 || port > 65535) return send(res, 400, { error: 'invalid port' });
      return send(res, 200, { port, used: await isPortUsed(port), available: !(await isPortUsed(port)) });
    }
    const hl = p.match(/^\/api\/sites\/([^/]+)\/health$/);
    if (hl && m === 'GET') {
      const s = cfg.sites.find(x => x.id === hl[1]);
      return s ? send(res, 200, await checkUrl(s.url)) : send(res, 404, { error: 'not found' });
    }
    return send(res, 404, { error: 'not found' });
  }

  let fp = path.join(PUBLIC, p === '/' ? '/index.html' : p);
  if (!fp.startsWith(PUBLIC)) return send(res, 403, 'Forbidden');
  if (fs.existsSync(fp) && fs.statSync(fp).isFile()) {
    res.writeHead(200, { 'Content-Type': MIME[path.extname(fp)] || 'application/octet-stream' });
    return fs.createReadStream(fp).pipe(res);
  }
  const idx = path.join(PUBLIC, 'index.html');
  if (fs.existsSync(idx)) { res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }); fs.createReadStream(idx).pipe(res); }
  else send(res, 404, 'Not Found');
});

server.listen(PORT, '0.0.0.0', () => console.log(`\n  SiteBox Dashboard  →  http://localhost:${PORT}\n`));
