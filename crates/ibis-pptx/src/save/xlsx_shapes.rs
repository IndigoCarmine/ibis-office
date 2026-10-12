// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Saving shapes added to or removed from a sheet: a text box, a shape, a
//! line or a picture added gets an anchor (`xdr:twoCellAnchor`, a picture's
//! `oneCell`) in the sheet's drawing, which is made when the sheet has none;
//! one removed (or a chart) loses its anchor.

use std::collections::{HashMap, HashSet};

use ibis_doc_model::{MediaFormat, MediaId, Presentation, Shape, ShapeId, ShapeKind};
use ibis_ooxml::{NodeId, Package};

use super::{SourceMap, create_drawing, sheet_of, unsupported};
use crate::Error;
use super::super::pptx_new::{Prefixes, shape_xml};

const REL_DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const ANCHORS: [&str; 3] = ["twoCellAnchor", "oneCellAnchor", "absoluteAnchor"];

fn addable(shape: &Shape) -> bool {
    matches!(shape.kind, ShapeKind::TextBox(_) | ShapeKind::Auto(_) | ShapeKind::Line(_) | ShapeKind::Picture(_))
}

/// The ids of the shapes added and removed (their changes are written here,
/// not compared with the file's); charts moved to another sheet (`moved`)
/// are written apart.
pub(super) fn write_shape_edits(package: &mut Package, map: &SourceMap, baseline: &Presentation, current: &Presentation, moved: &HashSet<ShapeId>, new_charts: &HashMap<ShapeId, super::super::NewChartEntry>, chart_data: &super::super::pptx_chart_data::ChartDataEdits) -> Result<HashSet<ShapeId>, Error> {
    let mut handled = HashSet::new();
    let Some(workbook) = current.workbook.as_deref() else { return Ok(handled) };
    let mut media: HashMap<MediaId, String> = HashMap::new();
    for new_slide in &current.slides {
        let Some(new_sheet) = sheet_of(new_slide) else { continue };
        let Some(old_slide) = baseline.slide(new_slide.id) else { continue };
        let is_floating = |s: &Shape| !matches!(s.kind, ShapeKind::Sheet(_)) && !moved.contains(&s.id);
        let added: Vec<&Shape> = new_slide.shapes.iter().filter(|s| is_floating(s) && !old_slide.shapes.iter().any(|o| o.id == s.id)).collect();
        let removed: Vec<&Shape> = old_slide.shapes.iter().filter(|s| is_floating(s) && !new_slide.shapes.iter().any(|n| n.id == s.id)).collect();
        if added.is_empty() && removed.is_empty() {
            continue;
        }
        let index = workbook.sheets.iter().position(|s| s.slide == Some(new_slide.id)).ok_or_else(|| unsupported("このシートの変更"))?;
        let sheet_part = map.sheet_parts.get(index).cloned().flatten().ok_or_else(|| unsupported("このシートの変更"))?;
        for shape in &removed {
            match &shape.kind {
                // Slicers are removed with their parts, apart.
                ShapeKind::Slicer(_) => {}
                ShapeKind::Chart(_) => {
                    let source = map.charts.iter().find(|s| s.shape == shape.id).ok_or_else(|| unsupported("このグラフの削除"))?;
                    remove_anchor(package, &source.drawing, source.frame_id.as_deref().unwrap_or(""), "graphicFrame")?;
                    remove_chart_parts(package, &source.drawing, &source.chart)?;
                    handled.insert(shape.id);
                }
                _ => {
                    let source = map.shapes.iter().find(|s| s.shape == shape.id).ok_or_else(|| unsupported("この図形の削除"))?;
                    remove_anchor(package, &source.drawing, &source.c_nv_pr_id, "")?;
                    handled.insert(shape.id);
                }
            }
        }
        for shape in &added {
            if matches!(shape.kind, ShapeKind::Slicer(_)) {
                return Err(unsupported("スライサーの追加"));
            }
            if !addable(shape) && !matches!(shape.kind, ShapeKind::Chart(_)) {
                return Err(unsupported("この種類の図形（グラフなど）の追加"));
            }
            let drawing = match package.relationships(&sheet_part)?.into_iter().find(|r| r.kind == REL_DRAWING && !r.external) {
                Some(rel) => rel.target_part(&sheet_part).ok_or_else(|| unsupported("シートの描画パート"))?,
                None => create_drawing(package, &sheet_part)?,
            };
            add_shape(package, &drawing, new_sheet, shape, current, &mut media, new_charts, chart_data)?;
            handled.insert(shape.id);
        }
    }
    Ok(handled)
}

/// The anchor holding the element with `cNvPr` id `id` (of local name
/// `kind` if given) detached.
fn remove_anchor(package: &mut Package, drawing: &str, id: &str, kind: &str) -> Result<(), Error> {
    let doc = package.xml_mut(drawing)?;
    let root = doc.root_element().ok_or_else(|| Error::new("描画の XML が空です"))?;
    let c_nv_pr = doc.descendants(root).into_iter().find(|&n| doc.local_name(n) == "cNvPr" && doc.attr(n, "id") == Some(id) && (kind.is_empty() || doc.parent(n).and_then(|p| doc.parent(p)).is_some_and(|f| doc.local_name(f) == kind)));
    let mut at = c_nv_pr;
    while let Some(n) = at {
        if ANCHORS.contains(&doc.local_name(n)) {
            doc.detach(n);
            return Ok(());
        }
        at = doc.parent(n);
    }
    Err(unsupported("削除する図形の位置"))
}

/// A removed chart's part, and the parts only it related to (its workbook,
/// style and colors), gone, with the drawing's relationship to it.
fn remove_chart_parts(package: &mut Package, drawing: &str, chart: &str) -> Result<(), Error> {
    let others = package.names().filter(|n| *n != drawing && n.ends_with(".rels")).map(str::to_string).collect::<Vec<_>>();
    let used_elsewhere = |package: &Package, part: &str, except: &str| -> bool {
        others.iter().filter(|r| r.as_str() != except).any(|rels| {
            let source = rels.replace("_rels/", "").trim_end_matches(".rels").to_string();
            package.relationships(&source).map(|v| v.iter().any(|r| r.target_part(&source).as_deref() == Some(part))).unwrap_or(false)
        })
    };
    let chart_rels = ibis_ooxml::package::relationships_part(chart);
    if used_elsewhere(package, chart, &ibis_ooxml::package::relationships_part(drawing)) {
        return Ok(());
    }
    for rel in package.relationships(drawing)?.into_iter().filter(|r| !r.external && r.target_part(drawing).as_deref() == Some(chart)) {
        package.remove_relationship(drawing, &rel.id)?;
    }
    for rel in package.relationships(chart)?.into_iter().filter(|r| !r.external) {
        if let Some(part) = rel.target_part(chart)
            && !used_elsewhere(package, &part, &chart_rels)
        {
            package.remove_override(&part)?;
            package.remove(&part);
        }
    }
    package.remove(&chart_rels);
    package.remove_override(chart)?;
    package.remove(chart);
    Ok(())
}

fn add_shape(package: &mut Package, drawing: &str, sheet: &ibis_doc_model::Sheet, shape: &Shape, current: &Presentation, media: &mut HashMap<MediaId, String>, new_charts: &HashMap<ShapeId, super::super::NewChartEntry>, chart_data: &super::super::pptx_chart_data::ChartDataEdits) -> Result<(), Error> {
    // The relationship of a picture's image, or of a chart's part.
    let rel = match &shape.kind {
        ShapeKind::Chart(after) => {
            let entry = new_charts.get(&shape.id).ok_or_else(|| unsupported("グラフの複製・貼り付け"))?;
            let (rel, part) = super::super::pptx_new_chart::add_sheet_chart_part(package, drawing, entry.kind)?;
            if let Some(template) = &entry.template {
                // Its series from the cells, then the rest of what was set since.
                let refs = chart_data.get(&shape.id).and_then(|e| e.iter().rev().find(|(c, _)| c.groups == after.groups)).map(|(_, d)| d.series.clone()).unwrap_or_else(|| entry.refs.clone());
                super::super::pptx_chart_data::write_sheet_chart_series(package, &part, after, &refs)?;
                let base = ibis_doc_model::Chart { groups: after.groups.clone(), ..template.clone() };
                if base != **after {
                    super::write_chart_settings(package, &part, &base, after)?;
                }
            }
            Some(rel)
        }
        ShapeKind::Picture(picture) => {
            let part = match media.get(&picture.media) {
                Some(p) => p.clone(),
                None => {
                    let part = write_image(package, current, picture.media)?;
                    media.insert(picture.media, part.clone());
                    part
                }
            };
            let target = super::super::pptx::relative(drawing, &part);
            Some(package.add_relationship(drawing, REL_IMAGE, &target, false)?)
        }
        _ => None,
    };
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let cell = |offsets: &[i64], v: i64| -> (usize, i64) {
        let i = offsets.partition_point(|&o| o <= v).saturating_sub(1).min(offsets.len().saturating_sub(2));
        (i, v - offsets.get(i).copied().unwrap_or(0))
    };
    let frame = shape.frame;
    let doc = package.xml_mut(drawing)?;
    let root = doc.root_element().ok_or_else(|| Error::new("描画の XML が空です"))?;
    let id = 1 + doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "cNvPr").filter_map(|n| doc.attr(n, "id")?.parse::<u32>().ok()).max().unwrap_or(0);
    let x = Prefixes::drawing(doc, root);
    let xdr = x.p.clone();
    let body = match &shape.kind {
        ShapeKind::Chart(_) => super::super::pptx_new_chart::frame_xml(&x.p, &x.a, &x.r, id, &format!("グラフ {}", id - 1), frame, rel.as_deref().unwrap_or("")),
        _ => shape_xml(&x, shape, id, rel.as_deref())?,
    }
    .replace(&format!("<{xdr}nvPr/>"), "");
    let ((c0, co0), (r0, ro0)) = (cell(&xs, frame.x), cell(&ys, frame.y));
    let ((c1, co1), (r1, ro1)) = (cell(&xs, frame.x + frame.width), cell(&ys, frame.y + frame.height));
    let corner = |name: &str, c: usize, co: i64, r: usize, ro: i64| format!("<{xdr}{name}><{xdr}col>{c}</{xdr}col><{xdr}colOff>{co}</{xdr}colOff><{xdr}row>{r}</{xdr}row><{xdr}rowOff>{ro}</{xdr}rowOff></{xdr}{name}>");
    let anchor = if matches!(shape.kind, ShapeKind::Picture(_)) {
        format!("<{xdr}oneCellAnchor>{}<{xdr}ext cx=\"{}\" cy=\"{}\"/>{body}<{xdr}clientData/></{xdr}oneCellAnchor>", corner("from", c0, co0, r0, ro0), frame.width, frame.height)
    } else {
        format!("<{xdr}twoCellAnchor>{}{}{body}<{xdr}clientData/></{xdr}twoCellAnchor>", corner("from", c0, co0, r0, ro0), corner("to", c1, co1, r1, ro1))
    };
    let node: NodeId = x.import(doc, &anchor)?;
    let count = doc.children(root).len();
    doc.insert(root, count, node);
    Ok(())
}

/// The image of media `id` written as `xl/media/imageN.ext`.
fn write_image(package: &mut Package, current: &Presentation, id: MediaId) -> Result<String, Error> {
    let m = current.media(id).ok_or_else(|| Error::new("画像のデータがありません"))?;
    let (ext, content_type) = match m.format {
        MediaFormat::Png => ("png", "image/png"),
        MediaFormat::Jpeg => ("jpeg", "image/jpeg"),
        MediaFormat::Gif => ("gif", "image/gif"),
        MediaFormat::Bmp => ("bmp", "image/bmp"),
        MediaFormat::Tiff => ("tiff", "image/tiff"),
        MediaFormat::Emf => ("emf", "image/x-emf"),
        MediaFormat::Wmf => ("wmf", "image/x-wmf"),
        _ => return Err(unsupported("この形式の画像の追加")),
    };
    let part = super::super::pptx::next_part_name(package, &format!("xl/media/image1.{ext}"));
    package.set_bytes(&part, m.data.clone());
    if package.content_type(&part)?.is_none() {
        super::super::pptx_shapes::add_default_content_type(package, ext, content_type)?;
    }
    Ok(part)
}
