// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Japanese numerals of `[DBNum1]` (漢数字), `[DBNum2]` (大字) and
//! `[DBNum3]` (全角数字).

const KANJI: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
const DAIJI: [char; 10] = ['〇', '壱', '弐', '参', '四', '伍', '六', '七', '八', '九'];
const FULL: [char; 10] = ['０', '１', '２', '３', '４', '５', '６', '７', '８', '９'];

fn digit(level: u8, d: u8) -> char {
    match level {
        1 => KANJI[d as usize],
        2 => DAIJI[d as usize],
        _ => FULL[d as usize],
    }
}

/// Each ASCII digit of `text` as the level's digit.
pub fn digits(text: &str, level: u8) -> String {
    if !(1..=3).contains(&level) {
        return text.to_string();
    }
    text.chars().map(|c| if c.is_ascii_digit() { digit(level, c as u8 - b'0') } else { c }).collect()
}

/// A whole number written with units (千二百三十四万…).
pub fn positional(int: &str, level: u8) -> String {
    let ds: Vec<u8> = int.bytes().filter(|b| b.is_ascii_digit()).map(|b| b - b'0').collect();
    if ds.iter().all(|d| *d == 0) {
        return digit(level, 0).to_string();
    }
    let small: [&str; 4] = if level == 2 { ["", "拾", "百", "阡"] } else { ["", "十", "百", "千"] };
    let big: [&str; 5] = if level == 2 { ["", "萬", "億", "兆", "京"] } else { ["", "万", "億", "兆", "京"] };
    let mut out = String::new();
    let n = ds.len();
    // Above 兆 there are no more units: the digits above the lowest twelve
    // are the 兆 part, written with units if they are four or fewer.
    if n > 16 {
        let top: String = ds[..n - 12].iter().map(|d| digit(level, *d)).collect();
        out.push_str(&top);
        out.push_str(big[3]);
        let rest: String = ds[n - 12..].iter().map(|d| (b'0' + d) as char).collect();
        let tail = positional(&rest, level);
        if tail != digit(level, 0).to_string() {
            out.push_str(&tail);
        }
        return out;
    }
    // Groups of four from the right.
    let groups = n.div_ceil(4);
    for g in (0..groups).rev() {
        let lo = g * 4;
        let mut group = String::new();
        for k in (0..4).rev() {
            let pos = lo + k;
            if pos >= n {
                continue;
            }
            let d = ds[n - 1 - pos];
            if d == 0 {
                continue;
            }
            // 一 before 十百千 is left out, except in 大字.
            if !(d == 1 && k > 0 && level != 2) {
                group.push(digit(level, d));
            }
            group.push_str(small[k]);
        }
        if !group.is_empty() {
            out.push_str(&group);
            out.push_str(big.get(g).copied().unwrap_or(""));
        }
    }
    out
}

/// A General number's text (`-1234.5678`) in the level's numerals: the
/// integer part with units, the decimals digit by digit.
pub fn general(text: &str, level: u8) -> String {
    if !(1..=3).contains(&level) {
        return text.to_string();
    }
    let (sign, body) = match text.strip_prefix('-') {
        Some(b) => ("-", b),
        None => ("", text),
    };
    // Numbers in the exponent form stay digit by digit.
    if body.contains('E') {
        return format!("{sign}{}", digits(body, level));
    }
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    let mut out = format!("{sign}{}", positional(int, level));
    if !frac.is_empty() {
        out.push('.');
        out.push_str(&digits(frac, level));
    }
    out
}

/// A number in a date (a day, an hour, an era year) in the level's
/// numerals: a four-digit year or a zero-padded number digit by digit,
/// others with units; full-width digits digit by digit.
pub fn date_number(text: &str, level: u8) -> String {
    if level == 3 || text.len() >= 4 || (text.len() > 1 && text.starts_with('0')) {
        return digits(text, level);
    }
    positional(text, level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numerals() {
        assert_eq!(general("1234.5678", 1), "千二百三十四.五六七八");
        assert_eq!(general("12345678", 1), "千二百三十四万五千六百七十八");
        assert_eq!(general("1230000000000000", 1), "千二百三十兆");
        assert_eq!(general("123456789012", 2), "壱阡弐百参拾四億伍阡六百七拾八萬九阡壱拾弐");
        assert_eq!(general("123456789012", 3), "千２百３十４億５千６百７十８万９千十２");
        assert_eq!(general("-0.001", 1), "-〇.〇〇一");
        assert_eq!(general("0", 2), "〇");
        assert_eq!(date_number("12", 2), "壱拾弐");
        assert_eq!(date_number("00", 1), "〇〇");
        assert_eq!(date_number("1900", 1), "一九〇〇");
    }
}
