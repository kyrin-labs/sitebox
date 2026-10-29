'use strict';
/**
 * The URL is a promise: if the card prints it, it must open.
 *
 * This guard exists because the first version replaced "localhost" with the browser's
 * hostname for every site — including sites bound to 127.0.0.1 — so the link the UI
 * offered was dead on any other device. A display decision with a security-adjacent
 * consequence belongs in one pure function with a test, not inline in a template.
 */
const { test } = require('node:test');
const assert = require('node:assert');
const { displayUrl, urlHint } = require('../public/js/site-url.js');

test('a loopback-bound site keeps localhost even when the dashboard is opened from the LAN', () => {
  const site = { port: 4451, host: '127.0.0.1', _localOnly: true };
  assert.equal(displayUrl(site, '192.168.1.50'), 'http://localhost:4451');
  assert.equal(displayUrl(site, 'box.lan'), 'http://localhost:4451');
});

test('a LAN-bound site uses the host the person already opened, never a guessed one', () => {
  const site = { port: 4600, host: '0.0.0.0', _localOnly: false };
  assert.equal(displayUrl(site, 'box.lan'), 'http://box.lan:4600');
  assert.equal(displayUrl(site, '127.0.0.1'), 'http://127.0.0.1:4600');
});

test('an entry with no host field is LAN-capable, matching the server default (0.0.0.0)', () => {
  assert.equal(displayUrl({ port: 4452, url: 'http://localhost:4452' }, 'box.lan'), 'http://box.lan:4452');
});

test('a site without a usable port falls back to its registered URL instead of inventing :0', () => {
  assert.equal(displayUrl({ port: 0, url: 'http://localhost:4500' }, 'box.lan'), 'http://localhost:4500');
  assert.equal(displayUrl({}, 'box.lan'), '');
});

test('the hint says where the URL works, so a local-only link is never offered as portable', () => {
  assert.match(urlHint({ _localOnly: true }), /only on the machine running SiteBox/);
  assert.match(urlHint({ _localOnly: false }), /LAN/);
  assert.doesNotMatch(urlHint({ _localOnly: false }), /only on the machine/);
});
