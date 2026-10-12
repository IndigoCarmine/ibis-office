// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reading an external workbook link (`xl/externalLinks/externalLink*.xml`,
//! ECMA-376 Part 1 §18.14): the file's name for formulas to show
//! (`[売上.xlsx]Sheet1!A1`) and the values of its cells the link caches
//! (`sheetDataSet`), which the formulas read while the workbook is not open.

use ibis_doc_model::{CellError, CellValue, ExternalBook, ExternalSheet};
use roxmltree::Node;

use crate::xml::{attr, child, children};

/// The name of the file a link's target names, as Excel writes it in a
/// formula: its last path part, percent-decoded (`file:///C:/data/%E5%A3%B2.xlsx`
/// is `売上.xlsx`).
pub(crate) fn book_name(target: &str) -> String {
    let last = target.rsplit(['/', '\\']).next().unwrap_or(target);
    let last = last.strip_prefix('[').and_then(|l| l.split_once(']').map(|(_, rest)| rest)).unwrap_or(last);
    percent_decode(last)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// A link part's root, with the target of its file (`None` for a link
/// that is not a workbook's).
pub(crate) fn read_link(root: Node, target: Option<&str>) -> ExternalBook {
    let Some(book) = child(root, "externalBook") else { return ExternalBook::default() };
    let mut sheets: Vec<ExternalSheet> = child(book, "sheetNames")
        .into_iter()
        .flat_map(|n| children(n, "sheetName"))
        .map(|n| ExternalSheet { name: attr(n, "val").unwrap_or("").to_string(), cells: Default::default() })
        .collect();
    if let Some(set) = child(book, "sheetDataSet") {
        for data in children(set, "sheetData") {
            let Some(sheet) = attr(data, "sheetId").and_then(|v| v.parse::<usize>().ok()).and_then(|i| sheets.get_mut(i)) else { continue };
            for row in children(data, "row") {
                for cell in children(row, "cell") {
                    let Some((column, r)) = attr(cell, "r").and_then(crate::xlsx::cell_ref) else { continue };
                    let Some(v) = child(cell, "v").and_then(|v| v.text()) else { continue };
                    let value = match attr(cell, "t") {
                        Some("str") => CellValue::Text(v.to_string()),
                        Some("b") => CellValue::Bool(v.trim() == "1" || v.trim().eq_ignore_ascii_case("true")),
                        Some("e") => CellValue::Error(CellError::parse(v.trim()).unwrap_or(CellError::Value)),
                        _ => match v.trim().parse::<f64>() {
                            Ok(n) => CellValue::Number(n),
                            Err(_) => continue,
                        },
                    };
                    sheet.cells.insert((r, column), value);
                }
            }
        }
    }
    ExternalBook { name: target.map(book_name).unwrap_or_default(), path: target.unwrap_or("").to_string(), sheets, broken: false }
}

/// The target of a link as a person reads it: percent-decoded, without `file:///`.
pub fn link_display(target: &str) -> String {
    percent_decode(target.strip_prefix("file:///").unwrap_or(target))
}

/// The file a link names, as a path to read: the target as written, with a
/// `file:///` prefix dropped and percent-decoded; a relative one is taken
/// from `dir` (the folder of the workbook that links to it).
pub fn link_path(target: &str, dir: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    if target.is_empty() {
        return None;
    }
    let decoded = percent_decode(target.strip_prefix("file:///").map(|t| if t.starts_with('/') || t.chars().nth(1) == Some(':') { t.to_string() } else { format!("/{t}") }).as_deref().unwrap_or(target));
    let path = std::path::PathBuf::from(&decoded);
    if path.is_absolute() { Some(path) } else { dir.map(|d| d.join(path)) }
}

/// The cached sheets of `book` made again from the workbook file `data`:
/// each cached cell takes the value of the same cell of the sheet of that
/// name there (empty when it is not there any more); a sheet the file has
/// no more keeps what it had.
pub fn refreshed_sheets(book: &ExternalBook, data: &[u8]) -> Result<Vec<ExternalSheet>, crate::Error> {
    let opened = crate::open(data.to_vec())?;
    Ok(book
        .sheets
        .iter()
        .map(|cached| {
            let found = opened.presentation.slides.iter().find(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&cached.name)));
            let Some(sheet) = found.and_then(ibis_doc_model::sheet_calc::sheet_of_slide) else { return cached.clone() };
            ExternalSheet { name: cached.name.clone(), cells: cached.cells.keys().map(|&(r, c)| ((r, c), sheet.cell(r, c).map(|x| x.value.clone()).unwrap_or(CellValue::Empty))).collect() }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_gives_its_name_and_cached_cells() {
        let xml = r#"<externalLink xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><externalBook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId1"><sheetNames><sheetName val="Data"/><sheetName val="Plan"/></sheetNames><sheetDataSet><sheetData sheetId="0"><row r="1"><cell r="A1"><v>1.5</v></cell><cell r="B1" t="str"><v>名前</v></cell></row><row r="3"><cell r="A3" t="b"><v>1</v></cell><cell r="C3" t="e"><v>#N/A</v></cell></row></sheetData><sheetData sheetId="1"><row r="2"><cell r="B2"><v>7</v></cell></row></sheetData></sheetDataSet></externalBook></externalLink>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let book = read_link(doc.root_element(), Some("file:///C:/data/%E5%A3%B2%E4%B8%8A.xlsx"));
        assert_eq!(book.name, "売上.xlsx");
        assert_eq!(book.sheets.len(), 2);
        assert_eq!(book.sheets[0].cells[&(0, 0)], CellValue::Number(1.5));
        assert_eq!(book.sheets[0].cells[&(0, 1)], CellValue::Text("名前".into()));
        assert_eq!(book.sheets[0].cells[&(2, 0)], CellValue::Bool(true));
        assert_eq!(book.sheets[0].cells[&(2, 2)], CellValue::Error(CellError::NA));
        assert_eq!(book.sheets[1].cells[&(1, 1)], CellValue::Number(7.0));
        assert_eq!(link_path("file:///Users/x/%E5%A3%B2.xlsx", None), Some(std::path::PathBuf::from("/Users/x/売.xlsx")));
        assert_eq!(link_path("sub/b.xlsx", Some(std::path::Path::new("/docs"))), Some(std::path::PathBuf::from("/docs/sub/b.xlsx")));
        assert_eq!(link_path("b.xlsx", None), None);
        assert_eq!(book_name("Book2.xlsx"), "Book2.xlsx");
        assert_eq!(book_name(r"C:\dir\Book 3.xlsx"), "Book 3.xlsx");
        assert_eq!(book_name("[1]Book4.xlsx"), "Book4.xlsx");
    }
}
