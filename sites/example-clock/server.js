const http = require('http');
const fs = require('fs');
const path = require('path');
const PORT = process.env.PORT || 4452;
const PUBLIC = path.join(__dirname, 'public');

const MIME = { '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8', '.js': 'application/javascript; charset=utf-8' };

http.createServer((req, res) => {
  let file = req.url.split('?')[0];
  if (file === '/') file = '/index.html';
  const fp = path.join(PUBLIC, file);
  if (fs.existsSync(fp) && fs.statSync(fp).isFile()) {
    res.writeHead(200, { 'Content-Type': MIME[path.extname(fp)] || 'text/plain' });
    fs.createReadStream(fp).pipe(res);
  } else {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    fs.createReadStream(path.join(PUBLIC, 'index.html')).pipe(res);
  }
}).listen(PORT, '0.0.0.0', () => console.log(`Clock running at http://localhost:${PORT}`));
