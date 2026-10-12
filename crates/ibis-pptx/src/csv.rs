// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! CSV / TSV: opened as a workbook of one sheet (the values read as Excel reads
//! them: numbers, dates, percentages and formulas as typed), supporting UTF-8
//! (with or without BOM), UTF-16, and Shift_JIS (CP932); with auto-detected
//! delimiters (comma, tab, semicolon), and writing sheets as CSV or TSV.

use ibis_calc::input::{Typed, parse_input};
use ibis_doc_model::{CellValue, Presentation};

use crate::Error;

pub use encoding_rs;

/// Character encodings supported for CSV / TSV export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CsvEncoding {
    /// UTF-8 with a BOM (\xEF\xBB\xBF) - Excel default 「CSV UTF-8」
    #[default]
    Utf8Bom,
    /// UTF-8 without a BOM
    Utf8,
    /// Shift_JIS (CP932 / Windows-31J) - Japanese Windows Excel 「CSV (コンマ区切り)」
    ShiftJis,
}

/// Encodes `text` as Shift_JIS (CP932 / Windows-31J) bytes.
pub fn encode_sjis(text: &str) -> Vec<u8> {
    let (cow, _, _) = encoding_rs::SHIFT_JIS.encode(text);
    cow.into_owned()
}


/// Options for exporting a sheet to delimited text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvOptions {
    pub encoding: CsvEncoding,
    pub delimiter: char,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            encoding: CsvEncoding::Utf8Bom,
            delimiter: ',',
        }
    }
}

/// Whether `data` is not a ZIP package (so taken for CSV/text data).
pub fn is_csv(data: &[u8]) -> bool {
    !data.starts_with(b"PK\x03\x04")
}

/// Decodes CSV bytes into a UTF-8 string.
/// Supports UTF-8 (with or without BOM), UTF-16LE/BE (with BOM),
/// and Shift_JIS (CP932 / Windows-31J).
pub fn decode(data: &[u8]) -> Result<String, Error> {
    if let Some(stripped) = data.strip_prefix(b"\xEF\xBB\xBF") {
        return String::from_utf8(stripped.to_vec())
            .map_err(|e| Error::new(format!("UTF-8 CSV の読み込みに失敗しました: {e}")));
    }
    if let Some(stripped) = data.strip_prefix(b"\xFF\xFE") {
        let (cow, _, had_errors) = encoding_rs::UTF_16LE.decode(stripped);
        if !had_errors {
            return Ok(cow.into_owned());
        }
    }
    if let Some(stripped) = data.strip_prefix(b"\xFE\xFF") {
        let (cow, _, had_errors) = encoding_rs::UTF_16BE.decode(stripped);
        if !had_errors {
            return Ok(cow.into_owned());
        }
    }
    // Try valid UTF-8
    if let Ok(s) = std::str::from_utf8(data) {
        return Ok(s.to_string());
    }
    // Try Shift_JIS (CP932)
    let (cow, _, had_errors) = encoding_rs::SHIFT_JIS.decode(data);
    if !had_errors {
        return Ok(cow.into_owned());
    }
    Err(Error::new("CSVの文字コードを判定できませんでした（UTF-8 または Shift_JIS 形式である必要があります）"))
}

/// The character encoding the text file is read in (テキスト/CSV から の
/// ファイル元): found from the bytes (自動), or the one chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextEncoding {
    #[default]
    Auto,
    Utf8,
    /// Shift_JIS (CP932 / Windows-31J).
    ShiftJis,
    /// UTF-16 (little endian unless the byte order mark says big endian).
    Utf16,
}

/// Decodes `data` in the chosen encoding; a byte order mark of UTF-8 or
/// UTF-16 is left out. A byte that is not valid in it is an error (the
/// text file would be shown garbled).
pub fn decode_as(data: &[u8], encoding: TextEncoding) -> Result<String, Error> {
    let fail = |name: &str| Error::new(format!("{name} として読み込めませんでした"));
    match encoding {
        TextEncoding::Auto => decode(data),
        TextEncoding::Utf8 => {
            let data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
            String::from_utf8(data.to_vec()).map_err(|_| fail("UTF-8"))
        }
        TextEncoding::ShiftJis => {
            let (cow, _, had_errors) = encoding_rs::SHIFT_JIS.decode(data);
            if had_errors { Err(fail("Shift_JIS")) } else { Ok(cow.into_owned()) }
        }
        TextEncoding::Utf16 => {
            let (enc, body) = match data.strip_prefix(b"\xFE\xFF") {
                Some(rest) => (encoding_rs::UTF_16BE, rest),
                None => (encoding_rs::UTF_16LE, data.strip_prefix(b"\xFF\xFE").unwrap_or(data)),
            };
            let (cow, _, had_errors) = enc.decode(body);
            if had_errors { Err(fail("UTF-16")) } else { Ok(cow.into_owned()) }
        }
    }
}

/// What a text file is read with (テキスト/CSV から): the encoding, the
/// delimiter (`None`: found from the text), and the row to start from
/// (1 for the first), the rows before it left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportOptions {
    pub encoding: TextEncoding,
    pub delimiter: Option<char>,
    pub start_row: usize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self { encoding: TextEncoding::Auto, delimiter: None, start_row: 1 }
    }
}

/// A text file read: its records (from the start row), and the delimiter used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub rows: Vec<Vec<String>>,
    pub delimiter: char,
}

/// The records of a text file as `options` say.
pub fn import(data: &[u8], options: &ImportOptions) -> Result<Imported, Error> {
    let text = decode_as(data, options.encoding)?;
    let delimiter = options.delimiter.unwrap_or_else(|| detect_delimiter(&text));
    let rows = parse_with_delimiter(&text, delimiter).into_iter().skip(options.start_row.max(1) - 1).collect();
    Ok(Imported { rows, delimiter })
}

/// Detects the field delimiter: comma (`,`), tab (`\t`), or semicolon (`;`).
pub fn detect_delimiter(text: &str) -> char {
    let mut comma_count = 0usize;
    let mut tab_count = 0usize;
    let mut semicolon_count = 0usize;
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut lines = 0;

    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                }
                '"' => quoted = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => comma_count += 1,
            '\t' => tab_count += 1,
            ';' => semicolon_count += 1,
            '\n' | '\r' => {
                lines += 1;
                if lines >= 50 {
                    break;
                }
            }
            _ => {}
        }
    }

    if tab_count > 0 && tab_count >= comma_count && tab_count >= semicolon_count {
        '\t'
    } else if semicolon_count > 0 && semicolon_count > comma_count {
        ';'
    } else {
        ','
    }
}

/// Records of fields separated by `delimiter`: quotes around a field, `""` for a
/// quote inside one, CRLF or LF between records.
pub fn parse_with_delimiter(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            c if c == delimiter => record.push(std::mem::take(&mut field)),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

/// Records of fields: automatically detects delimiter (comma, tab, semicolon).
pub fn parse(text: &str) -> Vec<Vec<String>> {
    let delimiter = detect_delimiter(text);
    parse_with_delimiter(text, delimiter)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub(crate) fn column_name(mut n: u32) -> String {
    let mut s = String::new();
    n += 1;
    while n > 0 {
        let r = (n - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        n = (n - 1) / 26;
    }
    s
}

/// A workbook (`.xlsx` bytes) of one sheet named `name` holding the CSV/TSV values.
pub fn workbook_from(text: &str, name: &str) -> Result<Vec<u8>, Error> {
    let delimiter = detect_delimiter(text);
    workbook_from_with_delimiter(text, name, delimiter)
}

/// A workbook (`.xlsx` bytes) of one sheet named `name` holding the delimited values.
pub fn workbook_from_with_delimiter(text: &str, name: &str, delimiter: char) -> Result<Vec<u8>, Error> {
    let records = parse_with_delimiter(text, delimiter);
    // Number formats the values take (as typing them gives), by code.
    let mut formats: Vec<String> = Vec::new();
    let mut rows = String::new();
    for (r, record) in records.iter().enumerate() {
        let mut cells = String::new();
        for (c, field) in record.iter().enumerate() {
            if field.is_empty() {
                continue;
            }
            let at = format!("{}{}", column_name(c as u32), r + 1);
            match parse_input(field, false) {
                Typed::Formula(f) => cells.push_str(&format!(r#"<c r="{at}"><f>{}</f></c>"#, escape(&ibis_calc::formula::to_file(&f)))),
                Typed::Value(v, format) => {
                    let style = match format {
                        Some(code) => {
                            let i = formats.iter().position(|f| f == code).unwrap_or_else(|| {
                                formats.push(code.to_string());
                                formats.len() - 1
                            });
                            format!(r#" s="{}""#, i + 1)
                        }
                        None => String::new(),
                    };
                    match v {
                        CellValue::Number(n) => cells.push_str(&format!(r#"<c r="{at}"{style}><v>{}</v></c>"#, ibis_calc::eval::number_to_text(n))),
                        CellValue::Bool(b) => cells.push_str(&format!(r#"<c r="{at}" t="b"><v>{}</v></c>"#, u8::from(b))),
                        CellValue::Text(t) => cells.push_str(&format!(r#"<c r="{at}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#, escape(&t))),
                        CellValue::Error(e) => cells.push_str(&format!(r#"<c r="{at}" t="e"><v>{}</v></c>"#, e.as_str())),
                        CellValue::Empty => {}
                    }
                }
            }
        }
        rows.push_str(&format!(r#"<row r="{}">{cells}</row>"#, r + 1));
    }
    let num_fmts: String = formats.iter().enumerate().map(|(i, f)| format!(r#"<numFmt numFmtId="{}" formatCode="{}"/>"#, 164 + i, escape(f))).collect();
    let xfs: String = (0..formats.len()).map(|i| format!(r#"<xf numFmtId="{}" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#, 164 + i)).collect();
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">{}<fonts count="1"><font><sz val="12"/><color theme="1"/><name val="游ゴシック"/><family val="2"/><charset val="128"/><scheme val="minor"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="{}"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>{xfs}</cellXfs><cellStyles count="1"><cellStyle name="標準" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#,
        if formats.is_empty() { String::new() } else { format!(r#"<numFmts count="{}">{num_fmts}</numFmts>"#, formats.len()) },
        formats.len() + 1
    );
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><dimension ref="A1"/><sheetViews><sheetView tabSelected="1" workbookViewId="0"/></sheetViews><sheetFormatPr baseColWidth="10" defaultRowHeight="20"/><sheetData>{rows}</sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/></worksheet>"#
    );
    let parts: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#.to_string()),
        ("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_string()),
        ("xl/workbook.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><bookViews><workbookView/></bookViews><sheets><sheet name="{}" sheetId="1" r:id="rId1"/></sheets><calcPr calcId="191029" fullCalcOnLoad="1"/></workbook>"#, escape(name))),
        ("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_string()),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/styles.xml", styles),
    ];
    let mut writer = ibis_ooxml::zip::Writer::new();
    for (name, xml) in parts {
        writer.add(name, ibis_ooxml::zip::EntryMeta::default(), xml.as_bytes()).map_err(|e| Error::new(format!("{e}")))?;
    }
    writer.finish().map_err(|e| Error::new(format!("{e}")))
}

/// The sheet at slide index `slide` of a workbook as CSV: UTF-8 with a
/// BOM, CRLF between records, each cell's text as it shows, quoted when it
/// holds a comma, a quote or a line break.
pub fn sheet_to_csv(presentation: &Presentation, slide: usize) -> Vec<u8> {
    sheet_to_delimited(presentation, slide, CsvOptions::default())
}

/// The sheet at slide index `slide` of a workbook as TSV (tab-separated values):
/// UTF-8 with a BOM, CRLF between records.
pub fn sheet_to_tsv(presentation: &Presentation, slide: usize) -> Vec<u8> {
    sheet_to_delimited(
        presentation,
        slide,
        CsvOptions {
            encoding: CsvEncoding::Utf8Bom,
            delimiter: '\t',
        },
    )
}

/// The sheet at slide index `slide` of a workbook as Shift_JIS CSV (standard Japanese
/// Excel CSV format): CRLF between records.
pub fn sheet_to_csv_sjis(presentation: &Presentation, slide: usize) -> Vec<u8> {
    sheet_to_delimited(
        presentation,
        slide,
        CsvOptions {
            encoding: CsvEncoding::ShiftJis,
            delimiter: ',',
        },
    )
}

/// The sheet at slide index `slide` formatted with custom encoding and delimiter options.
pub fn sheet_to_delimited(presentation: &Presentation, slide: usize, options: CsvOptions) -> Vec<u8> {
    let mut text = String::new();
    if let Some(sheet) = presentation.slides.get(slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide) {
        let (rows, columns) = sheet.cells.iter().filter(|c| !c.text.is_empty()).fold((0, 0), |(r, c), x| (r.max(x.row + 1), c.max(x.column + 1)));
        let delimiter_str = options.delimiter.to_string();
        for row in 0..rows {
            let fields: Vec<String> = (0..columns)
                .map(|column| {
                    let cell_text = sheet.cell(row, column).map_or(String::new(), |c| c.text.clone());
                    if cell_text.contains([options.delimiter, '"', '\n', '\r']) {
                        format!("\"{}\"", cell_text.replace('"', "\"\""))
                    } else {
                        cell_text
                    }
                })
                .collect();
            text.push_str(&fields.join(&delimiter_str));
            text.push_str("\r\n");
        }
    }
    match options.encoding {
        CsvEncoding::Utf8Bom => {
            let mut bytes = Vec::with_capacity(3 + text.len());
            bytes.extend_from_slice(b"\xEF\xBB\xBF");
            bytes.extend_from_slice(text.as_bytes());
            bytes
        }
        CsvEncoding::Utf8 => text.into_bytes(),
        CsvEncoding::ShiftJis => {
            let (cow, _, _) = encoding_rs::SHIFT_JIS.encode(&text);
            cow.into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_text_file_is_read_in_the_chosen_encoding_and_delimiter() {
        let sjis = encode_sjis("品名;数量\r\nりんご;3\r\n");
        assert!(decode_as(&sjis, TextEncoding::Utf8).is_err(), "Shift_JIS bytes are not UTF-8");
        let got = import(&sjis, &ImportOptions { encoding: TextEncoding::ShiftJis, ..Default::default() }).unwrap();
        assert_eq!(got.delimiter, ';');
        assert_eq!(got.rows, vec![vec!["品名", "数量"], vec!["りんご", "3"]]);
        // The delimiter chosen, and the first rows left out.
        let got = import(b"title\n\na b\nc d\n", &ImportOptions { delimiter: Some(' '), start_row: 3, ..Default::default() }).unwrap();
        assert_eq!(got.rows, vec![vec!["a", "b"], vec!["c", "d"]]);
        // UTF-16 with a byte order mark.
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend("x,y".encode_utf16().flat_map(|u| u.to_le_bytes()));
        assert_eq!(import(&utf16, &ImportOptions { encoding: TextEncoding::Utf16, ..Default::default() }).unwrap().rows, vec![vec!["x", "y"]]);
        assert_eq!(decode_as(b"\xEF\xBB\xBFabc", TextEncoding::Utf8).unwrap(), "abc");
    }

    use super::*;

    #[test]
    fn records() {
        assert_eq!(parse("a,b\r\n\"c,d\",\"e\"\"f\"\n"), vec![vec!["a", "b"], vec!["c,d", "e\"f"]]);
        assert_eq!(parse("1,,3"), vec![vec!["1", "", "3"]]);
    }

    #[test]
    fn round_trip() {
        let bytes = workbook_from("品名,数量,日付\nりんご,3,2026/9/30\n\"み,かん\",1.5,=B2*2\n", "test").unwrap();
        let opened = crate::save::open(bytes).unwrap();
        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&opened.presentation.slides[0]).unwrap().clone();
        assert_eq!(sheet.cell(1, 1).unwrap().value, CellValue::Number(3.0));
        assert_eq!(sheet.cell(1, 2).unwrap().text, "2026/9/30");
        assert_eq!(sheet.cell(2, 0).unwrap().value, CellValue::Text("み,かん".into()));
        assert_eq!(sheet.cell(2, 2).unwrap().formula.as_deref(), Some("B2*2"));
        let csv = String::from_utf8(sheet_to_csv(&opened.presentation, 0)).unwrap();
        assert!(csv.starts_with("\u{feff}品名,数量,日付\r\n"));
        assert!(csv.contains("\"み,かん\""));
    }

    #[test]
    fn shift_jis_decoding() {
        // Shift_JIS bytes for "品名,数量\r\nりんご,3\r\n①株式会社,100"
        let (sjis_bytes, _, _) = encoding_rs::SHIFT_JIS.encode("品名,数量\r\nりんご,3\r\n①株式会社,100");
        let decoded = decode(&sjis_bytes).expect("Shift_JIS decode succeeds");
        assert_eq!(decoded, "品名,数量\r\nりんご,3\r\n①株式会社,100");

        // Parse Shift_JIS CSV via open
        let opened = crate::save::open(sjis_bytes.to_vec()).unwrap();
        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&opened.presentation.slides[0]).unwrap().clone();
        assert_eq!(sheet.cell(0, 0).unwrap().text, "品名");
        assert_eq!(sheet.cell(1, 0).unwrap().text, "りんご");
        assert_eq!(sheet.cell(2, 0).unwrap().text, "①株式会社");
    }

    #[test]
    fn tsv_delimiter_detection() {
        let tsv = "ID\t名前\t部署\r\n101\t田中\t開発部\r\n102\t佐藤\t営業部\r\n";
        assert_eq!(detect_delimiter(tsv), '\t');
        let records = parse(tsv);
        assert_eq!(records, vec![
            vec!["ID", "名前", "部署"],
            vec!["101", "田中", "開発部"],
            vec!["102", "佐藤", "営業部"],
        ]);

        let opened = crate::save::open(tsv.as_bytes().to_vec()).unwrap();
        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&opened.presentation.slides[0]).unwrap().clone();
        assert_eq!(sheet.cell(0, 0).unwrap().text, "ID");
        assert_eq!(sheet.cell(0, 1).unwrap().text, "名前");
        assert_eq!(sheet.cell(1, 1).unwrap().text, "田中");
    }

    #[test]
    fn semicolon_delimiter_detection() {
        let semi = "product;price;tax\r\napple;100;10\r\norange;150;15\r\n";
        assert_eq!(detect_delimiter(semi), ';');
        let records = parse(semi);
        assert_eq!(records[0], vec!["product", "price", "tax"]);
        assert_eq!(records[1], vec!["apple", "100", "10"]);
    }

    #[test]
    fn delimited_export_formats() {
        let bytes = workbook_from("品名,単価\nバナナ,120\n", "test").unwrap();
        let opened = crate::save::open(bytes).unwrap();

        // 1. TSV export
        let tsv_bytes = sheet_to_tsv(&opened.presentation, 0);
        let tsv_str = decode(&tsv_bytes).unwrap();
        assert!(tsv_str.contains("品名\t単価\r\n"));
        assert!(tsv_str.contains("バナナ\t120\r\n"));

        // 2. Shift_JIS CSV export
        let sjis_bytes = sheet_to_csv_sjis(&opened.presentation, 0);
        assert!(!sjis_bytes.starts_with(b"\xEF\xBB\xBF"));
        let decoded_sjis = decode(&sjis_bytes).unwrap();
        assert!(decoded_sjis.contains("品名,単価\r\n"));
        assert!(decoded_sjis.contains("バナナ,120\r\n"));
    }
}
