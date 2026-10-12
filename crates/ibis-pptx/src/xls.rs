// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An old workbook (`.xls`, Excel 97–2003, BIFF8 in a compound file),
//! read from the public specifications (MS-XLS, MS-CFB) into the `.xlsx`
//! the rest of the program reads: values, strings, formulas (as text,
//! with the cached value), number formats, fonts, fills, borders,
//! alignment, column widths, row heights, merged cells, frozen panes,
//! defined names, margins and page setup. Anything else in the file
//! (charts, pictures, macros, comments) is left out; the file is only
//! read, and saved as `.xlsx`.

use std::collections::{BTreeMap, HashMap};

use crate::xls_features::{CfGroup, Dv, Link, Note, NoteReader};
use crate::xls_formula::{Context, column_letters, decompile};
use crate::{Error, cfb};

/// Whether `data` is a compound file (an old Office file).
pub fn is_xls(data: &[u8]) -> bool {
    // (A password-protected package is a compound file too: not an old workbook.)
    cfb::is_cfb(data) && !ibis_ooxml::encryption::is_encrypted(data)
}

fn bad(what: &str) -> Error {
    Error::new(format!("旧形式（.xls）のブックを読めませんでした（{what}）"))
}

pub(crate) fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

pub(crate) fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn f64_at(d: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

struct Record<'a> {
    id: u16,
    data: &'a [u8],
}

fn records(stream: &[u8]) -> Vec<Record<'_>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= stream.len() {
        let id = u16::from_le_bytes([stream[i], stream[i + 1]]);
        let len = u16::from_le_bytes([stream[i + 2], stream[i + 3]]) as usize;
        let end = (i + 4 + len).min(stream.len());
        out.push(Record { id, data: &stream[i + 4..end] });
        i += 4 + len;
    }
    out
}

/// A string of the file (`XLUnicodeString`, with a 1- or 2-byte length): its text and the bytes it took.
fn unicode_string(d: &[u8], at: usize, long: bool) -> Option<(String, usize)> {
    let (n, mut p) = if long { (u16_at(d, at)? as usize, at + 2) } else { (*d.get(at)? as usize, at + 1) };
    let flags = *d.get(p)?;
    p += 1;
    let rich_runs = if flags & 0x08 != 0 { let r = u16_at(d, p)? as usize; p += 2; r } else { 0 };
    let ext = if flags & 0x04 != 0 { let e = u32_at(d, p)? as usize; p += 4; e } else { 0 };
    let text = if flags & 1 != 0 {
        let units: Vec<u16> = (0..n).map(|k| u16_at(d, p + k * 2)).collect::<Option<_>>()?;
        p += n * 2;
        String::from_utf16_lossy(&units)
    } else {
        let s: String = d.get(p..p + n)?.iter().map(|&b| b as char).collect();
        p += n;
        s
    };
    p += rich_runs * 4 + ext;
    Some((text, p - at))
}

/// The shared strings (`SST`), the record and its `CONTINUE`s: a string may run over the
/// end of a record, where the next one begins with the flag byte of the rest.
fn shared_strings(parts: &[&[u8]]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(first) = parts.first() else { return out };
    let count = u32_at(first, 4).unwrap_or(0) as usize;
    let (mut part, mut at) = (0usize, 8usize);
    let next = |part: &mut usize, at: &mut usize| -> bool {
        *part += 1;
        *at = 0;
        *part < parts.len()
    };
    'strings: for _ in 0..count {
        if at >= parts[part].len() && !next(&mut part, &mut at) {
            break;
        }
        let d = parts[part];
        let Some(n) = u16_at(d, at) else { break };
        let Some(&flags) = d.get(at + 2) else { break };
        let mut p = at + 3;
        let runs = if flags & 0x08 != 0 { let r = u16_at(d, p).unwrap_or(0) as usize; p += 2; r } else { 0 };
        let ext = if flags & 0x04 != 0 { let e = u32_at(d, p).unwrap_or(0) as usize; p += 4; e } else { 0 };
        let mut wide = flags & 1 != 0;
        let mut text = String::new();
        let mut left = n as usize;
        let mut cur = p;
        while left > 0 {
            let d = parts[part];
            let unit = if wide { 2 } else { 1 };
            let available = d.len().saturating_sub(cur) / unit;
            let take = available.min(left);
            for k in 0..take {
                if wide {
                    let u = u16_at(d, cur + k * 2).unwrap_or(0);
                    text.push(char::from_u32(u as u32).unwrap_or('\u{FFFD}'));
                } else {
                    text.push(d[cur + k] as char);
                }
            }
            cur += take * unit;
            left -= take;
            if left > 0 {
                // The rest is in the next record, behind its own flag byte.
                if !next(&mut part, &mut at) {
                    out.push(text);
                    break 'strings;
                }
                let d = parts[part];
                wide = d.first().is_some_and(|f| f & 1 != 0);
                cur = 1;
            }
        }
        // The runs and the extension: skipped, over a record boundary too.
        let mut skip = runs * 4 + ext;
        loop {
            let d = parts[part];
            let have = d.len().saturating_sub(cur);
            if skip <= have {
                cur += skip;
                break;
            }
            skip -= have;
            if !next(&mut part, &mut at) {
                break;
            }
            cur = 0;
        }
        at = cur;
        out.push(text);
    }
    out
}

#[derive(Clone, Debug, Default)]
struct Font {
    height: u16,
    italic: bool,
    strike: bool,
    color: u16,
    bold: bool,
    script: u16,
    underline: u8,
    name: String,
}

#[derive(Clone, Debug, Default)]
struct Xf {
    font: u16,
    format: u16,
    horizontal: u8,
    wrap: bool,
    vertical: u8,
    rotation: u8,
    indent: u8,
    shrink: bool,
    /// left, right, top, bottom: (style, colour index)
    borders: [(u8, u16); 4],
    pattern: u8,
    fore: u16,
    back: u16,
    /// Full colours from the XF's extension (`XFEXT`), as the attributes of a `color`
    /// element (`rgb="FF336699"`, `theme="4" tint="0.4"`), over the palette's: the fill's
    /// foreground and background, the text, and the borders' (left, right, top, bottom).
    ext_fore: Option<String>,
    ext_back: Option<String>,
    ext_text: Option<String>,
    ext_borders: [Option<String>; 4],
}

#[derive(Clone, Debug)]
enum Value {
    Number(f64),
    Text(String),
    Bool(bool),
    Error(u8),
}

#[derive(Clone, Debug)]
struct Cell {
    xf: u16,
    value: Option<Value>,
    /// A formula's tokens, and the cell it is counted from.
    formula: Option<Vec<u8>>,
}

#[derive(Default)]
struct Sheet {
    name: String,
    state: u8,
    cells: BTreeMap<(u32, u32), Cell>,
    rows: BTreeMap<u32, (u16, u16)>,
    cols: Vec<(u16, u16, u16, u16, u16)>,
    merges: Vec<[u16; 4]>,
    default_row_twips: Option<u16>,
    default_col_chars: Option<u16>,
    frozen: Option<(u16, u16)>,
    show_grid: bool,
    show_headings: bool,
    zoom: Option<(u16, u16)>,
    margins: [Option<f64>; 4],
    page: Option<[u16; 6]>,
    header: Option<String>,
    footer: Option<String>,
    shared: HashMap<(u32, u32), ([u16; 4], Vec<u8>)>,
    arrays: HashMap<(u32, u32), ([u16; 4], Vec<u8>)>,
    cf: Vec<CfGroup>,
    dv: Vec<Dv>,
    links: Vec<Link>,
    notes: Vec<Note>,
    note_texts: std::collections::HashMap<u16, String>,
}

const DEFAULT_PALETTE: [u32; 56] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x800000, 0x008000, 0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080,
    0x9999FF, 0x993366, 0xFFFFCC, 0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, 0x000080, 0xFF00FF, 0xFFFF00, 0x00FFFF, 0x800080, 0x800000, 0x008080, 0x0000FF,
    0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99, 0x99CCFF, 0xFF99CC, 0xCC99FF, 0xFFCC99, 0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900, 0xFF6600, 0x666699, 0x969696,
    0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366, 0x333399, 0x333333,
];

struct Book {
    date1904: bool,
    fonts: Vec<Font>,
    formats: BTreeMap<u16, String>,
    xfs: Vec<Xf>,
    palette: Vec<u32>,
    strings: Vec<String>,
    sheets: Vec<Sheet>,
    sheet_offsets: Vec<usize>,
    cx: Context,
    /// Defined names: (name, built-in code, sheet index (None: the workbook's), rgce).
    names: Vec<(String, Option<u8>, Option<u16>, Vec<u8>)>,
    /// The theme (`theme1.xml`) the file carries, for the colours that name one.
    theme: Option<Vec<u8>>,
}

impl Book {
    /// A colour index as `AARRGGBB`; `None` for the automatic ones.
    fn color(&self, icv: u16) -> Option<String> {
        let rgb = match icv {
            0..=7 => DEFAULT_PALETTE[icv as usize],
            8..=63 => *self.palette.get(icv as usize - 8).unwrap_or(&DEFAULT_PALETTE[icv as usize - 8]),
            _ => return None,
        };
        Some(format!("FF{rgb:06X}"))
    }
}

/// An old workbook as the `.xlsx` bytes of one the rest of the program reads.
pub fn to_xlsx(data: &[u8]) -> Result<Vec<u8>, Error> {
    let stream = cfb::stream(data, "Workbook").or_else(|_| cfb::stream(data, "Book"))?;
    let recs = records(&stream);
    let mut book = Book { date1904: false, fonts: Vec::new(), formats: BTreeMap::new(), xfs: Vec::new(), palette: DEFAULT_PALETTE.to_vec(), strings: Vec::new(), sheets: Vec::new(), sheet_offsets: Vec::new(), cx: Context::default(), names: Vec::new(), theme: None };
    // Offsets of each record, for finding the sheets' streams.
    let mut offsets = Vec::with_capacity(recs.len());
    let mut at = 0usize;
    for r in &recs {
        offsets.push(at);
        at += 4 + r.data.len();
    }
    let mut i = 0;
    // The workbook globals, up to the first sheet's BOF's EOF.
    let mut depth = 0;
    let mut sst: Vec<&[u8]> = Vec::new();
    let mut last_was_sst = false;
    let mut theme_parts: Vec<&[u8]> = Vec::new();
    let mut last_was_theme = false;
    let mut pending_ext: Vec<(usize, [Option<String>; 7])> = Vec::new();
    while i < recs.len() {
        let r = &recs[i];
        match r.id {
            0x0809 => {
                depth += 1;
                // Only BIFF8 (Excel 97 and later) is read.
                if depth == 1 && u16_at(r.data, 0) != Some(0x0600) {
                    return Err(bad("Excel 95 以前の形式です"));
                }
            }
            0x002F => return Err(bad("パスワードで保護されています")),
            0x000A => {
                depth -= 1;
                if depth <= 0 {
                    i += 1;
                    break;
                }
            }
            0x0022 => book.date1904 = r.data.first() == Some(&1),
            0x0031 => {
                let d = r.data;
                let name = d.get(14..).and_then(|n| unicode_string(n, 0, false)).map(|(s, _)| s).unwrap_or_default();
                book.fonts.push(Font {
                    height: u16_at(d, 0).unwrap_or(200),
                    italic: u16_at(d, 2).unwrap_or(0) & 2 != 0,
                    strike: u16_at(d, 2).unwrap_or(0) & 8 != 0,
                    color: u16_at(d, 4).unwrap_or(0x7FFF),
                    bold: u16_at(d, 6).unwrap_or(400) >= 700,
                    script: u16_at(d, 8).unwrap_or(0),
                    underline: d.get(10).copied().unwrap_or(0),
                    name,
                });
            }
            0x041E => {
                if let (Some(id), Some((code, _))) = (u16_at(r.data, 0), unicode_string(r.data, 2, true)) {
                    book.formats.insert(id, code);
                }
            }
            0x00E0 => {
                let d = r.data;
                let flags = u16_at(d, 4).unwrap_or(0);
                let _ = flags;
                let borders1 = u16_at(d, 10).unwrap_or(0);
                let colors1 = u16_at(d, 12).unwrap_or(0);
                let top = u32_at(d, 14).unwrap_or(0);
                let fill = u16_at(d, 18).unwrap_or(0);
                book.xfs.push(Xf {
                    font: u16_at(d, 0).unwrap_or(0),
                    format: u16_at(d, 2).unwrap_or(0),
                    horizontal: d.get(6).map_or(0, |b| b & 7),
                    wrap: d.get(6).is_some_and(|b| b & 8 != 0),
                    vertical: d.get(6).map_or(2, |b| (b >> 4) & 7),
                    rotation: d.get(7).copied().unwrap_or(0),
                    indent: d.get(8).map_or(0, |b| b & 15),
                    shrink: d.get(8).is_some_and(|b| b & 16 != 0),
                    borders: [
                        ((borders1 & 15) as u8, colors1 & 0x7F),
                        (((borders1 >> 4) & 15) as u8, (colors1 >> 7) & 0x7F),
                        (((borders1 >> 8) & 15) as u8, (top & 0x7F) as u16),
                        (((borders1 >> 12) & 15) as u8, ((top >> 7) & 0x7F) as u16),
                    ],
                    pattern: ((top >> 26) & 0x3F) as u8,
                    fore: fill & 0x7F,
                    back: (fill >> 7) & 0x7F,
                    ..Default::default()
                });
            }
            0x087D => {
                // XFEXT: the full colours of an XF (ECMA-376's theme and RGB ones the palette cannot hold).
                let d = r.data;
                if let (Some(ixfe), Some(count)) = (u16_at(d, 14), u16_at(d, 18)) {
                    let mut p = 20;
                    let mut ext: [Option<String>; 7] = Default::default();
                    for _ in 0..count {
                        let (Some(kind), Some(size)) = (u16_at(d, p), u16_at(d, p + 2)) else { break };
                        let (size, kind) = (size as usize, kind as usize);
                        if size >= 12
                            && let (Some(color_kind), Some(tint), Some(value)) = (u16_at(d, p + 4), u16_at(d, p + 6), u32_at(d, p + 8))
                        {
                            let tint_attr = if tint != 0 { format!(" tint=\"{}\"", ibis_calc::number_to_xml(((tint as i16) as f64 / 32767.0 * 1e6).round() / 1e6)) } else { String::new() };
                            let attr = match color_kind {
                                0 => Some("auto=\"1\"".to_string()),
                                1 => Some(format!("indexed=\"{value}\"{tint_attr}")),
                                2 => Some(format!("rgb=\"{:02X}{:02X}{:02X}{:02X}\"", d.get(p + 11).copied().unwrap_or(0xFF), d.get(p + 8).copied().unwrap_or(0), d.get(p + 9).copied().unwrap_or(0), d.get(p + 10).copied().unwrap_or(0))),
                                3 => Some(format!("theme=\"{value}\"{tint_attr}")),
                                _ => None,
                            };
                            // 4 fore, 5 back, 7–10 borders (top, bottom, left, right), 13 text.
                            let slot = match kind {
                                4 => Some(0),
                                5 => Some(1),
                                7 => Some(2),
                                8 => Some(3),
                                9 => Some(4),
                                10 => Some(5),
                                13 => Some(6),
                                _ => None,
                            };
                            if let (Some(slot), Some(attr)) = (slot, attr) {
                                ext[slot] = Some(attr);
                            }
                        }
                        p += size.max(4);
                    }
                    pending_ext.push((ixfe as usize, ext));
                }
            }
            0x0896 => {
                // THEME: the file's theme part, zipped, after a 4-byte version.
                theme_parts = vec![r.data];
                last_was_theme = true;
                i += 1;
                continue;
            }
            0x003C if last_was_theme => theme_parts.push(r.data),
            0x0092 => {
                let n = u16_at(r.data, 0).unwrap_or(0) as usize;
                book.palette = (0..n).filter_map(|k| u32_at(r.data, 2 + k * 4).map(|v| ((v & 0xFF) << 16) | (v & 0xFF00) | ((v >> 16) & 0xFF))).collect();
                while book.palette.len() < 56 {
                    book.palette.push(DEFAULT_PALETTE[book.palette.len()]);
                }
            }
            0x0085 => {
                let d = r.data;
                let offset = u32_at(d, 0).unwrap_or(0) as usize;
                let state = d.get(4).copied().unwrap_or(0) & 3;
                let kind = d.get(5).copied().unwrap_or(0);
                let name = unicode_string(d, 6, false).map(|(s, _)| s).unwrap_or_default();
                book.sheets.push(Sheet { name, state, show_grid: true, show_headings: true, ..Default::default() });
                book.sheet_offsets.push(if kind == 0 { offset } else { usize::MAX });
            }
            0x00FC => {
                sst = vec![r.data];
                last_was_sst = true;
                i += 1;
                continue;
            }
            0x003C if last_was_sst => sst.push(r.data),
            0x01AE => {
                // SUPBOOK: a workbook of itself has 0x0401 where the name's length is.
                book.cx.internal_book.push(u16_at(r.data, 2) == Some(0x0401));
            }
            0x0017 => {
                let n = u16_at(r.data, 0).unwrap_or(0) as usize;
                book.cx.xti = (0..n).filter_map(|k| Some((u16_at(r.data, 2 + k * 6)?, u16_at(r.data, 4 + k * 6)?, u16_at(r.data, 6 + k * 6)?))).collect();
            }
            0x0018 => {
                let d = r.data;
                if let (Some(flags), Some(&cch), Some(cce), Some(itab)) = (u16_at(d, 0), d.get(3), u16_at(d, 4), u16_at(d, 8)) {
                    let (cch_menu, cch_desc, cch_help, cch_status) = (d.get(10).copied().unwrap_or(0) as usize, d.get(11).copied().unwrap_or(0) as usize, d.get(12).copied().unwrap_or(0) as usize, d.get(13).copied().unwrap_or(0) as usize);
                    let wide = d.get(14).is_some_and(|f| f & 1 != 0);
                    let name_len = if wide { cch as usize * 2 } else { cch as usize };
                    let name_bytes = d.get(15..15 + name_len).unwrap_or(&[]);
                    let name: String = if wide { String::from_utf16_lossy(&(0..cch as usize).filter_map(|k| u16_at(name_bytes, k * 2)).collect::<Vec<_>>()) } else { name_bytes.iter().map(|&b| b as char).collect() };
                    let built_in = (flags & 0x20 != 0).then(|| name_bytes.first().copied().unwrap_or(0));
                    let rgce_at = 15 + name_len + cch_menu + cch_desc + cch_help + cch_status;
                    let rgce = d.get(rgce_at..rgce_at + cce as usize).unwrap_or(&[]).to_vec();
                    book.names.push((name, built_in, (itab != 0).then(|| itab - 1), rgce));
                }
            }
            _ => {}
        }
        if r.id != 0x003C {
            last_was_sst = false;
            last_was_theme = false;
        }
        i += 1;
    }
    book.strings = shared_strings(&sst);
    for (ixfe, ext) in pending_ext {
        if let Some(xf) = book.xfs.get_mut(ixfe) {
            let [fore, back, top, bottom, left, right, text] = ext;
            xf.ext_fore = fore;
            xf.ext_back = back;
            xf.ext_text = text;
            xf.ext_borders = [left, right, top, bottom];
        }
    }
    // The theme: the zip after the record's 16 bytes (header, reserved, version).
    let blob: Vec<u8> = theme_parts.iter().enumerate().flat_map(|(n, p)| if n == 0 { p.get(16..).unwrap_or(&[]).to_vec() } else { p.to_vec() }).collect();
    if blob.starts_with(b"PK")
        && let Ok(archive) = ibis_ooxml::zip::Archive::new(std::sync::Arc::<[u8]>::from(blob))
        && let Some(entry) = archive.entries().iter().find(|e| e.name.ends_with("theme1.xml"))
    {
        book.theme = archive.read(entry).ok();
    }
    // The names' texts for the formulas (a built-in name has none to refer to by).
    book.cx.sheets = book.sheets.iter().map(|s| s.name.clone()).collect();
    book.cx.names = book.names.iter().map(|(n, b, _, _)| if b.is_some() { String::new() } else { n.clone() }).collect();
    // Each worksheet's stream.
    for k in 0..book.sheets.len() {
        let offset = book.sheet_offsets[k];
        if offset == usize::MAX {
            continue;
        }
        let start = offsets.iter().position(|&o| o == offset).ok_or_else(|| bad("シートの位置"))?;
        let mut sheet = std::mem::take(&mut book.sheets[k]);
        read_sheet(&recs[start..], &mut sheet);
        book.sheets[k] = sheet;
    }
    emit(&book)
}

fn rk_value(rk: u32) -> f64 {
    let v = if rk & 2 != 0 { ((rk as i32) >> 2) as f64 } else { f64::from_bits(((rk & 0xFFFF_FFFC) as u64) << 32) };
    if rk & 1 != 0 { v / 100.0 } else { v }
}

fn read_sheet(recs: &[Record<'_>], sheet: &mut Sheet) {
    let mut depth = 0;
    let mut pending_string: Option<(u32, u32)> = None;
    let mut frozen_flag = false;
    let mut pane: Option<(u16, u16)> = None;
    let mut note_reader = NoteReader::default();
    for r in recs {
        let d = r.data;
        match r.id {
            0x0809 => depth += 1,
            0x000A => {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
            }
            0x0207 => {
                if let (Some((row, col)), Some((s, _))) = (pending_string.take(), unicode_string(d, 0, true))
                    && let Some(cell) = sheet.cells.get_mut(&(row, col))
                {
                    cell.value = Some(Value::Text(s));
                }
                continue;
            }
            0x0203 => {
                if let (Some(row), Some(col), Some(xf), Some(v)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), f64_at(d, 6)) {
                    sheet.cells.insert((row as u32, col as u32), Cell { xf, value: Some(Value::Number(v)), formula: None });
                }
            }
            0x027E => {
                if let (Some(row), Some(col), Some(xf), Some(rk)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), u32_at(d, 6)) {
                    sheet.cells.insert((row as u32, col as u32), Cell { xf, value: Some(Value::Number(rk_value(rk))), formula: None });
                }
            }
            0x00BD => {
                if let (Some(row), Some(first)) = (u16_at(d, 0), u16_at(d, 2)) {
                    let n = d.len().saturating_sub(6) / 6;
                    for k in 0..n {
                        if let (Some(xf), Some(rk)) = (u16_at(d, 4 + k * 6), u32_at(d, 6 + k * 6)) {
                            sheet.cells.insert((row as u32, first as u32 + k as u32), Cell { xf, value: Some(Value::Number(rk_value(rk))), formula: None });
                        }
                    }
                }
            }
            0x00FD => {
                if let (Some(row), Some(col), Some(xf), Some(_idx)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), u32_at(d, 6)) {
                    // The text is filled in from the shared strings by the caller's emit (kept as an index here).
                    sheet.cells.insert((row as u32, col as u32), Cell { xf, value: Some(Value::Text(format!("\u{0}SST{}", u32_at(d, 6).unwrap_or(0)))), formula: None });
                }
            }
            0x0204 => {
                if let (Some(row), Some(col), Some(xf), Some((s, _))) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), unicode_string(d, 6, true)) {
                    sheet.cells.insert((row as u32, col as u32), Cell { xf, value: Some(Value::Text(s)), formula: None });
                }
            }
            0x0201 => {
                if let (Some(row), Some(col), Some(xf)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4)) {
                    sheet.cells.insert((row as u32, col as u32), Cell { xf, value: None, formula: None });
                }
            }
            0x00BE => {
                if let (Some(row), Some(first)) = (u16_at(d, 0), u16_at(d, 2)) {
                    let n = d.len().saturating_sub(6) / 2;
                    for k in 0..n {
                        if let Some(xf) = u16_at(d, 4 + k * 2) {
                            sheet.cells.insert((row as u32, first as u32 + k as u32), Cell { xf, value: None, formula: None });
                        }
                    }
                }
            }
            0x0205 => {
                if let (Some(row), Some(col), Some(xf), Some(&v), Some(&is_error)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), d.get(6), d.get(7)) {
                    let value = if is_error != 0 { Value::Error(v) } else { Value::Bool(v != 0) };
                    sheet.cells.insert((row as u32, col as u32), Cell { xf, value: Some(value), formula: None });
                }
            }
            0x0006 => {
                if let (Some(row), Some(col), Some(xf), Some(cce)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), u16_at(d, 20)) {
                    let rgce = d.get(22..22 + cce as usize).unwrap_or(&[]).to_vec();
                    let value = if u16_at(d, 12) == Some(0xFFFF) {
                        match d.get(6) {
                            Some(0) => {
                                pending_string = Some((row as u32, col as u32));
                                None
                            }
                            Some(1) => Some(Value::Bool(d.get(8).is_some_and(|&b| b != 0))),
                            Some(2) => Some(Value::Error(d.get(8).copied().unwrap_or(0))),
                            Some(3) => Some(Value::Text(String::new())),
                            _ => None,
                        }
                    } else {
                        f64_at(d, 6).map(Value::Number)
                    };
                    sheet.cells.insert((row as u32, col as u32), Cell { xf, value, formula: Some(rgce) });
                }
            }
            0x04BC => {
                if let (Some(r1), Some(r2), Some(&c1), Some(&c2), Some(cce)) = (u16_at(d, 0), u16_at(d, 2), d.get(4), d.get(5), u16_at(d, 8)) {
                    let rgce = d.get(10..10 + cce as usize).unwrap_or(&[]).to_vec();
                    sheet.shared.insert((r1 as u32, c1 as u32), ([r1, r2, c1 as u16, c2 as u16], rgce));
                }
            }
            0x0221 => {
                if let (Some(r1), Some(r2), Some(&c1), Some(&c2), Some(cce)) = (u16_at(d, 0), u16_at(d, 2), d.get(4), d.get(5), u16_at(d, 12)) {
                    let rgce = d.get(14..14 + cce as usize).unwrap_or(&[]).to_vec();
                    sheet.arrays.insert((r1 as u32, c1 as u32), ([r1, r2, c1 as u16, c2 as u16], rgce));
                }
            }
            0x0208 => {
                if let (Some(row), Some(height), Some(flags)) = (u16_at(d, 0), u16_at(d, 6), u16_at(d, 12)) {
                    sheet.rows.insert(row as u32, (height, flags));
                }
            }
            0x007D => {
                if let (Some(first), Some(last), Some(width), Some(xf), Some(flags)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), u16_at(d, 6), u16_at(d, 8)) {
                    sheet.cols.push((first, last, width, xf, flags));
                }
            }
            0x0055 => sheet.default_col_chars = u16_at(d, 0),
            0x0225 => sheet.default_row_twips = u16_at(d, 2),
            0x00E5 => {
                let n = u16_at(d, 0).unwrap_or(0) as usize;
                for k in 0..n {
                    if let (Some(r1), Some(r2), Some(c1), Some(c2)) = (u16_at(d, 2 + k * 8), u16_at(d, 4 + k * 8), u16_at(d, 6 + k * 8), u16_at(d, 8 + k * 8)) {
                        sheet.merges.push([r1, r2, c1, c2]);
                    }
                }
            }
            0x0041 => {
                if let (Some(x), Some(y)) = (u16_at(d, 0), u16_at(d, 2)) {
                    pane = Some((x, y));
                }
            }
            0x023E => {
                let g = u16_at(d, 0).unwrap_or(0);
                sheet.show_grid = g & 2 != 0;
                sheet.show_headings = g & 4 != 0;
                frozen_flag = g & 8 != 0;
            }
            0x00A0 => {
                if let (Some(n), Some(m)) = (u16_at(d, 0), u16_at(d, 2)) {
                    sheet.zoom = Some((n, m));
                }
            }
            0x0026 => sheet.margins[0] = f64_at(d, 0),
            0x0027 => sheet.margins[1] = f64_at(d, 0),
            0x0028 => sheet.margins[2] = f64_at(d, 0),
            0x0029 => sheet.margins[3] = f64_at(d, 0),
            0x00A1 => {
                // With fNoPls the printer's settings (paper, scale, orientation) are not valid.
                if let (Some(paper), Some(scale), Some(w), Some(h), Some(g)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 6), u16_at(d, 8), u16_at(d, 10))
                    && g & 4 == 0
                {
                    sheet.page = Some([paper, scale, w, h, g, 0]);
                }
            }
            0x005D => note_reader.object(d),
            0x01B6 => note_reader.text_box(d),
            0x003C => note_reader.continued(d),
            0x001C => sheet.notes.extend(crate::xls_features::read_note(d)),
            0x01B0 => {
                if let Some(g) = crate::xls_features::read_cf_header(d) {
                    sheet.cf.push(g);
                }
            }
            0x01B1 => {
                if let (Some(rule), Some(group)) = (crate::xls_features::read_cf(d), sheet.cf.last_mut()) {
                    group.rules.push(rule);
                }
            }
            0x0879 => {
                if let Some(g) = d.get(12..).and_then(crate::xls_features::read_cf_header) {
                    sheet.cf.push(g);
                }
            }
            0x087A => {
                if let (Some(rule), Some(group)) = (crate::xls_features::read_cf12(d), sheet.cf.last_mut()) {
                    group.rules.push(rule);
                }
            }
            0x087B => crate::xls_features::read_cf_ex(d, &mut sheet.cf),
            0x01BE => sheet.dv.extend(crate::xls_features::read_dv(d)),
            0x01B8 => sheet.links.extend(crate::xls_features::read_hlink(d)),
            0x0014 => sheet.header = unicode_string(d, 0, true).map(|(s, _)| s),
            0x0015 => sheet.footer = unicode_string(d, 0, true).map(|(s, _)| s),
            _ => {}
        }
        // The text of a formula's result comes in the record after it (past a shared or array formula's).
        if !matches!(r.id, 0x0006 | 0x04BC | 0x0221) {
            pending_string = None;
        }
    }
    sheet.frozen = pane.filter(|_| frozen_flag);
    sheet.note_texts = note_reader.texts;
}

fn esc(s: &str) -> String {
    s.chars()
        .filter(|&c| c == '\t' || c == '\n' || c == '\r' || c >= ' ')
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

fn error_text(code: u8) -> &'static str {
    match code {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0F => "#VALUE!",
        0x17 => "#REF!",
        0x1D => "#NAME?",
        0x24 => "#NUM!",
        0x2A => "#N/A",
        _ => "#VALUE!",
    }
}

fn border_style(code: u8) -> Option<&'static str> {
    Some(match code {
        1 => "thin",
        2 => "medium",
        3 => "dashed",
        4 => "dotted",
        5 => "thick",
        6 => "double",
        7 => "hair",
        8 => "mediumDashed",
        9 => "dashDot",
        10 => "mediumDashDot",
        11 => "dashDotDot",
        12 => "mediumDashDotDot",
        13 => "slantDashDot",
        _ => return None,
    })
}

fn pattern_name(code: u8) -> &'static str {
    const NAMES: [&str; 19] = ["none", "solid", "mediumGray", "darkGray", "lightGray", "darkHorizontal", "darkVertical", "darkDown", "darkUp", "darkGrid", "darkTrellis", "lightHorizontal", "lightVertical", "lightDown", "lightUp", "lightGrid", "lightTrellis", "gray125", "gray0625"];
    NAMES.get(code as usize).copied().unwrap_or("none")
}

/// The sheet's formula for a cell: the text, with the cell's own tokens or a shared / array formula's.
fn formula_text(sheet: &Sheet, row: u32, col: u32, rgce: &[u8], cx: &Context) -> Option<(String, Option<String>)> {
    if rgce.first() == Some(&0x01) {
        let (r, c) = (u16_at(rgce, 1)? as u32, u16_at(rgce, 3)? as u32);
        if let Some((range, tokens)) = sheet.arrays.get(&(r, c)) {
            // An array formula: written once, at its first cell, with its range.
            if (r, c) == (row, col) {
                let text = decompile(tokens, (row, col), cx)?;
                let area = format!("{}{}:{}{}", column_letters(range[2] as u32), range[0] as u32 + 1, column_letters(range[3] as u32), range[1] as u32 + 1);
                return Some((text, Some(area)));
            }
            return None;
        }
        let (range, tokens) = sheet.shared.get(&(r, c))?;
        let _ = range;
        return decompile(tokens, (row, col), cx).map(|t| (t, None));
    }
    decompile(rgce, (row, col), cx).map(|t| (t, None))
}

fn emit(book: &Book) -> Result<Vec<u8>, Error> {
    // ---- styles
    // A font as XML, with its colour (`color` is the attributes of a `color` element) if it has one.
    let font_xml = |f: &Font, color: Option<String>| -> String {
        let mut x = String::from("<font>");
        if f.bold {
            x.push_str("<b/>");
        }
        if f.italic {
            x.push_str("<i/>");
        }
        if f.strike {
            x.push_str("<strike/>");
        }
        match f.underline {
            1 => x.push_str("<u/>"),
            2 => x.push_str("<u val=\"double\"/>"),
            0x21 => x.push_str("<u val=\"singleAccounting\"/>"),
            0x22 => x.push_str("<u val=\"doubleAccounting\"/>"),
            _ => {}
        }
        match f.script {
            1 => x.push_str("<vertAlign val=\"superscript\"/>"),
            2 => x.push_str("<vertAlign val=\"subscript\"/>"),
            _ => {}
        }
        x.push_str(&format!("<sz val=\"{}\"/>", ibis_calc::number_to_xml(f.height as f64 / 20.0)));
        if let Some(c) = color {
            x.push_str(&format!("<color {c}/>"));
        }
        x.push_str(&format!("<name val=\"{}\"/><family val=\"2\"/></font>", esc(&f.name)));
        x
    };
    let mut font_list: Vec<String> = book.fonts.iter().map(|f| font_xml(f, book.color(f.color).map(|c| format!("rgb=\"{c}\"")))).collect();
    if font_list.is_empty() {
        font_list.push("<font><sz val=\"11\"/><name val=\"Arial\"/></font>".into());
    }
    let base_fonts = font_list.len();
    let mut fills: Vec<String> = vec!["<fill><patternFill patternType=\"none\"/></fill>".into(), "<fill><patternFill patternType=\"gray125\"/></fill>".into()];
    let mut borders: Vec<String> = vec!["<border><left/><right/><top/><bottom/><diagonal/></border>".into()];
    let mut num_fmts: Vec<(u16, String)> = Vec::new();
    let mut fmt_ids: HashMap<u16, u16> = HashMap::new();
    let mut next_fmt = 164u16.max(book.formats.keys().copied().max().map_or(0, |m| m + 1));
    let mut xfs_xml = String::new();
    for xf in &book.xfs {
        let font_at = if xf.font >= 5 { xf.font as usize - 1 } else { xf.font as usize };
        let mut font_id = if font_at < base_fonts { font_at } else { 0 };
        // The extension's text colour: a copy of the font with it.
        if let (Some(color), Some(f)) = (&xf.ext_text, book.fonts.get(font_id)) {
            let xml = font_xml(f, Some(color.clone()));
            font_id = font_list.iter().position(|x| *x == xml).unwrap_or_else(|| {
                font_list.push(xml);
                font_list.len() - 1
            });
        }
        // The fill: the pattern's foreground is the colour of a solid fill.
        let fill_id = if xf.pattern == 0 {
            0
        } else {
            let fg = xf.ext_fore.clone().or_else(|| book.color(xf.fore).map(|c| format!("rgb=\"{c}\""))).map(|c| format!("<fgColor {c}/>")).unwrap_or_default();
            let bg = xf.ext_back.clone().or_else(|| book.color(xf.back).map(|c| format!("rgb=\"{c}\""))).map(|c| format!("<bgColor {c}/>")).unwrap_or_default();
            let xml = format!("<fill><patternFill patternType=\"{}\">{fg}{bg}</patternFill></fill>", pattern_name(xf.pattern));
            fills.iter().position(|f| *f == xml).unwrap_or_else(|| {
                fills.push(xml);
                fills.len() - 1
            })
        };
        let side = |name: &str, (style, color): (u8, u16), ext: &Option<String>| -> String {
            match border_style(style) {
                Some(s) => {
                    let c = ext.clone().or_else(|| book.color(color).map(|c| format!("rgb=\"{c}\""))).unwrap_or_else(|| "auto=\"1\"".into());
                    let c = format!("<color {c}/>");
                    format!("<{name} style=\"{s}\">{c}</{name}>")
                }
                None => format!("<{name}/>"),
            }
        };
        let border_xml = format!("<border>{}{}{}{}<diagonal/></border>", side("left", xf.borders[0], &xf.ext_borders[0]), side("right", xf.borders[1], &xf.ext_borders[1]), side("top", xf.borders[2], &xf.ext_borders[2]), side("bottom", xf.borders[3], &xf.ext_borders[3]));
        let border_id = borders.iter().position(|b| *b == border_xml).unwrap_or_else(|| {
            borders.push(border_xml);
            borders.len() - 1
        });
        // A format with its own code (a custom one, or a built-in one the file spells out).
        let num_fmt = match book.formats.get(&xf.format) {
            Some(code) => *fmt_ids.entry(xf.format).or_insert_with(|| {
                let id = if xf.format >= 164 { xf.format } else { let n = next_fmt; next_fmt += 1; n };
                num_fmts.push((id, code.clone()));
                id
            }),
            None => xf.format,
        };
        let horizontal = match xf.horizontal {
            1 => " horizontal=\"left\"",
            2 => " horizontal=\"center\"",
            3 => " horizontal=\"right\"",
            4 => " horizontal=\"fill\"",
            5 => " horizontal=\"justify\"",
            6 => " horizontal=\"centerContinuous\"",
            7 => " horizontal=\"distributed\"",
            _ => "",
        };
        let vertical = match xf.vertical {
            0 => " vertical=\"top\"",
            1 => " vertical=\"center\"",
            3 => " vertical=\"justify\"",
            4 => " vertical=\"distributed\"",
            _ => "",
        };
        let rotation = if xf.rotation != 0 { format!(" textRotation=\"{}\"", xf.rotation) } else { String::new() };
        let indent = if xf.indent != 0 { format!(" indent=\"{}\"", xf.indent) } else { String::new() };
        xfs_xml.push_str(&format!(
            "<xf numFmtId=\"{num_fmt}\" fontId=\"{font_id}\" fillId=\"{fill_id}\" borderId=\"{border_id}\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\" applyAlignment=\"1\"><alignment{horizontal}{vertical}{}{rotation}{indent}{}/></xf>",
            if xf.wrap { " wrapText=\"1\"" } else { "" },
            if xf.shrink { " shrinkToFit=\"1\"" } else { "" },
        ));
    }
    if book.xfs.is_empty() {
        xfs_xml.push_str("<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>");
    }
    // The conditional formats' formats, the same one once.
    let mut dxfs: Vec<String> = Vec::new();
    let color = |icv: u16| book.color(icv);
    let dxf_ids: Vec<Vec<Vec<Option<usize>>>> = book
        .sheets
        .iter()
        .map(|sheet| {
            sheet
                .cf
                .iter()
                .map(|g| {
                    g.rules
                        .iter()
                        .map(|r| {
                            let xml = crate::xls_features::dxf_xml(r.dxf.as_ref()?, &color);
                            Some(dxfs.iter().position(|d| *d == xml).unwrap_or_else(|| {
                                dxfs.push(xml);
                                dxfs.len() - 1
                            }))
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    let fmts_xml: String = num_fmts.iter().map(|(id, code)| format!("<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>", esc(code))).collect();
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">{}<fonts count=\"{}\">{}</fonts><fills count=\"{}\">{}</fills><borders count=\"{}\">{}</borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"{}\">{xfs_xml}</cellXfs><cellStyles count=\"1\"><cellStyle name=\"標準\" xfId=\"0\" builtinId=\"0\"/></cellStyles>{}</styleSheet>",
        if num_fmts.is_empty() { String::new() } else { format!("<numFmts count=\"{}\">{fmts_xml}</numFmts>", num_fmts.len()) },
        font_list.len(),
        font_list.concat(),
        fills.len(),
        fills.concat(),
        borders.len(),
        borders.concat(),
        book.xfs.len().max(1),
        if dxfs.is_empty() { String::new() } else { format!("<dxfs count=\"{}\">{}</dxfs>", dxfs.len(), dxfs.concat()) },
    );
    // ---- sheets
    let mut sheet_files: Vec<(String, String)> = Vec::new();
    let mut workbook_sheets = String::new();
    let mut sheet_number = 0;
    let mut sheet_numbers: Vec<Option<usize>> = Vec::new();
    for (k, sheet) in book.sheets.iter().enumerate() {
        if book.sheet_offsets[k] == usize::MAX {
            sheet_numbers.push(None);
            continue;
        }
        sheet_number += 1;
        sheet_numbers.push(Some(sheet_number - 1));
        let mut rows_xml = String::new();
        let mut row_set: BTreeMap<u32, String> = BTreeMap::new();
        let (mut max_row, mut max_col) = (0u32, 0u32);
        for (&(row, col), cell) in &sheet.cells {
            max_row = max_row.max(row);
            max_col = max_col.max(col);
            let at = format!("{}{}", column_letters(col), row + 1);
            let style = if cell.xf != 0 { format!(" s=\"{}\"", cell.xf) } else { String::new() };
            let value = cell.value.as_ref().map(|v| match v {
                Value::Text(t) if t.starts_with("\u{0}SST") => Value::Text(t["\u{0}SST".len()..].parse::<usize>().ok().and_then(|i| book.strings.get(i).cloned()).unwrap_or_default()),
                other => other.clone(),
            });
            let formula = cell.formula.as_deref().and_then(|f| formula_text(sheet, row, col, f, &book.cx));
            let (t, v) = match &value {
                Some(Value::Number(n)) => (String::new(), format!("<v>{}</v>", ibis_calc::number_to_xml(*n))),
                Some(Value::Text(s)) if formula.is_some() => (" t=\"str\"".into(), format!("<v>{}</v>", esc(s))),
                Some(Value::Text(s)) => (" t=\"inlineStr\"".into(), format!("<is><t xml:space=\"preserve\">{}</t></is>", esc(s))),
                Some(Value::Bool(b)) => (" t=\"b\"".into(), format!("<v>{}</v>", u8::from(*b))),
                Some(Value::Error(e)) => (" t=\"e\"".into(), format!("<v>{}</v>", error_text(*e))),
                None => (String::new(), String::new()),
            };
            let f = match &formula {
                Some((text, Some(area))) => format!("<f t=\"array\" ref=\"{area}\">{}</f>", esc(&ibis_calc::formula::to_file(text))),
                Some((text, None)) => format!("<f>{}</f>", esc(&ibis_calc::formula::to_file(text))),
                None => String::new(),
            };
            // A formula with no readable text keeps its value as a constant.
            let (t, v) = if cell.formula.is_some() && formula.is_none() && matches!(value, Some(Value::Text(_))) { (" t=\"inlineStr\"".to_string(), v.replace("<v>", "<is><t xml:space=\"preserve\">").replace("</v>", "</t></is>")) } else { (t, v) };
            if cell.value.is_none() && cell.formula.is_none() && cell.xf == 0 {
                continue;
            }
            row_set.entry(row).or_default().push_str(&format!("<c r=\"{at}\"{style}{t}>{f}{v}</c>"));
        }
        let all_rows: std::collections::BTreeSet<u32> = row_set.keys().copied().chain(sheet.rows.keys().copied()).collect();
        for row in all_rows {
            let (height, flags) = sheet.rows.get(&row).copied().unwrap_or((0, 0));
            let mut attrs = String::new();
            // The file's own height of a row (what Excel showed, of a row that grew with its text too).
            if sheet.rows.contains_key(&row) && height & 0x8000 == 0 && height & 0x7FFF != 0 {
                attrs.push_str(&format!(" ht=\"{}\" customHeight=\"1\"", ibis_calc::number_to_xml((height & 0x7FFF) as f64 / 20.0)));
            }
            if flags & 0x20 != 0 {
                attrs.push_str(" hidden=\"1\"");
            }
            rows_xml.push_str(&format!("<row r=\"{}\"{attrs}>{}</row>", row + 1, row_set.get(&row).cloned().unwrap_or_default()));
        }
        let cols_xml: String = sheet
            .cols
            .iter()
            .map(|&(first, last, width, xf, flags)| {
                let w = width as f64 / 256.0;
                format!("<col min=\"{}\" max=\"{}\" width=\"{}\" customWidth=\"1\"{}{}/>", first as u32 + 1, (last.min(255)) as u32 + 1, ibis_calc::number_to_xml(w), if flags & 1 != 0 { " hidden=\"1\"" } else { "" }, if xf != 0 && xf < 0xF000 { format!(" style=\"{xf}\"") } else { String::new() })
            })
            .collect();
        let merges_xml = if sheet.merges.is_empty() { String::new() } else { format!("<mergeCells count=\"{}\">{}</mergeCells>", sheet.merges.len(), sheet.merges.iter().map(|m| format!("<mergeCell ref=\"{}{}:{}{}\"/>", column_letters(m[2] as u32), m[0] as u32 + 1, column_letters(m[3] as u32), m[1] as u32 + 1)).collect::<String>()) };
        let pane = sheet.frozen.map(|(x, y)| {
            let state = match (x > 0, y > 0) {
                (true, true) => "bottomRight",
                (true, false) => "topRight",
                _ => "bottomLeft",
            };
            format!("<pane{}{} topLeftCell=\"{}{}\" activePane=\"{state}\" state=\"frozen\"/>", if x > 0 { format!(" xSplit=\"{x}\"") } else { String::new() }, if y > 0 { format!(" ySplit=\"{y}\"") } else { String::new() }, column_letters(x as u32), y as u32 + 1)
        });
        let zoom = sheet.zoom.filter(|&(n, m)| m != 0 && n != m).map(|(n, m)| format!(" zoomScale=\"{}\"", (n as u32 * 100 / m as u32))).unwrap_or_default();
        let m = |i: usize, d: f64| ibis_calc::number_to_xml(sheet.margins[i].unwrap_or(d));
        let setup = sheet.page.map(|[paper, scale, w, h, g, _]| {
            let landscape = g & 2 == 0 && g & 0x40 == 0;
            format!("<pageSetup paperSize=\"{paper}\"{}{}{}{}/>", if scale != 0 && scale != 100 { format!(" scale=\"{scale}\"") } else { String::new() }, if landscape { " orientation=\"landscape\"" } else { " orientation=\"portrait\"" }, if w != 1 { format!(" fitToWidth=\"{w}\"") } else { String::new() }, if h != 1 { format!(" fitToHeight=\"{h}\"") } else { String::new() })
        });
        let header_footer = match (&sheet.header, &sheet.footer) {
            (None, None) => String::new(),
            (h, f) => format!("<headerFooter>{}{}</headerFooter>", h.as_ref().filter(|s| !s.is_empty()).map(|s| format!("<oddHeader>{}</oddHeader>", esc(s))).unwrap_or_default(), f.as_ref().filter(|s| !s.is_empty()).map(|s| format!("<oddFooter>{}</oddFooter>", esc(s))).unwrap_or_default()),
        };
        let header_footer = if header_footer == "<headerFooter></headerFooter>" { String::new() } else { header_footer };
        let default_row = sheet.default_row_twips.map(|t| format!(" defaultRowHeight=\"{}\" customHeight=\"1\"", ibis_calc::number_to_xml((t & 0x7FFF) as f64 / 20.0))).unwrap_or_else(|| " defaultRowHeight=\"15\"".into());
        // Conditional formats, data validations and hyperlinks (their addresses as the sheet's relationships).
        let formula_of = |tokens: &[u8], base: (u32, u32)| decompile(tokens, base, &book.cx);
        let mut has_notes = false;
        let features = {
            let mut x = crate::xls_features::cf_xml(&sheet.cf, &dxf_ids[k], &formula_of);
            x += &crate::xls_features::dv_xml(&sheet.dv, &formula_of);
            let mut targets: Vec<String> = Vec::new();
            x += &crate::xls_features::hyperlinks_xml(&sheet.links, &mut |t: &str| {
                let at = targets.iter().position(|e| e == t).unwrap_or_else(|| {
                    targets.push(t.to_string());
                    targets.len() - 1
                });
                format!("rIdL{}", at + 1)
            });
            let comments = crate::xls_features::comments_xml(&sheet.notes, &sheet.note_texts);
            if !targets.is_empty() || !comments.is_empty() {
                let mut rels: String = targets.iter().enumerate().map(|(i, t)| format!("<Relationship Id=\"rIdL{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>", i + 1, esc(t))).collect();
                if !comments.is_empty() {
                    rels += &format!("<Relationship Id=\"rIdC\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments\" Target=\"../comments{sheet_number}.xml\"/>");
                    sheet_files.push((format!("xl/comments{sheet_number}.xml"), comments));
                    rels += &format!("<Relationship Id=\"rIdV\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing\" Target=\"../drawings/vmlDrawing{sheet_number}.vml\"/>");
                    sheet_files.push((format!("xl/drawings/vmlDrawing{sheet_number}.vml"), crate::xls_features::notes_vml(&sheet.notes, &sheet.note_texts, sheet_number)));
                    has_notes = true;
                }
                sheet_files.push((format!("xl/worksheets/_rels/sheet{sheet_number}.xml.rels"), format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>")));
            }
            x
        };
        let legacy = if has_notes { "<legacyDrawing r:id=\"rIdV\"/>" } else { "" };
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><dimension ref=\"A1:{}{}\"/><sheetViews><sheetView{}{}{zoom} workbookViewId=\"0\"{}>{}</sheetView></sheetViews><sheetFormatPr{} baseColWidth=\"{}\"/>{}<sheetData>{rows_xml}</sheetData>{merges_xml}{features}<pageMargins left=\"{}\" right=\"{}\" top=\"{}\" bottom=\"{}\" header=\"0.3\" footer=\"0.3\"/>{}{header_footer}{legacy}</worksheet>",
            column_letters(max_col),
            max_row + 1,
            if sheet.show_grid { "" } else { " showGridLines=\"0\"" },
            if sheet.show_headings { "" } else { " showRowColHeaders=\"0\"" },
            if k == 0 { " tabSelected=\"1\"" } else { "" },
            pane.unwrap_or_default(),
            default_row,
            sheet.default_col_chars.unwrap_or(8),
            if cols_xml.is_empty() { String::new() } else { format!("<cols>{cols_xml}</cols>") },
            m(0, 0.7),
            m(1, 0.7),
            m(2, 0.75),
            m(3, 0.75),
            setup.unwrap_or_default(),
        )
;
        let state = match sheet.state {
            1 => " state=\"hidden\"",
            2 => " state=\"veryHidden\"",
            _ => "",
        };
        workbook_sheets.push_str(&format!("<sheet name=\"{}\" sheetId=\"{}\"{state} r:id=\"rId{}\"/>", esc(&sheet.name), sheet_number, sheet_number));
        sheet_files.push((format!("xl/worksheets/sheet{sheet_number}.xml"), xml));
    }
    // ---- names
    let mut names_xml = String::new();
    for (name, built_in, local, rgce) in &book.names {
        let Some(text) = decompile(rgce, (0, 0), &book.cx) else { continue };
        let local_id = local.and_then(|s| sheet_numbers.get(s as usize).copied().flatten());
        let name = match built_in {
            Some(6) => "_xlnm.Print_Area".to_string(),
            Some(7) => "_xlnm.Print_Titles".to_string(),
            Some(0x0D) => "_xlnm._FilterDatabase".to_string(),
            Some(_) => continue,
            None => name.clone(),
        };
        let local_attr = local_id.map(|s| format!(" localSheetId=\"{s}\"")).unwrap_or_default();
        names_xml.push_str(&format!("<definedName name=\"{}\"{local_attr}>{}</definedName>", esc(&name), esc(&text)));
    }
    if sheet_files.is_empty() {
        return Err(bad("ワークシートがありません"));
    }
    let count = sheet_number;
    let content_types = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"vml\" ContentType=\"application/vnd.openxmlformats-officedocument.vmlDrawing\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>{}{}</Types>",
        if book.theme.is_some() { "<Override PartName=\"/xl/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>" } else { "" },
        (1..=count).map(|n| format!("<Override PartName=\"/xl/worksheets/sheet{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>")).chain(sheet_files.iter().filter(|(name, _)| name.starts_with("xl/comments")).map(|(name, _)| format!("<Override PartName=\"/{name}\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml\"/>"))).collect::<String>()
    );
    let workbook_rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{}<Relationship Id=\"rIdS\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>{}</Relationships>",
        (1..=count).map(|n| format!("<Relationship Id=\"rId{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{n}.xml\"/>")).collect::<String>(),
        if book.theme.is_some() { "<Relationship Id=\"rIdT\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"theme/theme1.xml\"/>" } else { "" }
    );
    let workbook = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><workbookPr{}/><bookViews><workbookView/></bookViews><sheets>{workbook_sheets}</sheets>{}<calcPr calcId=\"191029\" fullCalcOnLoad=\"1\"/></workbook>",
        if book.date1904 { " date1904=\"1\"" } else { "" },
        if names_xml.is_empty() { String::new() } else { format!("<definedNames>{names_xml}</definedNames>") }
    );
    let mut parts: Vec<(String, String)> = vec![
        ("[Content_Types].xml".into(), content_types),
        ("_rels/.rels".into(), "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>".into()),
        ("xl/workbook.xml".into(), workbook),
        ("xl/_rels/workbook.xml.rels".into(), workbook_rels),
        ("xl/styles.xml".into(), styles),
    ];
    parts.extend(sheet_files);
    let mut writer = ibis_ooxml::zip::Writer::new();
    for (name, xml) in parts {
        writer.add(&name, ibis_ooxml::zip::EntryMeta::default(), xml.as_bytes()).map_err(|e| Error::new(format!("{e}")))?;
    }
    if let Some(theme) = &book.theme {
        writer.add("xl/theme/theme1.xml", ibis_ooxml::zip::EntryMeta::default(), theme).map_err(|e| Error::new(format!("{e}")))?;
    }
    writer.finish().map_err(|e| Error::new(format!("{e}")))
}
