'use strict';
/**
 * Auth gates. Runs with `node --test dashboard/test/auth.test.js`.
 * SITEBOX_DATA is redirected to a temp folder before anything is required, so this
 * never touches the real token store.
 */
const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'sitebox-auth-'));
process.env.SITEBOX_DATA = tmp;

const store = require('../tokenstore');
const auth = require('../auth');

const mkres = () => {
  const r = { status: 0, body: null, headers: {}, _finish: [] };
  r.setHeader = (k, v) => { r.headers[k.toLowerCase()] = v; };
  r.once = (ev, fn) => { if (ev === 'finish') r._finish.push(fn); };
  r.finish = () => { const fns = r._finish; r._finish = []; fns.forEach((fn) => fn()); };
  return r;
};
const send = (res, status, data) => { res.status = status; res.statusCode = status; res.body = data; };
const req = (o) => ({ method: 'GET', url: '/api/sites', headers: {}, socket: { remoteAddress: '::ffff:127.0.0.1' }, ...o });

async function guard(o, pathname) {
  const res = mkres();
  const url = new URL('http://box' + (o.url || pathname || '/api/sites'));
  const handled = await auth.guard(req(o), res, send, url, o.method || 'GET', pathname || url.pathname);
  return { handled, res };
}

test('fail closed: no token store means 401, not open', async () => {
  const g = await guard({});
  assert.equal(g.handled, true);
  assert.equal(g.res.status, 401);
});

test('a minted token authenticates over Bearer and is stored as a hash only', async () => {
  const key = store.createToken('mcp', 'write');
  assert.match(key, /^sbx_[A-Za-z0-9_-]{20,}$/);
  const raw = fs.readFileSync(store.TOKENS, 'utf8');
  assert.ok(!raw.includes(key), 'the plaintext key must never reach disk');
  assert.equal(fs.statSync(store.TOKENS).mode & 0o777, 0o600);

  const g = await guard({ headers: { authorization: 'Bearer ' + key } });
  assert.equal(g.handled, false, 'a valid write token may write');
  assert.equal(g.res.status, 0);
});

test('a wrong or revoked token is 401', async () => {
  const bad = await guard({ headers: { authorization: 'Bearer sbx_wrongwrongwrongwrong' } });
  assert.equal(bad.res.status, 401);
  store.revokeToken('mcp');
  assert.equal(store.verify('sbx_nope'), null);
});

test('a broken store file fails closed', async () => {
  fs.writeFileSync(store.TOKENS, '{ not json', { mode: 0o600 });
  const g = await guard({ headers: { authorization: 'Bearer sbx_anythingatall1234567' } });
  assert.equal(g.res.status, 401);
  fs.rmSync(store.TOKENS);
});

test('read scope cannot write, but can read', async () => {
  const ro = store.createToken('ui-read', 'read');
  const read = await guard({ headers: { authorization: 'Bearer ' + ro } }, '/api/sites');
  assert.equal(read.handled, false);
  const write = await guard({ method: 'POST', headers: { authorization: 'Bearer ' + ro } }, '/api/sites');
  assert.equal(write.res.status, 403);
});

test('Bearer ignores Origin (a header is never attached by a browser form)', async () => {
  const k = store.createToken('mcp2', 'write');
  const g = await guard({ method: 'POST', headers: { authorization: 'Bearer ' + k } }, '/api/sites');
  assert.equal(g.handled, false);
});

test('cookie session: safe method allowed, cross-origin write refused, same-origin write allowed', async () => {
  const k = store.createToken('ui', 'write');
  const secret = store.createSession('ui');
  const ck = { cookie: store.COOKIE + '=' + secret };

  const read = await guard({ headers: ck }, '/api/sites');
  assert.equal(read.handled, false, 'GET with a cookie must pass (this is the favicon path)');

  const noOrigin = await guard({ method: 'POST', headers: ck }, '/api/sites');
  assert.equal(noOrigin.res.status, 403, 'cookie + write + no Origin is refused');

  const goodOrigin = await guard({ method: 'POST', headers: { ...ck, host: 'box', origin: 'http://box' } }, '/api/sites');
  assert.equal(goodOrigin.handled, false);

  const badOrigin = await guard({ method: 'POST', headers: { ...ck, host: 'box', origin: 'http://evil.example' } }, '/api/sites');
  assert.equal(badOrigin.res.status, 403);
  assert.ok(k);
});

test('revoking a token drops its sessions immediately', async () => {
  const k = store.createToken('temp', 'write');
  const secret = store.createSession('temp');
  store.revokeToken('temp');
  const g = await guard({ headers: { cookie: store.COOKIE + '=' + secret } }, '/api/sites');
  assert.equal(g.res.status, 401);
  assert.equal(store.verify(k), null);
});

test('login endpoint exchanges a token for an HttpOnly SameSite=Strict cookie', async () => {
  const k = store.createToken('login', 'write');
  const res = mkres();
  const body = JSON.stringify({ token: k });
  const r = { method: 'POST', url: '/api/session', headers: {}, on: (ev, fn) => { if (ev === 'data') fn(body); if (ev === 'end') fn(); } };
  const handled = await auth.guard(r, res, send, new URL('http://box/api/session'), 'POST', '/api/session');
  assert.equal(handled, true);
  assert.equal(res.status, 200);
  const cookie = res.headers['set-cookie'];
  assert.match(cookie, /HttpOnly/);
  assert.match(cookie, /SameSite=Strict/);
  assert.match(cookie, /Max-Age=43200/);
});

test('token admin requires write scope and never echoes a key it did not just mint', async () => {
  const ro = store.createToken('ro-admin', 'read');
  const denied = await guard({ headers: { authorization: 'Bearer ' + ro } }, '/api/tokens');
  assert.equal(denied.res.status, 403);

  const rw = store.createToken('rw-admin', 'write');
  const listed = await guard({ headers: { authorization: 'Bearer ' + rw } }, '/api/tokens');
  assert.equal(listed.handled, true);
  assert.ok(Array.isArray(listed.res.body.tokens));
  for (const t of listed.res.body.tokens) {
    assert.deepEqual(Object.keys(t).sort(), ['created', 'label', 'lastUsed', 'scope']);
  }
});

test('the host CLI is the recovery path and the key is the last line of stdout', () => {
  const cli = path.join(__dirname, '..', 'token.js');
  const out = execFileSync('node', [cli, 'add', 'cli-test', '--scope', 'write', '--print'], {
    env: { ...process.env, SITEBOX_DATA: tmp }, encoding: 'utf8',
  });
  const lines = out.trim().split('\n');
  assert.match(lines[lines.length - 1], /^sbx_/);
  assert.equal(store.verify(lines[lines.length - 1]).label, 'cli-test');

  const listed = execFileSync('node', [cli, 'list'], { env: { ...process.env, SITEBOX_DATA: tmp }, encoding: 'utf8' });
  assert.match(listed, /cli-test/);
  const revoked = execFileSync('node', [cli, 'revoke', 'cli-test'], { env: { ...process.env, SITEBOX_DATA: tmp }, encoding: 'utf8' });
  assert.match(revoked, /revoked cli-test/);
});

test('the dashboard page issues a UI session with no token, and it is write-scoped', () => {
  const res = mkres();
  assert.equal(auth.issueUiSession(req({ url: '/' }), res), true);
  const setCookie = res.headers['set-cookie'];
  assert.match(setCookie, /^sbx_session=/);
  assert.match(setCookie, /HttpOnly/);
  assert.match(setCookie, /SameSite=Strict/);
  const secret = decodeURIComponent(setCookie.split(';')[0].split('=')[1]);
  const who = auth.authenticate(req({ headers: { cookie: store.COOKIE + '=' + encodeURIComponent(secret) } }));
  assert.equal(who.ok, true);
  assert.equal(who.ui, true);
  assert.equal(who.scope, 'session', 'a UI session must not claim a token scope it cannot use');
  // A second load must not mint a second session for the same browser.
  const again = mkres();
  assert.equal(auth.issueUiSession(req({ url: '/', headers: { cookie: store.COOKIE + '=' + encodeURIComponent(secret) } }), again), false);
  assert.equal(again.headers['set-cookie'], undefined);
});

test('revoking an MCP token never logs the browser out', () => {
  const res = mkres();
  auth.issueUiSession(req({ url: '/' }), res);
  const secret = decodeURIComponent(res.headers['set-cookie'].split(';')[0].split('=')[1]);
  const key = store.createToken('agent', 'write');
  store.revokeToken('agent');
  assert.equal(store.verify(key), null, 'the key is dead');
  const who = auth.authenticate(req({ headers: { cookie: store.COOKIE + '=' + encodeURIComponent(secret) } }));
  assert.equal(who.ok, true, 'the UI session survives');
});

test('the UI session does not remove the token gate for clients', async () => {
  const g = await guard({});
  assert.equal(g.handled, true);
  assert.equal(g.res.status, 401);
  // A UI cookie on an unsafe method still needs a matching Origin.
  const res = mkres();
  auth.issueUiSession(req({ url: '/' }), res);
  const secret = decodeURIComponent(res.headers['set-cookie'].split(';')[0].split('=')[1]);
  const post = await guard({ method: 'POST', headers: { cookie: store.COOKIE + '=' + encodeURIComponent(secret) } });
  assert.equal(post.res.status, 403);
});

/* ── LAN blast-radius guard ── */
function uiCookie() {
  const res = mkres();
  auth.issueUiSession(req({ url: '/' }), res);
  const secret = decodeURIComponent(res.headers['set-cookie'].split(';')[0].split('=')[1]);
  return { cookie: store.COOKIE + '=' + encodeURIComponent(secret), host: '192.168.1.102:4445', origin: 'http://192.168.1.102:4445' };
}
const fromLan = { socket: { remoteAddress: '192.168.1.50' } };

test('a LAN visitor may write: the network boundary is the gate, not the device', async () => {
  const ck = uiCookie();
  const read = await guard({ headers: ck, ...fromLan }, '/api/sites');
  assert.equal(read.handled, false, 'reading stays open to the LAN');
  const create = await guard({ method: 'POST', headers: ck, ...fromLan }, '/api/sites');
  assert.equal(create.handled, false, 'create is allowed from the LAN');
  const del = await guard({ method: 'DELETE', headers: ck, ...fromLan }, '/api/sites/relay');
  assert.equal(del.handled, false, 'delete is allowed from the LAN');
  const reorder = await guard({ method: 'POST', headers: ck, ...fromLan }, '/api/sites/reorder');
  assert.equal(reorder.handled, false, 'reorder is allowed from the LAN');
});

test('every write leaves exactly one audit line, and the actor names the credential', async () => {
  const ck = uiCookie();
  const audit = require('../audit');
  const before = fs.existsSync(audit.LOG) ? fs.readFileSync(audit.LOG, 'utf8').trim().split('\n').length : 0;
  const first = await guard({ method: 'POST', headers: ck, ...fromLan }, '/api/sites/reorder');
  first.res.finish(); // the line lands when the response finishes, with its real status
  const lines = fs.readFileSync(audit.LOG, 'utf8').trim().split('\n');
  assert.equal(lines.length, before + 1, 'one write must be exactly one line');
  const row = JSON.parse(lines[lines.length - 1]);
  assert.equal(row.method, 'POST');
  assert.equal(row.path, '/api/sites/reorder');
  assert.match(row.actor, /^session:[0-9a-f]{6}\|192\.168\.1\.50\|/);

  // A bearer token is named by its label — the only real identity in the system.
  const key = store.createToken('audit-bearer', 'write');
  const second = await guard({ method: 'POST', headers: { authorization: 'Bearer ' + key }, ...fromLan }, '/api/sites/reorder');
  second.res.finish();
  const after = fs.readFileSync(audit.LOG, 'utf8').trim().split('\n');
  const bearerRow = JSON.parse(after[after.length - 1]);
  assert.equal(bearerRow.actor, 'token:audit-bearer');

  // The log itself must never carry a credential.
  const raw = fs.readFileSync(audit.LOG, 'utf8');
  assert.ok(!/sbx_[A-Za-z0-9_-]{20,}/.test(raw), 'no key material');
  assert.ok(!/sbx_session=/.test(raw), 'no cookie value');
  assert.ok(!/Bearer\s/.test(raw), 'no Authorization header');
});

test('delete fails closed when the audit log cannot be written', async () => {
  const audit = require('../audit');
  const original = { record: audit.record, assertWritable: audit.assertWritable };
  audit.record = () => { throw new Error('disk full'); };
  audit.assertWritable = () => { throw new Error('disk full'); };
  try {
    const ck = uiCookie();
    const g = await guard({ method: 'DELETE', headers: ck, ...fromLan }, '/api/sites/relay');
    assert.equal(g.handled, true);
    assert.equal(g.res.status, 503, 'no record, no delete');
    // Other writes are best-effort: a broken log must not become a new outage.
    const other = await guard({ method: 'POST', headers: ck, ...fromLan }, '/api/sites/relay/start');
    assert.equal(other.handled, false);
  } finally {
    audit.record = original.record;
    audit.assertWritable = original.assertWritable;
  }
});

test('a forwarded-for header cannot fake loopback', async () => {
  const ck = uiCookie();
  const spoof = await guard({
    method: 'POST',
    headers: { ...ck, 'x-forwarded-for': '127.0.0.1', 'x-real-ip': '127.0.0.1' },
    socket: { remoteAddress: '192.168.1.50' },
  }, '/api/sites');
  assert.equal(spoof.handled, false, 'the header neither grants nor denies anything now');
  spoof.res.finish(); // the line lands on finish now
  const audit = require('../audit');
  const row = JSON.parse(fs.readFileSync(audit.LOG, 'utf8').trim().split('\n').pop());
  assert.match(row.actor, /192\.168\.1\.50/, 'the recorded address is the socket, never the header');
});

test('loopback counts in both of the forms Node actually reports', async () => {
  const ck = uiCookie();
  for (const addr of ['127.0.0.1', '::1', '::ffff:127.0.0.1']) {
    const g = await guard({ method: 'POST', headers: ck, socket: { remoteAddress: addr } }, '/api/sites');
    assert.equal(g.handled, false, addr + ' must be treated as local');
  }
  // Same cookie, same method, from the network: allowed too. The address no longer
  // grants privilege — it is recorded, which is what audit.log is for.
  const lan = await guard({ method: 'POST', headers: ck, socket: { remoteAddress: '192.168.1.50' } }, '/api/sites');
  assert.equal(lan.handled, false);
});

test('a write-scoped Bearer token is exempt, so an agent may live on another machine', async () => {
  const key = store.createToken('agent-lan', 'write');
  const g = await guard({
    method: 'POST',
    headers: { authorization: 'Bearer ' + key },
    socket: { remoteAddress: '192.168.1.50' },
  }, '/api/sites');
  assert.equal(g.handled, false);

  const ro = store.createToken('agent-ro', 'read');
  const denied = await guard({
    method: 'POST',
    headers: { authorization: 'Bearer ' + ro },
    socket: { remoteAddress: '192.168.1.50' },
  }, '/api/sites');
  assert.equal(denied.res.status, 403);
});

test('a UI session reports session scope but can still mint from the loopback socket', async () => {
  const res = mkres();
  auth.issueUiSession(req({ url: '/' }), res);
  const cookie = store.COOKIE + '=' + res.headers['set-cookie'].split(';')[0].split('=')[1];
  const who = auth.authenticate(req({ headers: { cookie } }));
  assert.equal(who.scope, 'session');

  // A POST body is parsed by the route, so the fake request needs a stream stub.
  const body = (extra) => ({
    ...req(extra),
    on: (ev, cb) => { if (ev === 'end') setTimeout(cb, 0); },
  });
  const local = body({
    method: 'POST',
    url: '/api/tokens',
    headers: { cookie, origin: 'http://box', host: 'box' },
    socket: { remoteAddress: '::ffff:127.0.0.1' },
  });
  const out = mkres();
  const handled = await auth.guard(local, out, send, new URL('http://box/api/tokens'), 'POST', '/api/tokens');
  assert.equal(handled, true, 'the token route answers inside the guard');
  assert.notEqual(out.status, 403, 'loopback UI must not be refused by the write gate');
});

test('a LAN UI cookie may start and delete alike', async () => {
  const res = mkres();
  auth.issueUiSession(req({ url: '/' }), res);
  const cookie = store.COOKIE + '=' + res.headers['set-cookie'].split(';')[0].split('=')[1];
  const lan = (method, pathname) => req({
    method, url: pathname,
    headers: { cookie, origin: 'http://box', host: 'box' },
    socket: { remoteAddress: '192.168.1.50' },
  });
  const start = await auth.guard(lan('POST', '/api/sites/x/start'), mkres(), send, new URL('http://box/api/sites/x/start'), 'POST', '/api/sites/x/start');
  assert.equal(start, false, 'start is allowed from the LAN');
  const del = await auth.guard(lan('DELETE', '/api/sites/x'), mkres(), send, new URL('http://box/api/sites/x'), 'DELETE', '/api/sites/x');
  assert.equal(del, false, 'delete is allowed from the LAN too — the brake lives in the UI');
});

test('GET /api/audit shows the newest lines to any authenticated caller, with no secrets', async () => {
  const audit = require('../audit');
  const key = store.createToken('audit-reader', 'read');
  // Seed lines whose request carries a cookie value that must never reach the response.
  const leaky = 'sbx_session=supersecretcookievaluethatmustneverappear123456';
  for (let i = 0; i < 2; i++) {
    audit.record({ who: { via: 'bearer', label: 'seed' }, req: req({ headers: { cookie: leaky } }), method: 'POST', path: '/api/sites' });
  }

  const asToken = await guard({ headers: { authorization: 'Bearer ' + key } }, '/api/audit');
  assert.equal(asToken.handled, true);
  assert.equal(asToken.res.status, 200);
  assert.ok(asToken.res.body.lines.length >= 2);
  assert.ok(asToken.res.body.lines.every((l) => l.time && l.actor && l.method && l.path));
  assert.ok(!JSON.stringify(asToken.res.body).includes('supersecretcookievalue'),
    'the cookie value must never appear in the audit response');

  // The keys pane is a browser surface, so a session must be able to read it as well.
  const res = mkres();
  auth.issueUiSession(req({ url: '/' }), res);
  const cookie = store.COOKIE + '=' + res.headers['set-cookie'].split(';')[0].split('=')[1];
  const asSession = await guard({ headers: { cookie }, socket: { remoteAddress: '192.168.1.50' } }, '/api/audit');
  assert.equal(asSession.res.status, 200);
  assert.ok(Array.isArray(asSession.res.body.lines));

  // ...but an anonymous caller cannot.
  const anon = await guard({}, '/api/audit');
  assert.equal(anon.res.status, 401);
});

/* ── everything that happens at the point of refusal ── */
const audit = require('../audit');
const auditLines = () => {
  try { return fs.readFileSync(audit.LOG, 'utf8').split('\n').filter(Boolean); } catch { return []; }
};

test('a refused write leaves exactly one audit line that says why', async () => {
  const ck = uiCookie();
  const before = auditLines().length;
  const res = mkres();
  await auth.guard(req({
    method: 'POST',
    url: '/api/sites',
    headers: { cookie: ck.cookie, host: '192.168.1.102:4445', origin: 'http://evil.example' },
    socket: { remoteAddress: '192.168.1.50' },
  }), res, send, new URL('http://box/api/sites'), 'POST', '/api/sites');

  const after = auditLines();
  assert.equal(after.length, before + 1, 'a blocked CSRF attempt must not be invisible');
  const rec = JSON.parse(after[after.length - 1]);
  assert.equal(rec.status, 403);
  assert.equal(rec.actor, 'denied:origin');
  assert.equal(rec.path, '/api/sites');
  assert.equal(rec.method, 'POST');
  assert.equal(res.status, 403);
});

test('an unauthenticated write is recorded as anon with 401', async () => {
  const before = auditLines().length;
  const res = mkres();
  await auth.guard(req({ method: 'DELETE', url: '/api/sites/relay' }), res, send, new URL('http://box/api/sites/relay'), 'DELETE', '/api/sites/relay');
  const after = auditLines();
  assert.equal(after.length, before + 1);
  const rec = JSON.parse(after[after.length - 1]);
  assert.equal(rec.status, 401);
  assert.equal(rec.actor, 'anon');
  assert.equal(rec.path, '/api/sites/relay');
});

test('an allowed write records the status the client actually got', async () => {
  const key = store.createToken('status-probe', 'write');
  const res = mkres();
  const handled = await auth.guard(
    req({ method: 'POST', url: '/api/sites/reorder', headers: { authorization: 'Bearer ' + key } }),
    res, send, new URL('http://box/api/sites/reorder'), 'POST', '/api/sites/reorder',
  );
  assert.equal(handled, false, 'the route still has to run');
  assert.equal(auditLines().pop().includes('reorder'), false, 'nothing is written before the handler answers');

  res.status = 200;
  res.statusCode = 200;
  res.finish();
  const rec = JSON.parse(auditLines().pop());
  assert.equal(rec.status, 200);
  assert.equal(rec.actor, 'token:status-probe');
  assert.equal(rec.path, '/api/sites/reorder');
});
