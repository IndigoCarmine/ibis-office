// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The data of a chartEx chart (`cx:chartSpace`: waterfall, histogram, Pareto,
//! funnel…) edited in its workbook: the references (`cx:f`) its data take
//! their names, categories and values from, read from the part, and the
//! caches (`cx:lvl`) rewritten from the workbook. Other changes to such a
//! chart are not written yet.

use ibis_doc_model::Presentation;
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::pptx_chart::REL_PACKAGE;
use super::pptx_chart_data::{cells, number, shown, ChartData, SeriesRefs};
use crate::Error;
use crate::save::unsupported;

const CX_NS: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";

/// Whether `part` is a chartEx part.
pub(crate) fn is_chart_ex(package: &Package, part: &str) -> bool {
    package.xml(part).ok().is_some_and(|d| d.root_element().is_some_and(|r| d.namespace(r) == Some(CX_NS)))
}

fn data_node(doc: &XmlDocument) -> Option<NodeId> {
    let root = doc.root_element()?;
    doc.child(root, "chartData").and_then(|d| doc.child(d, "data"))
}

fn formula(doc: &XmlDocument, node: Option<NodeId>) -> Option<String> {
    node.and_then(|n| doc.child(n, "f")).map(|f| doc.text(f)).filter(|f| !f.is_empty())
}

/// The references of the chart's data: one series (the first): its name, categories, values.
pub(crate) fn chart_refs(doc: &XmlDocument) -> Vec<SeriesRefs> {
    let Some(data) = data_node(doc) else { return Vec::new() };
    let by_type = |tag: &str, kind: &str| doc.children_named(data, tag).find(|&d| doc.attr(d, "type") == Some(kind));
    let name = doc.root_element().and_then(|r| doc.path(r, &["chart", "plotArea", "plotAreaRegion"])).and_then(|region| doc.children_named(region, "series").next()).and_then(|s| doc.path(s, &["tx", "txData"])).and_then(|t| formula(doc, Some(t)));
    let categories = formula(doc, by_type("strDim", "cat"));
    let values = formula(doc, by_type("numDim", "val"));
    if values.is_none() {
        return Vec::new();
    }
    vec![SeriesRefs { name, categories, values, sizes: None }]
}

/// The chart part's workbook (embedded) and references.
pub(crate) fn read_chart_data(package: &Package, chart_part: &str) -> Result<ChartData, Error> {
    let refs = chart_refs(&package.xml(chart_part)?);
    let book = package.relationships(chart_part)?.into_iter().find(|r| r.kind == REL_PACKAGE && !r.external).and_then(|r| r.target_part(chart_part)).filter(|p| package.contains(p));
    match book {
        Some(book) if !refs.is_empty() => Ok(ChartData { workbook: package.read(&book)?.to_vec(), series: refs }),
        _ => Err(unsupported("ブックのないグラフのデータの編集")),
    }
}

fn prefix_of(doc: &XmlDocument, node: NodeId) -> String {
    doc.name(node).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default()
}

/// The caches of the data of `chart_part` rewritten from `data`'s workbook (by the references
/// of its first series), and the workbook put in the file.
pub(crate) fn write_chart_data(package: &mut Package, chart_part: &str, data: &ChartData) -> Result<(), Error> {
    let opened = crate::open(data.workbook.clone()).map_err(|e| Error::new(format!("グラフのデータのブックを開けません: {e}")))?;
    let workbook: Presentation = opened.presentation;
    let refs = data.series.first().ok_or_else(|| unsupported("グラフのデータの範囲"))?;
    let doc = package.xml_mut(chart_part)?;
    let node = data_node(doc).ok_or_else(|| unsupported("このグラフのデータの変更"))?;
    for dim in doc.elements(node).filter(|&d| matches!(doc.local_name(d), "strDim" | "numDim")).collect::<Vec<_>>() {
        let (numeric, kind) = (doc.local_name(dim) == "numDim", doc.attr(dim, "type").unwrap_or("").to_string());
        let reference = match (kind.as_str(), numeric) {
            ("cat", false) => &refs.categories,
            ("val", true) => &refs.values,
            _ => continue,
        };
        let Some(f) = reference else { continue };
        let list = cells(&workbook, f).ok_or_else(|| unsupported("グラフのデータの範囲"))?;
        let p = prefix_of(doc, dim);
        let format = doc.child(dim, "lvl").and_then(|l| doc.attr(l, "formatCode")).map(str::to_string);
        for old in doc.children(dim).to_vec() {
            if doc.is_element(old) && matches!(doc.local_name(old), "f" | "lvl") {
                doc.detach(old);
            }
        }
        let fnode = doc.insert_element(dim, 0, &format!("{p}f"));
        doc.set_text(fnode, f);
        let end = doc.children(dim).len();
        let lvl = doc.insert_element(dim, end, &format!("{p}lvl"));
        doc.set_attr(lvl, "ptCount", &list.len().to_string());
        if numeric {
            doc.set_attr(lvl, "formatCode", format.as_deref().unwrap_or("General"));
        }
        for (i, cell) in list.iter().enumerate() {
            let text = if numeric { number(*cell).map(super::pptx_chart::number_text) } else { Some(shown(*cell)).filter(|t| !t.is_empty()) };
            if let Some(text) = text {
                let pt = doc.append_element(lvl, &format!("{p}pt"));
                doc.set_attr(pt, "idx", &i.to_string());
                doc.set_text(pt, &text);
            }
        }
    }
    // The series' name.
    let txdata = {
        let region = doc.root_element().and_then(|r| doc.path(r, &["chart", "plotArea", "plotAreaRegion"]));
        let first = region.and_then(|region| doc.children_named(region, "series").next());
        first.and_then(|s| doc.path(s, &["tx", "txData"]))
    };
    if let (Some(f), Some(txdata)) = (&refs.name, txdata) {
        let name = cells(&workbook, f).map(|c| c.into_iter().map(shown).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ")).unwrap_or_default();
        let p = prefix_of(doc, txdata);
        for old in doc.children(txdata).to_vec() {
            doc.detach(old);
        }
        let fnode = doc.append_element(txdata, &format!("{p}f"));
        doc.set_text(fnode, f);
        let v = doc.append_element(txdata, &format!("{p}v"));
        doc.set_text(v, &name);
    }
    let book = package.relationships(chart_part)?.into_iter().find(|r| r.kind == REL_PACKAGE && !r.external).and_then(|r| r.target_part(chart_part)).ok_or_else(|| unsupported("ブックのないグラフのデータの編集"))?;
    package.set_bytes(&book, data.workbook.clone());
    Ok(())
}
