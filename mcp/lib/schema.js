'use strict';
/**
 * Single source of truth for the prototype tier: enums, caps, canonical hashing.
 * Nothing here may depend on time or randomness — render purity depends on it.
 * The dashboard spawns these files; the MCP server renders them. There is no second copy.
 */

const ACCENTS = {
  'ink-green':  { hex: '#1F4B3F', paper: '#F7F4EE' },
  navy:         { hex: '#1F3A5F', paper: '#F7F4EE' },
  terracotta:   { hex: '#8C3B22', paper: '#F7F4EE' },
  firefly:      { hex: '#8C2F2A', paper: '#F7F4EE' },
  'deep-teal':  { hex: '#1B4A4A', paper: '#F7F4EE' },
  charcoal:     { hex: '#2B2B28', paper: '#F7F4EE' },
};

const INK = '#14161A';
const ON_ACCENT = '#FFFFFF';

const LAYOUTS = ['docs', 'dashboard', 'showcase', 'list', 'form'];
const SECTION_TYPES = ['hero', 'features', 'steps', 'table', 'faq', 'cta', 'footer-note'];
const LANGS = ['th', 'en'];

// Typed knobs: the only place a prototype is allowed to differ visually without
// becoming unverifiable. Every value renders to byte-identical CSS.
const KNOBS = {
  density: ['airy', 'normal', 'compact'],
  radius: ['square', 'soft', 'round'],
  typeScale: ['s', 'm', 'l'],
  hero: ['plain', 'band'],
  rules: ['hairline', 'double', 'none'],
  texture: ['none', 'paper'],
};
const KNOB_DEFAULTS = { density: 'normal', radius: 'soft', typeScale: 'm', hero: 'plain', rules: 'hairline', texture: 'none' };

// One line per layout, not three skeletons: the agent needs to know which shape fits,
// not to pick between three payloads. Choosing is the decision we removed.
const LAYOUT_CATALOG = [
  { name: 'docs', whenToUse: 'explains one thing top to bottom: product page, spec, policy, guide' },
  { name: 'dashboard', whenToUse: 'several comparable items at a glance: metrics, inventory, status board' },
  { name: 'showcase', whenToUse: 'one big claim with room around it: launch, campaign, portfolio piece' },
  { name: 'list', whenToUse: 'many short records scanned in order: changelog, directory, index' },
  { name: 'form', whenToUse: 'a single narrow task: signup, request, calculator, intake' },
];

// Deterministic cosmetic seed: two prototypes with different ids never look the same,
// and two runs of the same id always do. Layout and accent are never seeded — those
// are the agent's intent, not a hash.
function seedKnobs(id) {
  const crypto = require('crypto');
  const h = crypto.createHash('sha256').update(String(id || ''), 'utf8').digest();
  return {
    density: KNOBS.density[h[0] % KNOBS.density.length],
    radius: KNOBS.radius[h[1] % KNOBS.radius.length],
  };
}

// served files only: server.js + public/{index.html,style.css,app.js}
// sitebox.json sits beside server.js and is never served.
const CAPS = {
  id: 32, name: 40, title: 64, tagline: 90,
  heading: 56, body: 240, itemLabel: 40, itemText: 120, column: 24,
  items: 6, sections: 5,
  servedFiles: 5, bytes: 48 * 1024,
};

const ID_RE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const MARKER = '\u00ab'; // « marks a value the author still has to fill in

// Stable stringify: sorted keys, no whitespace surprises, no platform variance.
function canonical(value) {
  if (Array.isArray(value)) return '[' + value.map(canonical).join(',') + ']';
  if (value && typeof value === 'object') {
    return '{' + Object.keys(value).sort()
      .filter((k) => value[k] !== undefined)
      .map((k) => JSON.stringify(k) + ':' + canonical(value[k])).join(',') + '}';
  }
  return JSON.stringify(value === undefined ? null : value);
}

const crypto = require('crypto');
function hashSchema(schema) {
  return crypto.createHash('sha256').update(canonical(schema), 'utf8').digest('hex');
}

// A payload an agent can fill in without reading any documentation.
const SKELETON = {
  id: 'my-prototype',
  name: '\u00abshort product name\u00bb',
  lang: 'th',
  layout: 'docs',
  accent: 'ink-green',
  title: '\u00abwhat this page is\u00bb',
  tagline: '\u00abone sentence, who it is for\u00bb',
  sections: [
    { type: 'features', heading: '\u00abwhat it does\u00bb', body: '\u00abone line\u00bb',
      items: [{ label: '\u00abpoint 1\u00bb', text: '\u00abone line\u00bb' }] },
    { type: 'cta', heading: '\u00abwhat to do next\u00bb', body: '\u00abone line\u00bb' },
  ],
};

module.exports = {
  ACCENTS, INK, ON_ACCENT, LAYOUTS, SECTION_TYPES, LANGS,
  KNOBS, KNOB_DEFAULTS, seedKnobs, LAYOUT_CATALOG,
  CAPS, ID_RE, MARKER, SKELETON, canonical, hashSchema,
};
