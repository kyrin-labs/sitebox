'use strict';
/**
 * Audit log rotation. SITEBOX_DATA and SITEBOX_AUDIT_MAX_BYTES are injected before the
 * module loads, so this never touches the real log — and the limit is small enough that
 * rotation happens in milliseconds instead of after a megabyte of production writes.
 */
const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'sitebox-audit-'));
process.env.SITEBOX_DATA = tmp;
process.env.SITEBOX_AUDIT_MAX_BYTES = '2048';

const audit = require('../audit');
const LIMIT = 2048;

const req = { headers: { 'user-agent': 'rotation-test' }, socket: { remoteAddress: '127.0.0.1' } };

test('rotation happens before the append, so the live file never runs away', () => {
  for (let i = 0; i < 80; i++) {
    audit.record({ who: { via: 'bearer', label: 'mcp' }, req, method: 'POST', path: `/api/probe/${i}` });
  }

  const files = fs.readdirSync(tmp).filter((f) => f.startsWith('audit.log'));
  assert.ok(files.length > 1, 'the log must rotate once it reaches the limit');
  assert.ok(files.length <= 6, 'keep the live file plus at most five rotations');

  // Every line in every generation must still parse: a rotated file is history, not debris.
  let lines = 0;
  for (const f of files) {
    const raw = fs.readFileSync(path.join(tmp, f), 'utf8');
    for (const line of raw.split('\n').filter(Boolean)) {
      JSON.parse(line);
      lines++;
    }
  }
  assert.equal(lines, 80, 'rotation must not lose a line');

  // The live file is never much larger than the limit: one line of overshoot at most,
  // because the check runs before the write rather than after it.
  const live = fs.statSync(path.join(tmp, 'audit.log')).size;
  assert.ok(live < LIMIT + 512, `live log is ${live} bytes, limit ${LIMIT}`);
});

test('a rotated file keeps its name ordering, so a reader can sort them', () => {
  const files = fs.readdirSync(tmp).filter((f) => f.startsWith('audit.log')).sort();
  for (const f of files.slice(1)) assert.match(f, /^audit\.log\.\d+$/);
});
