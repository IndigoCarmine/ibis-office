// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The order of text in sorting: Japanese by its reading (furigana).
//!
//! Text with kanji is ordered by the reading the OS gives it (macOS:
//! CoreFoundation's string tokenizer, Latin transcription turned into
//! hiragana); everything else, and every platform without that service
//! (the Web, for now Windows), by its lower-case text as before. A reading
//! the cell itself carries (`phoneticPr`/`rPh`) is passed in by the caller
//! as `phonetic` and wins over the OS's.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;

/// Whether the text has a kanji (whose order is its reading, not its code).
fn has_kanji(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{3005}'))
}

/// Katakana as hiragana (so a reading in either compares alike), lower case.
fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).into_iter().collect::<Vec<_>>(),
            _ => c.to_lowercase().collect(),
        })
        .collect()
}

thread_local! {
    static CACHE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// The key text is ordered by: its reading when it has kanji (or `phonetic`
/// is given), else its lower-case text.
pub fn sort_key(text: &str, phonetic: Option<&str>) -> String {
    if let Some(p) = phonetic.filter(|p| !p.is_empty()) {
        return fold(p);
    }
    if !has_kanji(text) {
        return text.to_lowercase();
    }
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(k) = c.get(text) {
            return k.clone();
        }
        if c.len() > 200_000 {
            c.clear();
        }
        let k = reading(text).map_or_else(|| text.to_lowercase(), |r| fold(&r));
        c.insert(text.to_string(), k.clone());
        k
    })
}

/// Two texts in sorting order (reading first, then the text itself).
pub fn compare_for_sort(a: &str, b: &str) -> Ordering {
    compare_for_sort_with(a, None, b, None)
}

/// The same, with the readings the cells carry (`rPh`).
pub fn compare_for_sort_with(a: &str, pa: Option<&str>, b: &str, pb: Option<&str>) -> Ordering {
    compare_for_sort_with_case(a, pa, b, pb, false)
}

/// Compare two texts with option to distinguish case (`match_case`).
/// When `match_case` is false (default in Excel), texts differing only by case
/// are considered equal (so multi-level sort keys or stable ordering are respected).
/// When `match_case` is true (Excel's "大文字と小文字を区別する"), uppercase letters
/// precede their lowercase counterparts (e.g. 'A' < 'a') while preserving alphabetical order
/// (e.g. 'A' < 'a' < 'B' < 'b').
pub fn compare_for_sort_with_case(
    a: &str,
    pa: Option<&str>,
    b: &str,
    pb: Option<&str>,
    match_case: bool,
) -> Ordering {
    let ka = sort_key(a, pa);
    let kb = sort_key(b, pb);
    let ord = ka.cmp(&kb);
    if ord != Ordering::Equal {
        return ord;
    }
    if match_case {
        a.cmp(b)
    } else if fold(a) == fold(b) {
        Ordering::Equal
    } else {
        a.cmp(b)
    }
}

#[cfg(target_os = "macos")]
fn reading(text: &str) -> Option<String> {
    mac::reading(text)
}

#[cfg(not(target_os = "macos"))]
fn reading(_text: &str) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    type Ref = *const c_void;
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CFRange {
        location: isize,
        length: isize,
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFStringTransformLatinHiragana: Ref;
        fn CFStringCreateWithBytes(alloc: Ref, bytes: *const u8, len: isize, encoding: u32, external: u8) -> Ref;
        fn CFStringCreateMutableCopy(alloc: Ref, max: isize, s: Ref) -> Ref;
        fn CFStringTransform(s: Ref, range: *mut CFRange, transform: Ref, reverse: u8) -> u8;
        fn CFStringGetLength(s: Ref) -> isize;
        fn CFStringGetCharacters(s: Ref, range: CFRange, buffer: *mut u16);
        fn CFLocaleCreate(alloc: Ref, identifier: Ref) -> Ref;
        fn CFStringTokenizerCreate(alloc: Ref, s: Ref, range: CFRange, options: usize, locale: Ref) -> Ref;
        fn CFStringTokenizerAdvanceToNextToken(t: Ref) -> usize;
        fn CFStringTokenizerGetCurrentTokenRange(t: Ref) -> CFRange;
        fn CFStringTokenizerCopyCurrentTokenAttribute(t: Ref, attribute: usize) -> Ref;
        fn CFRelease(r: Ref);
    }

    const UTF8: u32 = 0x0800_0100;
    const UNIT_WORD: usize = 0;
    const LATIN_TRANSCRIPTION: usize = 1 << 16;

    unsafe fn string(s: &str) -> Ref {
        unsafe { CFStringCreateWithBytes(std::ptr::null(), s.as_ptr(), s.len() as isize, UTF8, 0) }
    }

    unsafe fn to_rust(s: Ref) -> String {
        unsafe {
            let n = CFStringGetLength(s);
            let mut buf = vec![0u16; n as usize];
            CFStringGetCharacters(s, CFRange { location: 0, length: n }, buf.as_mut_ptr());
            String::from_utf16_lossy(&buf)
        }
    }

    /// The text with each word of kanji as its reading in hiragana.
    pub(super) fn reading(text: &str) -> Option<String> {
        unsafe {
            let whole = string(text);
            if whole.is_null() {
                return None;
            }
            let utf16: Vec<u16> = text.encode_utf16().collect();
            let id = string("ja_JP");
            let locale = CFLocaleCreate(std::ptr::null(), id);
            CFRelease(id);
            let tokenizer = CFStringTokenizerCreate(std::ptr::null(), whole, CFRange { location: 0, length: utf16.len() as isize }, UNIT_WORD, locale);
            let mut out = String::new();
            if !tokenizer.is_null() {
                while CFStringTokenizerAdvanceToNextToken(tokenizer) != 0 {
                    let range = CFStringTokenizerGetCurrentTokenRange(tokenizer);
                    let piece = &utf16[range.location as usize..(range.location + range.length) as usize];
                    let latin = CFStringTokenizerCopyCurrentTokenAttribute(tokenizer, LATIN_TRANSCRIPTION);
                    let original = String::from_utf16_lossy(piece);
                    if latin.is_null() || !super::has_kanji(&original) {
                        out.push_str(&original);
                        if !latin.is_null() {
                            CFRelease(latin);
                        }
                        continue;
                    }
                    let copy = CFStringCreateMutableCopy(std::ptr::null(), 0, latin);
                    CFStringTransform(copy, std::ptr::null_mut(), kCFStringTransformLatinHiragana, 0);
                    out.push_str(&to_rust(copy));
                    CFRelease(copy);
                    CFRelease(latin);
                }
                CFRelease(tokenizer);
            }
            if !locale.is_null() {
                CFRelease(locale);
            }
            CFRelease(whole);
            Some(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_ordered_without_regard_to_case() {
        assert_eq!(compare_for_sort("abc", "ABD"), Ordering::Less);
        assert_eq!(sort_key("ABC", None), "abc");
    }

    #[test]
    fn a_given_reading_wins_and_katakana_is_folded() {
        assert_eq!(sort_key("東京", Some("トウキョウ")), "とうきょう");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn regions_are_ordered_by_reading() {
        let mut v = vec!["北海道", "九州", "東北", "関東", "関西"];
        v.sort_by(|a, b| compare_for_sort(a, b));
        assert_eq!(v, ["関西", "関東", "九州", "東北", "北海道"]);
    }

    #[test]
    fn case_sensitive_sort_distinguishes_case_with_uppercase_first() {
        assert_eq!(compare_for_sort_with_case("a", None, "A", None, false), Ordering::Equal);
        assert_eq!(compare_for_sort_with_case("a", None, "A", None, true), Ordering::Greater);
        assert_eq!(compare_for_sort_with_case("A", None, "a", None, true), Ordering::Less);

        let mut v = vec!["b", "B", "a", "A"];
        v.sort_by(|x, y| compare_for_sort_with_case(x, None, y, None, true));
        assert_eq!(v, ["A", "a", "B", "b"]);

        let mut v2 = vec!["apple", "Banana", "Apple", "banana"];
        v2.sort_by(|x, y| compare_for_sort_with_case(x, None, y, None, true));
        assert_eq!(v2, ["Apple", "apple", "Banana", "banana"]);
    }
}
