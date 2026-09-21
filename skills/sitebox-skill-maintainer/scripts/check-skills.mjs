#!/usr/bin/env node
/**
 * SiteBox skill drift checker — zero dependencies.
 *
 * Usage: node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
 * Exit code 1 when errors are found; warnings do not fail.
 */
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..', '..');
const SKILLS_DIR = path.join(ROOT, 'skills');
const SERVER = path.join(ROOT, 'dashboard', 'server.js');
const MAIN_JS = path.join(ROOT, 'dashboard', 'public', 'js', 'main.js');
const SITES_JSON = path.join(ROOT, 'dashboard', 'data', 'sites.json');
const README = path.join(ROOT, 'README.md');
const HOOK = path.join(ROOT, '.githooks', 'pre-commit');

const NAME_RE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const ALLOWED_SITE_IDS = new Set(['example-notes', 'example-clock']);

const errors = [];
const warnings = [];
const err = (m) => errors.push(m);
const warn = (m) => warnings.push(m);
const read = (p) => readFileSync(p, 'utf-8');
const rel = (p) => path.relative(ROOT, p).replace(/\\/g, '/');

function parseFrontmatter(src) {
  if (!src.startsWith('---')) return null;
  const end = src.indexOf('\n---', 3);
  if (end === -1) return null;
  const raw = src.slice(3, end);
  const fm = {};
  for (const line of raw.split(/\r?\n/)) {
    const m = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (m && m[1] !== 'metadata') fm[m[1]] = m[2].trim().replace(/^["']|["']$/g, '');
  }
  const metaMatch = raw.match(/^metadata:\s*\n((?:[ \t]+.*\n?)*)/m);
  if (metaMatch) {
    fm.metadata = {};
    for (const line of metaMatch[1].split(/\r?\n/)) {
      const m = line.match(/^\s+([A-Za-z0-9_-]+):\s*(.*)$/);
      if (m) fm.metadata[m[1]] = m[2].trim().replace(/^["']|["']$/g, '');
    }
  }
  return fm;
}

// Returns the body text under a `## Heading` whose title matches `headingRe`.
function sectionBody(src, headingRe) {
  const lines = src.split(/\r?\n/);
  let out = null;
  for (const line of lines) {
    if (/^##\s+/.test(line)) {
      if (headingRe.test(line)) { out = []; continue; }
      if (out) break;
    } else if (out) {
      out.push(line);
    }
  }
  return out ? out.join('\n') : null;
}

/* ── Discover skills ── */
if (!existsSync(SKILLS_DIR)) {
  console.error(`No skills/ directory at ${SKILLS_DIR}`);
  process.exit(1);
}

const skillDirs = readdirSync(SKILLS_DIR, { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => d.name);

const skills = [];
for (const dir of skillDirs) {
  const file = path.join(SKILLS_DIR, dir, 'SKILL.md');
  if (!existsSync(file)) {
    err(`skills/${dir}/ has no SKILL.md`);
    continue;
  }
  const src = read(file);
  skills.push({ dir, file, src });
  const fm = parseFrontmatter(src);

  if (!fm) { err(`skills/${dir}/SKILL.md has no YAML frontmatter`); continue; }
  if (!fm.name) err(`skills/${dir}/SKILL.md frontmatter is missing "name"`);
  else if (fm.name !== dir) err(`skills/${dir}/SKILL.md name "${fm.name}" does not match folder "${dir}"`);
  else if (!NAME_RE.test(fm.name)) err(`skill name "${fm.name}" is not lowercase kebab-case`);

  if (!fm.description) err(`skills/${dir}/SKILL.md frontmatter is missing "description"`);
  else {
    if (fm.description.length > 1024) err(`skill "${dir}" description is ${fm.description.length} chars (max 1024)`);
    if (!/use when|ใช้เมื่อ/i.test(fm.description)) warn(`skill "${dir}" description has no trigger phrase ("Use when…")`);
  }
  if (!fm.metadata?.version) warn(`skill "${dir}" has no metadata.version`);

  const lineCount = src.split(/\r?\n/).length;
  if (lineCount > 500) warn(`skills/${dir}/SKILL.md is ${lineCount} lines (guideline: < 500, move detail to references/)`);

  for (const m of src.matchAll(/\]\((?!https?:|mailto:|#)([^)\s]+)\)/g)) {
    const target = m[1].split('#')[0].split('?')[0];
    if (!/^(references|scripts|assets)\//.test(target)) continue;
    if (!existsSync(path.join(SKILLS_DIR, dir, target)))
      err(`skills/${dir}/SKILL.md links to missing file: ${target}`);
  }
}

/* ── API endpoints referenced by skills must exist in dashboard/server.js ── */
const serverSrc = read(SERVER).replace(/\\\//g, '/');
const endpoints = new Set();
for (const { src } of skills) {
  for (const m of src.matchAll(/\/api\/[A-Za-z0-9_:?=&./-]+/g)) {
    const ep = m[0].replace(/[.,;:`]+$/, '').split('?')[0].replace(/:id/g, 'x');
    endpoints.add(ep);
  }
}
for (const ep of endpoints) {
  const parts = ep.split('/').filter(Boolean);
  if (parts.length < 2) continue; // prose placeholder like "/api/..."
  const root = parts[1];
  if (!serverSrc.includes(`/api/${root}`)) {
    err(`skills reference ${ep} but /api/${root} is not in dashboard/server.js`);
    continue;
  }
  const leaf = parts[parts.length - 1];
  if (leaf !== root && !leaf.startsWith('x') && !serverSrc.includes(leaf))
    warn(`skills reference ${ep} — leaf "${leaf}" not found in server.js, verify manually`);
}

/* ── Icon lists must match dashboard/public/js/main.js ── */
const mainSrc = read(MAIN_JS);
const iconBlock = mainSrc.match(/const ICONS = \{([\s\S]*?)\n\};/);
const iconNames = iconBlock ? [...iconBlock[1].matchAll(/^\s*'?([a-z0-9-]+)'?\s*:/gm)].map((m) => m[1]) : [];
if (iconNames.length === 0) warn('could not parse ICONS from dashboard/public/js/main.js');
const iconSet = new Set(iconNames);
for (const { dir, src } of skills) {
  const section = sectionBody(src, /^##\s+(Available\s+)?Icons\b/i);
  if (!section) continue;
  const documented = new Set([...section.matchAll(/`([a-z0-9-]+)`/g)].map((m) => m[1]));
  for (const icon of documented) if (!iconSet.has(icon)) err(`skill "${dir}" documents icon "${icon}" which is not in main.js`);
  for (const icon of iconSet) if (!documented.has(icon)) warn(`skill "${dir}" icon list is missing "${icon}" (present in main.js)`);
}

/* ── sites.json must not contain runtime entries in the repo copy ── */
try {
  const cfg = JSON.parse(read(SITES_JSON));
  const extra = (cfg.sites || []).map((s) => s.id).filter((id) => !ALLOWED_SITE_IDS.has(id));
  if (extra.length) warn(`dashboard/data/sites.json contains runtime entries (${extra.join(', ')}) — do not commit this file`);
} catch (e) {
  err(`dashboard/data/sites.json is not valid JSON: ${e.message}`);
}

/* ── Repo docs and hooks ── */
const readme = read(README);
for (const { dir } of skills) if (!readme.includes(dir)) warn(`README.md does not mention skill "${dir}"`);
if (!existsSync(HOOK)) warn('.githooks/pre-commit is missing — the sites.json commit block is not enforced');

/* ── Report ── */
console.log(`SiteBox skill check — ${skills.length} skill(s), ${iconSet.size} icons, ${endpoints.size} endpoint reference(s)`);
for (const e of errors) console.log(`  ERROR  ${e}`);
for (const w of warnings) console.log(`  WARN   ${w}`);
console.log(`\n${errors.length} error(s), ${warnings.length} warning(s)`);
process.exit(errors.length ? 1 : 0);
