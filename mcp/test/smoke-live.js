#!/usr/bin/env node
'use strict';
/**
 * smoke-live — the MCP round trip against a REAL dashboard (default 4445).
 *
 *   SITEBOX_API_TOKEN=… node mcp/test/smoke-live.js [id]
 *
 * Proves, in one run: create renders and serves 200 with the right <title>, check passes
 * clean, logs are readable, delete purges the folder AND frees the port. Exit 0 only when
 * all of that is true; anything else prints the failure and exits 1.
 *
 * This is a gate, not a demo: it never reuses an existing site id, and it always deletes
 * what it created — a leftover listener would poison the next allocation.
 */
const { spawn } = require('child_process');
const http = require('http');
const fs = require('fs');
const path = require('path');

const API = process.env.SITEBOX_API || 'http://127.0.0.1:4445';
const SITES = process.env.SITEBOX_SITES || path.join(process.env.HOME || '', 'sitebox', 'sites');
const MCP = path.join(__dirname, '..', 'sitebox-mcp.js');
const ID = process.argv[2] || 'smoke-mcp';

function probes(port) {
  return new Promise((resolve) => {
    const req = http.get({ host: '127.0.0.1', port, path: '/', timeout: 1500 }, (res) => {
      res.resume(); resolve(res.statusCode);
    });
    req.on('error', () => resolve(0));
    req.on('timeout', () => { req.destroy(); resolve(0); });
  });
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

const results = [];
let failed = 0;
function gate(name, pass, detail) {
  results.push((pass ? 'PASS ' : 'FAIL ') + name + (detail ? '  [' + detail + ']' : ''));
  if (!pass) failed++;
}

(async () => {
  const c = connect();
  await c.rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'smoke-live', version: '1' } });

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
    gate('schema file is not served', (await probes(port)) === 200, 'root responds');

    const check = await c.call('check_site', { id: ID });
    gate('check_site clean', !check.isError, JSON.stringify(check.body.errors || []));

    const logs = await c.call('site_logs', { id: ID, lines: 20 });
    gate('site_logs readable', !logs.isError, 'errLines=' + logs.body.errorLines);

    const del = await c.call('delete_site', { id: ID, confirm: ID });
    gate('delete_site purged folder', !del.isError && del.body.purged === true, String(del.body.removed));
    const gone = !fs.existsSync(path.join(SITES, ID));
    gate('folder gone on disk', gone, path.join(SITES, ID));
    await new Promise((r) => setTimeout(r, 300));
    const status = await probes(port);
    gate('port freed', status === 0, 'port ' + port + ' -> ' + status);
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
