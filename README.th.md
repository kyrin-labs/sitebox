# SiteBox

SiteBox เก็บเว็บสแตติกหลาย ๆ ไว้บนเครื่องเดียว แดชบอร์ดกับเอนจินรวมอยู่ในไบนารี Rust ตัวเดียว แต่ละเว็บมี `sitebox.toml` ไฟล์เดียว ไม่ต้องมี `node_modules` แยกทีละเว็บ

[Read this in English](README.md)

## ทำไมถึงทำ

ผมอยากได้ที่วางเว็บเล็ก ๆ หลายอันโดยไม่ต้องดูแล Docker หลายตัวหรือ Node หลายชุด SiteBox เลยเสิร์ฟทุกเว็บจากโปรเซสเดียว มีตัวตรวจว่าเว็บยังตอบจริง มี log ให้ดู และมีคำสั่ง build/check/verify ที่ผลออกมาแน่นอน

ข้อแลกเปลี่ยนก็ตรงไปตรงมา: ทุกเว็บใช้โค้ดเสิร์ฟชุดเดียวกัน ถ้าเว็บไหนอยากได้ API ของตัวเองก็ต้องประกาศ `serve = "node"` แล้วดูแล `server.js` เอง

## เริ่มใช้

```bash
docker compose up -d
# เปิด http://localhost:4445
```

หรือรันจากซอร์ส:

```bash
cd engine && cargo run --release --bin siteboxd -- \
  --root .. --port 4445 --ui ../dashboard/public
```

ตอนเรียก API ครั้งแรก ตัวตรวจ manifest จะเจอเว็บตัวอย่างสองตัวให้เอง เปิดจากแดชบอร์ด หรือสั่งตรง ๆ ก็ได้:

```bash
curl -X POST http://localhost:4445/api/sites/example-notes/start
curl -X POST http://localhost:4445/api/sites/example-clock/start
```

## ข้างในมีอะไร

```
sites/<id>/
├── sitebox.toml      ← พอร์ต ชื่อ โหมดเสิร์ฟ pipeline checks
└── public/           ← ไฟล์ที่เบราว์เซอร์ได้

engine/
├── src/serve.rs      ← static server ร่วม (ETag/304, gzip, Range, 404, กัน traversal)
├── src/supervisor.rs ← listener ในโปรเซส และ node ลูกสำหรับเว็บที่ต้องการ
├── src/checks.rs     ← ชุดตรวจ SB0xx (static) และ SB1xx (HTTP)
├── src/manifest.rs   ← sitebox.toml + JSON Schema
├── src/assets.rs     ← แปลงรูปเป็น WebP (ลดขนาดเท่านั้น) + dims.json
├── src/pipeline.rs   ← ขั้นตอนตามลำดับ, fingerprint, scaffold
├── src/deploy.rs     ← mirror แล้วเทียบ sha256
├── src/dashboard.rs  ← API ที่ UI เรียก
├── src/studio/       ← Sols: agent loop, tools, patch, snapshot, bees, skills
├── assets/sols-widget.js ← ปุ่ม Sols ที่ฉีดเข้าไปในหน้าที่เสิร์ฟ
└── src/bin/          ← sitebox (CLI), siteboxd (แดชบอร์ด), sitebox-mcp (MCP)

dashboard/public/     ← UI รวมแผง Sols
dashboard/data/       ← registry และข้อมูลตอนรันของ Sols
```

เว็บสแตติกเสิร์ฟจากโปรเซสเดียวกัน เว็บละ listener ไม่ต้องมี Node ลูก ส่วนเว็บที่อยากได้ API ของตัวเองให้ประกาศ `serve = "node"` supervisor จะ spawn ให้ ตรวจพอร์ตและเก็บ log ไว้ให้ ส่วน `serve = "proxy"` เสิร์ฟไฟล์แล้วส่ง `/api/*` ต่อไป upstream

## ทำอะไรได้บ้าง

- เซิร์ฟเวอร์ร่วมตัวเดียว: กัน path traversal, ETag/304, gzip, Range และ security headers ทุกเว็บที่เอนจินเสิร์ฟได้ 404 จริง
- `sitebox.toml` อ่านด้วยเครื่องได้ มี JSON Schema (`sitebox schema`)
- ชุดตรวจที่ id คงที่ (`SB001` ถึง `SB107`) พร้อม `self-test` ที่พิสูจน์ว่าทุก guard ล้มได้จริง
- `ok:true` ตอน start ออกให้ก็ต่อเมื่อพอร์ตเปิดอยู่จริง
- log 500 บรรทัดล่าสุดในหน่วยความจำ และเก็บลง `logs/<id>.jsonl`
- pipeline จำ fingerprint ขั้นตอนที่ไม่เปลี่ยนจะไม่รันซ้ำ
- รูปถูกแปลงเป็น WebP ครั้งเดียว ลดขนาดเท่านั้น ไม่มีการครอป `dims.json` เขียนจากไฟล์ที่ส่งจริง
- deploy แบบ mirror: คัดลอก ลบไฟล์เกิน แล้วเทียบ sha256
- MCP server 15 tools สำหรับ agent
- Sols เอเจนต์ที่แก้เว็บในแดชบอร์ด พร้อมแผงแชทที่แนบภาพได้
- วางโฟลเดอร์ที่มี `sitebox.toml` (หรือ `server.js` แบบเก่า) ลงใน `sites/` แล้วระบบเจอเอง
- ธีมสว่าง-มืด สีไอคอน favicon ที่ตรวจเจอเอง และช่องค้นหา/กรอง

## Sols: เอเจนต์ที่แก้เว็บให้

Sols อยู่ในแผงแชทของแดชบอร์ด และมีปุ่มลอยบนหน้าที่เอนจินเสิร์ฟ บอกเป็นภาษาไทยว่าอยากได้อะไร มันจะอ่าน skill ของเว็บ เสนอ patch รัน checks แล้วส่ง screenshot มาให้ดู

มันไม่ได้เขียนไฟล์ตรง ๆ ทุกการแก้คือ patch ที่ระบุข้อความเดิมกับข้อความใหม่ ชัดเจนว่าแทนตรงไหนกี่ครั้ง ตัวเอนจินเอา patch ไปทดลองในสำเนาก่อน รัน checks แล้วเทียบว่ามีอะไรพังเพิ่ม จากนั้นค่อย apply พร้อม snapshot เผื่ออยากย้อน

### เปิดใช้

```bash
export STUDIO_TOKEN=$(openssl rand -hex 16)   # ไม่ใส่ก็ได้ ใส่เพื่อให้สคริปต์ส่ง x-studio-token
export CHROME=/path/to/chrome                # ไม่ใส่ก็ได้ แต่จะไม่มี screenshot/probe
export SEARXNG_URL=http://127.0.0.1:8899     # ไม่ใส่ก็ได้ ถ้าอยากให้ค้นเว็บได้
export SOLS_FETCH_ALLOW=127.0.0.1:4610       # เปิด host ใน LAN ให้ fetch ได้
export SOLS_WIDGET=0                          # ซ่อนปุ่มในหน้าเว็บ
```

ที่อยู่โมเดล ชื่อโมเดล และ API key ตั้งใน Studio settings ของ panel คีย์ถูกเก็บแบบ write-only: API ตอบแค่ว่ามีคีย์ไหม ไม่เคยส่งค่ากลับ และไฟล์เป็นสิทธิ์ `0600`

### ทำอะไรได้

| Tool | หน้าที่ |
|------|--------|
| `sitebox_propose_patch` | แก้ข้อความแบบ exact match หรือสร้างไฟล์ใหม่ พร้อมจำนวนครั้งที่ต้องเจอ |
| `sitebox_new_page` | สร้างหน้าจาก shell เดิมของเว็บ ใส่แค่เนื้อหา |
| `sitebox_probe` | เรนเดอร์สูงสุด 6 route ที่ความกว้างเดียวต่อการเรียก (ค่าตั้งต้น 375 px เพดาน 1440) แล้วบอกว่ามีอะไรล้นจอ ข้อความถูกตัด หรือตัวอักษรเล็กเกิน |
| `sitebox_shot` `sitebox_check` `sitebox_verify` | screenshot, checks แบบ static, checks แบบ HTTP |
| `sols_create_site` `sols_rename_site` | สร้างเว็บจากบทสนทนา และเปลี่ยนชื่อทั้ง manifest กับการ์ดในแดชบอร์ด |
| `sols_bee` | ผึ้งงาน (bee) 1 ถึง 4 ตัว อ่านอย่างเดียว มี brief ของตัวเอง และใช้ board ร่วมกัน |
| `sols_search` `sols_fetch` `sols_youtube` `sols_steam` `sols_asset` | ค้นเว็บ ดึงหน้าเว็บ อ่านช่อง YouTube ข้อมูลร้าน Steam และโหลดรูป |
| `sols_plan` `sols_design` `sols_import` | วางแผนงาน จำ design ของแต่ละเว็บ และดึงหน้าเว็บเดิมเข้ามา |
| `sols_skill` `sols_skills` | อ่านคู่มือของ SiteBox รวม `sitebox-playbook` ตามต้องการ |

### แนบภาพ

หนึ่งข้อความแนบได้สูงสุด 4 ภาพ กดปุ่ม paperclip เพื่อเลือกไฟล์ หรือวางภาพจาก clipboard ได้เลย ภาพละไม่เกิน 8 MB และต้องเป็น png, jpeg, webp หรือ gif โดยตรวจ magic bytes ไม่ใช่แค่นามสกุล

ภาพถูกเก็บใต้ `dashboard/data/studio/uploads/<thread>/` แล้วส่งให้โมเดลเป็น image part คู่กับข้อความ ถ้าโมเดลที่ตั้งไว้อ่านภาพไม่ได้ ระบบจะปฏิเสธก่อนเริ่มเทิร์น: `400 vision_unsupported` พร้อมชื่อโมเดล หรือ `503 vision_unknown` เมื่อตรวจไม่ได้ คำตอบถูก cache ต่อ endpoint + model ใน `vision.json`

### ความปลอดภัย

โมเดลไม่มีสิทธิ์เขียนไฟล์ รันคำสั่ง หรืออ่าน filesystem ตรง ๆ ทางเดียวที่มันแตะไฟล์ได้คือ patch และทุก path ถูกแปลงให้อยู่ใต้โฟลเดอร์ของเว็บเดียว

patch ทุกอันถูกทดลองในสำเนาแบบ hard link และรัน checks ที่นั่น นับเฉพาะ check ที่ patch นี้ทำให้พังเพิ่ม เว็บที่แดงอยู่ก่อนแล้วจึงไม่บล็อกการแก้

ก่อน apply จะ snapshot ไว้ (เก็บ 20 ชุด) ถ้า check ใหม่ล้มก็ rollback ให้ ส่วน undo จะคืนไฟล์จาก snapshot ล่าสุด

การเรียก HTTP เป็น search/fetch แบบอ่านอย่างเดียวและผ่าน SSRF guard ถ้าอยากให้เข้าถึง host ใน LAN ต้องระบุใน `SOLS_FETCH_ALLOW` ไม่มี tool ที่รัน shell

ผึ้งงานเห็นแค่ tool ชุดอ่าน (read, grep, outline, check, http, probe, shot) patch, build หรือ spawn อะไรไม่ได้

### ข้อจำกัด

- หนึ่งเทิร์นคือคำขอหนึ่งครั้งจากคุณ หนึ่งรอบคือรอบของ agent loop ภายในเทิร์นนั้น
- เทิร์นแก้ไขจะเสนอ patch ให้กด apply เอง เว้นแต่สลับบทสนทนาเป็น **All-in** (`POST /api/studio/threads/:id/mode`) ซึ่ง apply ให้ทันทีที่ผ่าน checks ส่วนเทิร์นที่สร้างเว็บ apply ให้เองเสมอ
- หนึ่งเทิร์นต่อหนึ่งเว็บ คนละเว็บรันพร้อมกันได้ แต่สองบทสนทนาบนเว็บเดียวกันจะรันทีละอัน เพราะเขียนไฟล์ชุดเดียวกัน
- 16 รอบต่อเทิร์นการแก้ (`max_rounds` ใน settings), 26 รอบต่อเทิร์นสร้างเว็บ, เพดาน 32
- patch ใหญ่สุด 200 KB, ไฟล์ใหญ่สุด 2 MB, 12 patch ต่อครั้ง
- ข้อความยาวสุด 8,000 ตัวอักษร, 60 เทิร์นต่อเว็บต่อชั่วโมง, หน้าที่ generate ได้ใหญ่สุด 120 KB
- ปิดเบราว์เซอร์แล้วเทิร์นยังทำต่อ จะหยุดเมื่อกด Stop, โมเดล error หรือเอนจินรีสตาร์ท เปิด panel กลับมาจะเห็น transcript และปุ่ม Stop ถ้ายังรันอยู่

### Studio API

| Method | Endpoint | หน้าที่ |
|--------|----------|---------|
| `GET` | `/api/studio/status` | probe สาธารณะ: โมเดล เว็บ search browser |
| `GET` `PUT` | `/api/studio/config` | settings (`has_key` เท่านั้น) |
| `GET` `POST` | `/api/studio/threads` | ดูและสร้างบทสนทนา (`?site=`) |
| `GET` | `/api/studio/threads/:id/messages` | transcript และ `running` |
| `POST` | `/api/studio/chat` | เริ่มเทิร์น (202, หนึ่งต่อหนึ่งเว็บ) |
| `POST` | `/api/studio/abort` | หยุดเทิร์น |
| `GET` | `/api/studio/events?thread=` | SSE: ข้อความ, tool start/end, plan, bees |
| `POST` | `/api/studio/threads/:id/mode` | สลับโหมด ask/all-in |
| `GET` | `/api/studio/threads/:id/timings` | เวลาต่อรอบของบทสนทนา |
| `GET` | `/api/studio/uploads/:thread/:file` | เสิร์ฟภาพที่แนบกลับให้แผง |
| `POST` | `/api/studio/patches/:id/apply` | apply patch |
| `POST` | `/api/studio/sites/:id/undo` | ย้อนกลับ snapshot ล่าสุด |

ข้อมูลตอนรันอยู่ใน `dashboard/data/studio/` มี `threads/`, `patches/`, `snapshots/`, `shots/`, `stage/`, `uploads/` และไฟล์ `settings.json`, `vision.json`, `design.json`, `audit.jsonl`

## เพิ่มเว็บ

```bash
sitebox --root ~/sitebox new my-site \
  --name "My Site" --port 4500 --description "One real line"
sitebox --root ~/sitebox build my-site
sitebox --root ~/sitebox check my-site
sitebox --root ~/sitebox verify my-site     # start, ตรวจ HTTP, แล้วหยุด
```

ในแดชบอร์ดใช้ปุ่ม Add Site หรือ `POST /api/sites` ก็ได้ เอนจินจะสร้างหรืออัปเดต manifest ให้เอง

ตัวอย่าง manifest:

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

ลบคือลบจริง หยุดเว็บ ลบโฟลเดอร์ แล้วเอา entry ออก (ถ้าไม่เอาออก auto-detect จะใส่กลับมา)

```bash
curl -X DELETE http://localhost:4445/api/sites/my-site
curl -X DELETE "http://localhost:4445/api/sites/my-site?keep=1"   # เอาแค่ entry
```

จะลบโฟลเดอร์ก็ต่อเมื่อ path ที่ได้ออกมาอยู่ใต้ `sites/` เท่านั้น ถ้าลบแบบ purge บทสนทนาของ Sols, patch ที่ค้าง, snapshot และ screenshot ของเว็บนั้นจะถูกลบไปด้วย ถ้าใช้ `?keep=1` ทั้งโฟลเดอร์และบทสนทนายังอยู่

## Dashboard API

| Method | Endpoint | คำอธิบาย |
|--------|----------|----------|
| `GET` | `/api/sites` | รายการเว็บ (`_running`, `_stale`, `_manifest`, `_logLines`, `_favicon`) |
| `POST` | `/api/sites` | เพิ่มหรืออัปเดต (ตรวจ id, ชื่อ, พอร์ตไม่ซ้ำ) |
| `POST` | `/api/sites/reorder` | บันทึกลำดับการ์ด |
| `DELETE` | `/api/sites/:id` | หยุด ลบโฟลเดอร์ ลบ entry (`?keep=1` เก็บไฟล์) |
| `POST` | `/api/sites/:id/start` | เริ่มและตรวจว่าพอร์ตฟังจริง |
| `POST` | `/api/sites/:id/stop` | หยุด |
| `POST` | `/api/sites/:id/restart` | หยุดแล้วเริ่ม |
| `GET` | `/api/sites/:id/health` | ตรวจ HTTP |
| `GET` | `/api/sites/:id/icon` | favicon ที่ตรวจเจอ (404 ถ้าไม่มี) |
| `GET` | `/api/sites/:id/logs` | log ในหน่วยความจำและที่เก็บไว้ (`?lines=1..500`) |
| `DELETE` | `/api/sites/:id/logs` | ล้าง log |
| `GET` | `/api/ports/check` | พอร์ตว่างไหม (`?port=4500`) |
| `GET` | `/api/doctor` | ตรวจ registry, โฟลเดอร์, manifest และพอร์ตว่าตรงกันไหม |

## Checks

`sitebox check <id>` รันชุด static ส่วน `sitebox verify <id>` รันชุด HTTP กับเว็บที่เปิดอยู่

- `SB001` มี entry point, `SB002` ไม่มี asset จากบุคคลที่สาม, `SB003` ขนาดรูป, `SB004` splice marker, `SB005` asset ที่อ้างถึงมีจริง
- `SB010` `dims.json` ตรงกับไฟล์, `SB020` ไม่มีข้อมูล placeholder, `SB021` ไม่มี `NaN` หรือ `undefined`, `SB030` ข้อมูล parse ได้
- `SB101` ได้ 200, `SB102` 404 ทำงาน, `SB103` ETag/304, `SB104` gzip, `SB105` Range, `SB106` nosniff, `SB107` traversal ถูกปฏิเสธ

สัญญาฉบับเต็มอยู่ใน `skills/sitebox-engine/SKILL.md` exit code: `0` ผ่าน, `1` ผิดพลาดตอนทำงาน, `2` checks ไม่ผ่าน

## Skills

| Skill | ใช้ตอนไหน |
|-------|-----------|
| `sitebox-engine` | คำสั่ง check id กฎ pipeline และ MCP tools |
| `sitebox-create` | brief → design → build → register → verify → debug |
| `sitebox-design` | token ขนาดรูป audit accessibility/UX และตรวจ copy |
| `sitebox-data` | เนื้อหาจริงที่ตรวจที่มาได้และอัปเดตได้ |
| `sitebox-verify` | พิสูจน์ layout ด้วยเบราว์เซอร์และ deploy mirror |
| `sitebox-config` | lifecycle API พอร์ต log และ troubleshooting |
| `sitebox-playbook` | รสนิยมของเจ้าของเว็บและข้อผิดพลาดที่เจอบ่อย |
| `sitebox-skill-maintainer` | ธรรมเนียมการเขียน skill และตัวตรวจ drift |

```bash
node skills/sitebox-skill-maintainer/scripts/check-skills.mjs
```

## ข้อมูลเว็บกับ git

`dashboard/data/sites.json` ถูก track ไว้และเก็บแค่เว็บตัวอย่าง ตอนรันเอนจินเขียนไฟล์นี้ใหม่ เลยเห็น diff ในเครื่องได้ปกติ

เว็บส่วนตัวทุกอันใต้ `sites/` ยกเว้น `example-notes` กับ `example-clock` ไม่เข้า repository และ `.githooks/pre-commit` กันไว้อีกชั้นแม้จะสั่ง force

เปิด hook ครั้งเดียวต่อ clone:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit
```

## ช่วงพอร์ต

| ช่วง | ใช้กับ |
|-------|--------|
| 4445 | แดชบอร์ด |
| 4450-4499 | ตัวอย่างและเทสต์ |
| 4500-4599 | tools |
| 4600-4699 | publishing |
| 4700-4799 | widgets |
| 4800-4899 | experimental และ auto-detect |

## ไอคอน

แดชบอร์ดจะใช้ favicon ที่เว็บมีจริงก่อน (`<link rel="icon">` แล้วค่อย `favicon.svg|png|ico`) อ่านเฉพาะไฟล์ในเครื่อง ไอคอนที่ hotlink หรือเป็น `data:` จะถูกข้าม ถ้าไม่มีก็ใช้ไอคอน Lucide: `globe`, `book`, `clock`, `code`, `camera`, `gamepad`, `music`, `settings`, `notebook`, `notebook-pen`, `database`, `star` ถ้าไม่ระบุจะใช้ `globe`

## เทสต์

```bash
cd engine
cargo test                 # 41 unit tests ไม่ต้องใช้เน็ตหรือโมเดล
python3 ../scripts/studio-e2e.py   # 90 checks กับโมเดลจำลองที่พอร์ต 4811
sitebox self-test          # negative control: ทุก check ต้องล้มบน fixture ที่พัง
```

## ความปลอดภัย

แดชบอร์ดไม่มีระบบล็อกอินและไม่มีสิทธิ์แยกต่ออุปกรณ์ ใครโหลดหน้าแดชบอร์ดได้ก็สร้าง ลบ หรือจัดลำดับเว็บได้หมด เราตั้งใจให้เป็นแบบนี้ ด่านจริงคือเครือข่าย ไม่ใช่หน้าล็อกอิน

| ชั้น | ทำอะไร |
|------|--------|
| ขอบเขตเครือข่าย | แดชบอร์ดฟังที่ `0.0.0.0:4445` ไม่มีตัวแปรให้เปลี่ยน bind ถ้าจะให้เข้าถึงได้แค่เครื่องนี้ต้องกันที่ไฟร์วอลล์ หรือวางไว้หลัง tailscale / SSH tunnel |
| Studio token | เฉพาะ `/api/studio/*` ที่ตรวจ `x-studio-token` (สำหรับสคริปต์) หรือ cookie `studio_sid` ที่เบราว์เซอร์ได้ตอนโหลด ส่วน API จัดการเว็บไม่มี auth |
| Origin | CORS ตอบ origin ของผู้เรียกให้ปุ่ม Sols เท่านั้น ไม่ใช่ตัวกัน write เพราะ write ไม่ได้แนบ credential อะไรมาอยู่แล้ว |
| Apply audit | `dashboard/data/studio/audit.jsonl` บรรทัดละครั้งต่อ patch ที่ apply หรือ undo การเขียนอื่นในแดชบอร์ดไม่ถูกบันทึก |
| Delete brake | ตอนลบต้องพิมพ์ id ของเว็บ กันกดพลาด ไม่ได้กันสคริปต์ในเครือข่ายเดียวกัน |

ผลที่ตามมา:

- ใครอยู่ใน LAN เดียวกันทำได้เท่าที่คุณทำได้ วางเครือข่ายให้เหมาะ
- เว็บ `serve = "node"` ผูก `127.0.0.1` แล้วออกผ่าน proxy ของเอนจิน ส่วน `static` กับ `proxy` ผูก `0.0.0.0` ตามพอร์ตใน manifest
- request body จำกัด 4 MB ยกเว้น `/api/studio/chat` ที่ 48 MB เพื่อให้ภาพ 8 MB สี่ภาพใส่ base64 ได้ ถ้าไม่มีเพดาน POST ก้อนเดียวก็น็อกแดชบอร์ดและทุกเว็บได้
