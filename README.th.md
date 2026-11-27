# SiteBox

แดชบอร์ดและเอนจินสำหรับโฮสต์เว็บสแตติกแบบ self-hosted ที่ “พิสูจน์ได้ว่าดีจริง”

**ไบนารี Rust ตัวเดียว · manifest หนึ่งไฟล์ต่อหนึ่งเว็บ · เซิร์ฟเวอร์ร่วมกัน · ไม่มี `node_modules` แยกต่อเว็บ**

[Read this in English](README.md)

## เริ่มใช้งานเร็ว

```bash
# Docker (แนะนำ — อยู่รอดหลังรีบูต)
docker compose up -d
# http://localhost:4445

# หรือรันจากซอร์ส
cd engine && cargo run --release --bin siteboxd -- \
  --root .. --port 4445 --ui ../dashboard/public
```

เว็บตัวอย่างสองตัวจะถูกตรวจพบจาก manifest เองเมื่อเรียก API ครั้งแรก เปิดได้จากแดชบอร์ด หรือ:

```bash
curl -X POST http://localhost:4445/api/sites/example-notes/start
curl -X POST http://localhost:4445/api/sites/example-clock/start
```

## สถาปัตยกรรม

```
sites/<id>/
├── sitebox.toml      ← สัญญาของเว็บ: พอร์ต ชื่อ โหมดเสิร์ฟ pipeline checks
└── public/           ← สิ่งที่เบราว์เซอร์ได้รับ

engine/               ← โปรเซสเดียวดูแลการเสิร์ฟ, supervision และ checks
├── src/serve.rs      ← static server ร่วม (guard, ETag/304, gzip, Range, 404)
├── src/supervisor.rs ← listener ในโปรเซส + node ลูก + log JSONL
├── src/checks.rs     ← ชุดตรวจ SB0xx (static) / SB1xx (HTTP) + negative control
├── src/manifest.rs   ← sitebox.toml + JSON Schema
├── src/assets.rs     ← แปลงรูปเป็น WebP (ลดขนาดเท่านั้น) + dims.json
├── src/pipeline.rs   ← ขั้นตอนตามลำดับ, fingerprint, scaffold
├── src/deploy.rs     ← mirror + ตรวจ sha256
├── src/dashboard.rs  ← API ที่ UI เรียก
├── src/studio/       ← Sols: agent loop, tools, patches, snapshot, bees, skills
├── assets/sols-widget.js ← ปุ่ม Sols ที่ถูกฉีดเข้าไปในทุกหน้าที่เสิร์ฟ
└── src/bin/          ← sitebox (CLI), siteboxd (dashboard), sitebox-mcp (MCP)

dashboard/public/     ← UI ของแดชบอร์ด
dashboard/data/       ← registry runtime (ไม่ commit)
```

เว็บสแตติกเสิร์ฟในโปรเซสเดียวกัน: หนึ่ง listener ต่อหนึ่งเว็บ ไม่มี Node ลูก เว็บที่ต้องมี API ของตัวเองประกาศ `serve = "node"` และเก็บ `server.js` ไว้เอง — supervisor จะ spawn, ตรวจพอร์ต และเก็บ log ให้ ส่วน `serve = "proxy"` เสิร์ฟไฟล์สแตติกแล้วส่ง `/api/*` ต่อไป upstream

## ความสามารถ

- **เซิร์ฟเวอร์ร่วมตัวเดียว** — กัน path traversal, ETag/304, gzip, Range, security headers และ 404 จริง เหมือนกันทุกเว็บ
- **Manifest ไม่ใช่คำบรรยาย** — `sitebox.toml` อ่านได้ด้วยเครื่องและมี JSON Schema (`sitebox schema`)
- **“เสร็จ” ที่พิสูจน์ได้** — ชุดตรวจที่มี id คงที่ (`SB001`–`SB107`) พร้อม `self-test` ที่พิสูจน์ว่าทุก guard ล้มได้จริง
- **เริ่มแล้วต้องฟังจริง** — `ok:true` เมื่อพอร์ตเปิดอยู่จริงเท่านั้น
- **Log อยู่ถาวร** — วงแหวน 500 บรรทัดในหน่วยความจำ + `logs/<id>.jsonl` บนดิสก์
- **Pipeline มี fingerprint** — ขั้นตอนเดิมไม่รันซ้ำ; คำสั่งหรือ input เปลี่ยนจึงรันใหม่
- **Asset ไม่เพี้ยน** — WebP แบบลดขนาดเท่านั้น, `dims.json` เขียนจากไฟล์จริง, ไม่ครอป
- **Deploy แบบ mirror** — คัดลอก, ลบไฟล์เกิน, แล้วเทียบ sha256
- **MCP server** — 15 tools สำหรับ agent (`sitebox_list`, `check`, `verify`, `build`, `up`, …)
- **Sols เอเจนต์แก้เว็บ** — แผงในแดชบอร์ดและในทุกหน้าเว็บ: เสนอ patch แบบ anchored, stage ในสำเนา, รัน checks, apply พร้อม snapshot และ undo; มี skills ของ SiteBox, ผึ้งงาน (Bee) แบบอ่านอย่างเดียว, ค้นหา/ดึงเว็บ, screenshot และสร้างเว็บใหม่จากบทสนทนา
- ธีมสว่าง/มืด, สีไอคอนกำหนดเอง, favicon ตรวจอัตโนมัติ, ค้นหา/กรอง

## Sols — เอเจนต์แก้เว็บ

Sols เปลี่ยนแดชบอร์ดให้เป็นตัวแก้เว็บของเว็บที่มันเสิร์ฟ เปิดแผงได้จากการ์ดเว็บในแดชบอร์ด หรือจากปุ่ม **Sols** ในหน้าเว็บ พิมพ์สั่งเป็นภาษาธรรมชาติ; เอเจนต์จะอ่าน skill ของเว็บ, วางแผนให้เห็น, แก้ผ่าน patch และพิสูจน์ผลด้วย checks กับ screenshot

### เปิดใช้

```bash
export STUDIO_TOKEN=$(openssl rand -hex 16)   # แผงจะถามครั้งเดียวต่อเบราว์เซอร์
export CHROME=/path/to/chrome                # ไม่บังคับ: screenshot และ probe
export SEARXNG_URL=http://127.0.0.1:8888     # ไม่บังคับ: ค้นเว็บ
export SOLS_FETCH_ALLOW=127.0.0.1:4610       # ไม่บังคับ: อนุญาต host ใน LAN
export SOLS_WIDGET=0                          # ไม่บังคับ: ซ่อนปุ่มในหน้าเว็บ
```

ที่อยู่โมเดล ชื่อโมเดล และ API key ตั้งใน **Studio settings** ของแผง คีย์เป็นแบบ write-only จากเบราว์เซอร์: API ตอบแค่ `has_key` ไม่เคยส่งค่ากลับ และไฟล์ถูกเขียนสิทธิ์ `0600`

### สิ่งที่ทำได้

| Tool | หน้าที่ |
|------|--------|
| `sitebox_propose_patch` | แก้แบบ exact-substring (หรือสร้างไฟล์) พร้อม `count`; stage, ตรวจ, เข้าคิวหรือ apply |
| `sitebox_new_page` | สร้างหน้าใหม่จาก shell ของเว็บ + sections |
| `sitebox_probe` | เรนเดอร์สูงสุด 6 route ที่ 500/760/1440 แล้วรายงาน overflow/clipped/ตัวอักษรเล็ก |
| `sitebox_shot` · `sitebox_check` · `sitebox_verify` | screenshot, checks แบบ static, checks แบบ HTTP |
| `sols_create_site` · `sols_rename_site` | สร้างเว็บจากบทสนทนา; เปลี่ยนชื่อทั้ง manifest และการ์ดแดชบอร์ด |
| `sols_bee` | ผึ้งงานอ่านอย่างเดียว 1–4 ตัว มี brief ของตัวเองและ board ร่วม |
| `sols_search` · `sols_fetch` · `sols_youtube` · `sols_steam` · `sols_asset` | ข้อมูลจริงและรูปภาพ |
| `sols_skill` · `sols_skills` | อ่าน doctrine ของ SiteBox ตามต้องการ |

### หลักความปลอดภัย

- โมเดลเขียนไฟล์เองไม่ได้ รันคำสั่งไม่ได้ และอ่าน filesystem ตรง ๆ ไม่ได้ patch คือคู่ `old`/`new` แบบ anchored พร้อมจำนวนครั้งที่คาดไว้ และทุก path ถูก resolve อยู่ภายในโฟลเดอร์เว็บเดียว
- ทุก patch ถูก stage ในสำเนาแบบ hard link และรัน checks ที่นั่น เฉพาะ check ที่ **patch นี้ทำให้ล้ม** เท่านั้นที่นับ ดังนั้นเว็บที่แดงอยู่ก่อนแล้วจะไม่บล็อกการแก้
- ก่อน apply จะ snapshot (content-addressed เก็บ 20 ชุด) และ rollback เมื่อ check ใหม่ล้ม; undo คืนไฟล์จาก snapshot ล่าสุด
- HTTP เป็น search/fetch แบบอ่านอย่างเดียว ผ่าน SSRF guard; `SOLS_FETCH_ALLOW` เปิด host ที่ระบุ ไม่มี tool ที่เป็น shell
- Bee เห็นเฉพาะ toolbox แบบ leaf (read/grep/outline/check/http/probe/shot) และ patch/build/spawn ไม่ได้

### รอบการทำงานและข้อจำกัด

- เทิร์นแก้ไขจะ **เสนอ** patch ให้ผู้ใช้กด apply; เทิร์นที่สร้างเว็บจะ apply เองเมื่อผ่าน checks
- หนึ่งเทิร์นต่อ **หนึ่งเว็บ**; คนละเว็บรันขนานกันได้ แต่บทสนทนาบนเว็บเดียวกันจะรันทีละเทิร์นเพราะเขียนไฟล์ชุดเดียวกัน
- 16 รอบต่อการแก้ (`max_rounds` ใน settings), 26 รอบตอนสร้างเว็บ, เพดาน 32
- patch ใหญ่สุด 200 KB, ไฟล์ใหญ่สุด 2 MB, สูงสุด 12 patch ต่อครั้ง
- ปิดเบราว์เซอร์แล้วเทิร์นยังทำต่อ; หยุดเมื่อกด Stop, โมเดล error หรือ engine รีสตาร์ท เปิดแผงกลับมาจะเห็น transcript (และปุ่ม Stop ถ้ายังรันอยู่)

### Studio API

| Method | Endpoint | หน้าที่ |
|--------|----------|---------|
| `GET` | `/api/studio/status` | probe สาธารณะ: โมเดล, เว็บ, search, browser |
| `GET`/`PUT` | `/api/studio/config` | settings (`has_key` เท่านั้น ไม่ส่ง key) |
| `GET`/`POST` | `/api/studio/threads` | ดู/สร้างบทสนทนา (`?site=`) |
| `GET` | `/api/studio/threads/:id/messages` | transcript + `running` |
| `POST` | `/api/studio/chat` | เริ่มเทิร์น (202; หนึ่งต่อหนึ่งเว็บ) |
| `POST` | `/api/studio/abort` | หยุดเทิร์น |
| `GET` | `/api/studio/events?thread=` | SSE: text, tool start/end, plan, bees |
| `POST` | `/api/studio/patches/:id/apply` · `/api/studio/sites/:id/undo` | apply / ย้อนกลับ |

ข้อมูล runtime อยู่ใน `dashboard/data/studio/`: `threads/`, `patches/`, `snapshots/`, `shots/`, `stage/`, `skills/` และ `settings.json`

## เพิ่มเว็บ

### CLI (สิ่งที่ agent ควรใช้)

```bash
sitebox --root ~/sitebox new my-site \
  --name "My Site" --port 4500 --description "One real line"
sitebox --root ~/sitebox build my-site
sitebox --root ~/sitebox check my-site
sitebox --root ~/sitebox verify my-site     # เริ่ม, ตรวจ HTTP, แล้วหยุด
```

### API / UI

ปุ่ม **Add Site** ในแดชบอร์ด และ `POST /api/sites` ใช้ได้เหมือนกัน; engine จะสร้างหรืออัปเดต manifest ข้างโฟลเดอร์เว็บให้

### ตัวอย่าง manifest

```toml
schema = 1

[site]
id = "my-site"
name = "My Site"
description = "One real line"
port = 4500
category = "tools"
icon = "globe"
icon_color = "#58a6ff"
path = "sites/my-site"
public_dir = "public"
serve = "static"

[[pipeline]]
name = "assets"
kind = "images"
inputs = ["assets-raw"]
outputs = ["public/img"]
rules = { max_edge = 1080, quality = 82 }
```

## ลบเว็บ

การลบคือลบจริง: หยุด, ลบ `sites/<id>/`, ลบ entry (ไม่เช่นนั้น auto-detect จะใส่กลับมา)

```bash
curl -X DELETE http://localhost:4445/api/sites/my-site
curl -X DELETE "http://localhost:4445/api/sites/my-site?keep=1"   # เอาแค่ entry
```

โฟลเดอร์จะถูกลบเมื่อ path ที่ resolve ได้อยู่ภายใน `sites/` เท่านั้น การลบแบบ purge จะลบบทสนทนาของ Sols, patch ที่ค้าง, snapshot และ screenshot ของเว็บนั้นด้วย; `?keep=1` เก็บโฟลเดอร์และบทสนทนาไว้

## Dashboard API

| Method | Endpoint | คำอธิบาย |
|--------|----------|----------|
| `GET` | `/api/sites` | รายการเว็บ (`_running`, `_stale`, `_manifest`, `_logLines`, `_favicon`) |
| `POST` | `/api/sites` | เพิ่ม/อัปเดต (ตรวจ id, ชื่อ, พอร์ตไม่ซ้ำ) |
| `POST` | `/api/sites/reorder` | บันทึกลำดับการ์ด |
| `DELETE` | `/api/sites/:id` | หยุด, ลบโฟลเดอร์, ลบ entry (`?keep=1` เก็บไฟล์) |
| `POST` | `/api/sites/:id/start` | เริ่มและตรวจว่าพอร์ตฟังจริง |
| `POST` | `/api/sites/:id/stop` | หยุด |
| `POST` | `/api/sites/:id/restart` | หยุด + เริ่ม |
| `GET` | `/api/sites/:id/health` | ตรวจ HTTP |
| `GET` | `/api/sites/:id/icon` | favicon ที่ตรวจพบ (404 ถ้าไม่มี) |
| `GET` | `/api/sites/:id/logs` | วงแหวน + log ที่เก็บไว้ (`?lines=1..500`) |
| `DELETE` | `/api/sites/:id/logs` | ล้าง log |
| `GET` | `/api/ports/check` | พอร์ตว่างหรือไม่ (`?port=4500`) |
| `GET` | `/api/doctor` | ตรวจ registry ↔ โฟลเดอร์ ↔ manifest ↔ พอร์ต |

## Checks

`sitebox check <id>` รันชุด static; `sitebox verify <id>` รันชุด HTTP กับเว็บที่รันอยู่

- `SB001` entry point · `SB002` ห้าม asset บุคคลที่สาม · `SB003` ขนาดรูป · `SB004` splice marker · `SB005` asset ที่อ้างถึงมีจริง
- `SB010` `dims.json` ตรงกับไฟล์ · `SB020` ไม่มีข้อมูล placeholder · `SB021` ไม่มี `NaN`/`undefined` · `SB030` ข้อมูล parse ได้
- `SB101` 200 HTML · `SB102` 404 · `SB103` ETag/304 · `SB104` gzip · `SB105` Range · `SB106` nosniff · `SB107` traversal ถูกปฏิเสธ

สัญญาฉบับเต็ม: `skills/sitebox-engine/SKILL.md` exit code: `0` ผ่าน, `1` ผิดพลาดเชิงปฏิบัติการ, `2` checks ไม่ผ่าน

## Skills

| Skill | หน้าที่ |
|-------|---------|
| `sitebox-engine` | สัญญาที่รันได้: คำสั่ง, check id, กฎ pipeline, MCP tools |
| `sitebox-create` | brief → design → build → register → verify → debug |
| `sitebox-design` | token, ขนาดรูป, audit accessibility/UX, ตรวจ copy |
| `sitebox-data` | เนื้อหาจริง ตรวจสอบที่มา อัปเดตได้: provenance, coverage, fixture |
| `sitebox-verify` | พิสูจน์ layout ด้วยเบราว์เซอร์, negative control, deploy mirror |
| `sitebox-config` | lifecycle API, พอร์ต, log, troubleshooting, นโยบาย git |
| `sitebox-playbook` | รสนิยมของ operator และข้อบกพร่องที่เจอบ่อย (อ่านก่อนสร้างเว็บใหม่) |
| `sitebox-skill-maintainer` | meta: ธรรมเนียมและการตรวจ drift |

```bash
node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
```

## ข้อมูลเว็บและนโยบาย Git

สถานะสองชนิดไม่เข้า repository:

1. `dashboard/data/sites.json` — ถูกเขียนใหม่ตอน runtime โดย add/delete และ auto-detect ไฟล์ที่ commit ใช้บอกเฉพาะเว็บตัวอย่าง
2. **เว็บส่วนตัว** — ทุกอย่างใต้ `sites/` ยกเว้น `example-notes` และ `example-clock` เนื้อหา manifest ข้อมูล และ asset เป็นส่วนตัว

`.gitignore` กันทั้งสองอย่าง และ `.githooks/pre-commit` บล็อกแม้สั่ง force เปิด hook ครั้งเดียวต่อ clone:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit
```

## ช่วงพอร์ต

| ช่วง | ใช้กับ |
|-------|--------|
| 4445 | แดชบอร์ด |
| 4450–4499 | ตัวอย่าง / ทดสอบ |
| 4500–4599 | tools |
| 4600–4699 | publishing |
| 4700–4799 | widgets |
| 4800–4899 | experimental / auto-detected |

## ไอคอน

แดชบอร์ดเลือก favicon ที่เว็บมีจริงก่อน (`<link rel="icon">` แล้วจึง `favicon.svg|png|ico`) เฉพาะไฟล์ในเครื่อง; ไอคอนที่ hotlink หรือ `data:` ถูกมองข้าม มิฉะนั้นจะใช้ไอคอน Lucide — `globe`, `book`, `clock`, `code`, `camera`, `gamepad`, `music`, `settings`, `notebook`, `notebook-pen`, `database`, `star` — โดย `globe` เป็นค่าตั้งต้น

## การทดสอบ

```bash
cd engine
cargo test                 # 40 unit tests ไม่ต้องใช้เน็ตหรือโมเดล
python3 ../scripts/studio-e2e.py   # 90 checks กับโมเดลจำลอง (พอร์ต 4811)
sitebox self-test          # negative control: ทุก check ต้องล้มบน fixture ที่พัง
```

## สิทธิ์การใช้งาน

MIT ไอคอนที่ฝังมาเป็นรูปทรง Lucide SVG (ISC License)
