'use strict';
/**
 * render.js — schema in, files out. Pure: no Date, no random, no I/O reads of the
 * site folder. Given the same schema it must produce byte-identical output on any
 * machine, twice in a row and across processes. `check_site` re-renders and compares
 * hashes against disk; impurity here would make E_DRIFT fire forever.
 */
const fs = require('fs');
const path = require('path');
const { ACCENTS, INK, ON_ACCENT, hashSchema, KNOBS, KNOB_DEFAULTS } = require('./schema');

/** Typed knobs → a few computed numbers. Deterministic by construction. */
function knobCss(raw) {
  const k = Object.assign({}, KNOB_DEFAULTS, raw || {});
  const sp = { airy: 1.3, normal: 1, compact: 0.82 }[k.density] || 1;
  const fs = { s: 16, m: 17, l: 18 }[k.typeScale] || 17;
  const rad = { square: 0, soft: 3, round: 10 }[k.radius] || 3;
  const gap = { airy: 20, normal: 14, compact: 10 }[k.density] || 14;
  const out = [
    ':root { --radius: ' + rad + 'px; }',
    'body { font-size: ' + fs + 'px; }',
    'section { padding: ' + Math.round(44 * sp) + 'px 0 ' + Math.round(8 * sp) + 'px; }',
    '.grid { gap: ' + gap + 'px; }',
    '.grid li, .cta, .steps li::before, details { border-radius: var(--radius); }',
  ];
  if (k.hero === 'band') {
    out.push('.masthead { margin-top: 24px; padding: 44px 28px; background: color-mix(in srgb, var(--accent) 7%, var(--paper)); border: 1px solid var(--rule); border-radius: var(--radius); }');
  }
  if (k.rules === 'double') {
    out.push('.masthead { border-bottom: 3px double var(--rule); }');
    out.push('.colophon { border-top: 3px double var(--rule); }');
  }
  if (k.rules === 'none') {
    out.push('.masthead, .colophon, details, th, td { border-color: transparent; }');
    out.push('tbody tr:nth-child(odd) { background: color-mix(in srgb, var(--ink) 3%, var(--paper)); }');
  }
  if (k.texture === 'paper') {
    out.push('body { background-image: radial-gradient(color-mix(in srgb, var(--ink) 4%, transparent) 0.5px, transparent 0.5px); background-size: 5px 5px; }');
  }
  return out.join('\n') + '\n';
}

const SERVER_TEMPLATE = fs.readFileSync(path.join(__dirname, 'templates', 'server.js'), 'utf8');

const esc = (s) => String(s == null ? '' : s)
  .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;').replace(/'/g, '&#39;');

const items = (s) => (Array.isArray(s.items) ? s.items : []);

function sectionHtml(s) {
  const h = s.heading ? '  <h2>' + esc(s.heading) + '</h2>\n' : '';
  const b = s.body ? '  <p class="lede">' + esc(s.body) + '</p>\n' : '';
  switch (s.type) {
    case 'hero':
      return '<section class="hero">\n  <h2 class="hero__h">' + esc(s.heading || '') + '</h2>\n'
        + (s.body ? '  <p class="hero__p">' + esc(s.body) + '</p>\n' : '') + '</section>\n';
    case 'features':
      return '<section class="features">\n' + h + b + '  <ul class="grid">\n'
        + items(s).map((i) => '    <li><h3>' + esc(i.label) + '</h3>'
          + (i.text ? '<p>' + esc(i.text) + '</p>' : '') + '</li>').join('\n')
        + '\n  </ul>\n</section>\n';
    case 'steps':
      return '<section class="steps">\n' + h + b + '  <ol>\n'
        + items(s).map((i) => '    <li><h3>' + esc(i.label) + '</h3>'
          + (i.text ? '<p>' + esc(i.text) + '</p>' : '') + '</li>').join('\n')
        + '\n  </ol>\n</section>\n';
    case 'table': {
      const cols = Array.isArray(s.columns) ? s.columns : ['', ''];
      return '<section class="table">\n' + h + b
        + '  <table class="sortable">\n    <thead><tr>'
        + '<th scope="col">' + esc(cols[0] || '') + '</th>'
        + '<th scope="col">' + esc(cols[1] || '') + '</th></tr></thead>\n'
        + '    <tbody>\n'
        + items(s).map((i) => '      <tr><td>' + esc(i.label) + '</td><td>' + esc(i.text || '') + '</td></tr>').join('\n')
        + '\n    </tbody>\n  </table>\n</section>\n';
    }
    case 'faq':
      return '<section class="faq">\n' + h + b
        + items(s).map((i) => '  <details><summary>' + esc(i.label) + '</summary>'
          + (i.text ? '<p>' + esc(i.text) + '</p>' : '') + '</details>').join('\n')
        + '\n</section>\n';
    case 'cta':
      return '<section class="cta">\n' + h + b + '</section>\n';
    case 'footer-note':
      return '<section class="note">\n' + (h || '') + b + '</section>\n';
    default:
      return '';
  }
}

function styleCss(accent, paper) {
  const a = ACCENTS[accent] || ACCENTS['ink-green'];
  return `:root {
  --accent: ${a.hex};
  --paper: ${paper};
  --ink: ${INK};
  --on-accent: ${ON_ACCENT};
  --rule: color-mix(in srgb, var(--ink) 14%, transparent);
  --muted: color-mix(in srgb, var(--ink) 62%, var(--paper));
  --font: -apple-system, "Segoe UI", Roboto, "Noto Sans Thai", "Helvetica Neue", Arial, sans-serif;
  --measure: 68ch;
}
* { box-sizing: border-box; }
html { -webkit-text-size-adjust: 100%; }
body {
  margin: 0; padding: 0 20px 96px; background: var(--paper); color: var(--ink);
  font-family: var(--font); font-size: 17px; line-height: 1.65;
  font-kerning: normal; text-rendering: optimizeLegibility;
}
main, .masthead, .colophon { max-width: var(--measure); margin: 0 auto; }
.masthead { padding: 72px 0 40px; border-bottom: 1px solid var(--rule); }
.eyebrow { margin: 0 0 12px; font-size: 13px; letter-spacing: .14em; text-transform: uppercase; color: var(--accent); font-weight: 650; }
h1 { margin: 0; font-size: clamp(30px, 5vw, 44px); line-height: 1.15; letter-spacing: -0.01em; font-weight: 700; }
.tagline { margin: 16px 0 0; font-size: 19px; color: var(--muted); max-width: 54ch; }
section { padding: 44px 0 8px; }
h2 { margin: 0 0 10px; font-size: 22px; line-height: 1.3; font-weight: 650; }
h3 { margin: 0 0 6px; font-size: 17px; font-weight: 650; }
p { margin: 0 0 14px; }
.lede { color: var(--muted); max-width: 58ch; }
.grid { list-style: none; margin: 22px 0 0; padding: 0; display: grid; gap: 14px; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); }
.grid li { padding: 18px 18px 14px; border: 1px solid var(--rule); border-radius: 3px; background: color-mix(in srgb, var(--ink) 2%, var(--paper)); }
.grid p { margin: 0; color: var(--muted); font-size: 15.5px; }
.steps ol { margin: 22px 0 0; padding-left: 0; list-style: none; counter-reset: step; }
.steps li { counter-increment: step; padding: 0 0 18px 44px; position: relative; }
.steps li::before { content: counter(step); position: absolute; left: 0; top: 1px; width: 28px; height: 28px; border-radius: 50%; background: var(--accent); color: var(--on-accent); font-size: 14px; font-weight: 700; display: grid; place-items: center; }
.steps p { color: var(--muted); margin: 0; }
table { width: 100%; border-collapse: collapse; margin-top: 18px; font-size: 16px; }
th, td { text-align: left; padding: 10px 12px; border-bottom: 1px solid var(--rule); vertical-align: top; }
th { font-size: 13px; letter-spacing: .06em; text-transform: uppercase; color: var(--muted); cursor: pointer; }
tbody tr:hover { background: color-mix(in srgb, var(--accent) 5%, var(--paper)); }
details { border-bottom: 1px solid var(--rule); padding: 12px 0; }
summary { cursor: pointer; font-weight: 650; }
details p { margin: 10px 0 0; color: var(--muted); }
.cta { margin: 40px 0 8px; padding: 28px 26px; border-left: 4px solid var(--accent); background: color-mix(in srgb, var(--accent) 6%, var(--paper)); }
.cta h2 { font-size: 20px; }
.cta p:last-child { margin: 0; }
.hero { padding: 56px 0 20px; }
.hero__h { font-size: clamp(26px, 4vw, 36px); }
.hero__p { font-size: 19px; color: var(--muted); }
.note { font-size: 15px; color: var(--muted); }
.colophon { margin-top: 56px; padding-top: 18px; border-top: 1px solid var(--rule); font-size: 13.5px; color: var(--muted); }
body[data-layout="dashboard"] { --measure: 1100px; }
body[data-layout="dashboard"] .grid { grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 18px; }
body[data-layout="dashboard"] .grid li { padding: 22px; }
body[data-layout="showcase"] { --measure: 980px; }
body[data-layout="showcase"] .masthead { padding: 116px 0 64px; }
body[data-layout="showcase"] .hero__h { font-size: clamp(32px, 6vw, 54px); }
body[data-layout="list"] { --measure: 80ch; font-size: 16px; }
body[data-layout="list"] .grid { grid-template-columns: 1fr; gap: 0; }
body[data-layout="list"] .grid li { border-width: 0 0 1px; border-radius: 0; background: none; padding: 14px 0; }
body[data-layout="form"] { --measure: 640px; }
body[data-layout="form"] .cta { border-left-width: 0; border: 1px solid var(--rule); }
@media (max-width: 560px) {
  body { font-size: 16px; padding: 0 16px 72px; }
  .masthead { padding: 44px 0 28px; }
  section { padding: 32px 0 4px; }
}
@media (prefers-reduced-motion: reduce) { * { animation: none !important; transition: none !important; } }
`;
}

const APP_JS = `(function () {
  var tables = document.querySelectorAll('table.sortable');
  for (var t = 0; t < tables.length; t++) (function (table) {
    var body = table.tBodies[0];
    if (!body) return;
    var heads = table.querySelectorAll('thead th');
    for (var i = 0; i < heads.length; i++) (function (th, index) {
      th.addEventListener('click', function () {
        var dir = th.dataset.dir === 'asc' ? -1 : 1;
        th.dataset.dir = dir === 1 ? 'asc' : 'desc';
        var rows = Array.prototype.slice.call(body.rows);
        rows.sort(function (a, b) {
          return a.cells[index].textContent.trim().localeCompare(
            b.cells[index].textContent.trim(), document.documentElement.lang || 'en'
          ) * dir;
        });
        for (var r = 0; r < rows.length; r++) body.appendChild(rows[r]);
      });
    })(heads[i], i);
  })(tables[t]);
})();
`;

function render(schema) {
  const hash = hashSchema(schema).slice(0, 8);
  const accent = ACCENTS[schema.accent] ? schema.accent : 'ink-green';
  const paper = ACCENTS[accent].paper;

  const html = '<!doctype html>\n'
    + '<html lang="' + esc(schema.lang || 'th') + '">\n<head>\n'
    + '<meta charset="utf-8">\n'
    + '<meta name="viewport" content="width=device-width, initial-scale=1">\n'
    + '<title>' + esc(schema.title) + '</title>\n'
    + '<meta name="description" content="' + esc(schema.tagline || '') + '">\n'
    + '<link rel="stylesheet" href="style.css?v=' + hash + '">\n'
    + '</head>\n<body data-layout="' + esc(schema.layout) + '">\n'
    + '<header class="masthead">\n'
    + '  <p class="eyebrow">' + esc(schema.name) + '</p>\n'
    + '  <h1>' + esc(schema.title) + '</h1>\n'
    + (schema.tagline ? '  <p class="tagline">' + esc(schema.tagline) + '</p>\n' : '')
    + '</header>\n<main>\n'
    + (schema.sections || []).map(sectionHtml).join('')
    + '</main>\n<footer class="colophon">' + esc(schema.name) + ' \u00b7 SiteBox prototype</footer>\n'
    + '<script defer src="app.js?v=' + hash + '"></script>\n'
    + '</body>\n</html>\n';

  const files = {
    'server.js': SERVER_TEMPLATE,
    'public/index.html': html,
    'public/style.css': styleCss(accent, paper).replace('@media (max-width: 560px)', knobCss(schema) + '@media (max-width: 560px)'),
    'public/app.js': APP_JS,
  };
  return { files, hash, bytes: Object.values(files).reduce((n, f) => n + Buffer.byteLength(f, 'utf8'), 0) };
}

module.exports = { render };
