const http = require('http');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const PORT = 4445;
const ROOT = path.join(__dirname, '..');
const DATA = path.join(__dirname, 'data', 'sites.json');
const PUBLIC = path.join(__dirname, 'public');
const SITES_DIR = path.join(ROOT, 'sites');

const ID_RE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const LOG_LIMIT = 500;
const START_TIMEOUT = 6000;
const LOG_TAIL = 20;

const running = new Map();
const logs = new Map();

/* ── Helpers ── */
// A malformed sites.json must not take the whole dashboard down: keep the last
// error, serve a safe empty config, and refuse to overwrite the bad file so it
// can still be repaired by hand.
let configError = null;
function defaultConfig() {
  return { dashboard: { name: 'SiteBox', port: PORT }, sites: [] };
}
function readConfig() {
  try {
    const cfg = JSON.parse(fs.readFileSync(DATA, 'utf-8'));
    if (!cfg || typeof cfg !== 'object' || !Array.isArray(cfg.sites))
      throw new Error('sites.json must be an object with a "sites" array');
    configError = null;
    return cfg;
  } catch (e) {
    if (configError !== e.message) console.error(`readConfig failed: ${e.message}`);
    configError = e.message;
    return defaultConfig();
  }
}
function writeConfig(cfg) {
  if (configError) {
    console.error('writeConfig skipped: sites.json is unreadable (fix it, then retry)');
    return;
  }
  const tmp = DATA + '.tmp';
  fs.writeFileSync(tmp, JSON.stringify(cfg, null, 2) + '\n');
  fs.renameSync(tmp, DATA);
}
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
function sleep(ms) { return new Promise((r) => setTimeout(r, ms)); }
function isAlive(child) { return !!child && child.exitCode === null && child.signalCode === null; }
function siteFile(site) { return path.join(ROOT, site.path || '', 'server.js'); }

/* ── Site favicon detection ──
 * The dashboard prefers a site's real favicon over its configured Lucide icon.
 * Detection is filesystem-based and runtime-only: it never rewrites sites.json
 * (browsing must not dirty config) and it works while the site is stopped.
 * Order:
 *   1. the <link rel="icon"> declared in public/index.html
 *   2. the conventional favicon filenames at the root of public/
 * Only local files are eligible. A hotlinked or data-URI favicon is ignored on
 * purpose: the dashboard must render offline and never depend on a third party.
 */
const FAVICON_FILES = [
  'favicon.svg', 'favicon.png', 'favicon.ico', 'favicon.webp',
  'icon.svg', 'icon.png', 'apple-touch-icon.png',
];

// Resolve `rel` inside `baseDir`, refusing anything that escapes it.
function resolveInside(baseDir, rel) {
  const base = path.resolve(baseDir);
  const fp = path.resolve(base, String(rel).replace(/^\/+/, ''));
  if (fp !== base && !fp.startsWith(base + path.sep)) return null;
  try { return fs.statSync(fp).isFile() ? fp : null; } catch { return null; }
}

function detectFavicon(site) {
  const pub = path.join(ROOT, site.path || `sites/${site.id}`, 'public');
  const indexPath = path.join(pub, 'index.html');
  if (fs.existsSync(indexPath)) {
    let html = '';
    try { html = fs.readFileSync(indexPath, 'utf-8'); } catch { html = ''; }
    for (const tag of html.match(/<link\b[^>]*>/gi) || []) {
      if (!/\brel\s*=\s*["']?[^"'>]*\bicon/i.test(tag)) continue;
      const href = (tag.match(/\bhref\s*=\s*["']([^"']*)["']/i) || [])[1];
      if (!href || /^(?:https?:)?\/\//i.test(href) || href.startsWith('data:')) continue;
      const fp = resolveInside(pub, href.split(/[?#]/)[0]);
      if (fp) return fp;
    }
  }
  for (const name of FAVICON_FILES) {
    const fp = resolveInside(pub, name);
    if (fp) return fp;
  }
  return null;
}

/* ── Logs (ring buffer, last 500 lines per site) ── */
function appendLog(id, stream, chunk) {
  const buf = logs.get(id) || [];
  const t = new Date().toISOString();
  for (const line of String(chunk).split(/\r?\n/)) {
    if (line === '') continue;
    buf.push({ t, stream, text: line });
  }
  while (buf.length > LOG_LIMIT) buf.shift();
  logs.set(id, buf);
}
function getLogs(id, limit = 200) {
  const buf = logs.get(id) || [];
  return buf.slice(-limit);
}
function crashHint(lines) {
  const text = lines.map((l) => l.text).join('\n');
  if (/EADDRINUSE/.test(text)) return 'Port is already in use. Check GET /api/ports/check, update the site with a free port (POST /api/sites), then start again.';
  if (/Cannot find module|MODULE_NOT_FOUND/.test(text)) return 'A required file or module is missing. Check the files in the site folder.';
  if (/SyntaxError/.test(text)) return 'server.js (or a required file) has a syntax error. Fix it, then start again.';
  return 'Read the full output with GET /api/sites/' + '{id}' + '/logs.';
}

/* ── Port / listener checks ──
 * The bind probe alone is unreliable on Windows (a wildcard 0.0.0.0 listener
 * does not always block a 127.0.0.1 bind), so "in use" is the union of a bind
 * probe and an HTTP probe. */
function isPortBound(port) {
  return new Promise((resolve) => {
    const srv = require('net').createServer();
    srv.once('error', () => resolve(true));
    srv.once('listening', () => { srv.close(); resolve(false); });
    srv.listen(port, '127.0.0.1');
  });
}
function isHttpUp(port, timeout = 800) {
  return new Promise((resolve) => {
    const req = http.get({ host: '127.0.0.1', port, path: '/', timeout }, (res) => {
      res.resume();
      resolve(true);
    });
    req.on('error', () => resolve(false));
    req.on('timeout', () => { req.destroy(); resolve(false); });
  });
}
async function isPortUsed(port) {
  return (await isPortBound(port)) || (await isHttpUp(port));
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
  if (configError) return cfg; // never scan/write against an unreadable config
  const existing = new Set(cfg.sites.map(s => s.id));
  if (!fs.existsSync(SITES_DIR)) return cfg;

  let added = 0;
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
    added++;
  }
  // Only touch the file when something actually changed — browsing the
  // dashboard must never dirty sites.json.
  if (added > 0) writeConfig(cfg);
  return cfg;
}

/* ── Site process management ── */
async function startSite(site) {
  const existing = running.get(site.id);
  if (isAlive(existing)) {
    return { ok: true, message: 'already running', pid: existing.pid, port: site.port, verified: true };
  }
  running.delete(site.id);

  if (!fs.existsSync(siteFile(site))) {
    return {
      ok: false,
      message: `server.js not found at ${site.path || '(no path)'}/server.js`,
      hint: 'The site folder is missing or the path in sites.json is wrong. Check the "files missing" flag on GET /api/sites (or _stale), then fix the path or delete the stale entry.',
      logs: [],
    };
  }

  // A leftover process (e.g. from a previous dashboard run) would make the
  // new one crash with EADDRINUSE — catch it before spawning.
  if (await isPortUsed(site.port)) {
    return {
      ok: false,
      message: `port ${site.port} is already in use before starting`,
      hint: 'Another process owns this port — often a leftover site process from an earlier dashboard run. Stop it, restart the dashboard, or move the site to a free port (GET /api/ports/check).',
      logs: [],
    };
  }

  appendLog(site.id, 'sys', `--- start on port ${site.port} ---`);
  let child;
  try {
    child = spawn('node', [siteFile(site)], {
      cwd: path.join(ROOT, site.path),
      env: { ...process.env, PORT: String(site.port) },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (e) {
    return { ok: false, message: e.message, logs: getLogs(site.id, LOG_TAIL) };
  }

  let dead = false;
  child.stdout?.on('data', (c) => appendLog(site.id, 'out', c));
  child.stderr?.on('data', (c) => appendLog(site.id, 'err', c));
  child.unref();
  child.on('error', (e) => { dead = true; running.delete(site.id); appendLog(site.id, 'err', `spawn error: ${e.message}`); });
  child.on('exit', (code, signal) => {
    dead = true;
    running.delete(site.id);
    appendLog(site.id, 'sys', `process exited (code=${code ?? 'null'} signal=${signal ?? 'null'})`);
  });
  running.set(site.id, child);

  const alive = () => !dead && child.exitCode === null;
  const deadline = Date.now() + START_TIMEOUT;

  while (Date.now() < deadline) {
    if (!alive()) {
      const tail = getLogs(site.id, LOG_TAIL);
      return {
        ok: false,
        message: `process exited before it started listening (code=${child.exitCode ?? 'null'})`,
        hint: crashHint(tail),
        logs: tail,
      };
    }
    if (await isPortUsed(site.port)) {
      // Settle briefly: if the port belongs to someone else the child will die now.
      await sleep(250);
      if (!alive()) {
        const tail = getLogs(site.id, LOG_TAIL);
        return {
          ok: false,
          message: `process exited while starting (code=${child.exitCode ?? 'null'})`,
          hint: crashHint(tail),
          logs: tail,
        };
      }
      return { ok: true, message: 'started and verified', pid: child.pid, port: site.port, verified: true };
    }
    await sleep(150);
  }

  const tail = getLogs(site.id, LOG_TAIL);
  return {
    ok: false,
    message: `process is alive but port ${site.port} never started listening within ${START_TIMEOUT}ms`,
    hint: 'Check the logs — the site may listen on a different port than sites.json says, or it crashed after startup.',
    logs: tail,
  };
}

async function stopSite(site) {
  const child = running.get(site.id);
  if (!isAlive(child)) { running.delete(site.id); return { ok: true, message: 'not running' }; }
  const exited = new Promise((r) => child.once('exit', r));
  try { child.kill('SIGTERM'); } catch {}
  const done = await Promise.race([exited.then(() => true), sleep(2500).then(() => false)]);
  if (!done && isAlive(child)) {
    try { child.kill('SIGKILL'); } catch {}
    await Promise.race([exited, sleep(1500)]);
  }
  running.delete(site.id);
  appendLog(site.id, 'sys', 'stopped from dashboard');
  return { ok: true, message: isAlive(child) ? 'stop signal sent' : 'stopped' };
}

async function restartSite(site) {
  await stopSite(site);
  const deadline = Date.now() + 2000;
  while ((await isPortUsed(site.port)) && Date.now() < deadline) await sleep(150);
  return startSite(site);
}

/* ── HTTP ── */
const MIME = {
  '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8', '.json': 'application/json; charset=utf-8',
  '.png': 'image/png', '.svg': 'image/svg+xml', '.ico': 'image/x-icon',
  '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.webp': 'image/webp', '.gif': 'image/gif',
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
    if (configError) return send(res, 500, {
      error: 'sites.json is unreadable',
      detail: configError,
      hint: 'Fix or restore dashboard/data/sites.json, then retry.',
    });

    if (p === '/api/sites' && m === 'GET')
      return send(res, 200, cfg.sites.map(s => {
        const fav = detectFavicon(s);
        return {
          ...s,
          _running: isAlive(running.get(s.id)),
          _stale: !fs.existsSync(siteFile(s)),
          _logLines: (logs.get(s.id) || []).length,
          // Runtime-only: a detected favicon overrides the configured icon and is
          // never written back to sites.json, so removing the file restores it.
          _favicon: fav ? `/api/sites/${encodeURIComponent(s.id)}/icon` : null,
          _faviconSource: fav ? path.relative(ROOT, fav).replace(/\\/g, '/') : null,
        };
      }));

    if (p === '/api/sites' && m === 'POST') {
      const body = await parseBody(req);
      if (!body || typeof body !== 'object') return send(res, 400, { error: 'invalid JSON body' });
      if (!body.id) return send(res, 400, { error: 'id required' });
      if (!ID_RE.test(body.id)) return send(res, 400, { error: 'id must be kebab-case — lowercase letters, numbers, single hyphens (e.g. my-site)' });

      const idx = cfg.sites.findIndex(s => s.id === body.id);
      const next = idx >= 0 ? { ...cfg.sites[idx], ...body } : { ...body };
      delete next._running; delete next._stale; delete next._logLines;

      const port = Number(next.port);
      if (!Number.isInteger(port) || port < 1024 || port > 65535)
        return send(res, 400, { error: 'port must be an integer between 1024 and 65535' });
      next.port = port;

      const clash = cfg.sites.find(s => s.id !== body.id && Number(s.port) === port);
      if (clash) return send(res, 409, { error: `port ${port} is already assigned to "${clash.id}"` });

      if (!next.name) return send(res, 400, { error: 'name required' });
      next.path = next.path || `sites/${next.id}`;
      // Keep the default url in sync when the port changes and no explicit url was sent.
      if (!body.url && /^https?:\/\/localhost:\d+$/.test(next.url || '')) next.url = `http://localhost:${port}`;
      next.url = next.url || `http://localhost:${port}`;

      if (idx >= 0) cfg.sites[idx] = next;
      else { next.created = next.created || new Date().toISOString().slice(0, 10); cfg.sites.push(next); }
      writeConfig(cfg);
      return send(res, 200, { ok: true, site: next });
    }


    // POST /api/sites/reorder — reorder sites
    if (p === '/api/sites/reorder' && m === 'POST') {
      const body = await parseBody(req);
      if (!body?.order || !Array.isArray(body.order)) return send(res, 400, { error: 'order array required' });
      const order = body.order;
      const reordered = order.map(id => cfg.sites.find(s => s.id === id)).filter(Boolean);
      // Add any sites not in the order array at the end
      cfg.sites.forEach(s => { if (!order.includes(s.id)) reordered.push(s); });
      cfg.sites = reordered;
      writeConfig(cfg);
      return send(res, 200, { ok: true });
    }

    const startM = p.match(/^\/api\/sites\/([^/]+)\/start$/);
    if (startM && m === 'POST') {
      const s = cfg.sites.find(x => x.id === startM[1]);
      return s ? send(res, 200, await startSite(s)) : send(res, 404, { error: 'not found' });
    }
    const stopM = p.match(/^\/api\/sites\/([^/]+)\/stop$/);
    if (stopM && m === 'POST') {
      const s = cfg.sites.find(x => x.id === stopM[1]);
      return s ? send(res, 200, await stopSite(s)) : send(res, 404, { error: 'not found' });
    }
    const restartM = p.match(/^\/api\/sites\/([^/]+)\/restart$/);
    if (restartM && m === 'POST') {
      const s = cfg.sites.find(x => x.id === restartM[1]);
      return s ? send(res, 200, await restartSite(s)) : send(res, 404, { error: 'not found' });
    }
    const logsM = p.match(/^\/api\/sites\/([^/]+)\/logs$/);
    if (logsM && m === 'GET') {
      const s = cfg.sites.find(x => x.id === logsM[1]);
      if (!s) return send(res, 404, { error: 'not found' });
      const requested = parseInt(url.searchParams.get('lines'), 10);
      const limit = Number.isInteger(requested) ? Math.min(Math.max(requested, 1), LOG_LIMIT) : 200;
      return send(res, 200, {
        id: s.id,
        running: isAlive(running.get(s.id)),
        count: (logs.get(s.id) || []).length,
        lines: getLogs(s.id, limit),
      });
    }
    if (logsM && m === 'DELETE') {
      const s = cfg.sites.find(x => x.id === logsM[1]);
      if (!s) return send(res, 404, { error: 'not found' });
      logs.delete(s.id);
      return send(res, 200, { ok: true, cleared: true });
    }
    const del = p.match(/^\/api\/sites\/([^/]+)$/);
    if (del && m === 'DELETE') {
      const s = cfg.sites.find(x => x.id === del[1]);
      if (!s) return send(res, 404, { error: 'not found' });
      const purge = url.searchParams.get('purge') === '1';
      let abs = null;
      if (purge) {
        abs = path.resolve(ROOT, s.path || '');
        if (!abs.startsWith(SITES_DIR + path.sep) || abs === SITES_DIR)
          return send(res, 400, { error: 'refusing to purge: path is outside sites/' });
      }
      await stopSite(s);
      if (purge) fs.rmSync(abs, { recursive: true, force: true });
      cfg.sites = cfg.sites.filter(x => x.id !== del[1]);
      writeConfig(cfg);
      logs.delete(del[1]);
      return send(res, 200, {
        ok: true,
        purged: purge,
        note: purge
          ? 'config and site folder removed'
          : 'config removed only — the folder is still on disk, so it will be auto-detected again',
      });
    }
    if (p === '/api/ports/check' && m === 'GET') {
      const port = parseInt(url.searchParams.get('port'), 10);
      if (!port || port < 1 || port > 65535) return send(res, 400, { error: 'invalid port' });
      const used = await isPortUsed(port);
      return send(res, 200, { port, used, available: !used });
    }
    const hl = p.match(/^\/api\/sites\/([^/]+)\/health$/);
    if (hl && m === 'GET') {
      const s = cfg.sites.find(x => x.id === hl[1]);
      return s ? send(res, 200, await checkUrl(s.url)) : send(res, 404, { error: 'not found' });
    }
    // GET /api/sites/:id/icon — serve the auto-detected favicon from disk.
    // Served through the dashboard so it works while the site is stopped and
    // never crosses origins; ETag + no-cache so a changed file is picked up.
    const iconM = p.match(/^\/api\/sites\/([^/]+)\/icon$/);
    if (iconM && m === 'GET') {
      const s = cfg.sites.find(x => x.id === iconM[1]);
      if (!s) return send(res, 404, { error: 'not found' });
      const fp = detectFavicon(s);
      if (!fp) return send(res, 404, { error: 'no favicon detected' });
      const stat = fs.statSync(fp);
      const etag = `"${stat.size.toString(16)}-${stat.mtimeMs.toString(16)}"`;
      res.setHeader('X-Content-Type-Options', 'nosniff');
      res.setHeader('Cache-Control', 'no-cache');
      res.setHeader('ETag', etag);
      if (req.headers['if-none-match'] === etag) { res.writeHead(304); return res.end(); }
      res.writeHead(200, { 'Content-Type': MIME[path.extname(fp).toLowerCase()] || 'application/octet-stream' });
      return fs.createReadStream(fp).pipe(res);
    }
    return send(res, 404, { error: 'not found' });
  }

  let fp = path.join(PUBLIC, p === '/' ? '/index.html' : p);
  if (!fp.startsWith(PUBLIC)) return send(res, 403, 'Forbidden');
  if (fs.existsSync(fp) && fs.statSync(fp).isFile()) {
    res.writeHead(200, { 'Content-Type': MIME[path.extname(fp)] || 'application/octet-stream' });
    return fs.createReadStream(fp).pipe(res);
  }
  // A missing asset is a real 404 — only extension-less routes fall back to the SPA.
  if (path.extname(p)) return send(res, 404, 'Not Found');
  const idx = path.join(PUBLIC, 'index.html');
  if (fs.existsSync(idx)) { res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }); fs.createReadStream(idx).pipe(res); }
  else send(res, 404, 'Not Found');
});

server.listen(PORT, '0.0.0.0', () => console.log(`\n  SiteBox Dashboard  →  http://localhost:${PORT}\n`));
