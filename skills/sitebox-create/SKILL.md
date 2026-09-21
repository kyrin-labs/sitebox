---
name: sitebox-create
description: Create professional static sites in SiteBox format.
---

# Create Site (SiteBox)

Build high-quality, professional static sites. Every site should feel like a real product — not a demo, not a template.

## Workflow

### 1. Understand the brief

Before writing code, answer:
- What is this site? Who visits it?
- What content does it need? (pages, sections, data)
- What feeling should it evoke? (professional, warm, minimal, bold)
- What category does it belong to?

### 2. Design system (plan before coding)

Define these tokens upfront:

**Color** — 5-6 colors max. Match the subject:
- Publishing → warm browns, ambers, creams
- Tech → cool blues, grays
- Creative → expressive unique accent
- Corporate → navy, slate

**Typography** — max 2 families:
- Editorial/literary: `Playfair Display` + `Noto Serif Thai` for headlines, `Inter` for body
- Modern/tech: `Inter` + `Noto Sans Thai` for everything
- Display/impact: `Space Grotesk` or `Sora` for headlines

**Layout concept** — describe in one sentence:
- "Left-aligned editorial with generous whitespace"
- "Centered hero, then asymmetric grid"
- "Single-column reading experience"

### 3. Create site structure

```bash
# Find free port
curl -s http://localhost:4445/api/sites | python3 -c "
import sys,json
used={s['port'] for s in json.load(sys.stdin)}
print(next(p for p in range(4500,4900) if p not in used))
"

# Create directory
mkdir -p ~/sitebox/sites/{ID}/public
```

### 4. server.js (always this pattern)

```js
const http = require('http');
const fs = require('fs');
const path = require('path');
const PORT = process.env.PORT || PORT_NUMBER;
const PUBLIC = path.join(__dirname, 'public');
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png', '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml', '.ico': 'image/x-icon',
};

http.createServer((req, res) => {
  let file = req.url.split('?')[0];
  if (file === '/') file = '/index.html';
  const fp = path.join(PUBLIC, file);
  if (fs.existsSync(fp) && fs.statSync(fp).isFile()) {
    res.writeHead(200, { 'Content-Type': MIME[path.extname(fp)] || 'text/plain' });
    fs.createReadStream(fp).pipe(res);
  } else if (!path.extname(file)) {
    const html = path.join(PUBLIC, file + '.html');
    if (fs.existsSync(html)) { res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }); fs.createReadStream(html).pipe(res); }
    else { const idx = path.join(PUBLIC, file, 'index.html'); if (fs.existsSync(idx)) { res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }); fs.createReadStream(idx).pipe(res); } else { res.writeHead(404); res.end('Not Found'); } }
  } else { res.writeHead(404); res.end('Not Found'); }
}).listen(PORT, '0.0.0.0', () => console.log(`Running at http://localhost:${PORT}`));
```

### 5. Build the site (public/)

Single `index.html` with embedded CSS for simple sites. Separate files for multi-page.

**Required HTML structure:**
```html
<!DOCTYPE html>
<html lang="th">  <!-- or "en" -->
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Page Title — SiteBox</title>
  <meta name="description" content="...">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=..." rel="stylesheet">
  <link rel="icon" type="image/svg+xml" href="/favicon.svg">
  <style>/* all CSS here */</style>
</head>
<body>
  <main><!-- content --></main>
</body>
</html>
```

### 6. Register & start

```bash
curl -X POST http://localhost:4445/api/sites \
  -H 'Content-Type: application/json' \
  -d '{"id":"ID","name":"Name","description":"Desc","icon":"globe","iconColor":"#58a6ff","port":PORT,"category":"tools","url":"http://localhost:PORT","path":"sites/ID"}'

curl -X POST http://localhost:4445/api/sites/ID/start
```

---

## Quality Standards

### Typography
- Base: 16px, line-height: 1.6-1.8
- Max line length: 60-70 chars
- Heading scale: h1=2.5rem, h2=1.8rem, h3=1.3rem
- Letter-spacing: slightly negative for large headings
- Font weights: 400 body, 600-700 headings

### Color Contrast
- Text on background: minimum 4.5:1 (WCAG AA)
- Large text (18px+): minimum 3:1
- Interactive elements: minimum 3:1

### Spacing
- Between sections: 4-6rem
- Between elements: 1.5-2rem
- Padding in cards: 1.5-2rem
- Use consistent spacing scale (multiples of 0.25rem)

### Layout
- CSS Grid or Flexbox only
- Max-width: 1100-1200px
- Mobile-first responsive design
- Left-aligned for editorial, centered for short hero text
- Generous whitespace — let content breathe

### Content
- Real content, never lorem ipsum
- Thai primary, English secondary (for Thai projects)
- Specific headlines, not generic
- Every section must earn its place

### Accessibility
- Semantic HTML: header, main, nav, footer, article, section
- Alt text on all images
- Focus states on interactive elements
- Keyboard navigable
- prefers-reduced-motion: reduce animations

### Icons
- Use inline SVG (Lucide icons), never emoji
- Consistent size and stroke width
- Color via CSS variable or currentColor

---

## Anti-Patterns (Never Do These)

- Gradient backgrounds without purpose
- Glassmorphism everywhere
- Centered everything
- All-caps labels
- Feature-tile grids (icon + title × 3)
- Unnecessary animations
- Stock photos
- Template-looking layouts
- Placeholder text
- Generic color palettes
- Emoji as icons
- Borders/shadows on everything

---

## Port Ranges

| Range | Category |
|-------|----------|
| 4444 | Reserved |
| 4445 | Dashboard |
| 4450-4499 | Examples |
| 4500-4599 | Tools |
| 4600-4699 | Publishing |
| 4700-4799 | Widgets |
| 4800-4899 | Experimental |

## Rules

- Each site = its own directory under `sites/`
- Each site has `server.js` + `public/`
- Use `process.env.PORT` with fallback
- Self-contained, no shared deps between sites
- Vanilla HTML/CSS/JS (no build step)
- NEVER commit new sites to git (only examples are tracked)
