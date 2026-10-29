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
  const r = { status: 0, body: null, headers: {} };
  r.setHeader = (k, v) => { r.headers[k.toLowerCase()] = v; };
  return r;
};
const send = (res, status, data) => { res.status = status; res.body = data; };
const req = (o) => ({ method: 'GET', url: '/api/sites', headers: {}, ...o });

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
  assert.equal(who.scope, 'write');
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
