'use strict';
/**
 * The keys pane and the site card are static files, so the only honest guard is one
 * that reads the shipped artifact. Each test here pins a decision that was made
 * because its absence cost a session:
 *
 *   - the Keys button is an icon with an accessible name, not icon + the same word;
 *   - the default key scope is the one an MCP client needs, and the labels say that
 *     write includes read instead of implying a choice that does not exist;
 *   - the URL logic is loaded and used, so no template re-introduces a hostname guess.
 */
const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');

const PUB = path.join(__dirname, '..', 'public');
const html = fs.readFileSync(path.join(PUB, 'index.html'), 'utf8');
const main = fs.readFileSync(path.join(PUB, 'js', 'main.js'), 'utf8');

test('the Keys button is icon-only and still has a name a screen reader can say', () => {
  const btn = html.match(/<button id="btn-keys"[\s\S]*?<\/button>/);
  assert.ok(btn, 'the keys button must exist');
  assert.match(btn[0], /aria-label="[^"]+"/, 'an icon-only button needs an accessible name');
  assert.match(btn[0], /title="[^"]+"/, 'and a visible tooltip');
  const inner = btn[0].slice(btn[0].indexOf('>') + 1, btn[0].lastIndexOf('</button>'));
  const text = inner.replace(/<svg[\s\S]*?<\/svg>/g, '').replace(/<!--[\s\S]*?-->/g, '').trim();
  assert.equal(text, '', 'the word "Keys" beside a key icon is duplication, not a label');
});

test('the default key scope is read + write, because the pane exists for MCP clients', () => {
  const select = html.match(/<select id="key-scope"[\s\S]*?<\/select>/);
  assert.ok(select, 'the scope control must exist');
  assert.match(select[0], /<option value="write" selected>/, 'MCP needs write; read-only as the default produces a 403 on the first create');
  assert.doesNotMatch(select[0], /<option value="read" selected>/);
  assert.match(select[0], /read \+ write/, 'the write label must say it includes read');
  assert.match(select[0], /read only/);
  assert.match(html, /write includes read/, 'the pane must explain that one key covers both');
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
