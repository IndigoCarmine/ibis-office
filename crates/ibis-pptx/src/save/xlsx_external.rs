// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Saving external workbook links (`xl/externalLinks/externalLink*.xml`):
//! the cached cells an update changed are written in their `cell`s, and a
//! broken link (リンクの解除) is taken out: its reference, relationship and
//! part go, and the numbers of the links after it (`[2]` in a formula)
//! close up in every formula of the file.

use ibis_doc_model::{CellValue, ExternalBook, Presentation};
use ibis_ooxml::Package;

use crate::Error;

/// The links' books of the workbook, in order.
fn books(p: &Presentation) -> &[ExternalBook] {
    p.workbook.as_deref().map_or(&[], |w| w.external_books.as_slice())
}

/// A cached value as a link's `cell` writes it: its `t` and its `v`.
fn cell_value(v: &CellValue) -> Option<(Option<&'static str>, String)> {
    Some(match v {
        CellValue::Number(n) => (None, ibis_calc::number_to_xml(*n)),
        CellValue::Text(t) => (Some("str"), t.clone()),
        CellValue::Bool(b) => (Some("b"), u8::from(*b).to_string()),
        CellValue::Error(e) => (Some("e"), e.as_str().to_string()),
        CellValue::Empty => return None,
    })
}

/// The cells of links whose cache changed are written again; broken links go.
pub(super) fn write_external_links(package: &mut Package, workbook_part: &str, parts: &[Option<String>], baseline: &Presentation, current: &Presentation) -> Result<(), Error> {
    let (old, new) = (books(baseline), books(current));
    for (i, (a, b)) in old.iter().zip(new).enumerate() {
        if a.sheets == b.sheets || b.broken {
            continue;
        }
        let Some(part) = parts.get(i).cloned().flatten() else { continue };
        let doc = package.xml_mut(&part)?;
        let Some(root) = doc.root_element() else { continue };
        for data in doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "sheetData").collect::<Vec<_>>() {
            let Some(sheet) = doc.attr(data, "sheetId").and_then(|v| v.parse::<usize>().ok()).and_then(|k| b.sheets.get(k)) else { continue };
            for cell in doc.descendants(data).into_iter().filter(|&n| doc.local_name(n) == "cell").collect::<Vec<_>>() {
                let Some((column, row)) = doc.attr(cell, "r").and_then(crate::xlsx::cell_ref) else { continue };
                let Some((t, text)) = sheet.cells.get(&(row, column)).and_then(cell_value) else { continue };
                match t {
                    Some(t) => doc.set_attr(cell, "t", t),
                    None => doc.remove_attr(cell, "t"),
                }
                if let Some(v) = doc.child(cell, "v") {
                    doc.set_text(v, &text);
                }
            }
        }
    }
    let broken: Vec<usize> = new.iter().enumerate().filter(|(i, b)| b.broken && !old.get(*i).is_some_and(|o| o.broken)).map(|(i, _)| i).collect();
    if broken.is_empty() {
        return Ok(());
    }
    remove_links(package, workbook_part, parts, &broken, new.len())
}

fn remove_links(package: &mut Package, workbook_part: &str, parts: &[Option<String>], broken: &[usize], count: usize) -> Result<(), Error> {
    // The new numbers (1-based) of the links that stay; the broken ones have none.
    let mut next = 0;
    let map: Vec<(usize, Option<usize>)> = (0..count)
        .map(|i| {
            if broken.contains(&i) {
                (i + 1, None)
            } else {
                next += 1;
                (i + 1, Some(next))
            }
        })
        .collect();
    // Their references, relationships and parts.
    let rels = package.relationships(workbook_part)?;
    let doc = package.xml_mut(workbook_part)?;
    let Some(root) = doc.root_element() else { return Ok(()) };
    let mut removed_ids = Vec::new();
    if let Some(list) = doc.child(root, "externalReferences") {
        let refs: Vec<_> = doc.children_named(list, "externalReference").collect();
        for &i in broken {
            if let Some(&r) = refs.get(i) {
                if let Some(id) = doc.attr_local(r, "id") {
                    removed_ids.push(id.to_string());
                }
                doc.detach(r);
            }
        }
        if doc.children_named(list, "externalReference").next().is_none() {
            doc.detach(list);
        }
    }
    for id in &removed_ids {
        package.remove_relationship(workbook_part, id)?;
    }
    for rel in rels.iter().filter(|r| removed_ids.contains(&r.id)) {
        if let Some(part) = rel.target_part(workbook_part) {
            package.remove_override(&part)?;
            package.remove(&part);
            package.remove(&ibis_ooxml::package::relationships_part(&part));
        }
    }
    let _ = parts;
    // The numbers in every formula of the file.
    let names: Vec<String> = package.names().filter(|n| n.starts_with("xl/worksheets/") || n.starts_with("xl/charts/chart") || *n == workbook_part).map(str::to_string).collect();
    for name in names.into_iter().filter(|n| n.ends_with(".xml")) {
        let doc = package.xml_mut(&name)?;
        let Some(root) = doc.root_element() else { continue };
        for n in doc.descendants(root) {
            if !matches!(doc.local_name(n), "f" | "formula" | "formula1" | "formula2" | "definedName") {
                continue;
            }
            let text = doc.text(n);
            if text.contains('[') && ibis_calc::formula::has_external_book(&text) {
                let new = ibis_calc::formula::renumber_books(&text, &map);
                if new != text {
                    doc.set_text(n, &new);
                }
            }
        }
    }
    Ok(())
}
