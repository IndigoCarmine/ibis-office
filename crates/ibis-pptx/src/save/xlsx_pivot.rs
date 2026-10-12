// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Saving pivot tables: one built again keeps its parts and asks Excel to
//! build its cache again on opening; one made in the app, or laid out
//! anew, gets its definition and its cache's definition written from the
//! model (ECMA-376 Part 1 §18.10), with no records and `refreshOnLoad`, so
//! Excel builds them from the source when it opens the file.

use ibis_doc_model::{CellValue, PivotAxis, PivotTable, PivotSort};
use ibis_ooxml::Package;

use super::super::unsupported;
use crate::Error;

const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_PIVOT_TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable";
const REL_CACHE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition";
const REL_RECORDS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheRecords";
const CT_PIVOT_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml";
const CT_CACHE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml";
const CT_RECORDS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn range(r: ibis_doc_model::CellRange) -> String {
    format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row))
}

/// The same table but for what building it again changes.
fn layout(p: &PivotTable) -> PivotTable {
    PivotTable {
        location: ibis_doc_model::CellRange { last_row: p.location.first_row, last_column: p.location.first_column, ..p.location },
        first_header_row: 0,
        first_data_row: 0,
        first_data_col: 0,
        row_items: Vec::new(),
        col_items: Vec::new(),
        refreshed: false,
        refresh_on_load: false,
        buttons: Vec::new(),
        corner: None,
        ..p.clone()
    }
}

/// A number as the file writes it.
fn number(n: f64) -> String {
    ibis_calc::number_to_xml(n)
}

/// The cache's definition: its source and each field's items (the ones
/// the table's items point at), no records, built again on opening.
fn cache_xml(p: &PivotTable, records_rid: &str) -> String {
    let mut out = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><pivotCacheDefinition xmlns="{MAIN_NS}" xmlns:r="{REL_NS}" r:id="{records_rid}" refreshOnLoad="1" createdVersion="6" refreshedVersion="6" minRefreshableVersion="3" recordCount="0"><cacheSource type="worksheet">"#);
    out += "<worksheetSource";
    if let Some(n) = &p.source.name {
        out += &format!(r#" name="{}""#, esc(n));
    } else {
        if let Some(r) = p.source.range {
            out += &format!(r#" ref="{}""#, range(r));
        }
        if let Some(s) = &p.source.sheet {
            out += &format!(r#" sheet="{}""#, esc(s));
        }
    }
    out += &format!(r#"/></cacheSource><cacheFields count="{}">"#, p.fields.len());
    for f in &p.fields {
        let formula_attr = f.formula.as_ref().map(|fml| format!(r#" formula="{}""#, esc(fml.trim_start_matches('=').trim()))).unwrap_or_default();
        out += &format!(r#"<cacheField name="{}"{formula_attr} numFmtId="0">"#, esc(&f.name));
        let numbers: Vec<f64> = f.shared.iter().filter_map(|v| if let CellValue::Number(n) = v { Some(*n) } else { None }).collect();
        let texts = f.shared.iter().any(|v| matches!(v, CellValue::Text(_) | CellValue::Bool(_) | CellValue::Error(_)));
        let blank = f.shared.iter().any(|v| matches!(v, CellValue::Empty));
        let mut attrs = String::new();
        if !numbers.is_empty() {
            if !texts {
                attrs += r#" containsSemiMixedTypes="0" containsString="0""#;
            } else {
                attrs += r#" containsMixedTypes="1""#;
            }
            let all_int = numbers.iter().all(|n| n.fract() == 0.0);
            attrs += r#" containsNumber="1""#;
            if all_int {
                attrs += r#" containsInteger="1""#;
            }
            let (lo, hi) = numbers.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &n| (a.min(n), b.max(n)));
            attrs += &format!(r#" minValue="{}" maxValue="{}""#, number(lo), number(hi));
        }
        if blank {
            attrs += r#" containsBlank="1""#;
        }
        out += &format!(r#"<sharedItems{attrs} count="{}">"#, f.shared.len());
        for v in &f.shared {
            out += &match v {
                CellValue::Text(t) => format!(r#"<s v="{}"/>"#, esc(t)),
                CellValue::Number(n) => format!(r#"<n v="{}"/>"#, number(*n)),
                CellValue::Bool(b) => format!(r#"<b v="{}"/>"#, u8::from(*b)),
                CellValue::Error(e) => format!(r#"<e v="{}"/>"#, esc(e.as_str())),
                CellValue::Empty => "<m/>".to_string(),
            };
        }
        out += "</sharedItems>";
        if let Some(g) = &f.grouping {
            out += "<fieldGroup>";
            match g {
                ibis_doc_model::PivotFieldGrouping::Number { start, end, interval, auto_start, auto_end } => {
                    let mut rp = format!(r#"<rangePr groupBy="range" startNum="{}" endNum="{}" groupInterval="{}""#, number(*start), number(*end), number(*interval));
                    if !auto_start {
                        rp += r#" autoStart="0""#;
                    }
                    if !auto_end {
                        rp += r#" autoEnd="0""#;
                    }
                    rp += "/>";
                    out += &rp;
                }
                ibis_doc_model::PivotFieldGrouping::Date { by, start, end, auto_start, auto_end } => {
                    let mut rp = format!(r#"<rangePr groupBy="{}""#, by.as_str());
                    if let Some(s) = start {
                        rp += &format!(r#" startNum="{}""#, number(*s));
                    }
                    if let Some(e) = end {
                        rp += &format!(r#" endNum="{}""#, number(*e));
                    }
                    if !auto_start {
                        rp += r#" autoStart="0""#;
                    }
                    if !auto_end {
                        rp += r#" autoEnd="0""#;
                    }
                    rp += "/>";
                    out += &rp;
                }
                ibis_doc_model::PivotFieldGrouping::Manual { groups } => {
                    out += "<discretePr/>";
                    let mut unique_groups: Vec<&String> = groups.values().collect();
                    unique_groups.sort();
                    unique_groups.dedup();
                    out += &format!(r#"<groupItems count="{}">"#, unique_groups.len());
                    for grp in &unique_groups {
                        out += &format!(r#"<s v="{}"/>"#, esc(grp));
                    }
                    out += "</groupItems>";
                    out += &format!(r#"<discreteItems count="{}">"#, groups.len());
                    for (item, grp) in groups {
                        out += &format!(r#"<item name="{}" group="{}"/>"#, esc(item), esc(grp));
                    }
                    out += "</discreteItems>";
                }
            }
            out += "</fieldGroup>";
        }
        out += "</cacheField>";
    }
    out += "</cacheFields>";
    if !p.calculated_items.is_empty() {
        out += &format!(r#"<calculatedItems count="{}">"#, p.calculated_items.len());
        for ci in &p.calculated_items {
            let clean = ci.formula.trim().trim_start_matches('=').trim();
            out += &format!(r#"<calculatedItem field="{}" name="{}" formula="{}"/>"#, ci.field, esc(&ci.name), esc(clean));
        }
        out += "</calculatedItems>";
    }
    out += "</pivotCacheDefinition>";
    out
}

const RECORDS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><pivotCacheRecords xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" count="0"/>"#;

/// The table's definition, without its lines (`rowItems`, `colItems`),
/// which Excel builds on opening.
fn table_xml(p: &PivotTable, cache_id: u32) -> String {
    let auto_fit = if p.auto_fit_columns { "1" } else { "0" };
    let use_auto_fmt = if p.preserve_formatting { "1" } else { "0" };
    let merge_item_attr = if p.merge_item { r#" mergeItem="1""# } else { "" };
    let multiple_filters = if p.multiple_field_filters { "1" } else { "0" };
    let mut out = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><pivotTableDefinition xmlns="{MAIN_NS}" name="{}" cacheId="{cache_id}" applyNumberFormats="0" applyBorderFormats="0" applyFontFormats="0" applyPatternFormats="0" applyAlignmentFormats="0" applyWidthHeightFormats="{auto_fit}" dataCaption="{}" updatedVersion="6" minRefreshableVersion="3" useAutoFormatting="{use_auto_fmt}" itemPrintTitles="1" createdVersion="6" indent="0"{merge_item_attr}"#,
        esc(&p.name),
        esc(p.data_caption.as_deref().unwrap_or("値")),
    );
    if !p.row_grand_totals {
        out += r#" rowGrandTotals="0""#;
    }
    if !p.col_grand_totals {
        out += r#" colGrandTotals="0""#;
    }
    if !p.compact {
        out += r#" compact="0" compactData="0""#;
    }
    if p.outline {
        out += r#" outline="1" outlineData="1""#;
    }
    for (name, v) in [("rowHeaderCaption", &p.row_header_caption), ("colHeaderCaption", &p.col_header_caption), ("grandTotalCaption", &p.grand_total_caption)] {
        if let Some(v) = v {
            out += &format!(r#" {name}="{}""#, esc(v));
        }
    }
    if let Some(c) = &p.missing_caption {
        out += &format!(r#" showMissing="1" missingCaption="{}""#, esc(c));
    } else if p.missing_zero {
        out += r#" showMissing="0""#;
    }
    if let Some(c) = &p.error_caption {
        out += &format!(r#" showError="1" errorCaption="{}""#, esc(c));
    }
    out += &format!(r#" multipleFieldFilters="{multiple_filters}">"#);
    let pages = p.page_fields.len();
    out += &format!(r#"<location ref="{}" firstHeaderRow="1" firstDataRow="{}" firstDataCol="{}""#, range(p.location), p.first_data_row.max(1), p.first_data_col.max(1));
    if pages > 0 {
        out += &format!(r#" rowPageCount="{pages}" colPageCount="1""#);
    }
    out += "/>";
    out += &format!(r#"<pivotFields count="{}">"#, p.fields.len());
    for (i, f) in p.fields.iter().enumerate() {
        out += "<pivotField";
        let axis = match p.place_of(i).map(|(a, _)| a).filter(|a| *a != PivotAxis::Values) {
            Some(PivotAxis::Row) => Some("axisRow"),
            Some(PivotAxis::Column) => Some("axisCol"),
            Some(PivotAxis::Page) => Some("axisPage"),
            _ => None,
        };
        if let Some(a) = axis {
            out += &format!(r#" axis="{a}""#);
        }
        if p.data_fields.iter().any(|d| d.field == i as u32) {
            out += r#" dataField="1""#;
        }
        if !f.compact {
            out += r#" compact="0""#;
        }
        if !f.outline {
            out += r#" outline="0""#;
        }
        out += r#" showAll="0""#;
        if !f.subtotal_top {
            out += r#" subtotalTop="0""#;
        }
        match f.sort {
            PivotSort::Ascending => {
                out += r#" sortType="ascending""#;
                if let Some(df) = f.sort_data_field {
                    out += &format!(r#" dataField="{df}""#);
                }
            }
            PivotSort::Descending => {
                out += r#" sortType="descending""#;
                if let Some(df) = f.sort_data_field {
                    out += &format!(r#" dataField="{df}""#);
                }
            }
            PivotSort::Manual => {}
        }
        if !f.default_subtotal {
            out += r#" defaultSubtotal="0""#;
        }
        let fill_down = p.fill_down_labels && axis == Some("axisRow");
        if axis.is_some() && !f.items.is_empty() {
            let items: Vec<_> = f.items.iter().filter(|it| f.default_subtotal || it.kind.is_none()).collect();
            out += &format!(r#"><items count="{}">"#, items.len());
            for it in items {
                match (&it.kind, it.x) {
                    (Some(k), _) => out += &format!(r#"<item t="{}"/>"#, esc(k)),
                    (None, Some(x)) => out += &format!(r#"<item x="{x}"{}/>"#, if it.hidden { r#" h="1""# } else { "" }),
                    (None, None) => {}
                }
            }
            out += "</items>";
            if fill_down {
                out += FILL_DOWN_FIELD;
            }
            out += "</pivotField>";
        } else if fill_down {
            out += ">";
            out += FILL_DOWN_FIELD;
            out += "</pivotField>";
        } else {
            out += "/>";
        }
    }
    out += "</pivotFields>";
    let fields = |name: &str, list: &[i32]| if list.is_empty() { String::new() } else { format!(r#"<{name} count="{}">{}</{name}>"#, list.len(), list.iter().map(|x| format!(r#"<field x="{x}"/>"#)).collect::<String>()) };
    out += &fields("rowFields", &p.row_fields);
    out += &fields("colFields", &p.col_fields);
    if pages > 0 {
        out += &format!(r#"<pageFields count="{pages}">"#);
        for pf in &p.page_fields {
            out += &format!(r#"<pageField fld="{}"{} hier="-1"/>"#, pf.field, pf.item.map(|i| format!(r#" item="{i}""#)).unwrap_or_default());
        }
        out += "</pageFields>";
    }
    if !p.data_fields.is_empty() {
        out += &format!(r#"<dataFields count="{}">"#, p.data_fields.len());
        for d in &p.data_fields {
            out += &format!(r#"<dataField name="{}" fld="{}""#, esc(&d.name), d.field);
            if d.function != "sum" {
                out += &format!(r#" subtotal="{}""#, esc(&d.function));
            }
            out += r#" baseField="0" baseItem="0""#;
            if let Some(show_as) = &d.show_as {
                out += &format!(r#" showDataAs="{}""#, esc(show_as));
            }
            let num_fmt_id = d.number_format.as_deref().and_then(crate::xlsx::builtin_format_id)
                .or_else(|| if d.show_as.as_deref().is_some_and(|s| s.starts_with("percent")) { Some(10) } else { None });
            if let Some(id) = num_fmt_id {
                out += &format!(r#" numFmtId="{id}""#);
            }
            out += "/>";
        }
        out += "</dataFields>";
    }
    if let Some(s) = &p.style {
        let b = |v: bool| u8::from(v);
        out += &format!(r#"<pivotTableStyleInfo name="{}" showRowHeaders="{}" showColHeaders="{}" showRowStripes="{}" showColStripes="{}" showLastColumn="{}"/>"#, esc(&s.name), b(s.row_headers), b(s.col_headers), b(s.row_stripes), b(s.col_stripes), b(s.last_column));
    }
    if !p.filters.is_empty() {
        out += &format!(r#"<filters count="{}">"#, p.filters.len());
        for (i, f) in p.filters.iter().enumerate() {
            out += &format!(r#"<filter fld="{}" type="{}" evalOrder="-1" id="{}""#, f.field, esc(&f.kind), i + 1);
            if !f.is_label() {
                out += &format!(r#" iMeasureFld="{}""#, f.measure);
            }
            out += r#"><autoFilter ref="A1"><filterColumn colId="0">"#;
            let custom = |op: Option<&str>, val: &str| format!(r#"<customFilter{} val="{}"/>"#, op.map(|o| format!(r#" operator="{o}""#)).unwrap_or_default(), esc(val));
            let v = f.value.as_str();
            let wild = |before: &str, after: &str| format!("{before}{v}{after}");
            let two = f.value2.as_deref().unwrap_or("");
            let body = match f.kind.as_str() {
                "captionEqual" | "valueEqual" => format!("<customFilters>{}</customFilters>", custom(None, v)),
                "captionNotEqual" | "valueNotEqual" => format!("<customFilters>{}</customFilters>", custom(Some("notEqual"), v)),
                "captionBeginsWith" => format!("<customFilters>{}</customFilters>", custom(None, &wild("", "*"))),
                "captionNotBeginsWith" => format!("<customFilters>{}</customFilters>", custom(Some("notEqual"), &wild("", "*"))),
                "captionEndsWith" => format!("<customFilters>{}</customFilters>", custom(None, &wild("*", ""))),
                "captionNotEndsWith" => format!("<customFilters>{}</customFilters>", custom(Some("notEqual"), &wild("*", ""))),
                "captionContains" => format!("<customFilters>{}</customFilters>", custom(None, &wild("*", "*"))),
                "captionNotContains" => format!("<customFilters>{}</customFilters>", custom(Some("notEqual"), &wild("*", "*"))),
                "captionGreaterThan" | "valueGreaterThan" => format!("<customFilters>{}</customFilters>", custom(Some("greaterThan"), v)),
                "captionGreaterThanOrEqual" | "valueGreaterThanOrEqual" => format!("<customFilters>{}</customFilters>", custom(Some("greaterThanOrEqual"), v)),
                "captionLessThan" | "valueLessThan" => format!("<customFilters>{}</customFilters>", custom(Some("lessThan"), v)),
                "captionLessThanOrEqual" | "valueLessThanOrEqual" => format!("<customFilters>{}</customFilters>", custom(Some("lessThanOrEqual"), v)),
                "captionBetween" | "valueBetween" => format!(r#"<customFilters and="1">{}{}</customFilters>"#, custom(Some("greaterThanOrEqual"), v), custom(Some("lessThanOrEqual"), two)),
                "captionNotBetween" | "valueNotBetween" => format!("<customFilters>{}{}</customFilters>", custom(Some("lessThan"), v), custom(Some("greaterThan"), two)),
                "count" | "percent" | "sum" => format!(r#"<top10{}{} val="{}" filterVal="{}"/>"#, if f.top { "" } else { r#" top="0""# }, if f.kind == "percent" { r#" percent="1""# } else { "" }, esc(v), esc(v)),
                _ => String::new(),
            };
            out += &body;
            out += "</filterColumn></autoFilter></filter>";
        }
        out += "</filters>";
    }
    if p.fill_down_labels {
        out += r#"<extLst><ext uri="{962EF5D1-5CA2-4c93-8EF4-DBF39C750712}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:pivotTableDefinition fillDownLabelsDefault="1"/></ext></extLst>"#;
    }
    out += "</pivotTableDefinition>";
    out
}

/// A row field's own 項目ラベルを繰り返す.
const FILL_DOWN_FIELD: &str = r#"<extLst><ext uri="{2946ED86-A175-432a-8AC1-64E0C546D7DE}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:pivotField fillDownLabels="1"/></ext></extLst>"#;

/// The path from `from`'s folder to `to` (both part names).
fn relative(from: &str, to: &str) -> String {
    let from_dir: Vec<&str> = from.rsplit_once('/').map_or(Vec::new(), |(d, _)| d.split('/').collect());
    let to_parts: Vec<&str> = to.split('/').collect();
    let common = from_dir.iter().zip(&to_parts).take_while(|(a, b)| a == b).count();
    let mut out: Vec<&str> = vec![".."; from_dir.len() - common];
    out.extend(&to_parts[common..]);
    out.join("/")
}

fn free_name(package: &Package, pattern: &str) -> (String, u32) {
    let mut n = 1;
    while package.contains(&pattern.replace("{}", &n.to_string())) {
        n += 1;
    }
    (pattern.replace("{}", &n.to_string()), n)
}

/// The workbook's cache ids, and a new one.
fn new_cache_id(package: &mut Package, workbook: &str) -> Result<u32, Error> {
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    let max = doc
        .child(root, "pivotCaches")
        .map(|c| doc.children_named(c, "pivotCache").filter_map(|e| doc.attr(e, "cacheId").and_then(|v| v.parse::<u32>().ok())).max().unwrap_or(0))
        .unwrap_or(0);
    Ok(max + 1)
}

/// The workbook's `pivotCache` for the cache `rid`, in its place.
fn add_workbook_cache(package: &mut Package, workbook: &str, cache_id: u32, rid: &str) -> Result<(), Error> {
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    let prefix = doc.prefix(root).map(|p| format!("{p}:")).unwrap_or_default();
    let r = doc.lookup_prefix(root, REL_NS).unwrap_or_else(|| "r".into());
    let list = match doc.child(root, "pivotCaches") {
        Some(l) => l,
        None => {
            let l = doc.insert_element(ibis_ooxml::XmlDocument::ROOT, 0, &format!("{prefix}pivotCaches"));
            doc.detach(l);
            let after = ["smartTagPr", "smartTagTypes", "webPublishing", "fileRecoveryPr", "webPublishObjects", "extLst"];
            let anchor = doc.elements(root).find(|&c| after.contains(&doc.local_name(c)));
            match anchor {
                Some(a) => doc.insert_before(a, l),
                None => {
                    let n = doc.children(root).len();
                    doc.insert(root, n, l);
                }
            }
            l
        }
    };
    if doc.lookup_prefix(root, REL_NS).is_none() {
        doc.set_attr(root, "xmlns:r", REL_NS);
    }
    let e = doc.append_element(list, &format!("{prefix}pivotCache"));
    doc.set_attr(e, "cacheId", &cache_id.to_string());
    doc.set_attr(e, &format!("{r}:id"), rid);
    Ok(())
}

/// The sheet's pivot tables as the model has them now: built again (its
/// place and `refreshOnLoad`), laid out anew (definitions written again),
/// or new (parts added).
pub(crate) fn write_pivots(package: &mut Package, sheet_part: &str, workbook: &str, old: &[PivotTable], new: &[PivotTable]) -> Result<(), Error> {
    if old.iter().any(|o| o.part.is_some() && !new.iter().any(|n| n.part == o.part)) {
        return Err(unsupported("ピボットテーブルの削除"));
    }
    for n in new {
        let before = n.part.as_ref().and_then(|part| old.iter().find(|o| o.part.as_ref() == Some(part)));
        if before == Some(n) {
            continue;
        }
        match (before, &n.part, &n.cache_part) {
            // Built again only: its place, and its cache built again on opening.
            (Some(o), Some(table), Some(cache)) if layout(o) == layout(n) => {
                let doc = package.xml_mut(table)?;
                if let Some(root) = doc.root_element()
                    && let Some(loc) = doc.child(root, "location")
                {
                    doc.set_attr(loc, "ref", &range(n.location));
                    doc.set_attr(loc, "firstHeaderRow", &n.first_header_row.to_string());
                    doc.set_attr(loc, "firstDataRow", &n.first_data_row.to_string());
                    doc.set_attr(loc, "firstDataCol", &n.first_data_col.to_string());
                }
                let doc = package.xml_mut(cache)?;
                if let Some(root) = doc.root_element() {
                    doc.set_attr(root, "refreshOnLoad", "1");
                }
            }
            // Laid out anew: the table's definition written again. The cache's
            // is kept (its records too) when nothing it holds changed (a filter
            // changed which items are shown, say), else written again with the
            // extensions it had (the slicers find the cache by the number in
            // `x14:pivotCacheDefinition`) and its records emptied.
            (Some(o), Some(table), Some(cache)) => {
                let cache_id = {
                    let doc = package.xml_mut(table)?;
                    doc.root_element().and_then(|r| doc.attr(r, "cacheId")).and_then(|v| v.parse::<u32>().ok()).unwrap_or(1)
                };
                package.set_bytes(table, table_xml(n, cache_id).into_bytes());
                let key = |p: &PivotTable| (p.source.clone(), p.fields.iter().map(|f| (f.name.clone(), f.formula.clone(), f.shared.clone(), f.grouping.clone())).collect::<Vec<_>>(), p.calculated_items.clone());
                if key(o) == key(n) {
                    let doc = package.xml_mut(cache)?;
                    if let Some(root) = doc.root_element() {
                        doc.set_attr(root, "refreshOnLoad", "1");
                    }
                    continue;
                }
                // Items only added to the fields' shared items: the old ones keep their numbers, so the records stay.
                let appended = o.source == n.source
                    && o.calculated_items == n.calculated_items
                    && o.fields.len() == n.fields.len()
                    && o.fields.iter().zip(&n.fields).all(|(a, b)| a.name == b.name && a.formula == b.formula && a.grouping == b.grouping && b.shared.starts_with(&a.shared));
                if appended {
                    let doc = package.xml_mut(cache)?;
                    if let Some(root) = doc.root_element() {
                        doc.set_attr(root, "refreshOnLoad", "1");
                        if let Some(fields) = doc.child(root, "cacheFields") {
                            let nodes: Vec<_> = doc.children_named(fields, "cacheField").collect();
                            for (node, (a, b)) in nodes.into_iter().zip(o.fields.iter().zip(&n.fields)) {
                                if a.shared.len() == b.shared.len() {
                                    continue;
                                }
                                let Some(shared) = doc.child(node, "sharedItems") else { continue };
                                let prefix = doc.prefix(shared).map(|p| format!("{p}:")).unwrap_or_default();
                                for name in doc.attributes(shared).iter().map(|a| a.name.clone()).filter(|a| a.starts_with("contains") || a == "minValue" || a == "maxValue").collect::<Vec<_>>() {
                                    doc.remove_attr(shared, &name);
                                }
                                for v in &b.shared[a.shared.len()..] {
                                    let (tag, value) = match v {
                                        CellValue::Text(t) => ("s", Some(t.clone())),
                                        CellValue::Number(x) => ("n", Some(number(*x))),
                                        CellValue::Bool(x) => ("b", Some(u8::from(*x).to_string())),
                                        CellValue::Error(e) => ("e", Some(e.as_str().to_string())),
                                        CellValue::Empty => ("m", None),
                                    };
                                    let count = doc.children(shared).len();
                                    let item = doc.insert_element(shared, count, &format!("{prefix}{tag}"));
                                    if let Some(value) = value {
                                        doc.set_attr(item, "v", &value);
                                    }
                                }
                                doc.set_attr(shared, "count", &b.shared.len().to_string());
                            }
                        }
                    }
                    continue;
                }
                let records = package.relationships(cache)?.into_iter().find(|r| r.kind == REL_RECORDS && !r.external);
                let rid = match &records {
                    Some(r) => {
                        if let Some(part) = r.target_part(cache) {
                            package.set_bytes(&part, RECORDS_XML.as_bytes().to_vec());
                        }
                        r.id.clone()
                    }
                    None => {
                        let (part, _) = free_name(package, "xl/pivotCache/pivotCacheRecords{}.xml");
                        package.set_bytes(&part, RECORDS_XML.as_bytes().to_vec());
                        package.set_override(&part, CT_RECORDS)?;
                        package.add_relationship(cache, REL_RECORDS, &relative(cache, &part), false)?
                    }
                };
                let old = package.xml(cache)?;
                let mut fresh = ibis_ooxml::XmlDocument::parse(cache_xml(n, &rid).as_bytes()).map_err(|e| Error::new(format!("ピボットのキャッシュを作れません: {e}")))?;
                if let (Some(old_root), Some(root)) = (old.root_element(), fresh.root_element())
                    && let Some(ext) = old.child(old_root, "extLst")
                {
                    for a in old.attributes(old_root).iter().filter(|a| a.name.starts_with("xmlns:")) {
                        if fresh.attr(root, &a.name).is_none() {
                            fresh.set_attr(root, &a.name, &a.value);
                        }
                    }
                    // A field whose values the records hold (no shared items of its own): its attributes as they were.
                    if let (Some(old_fields), Some(new_fields)) = (old.child(old_root, "cacheFields"), fresh.child(root, "cacheFields")) {
                        let old_nodes: Vec<_> = old.children_named(old_fields, "cacheField").collect();
                        let new_nodes: Vec<_> = fresh.children_named(new_fields, "cacheField").collect();
                        for nf in new_nodes {
                            let name = fresh.attr(nf, "name").map(str::to_string);
                            let Some(of) = old_nodes.iter().copied().find(|&f| old.attr(f, "name") == name.as_deref()) else { continue };
                            let (Some(os), Some(ns)) = (old.child(of, "sharedItems"), fresh.child(nf, "sharedItems")) else { continue };
                            if fresh.children(ns).iter().any(|&c| fresh.is_element(c)) || old.children(os).iter().any(|&c| old.is_element(c)) {
                                continue;
                            }
                            let at = fresh.children(nf).iter().position(|&c| c == ns).unwrap_or(0);
                            fresh.detach(ns);
                            let copy = fresh.import(&old, os);
                            fresh.insert(nf, at, copy);
                        }
                    }
                    let copy = fresh.import(&old, ext);
                    let at = fresh.children(root).len();
                    fresh.insert(root, at, copy);
                }
                package.set_xml(cache, fresh);
            }
            // New: its parts, their relationships, and the workbook's cache.
            (None, None, _) => {
                let (cache, k) = free_name(package, "xl/pivotCache/pivotCacheDefinition{}.xml");
                let records = format!("xl/pivotCache/pivotCacheRecords{k}.xml");
                let (table, _) = free_name(package, "xl/pivotTables/pivotTable{}.xml");
                let cache_id = new_cache_id(package, workbook)?;
                package.set_bytes(&records, RECORDS_XML.as_bytes().to_vec());
                package.set_override(&records, CT_RECORDS)?;
                // The cache's part is written before its relationship to the records.
                package.set_bytes(&cache, b"<pivotCacheDefinition/>".to_vec());
                package.set_override(&cache, CT_CACHE)?;
                let rid = package.add_relationship(&cache, REL_RECORDS, &relative(&cache, &records), false)?;
                package.set_bytes(&cache, cache_xml(n, &rid).into_bytes());
                package.set_bytes(&table, table_xml(n, cache_id).into_bytes());
                package.set_override(&table, CT_PIVOT_TABLE)?;
                package.add_relationship(&table, REL_CACHE, &relative(&table, &cache), false)?;
                package.add_relationship(sheet_part, REL_PIVOT_TABLE, &relative(sheet_part, &table), false)?;
                let wb_rid = package.add_relationship(workbook, REL_CACHE, &relative(workbook, &cache), false)?;
                add_workbook_cache(package, workbook, cache_id, &wb_rid)?;
            }
            _ => return Err(unsupported("ピボットテーブルの変更")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::relative;

    #[test]
    fn relative_paths() {
        assert_eq!(relative("xl/worksheets/sheet2.xml", "xl/pivotTables/pivotTable1.xml"), "../pivotTables/pivotTable1.xml");
        assert_eq!(relative("xl/workbook.xml", "xl/pivotCache/pivotCacheDefinition1.xml"), "pivotCache/pivotCacheDefinition1.xml");
        assert_eq!(relative("xl/pivotCache/pivotCacheDefinition1.xml", "xl/pivotCache/pivotCacheRecords1.xml"), "pivotCacheRecords1.xml");
    }
}
