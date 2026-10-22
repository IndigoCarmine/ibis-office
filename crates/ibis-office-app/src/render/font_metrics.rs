// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! How high the Office apps make a line of text and where they put its
//! baseline, shared by the formats.
//!
//! The Office apps take a font's height from its Windows ascent and descent
//! (`OS/2` `usWinAscent` / `usWinDescent`), or from its typographic ones
//! when the font asks for those (`fsSelection` bit 7, `USE_TYPO_METRICS`:
//! Aptos). macOS's text system reports `hhea`'s, which differ for some of
//! the fonts Office uses (Yu Gothic: 0.88 / 0.22 against 0.985 / 0.302;
//! Consolas, Candara, Corbel, Constantia, Century Gothic). Measured against
//! the PDFs PowerPoint, Excel and Word make (the text's coordinates in them).
//!
//! - [`font_file_metrics`] reads the ascent and descent from a font file;
//!   `Resources::line_metrics` uses it for the fonts drawn.
//! - [`office_metrics`] has the values of the typefaces Office uses, for
//!   when the typeface is not installed and another font stands in for it.
//! - [`powerpoint_below_baseline`] is where PowerPoint puts the baseline in
//!   a line (`rich_text` lays out the lines with it).
//!
//! For Word (docx): a line is the font's height, `(ascent + descent) ×
//! size`, and the baseline its ascent below the line's top (commit cf33cf3
//! of the docx branch). `Resources::line_metrics(font)` gives the ascent and
//! descent of the font drawn in ems, from its file (as Word takes them), and
//! [`office_metrics`] (the typeface's name) those of a typeface that is not
//! installed; together they replace `docx::flow_layout::word_metrics`.

/// The height of a single-spaced line in PowerPoint, in multiples of the
/// font's size, whatever the font (measured: Calibri, Arial, Times New
/// Roman, Cambria, Century, Aptos, 游ゴシック, 游明朝, メイリオ, ＭＳ ゴシック at
/// 8 to 54 pt all advance 1.2 × size a line).
pub(crate) const POWERPOINT_SINGLE: f32 = 1.2;

/// The ascent and descent (ems) of the font in `data` (the `index`th of a
/// collection): its Windows ones, or its typographic ones when it asks for
/// them to be used. `None` when the file cannot be read.
pub(crate) fn font_file_metrics(data: &[u8], index: u32) -> Option<(f32, f32)> {
    let u16_at = |at: usize| -> Option<u16> { Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?])) };
    let u32_at = |at: usize| -> Option<u32> { Some(u32::from_be_bytes([*data.get(at)?, *data.get(at + 1)?, *data.get(at + 2)?, *data.get(at + 3)?])) };
    let offset = if data.starts_with(b"ttcf") {
        let count = u32_at(8)?;
        if index >= count {
            return None;
        }
        u32_at(12 + 4 * index as usize)? as usize
    } else {
        0
    };
    let tables = u16_at(offset + 4)? as usize;
    let find = |tag: &[u8; 4]| -> Option<usize> {
        (0..tables).find_map(|i| {
            let record = offset + 12 + 16 * i;
            (data.get(record..record + 4)? == tag).then(|| u32_at(record + 8).map(|o| o as usize)).flatten()
        })
    };
    let head = find(b"head")?;
    let em = f32::from(u16_at(head + 18)?);
    if em <= 0.0 {
        return None;
    }
    let os2 = find(b"OS/2")?;
    let selection = u16_at(os2 + 62)?;
    let (ascent, descent) = if selection & 0x80 != 0 {
        let ascent = u16_at(os2 + 68)? as i16;
        let descent = u16_at(os2 + 70)? as i16;
        (f32::from(ascent), -f32::from(descent))
    } else {
        (f32::from(u16_at(os2 + 74)?), f32::from(u16_at(os2 + 76)?))
    };
    (ascent > 0.0 && descent >= 0.0).then(|| (ascent / em, descent / em))
}

/// How much an East Asian font's line is taller than its Windows ascent and
/// descent in Word for Windows (see [`word_windows_line_of`]).
pub(crate) const WORD_EAST_ASIAN_EXTRA: f32 = 0.3;

/// A single-spaced line of Word for Windows and where its baseline is in
/// it (ems: the ascent, the line), for a font of Windows ascent and descent
/// `win_ascent` / `win_descent`, of `hhea` ascent, descent and line gap
/// `hhea_line` in all, that is or is not an East Asian font. Measured on
/// Word's PDFs (Word for Microsoft 365, Windows; 22 fonts at 10, 10.5 and
/// 20 pt, no line grid):
///
/// - a Latin font's line is GDI's, its height and external leading:
///   `max(win, hhea_line)` (Arial 1.150, Times New Roman 1.150, Arial Narrow
///   1.147, Calibri 1.221, Segoe UI 1.330), the external leading above the
///   text;
/// - an East Asian font's (one with a CJK code page) is 1.3 × its Windows
///   height (ＭＳ 明朝 1.296, 游明朝 1.672, メイリオ 1.948, Arial Unicode MS
///   1.744), the 0.3 split evenly above and below the text.
///
/// Word for Mac differs (游ゴシック 1.43 em: its `hhea` ascent and descent
/// × 1.3), which is what the other platforms keep.
pub(crate) fn word_windows_line_of(win_ascent: f32, win_descent: f32, hhea_line: f32, east_asian: bool) -> (f32, f32) {
    let win = win_ascent + win_descent;
    if east_asian {
        (win_ascent + WORD_EAST_ASIAN_EXTRA / 2.0 * win, (1.0 + WORD_EAST_ASIAN_EXTRA) * win)
    } else {
        let line = win.max(hhea_line);
        (win_ascent + line - win, line)
    }
}

/// [`word_windows_line_of`] for the font in `data` (the `index`th of a
/// collection), and whether the font is an East Asian one (has a CJK code
/// page): `None` when the file cannot be read.
pub(crate) fn word_windows_font(data: &[u8], index: u32) -> Option<((f32, f32), bool)> {
    let u16_at = |at: usize| -> Option<u16> { Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?])) };
    let u32_at = |at: usize| -> Option<u32> { Some(u32::from_be_bytes([*data.get(at)?, *data.get(at + 1)?, *data.get(at + 2)?, *data.get(at + 3)?])) };
    let offset = if data.starts_with(b"ttcf") {
        let count = u32_at(8)?;
        if index >= count {
            return None;
        }
        u32_at(12 + 4 * index as usize)? as usize
    } else {
        0
    };
    let tables = u16_at(offset + 4)? as usize;
    let find = |tag: &[u8; 4]| -> Option<usize> {
        (0..tables).find_map(|i| {
            let record = offset + 12 + 16 * i;
            (data.get(record..record + 4)? == tag).then(|| u32_at(record + 8).map(|o| o as usize)).flatten()
        })
    };
    let em = f32::from(u16_at(find(b"head")? + 18)?);
    if em <= 0.0 {
        return None;
    }
    let hhea = find(b"hhea")?;
    let hhea_line = f32::from(u16_at(hhea + 4)? as i16) - f32::from(u16_at(hhea + 6)? as i16) + f32::from(u16_at(hhea + 8)? as i16);
    let os2 = find(b"OS/2")?;
    let (ascent, descent) = (f32::from(u16_at(os2 + 74)?), f32::from(u16_at(os2 + 76)?));
    // JIS, GB 2312, Korean Wansung, Big5, Johab (`ulCodePageRange1` bits 17 to 21, OS/2 version 1 on).
    let east_asian = u16_at(os2)? >= 1 && u32_at(os2 + 78)? >> 17 & 0x1f != 0;
    (ascent > 0.0).then(|| (word_windows_line_of(ascent / em, descent / em, hhea_line / em, east_asian), east_asian))
}

/// The ascent and descent (ems) Office gives the typeface `name` (as a
/// document names it), from the fonts it ships (`OS/2`); `None` for other
/// typefaces.
///
/// ＭＳ ゴシック・ＭＳ 明朝 (and their P and UI forms): PowerPoint takes the
/// fonts' Windows values, 0.8594 / 0.1406 (measured on its PDF: ＭＳ ゴシック
/// single-spaced at 8 to 54 pt puts the baseline 1.2 × 0.8594 × size below
/// the line's top, rounded; at 40 pt 41 pt, where 0.87 would give 42). Word
/// puts it at 0.87 of a 1 em line (the docx owner's measurement on Word's
/// PDF), so the difference is the app's: docx keeps its own value for
/// these typefaces.
pub(crate) fn office_metrics(name: &str) -> Option<(f32, f32)> {
    let n = name.trim();
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| n.starts_with(p));
    Some(match n {
        _ if starts(&["ＭＳ", "MS Gothic", "MS PGothic", "MS UI Gothic", "MS Mincho", "MS PMincho"]) => (0.8594, 0.1406), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ if starts(&["游ゴシック", "Yu Gothic UI"]) && n.contains("UI") => (1.0791, 0.251), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ if starts(&["游ゴシック", "Yu Gothic", "YuGothic"]) => (0.9849, 0.3022), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ if starts(&["游明朝", "Yu Mincho", "YuMincho"]) => (0.9951, 0.292), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        "メイリオ" | "Meiryo" => (1.0601, 0.4399), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ if starts(&["Meiryo UI"]) => (1.0601, 0.21),
        _ if starts(&["Calibri", "Carlito", "Candara", "Corbel", "Constantia"]) => (0.9521, 0.2686),
        "Cambria" => (0.9502, 0.2222),
        "Arial" => (0.9053, 0.2119),
        "Times New Roman" => (0.8911, 0.2163),
        "Century" | "Century Schoolbook" => (0.9858, 0.2158),
        "Century Gothic" => (0.9712, 0.2202),
        "Consolas" => (0.9199, 0.251),
        "Verdana" => (1.0054, 0.21),
        _ if starts(&["Aptos"]) => (0.939, 0.2817),
        _ if starts(&["Hiragino Sans", "Hiragino Kaku Gothic", "ヒラギノ角ゴ"]) => (1.0, 0.234), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ => return None,
    })
}

/// How far below its baseline PowerPoint ends a line `height` high whose
/// single spacing would be `single` high (1.2 × its largest size), for a
/// font whose ascent and descent divide single spacing so that `natural`
/// of it is below the baseline (`single × descent / (ascent + descent)`).
/// The baseline is this far above the line's bottom.
///
/// A single-spaced line is divided as the font's ascent and descent
/// divide its height. A line with other spacing (a multiple or an exact
/// height) has a quarter of its height below the baseline, and more by as
/// much as the font's own part below the baseline exceeds a quarter of
/// single spacing (メイリオ); a line smaller than single spacing keeps no more
/// below the baseline than single spacing does. (Measured on PowerPoint's
/// PDF: Calibri, 游ゴシック, メイリオ, ＭＳ ゴシック, Arial and Aptos at 60 to 250 %
/// and exactly 6 to 50 pt, 20 pt text.)
pub(crate) fn powerpoint_below_baseline(height: f32, single: f32, natural: f32) -> f32 {
    if (height - single).abs() < single * 1e-4 {
        return natural;
    }
    let quarter = 0.25 * height + (natural - 0.25 * single).max(0.0);
    if height < single { quarter.min(natural) } else { quarter }
}

/// Where PowerPoint puts the baseline in a line `height` high, from its
/// top, for a font with ascent `ascent` and descent `descent` (any unit)
/// whose single spacing is `single` high (see
/// [`powerpoint_below_baseline`]).
pub(crate) fn powerpoint_baseline(height: f32, single: f32, ascent: f32, descent: f32) -> f32 {
    height - powerpoint_below_baseline(height, single, natural_below(single, ascent, descent))
}

/// How much of single spacing `single` is below the baseline for a font
/// with ascent `ascent` and descent `descent`.
pub(crate) fn natural_below(single: f32, ascent: f32, descent: f32) -> f32 {
    if ascent + descent > 0.0 { single * descent / (ascent + descent) } else { single * 0.2 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 20 pt text: (line height, font's part below the baseline) → the
    /// baseline from the line's top PowerPoint drew (rounded to points).
    #[test]
    fn baselines_as_powerpoint_draws_them() {
        let single = 24.0;
        let natural = |a: f32, d: f32| single * d / (a + d);
        let calibri = natural(0.9521, 0.2686);
        let meiryo = natural(1.0601, 0.4399);
        let ms_gothic = natural(0.8594, 0.1406);
        let base = |h: f32, n: f32| (h - powerpoint_below_baseline(h, single, n)).round();
        // Single spacing: the font's division.
        assert_eq!(base(24.0, calibri), 19.0);
        assert_eq!(base(24.0, meiryo), 17.0);
        assert_eq!(base(24.0, ms_gothic), 21.0);
        // Multiples.
        assert_eq!(base(36.0, calibri), 27.0);
        assert_eq!(base(60.0, calibri), 45.0);
        assert_eq!(base(60.0, meiryo), 44.0);
        assert_eq!(base(25.2, ms_gothic), 19.0);
        assert_eq!(base(22.8, ms_gothic), 19.0);
        assert_eq!(base(16.8, calibri), 13.0);
        assert_eq!(base(22.8, meiryo), 16.0);
        // Exact heights.
        assert_eq!(base(10.0, calibri), 8.0);
        assert_eq!(base(40.0, calibri), 30.0);
        assert_eq!(base(20.0, meiryo), 14.0);
    }

    #[test]
    fn reads_windows_metrics_from_a_font_file() {
        let data = include_bytes!("../../fonts/carlito/Carlito-Regular.ttf");
        let (a, d) = font_file_metrics(data, 0).unwrap();
        assert!((a - 0.9521).abs() < 1e-3 && (d - 0.2686).abs() < 1e-3, "{a} {d}");
    }

    #[test]
    fn a_word_for_windows_line_is_gdis_or_1_3_times_an_east_asian_fonts() {
        let near = |(a, l): (f32, f32), (ea, el): (f32, f32)| assert!((a - ea).abs() < 2e-3 && (l - el).abs() < 2e-3, "{a} {l} against {ea} {el}");
        // Arial: its hhea line gap above the text (1.150 em in Word's PDF).
        near(word_windows_line_of(0.9053, 0.2119, 1.1499, false), (0.9380, 1.1499));
        // Calibri: its Windows height (1.221).
        near(word_windows_line_of(0.9521, 0.2686, 1.2207, false), (0.9521, 1.2207));
        // ＭＳ 明朝 1.3 (1.296), 游明朝 1.673 (1.672), the 0.3 split above and below.
        near(word_windows_line_of(0.8594, 0.1406, 1.0, true), (0.8594 + 0.15, 1.3));
        near(word_windows_line_of(0.9951, 0.2920, 1.6021, true), (0.9951 + 0.15 * 1.2871, 1.6732));
        // From a file: Carlito, a Latin font.
        let data = include_bytes!("../../fonts/carlito/Carlito-Regular.ttf");
        let (line, east_asian) = word_windows_font(data, 0).unwrap();
        near(line, (0.9521, 1.2207));
        assert!(!east_asian);
    }
}
