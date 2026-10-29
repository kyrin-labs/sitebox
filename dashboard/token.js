#!/usr/bin/env node
'use strict';
/**
 * token.js — mint and revoke tokens from the host.
 *
 *   node dashboard/token.js add mcp --scope write --print
 *   node dashboard/token.js list
 *   node dashboard/token.js revoke mcp
 *
 * This CLI is the recovery path: if the dashboard ever locks you out, it is the only
 * way back in that does not need a working token. Keep dashboard/data writable by the
 * user who runs it (the deploy chown sweep guarantees that).
 *
 * With --print the key is the LAST line of stdout, so `KEY=$(node dashboard/token.js add mcp --print | tail -1)`
 * always works. The key is never written to disk — only its sha256.
 */
const fs = require('fs');
const store = require('./tokenstore');

const argv = process.argv.slice(2);
const cmd = argv[0];
const flag = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && argv[i + 1] && !argv[i + 1].startsWith('--') ? argv[i + 1] : fallback;
};
const has = (name) => argv.includes(name);
// One string on disk, two readable labels in the output: write is read + write, so a
// single value is never ambiguous to a person reading `token.js list`.
const describeScope = (scope) => scope === 'write' ? 'write (read + write)' : 'read (read only)';

function fail(msg) {
  process.stderr.write('token.js: ' + msg + '\n');
  process.exit(1);
}

try {
  if (cmd === 'add') {
    const label = argv[1];
    if (!label) fail('usage: add <label> [--scope read|write] [--print]   (write includes read)');
    const scope = flag('--scope', 'write');
    const key = store.createToken(label, scope);
    process.stdout.write('label:  ' + label + '\n');
    process.stdout.write('scope:  ' + describeScope(scope) + '\n');
    process.stdout.write('store:  ' + store.TOKENS + ' (0600, hash only)\n');
    if (has('--print')) {
      process.stdout.write('warning: shown once; store it in ~/.config/sitebox/mcp.token (600)\n');
      process.stdout.write(key + '\n');
    } else {
      process.stdout.write('key not printed (pass --print to show it once)\n');
    }
  } else if (cmd === 'list') {
    const rows = store.summary();
    if (rows === null) {
      process.stdout.write('no token store yet (' + store.TOKENS + ') — dashboard will answer 401 to every API call\n');
      process.exit(0);
    }
    if (!rows.length) {
      process.stdout.write('token store is empty — every API call is refused (fail closed)\n');
      process.exit(0);
    }
    for (const r of rows) {
      process.stdout.write([r.label.padEnd(12), r.scope.padEnd(6), r.created || '-', r.lastUsed || 'never'].join('  ') + '\n');
    }
  } else if (cmd === 'revoke') {
    const label = argv[1];
    if (!label) fail('usage: revoke <label>');
    const r = store.revokeToken(label);
    if (!r.removed) fail('no token labelled "' + label + '"');
    process.stdout.write('revoked ' + label + '; dropped ' + r.sessions + ' session(s)\n');
  } else if (cmd === 'verify') {
    const key = argv[1];
    if (!key) fail('usage: verify <key>');
    const t = store.verify(key);
    process.stdout.write(t ? 'valid: ' + t.label + ' (' + describeScope(t.scope) + ')\n' : 'invalid\n');
    process.exit(t ? 0 : 1);
  } else if (cmd === 'path') {
    process.stdout.write(store.TOKENS + '\n');
  } else {
    fail('usage: token.js add <label> [--scope read|write] [--print] | list | revoke <label> | verify <key> | path');
  }
} catch (e) {
  fs.existsSync(store.DATA);
  fail(e.code === 'EACCES'
    ? 'cannot write ' + store.TOKENS + ' — the data folder is owned by another user (run the chown sweep first)'
    : e.message);
}
