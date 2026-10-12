// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Saving slicers (スライサー): a slicer's parts are kept as they were; when
//! the selection of a pivot table's slicer changed in the app, the items
//! its cache (`xl/slicerCaches/slicerCache*.xml`) lists as selected (`s`)
//! follow it. A table's slicer keeps its selection in the table's filter
//! (saved with the table), so its cache has nothing to change.

use std::collections::HashMap;

use ibis_doc_model::slicer::{Slicer, SlicerSource};
use ibis_doc_model::{Presentation, ShapeKind};
use ibis_ooxml::Package;

use crate::Error;

fn slicers_of(p: &Presentation) -> HashMap<String, Slicer> {
    p.slides.iter().flat_map(|s| s.shapes.iter()).filter_map(|sh| match &sh.kind {
        ShapeKind::Slicer(s) => Some((s.name.clone(), (**s).clone())),
        _ => None,
    }).collect()
}

/// The shared-item index (`x`) of each of the slicer's items, in order.
fn shared_indexes(current: &Presentation, slicer: &Slicer) -> Option<Vec<u32>> {
    let SlicerSource::Pivot { field, pivots } = &slicer.source else { return None };
    let (slide, index) = pivots.iter().find_map(|n| ibis_doc_model::slicer::locate_pivot(current, n))?;
    let sheet = current.slide(slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide)?;
    let f = sheet.pivots.get(index)?.fields.iter().find(|f| &f.name == field)?;
    Some(f.items.iter().filter(|it| it.kind.is_none()).filter_map(|it| it.x).collect())
}

/// Make the slicer caches of the pivot slicers whose selection changed say so.
pub(super) fn write_slicer_selection(package: &mut Package, workbook_part: &str, baseline: &Presentation, current: &Presentation) -> Result<(), Error> {
    let (old, new) = (slicers_of(baseline), slicers_of(current));
    let changed: Vec<&Slicer> = new.values().filter(|s| matches!(s.source, SlicerSource::Pivot { .. }) && old.get(&s.name).is_some_and(|o| o.items != s.items)).collect();
    if changed.is_empty() {
        return Ok(());
    }
    let caches: Vec<String> = package.relationships(workbook_part)?.iter().filter(|r| r.kind.ends_with("/slicerCache")).filter_map(|r| r.target_part(workbook_part)).collect();
    for slicer in changed {
        let Some(indexes) = shared_indexes(current, slicer) else { continue };
        for part in &caches {
            let doc = package.xml_mut(part)?;
            let Some(root) = doc.root_element() else { continue };
            if doc.attr(root, "name") != Some(slicer.cache.as_str()) {
                continue;
            }
            for i in doc.descendants(root) {
                if doc.local_name(i) != "i" {
                    continue;
                }
                let Some(x) = doc.attr(i, "x").and_then(|x| x.parse::<u32>().ok()) else { continue };
                let Some(item) = indexes.iter().position(|&v| v == x).and_then(|k| slicer.items.get(k)) else { continue };
                if item.selected {
                    doc.set_attr(i, "s", "1");
                } else {
                    doc.remove_attr(i, "s");
                }
            }
        }
    }
    Ok(())
}

/// A moved or resized slicer: its anchor in the sheet's drawing set anew.
pub(super) fn write_slicer_frames(package: &mut Package, sheet_part: &str, sheet: &ibis_doc_model::Sheet, old: &[ibis_doc_model::Shape], new: &[ibis_doc_model::Shape]) -> Result<(), Error> {
    let moved: Vec<(&str, ibis_doc_model::EmuRect)> = new
        .iter()
        .filter_map(|n| match &n.kind {
            ShapeKind::Slicer(s) => old.iter().find(|o| o.id == n.id).filter(|o| o.frame != n.frame).map(|_| (s.name.as_str(), n.frame)),
            _ => None,
        })
        .collect();
    if moved.is_empty() {
        return Ok(());
    }
    let drawing = package
        .relationships(sheet_part)?
        .iter()
        .find(|r| r.kind.ends_with("/drawing"))
        .and_then(|r| r.target_part(sheet_part))
        .ok_or_else(|| Error::new("スライサーの描画が見つかりません"))?;
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let cell = |offsets: &[i64], v: i64| -> (usize, i64) {
        let i = offsets.partition_point(|&o| o <= v).saturating_sub(1).min(offsets.len().saturating_sub(2));
        (i, v - offsets.get(i).copied().unwrap_or(0))
    };
    let doc = package.xml_mut(&drawing)?;
    let root = doc.root_element().ok_or_else(|| Error::new("描画の XML が空です"))?;
    for (name, frame) in moved {
        let node = doc.descendants(root).into_iter().find(|&n| doc.local_name(n) == "slicer" && doc.attr(n, "name") == Some(name));
        let mut anchor = node;
        while let Some(a) = anchor {
            if matches!(doc.local_name(a), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
                break;
            }
            anchor = doc.parent(a);
        }
        let Some(anchor) = anchor else { continue };
        let kind = doc.local_name(anchor).to_string();
        if kind == "absoluteAnchor" {
            if let Some(p) = doc.child(anchor, "pos") {
                doc.set_attr(p, "x", &frame.x.to_string());
                doc.set_attr(p, "y", &frame.y.to_string());
            }
            if let Some(e) = doc.child(anchor, "ext") {
                doc.set_attr(e, "cx", &frame.width.to_string());
                doc.set_attr(e, "cy", &frame.height.to_string());
            }
            continue;
        }
        let corners: Vec<(&str, i64, i64)> = if kind == "twoCellAnchor" { vec![("from", frame.x, frame.y), ("to", frame.x + frame.width, frame.y + frame.height)] } else { vec![("from", frame.x, frame.y)] };
        for (corner_name, x, y) in corners {
            let Some(corner) = doc.child(anchor, corner_name) else { continue };
            let ((c, co), (r, ro)) = (cell(&xs, x), cell(&ys, y));
            for (n, v) in [("col", c as i64), ("colOff", co), ("row", r as i64), ("rowOff", ro)] {
                if let Some(e) = doc.child(corner, n) {
                    doc.set_text(e, &v.to_string());
                }
            }
        }
        if kind == "oneCellAnchor"
            && let Some(e) = doc.child(anchor, "ext")
        {
            doc.set_attr(e, "cx", &frame.width.to_string());
            doc.set_attr(e, "cy", &frame.height.to_string());
        }
    }
    Ok(())
}

/// Slicers deleted in the app: each one's anchor leaves the drawing and its
/// `x14:slicer` the slicers part (the part goes with its last slicer, with
/// its relationship and the sheet's `slicerList`); a cache no slicer uses
/// any more goes the same way from the workbook.
pub(super) fn write_slicer_removals(package: &mut Package, sheet_part: &str, workbook_part: &str, removed: &[Slicer], remaining: &[Slicer]) -> Result<(), Error> {
    if removed.is_empty() {
        return Ok(());
    }
    // Their anchors in the sheet's drawing.
    if let Some(drawing) = package.relationships(sheet_part)?.iter().find(|r| r.kind.ends_with("/drawing")).and_then(|r| r.target_part(sheet_part)) {
        let doc = package.xml_mut(&drawing)?;
        if let Some(root) = doc.root_element() {
            for s in removed {
                let node = doc.descendants(root).into_iter().find(|&n| doc.local_name(n) == "slicer" && doc.attr(n, "name") == Some(s.name.as_str()));
                let mut anchor = node;
                while let Some(a) = anchor {
                    if matches!(doc.local_name(a), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
                        break;
                    }
                    anchor = doc.parent(a);
                }
                if let Some(a) = anchor {
                    // The `mc:AlternateContent` Excel wraps it in goes too.
                    let target = doc.parent(a).filter(|&p| doc.local_name(p) == "Choice").and_then(|c| doc.parent(c)).filter(|&ac| doc.local_name(ac) == "AlternateContent").unwrap_or(a);
                    doc.detach(target);
                }
            }
        }
    }
    // Their entries in the slicers parts.
    for rel in package.relationships(sheet_part)?.into_iter().filter(|r| r.kind.ends_with("/slicer")) {
        let Some(part) = rel.target_part(sheet_part) else { continue };
        let doc = package.xml_mut(&part)?;
        let Some(root) = doc.root_element() else { continue };
        for n in doc.children_named(root, "slicer").collect::<Vec<_>>() {
            if doc.attr(n, "name").is_some_and(|name| removed.iter().any(|s| s.name == name)) {
                doc.detach(n);
            }
        }
        if doc.children_named(root, "slicer").next().is_some() {
            continue;
        }
        package.remove_relationship(sheet_part, &rel.id)?;
        package.remove_override(&part)?;
        package.remove(&part);
        let sheet = package.xml_mut(sheet_part)?;
        let Some(sheet_root) = sheet.root_element() else { continue };
        for n in sheet.descendants(sheet_root) {
            if sheet.local_name(n) == "slicer" && sheet.attr_local(n, "id") == Some(rel.id.as_str()) {
                sheet.detach(n);
            }
        }
        for list in sheet.descendants(sheet_root).into_iter().filter(|&n| sheet.local_name(n) == "slicerList").collect::<Vec<_>>() {
            if sheet.children(list).iter().all(|&c| !sheet.is_element(c)) {
                // The `ext`, and the `extLst` when it was the only one.
                let ext = sheet.parent(list).filter(|&e| sheet.local_name(e) == "ext");
                let ext_lst = ext.and_then(|e| sheet.parent(e));
                if let Some(e) = ext {
                    sheet.detach(e);
                }
                if let Some(l) = ext_lst.filter(|&l| sheet.local_name(l) == "extLst" && sheet.elements(l).next().is_none()) {
                    sheet.detach(l);
                }
            }
        }
    }
    // The caches nothing uses now.
    let unused: Vec<&str> = removed.iter().map(|s| s.cache.as_str()).filter(|c| !remaining.iter().any(|s| s.cache == *c)).collect();
    if unused.is_empty() {
        return Ok(());
    }
    for rel in package.relationships(workbook_part)?.into_iter().filter(|r| r.kind.ends_with("/slicerCache")) {
        let Some(part) = rel.target_part(workbook_part) else { continue };
        let name = package.xml(&part).ok().and_then(|d| d.root_element().and_then(|r| d.attr(r, "name").map(str::to_string)));
        if !name.as_deref().is_some_and(|n| unused.contains(&n)) {
            continue;
        }
        package.remove_relationship(workbook_part, &rel.id)?;
        package.remove_override(&part)?;
        package.remove(&part);
        let wb = package.xml_mut(workbook_part)?;
        let Some(wb_root) = wb.root_element() else { continue };
        // The name Excel makes for the slicer (`スライサー_商品`, `Slicer_Product`).
        if let Some(name) = name.as_deref() {
            for list in wb.child(wb_root, "definedNames").into_iter().collect::<Vec<_>>() {
                for n in wb.children_named(list, "definedName").collect::<Vec<_>>() {
                    if wb.attr(n, "name") == Some(name) {
                        wb.detach(n);
                    }
                }
                if wb.children_named(list, "definedName").next().is_none() {
                    wb.detach(list);
                }
            }
        }
        for n in wb.descendants(wb_root) {
            if wb.local_name(n) == "slicerCache" && wb.attr_local(n, "id") == Some(rel.id.as_str()) {
                wb.detach(n);
            }
        }
        for list in wb.descendants(wb_root).into_iter().filter(|&n| wb.local_name(n) == "slicerCaches").collect::<Vec<_>>() {
            if wb.elements(list).next().is_none() {
                let ext = wb.parent(list).filter(|&e| wb.local_name(e) == "ext");
                let ext_lst = ext.and_then(|e| wb.parent(e));
                if let Some(e) = ext {
                    wb.detach(e);
                }
                if let Some(l) = ext_lst.filter(|&l| wb.local_name(l) == "extLst" && wb.elements(l).next().is_none()) {
                    wb.detach(l);
                }
            }
        }
    }
    Ok(())
}
