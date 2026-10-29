'use strict';
/* Prove the README's tool list matches what the server actually advertises. */
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..');
const c = spawn('node', [path.join(ROOT, 'sitebox-mcp.js')], { cwd: ROOT, stdio: ['pipe', 'pipe', 'ignore'] });
let buf = '';
c.stdout.setEncoding('utf8');
c.stdout.on('data', (d) => {
  buf += d;
  const nl = buf.indexOf('\n');
  if (nl < 0) return;
  const tools = JSON.parse(buf.slice(0, nl)).result.tools.map((t) => t.name);
  const readme = fs.readFileSync(path.join(ROOT, 'README.md'), 'utf8');
  const documented = [...readme.matchAll(/^\| `([a-z_]+)\(?/gm)].map((m) => m[1]);
  const missing = tools.filter((t) => !documented.includes(t));
  console.log('advertised:', tools.join(','));
  console.log('documented:', documented.join(','));
  console.log(missing.length ? 'MISSING FROM README: ' + missing.join(',') : 'all eight tools are documented');
  c.kill();
  process.exit(missing.length ? 1 : 0);
});
c.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list' }) + '\n');
