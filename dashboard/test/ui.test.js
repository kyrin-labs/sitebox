'use strict';
/**
 * The dashboard is static files, so the only honest guard is one that reads the
 * shipped artifact. Each test here pins a decision that was made because its
 * absence cost a session:
 *
 *   - there is no key/token UI: the Rust dashboard has no token store, and a
 *     button that calls a missing endpoint is worse than no button;
 *   - buttons use one Lucide line set, not emoji;
 *   - the URL logic is loaded and used, so no template re-introduces a hostname guess.
 */
const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');

const PUB = path.join(__dirname, '..', 'public');
const html = fs.readFileSync(path.join(PUB, 'index.html'), 'utf8');
const main = fs.readFileSync(path.join(PUB, 'js', 'main.js'), 'utf8');
const studio = fs.readFileSync(path.join(PUB, 'js', 'studio.js'), 'utf8');

test('the dead key and audit UI is gone', () => {
  assert.ok(!html.includes('btn-keys'), 'no keys button');
  assert.ok(!html.includes('keys-modal'), 'no keys modal');
  assert.ok(!main.includes('/api/tokens'), 'no token endpoint call');
  assert.ok(!main.includes('/api/me'), 'no capability endpoint call');
  assert.ok(!main.includes('/api/audit'), 'no dashboard audit call');
});

test('buttons use Lucide geometry, not emoji', () => {
  const emoji = /[\u{1F300}-\u{1FAFF}\u2700-\u27BF\uFF0B]/u;
  assert.doesNotMatch(main, emoji, 'main.js must not use emoji as an icon');
  assert.doesNotMatch(studio, emoji, 'studio.js must not use emoji as an icon');
  assert.match(studio, /function icon\(name/, 'the panel renders icons through one helper');
  assert.match(main, /<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg"/, 'cards use inline SVG');
});

test('the page loads the URL decision before the code that renders it', () => {
  const urlAt = html.indexOf('/js/site-url.js');
  const mainAt = html.indexOf('/js/main.js');
  assert.ok(urlAt > 0 && mainAt > urlAt, 'site-url.js must load first');
  assert.match(main, /SiteUrl\.displayUrl\(/, 'the card must use the shared decision');
  assert.ok(!main.includes('replace(/localhost'),
    'the browser must never rewrite a URL that sites.json did not authorise');
});

test('the site card offers the URL as copyable text, not only an Open button', () => {
  assert.match(main, /class="site-card__url"/);
  assert.match(main, /data-copy="\$\{esc\(openUrl\)\}"/);
  assert.match(main, /aria-label="Copy URL for /);
  assert.ok(!main.includes('<code>:${s.port}</code>'), 'a bare port is not a URL anyone can paste');
});
