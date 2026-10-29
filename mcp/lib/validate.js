'use strict';
/**
 * validate.js — the only place a prototype rule is enforced. `create_site`,
 * `update_site` and `check_site` all call this; there is no second opinion.
 * Every failure carries code + path + fix so one round of fixes is enough.
 */
const crypto = require('crypto');
const {
  ACCENTS, LAYOUTS, SECTION_TYPES, LANGS, KNOBS, CAPS, ID_RE, MARKER,
} = require('./schema');

const HAN = /[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]/;
const PLACEHOLDER = /\u00ab|\u00bb|\b(?:lorem ipsum|your text here|section heading|tagline here|placeholder|tbd|todo|fixme)\b/i;
const EXT_ATTR = /(?:href|src|action|poster)\s*=\s*["'](?:https?:)?\/\//i;
const EXT_CSS = /@import\s+url\(\s*["']?(?:https?:)?\/\/|url\(\s*["']?(?:https?:)?\/\//i;

const short = (s, n) => String(s == null ? '' : s).trim().length > n;

function field(failures, path, value, cap, code) {
  const v = String(value == null ? '' : value).trim();
  if (!v) failures.push({ code: code || 'E_EMPTY', path, fix: 'fill this field with real copy' });
  else if (PLACEHOLDER.test(v)) failures.push({ code: 'E_PLACEHOLDER', path, fix: 'replace the marker or placeholder wording with real content' });
  else if (short(v, cap)) failures.push({ code: 'E_CAP', path, fix: 'shorten to ' + cap + ' characters or fewer (now ' + v.length + ')' });
  return v;
}

/** Shape + copy rules. Returns { ok, failures }. */
function validate(schema) {
  const f = [];
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) {
    return { ok: false, failures: [{ code: 'E_SHAPE', path: '$', fix: 'send a JSON object, not a string or array' }] };
  }

  const id = String(schema.id || '');
  if (!ID_RE.test(id)) f.push({ code: 'E_ID_FORMAT', path: 'id', fix: 'kebab-case: lowercase letters, digits, single hyphens (the dashboard rejects anything else with 400)' });
  else if (id.length > CAPS.id) f.push({ code: 'E_CAP', path: 'id', fix: 'keep id at ' + CAPS.id + ' characters or fewer' });

  if (schema.lang !== undefined && !LANGS.includes(schema.lang)) {
    f.push({ code: 'E_ENUM', path: 'lang', fix: 'one of: ' + LANGS.join(', ') });
  }
  if (!LAYOUTS.includes(schema.layout)) {
    f.push({ code: 'E_ENUM', path: 'layout', fix: 'one of: ' + LAYOUTS.join(', ') });
  }
  if (!ACCENTS[schema.accent]) {
    f.push({ code: 'E_ENUM', path: 'accent', fix: 'one of: ' + Object.keys(ACCENTS).join(', ') });
  }
  for (const [knob, allowed] of Object.entries(KNOBS)) {
    if (schema[knob] === undefined) continue;
    if (!allowed.includes(schema[knob])) f.push({ code: 'E_ENUM', path: knob, fix: 'one of: ' + allowed.join(', ') });
  }

  field(f, 'name', schema.name, CAPS.name);
  field(f, 'title', schema.title, CAPS.title);
  if (schema.tagline !== undefined) field(f, 'tagline', schema.tagline, CAPS.tagline);

  const sections = Array.isArray(schema.sections) ? schema.sections : null;
  if (!sections) {
    f.push({ code: 'E_SHAPE', path: 'sections', fix: 'sections must be an array of section objects' });
  } else {
    if (sections.length === 0) f.push({ code: 'E_EMPTY', path: 'sections', fix: 'at least one section is required' });
    if (sections.length > CAPS.sections) {
      f.push({ code: 'E_CAP', path: 'sections', fix: 'at most ' + CAPS.sections + ' sections (now ' + sections.length + '); merge or drop one' });
    }
    sections.forEach((s, i) => {
      const at = 'sections[' + i + ']';
      if (!s || typeof s !== 'object') {
        f.push({ code: 'E_SHAPE', path: at, fix: 'each section must be an object with type/heading' });
        return;
      }
      if (!SECTION_TYPES.includes(s.type)) {
        f.push({ code: 'E_ENUM', path: at + '.type', fix: 'one of: ' + SECTION_TYPES.join(', ') });
      }
      if (s.heading !== undefined) field(f, at + '.heading', s.heading, CAPS.heading);
      if (s.body !== undefined) field(f, at + '.body', s.body, CAPS.body);
      if (s.items !== undefined) {
        if (!Array.isArray(s.items)) {
          f.push({ code: 'E_SHAPE', path: at + '.items', fix: 'items must be an array of {label, text}' });
        } else {
          if (s.items.length > CAPS.items) {
            f.push({ code: 'E_CAP', path: at + '.items', fix: 'at most ' + CAPS.items + ' items (now ' + s.items.length + ')' });
          }
          s.items.forEach((it, j) => {
            const iat = at + '.items[' + j + ']';
            if (!it || typeof it !== 'object') {
              f.push({ code: 'E_SHAPE', path: iat, fix: 'each item must be an object with label' });
              return;
            }
            field(f, iat + '.label', it.label, CAPS.itemLabel);
            if (it.text !== undefined) field(f, iat + '.text', it.text, CAPS.itemText);
          });
        }
      }
      if (['features', 'steps', 'table', 'faq'].includes(s.type) && !Array.isArray(s.items)) {
        f.push({ code: 'E_SHAPE', path: at + '.items', fix: 'type "' + s.type + '" needs an items array' });
      }
      if (s.type === 'table') {
        if (!Array.isArray(s.columns) || s.columns.length !== 2) {
          f.push({ code: 'E_SHAPE', path: at + '.columns', fix: 'a table needs columns: ["left header", "right header"]' });
        } else {
          s.columns.forEach((c, j) => field(f, at + '.columns[' + j + ']', c, CAPS.column));
        }
      }
    });
  }

  return { ok: f.length === 0, failures: f };
}

/** Rules that can only be checked on the rendered output. */
function validateOutput({ files, bytes }) {
  const f = [];
  const entries = Object.entries(files);
  const served = entries.filter(([p]) => p !== 'sitebox.json');
  if (served.length > CAPS.servedFiles) {
    f.push({ code: 'E_FILES', path: '$', fix: 'at most ' + CAPS.servedFiles + ' served files (now ' + served.length + ')' });
  }
  if (bytes > CAPS.bytes) {
    f.push({ code: 'E_BUDGET', path: '$', fix: 'total ' + Math.round(bytes / 1024) + 'KB exceeds ' + Math.round(CAPS.bytes / 1024) + 'KB; cut copy, not rules' });
  }
  for (const [p, body] of entries) {
    if (HAN.test(body)) {
      const m = body.match(HAN);
      f.push({ code: 'E_CJK', path: p, fix: 'remove the Han character "' + (m && m[0]) + '" — this repo ships zero CJK in site output' });
    }
    if (p.endsWith('.html') && EXT_ATTR.test(body)) {
      f.push({ code: 'E_EXTERNAL', path: p, fix: 'no external href/src — the page must work offline' });
    }
    if (p.endsWith('.css') && EXT_CSS.test(body)) {
      f.push({ code: 'E_EXTERNAL', path: p, fix: 'no @import or url() to a network host — use the system font stack' });
    }
    if (p.endsWith('.html') && /<link\b[^>]*href\s*=\s*["'](?:https?:)?\/\//i.test(body)) {
      f.push({ code: 'E_EXTERNAL', path: p, fix: 'remove the CDN stylesheet link' });
    }
    if (p === 'server.js' && /process\.env\.HOST\s*\|\|\s*['"]0\.0\.0\.0/.test(body)) {
      f.push({ code: 'E_BIND', path: p, fix: 'prototype server must default to 127.0.0.1, not 0.0.0.0' });
    }
  }
  return { ok: f.length === 0, failures: f };
}

/** Render purity gate: same schema must produce the same bytes twice and across processes. */
function fingerprint(files) {
  const h = crypto.createHash('sha256');
  Object.keys(files).sort().forEach((k) => {
    h.update(k, 'utf8'); h.update('\0', 'utf8');
    h.update(files[k], 'utf8'); h.update('\0', 'utf8');
  });
  return h.digest('hex');
}

/** Drift: what is on disk vs what this schema renders to. */
function drift(diskFiles, renderedFiles) {
  const f = [];
  const names = new Set([...Object.keys(diskFiles || {}), ...Object.keys(renderedFiles || {})]);
  for (const name of [...names].sort()) {
    const a = diskFiles && diskFiles[name];
    const b = renderedFiles && renderedFiles[name];
    if (a == null && b != null) f.push({ code: 'E_DRIFT', path: name, fix: 'file is missing on disk; call update_site to re-render' });
    else if (a != null && b == null) f.push({ code: 'E_DRIFT', path: name, fix: 'unexpected file in the site folder; delete it or fold it into sitebox.json' });
    else if (a !== b) f.push({ code: 'E_DRIFT', path: name, fix: 'hand-edited file differs from sitebox.json; call update_site (do not edit rendered files)' });
  }
  return { ok: f.length === 0, failures: f };
}

module.exports = { validate, validateOutput, fingerprint, drift, PLACEHOLDER, HAN, MARKER };
