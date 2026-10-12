// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Open and Repair subsystem for damaged or corrupted Excel (.xlsx) workbooks.
//!
//! Provides structural salvaging (local header scan, XML sanitization, unclosed
//! tag balancing, missing part synthesis) and data extraction modes (Recover
//! Formulas, Convert to Values) complying with Excel's "開いて修復" workflow.

use std::collections::HashMap;
use std::sync::Arc;

use ibis_doc_model::Presentation;
use ibis_ooxml::zip::{EntryMeta, Writer};

use crate::{Error, Source};

/// Repair execution mode (Excel's 開いて修復 options).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RepairMode {
    /// できる限りのデータ・書式・数式を修復して開く（推奨）
    #[default]
    Repair,
    /// データの抽出: 数式を回復（数式と値を抽出）
    ExtractFormulas,
    /// データの抽出: 値に変換（数式を計算済みの値に変換・除去して抽出）
    ExtractValues,
}

impl RepairMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Repair => "repair",
            Self::ExtractFormulas => "extract_formulas",
            Self::ExtractValues => "extract_values",
        }
    }
}

/// Severity / classification of a log entry during repair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairLogLevel {
    Info,
    Warning,
    Repaired,
    Removed,
}

impl RepairLogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARN",
            Self::Repaired => "REPAIRED",
            Self::Removed => "REMOVED",
        }
    }
}

/// A single logged event during workbook repair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepairLogEntry {
    pub level: RepairLogLevel,
    pub message: String,
}

/// Detailed report generated after repair.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepairReport {
    pub mode: RepairMode,
    pub success: bool,
    pub logs: Vec<RepairLogEntry>,
    pub sheets_recovered: usize,
    pub cells_recovered: usize,
    pub formulas_recovered: usize,
    pub formulas_converted_to_values: usize,
    pub parts_recovered: usize,
    pub parts_reconstructed: usize,
}

impl RepairReport {
    pub fn summary_text(&self) -> String {
        format!(
            "修復完了: シート {} 枚、セル {} 個、数式 {} 個復旧（再構築パーツ {} 件）",
            self.sheets_recovered, self.cells_recovered, self.formulas_recovered, self.parts_reconstructed
        )
    }
}

/// Result of a successful repair operation.
pub struct RepairedWorkbook {
    pub presentation: Presentation,
    pub source: Source,
    pub repaired_bytes: Vec<u8>,
    pub report: RepairReport,
}

/// Repair or extract data from corrupted Excel (.xlsx) file bytes.
pub fn repair_workbook(data: &[u8], mode: RepairMode) -> Result<RepairedWorkbook, Error> {
    let mut logs = Vec::new();
    let mut parts_reconstructed = 0;
    let mut formulas_converted = 0;

    logs.push(RepairLogEntry {
        level: RepairLogLevel::Info,
        message: format!("破損ブックの修復処理を開始しました（モード: {:?}）", mode),
    });

    // 1. Raw extraction: try standard archive, fallback to local headers scan.
    let mut parts = extract_parts(data, &mut logs);
    if parts.is_empty() {
        return Err(Error::new("修復可能なデータ（ワークシートまたはパッケージ）が見つかりませんでした"));
    }

    let parts_recovered = parts.len();

    // 2. XML sanitization & tag balancing on all XML parts.
    let part_names: Vec<String> = parts.keys().cloned().collect();
    for name in part_names {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            if let Some(raw_bytes) = parts.remove(&name) {
                let sanitized = sanitize_xml_bytes(&raw_bytes, &name, &mut logs);
                let text = match String::from_utf8(sanitized) {
                    Ok(t) => t,
                    Err(e) => {
                        logs.push(RepairLogEntry {
                            level: RepairLogLevel::Warning,
                            message: format!("パーツ '{}' は非 UTF-8 バイトを含んでいたため損失許容デコードを行いました: {}", name, e),
                        });
                        String::from_utf8_lossy(&e.into_bytes()).into_owned()
                    }
                };
                let repaired_text = repair_xml_syntax(&text, &name, &mut logs);
                parts.insert(name, repaired_text.into_bytes());
            }
        }
    }

    // 3. Find and inventory worksheets.
    let mut sheet_parts: Vec<String> = parts
        .keys()
        .filter(|k| k.starts_with("xl/worksheets/sheet") && k.ends_with(".xml"))
        .cloned()
        .collect();
    sheet_parts.sort();

    // If no standard sheet parts found, check if any XML part looks like a sheet.
    if sheet_parts.is_empty() {
        for (name, content) in &parts {
            if let Ok(text) = std::str::from_utf8(content) {
                if text.contains("<sheetData") || text.contains("<worksheet") {
                    let fallback_name = "xl/worksheets/sheet1.xml".to_string();
                    logs.push(RepairLogEntry {
                        level: RepairLogLevel::Repaired,
                        message: format!("シート様パーツ '{}' を標準パス '{}' に再配置しました", name, fallback_name),
                    });
                    sheet_parts.push(fallback_name);
                    break;
                }
            }
        }
        if !sheet_parts.is_empty() {
            let src = parts.keys().find(|k| {
                if let Ok(text) = std::str::from_utf8(&parts[*k]) {
                    text.contains("<sheetData") || text.contains("<worksheet")
                } else {
                    false
                }
            }).cloned().unwrap();
            let bytes = parts.remove(&src).unwrap();
            parts.insert("xl/worksheets/sheet1.xml".to_string(), bytes);
        }
    }

    // If still no sheets, synthesize an empty sheet.
    if sheet_parts.is_empty() {
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Warning,
            message: "ワークシートが存在しないため、空のワークシートを作成しました".to_string(),
        });
        parts.insert(
            "xl/worksheets/sheet1.xml".to_string(),
            b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData/></worksheet>".to_vec(),
        );
        sheet_parts.push("xl/worksheets/sheet1.xml".to_string());
        parts_reconstructed += 1;
    }

    // 4. Mode-specific adjustments on sheets.
    if mode == RepairMode::ExtractValues {
        for sheet_name in &sheet_parts {
            if let Some(bytes) = parts.remove(sheet_name) {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    let stripped = strip_formulas_from_sheet(text, &mut formulas_converted);
                    parts.insert(sheet_name.clone(), stripped.into_bytes());
                } else {
                    parts.insert(sheet_name.clone(), bytes);
                }
            }
        }
        if formulas_converted > 0 {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Repaired,
                message: format!("モード【値に変換】により {} 個の数式を静的な値に変換しました", formulas_converted),
            });
        }
    }

    // 5. Ensure xl/workbook.xml exists and references our sheets.
    let mut need_workbook = true;
    if let Some(wb_bytes) = parts.get("xl/workbook.xml") {
        if let Ok(text) = std::str::from_utf8(wb_bytes) {
            if roxmltree::Document::parse(text).is_ok() && text.contains("<sheet") {
                need_workbook = false;
            }
        }
    }
    if need_workbook {
        let mut wb_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\n  <sheets>\n");
        for (idx, _) in sheet_parts.iter().enumerate() {
            let sheet_name = format!("Sheet{}", idx + 1);
            let r_id = format!("rId{}", idx + 1);
            wb_xml.push_str(&format!("    <sheet name=\"{}\" sheetId=\"{}\" r:id=\"{}\"/>\n", sheet_name, idx + 1, r_id));
        }
        wb_xml.push_str("  </sheets>\n</workbook>\n");
        parts.insert("xl/workbook.xml".to_string(), wb_xml.into_bytes());
        parts_reconstructed += 1;
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: format!("欠落または破損していた xl/workbook.xml を {} 枚のシートで再構築しました", sheet_parts.len()),
        });
    }

    // 6. Ensure xl/_rels/workbook.xml.rels exists.
    let mut need_wb_rels = true;
    if let Some(wb_rels_bytes) = parts.get("xl/_rels/workbook.xml.rels") {
        if let Ok(text) = std::str::from_utf8(wb_rels_bytes) {
            if roxmltree::Document::parse(text).is_ok() {
                need_wb_rels = false;
            }
        }
    }
    if need_wb_rels {
        let mut rels_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\n");
        for (idx, sheet_name) in sheet_parts.iter().enumerate() {
            let r_id = format!("rId{}", idx + 1);
            let target = sheet_name.strip_prefix("xl/").unwrap_or(sheet_name);
            rels_xml.push_str(&format!("  <Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"{}\"/>\n", r_id, target));
        }
        let mut next_id = sheet_parts.len() + 1;
        if parts.contains_key("xl/styles.xml") {
            rels_xml.push_str(&format!("  <Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\n", next_id));
            next_id += 1;
        }
        if parts.contains_key("xl/sharedStrings.xml") {
            rels_xml.push_str(&format!("  <Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/>\n", next_id));
        }
        rels_xml.push_str("</Relationships>\n");
        parts.insert("xl/_rels/workbook.xml.rels".to_string(), rels_xml.into_bytes());
        parts_reconstructed += 1;
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: "欠落していた xl/_rels/workbook.xml.rels のリレーションシップを再構築しました".to_string(),
        });
    }

    // 7. Ensure _rels/.rels exists.
    let mut need_root_rels = true;
    if let Some(rels_bytes) = parts.get("_rels/.rels") {
        if let Ok(text) = std::str::from_utf8(rels_bytes) {
            if roxmltree::Document::parse(text).is_ok() {
                need_root_rels = false;
            }
        }
    }
    if need_root_rels {
        let rels_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\n  <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\n</Relationships>\n";
        parts.insert("_rels/.rels".to_string(), rels_xml.as_bytes().to_vec());
        parts_reconstructed += 1;
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: "欠落していた _rels/.rels を再構築しました".to_string(),
        });
    }

    // 8. Ensure [Content_Types].xml exists.
    let mut need_ct = true;
    if let Some(ct_bytes) = parts.get("[Content_Types].xml") {
        if let Ok(text) = std::str::from_utf8(ct_bytes) {
            if roxmltree::Document::parse(text).is_ok() && text.contains("spreadsheetml") {
                need_ct = false;
            }
        }
    }
    if need_ct {
        let mut ct_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\n  <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\n  <Default Extension=\"xml\" ContentType=\"application/xml\"/>\n  <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\n");
        for sheet_name in &sheet_parts {
            let part_name = if sheet_name.starts_with('/') { sheet_name.clone() } else { format!("/{}", sheet_name) };
            ct_xml.push_str(&format!("  <Override PartName=\"{}\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\n", part_name));
        }
        if parts.contains_key("xl/styles.xml") {
            ct_xml.push_str("  <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>\n");
        }
        if parts.contains_key("xl/sharedStrings.xml") {
            ct_xml.push_str("  <Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>\n");
        }
        ct_xml.push_str("</Types>\n");
        parts.insert("[Content_Types].xml".to_string(), ct_xml.into_bytes());
        parts_reconstructed += 1;
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: "欠落していた [Content_Types].xml を再構築しました".to_string(),
        });
    }

    // 9. Re-bundle into a clean OOXML ZIP package.
    let mut writer = Writer::new();
    for (name, content) in &parts {
        if let Err(e) = writer.add(name, EntryMeta::new(), content) {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Warning,
                message: format!("パーツ '{}' の書き込みをスキップしました: {}", name, e),
            });
        }
    }

    let clean_bytes = writer.finish().map_err(|e| Error::new(format!("パッケージの再構築に失敗しました: {}", e)))?;

    // 10. Open using standard ibis_pptx::open.
    let opened = crate::open(clean_bytes.clone()).map_err(|e| {
        Error::new(format!("修復後データの解析に失敗しました: {}", e))
    })?;

    // 11. Count recovered statistics.
    let sheets_recovered = opened.presentation.slides.len();
    let mut cells_recovered = 0;
    let mut formulas_recovered = 0;
    for slide in &opened.presentation.slides {
        for shape in &slide.shapes {
            if let ibis_doc_model::ShapeKind::Sheet(sheet) = &shape.kind {
                for cell in &sheet.cells {
                    cells_recovered += 1;
                    if cell.formula.is_some() {
                        formulas_recovered += 1;
                    }
                }
            }
        }
    }
    if let Some(wb) = &opened.presentation.workbook {
        for s in &wb.sheets {
            if let Some(hidden) = &s.hidden {
                for cell in &hidden.cells {
                    cells_recovered += 1;
                    if cell.formula.is_some() {
                        formulas_recovered += 1;
                    }
                }
            }
        }
    }

    logs.push(RepairLogEntry {
        level: RepairLogLevel::Info,
        message: format!(
            "修復完了: {} 枚のシート、{} 個のセル、{} 個の数式を復旧しました",
            sheets_recovered, cells_recovered, formulas_recovered
        ),
    });

    let report = RepairReport {
        mode,
        success: true,
        logs,
        sheets_recovered,
        cells_recovered,
        formulas_recovered,
        formulas_converted_to_values: formulas_converted,
        parts_recovered,
        parts_reconstructed,
    };

    Ok(RepairedWorkbook {
        presentation: opened.presentation,
        source: opened.source,
        repaired_bytes: clean_bytes,
        report,
    })
}

/// Extract all recoverable parts from data.
fn extract_parts(data: &[u8], logs: &mut Vec<RepairLogEntry>) -> HashMap<String, Vec<u8>> {
    let mut parts = HashMap::new();

    // First attempt: read via standard Archive.
    if let Ok(archive) = ibis_ooxml::zip::Archive::new(Arc::from(data)) {
        for entry in archive.entries() {
            if let Ok(bytes) = archive.read(entry) {
                parts.insert(entry.name.clone(), bytes);
            }
        }
        if !parts.is_empty() {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Info,
                message: format!("ZIP セントラルディレクトリから {} 個のパーツを回収しました", parts.len()),
            });
            return parts;
        }
    }

    // Fallback: scan for local file headers (0x0403_4b50 / PK\x03\x04).
    logs.push(RepairLogEntry {
        level: RepairLogLevel::Warning,
        message: "セントラルディレクトリからパーツを回収できなかったため、ローカルヘッダー走査を実行します".to_string(),
    });

    let mut i = 0;
    while i + 30 <= data.len() {
        if &data[i..i + 4] == b"PK\x03\x04" {
            let method = u16::from_le_bytes(data[i + 8..i + 10].try_into().unwrap_or_default());
            let comp_size = u32::from_le_bytes(data[i + 18..i + 22].try_into().unwrap_or_default()) as usize;
            let uncomp_size = u32::from_le_bytes(data[i + 22..i + 26].try_into().unwrap_or_default()) as usize;
            let name_len = u16::from_le_bytes(data[i + 26..i + 28].try_into().unwrap_or_default()) as usize;
            let extra_len = u16::from_le_bytes(data[i + 28..i + 30].try_into().unwrap_or_default()) as usize;
            let name_start = i + 30;

            if name_start + name_len <= data.len() {
                if let Ok(name_str) = std::str::from_utf8(&data[name_start..name_start + name_len]) {
                    let is_plausible_name = name_str == "[Content_Types].xml"
                        || name_str.starts_with("xl/")
                        || name_str.starts_with("_rels/")
                        || name_str.starts_with("docProps/");

                    if is_plausible_name && !parts.contains_key(name_str) {
                        let data_start = name_start + name_len + extra_len;
                        if comp_size > 0 && data_start + comp_size <= data.len() {
                            let raw = &data[data_start..data_start + comp_size];
                            let decompressed = match method {
                                0 => Some(raw.to_vec()),
                                8 => miniz_oxide::inflate::decompress_to_vec_with_limit(
                                    raw,
                                    uncomp_size.max(comp_size).max(1024 * 1024 * 16),
                                ).ok(),
                                _ => None,
                            };
                            if let Some(content) = decompressed {
                                logs.push(RepairLogEntry {
                                    level: RepairLogLevel::Repaired,
                                    message: format!("ローカルヘッダー走査によりパーツ '{}' を回収しました", name_str),
                                });
                                parts.insert(name_str.to_string(), content);
                            }
                        } else if method == 8 && data_start < data.len() {
                            let max_decomp = 1024 * 1024 * 16;
                            if let Ok(content) = miniz_oxide::inflate::decompress_to_vec_with_limit(&data[data_start..], max_decomp) {
                                logs.push(RepairLogEntry {
                                    level: RepairLogLevel::Repaired,
                                    message: format!("ストリーム展開によりパーツ '{}' を回収しました", name_str),
                                });
                                parts.insert(name_str.to_string(), content);
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }

    parts
}

/// Replace invalid XML 1.0 control characters (0x00..=0x1F except \t, \n, \r) with space.
fn sanitize_xml_bytes(bytes: &[u8], name: &str, logs: &mut Vec<RepairLogEntry>) -> Vec<u8> {
    let mut modified = false;
    let mut out = Vec::with_capacity(bytes.len());
    for &b in bytes {
        if b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r' {
            out.push(b' ');
            modified = true;
        } else {
            out.push(b);
        }
    }
    if modified {
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: format!("パーツ '{}' 内の不正な XML 制御文字 (0x00-0x1F) をサニタイズしました", name),
        });
    }
    out
}

/// Ensure XML string is well-formed; if truncated or unclosed, balance/close tags.
fn repair_xml_syntax(xml_str: &str, name: &str, logs: &mut Vec<RepairLogEntry>) -> String {
    if roxmltree::Document::parse(xml_str).is_ok() {
        return xml_str.to_string();
    }

    logs.push(RepairLogEntry {
        level: RepairLogLevel::Warning,
        message: format!("パーツ '{}' の XML 構文破損を検知しました。構文修復を試みます", name),
    });

    let mut repaired = xml_str.to_string();

    // 1. If ends with incomplete open tag like "<c r=\"A1\"", strip it.
    if let Some(last_lt) = repaired.rfind('<') {
        let after_lt = &repaired[last_lt..];
        if !after_lt.contains('>') {
            repaired.truncate(last_lt);
        }
    }

    // 2. Try closing blocks commonly found in SpreadsheetML.
    for block in [
        "</v></c></row></sheetData></worksheet>",
        "</c></row></sheetData></worksheet>",
        "</row></sheetData></worksheet>",
        "</sheetData></worksheet>",
        "</worksheet>",
        "</workbook>",
        "</Relationships>",
        "</Types>",
    ] {
        let test = format!("{}{}", repaired, block);
        if roxmltree::Document::parse(&test).is_ok() {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Repaired,
                message: format!("パーツ '{}' の構造終了タグ群を補完修復しました", name),
            });
            return test;
        }
    }

    // 3. Fallback: Tag-stack based auto-closing of unclosed tags.
    let mut stack = Vec::new();
    let mut i = 0;
    let bytes = repaired.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if i + 1 < bytes.len() && (bytes[i + 1] == b'?' || bytes[i + 1] == b'!') {
                if let Some(end) = repaired[i..].find('>') {
                    i += end + 1;
                    continue;
                }
            } else if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                let start = i + 2;
                if let Some(end) = repaired[start..].find('>') {
                    let tag_name = repaired[start..start + end].trim().to_string();
                    if let Some(pos) = stack.iter().rposition(|t| t == &tag_name) {
                        stack.truncate(pos);
                    }
                    i = start + end + 1;
                    continue;
                }
            } else {
                let start = i + 1;
                if let Some(end) = repaired[start..].find('>') {
                    let tag_content = &repaired[start..start + end];
                    if !tag_content.ends_with('/') {
                        let tag_name = tag_content.split_whitespace().next().unwrap_or("").to_string();
                        if !tag_name.is_empty() {
                            stack.push(tag_name);
                        }
                    }
                    i = start + end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }

    if !stack.is_empty() {
        let mut test = repaired.clone();
        while let Some(tag) = stack.pop() {
            test.push_str(&format!("</{}>", tag));
        }
        if roxmltree::Document::parse(&test).is_ok() {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Repaired,
                message: format!("パーツ '{}' の階層タグを自動スタック補完修復しました", name),
            });
            return test;
        }
    }

    repaired
}

/// Strip <f>...</f> formula elements from sheet XML for ExtractValues mode.
fn strip_formulas_from_sheet(sheet_xml: &str, count: &mut usize) -> String {
    let mut result = String::with_capacity(sheet_xml.len());
    let mut pos = 0;
    while let Some(start) = sheet_xml[pos..].find("<f") {
        let abs_start = pos + start;
        let next_char = sheet_xml[abs_start..].chars().nth(2);
        if next_char == Some('>') || next_char == Some(' ') || next_char == Some('/') {
            if let Some(end_tag) = sheet_xml[abs_start..].find("</f>") {
                let abs_end = abs_start + end_tag + 4;
                result.push_str(&sheet_xml[pos..abs_start]);
                pos = abs_end;
                *count += 1;
                continue;
            } else if let Some(self_close) = sheet_xml[abs_start..].find("/>") {
                let abs_end = abs_start + self_close + 2;
                result.push_str(&sheet_xml[pos..abs_start]);
                pos = abs_end;
                *count += 1;
                continue;
            }
        }
        result.push_str(&sheet_xml[pos..abs_start + 2]);
        pos = abs_start + 2;
    }
    result.push_str(&sheet_xml[pos..]);
    result
}
