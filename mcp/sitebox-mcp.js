#!/usr/bin/env node
'use strict';
/**
 * sitebox-mcp — stdio JSON-RPC 2.0, zero dependencies.
 * Eight tools. No business logic of its own: it validates, renders, and calls the
 * dashboard API. stdout is the protocol channel; every log line goes to stderr.
 */
const fs = require('fs');
const os = require('os');
const path = require('path');

const api = require('./lib/api');
const { ACCENTS, LAYOUTS, SECTION_TYPES, LANGS, SKELETON, CAPS, hashSchema, KNOBS, seedKnobs, LAYOUT_CATALOG } = require('./lib/schema');
const { render } = require('./lib/render');
const { validate, validateOutput, drift } = require('./lib/validate');
const { createHttpTransport } = require('./lib/http-transport');

const SITES = process.env.SITEBOX_SITES || path.join(os.homedir(), 'sitebox', 'sites');
const VERSION = '0.1.0';
const log = (...a) => process.stderr.write('[sitebox-mcp] ' + a.join(' ') + '\n');

const esc = (s) => String(s == null ? '' : s).replace(/&/g, '&amp;').replace(/</g, '&lt;')
  .replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');

/* ── result shapes ── */
const text = (obj) => ({ content: [{ type: 'text', text: JSON.stringify(obj) }] });
const ok = (obj) => text(Object.assign({ ok: true }, obj));
const bad = (failures, extra) => ({
  content: [{ type: 'text', text: JSON.stringify(Object.assign({ ok: false, errors: [].concat(failures) }, extra || {})) }],
  isError: true,
});
const one = (code, message, fix, extra) => bad([{ code, message, fix }], extra);

/* ── site folder helpers ── */
const siteDir = (id) => path.join(SITES, id);
const schemaPath = (id) => path.join(siteDir(id), 'sitebox.json');

function readSchema(id) {
  try { return JSON.parse(fs.readFileSync(schemaPath(id), 'utf8')); } catch { return null; }
}

function writeTree(id, files, schema) {
  const dir = siteDir(id);
  fs.mkdirSync(path.join(dir, 'public'), { recursive: true });
  for (const [rel, body] of Object.entries(files)) fs.writeFileSync(path.join(dir, rel), body, 'utf8');
  fs.writeFileSync(schemaPath(id), JSON.stringify(schema, null, 2) + '\n', 'utf8');
}

function readTree(id, files) {
  const out = {};
  for (const rel of Object.keys(files)) {
    try { out[rel] = fs.readFileSync(path.join(siteDir(id), rel), 'utf8'); } catch { out[rel] = null; }
  }
  return out;
}

async function probe(port, timeout = 600) {
  return api.get('http://127.0.0.1:' + port + '/', timeout);
}

/**
 * The URL a caller can act on, plus whether it is only good on the host. Never invent
 * a hostname: a site bound to 127.0.0.1 has no LAN URL, and printing one would send the
 * operator to a dead page. The note says where the URL does work.
 */
function siteUrl(site) {
  const port = Number(site && site.port);
  if (!Number.isInteger(port) || port <= 0) return { url: null, localOnly: null };
  const localOnly = site.host === '127.0.0.1' || site._localOnly === true;
  return {
    url: 'http://localhost:' + port,
    localOnly,
    ...(localOnly
      ? { note: 'bound to 127.0.0.1 — opens only on the host running SiteBox, not from another device' }
      : { note: 'reachable on the LAN at the host that serves the dashboard' }),
  };
}

/** A started site is only up when `/` answers 200 with the expected title. */
async function verify(schema, port) {
  const r = await probe(port, 3000);
  const want = '<title>' + esc(schema.title) + '</title>';
  const titleOk = r.body.includes(want);
  return {
    ok: r.status === 200 && titleOk,
    status: r.status,
    titleMatch: titleOk,
    ...(r.status === 200 && titleOk ? {} : { fix: r.status === 0
      ? 'nothing is listening on 127.0.0.1:' + port + ' — read site_logs'
      : 'served page does not match the schema title — call check_site' }),
  };
}

/** Refuse to talk to a dashboard that lacks a capability instead of guessing from a 400. */
async function requireFeatures(need) {
  let h;
  try { h = await api.health(); } catch (e) {
    if (e.code === 'E_NOT_FOUND') {
      throw Object.assign(new Error('dashboard has no /api/health'), {
        code: 'E_TOO_OLD', fix: 'the dashboard image is older than this MCP server: rebuild and recreate it, then retry',
      });
    }
    throw e;
  }
  const have = Array.isArray(h && h.features) ? h.features : [];
  const missing = need.filter((f) => !have.includes(f));
  if (missing.length) {
    throw Object.assign(new Error('dashboard is missing capabilities: ' + missing.join(', ')), {
      code: 'E_TOO_OLD', fix: 'rebuild the dashboard image so /api/health advertises ' + missing.join(' + '), features: have,
    });
  }
  return have;
}

/** Latest baseline recorded before a deploy: the only honest answer to "who should come back". */
function readBaseline() {
  const dir = process.env.SITEBOX_BASELINE_DIR || path.join(os.homedir(), 'backups', 'sitebox-baseline');
  let files;
  try {
    files = fs.readdirSync(dir).filter((f) => f.endsWith('.json')).sort();
  } catch {
    return null;
  }
  if (!files.length) return null;
  const file = path.join(dir, files[files.length - 1]);
  try {
    const data = JSON.parse(fs.readFileSync(file, 'utf8'));
    if (!data || !Array.isArray(data.listening)) return null;
    return {
      file,
      stamp: (data.stamp || files[files.length - 1].split('-pre-')[0] || '').slice(0, 24),
      listening: data.listening.map((x) => ({ id: x.id, port: Number(x.port) })),
    };
  } catch {
    return null;
  }
}

/* ── tools ── */
const BASE_SCHEMA = {
  id: { type: 'string', description: 'kebab-case, <=32 chars; becomes sites/<id>/ and the URL path' },
  name: { type: 'string', description: 'short product name, <=40 chars' },
  lang: { type: 'string', enum: LANGS, description: 'document language, default th' },
  layout: { type: 'string', enum: LAYOUTS, description: 'docs | dashboard | showcase | list | form' },
  accent: { type: 'string', enum: Object.keys(ACCENTS), description: 'pre-measured accent; every value passes AA on paper — do not invent one' },
  title: { type: 'string', description: '<=64 chars, the page h1' },
  tagline: { type: 'string', description: '<=90 chars, one sentence' },
  ...Object.fromEntries(Object.entries(KNOBS).map(([k, v]) => [k, { type: 'string', enum: v, description: 'visual knob; omit to get a deterministic cosmetic seed' }])),
};

const SECTION_SCHEMA = {
  type: 'array',
  maxItems: CAPS.sections,
  description: 'at most ' + CAPS.sections + ' sections; heading <=' + CAPS.heading + ', body <=' + CAPS.body + ', items <=' + CAPS.items,
  items: {
    type: 'object',
    properties: {
      type: { type: 'string', enum: SECTION_TYPES },
      heading: { type: 'string' },
      body: { type: 'string' },
      columns: { type: 'array', items: { type: 'string' }, description: 'table only: exactly two column headings' },
      items: {
        type: 'array',
        maxItems: CAPS.items,
        items: {
          type: 'object',
          properties: { label: { type: 'string' }, text: { type: 'string' } },
          required: ['label'],
        },
      },
    },
    required: ['type'],
  },
};

const TOOLS = [
  {
    name: 'doctor',
    description: 'Preflight before creating anything: dashboard capabilities, token, stale listeners, root-owned files, free disk. Also returns create_site_skeleton — a payload that is structurally valid whose text markers the validator rejects.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    handler: async () => {
      const report = { api: api.BASE, token: api.hasToken() ? 'present' : 'none', token_access: null, features: null, sites: {}, issues: [] };
      if (!api.hasToken()) {
        report.issues.push({
          code: 'E_AUTH',
          message: 'no token configured for this MCP server',
          fix: 'mint one on the host: node dashboard/token.js add mcp --scope write --print, then set SITEBOX_API_TOKEN or ~/.config/sitebox/mcp.token (600). The UI opening without a token in a browser does not help an agent: only Bearer authenticates a client.',
        });
      }
      try {
        const h = await api.health();
        report.features = h.features || [];
        if (!report.features.includes('port0')) report.issues.push({ code: 'E_TOO_OLD', message: 'dashboard cannot allocate port 0', fix: 'rebuild the dashboard image' });
        // A read-only key is the difference between "doctor is green" and "create_site
        // answers 403". Ask the dashboard what this credential may do instead of
        // assuming the word "write" means something narrower.
        if (api.hasToken()) {
          try {
            const me = await api.me();
            report.token_access = {
              scope: me.scope || null,
              canRead: me.canRead !== false,
              canWrite: Boolean(me.canWrite),
            };
            if (!report.token_access.canWrite) {
              report.issues.push({
                code: 'E_SCOPE',
                message: 'this token is read-only: create_site, update_site, site_ctl and delete_site will answer 403',
                fix: 'mint a write key — write includes read, so one key covers every tool: node dashboard/token.js add mcp --scope write --print',
              });
            }
          } catch { /* a dashboard without /api/me: the first write will say so */ }
        }
      } catch (e) {
        const tooOld = e.code === 'E_NOT_FOUND';
        report.issues.push({
          code: tooOld ? 'E_TOO_OLD' : (e.code || 'E_UNREACHABLE'),
          message: tooOld ? 'this dashboard has no /api/health — the image predates this MCP server' : e.message,
          fix: tooOld ? 'rebuild and recreate the dashboard image, then run doctor again' : (e.fix || 'start the dashboard'),
        });
        return bad(report.issues, { report: Object.assign(report, { skeleton: SKELETON }) });
      }
      try {
        const sites = await api.sites();
        const probes = await Promise.all(sites.map((s) => probe(s.port)));
        const staleListeners = sites.filter((s, i) => probes[i].status === 200 && !s._running).map((s) => ({ id: s.id, port: s.port }));
        report.sites = {
          total: sites.length,
          running: sites.filter((s) => s._running).length,
          reachable: probes.filter((p) => p.status === 200).length,
          staleEntries: sites.filter((s) => s._stale).length,
          staleListeners, // port answers but the dashboard has no process for it
        };
        if (staleListeners.length) report.issues.push({ code: 'E_STALE_LISTENER', message: staleListeners.map((s) => s.id + ':' + s.port).join(' '), fix: 'restart_site adopts nothing — use site_ctl restart, which cleans the PID map' });
        // A container recreate wipes the in-memory PID map: every site reads as stopped
        // while nothing is listening. That is not "broken", and it is not the agent's
        // call to fix — surface it as an operator decision.
        const s0 = report.sites;
        const recreate = s0.total > 0 && s0.running === 0 && s0.reachable === 0;
        const base = readBaseline();
        if (base) {
          // A recorded baseline is the truth: it knows which sites were listening before
          // the restart, and inference only ever sees the aftermath.
          const live = new Set(sites.filter((s, i) => probes[i].status === 200).map((s) => s.id));
          const missing = base.listening.filter((x) => !live.has(x.id));
          report.baseline = { source: 'baseline', file: base.file, stamp: base.stamp, expect: base.listening.length };
          report.expected_after_restart = base.listening.length;
          report.expected_set = base.listening;
          report.missing_from_baseline = missing;
          if (missing.length) {
            report.notes = (report.notes || []).concat([{
              code: 'E_BASELINE_GAP',
              message: missing.map((x) => x.id + ':' + x.port).join(' ') + ' were running before the restart',
              fix: 'start only these with site_ctl, one at a time; do not start the rest',
            }]);
          }
        } else {
          report.baseline = { source: 'inferred', note: 'no baseline file found — this count is a guess, not a record' };
          report.expected_after_restart = recreate ? s0.total : 0;
          if (recreate) {
            report.notes = [{
              code: 'E_ALL_STOPPED',
              message: s0.total + ' sites registered, 0 running, 0 listening — the dashboard was restarted',
              fix: 'do not bulk-start. Ask the operator which of the ' + s0.total + ' should run, then start them one at a time with site_ctl',
            }];
          }
        }
      } catch (e) {
        report.issues.push({ code: e.code, message: e.message, fix: e.fix });
      }
      let rootOwned = 0;
      try {
        const walk = (dir, depth) => {
          if (depth > 3) return;
          for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
            const fp = path.join(dir, entry.name);
            try { if (fs.statSync(fp).uid === 0) rootOwned++; } catch {}
            if (entry.isDirectory()) walk(fp, depth + 1);
          }
        };
        walk(SITES, 0);
      } catch {}
      report.rootOwnedFiles = rootOwned;
      if (rootOwned) report.issues.push({ code: 'E_ROOT_OWNED', message: rootOwned + ' file(s) owned by root inside sites/', fix: 'run the chown sweep with the container stopped, then recreate' });
      // Recovery path check: if the dashboard locks us out, the host CLI is the only way
      // back in. Measured on the host, because the container cannot mint its own way in.
      const hostData = process.env.SITEBOX_DATA_HOST || path.join(os.homedir(), 'sitebox', 'dashboard', 'data');
      const mint = { dir: hostData, ok: false };
      try {
        fs.accessSync(hostData, fs.constants.W_OK);
        const T = path.join(hostData, 'tokens.json');
        if (fs.existsSync(T)) fs.accessSync(T, fs.constants.W_OK);
        mint.ok = true;
      } catch (e) {
        mint.reason = e.code === 'EACCES'
          ? 'not writable by this user — the deploy chown sweep has not run'
          : e.code || e.message;
        report.issues.push({ code: 'E_NO_MINT_PATH', message: 'token CLI cannot write ' + hostData + ' (' + mint.reason + ')', fix: 'run the chown sweep with the container stopped; this is the recovery path if auth locks you out' });
      }
      report.host_cli_can_mint = mint;
      try {
        const st = fs.statfsSync(SITES);
        report.diskFreeGB = Math.round((st.bavail * st.bsize) / 1e9 * 10) / 10;
      } catch {}
      // The skeleton cannot ship: the validator rejects its markers on purpose.
      report.create_site_skeleton = SKELETON;
      report.layouts = LAYOUT_CATALOG;
      report.enums = { layout: LAYOUTS, accent: Object.keys(ACCENTS), section: SECTION_TYPES };
      report.caps = CAPS;
      return report.issues.length ? bad(report.issues, { report }) : ok(report);
    },
  },
  {
    name: 'list_sites',
    description: 'List registered sites with real reachability (HTTP GET on the port, not the _running flag). Pass id for a single site.',
    inputSchema: { type: 'object', properties: { id: { type: 'string' } }, additionalProperties: false },
    handler: async ({ id }) => {
      const sites = await api.sites();
      const chosen = id ? sites.filter((s) => s.id === id) : sites;
      if (id && !chosen.length) return one('E_NOT_FOUND', 'no site with id ' + id, 'call list_sites without an argument');
      const probes = await Promise.all(chosen.map((s) => probe(s.port)));
      const rows = chosen.map((s, i) => ({
        id: s.id, name: s.name, port: s.port,
        running: Boolean(s._running),
        reachable: probes[i].status === 200,
        staleEntry: Boolean(s._stale),
        ...siteUrl(s),
        prototype: fs.existsSync(schemaPath(s.id)),
      }));
      return ok({ count: rows.length, sites: rows });
    },
  },
  {
    name: 'create_site',
    description: 'Create a prototype site from a schema: validates, renders 4 files, registers on a self-allocated port, starts, and verifies GET / = 200 with a matching <title>. Content not structure: never send HTML. Same payload twice is a no-op.',
    inputSchema: {
      type: 'object',
      properties: Object.assign({}, BASE_SCHEMA, { sections: SECTION_SCHEMA }),
      required: ['id', 'name', 'layout', 'accent', 'title', 'sections'],
      additionalProperties: false,
    },
    handler: async (args) => {
      const startedAt = Date.now();
      const schema = { lang: 'th', ...seedKnobs(args.id), ...args };
      const v = validate(schema);
      if (!v.ok) return bad(v.failures);
      const r = render(schema);
      const o = validateOutput(r);
      if (!o.ok) return bad(o.failures);
      const hash = hashSchema(schema);

      try {
        await requireFeatures(['port0', 'health']);
      } catch (e) {
        return one(e.code || 'E_UNEXPECTED', e.message, e.fix);
      }

      const sites = await api.sites();
      const existing = sites.find((s) => s.id === schema.id);
      if (existing) {
        const disk = readSchema(schema.id);
        const hasServer = fs.existsSync(path.join(siteDir(schema.id), 'server.js'));
        if (disk && hashSchema(disk) === hash) {
          // identical payload: make it serve, do not fail a retry
          if (!existing._running) await api.start(schema.id).catch(() => null);
          const vfy = await verify(schema, existing.port);
          return ok({ id: schema.id, noop: true, port: existing.port, ...siteUrl(existing), verified: vfy, files: Object.keys(r.files), bytes: r.bytes, hash: hash.slice(0, 8) });
        }
        if (hasServer || disk) {
          return one('E_ID_TAKEN', 'sites/' + schema.id + ' already exists with different content',
            'use update_site to change an existing prototype; pick another id for a new one',
            { id: schema.id, currentHash: disk ? hashSchema(disk).slice(0, 8) : null, wantedHash: hash.slice(0, 8) });
        }
        // registered earlier but no files yet (interrupted create): resume below
      }

      let site = existing;
      if (!site) {
        try {
          site = await registerPrototype(schema);
        } catch (e) {
          return one(e.code || 'E_UNEXPECTED', e.message, e.fix || 'run doctor', e.extra);
        }
      }

      writeTree(schema.id, r.files, schema);
      let started = await api.start(schema.id);
      if (!started || started.ok === false) {
        // A listener can appear between register and start; the dashboard owns the port,
        // so ask it for a fresh one instead of reporting a dead end.
        if (/port|EADDRINUSE/i.test(String(started && started.message))) {
          const fresh = await registerPrototype(schema).catch(() => null);
          if (fresh) { site = fresh; started = await api.start(schema.id); }
        }
        if (!started || started.ok === false) {
          return one('E_START_FAILED', String((started && started.message) || 'start returned no result'),
            String((started && started.hint) || 'read site_logs for this id'), { logs: (started && started.logs) || [] });
        }
      }
      const vfy = await verify(schema, site.port);
      if (!vfy.ok) return one('E_VERIFY_FAILED', 'started but the page did not verify (status ' + vfy.status + ')', vfy.fix, { id: schema.id, port: site.port });
      return ok({
        id: schema.id, port: site.port, ...siteUrl(site),
        files: Object.keys(r.files), bytes: r.bytes, hash: hash.slice(0, 8),
        verified: vfy, elapsedMs: Date.now() - startedAt,
      });
    },
  },
  {
    name: 'update_site',
    description: 'Change an existing prototype: patch sitebox.json, re-render, restart, verify. One call for a colour or a sentence — do not rebuild the site.',
    inputSchema: {
      type: 'object',
      properties: Object.assign({ id: { type: 'string', description: 'existing prototype id' } }, (() => {
        const p = Object.assign({}, BASE_SCHEMA);
        delete p.id;
        return p;
      })()),
      required: ['id'],
      additionalProperties: false,
    },
    handler: async (args) => {
      const { id, ...patch } = args;
      const disk = readSchema(id);
      if (!disk) {
        return one('E_NOT_PROTOTYPE', 'no sitebox.json in sites/' + id, 'this site was not built from a schema — edit it with the full sitebox-create pipeline, not here');
      }
      const merged = Object.assign({}, disk, patch);
      const v = validate(merged);
      if (!v.ok) return bad(v.failures);
      const r = render(merged);
      const o = validateOutput(r);
      if (!o.ok) return bad(o.failures);
      writeTree(id, r.files, merged);
      const started = await api.restart(id);
      const vfy = await verify(merged, Number(started && started.port) || 0);
      const fresh = (await api.sites().catch(() => [])).find((s) => s.id === id) || null;
      return ok({
        id, changed: Object.keys(patch), hash: hashSchema(merged).slice(0, 8),
        ...siteUrl(fresh),
        files: Object.keys(r.files), bytes: r.bytes, verified: vfy,
        ...(vfy.ok ? {} : { warning: 'files updated but the page did not verify' }),
      });
    },
  },
  {
    name: 'check_site',
    description: 'Validate a prototype without changing it: schema rules, output rules, and drift between sitebox.json and the files on disk (re-renders and compares). Returns every failure at once with code/path/fix.',
    inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'], additionalProperties: false },
    handler: async ({ id }) => {
      const disk = readSchema(id);
      if (!disk) return one('E_NOT_PROTOTYPE', 'no sitebox.json in sites/' + id, 'not a schema-managed prototype');
      const v = validate(disk);
      const r = render(disk);
      const o = validateOutput(r);
      const onDisk = readTree(id, r.files);
      const d = drift(onDisk, r.files);
      const failures = [...v.failures, ...o.failures, ...d.failures];
      const sites = await api.sites().catch(() => []);
      const site = sites.find((s) => s.id === id);
      const live = site ? await verify(disk, site.port) : null;
      return failures.length
        ? bad(failures, { id, hash: hashSchema(disk).slice(0, 8), bytes: r.bytes, live })
        : ok({ id, hash: hashSchema(disk).slice(0, 8), files: Object.keys(r.files), bytes: r.bytes, live });
    },
  },
  {
    name: 'site_ctl',
    description: 'Start, stop or restart a site. restart is also how a stale PID map is cleaned up after a container recreate.',
    inputSchema: {
      type: 'object',
      properties: { id: { type: 'string' }, action: { type: 'string', enum: ['start', 'stop', 'restart'] } },
      required: ['id', 'action'],
      additionalProperties: false,
    },
    handler: async ({ id, action }) => {
      const res = await api[action === 'start' ? 'start' : action === 'stop' ? 'stop' : 'restart'](id);
      if (res && res.ok === false) {
        return one('E_START_FAILED', String(res.message), String(res.hint || 'read site_logs'), { logs: res.logs || [] });
      }
      const sites = await api.sites();
      const site = sites.find((s) => s.id === id);
      const live = action === 'stop' || !site ? null : await verifyShadow(id, site.port);
      return ok({ id, action, port: site ? site.port : null, ...siteUrl(site), running: site ? Boolean(site._running) : false, live });
    },
  },
  {
    name: 'site_logs',
    description: 'Tail a site log (dashboard ring buffer, last 500 lines).',
    inputSchema: {
      type: 'object',
      properties: { id: { type: 'string' }, lines: { type: 'number', description: '1-200, default 60' } },
      required: ['id'],
      additionalProperties: false,
    },
    handler: async ({ id, lines }) => {
      const res = await api.logs(id, Math.min(Math.max(Number(lines) || 60, 1), 200));
      const rows = (res.lines || []).map((l) => l.stream + ' ' + l.text);
      const errs = (res.lines || []).filter((l) => l.stream === 'err').length;
      return ok({ id, running: res.running, count: res.count, errorLines: errs, lines: rows.slice(-Math.min(rows.length, 200)) });
    },
  },
  {
    name: 'delete_site',
    description: 'Delete a site and its folder. Requires confirm equal to the id. No undo: this is the dashboard delete, which purges sites/<id>.',
    inputSchema: {
      type: 'object',
      properties: { id: { type: 'string' }, confirm: { type: 'string', description: 'must equal id' } },
      required: ['id', 'confirm'],
      additionalProperties: false,
    },
    handler: async ({ id, confirm }) => {
      if (confirm !== id) return one('E_CONFIRM', 'confirm must equal the id being deleted', 'send confirm: "' + id + '"');
      const res = await api.remove(id);
      return ok({ id, purged: res.purged !== false, removed: res.removed || null, note: res.note });
    },
  },
];

/** Register with a dashboard-allocated port; retries only on the race we can actually lose. */
async function registerPrototype(schema) {
  const accent = ACCENTS[schema.accent] ? schema.accent : 'ink-green';
  const payload = {
    id: schema.id,
    name: schema.name,
    description: (schema.tagline || schema.name).slice(0, 90),
    icon: 'flask-conical',
    iconColor: ACCENTS[accent].hex,
    category: 'prototype',
    port: 0,
    host: '127.0.0.1',
    path: 'sites/' + schema.id,
  };
  let lastErr = null;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      const res = await api.register(payload);
      return res.site || res;
    } catch (e) {
      lastErr = e;
      if (e.code !== 'E_PORT_TAKEN' || !e.retryable) throw e;
    }
  }
  throw lastErr;
}

async function verifyShadow(id, port) {
  const disk = readSchema(id);
  if (!disk) {
    const r = await probe(port, 2500);
    return { ok: r.status === 200, status: r.status };
  }
  return verify(disk, port);
}

/* ── JSON-RPC ── */
const SERVER_INFO = { name: 'sitebox', version: VERSION };
const SUPPORTED = ['2025-06-18', '2025-03-26', '2024-11-05'];

/**
 * One dispatcher for every transport: it returns the response object, or null for a
 * notification (which carries no response at all). The transports differ only in how
 * bytes arrive and where the answer goes, so the tool contract cannot drift between
 * stdio and HTTP.
 */
async function dispatch(msg) {
  const { id, method, params } = msg;
  const isNotification = id === undefined || id === null;
  try {
    if (method === 'initialize') {
      const asked = params && params.protocolVersion;
      return { jsonrpc: '2.0', id, result: {
        protocolVersion: SUPPORTED.includes(asked) ? asked : SUPPORTED[0],
        capabilities: { tools: {} },
        serverInfo: SERVER_INFO,
      } };
    }
    if (method === 'notifications/initialized' || method === 'initialized') return null;
    if (method === 'ping') return { jsonrpc: '2.0', id, result: {} };
    if (method === 'tools/list') {
      return { jsonrpc: '2.0', id, result: { tools: TOOLS.map((t) => ({ name: t.name, description: t.description, inputSchema: t.inputSchema })) } };
    }
    if (method === 'tools/call') {
      const name = params && params.name;
      const tool = TOOLS.find((t) => t.name === name);
      if (!tool) return { jsonrpc: '2.0', id, result: { content: [{ type: 'text', text: JSON.stringify({ ok: false, errors: [{ code: 'E_NO_TOOL', message: 'unknown tool ' + name, fix: 'call tools/list' }] }) }], isError: true } };
      const args = (params && params.arguments) || {};
      const startedAt = Date.now();
      try {
        const result = await tool.handler(args);
        log(name, Date.now() - startedAt + 'ms', result.isError ? 'error' : 'ok');
        return { jsonrpc: '2.0', id, result };
      } catch (e) {
        log(name, 'threw', e.code || '', e.message);
        return { jsonrpc: '2.0', id, result: one(e.code || 'E_UNEXPECTED', e.message, e.fix || 'read the message above; if it is a stack trace, the MCP server has a bug') };
      }
    }
    if (isNotification) return null;
    return { jsonrpc: '2.0', id, error: { code: -32601, message: 'method not found: ' + method } };
  } catch (e) {
    log('handler crash', e.stack || e.message);
    if (isNotification) return null;
    return { jsonrpc: '2.0', id, error: { code: -32603, message: String(e.message) } };
  }
}

function send(msg) { process.stdout.write(JSON.stringify(msg) + '\n'); }

/* ── stdio (the default) ── */
function runStdio() {
  let buf = '';
  process.stdin.setEncoding('utf8');
  process.stdin.on('data', (chunk) => {
    buf += chunk;
    const lines = buf.split('\n');
    buf = lines.pop();
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      let msg;
      try { msg = JSON.parse(trimmed); } catch (e) {
        log('bad JSON on stdin:', e.message);
        send({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'parse error' } });
        continue;
      }
      dispatch(msg).then((res) => { if (res) send(res); }).catch((e) => log('dispatch failed', e.message));
    }
  });
  process.stdin.on('end', () => process.exit(0));
}

/* ── streamable HTTP (opt-in) ── */
function runHttp() {
  const token = api.token();
  if (!token) {
    // An open endpoint that can create and delete sites is not a trade-off worth making.
    log('http: no token configured — refusing to start. Set SITEBOX_API_TOKEN or ~/.config/sitebox/mcp.token (600).');
    process.exit(1);
  }
  const port = Number(process.env.SITEBOX_MCP_HTTP_PORT || 4446);
  const host = process.env.SITEBOX_MCP_HTTP_HOST || '127.0.0.1';
  const server = createHttpTransport({ dispatch, token, protocolVersions: SUPPORTED, log });
  server.on('error', (e) => { log('http error:', e.message); process.exit(1); });
  server.listen(port, host, () => {
    const bound = server.address();
    log('http listening on http://' + host + ':' + bound.port + '/mcp');
  });
}

const TRANSPORT = (process.env.SITEBOX_MCP_TRANSPORT || (process.argv.includes('--http') ? 'http' : 'stdio')).toLowerCase();
if (TRANSPORT === 'http') runHttp();
else if (TRANSPORT === 'stdio') runStdio();
else {
  log('unknown SITEBOX_MCP_TRANSPORT "' + TRANSPORT + '" — expected stdio or http');
  process.exit(2);
}
