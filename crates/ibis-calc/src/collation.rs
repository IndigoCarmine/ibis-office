// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! How Excel for Mac orders and compares text: by the system's Japanese
//! collation (CoreFoundation, locale `ja`), without regard to case and
//! width, kana kinds together (あ = ア = ｱ), kanji in the order of JIS X 0208
//! (by reading). Where the system has none, by the characters' codes.

use std::cmp::Ordering;

/// `a` against `b` as Excel for Mac does.
pub(crate) fn compare(a: &str, b: &str) -> Ordering {
    // Letters and digits alone: the same in any collation, and quick.
    if a.bytes().all(|c| c.is_ascii_alphanumeric()) && b.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return a.bytes().map(|c| c.to_ascii_lowercase()).cmp(b.bytes().map(|c| c.to_ascii_lowercase()));
    }
    #[cfg(target_os = "macos")]
    {
        return mac::compare(a, b);
    }
    #[cfg(not(target_os = "macos"))]
    {
        a.to_lowercase().cmp(&b.to_lowercase())
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::cmp::Ordering;
    use std::ffi::c_void;
    use std::sync::OnceLock;

    type Ref = *const c_void;

    #[repr(C)]
    struct Range {
        location: isize,
        length: isize,
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithBytes(alloc: Ref, bytes: *const u8, count: isize, encoding: u32, external: u8) -> Ref;
        fn CFLocaleCreate(alloc: Ref, identifier: Ref) -> Ref;
        fn CFStringCompareWithOptionsAndLocale(a: Ref, b: Ref, range: Range, options: u64, locale: Ref) -> isize;
        fn CFStringGetLength(s: Ref) -> isize;
        fn CFRelease(r: Ref);
    }

    const UTF8: u32 = 0x0800_0100;
    const CASE_INSENSITIVE: u64 = 1;
    const NONLITERAL: u64 = 16;
    const WIDTH_INSENSITIVE: u64 = 32;

    /// The `ja` locale, made once (its address, to be shared between threads).
    fn locale() -> usize {
        static LOCALE: OnceLock<usize> = OnceLock::new();
        *LOCALE.get_or_init(|| unsafe {
            let id = CFStringCreateWithBytes(std::ptr::null(), b"ja".as_ptr(), 2, UTF8, 0);
            let locale = CFLocaleCreate(std::ptr::null(), id);
            CFRelease(id);
            locale as usize
        })
    }

    pub(super) fn compare(a: &str, b: &str) -> Ordering {
        unsafe {
            let make = |s: &str| CFStringCreateWithBytes(std::ptr::null(), s.as_ptr(), s.len() as isize, UTF8, 0);
            let (x, y) = (make(a), make(b));
            if x.is_null() || y.is_null() {
                for r in [x, y] {
                    if !r.is_null() {
                        CFRelease(r);
                    }
                }
                return a.to_lowercase().cmp(&b.to_lowercase());
            }
            let result = CFStringCompareWithOptionsAndLocale(x, y, Range { location: 0, length: CFStringGetLength(x) }, CASE_INSENSITIVE | NONLITERAL | WIDTH_INSENSITIVE, locale() as Ref);
            CFRelease(x);
            CFRelease(y);
            result.cmp(&0)
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::compare;
    use std::cmp::Ordering::{Equal, Greater, Less};

    #[test]
    fn text_is_ordered_as_excel_for_mac_does() {
        // Kanji by reading (JIS X 0208): だい, とう, ふく, めい.
        let mut cities = vec!["名古屋", "福岡", "東京", "大阪"];
        cities.sort_by(|a, b| compare(a, b));
        assert_eq!(cities, ["大阪", "東京", "福岡", "名古屋"]);
        // Case, width and the kinds of kana do not matter; a voiced kana is another.
        assert_eq!(compare("A", "a"), Equal);
        assert_eq!(compare("Ａ", "A"), Equal);
        assert_eq!(compare("あ", "ア"), Equal);
        assert_eq!(compare("ｱ", "ア"), Equal);
        assert_ne!(compare("が", "か"), Equal);
        // Marks, then digits, Latin letters, kana, kanji.
        assert_eq!(compare("!", "1"), Less);
        assert_eq!(compare("1", "a"), Less);
        assert_eq!(compare("a", "あ"), Less);
        assert_eq!(compare("あ", "愛"), Less);
        assert_eq!(compare("東京", "大阪"), Greater);
    }
}
