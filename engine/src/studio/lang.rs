//! Script hygiene for answers.
//!
//! A model that writes Thai sometimes drops a Chinese word into the middle of a
//! clause (a stray ideaograph inside a Thai word). The output still renders, so nothing fails —
//! the operator just sees a bug. The prompt forbids it; this module makes the
//! promise true for the characters that matter.

/// Characters that should never appear in a answer written for a non-CJK
/// operator: CJK ideographs, their compatibility blocks, CJK punctuation and
/// full-width forms.
fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x3000..=0x303F      // CJK symbols and punctuation
        | 0x3400..=0x4DBF    // extension A
        | 0x4E00..=0x9FFF    // unified ideographs
        | 0xF900..=0xFAFF    // compatibility ideographs
        | 0xFF00..=0xFFEF    // full-width forms
        | 0x20000..=0x2FA1F  // extensions B-F
    )
}

pub fn has_cjk(text: &str) -> bool {
    text.chars().any(is_cjk)
}

/// Drop CJK characters. A mixed token becomes the token without the stray
/// characters, which is what the operator would have edited by hand.
pub fn strip_cjk(text: &str) -> String {
    text.chars().filter(|c| !is_cjk(*c)).collect()
}

/// Strip and collapse the whitespace the removal can leave behind.
pub fn clean(text: &str) -> String {
    let stripped = strip_cjk(text);
    let mut out = String::with_capacity(stripped.len());
    let mut last_space = false;
    for c in stripped.chars() {
        let space = c.is_whitespace();
        if space {
            if !last_space {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
        last_space = space;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_and_removes_mixed_scripts() {
        let s = "ถ้าจะให้ผมสร้างเว็บ\u{4E5F}\u{8BA9}ช่วยบอก 3 อย่าง";
        assert!(has_cjk(s));
        assert_eq!(strip_cjk(s), "ถ้าจะให้ผมสร้างเว็บช่วยบอก 3 อย่าง");
        assert!(!has_cjk("ไม่มีอักษรจีนเลย"));
    }

    #[test]
    fn leaves_latin_and_thai_alone() {
        let s = "ตัวอักษร (ABC) — เครื่องหมาย ๑๒๓";
        assert!(!has_cjk(s));
        assert_eq!(strip_cjk(s), s);
    }
}
