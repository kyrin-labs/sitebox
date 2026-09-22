# Copy Passes: Humanizer & Deslop

Copy is design content, not decoration. Run all three passes before a site is considered done — after the first draft and again after any content edits.

Lineage: adapted from the `humanizer` and `deslop` skills on skills.sh, plus Anthropic `frontend-design`'s writing guidance. The rules are tuned for Thai-primary sites with English secondary text.

## Pass 0 — Real content is verbatim

Before humanizing anything, split the copy in two. They have opposite rules.

| | Your UI copy | Content from a real source |
|---|---|---|
| Rule | Rewrite freely — humanize, deslop, make it Thai that reads Thai | **Never touch it.** Verbatim, in its original language |
| Examples | nav labels, buttons, empty states, errors, toasts, `<title>`, meta description | video titles, post captions, comments, usernames, product names |

**Never translate scraped content.** A Japanese video title stays Japanese; a Korean caption stays Korean. Translating it is not "localizing", it is fabricating — and it destroys the only reason the site exists. Relay, Folio and Nearly all ship real content in its source language, mixed with Thai chrome.

This also means the stray-glyph scan has to be **scoped**: real content legitimately contains CJK, Cyrillic and Korean, so scanning the whole file produces false positives. Scan **your UI copy only** — the string tables and CSS. `sitebox-verify` owns that check, and it must fail when it cannot locate the copy block, rather than reporting `✓` over an empty sample.

## Pass 1 — Humanizer

Goal: text that reads like a person who knows the subject wrote it.

### Principles

1. **Specific beats clever.** "ซ่อมจักรยานในเชียงใหม่ตั้งแต่ 2558" beats "ผู้เชี่ยวชาญด้านจักรยาน".
2. **Active voice, plain verbs.** "กดบันทึกแล้วข้อมูลจะถูกส่ง" → "กดบันทึกเพื่อส่งข้อมูล".
3. **One job per element.** A headline orients; a subhead explains; a CTA says what happens. No element does two jobs.
4. **Consistent vocabulary.** The button "เผยแพร่" produces the toast "เผยแพร่แล้ว". Pick one word per concept and reuse it.
5. **Write from the user's side.** Name things by what people see ("การแจ้งเตือน"), not how the system is built ("webhook config").
6. **Thai that reads Thai.** Avoid translationese:
   - "ทำการบันทึก" → "บันทึก"
   - "ซึ่งจะช่วยให้..." chains → split into two sentences
   - "นั้น" / "นี้" trailing every clause → delete most
   - English words where a normal Thai word exists ("ไฮไลต์" is fine, "อิมพลีเมนต์" usually isn't)
7. **Match tone to brand.** Warm shop vs. systems studio are different voices; pick one and hold it.

### Pass procedure

Read each string aloud mentally. If you'd never say it to a customer, rewrite it. Then check naming consistency across buttons, headings, toasts, and errors.

## Pass 2 — Deslop (AI tells in copy)

These patterns signal "generated" even when the design is good. Remove all matches.

### Openers and clichés

| Thai | English | Problem |
|------|---------|---------|
| ในยุคที่... / ในโลกดิจิทัล... | In today's fast-paced world... | Says nothing, delays the point |
| ปลดล็อก / ยกระดับ / ปฏิวัติ | unlock, elevate, revolutionize | Buzzword inflation |
| ไม่ใช่แค่ X แต่เป็น Y | not just X, but Y | Formulaic, used everywhere |
| ก้าวสู่มิติใหม่ | take it to the next level | Empty promise |
| ครบ จบ ในที่เดียว | all-in-one solution | Generic |
| ตอบโจทย์ทุกความต้องการ | meets all your needs | Untestable claim |

### Structural tells

- **Rule of three everywhere.** Three feature cards, three adjectives, three bullet points of equal weight. Keep only what's true; unequal is fine.
- **Fake specificity.** "ผู้ใช้มากกว่า 10,000 คน", "ลดเวลาลง 87%" with no source. Use real numbers or none.
- **Headline fragments as eyebrows.** A small ALL-CAPS label above every heading.
- **Meta strings with middle dots.** "เรียบง่าย · รวดเร็ว · ปลอดภัย".
- **Spaced em-dash labels.** "ร้านกาแฟ — คัดเกรดเอง" repeated on every card.
- **Arrows appended to every link.** "ดูเพิ่มเติม →" on all links.
- **The single accented word.** One word of the headline in a different color.
- **Generic page names.** "หน้าแรก / บริการของเรา / เกี่ยวกับเรา / ติดต่อเรา" with no specificity ("เกี่ยวกับเรา" → "ทำไมเราถึงคั่วกาแฟเอง").
- **Testimonials with invented names and photos.** If there are no real quotes, omit the section.
- **Empty section openers.** "สินค้าของเรา" followed by a grid — the heading adds nothing if the content is obvious.

### Pass procedure

1. Search the copy for each table/pattern above.
2. Delete or rewrite every match. Deleting is usually better than rewording.
3. Re-read for a remaining "marketing fog" feeling: if a sentence survives with a generic noun swapped out, cut it.

## Before / after

**Thai**

| Before | After |
|--------|-------|
| ในยุคดิจิทัลที่เทคโนโลยีเปลี่ยนแปลงอย่างรวดเร็ว เราปลดล็อกศักยภาพของคุณด้วยโซลูชันครบวงจร | รับทำเว็บไซต์ร้านอาหาร เริ่ม 15,000 บาท ส่งงานใน 10 วัน |
| สินค้าของเรา — คุณภาพที่เหนือระดับ | กระดาษจากเยื่อไผ่ ทนความชื้น ใช้ในครัวได้ |
| ไม่ใช่แค่เครื่องชงกาแฟ แต่เป็นประสบการณ์ | เครื่องชงกาแฟสำหรับบ้าน ตั้งค่าเสร็จใน 2 นาที |
| ทีมงานมืออาชีพพร้อมให้บริการ | ช่างซ่อม 3 คน รับงานในกรุงเทพฯ วันเดียวกัน |

**English**

| Before | After |
|--------|-------|
| We elevate your brand with seamless, end-to-end solutions | We design, print, and install shop signs in Bangkok |
| Not just a notebook — a creative companion | Notebook, 120 pages, lays flat |
| Unlock your team's potential today | Cut onboarding from two weeks to three days |

## Where to apply

Headline, subhead, body, CTA labels, nav labels, empty states, error messages, toasts, footer, `<title>`, and `meta description`. The title and description are copy too — write them like a specific sentence, not keywords.

## Done criteria

- Zero matches from the deslop lists.
- Thai copy passes the read-aloud test.
- One vocabulary per concept across the whole site.
- No claim without a real number behind it — and **no number without a source**. If the source does not publish it, omit it; never derive it from something else and label it as the real thing. Nearly shipped a like count computed as 4 % of the view count; the real ratio was 1.21 %, so every number on the page was **3.3× too high**.
- Real content is verbatim and untranslated (Pass 0).
- Your own UI copy contains no borrowed Chinese or Cyrillic characters.
