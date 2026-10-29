'use strict';
/**
 * Permanent gate suite. Runs with `node --test mcp/test/` — no Docker, no port 4445,
 * no production access. If a rule stops being enforced, this file goes red.
 */
const { test, before, after } = require('node:test');
const assert = require('node:assert');
const { spawn, execFileSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const ROOT = path.join(__dirname, '..');
const MCP = path.join(ROOT, 'sitebox-mcp.js');
const STUB = path.join(__dirname, 'stub-dashboard.js');
const { render } = require(path.join(ROOT, 'lib/render.js'));
const { validate, validateOutput, drift, fingerprint } = require(path.join(ROOT, 'lib/validate.js'));
const { SKELETON } = require(path.join(ROOT, 'lib/schema.js'));

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'sitebox-mcp-test-'));
const SITES = path.join(tmp, 'sites');
fs.mkdirSync(SITES, { recursive: true });

/* ── stub lifecycle ── */
function startStub(features) {
  const child = spawn('node', [STUB], {
    env: { ...process.env, STUB_SITES: SITES, STUB_FEATURES: features || 'health,port0,host,auth' },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  return new Promise((resolve, reject) => {
    let buf = '';
    child.stdout.on('data', (c) => {
      buf += c;
      const nl = buf.indexOf('\n');
      if (nl >= 0) {
        const info = JSON.parse(buf.slice(0, nl));
        resolve({
          port: info.port,
          api: 'http://127.0.0.1:' + info.port,
          close: () => { child.kill('SIGTERM'); },
        });
      }
    });
    child.on('error', reject);
    setTimeout(() => reject(new Error('stub did not start')), 5000);
  });
}

/* ── MCP client over real stdio ── */
function mcp(apiBase) {
  const child = spawn('node', [MCP], {
    env: { ...process.env, SITEBOX_API: apiBase, SITEBOX_SITES: SITES, SITEBOX_API_TOKEN: 'sbx_test_placeholder_token' },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  let buf = '';
  let id = 1;
  const pending = new Map();
  let stderr = '';
  child.stdout.setEncoding('utf8');
  child.stderr.on('data', (c) => (stderr += c));
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
    setTimeout(() => { if (pending.has(n)) { pending.delete(n); reject(new Error('timeout ' + method)); } }, 30000);
  });
  return {
    rpc,
    notify: (method, params) => child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method, params }) + '\n'),
    call: async (name, args) => {
      const r = await rpc('tools/call', { name, arguments: args || {} });
      return { isError: Boolean(r.result && r.result.isError), body: JSON.parse(r.result.content[0].text), raw: r };
    },
    stderr: () => stderr,
    close: () => child.kill(),
  };
}

const GOOD = {
  id: 'gate-lamp', name: 'Gate Lamp', layout: 'docs', accent: 'ink-green',
  title: 'ทดสอบโคมอ่านหนังสือ',
  tagline: 'โคมสำหรับคนที่นั่งโต๊ะนาน ๆ',
  sections: [
    { type: 'features', heading: 'ทำไมต้องโคมนี้', body: 'แสงกว้างพอ',
      items: [{ label: 'แสงนิ่ง', text: 'ไม่มี flicker' }] },
    { type: 'table', heading: 'สเปก', columns: ['รุ่น', 'กำลังไฟ'],
      items: [{ label: 'Lamp-01', text: '8 W' }] },
    { type: 'cta', heading: 'ลองสิบสี่วัน', body: 'คืนได้' },
  ],
};

let stub;
let c;

before(async () => {
  stub = await startStub();
  c = mcp(stub.api);
  await c.rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'test', version: '1' } });
  c.notify('notifications/initialized', {});
});

after(() => {
  if (c) c.close();
  if (stub) stub.close();
  fs.rmSync(tmp, { recursive: true, force: true });
});

/* ── transport ── */
test('initialize echoes the client protocol version', async () => {
  const r = await c.rpc('initialize', { protocolVersion: '2025-03-26', capabilities: {}, clientInfo: { name: 'x', version: '1' } });
  assert.equal(r.result.protocolVersion, '2025-03-26');
  assert.equal(r.result.serverInfo.name, 'sitebox');
});

test('tools/list exposes exactly eight tools', async () => {
  const r = await c.rpc('tools/list', {});
  const names = r.result.tools.map((t) => t.name).sort();
  assert.deepEqual(names, ['check_site', 'create_site', 'delete_site', 'doctor', 'list_sites', 'site_ctl', 'site_logs', 'update_site']);
});

test('unknown method answers -32601 and unknown tool answers isError', async () => {
  const m = await c.rpc('nope/nope', {});
  assert.equal(m.error.code, -32601);
  const t = await c.call('no_such_tool', {});
  assert.equal(t.isError, true);
  assert.equal(t.body.errors[0].code, 'E_NO_TOOL');
});

/* ── render purity ── */
test('render is byte-identical in-process and across processes', () => {
  const a = render(GOOD);
  const b = render(GOOD);
  assert.equal(fingerprint(a.files), fingerprint(b.files));
  const child = execFileSync('node', ['-e',
    'const {render}=require(' + JSON.stringify(path.join(ROOT, 'lib/render.js')) + ');'
    + 'const {fingerprint}=require(' + JSON.stringify(path.join(ROOT, 'lib/validate.js')) + ');'
    + 'process.stdout.write(fingerprint(render(JSON.parse(process.argv[1])).files))',
    JSON.stringify(GOOD)], { encoding: 'utf8' });
  assert.equal(child, fingerprint(a.files));
});

test('cosmetic seed is stable per id and never touches layout or accent', () => {
  const one = render(Object.assign({}, GOOD, require(path.join(ROOT, 'lib/schema.js')).seedKnobs(GOOD.id)));
  const two = render(Object.assign({}, GOOD, require(path.join(ROOT, 'lib/schema.js')).seedKnobs(GOOD.id)));
  assert.equal(fingerprint(one.files), fingerprint(two.files));
  assert.match(one.files['public/style.css'], /--radius: \d+px/);
  assert.match(one.files['public/index.html'], /data-layout="docs"/);
});

/* ── validator: every code must be able to fire ── */
test('every validation code fires under a negative control', () => {
  const clone = (s) => JSON.parse(JSON.stringify(s));
  const fires = (mutate) => {
    const s = clone(GOOD);
    mutate(s);
    return validate(s).failures.map((f) => f.code);
  };

  assert.ok(fires((s) => { s.sections[0].heading = '\u00abheading\u00bb'; }).includes('E_PLACEHOLDER'));
  assert.ok(fires((s) => { s.sections[0].heading = 'ก'.repeat(60); }).includes('E_CAP'));
  assert.ok(fires((s) => { s.sections = Array.from({ length: 6 }, () => ({ type: 'cta', heading: 'ก', body: 'ข' })); }).includes('E_CAP'));
  assert.ok(fires((s) => { s.accent = 'violet'; }).includes('E_ENUM'));
  assert.ok(fires((s) => { s.layout = 'landing'; }).includes('E_ENUM'));
  assert.ok(fires((s) => { s.density = 'roomy'; }).includes('E_ENUM'));
  assert.ok(fires((s) => { s.sections[0].type = 'pricing'; }).includes('E_ENUM'));
  assert.ok(fires((s) => { s.id = 'Not_Kebab'; }).includes('E_ID_FORMAT'));
  assert.ok(fires((s) => { s.title = '  '; }).includes('E_EMPTY'));
  assert.ok(fires((s) => { delete s.sections; }).includes('E_SHAPE'));

  // Written as escapes on purpose: this repo ships zero Han characters in any file.
  const cjk = clone(GOOD); cjk.tagline = 'สวัสดี ' + '\u4f60\u597d';
  assert.ok(validateOutput(render(cjk)).failures.some((f) => f.code === 'E_CJK'));

  const ext = render(Object.assign(clone(GOOD), {})).files;
  ext['public/index.html'] = ext['public/index.html'].replace('</body>', '<img src="https://cdn.example/a.png" alt=""></body>');
  assert.ok(validateOutput({ files: ext, bytes: 100 }).failures.some((f) => f.code === 'E_EXTERNAL'));

  const css = render(clone(GOOD)).files;
  css['public/style.css'] += '\n@import url(https://fonts.example/x.css);\n';
  assert.ok(validateOutput({ files: css, bytes: 100 }).failures.some((f) => f.code === 'E_EXTERNAL'));

  assert.ok(validateOutput({ files: render(clone(GOOD)).files, bytes: 99 * 1024 }).failures.some((f) => f.code === 'E_BUDGET'));

  const drifty = Object.assign({}, render(clone(GOOD)).files);
  drifty['public/index.html'] = drifty['public/index.html'].replace('</main>', '<p>x</p></main>');
  assert.ok(drift(drifty, render(clone(GOOD)).files).failures.some((f) => f.code === 'E_DRIFT'));
});

test('the doctor skeleton is rejected as content', () => {
  assert.equal(validate(SKELETON).ok, false);
  assert.ok(validate(SKELETON).failures.some((f) => f.code === 'E_PLACEHOLDER'));
});

/* ── end to end against the stub dashboard ── */
test('doctor reports capabilities, a layout catalog and a fill-in skeleton', async () => {
  const d = await c.call('doctor', {});
  assert.equal(d.isError, false, JSON.stringify(d.body.errors || d.body));
  assert.ok(d.body.features.includes('port0'));
  assert.ok(d.body.create_site_skeleton.sections.length > 0);
  assert.equal(d.body.layouts.length, 5);
  for (const l of d.body.layouts) {
    assert.ok(l.name && l.whenToUse && l.whenToUse.length > 10, 'every layout needs a one-line whenToUse');
  }
  assert.deepEqual(d.body.layouts.map((l) => l.name).sort(), ['dashboard', 'docs', 'form', 'list', 'showcase']);
});

test('doctor reports the recreate signature as an operator decision, not an error', async () => {
  const http = require('http');
  const register = (id, port) => new Promise((resolve) => {
    const req = http.request(stub.api + '/api/sites', { method: 'POST', headers: { 'content-type': 'application/json' } }, (res) => {
      let d = ''; res.on('data', (x) => (d += x)); res.on('end', () => resolve(JSON.parse(d)));
    });
    req.write(JSON.stringify({ id, name: id, port, host: '127.0.0.1', path: 'sites/' + id }));
    req.end();
  });
  await register('ghost-a', 4790);
  await register('ghost-b', 4791);
  const d = await c.call('doctor', {});
  assert.equal(d.isError, false, 'the recreate signature is a note, not a failure');
  assert.ok(d.body.expected_after_restart >= 2);
  assert.ok((d.body.notes || []).some((n) => n.code === 'E_ALL_STOPPED'));
  assert.match((d.body.notes || []).find((n) => n.code === 'E_ALL_STOPPED').fix, /do not bulk-start/);
  for (const id of ['ghost-a', 'ghost-b']) await c.call('delete_site', { id, confirm: id });
});

test('create_site allocates a port, serves 200 and matches the title', async () => {
  const r = await c.call('create_site', GOOD);
  assert.equal(r.isError, false, JSON.stringify(r.body.errors || r.body));
  assert.equal(r.body.verified.status, 200);
  assert.equal(r.body.verified.titleMatch, true);
  assert.equal(r.body.files.length, 4);
  assert.ok(r.body.bytes < 48 * 1024);
  assert.ok(fs.existsSync(path.join(SITES, GOOD.id, 'sitebox.json')));
  assert.ok(fs.existsSync(path.join(SITES, GOOD.id, 'public/index.html')));
});

test('the same payload is a noop, different content is a conflict', async () => {
  const same = await c.call('create_site', GOOD);
  assert.equal(same.isError, false);
  assert.equal(same.body.noop, true);
  const diff = await c.call('create_site', Object.assign({}, GOOD, { accent: 'terracotta' }));
  assert.equal(diff.isError, true);
  assert.equal(diff.body.errors[0].code, 'E_ID_TAKEN');
});

test('an interrupted create resumes instead of blocking its own retry', async () => {
  const payload = Object.assign({}, GOOD, { id: 'gate-resume', name: 'Gate Resume' });
  const reg = await new Promise((resolve) => {
    const req = require('http').request(stub.api + '/api/sites', { method: 'POST', headers: { 'content-type': 'application/json' } }, (res) => {
      let d = ''; res.on('data', (x) => (d += x)); res.on('end', () => resolve({ status: res.statusCode, json: JSON.parse(d) }));
    });
    req.write(JSON.stringify({ id: 'gate-resume', name: 'Gate Resume', port: 0, host: '127.0.0.1', path: 'sites/gate-resume' }));
    req.end();
  });
  assert.equal(reg.status, 200);
  const r = await c.call('create_site', payload);
  assert.equal(r.isError, false, JSON.stringify(r.body.errors || r.body));
  assert.equal(r.body.verified.status, 200);
});

test('check_site catches a hand-edited rendered file', async () => {
  const clean = await c.call('check_site', { id: GOOD.id });
  assert.equal(clean.isError, false, JSON.stringify(clean.body.errors || clean.body));
  fs.appendFileSync(path.join(SITES, GOOD.id, 'public/index.html'), '<!-- hand edit -->\n');
  const dirty = await c.call('check_site', { id: GOOD.id });
  assert.equal(dirty.isError, true);
  assert.ok(dirty.body.errors.some((e) => e.code === 'E_DRIFT'));
  const fixed = await c.call('update_site', { id: GOOD.id, accent: 'navy' });
  assert.equal(fixed.isError, false, JSON.stringify(fixed.body.errors || fixed.body));
  const after = await c.call('check_site', { id: GOOD.id });
  assert.equal(after.isError, false);
  assert.equal(after.body.errors, undefined);
});

test('site_ctl stops and starts, list_sites reports real reachability', async () => {
  await c.call('site_ctl', { id: GOOD.id, action: 'stop' });
  const down = await c.call('list_sites', { id: GOOD.id });
  assert.equal(down.body.sites[0].reachable, false);
  const up = await c.call('site_ctl', { id: GOOD.id, action: 'start' });
  assert.equal(up.body.live.status, 200);
});

test('delete_site refuses a wrong confirm and purges on the right one', async () => {
  const wrong = await c.call('delete_site', { id: GOOD.id, confirm: 'other' });
  assert.equal(wrong.isError, true);
  assert.equal(wrong.body.errors[0].code, 'E_CONFIRM');
  const done = await c.call('delete_site', { id: GOOD.id, confirm: GOOD.id });
  assert.equal(done.isError, false);
  assert.equal(fs.existsSync(path.join(SITES, GOOD.id)), false);
});

/* ── capability gate ── */
test('an older dashboard is refused instead of guessing at a 400', async () => {
  const oldStub = await startStub('health,host');
  const oldClient = mcp(oldStub.api);
  await oldClient.rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'old', version: '1' } });
  const r = await oldClient.call('create_site', Object.assign({}, GOOD, { id: 'gate-old' }));
  assert.equal(r.isError, true);
  assert.equal(r.body.errors[0].code, 'E_TOO_OLD');
  const d = await oldClient.call('doctor', {});
  assert.equal(d.isError, true);
  oldClient.close();
  oldStub.close();
});

test('stdout carries only JSON-RPC frames', () => {
  const out = c.stderr();
  assert.ok(!/"jsonrpc"/.test(out), 'stderr must not contain protocol frames');
});
