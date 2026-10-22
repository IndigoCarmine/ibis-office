// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The new chart kinds (`cx:chartSpace`, "chartEx": waterfall, histogram,
//! Pareto, …) read as [`Chart`].
//!
//! A kind that Excel can also draw with the old chart parts is turned into
//! the equivalent `c:chartSpace` (a waterfall is a stacked column chart whose
//! first series is invisible) and read by the ordinary chart reader, so the
//! axes, the legend, the title and the labels are drawn by the one code. The
//! package keeps the original part; this only builds the model to draw.

use ibis_doc_model::Chart;
use roxmltree::Node;

use crate::chart::{points_by_index, MAX_POINTS};
use crate::theme::Colors;
use crate::xml::{attr, child, children, elements};

pub(crate) const CX_NS: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";

/// Whether `space` is a `cx:chartSpace`.
pub(crate) fn is_chart_ex(space: Node) -> bool {
    space.tag_name().namespace() == Some(CX_NS)
}

/// One data dimension (`cx:strDim` / `cx:numDim`): its points in order.
struct Dim {
    kind: String,
    format: Option<String>,
    points: Vec<Option<String>>,
    /// Every level of a hierarchy (`cx:lvl`, the finest first), by point index.
    levels: Vec<Vec<Option<String>>>,
}

fn dims(data: Node) -> Vec<Dim> {
    elements(data)
        .filter(|n| matches!(n.tag_name().name(), "strDim" | "numDim"))
        .map(|d| {
            let kind = attr(d, "type").unwrap_or("").to_string();
            let read_level = |lvl: Node| {
                let count = attr(lvl, "ptCount").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                let pts = children(lvl, "pt").map(|pt| (attr(pt, "idx").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0), pt.text().map(str::to_string)));
                points_by_index(count, MAX_POINTS, pts)
            };
            let levels: Vec<Vec<Option<String>>> = children(d, "lvl").map(read_level).collect();
            let lvl = child(d, "lvl");
            Dim { kind, format: lvl.and_then(|l| attr(l, "formatCode")).map(str::to_string), points: levels.first().cloned().unwrap_or_default(), levels }
        })
        .collect()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The children of `node`, as the source writes them.
fn inner(node: Node) -> String {
    let text = node.document().input_text();
    elements(node).map(|n| &text[n.range()]).collect()
}

fn is_japanese(texts: &[&str]) -> bool {
    texts.iter().any(|t| t.chars().any(|c| c as u32 >= 0x3040))
}

/// Read `space` (`cx:chartSpace`) as a chart; `None` for a kind not drawn yet.
pub(crate) fn read(space: Node, colors: &Colors, legacy_border: bool) -> Option<Chart> {
    read_ex(space, colors, legacy_border)
}

fn read_ex(space: Node, colors: &Colors, legacy_border: bool) -> Option<Chart> {
    let chart = child(space, "chart")?;
    let plot = child(chart, "plotArea")?;
    let region = child(plot, "plotAreaRegion")?;
    let series = child(region, "series")?;
    let data_id = child(series, "dataId").and_then(|d| attr(d, "val")).unwrap_or("0");
    let data = child(space, "chartData")
        .and_then(|d| children(d, "data").find(|n| attr(*n, "id") == Some(data_id)).or_else(|| child(d, "data")))?;
    let dims = dims(data);
    // A Pareto chart is columns with a `paretoLine` series beside them.
    let layout = if children(region, "series").any(|s| attr(s, "layoutId") == Some("paretoLine")) { "pareto" } else { attr(series, "layoutId")? };
    let stagger = matches!(layout, "clusteredColumn" | "pareto");
    let mut ex = None;
    let xml = match layout {
        "boxWhisker" => {
            let (xml, boxes, flags) = box_whisker(chart, plot, series, &dims)?;
            ex = Some(ibis_doc_model::ChartEx::Box { boxes, fill: ibis_doc_model::Fill::Solid(colors.named("accent1")), mean_marker: flags.0, mean_line: flags.1, outliers: flags.2 });
            xml
        }
        "treemap" | "sunburst" => {
            let tree = tree_nodes(&dims, colors, layout == "sunburst");
            ex = Some(if layout == "treemap" { ibis_doc_model::ChartEx::Treemap(tree) } else { ibis_doc_model::ChartEx::Sunburst(tree) });
            tree_dummy(chart)
        }
        _ => String::new(),
    };
    if ex.is_some() {
        let doc = roxmltree::Document::parse(&xml).ok()?;
        let mut chart = crate::chart::read_in(doc.root_element(), colors, legacy_border)?;
        chart.ex = ex;
        return Some(chart);
    }
    let xml = match layout {
        "waterfall" => waterfall(space, chart, plot, series, &dims)?,
        "clusteredColumn" => histogram(chart, plot, series, &dims)?,
        "pareto" => pareto(chart, plot, series, &dims)?,
        "funnel" => funnel(chart, plot, series, &dims)?,
        _ => return None,
    };
    let doc = roxmltree::Document::parse(&xml).ok()?;
    let mut chart = crate::chart::read_in(doc.root_element(), colors, legacy_border)?;
    if stagger {
        for axis in chart.axes.iter_mut().filter(|a| a.kind == ibis_doc_model::AxisKind::Category) {
            axis.stagger_labels = true;
        }
    }
    // Read as an old kind of chart, but a new one's (drawn with its padding).
    chart.ex = Some(ibis_doc_model::ChartEx::Synthesized);
    Some(chart)
}

/// The `c:title` of a `cx:title`.
fn title_xml(chart: Node) -> String {
    let Some(title) = child(chart, "title") else { return "<c:autoTitleDeleted val=\"1\"/>".into() };
    let text = child(title, "tx").and_then(|tx| {
        child(tx, "txData").and_then(|d| child(d, "v")).and_then(|v| v.text().map(escape)).map(|t| format!("<c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>{t}</a:t></a:r></a:p></c:rich></c:tx>")).or_else(|| {
            child(tx, "rich").map(|r| {
                let text = inner(r);
                format!("<c:tx><c:rich>{text}</c:rich></c:tx>")
            })
        })
    });
    let tx_pr = child(title, "txPr").map(|t| format!("<c:txPr>{}</c:txPr>", inner(t))).unwrap_or_default();
    match text {
        Some(text) => format!("<c:title>{text}{tx_pr}<c:overlay val=\"0\"/></c:title><c:autoTitleDeleted val=\"0\"/>"),
        None => format!("<c:title>{tx_pr}<c:overlay val=\"0\"/></c:title><c:autoTitleDeleted val=\"0\"/>"),
    }
}

fn legend_xml(chart: Node, hidden: &[usize]) -> String {
    let Some(legend) = child(chart, "legend") else { return String::new() };
    let pos = match attr(legend, "pos").unwrap_or("r") {
        p @ ("t" | "b" | "l" | "r") => p,
        _ => "r",
    };
    let entries: String = hidden.iter().map(|i| format!("<c:legendEntry><c:idx val=\"{i}\"/><c:delete val=\"1\"/></c:legendEntry>")).collect();
    let tx_pr = child(legend, "txPr").map(|t| format!("<c:txPr>{}</c:txPr>", inner(t))).unwrap_or_default();
    format!("<c:legend><c:legendPos val=\"{pos}\"/>{entries}<c:overlay val=\"0\"/>{tx_pr}</c:legend>")
}

/// The `c:catAx` and `c:valAx` for the axes of a `cx:plotArea`.
fn axes_xml(plot: Node, value_format: &str) -> String {
    let (mut cat, mut val) = (None, None);
    for axis in children(plot, "axis") {
        if child(axis, "catScaling").is_some() {
            cat = cat.or(Some(axis));
        } else if child(axis, "valScaling").is_some() {
            // The first (a Pareto chart's second is its share axis).
            val = val.or(Some(axis));
        }
    }
    let line = "<c:spPr><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"D9D9D9\"/></a:solidFill></a:ln></c:spPr>";
    let no_line = "<c:spPr><a:ln><a:noFill/></a:ln></c:spPr>";
    let hidden = |axis: Option<Node>| axis.is_some_and(|a| attr(a, "hidden").is_some_and(|v| v == "1" || v == "true"));
    let title = |axis: Option<Node>| {
        axis.and_then(|a| child(a, "title"))
            .and_then(|t| child(t, "tx"))
            .and_then(|tx| child(tx, "txData").and_then(|d| child(d, "v")).and_then(|v| v.text().map(escape)))
            .map(|t| format!("<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>{t}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>"))
            .unwrap_or_default()
    };
    let tx_pr = |axis: Option<Node>| axis.and_then(|a| child(a, "txPr")).map(|t| format!("<c:txPr>{}</c:txPr>", inner(t))).unwrap_or_default();
    let labels = |axis: Option<Node>| if axis.is_some_and(|a| child(a, "tickLabels").is_some()) { "nextTo" } else { "none" };
    let mut scaling = String::new();
    let mut major_unit = String::new();
    if let Some(vs) = val.and_then(|a| child(a, "valScaling")) {
        if let Some(max) = attr(vs, "max").filter(|v| v.parse::<f64>().is_ok()) {
            scaling.push_str(&format!("<c:max val=\"{max}\"/>"));
        }
        if let Some(min) = attr(vs, "min").filter(|v| v.parse::<f64>().is_ok()) {
            scaling.push_str(&format!("<c:min val=\"{min}\"/>"));
        }
        if let Some(unit) = attr(vs, "majorUnit").filter(|v| v.parse::<f64>().is_ok()) {
            major_unit = format!("<c:majorUnit val=\"{unit}\"/>");
        }
    }
    let format = val
        .and_then(|a| child(a, "numFmt"))
        .and_then(|f| attr(f, "formatCode"))
        .map(escape)
        .unwrap_or_else(|| escape(value_format));
    let gridlines = if val.is_some_and(|a| child(a, "majorGridlines").is_some()) {
        "<c:majorGridlines><c:spPr><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"D9D9D9\"/></a:solidFill></a:ln></c:spPr></c:majorGridlines>"
    } else {
        ""
    };
    format!(
        "<c:catAx><c:axId val=\"1\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"{cd}\"/><c:axPos val=\"b\"/>{ct}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"{cl}\"/>{line}{ctx}<c:crossAx val=\"2\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
         <c:valAx><c:axId val=\"2\"/><c:scaling><c:orientation val=\"minMax\"/>{scaling}</c:scaling><c:delete val=\"{vd}\"/><c:axPos val=\"l\"/>{gridlines}{vt}<c:numFmt formatCode=\"{format}\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"{vl}\"/>{no_line}{vtx}<c:crossAx val=\"1\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/>{major_unit}</c:valAx>",
        cd = hidden(cat) as u8,
        cl = labels(cat),
        ct = title(cat),
        ctx = tx_pr(cat),
        vd = hidden(val) as u8,
        vl = labels(val),
        vt = title(val),
        vtx = tx_pr(val),
    )
}

fn number_cache(values: &[Option<f64>], format: &str) -> String {
    let pts: String = values.iter().enumerate().filter_map(|(i, v)| v.map(|v| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>"))).collect();
    format!("<c:numCache><c:formatCode>{}</c:formatCode><c:ptCount val=\"{}\"/>{pts}</c:numCache>", escape(format), values.len())
}

fn category_cache(categories: &[String]) -> String {
    let pts: String = categories.iter().enumerate().map(|(i, c)| format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", escape(c))).collect();
    format!("<c:cat><c:strRef><c:f></c:f><c:strCache><c:ptCount val=\"{}\"/>{pts}</c:strCache></c:strRef></c:cat>", categories.len())
}

/// A series of the stacked column chart: name, fill, its values.
fn ser_xml(idx: usize, name: &str, fill: &str, values: &[Option<f64>], format: &str, categories: &[String], labels: &str) -> String {
    format!(
        "<c:ser><c:idx val=\"{idx}\"/><c:order val=\"{idx}\"/><c:tx><c:v>{}</c:v></c:tx><c:spPr>{fill}</c:spPr><c:invertIfNegative val=\"0\"/>{labels}{}<c:val><c:numRef><c:f></c:f>{}</c:numRef></c:val></c:ser>",
        escape(name),
        category_cache(categories),
        number_cache(values, format),
    )
}

fn waterfall(space: Node, chart: Node, plot: Node, series: Node, dims: &[Dim]) -> Option<String> {
    let cat = dims.iter().find(|d| d.kind == "cat");
    let val = dims.iter().find(|d| d.kind == "val")?;
    let n = val.points.len().max(cat.map_or(0, |c| c.points.len()));
    let categories: Vec<String> = (0..n).map(|i| cat.and_then(|c| c.points.get(i).cloned().flatten()).unwrap_or_default()).collect();
    let numbers: Vec<Option<f64>> = (0..n).map(|i| val.points.get(i).cloned().flatten().and_then(|v| v.trim().parse().ok())).collect();
    let format = val.format.as_deref().unwrap_or("General");
    let subtotals: Vec<usize> = child(series, "layoutPr")
        .and_then(|l| child(l, "subtotals"))
        .map(|s| children(s, "idx").filter_map(|i| attr(i, "val").and_then(|v| v.parse().ok())).collect())
        .unwrap_or_default();

    // Each bar from where the running total stood to where it ends; a bar
    // across zero is a positive and a negative part.
    let mut base = vec![None; n];
    let mut up = vec![None; n];
    let mut down = vec![None; n];
    let mut total = vec![None; n];
    let mut up_neg = vec![None; n];
    let mut down_neg = vec![None; n];
    let mut running = 0.0f64;
    // Where the running total stands after each point (the connector lines).
    let mut levels: Vec<Option<f64>> = vec![None; n];
    // The series holding the visible bar of each point (0 up, 1 down, 2 total).
    let mut owner = vec![None; n];
    for i in 0..n {
        let Some(v) = numbers[i] else { continue };
        if subtotals.contains(&i) {
            total[i] = Some(v);
            owner[i] = Some(2);
            running = v;
            levels[i] = Some(running);
            continue;
        }
        owner[i] = Some(if v >= 0.0 { 0 } else { 1 });
        let (start, end) = (running, running + v);
        running = end;
        levels[i] = Some(running);
        let (lo, hi) = (start.min(end), start.max(end));
        let (pos, neg) = if v >= 0.0 { (&mut up, &mut up_neg) } else { (&mut down, &mut down_neg) };
        if lo >= 0.0 {
            base[i] = Some(lo);
            pos[i] = Some(hi - lo);
        } else if hi <= 0.0 {
            base[i] = Some(hi);
            pos[i] = Some(lo - hi);
        } else {
            pos[i] = Some(hi);
            neg[i] = Some(lo);
        }
    }

    let japanese = is_japanese(&categories.iter().map(String::as_str).collect::<Vec<_>>());
    let (up_name, down_name, total_name) = if japanese { ("増加", "減少", "合計") } else { ("Increase", "Decrease", "Total") };
    let scheme = |name: &str| format!("<a:solidFill><a:schemeClr val=\"{name}\"/></a:solidFill>");
    let none = "<a:noFill/>".to_string();
    let _ = space;
    // The values written on the bars: each point's own value.
    let label_node = child(series, "dataLabels");
    let label_format = label_node.and_then(|l| child(l, "numFmt")).and_then(|f| attr(f, "formatCode")).unwrap_or(format);
    let label_for = |kind: usize| -> String {
        let Some(node) = label_node else { return String::new() };
        let shows_value = child(node, "visibility").is_none_or(|v| attr(v, "value").is_none_or(|x| x == "1" || x == "true"));
        if !shows_value {
            return String::new();
        }
        // The end of a decrease is its bottom: its labels stand under it.
        let pos = match attr(node, "pos") {
            Some("inEnd") => "inEnd",
            Some("ctr") => "ctr",
            Some("inBase") => "inBase",
            _ if kind == 1 => "b",
            _ => "outEnd",
        };
        let points: String = (0..n)
            .filter(|i| owner[*i] == Some(kind))
            .map(|i| {
                let text = ibis_doc_model::format_number(numbers[i].unwrap_or(0.0), label_format);
                format!("<c:dLbl><c:idx val=\"{i}\"/><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:dLblPos val=\"{pos}\"/><c:showLegendKey val=\"0\"/><c:showVal val=\"1\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbl>", escape(&text))
            })
            .collect();
        let tx_pr = child(node, "txPr").map(|t| format!("<c:txPr>{}</c:txPr>", inner(t))).unwrap_or_default();
        format!("<c:dLbls>{points}{tx_pr}<c:dLblPos val=\"{pos}\"/><c:showLegendKey val=\"0\"/><c:showVal val=\"0\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>")
    };
    let gap = children(plot, "axis")
        .find_map(|a| child(a, "catScaling"))
        .and_then(|s| attr(s, "gapWidth"))
        .and_then(|g| g.parse::<f64>().ok())
        .map(|g| (g * 100.0).round() as i64)
        .unwrap_or(50);

    let mut sers = String::new();
    sers.push_str(&ser_xml(0, "", &none, &base, format, &categories, ""));
    sers.push_str(&ser_xml(1, up_name, &scheme("accent1"), &up, format, &categories, &label_for(0)));
    sers.push_str(&ser_xml(2, down_name, &scheme("accent2"), &down, format, &categories, &label_for(1)));
    sers.push_str(&ser_xml(3, total_name, &scheme("accent3"), &total, format, &categories, &label_for(2)));
    sers.push_str(&ser_xml(4, up_name, &scheme("accent1"), &up_neg, format, &categories, ""));
    sers.push_str(&ser_xml(5, down_name, &scheme("accent2"), &down_neg, format, &categories, ""));

    // Connector lines: a level line from the end of each bar to the next bar,
    // drawn as two-point line series on the same axes (a line runs from the
    // middle of one column to the middle of the next, along the bars' edges).
    let connectors = child(series, "layoutPr").and_then(|l| child(l, "visibility")).and_then(|v| attr(v, "connectorLines")).is_none_or(|v| v == "1" || v == "true");
    let mut hidden = vec![0, 4, 5];
    let mut lines = String::new();
    if connectors {
        let points: Vec<usize> = (0..n).filter(|i| levels[*i].is_some()).collect();
        for pair in points.windows(2) {
            let level = levels[pair[0]];
            let mut values = vec![None; n];
            values[pair[0]] = level;
            values[pair[1]] = level;
            let idx = 6 + hidden.len() - 3;
            hidden.push(idx);
            lines.push_str(&format!(
                "<c:ser><c:idx val=\"{idx}\"/><c:order val=\"{idx}\"/><c:tx><c:v></c:v></c:tx><c:spPr><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"7F7F7F\"/></a:solidFill></a:ln></c:spPr><c:marker><c:symbol val=\"none\"/></c:marker>{}<c:val><c:numRef><c:f></c:f>{}</c:numRef></c:val><c:smooth val=\"0\"/></c:ser>",
                category_cache(&categories),
                number_cache(&values, format),
            ));
        }
    }
    let line_chart = if lines.is_empty() { String::new() } else { format!("<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{lines}<c:marker val=\"1\"/><c:axId val=\"1\"/><c:axId val=\"2\"/></c:lineChart>") };
    let title = title_xml(chart);
    let legend = legend_xml(chart, &hidden);
    let axes = axes_xml(plot, format);
    Some(format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
         <c:chart>{title}<c:plotArea><c:layout/><c:barChart><c:barDir val=\"col\"/><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>{sers}<c:gapWidth val=\"{gap}\"/><c:overlap val=\"100\"/><c:axId val=\"1\"/><c:axId val=\"2\"/></c:barChart>{line_chart}{axes}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/></c:chart>\
         <c:spPr><a:solidFill><a:schemeClr val=\"bg1\"/></a:solidFill><a:ln><a:noFill/></a:ln></c:spPr>\
         <c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"900\"><a:solidFill><a:srgbClr val=\"595959\"/></a:solidFill></a:defRPr></a:pPr><a:endParaRPr lang=\"en-US\"/></a:p></c:txPr></c:chartSpace>"
    ))
}

fn space_xml(title: &str, plot_body: &str, legend: &str) -> String {
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
         <c:chart>{title}<c:plotArea><c:layout/>{plot_body}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/></c:chart>\
         <c:spPr><a:solidFill><a:schemeClr val=\"bg1\"/></a:solidFill><a:ln><a:noFill/></a:ln></c:spPr>\
         <c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"900\"><a:solidFill><a:srgbClr val=\"595959\"/></a:solidFill></a:defRPr></a:pPr><a:endParaRPr lang=\"en-US\"/></a:p></c:txPr></c:chartSpace>"
    )
}

/// The series' name (`cx:tx/cx:txData/cx:v`).
fn series_name(series: Node) -> Option<String> {
    child(series, "tx").and_then(|t| child(t, "txData")).and_then(|d| child(d, "v")).and_then(|v| v.text()).map(str::to_string)
}

/// The gap between columns (`cx:catScaling gapWidth`, a fraction) in percent.
fn gap_percent(plot: Node, default: f64) -> i64 {
    children(plot, "axis")
        .find_map(|a| child(a, "catScaling"))
        .and_then(|s| attr(s, "gapWidth"))
        .and_then(|g| g.parse::<f64>().ok())
        .map_or(default, |g| g * 100.0)
        .round() as i64
}

/// A series' data labels: each point's value text (`cx:dataLabels`), or none.
fn value_labels(series: Node, texts: &[String]) -> String {
    let Some(node) = child(series, "dataLabels") else { return String::new() };
    if !child(node, "visibility").is_none_or(|v| attr(v, "value").is_none_or(|x| x == "1" || x == "true")) {
        return String::new();
    }
    let pos = match attr(node, "pos") {
        Some("inEnd") => "inEnd",
        Some("ctr") => "ctr",
        Some("inBase") => "inBase",
        _ => "outEnd",
    };
    let points: String = texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            format!("<c:dLbl><c:idx val=\"{i}\"/><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:dLblPos val=\"{pos}\"/><c:showLegendKey val=\"0\"/><c:showVal val=\"1\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbl>", escape(t))
        })
        .collect();
    let tx_pr = child(node, "txPr").map(|t| format!("<c:txPr>{}</c:txPr>", inner(t))).unwrap_or_default();
    format!("<c:dLbls>{points}{tx_pr}<c:dLblPos val=\"{pos}\"/><c:showLegendKey val=\"0\"/><c:showVal val=\"0\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>")
}

fn general(v: f64) -> String {
    ibis_doc_model::format_number(v, "General")
}

/// 頻度 (histogram): the values counted in bins (`cx:binning`: a size, a
/// count, or Scott's rule), drawn as columns with no gap between them.
fn histogram(chart: Node, plot: Node, series: Node, dims: &[Dim]) -> Option<String> {
    let val = dims.iter().find(|d| d.kind == "val")?;
    let mut values: Vec<f64> = val.points.iter().filter_map(|p| p.as_deref().and_then(|v| v.trim().parse().ok())).collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let binning = child(series, "layoutPr").and_then(|l| child(l, "binning"));
    let closed_left = binning.and_then(|b| attr(b, "intervalClosed")) == Some("l");
    let number = |name: &str| binning.and_then(|b| child(b, name)).and_then(|n| attr(n, "val")).and_then(|v| v.parse::<f64>().ok());
    let bound = |name: &str| binning.and_then(|b| attr(b, name)).and_then(|v| v.parse::<f64>().ok());
    let (underflow, overflow) = (bound("underflow"), bound("overflow"));
    let n = values.len() as f64;
    let (min, max) = (underflow.unwrap_or(values[0]), overflow.unwrap_or(values[values.len() - 1]));
    let width = if let Some(size) = number("binSize").filter(|s| *s > 0.0) {
        size
    } else if let Some(count) = number("binCount").filter(|c| *c >= 1.0) {
        (max - min) / count
    } else {
        // Scott's normal reference rule: 3.49 σ n^(−1/3).
        let mean = values.iter().sum::<f64>() / n;
        let sigma = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
        let w = 3.49 * sigma / n.cbrt();
        // Excel shows the width rounded (30.1 as 30): to a whole number from
        // 1 up, else to 2 significant digits.
        if w >= 1.0 {
            w.round()
        } else if w > 0.0 {
            let magnitude = 10f64.powf(1.0 - w.log10().floor());
            (w * magnitude).round() / magnitude
        } else {
            1.0
        }
    };
    let width = if width > 0.0 { width } else { 1.0 };
    let bins = (((max - min) / width).ceil().max(1.0) as usize).min(1000);
    let edge = |i: usize| min + width * i as f64;
    let mut counts = vec![0u32; bins];
    let (mut low, mut high) = (0u32, 0u32);
    for v in &values {
        if underflow.is_some_and(|u| *v <= u) {
            low += 1;
        } else if overflow.is_some_and(|o| *v > o) {
            high += 1;
        } else {
            // Right-closed: (edge(i), edge(i+1)], the first also holding its
            // lower edge; left-closed: [edge(i), edge(i+1)), the last also
            // holding its upper edge.
            let mut i = if closed_left { ((*v - min) / width).floor() } else { ((*v - min) / width).ceil() - 1.0 };
            i = i.clamp(0.0, (bins - 1) as f64);
            counts[i as usize] += 1;
        }
    }
    let mut labels = Vec::new();
    let mut data = Vec::new();
    if underflow.is_some() {
        labels.push(format!("\u{2264} {}", general(min)));
        data.push(low);
    }
    for i in 0..bins {
        let (a, b) = (general(edge(i)), general(edge(i + 1)));
        // The first bin holds its lower edge, unless a bin of lower values stands before it.
        let first_closed = underflow.is_none() && i == 0;
        labels.push(if closed_left {
            if i + 1 == bins && overflow.is_none() { format!("[{a}, {b}]") } else { format!("[{a}, {b})") }
        } else if first_closed {
            format!("[{a}, {b}]")
        } else {
            format!("({a}, {b}]")
        });
        data.push(counts[i]);
    }
    if overflow.is_some() {
        labels.push(format!("> {}", general(max)));
        data.push(high);
    }
    let numbers: Vec<Option<f64>> = data.iter().map(|c| Some(*c as f64)).collect();
    let texts: Vec<String> = data.iter().map(|c| c.to_string()).collect();
    let name = series_name(series).unwrap_or_else(|| if is_japanese(&labels.iter().map(String::as_str).collect::<Vec<_>>()) { "頻度".into() } else { "Frequency".into() });
    let ser = ser_xml(0, &name, "<a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill>", &numbers, "General", &labels, &value_labels(series, &texts));
    let gap = gap_percent(plot, 0.0);
    let body = format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{ser}<c:gapWidth val=\"{gap}\"/><c:axId val=\"1\"/><c:axId val=\"2\"/></c:barChart>{}", axes_xml(plot, "General"));
    Some(space_xml(&title_xml(chart), &body, &legend_xml(chart, &[])))
}

/// パレート図: the categories' totals (or counts) from the largest down as
/// columns, and the running share of the whole as a line on a second axis
/// (0 to 100 %).
fn pareto(chart: Node, plot: Node, series: Node, dims: &[Dim]) -> Option<String> {
    let cat = dims.iter().find(|d| d.kind == "cat")?;
    let val = dims.iter().find(|d| d.kind == "val");
    let n = cat.points.len().max(val.map_or(0, |v| v.points.len()));
    let mut totals: Vec<(String, f64)> = Vec::new();
    for i in 0..n {
        let name = cat.points.get(i).cloned().flatten().unwrap_or_default();
        let amount = match val {
            Some(v) => v.points.get(i).cloned().flatten().and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0),
            None => 1.0,
        };
        match totals.iter_mut().find(|(k, _)| *k == name) {
            Some((_, t)) => *t += amount,
            None => totals.push((name, amount)),
        }
    }
    totals.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = totals.iter().map(|t| t.1).sum();
    let labels: Vec<String> = totals.iter().map(|t| t.0.clone()).collect();
    let numbers: Vec<Option<f64>> = totals.iter().map(|t| Some(t.1)).collect();
    let mut running = 0.0;
    let shares: Vec<Option<f64>> = totals.iter().map(|t| { running += t.1; Some(if sum > 0.0 { running / sum } else { 0.0 }) }).collect();
    let format = val.and_then(|v| v.format.as_deref()).unwrap_or("General");
    let texts: Vec<String> = totals.iter().map(|t| ibis_doc_model::format_number(t.1, format)).collect();
    let name = series_name(series).unwrap_or_default();
    let bars = ser_xml(0, &name, "<a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill>", &numbers, format, &labels, &value_labels(series, &texts));
    let line = format!(
        "<c:ser><c:idx val=\"1\"/><c:order val=\"1\"/><c:tx><c:v></c:v></c:tx><c:spPr><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"accent2\"/></a:solidFill></a:ln></c:spPr><c:marker><c:symbol val=\"none\"/></c:marker>{}<c:val><c:numRef><c:f></c:f>{}</c:numRef></c:val><c:smooth val=\"0\"/></c:ser>",
        category_cache(&labels),
        number_cache(&shares, "0%"),
    );
    let gap = gap_percent(plot, 0.5);
    // The share axis on the right: 0 to 100 %, its category axis not shown.
    let second = "<c:valAx><c:axId val=\"4\"/><c:scaling><c:orientation val=\"minMax\"/><c:max val=\"1\"/><c:min val=\"0\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"r\"/><c:numFmt formatCode=\"0%\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:spPr><a:ln><a:noFill/></a:ln></c:spPr><c:crossAx val=\"3\"/><c:crosses val=\"max\"/><c:crossBetween val=\"between\"/></c:valAx>\
        <c:catAx><c:axId val=\"3\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"1\"/><c:axPos val=\"b\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"4\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>";
    let body = format!(
        "<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{bars}<c:gapWidth val=\"{gap}\"/><c:axId val=\"1\"/><c:axId val=\"2\"/></c:barChart>\
         <c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{line}<c:marker val=\"1\"/><c:axId val=\"3\"/><c:axId val=\"4\"/></c:lineChart>{}{second}",
        axes_xml(plot, format)
    );
    Some(space_xml(&title_xml(chart), &body, &legend_xml(chart, &[1])))
}

/// じょうごグラフ: horizontal bars, each centred, the first category on top.
fn funnel(chart: Node, plot: Node, series: Node, dims: &[Dim]) -> Option<String> {
    let cat = dims.iter().find(|d| d.kind == "cat");
    let val = dims.iter().find(|d| d.kind == "val")?;
    let n = val.points.len().max(cat.map_or(0, |c| c.points.len()));
    let labels: Vec<String> = (0..n).map(|i| cat.and_then(|c| c.points.get(i).cloned().flatten()).unwrap_or_default()).collect();
    let numbers: Vec<Option<f64>> = (0..n).map(|i| val.points.get(i).cloned().flatten().and_then(|v| v.trim().parse().ok())).collect();
    let max = numbers.iter().flatten().cloned().fold(0.0f64, f64::max);
    let spacer: Vec<Option<f64>> = numbers.iter().map(|v| v.map(|v| (max - v) / 2.0)).collect();
    let format = val.format.as_deref().unwrap_or("General");
    let texts: Vec<String> = numbers.iter().map(|v| ibis_doc_model::format_number(v.unwrap_or(0.0), format)).collect();
    let fill = "<a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill>";
    let sers = format!(
        "{}{}",
        ser_xml(0, "", "<a:noFill/>", &spacer, format, &labels, ""),
        ser_xml(1, &series_name(series).unwrap_or_default(), fill, &numbers, format, &labels, &value_labels_centered(series, &texts)),
    );
    let gap = gap_percent(plot, 0.06).max(0);
    let axes = format!("<c:catAx><c:axId val=\"1\"/><c:scaling><c:orientation val=\"maxMin\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:spPr><a:ln><a:noFill/></a:ln></c:spPr><c:crossAx val=\"2\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
        <c:valAx><c:axId val=\"2\"/><c:scaling><c:orientation val=\"minMax\"/><c:max val=\"{max}\"/><c:min val=\"0\"/></c:scaling><c:delete val=\"1\"/><c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"1\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>");
    let body = format!("<c:barChart><c:barDir val=\"bar\"/><c:grouping val=\"stacked\"/><c:varyColors val=\"0\"/>{sers}<c:gapWidth val=\"{gap}\"/><c:overlap val=\"100\"/><c:axId val=\"1\"/><c:axId val=\"2\"/></c:barChart>{axes}");
    Some(space_xml(&title_xml(chart), &body, &legend_xml(chart, &[0])))
}

/// `value_labels` at the middle of each bar (a funnel's labels sit in it, in white).
fn value_labels_centered(series: Node, texts: &[String]) -> String {
    let labels = value_labels(series, texts).replace("outEnd", "ctr");
    if labels.contains("<c:txPr>") || labels.is_empty() {
        return labels;
    }
    let white = "<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill></a:defRPr></a:pPr><a:endParaRPr lang=\"en-US\"/></a:p></c:txPr>";
    labels.replacen("<c:dLblPos val=\"ctr\"/><c:showLegendKey val=\"0\"/><c:showVal val=\"0\"/>", &format!("{white}<c:dLblPos val=\"ctr\"/><c:showLegendKey val=\"0\"/><c:showVal val=\"0\"/>"), 1)
}

/// The values of `values` at fraction `p` as QUARTILE.EXC / QUARTILE.INC
/// would (sorted ascending).
fn quartile(sorted: &[f64], p: f64, exclusive: bool) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    let position = if exclusive { p * (n as f64 + 1.0) } else { p * (n as f64 - 1.0) + 1.0 };
    let position = position.clamp(1.0, n as f64);
    let (lo, frac) = (position.floor() as usize, position.fract());
    let a = sorted[lo - 1];
    let b = sorted[(lo).min(n - 1)];
    a + (b - a) * frac
}

/// 箱ひげ図: one box for each category (`cx:strDim`) of the values, with
/// its quartiles, whiskers at the furthest value within 1.5 box heights,
/// the mean, and what lies beyond the whiskers. The chart is read as clustered
/// columns, invisible, that give the axes and the scale; the boxes are drawn
/// over them (`ChartEx::Box`).
fn box_whisker(chart: Node, plot: Node, series: Node, dims: &[Dim]) -> Option<(String, Vec<ibis_doc_model::BoxStats>, (bool, bool, bool))> {
    use ibis_doc_model::{BoxStats, Number};
    let cat = dims.iter().find(|d| d.kind == "cat");
    let val = dims.iter().find(|d| d.kind == "val")?;
    let mut groups: Vec<(String, Vec<f64>)> = Vec::new();
    for (i, p) in val.points.iter().enumerate() {
        let Some(v) = p.as_deref().and_then(|t| t.trim().parse::<f64>().ok()) else { continue };
        let name = cat.and_then(|c| c.points.get(i).cloned().flatten()).unwrap_or_default();
        match groups.iter_mut().find(|(k, _)| *k == name) {
            Some((_, list)) => list.push(v),
            None => groups.push((name, vec![v])),
        }
    }
    if groups.is_empty() {
        return None;
    }
    let layout = child(series, "layoutPr");
    let exclusive = layout.and_then(|l| child(l, "statistics")).and_then(|s| attr(s, "quartileMethod")) != Some("inclusive");
    let flag = |name: &str, default: bool| layout.and_then(|l| child(l, "visibility")).and_then(|v| attr(v, name)).map_or(default, |v| v == "1" || v == "true");
    let (mean_marker, mean_line, outliers) = (flag("meanMarker", true), flag("meanLine", false), flag("outliers", true));
    let mut boxes = Vec::new();
    for (name, mut list) in groups {
        list.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let (q1, median, q3) = (quartile(&list, 0.25, exclusive), quartile(&list, 0.5, exclusive), quartile(&list, 0.75, exclusive));
        let spread = 1.5 * (q3 - q1);
        let within: Vec<f64> = list.iter().copied().filter(|v| *v >= q1 - spread && *v <= q3 + spread).collect();
        let (low, high) = (within.first().copied().unwrap_or(q1), within.last().copied().unwrap_or(q3));
        let out: Vec<Number> = list.iter().copied().filter(|v| *v < q1 - spread || *v > q3 + spread).map(Number).collect();
        let mean = list.iter().sum::<f64>() / list.len() as f64;
        boxes.push(BoxStats { name, q1: Number(q1), median: Number(median), q3: Number(q3), whisker_low: Number(low), whisker_high: Number(high), mean: Number(mean), outliers: out });
    }
    let labels: Vec<String> = boxes.iter().map(|b| b.name.clone()).collect();
    let tops: Vec<Option<f64>> = boxes.iter().map(|b| Some(b.outliers.iter().map(|o| o.0).fold(b.whisker_high.0, f64::max))).collect();
    let bottoms: Vec<Option<f64>> = boxes.iter().map(|b| Some(b.outliers.iter().map(|o| o.0).fold(b.whisker_low.0, f64::min).min(0.0))).collect();
    let format = val.format.as_deref().unwrap_or("General");
    let none = "<a:noFill/>";
    let sers = format!("{}{}", ser_xml(0, "", none, &tops, format, &labels, ""), ser_xml(1, "", none, &bottoms, format, &labels, ""));
    let gap = gap_percent(plot, 1.0);
    let body = format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{sers}<c:gapWidth val=\"{gap}\"/><c:axId val=\"1\"/><c:axId val=\"2\"/></c:barChart>{}", axes_xml(plot, format));
    let xml = space_xml(&title_xml(chart), &body, &legend_xml(chart, &[0, 1]));
    Some((xml, boxes, (mean_marker, mean_line, outliers)))
}

/// The paths (outermost level first) and sizes of a hierarchy's leaves.
fn tree_nodes(dims: &[Dim], colors: &Colors, sunburst: bool) -> Vec<ibis_doc_model::TreeNode> {
    use ibis_doc_model::{Number, TreeNode};
    let Some(cat) = dims.iter().find(|d| d.kind == "cat") else { return Vec::new() };
    let size = dims.iter().find(|d| d.kind == "size" || d.kind == "val");
    let count = cat.levels.iter().map(Vec::len).max().unwrap_or(0).max(size.map_or(0, |s| s.points.len()));
    // A name at a level holds until the next one: parents are written at their first child.
    let name_at = |level: usize, i: usize| -> String {
        let l = &cat.levels[level];
        (0..=i.min(l.len().saturating_sub(1))).rev().find_map(|j| l.get(j).cloned().flatten()).unwrap_or_default()
    };
    let depth = cat.levels.len();
    let mut roots: Vec<TreeNode> = Vec::new();
    for i in 0..count {
        let value = size.and_then(|s| s.points.get(i).cloned().flatten()).and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0);
        if value <= 0.0 {
            continue;
        }
        let path: Vec<String> = (0..depth).rev().map(|level| name_at(level, i)).collect();
        let mut nodes = &mut roots;
        for (d, name) in path.iter().enumerate() {
            let at = match nodes.iter().position(|n| n.name == *name && d + 1 < depth) {
                Some(at) => at,
                None => {
                    nodes.push(TreeNode { name: name.clone(), value: Number(0.0), color: colors.named("accent1"), children: Vec::new() });
                    nodes.len() - 1
                }
            };
            nodes[at].value = Number(nodes[at].value.0 + value);
            nodes = &mut nodes[at].children;
        }
    }
    // Colours: each top-level node its accent; its descendants lighter by depth and place.
    let accents: Vec<_> = (0..6).map(|k| colors.named(&format!("accent{}", k + 1))).collect();
    fn lighten(c: ibis_doc_model::Color, t: f32) -> ibis_doc_model::Color {
        let mix = |v: u32| (v as f32 + (255.0 - v as f32) * t).round().clamp(0.0, 255.0) as u32;
        let rgb = c.rgb.0;
        ibis_doc_model::Color { rgb: ibis_doc_model::Rgb((mix((rgb >> 16) & 0xff) << 16) | (mix((rgb >> 8) & 0xff) << 8) | mix(rgb & 0xff)), ..c }
    }
    fn paint(nodes: &mut [TreeNode], base: ibis_doc_model::Color, level: usize, sunburst: bool) {
        let n = nodes.len().max(1);
        for (j, node) in nodes.iter_mut().enumerate() {
            // Treemap: siblings alternate a little; a sunburst's rings go lighter outwards.
            let t = if sunburst { 0.18 * level as f32 } else { 0.07 * (j % 4) as f32 + 0.1 * (level as f32 - 1.0).max(0.0) };
            node.color = lighten(base, t.min(0.7) + 0.0 * n as f32);
            let child_base = base;
            paint(&mut node.children, child_base, level + 1, sunburst);
        }
    }
    for (k, root) in roots.iter_mut().enumerate() {
        let base = accents[k % 6];
        root.color = base;
        paint(&mut root.children, base, 1, sunburst);
    }
    roots
}

/// A chart of one invisible column and nothing else: the title and legend of
/// a treemap or sunburst are read from it (its groups and axes are not drawn).
fn tree_dummy(chart: Node) -> String {
    let none = "<a:noFill/>";
    let ser = ser_xml(0, "", none, &[Some(1.0)], "General", &[String::new()], "");
    let body = format!("<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>{ser}<c:gapWidth val=\"0\"/><c:axId val=\"1\"/><c:axId val=\"2\"/></c:barChart><c:catAx><c:axId val=\"1\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"1\"/><c:axPos val=\"b\"/><c:crossAx val=\"2\"/></c:catAx><c:valAx><c:axId val=\"2\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"1\"/><c:axPos val=\"l\"/><c:crossAx val=\"1\"/></c:valAx>");
    space_xml(&title_xml(chart), &body, &legend_xml(chart, &[]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_doc_model::{ChartType, Grouping};

    const WATERFALL: &str = r#"<cx:chartSpace xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:cx="http://schemas.microsoft.com/office/drawing/2014/chartex"><cx:chartData><cx:data id="0"><cx:strDim type="cat"><cx:lvl ptCount="5"><cx:pt idx="0">Start</cx:pt><cx:pt idx="1">Up</cx:pt><cx:pt idx="2">Down</cx:pt><cx:pt idx="3">Across</cx:pt><cx:pt idx="4">End</cx:pt></cx:lvl></cx:strDim><cx:numDim type="val"><cx:lvl ptCount="5" formatCode="General"><cx:pt idx="0">100</cx:pt><cx:pt idx="1">40</cx:pt><cx:pt idx="2">-30</cx:pt><cx:pt idx="3">-130</cx:pt><cx:pt idx="4">-20</cx:pt></cx:lvl></cx:numDim></cx:data></cx:chartData><cx:chart><cx:plotArea><cx:plotAreaRegion><cx:series layoutId="waterfall"><cx:dataId val="0"/><cx:layoutPr><cx:subtotals><cx:idx val="0"/><cx:idx val="4"/></cx:subtotals></cx:layoutPr></cx:series></cx:plotAreaRegion><cx:axis id="0"><cx:catScaling gapWidth="0.5"/></cx:axis><cx:axis id="1"><cx:valScaling/></cx:axis></cx:plotArea></cx:chart></cx:chartSpace>"#;

    fn values(chart: &Chart, name: &str, which: usize) -> Vec<Option<f64>> {
        chart.groups[0].series.iter().filter(|s| s.name == name).nth(which).unwrap().values.iter().map(|v| v.map(|n| n.0)).collect()
    }

    #[test]
    fn waterfall_is_a_stacked_column_chart_with_a_hidden_base() {
        let doc = roxmltree::Document::parse(WATERFALL).unwrap();
        let theme = crate::theme::Theme::default();
        let colors = Colors::new(&theme, None, &[]);
        let chart = crate::chart::read_in(doc.root_element(), &colors, false).unwrap();
        let group = &chart.groups[0];
        assert_eq!(group.grouping, Grouping::Stacked);
        assert!(matches!(group.kind, ChartType::Bar { horizontal: false, gap_width: 50, overlap: 100 }));
        // Start (total 100), +40 from 100, −30 from 140, −130 from 110 (across
        // zero: to −20), then a total of −20.
        assert_eq!(values(&chart, "", 0), vec![None, Some(100.0), Some(110.0), None, None]);
        assert_eq!(values(&chart, "Increase", 0), vec![None, Some(40.0), None, None, None]);
        assert_eq!(values(&chart, "Decrease", 0), vec![None, None, Some(30.0), Some(110.0), None]);
        assert_eq!(values(&chart, "Decrease", 1), vec![None, None, None, Some(-20.0), None]);
        assert_eq!(values(&chart, "Total", 0), vec![Some(100.0), None, None, None, Some(-20.0)]);
        assert_eq!(chart.legend.as_ref().map(|l| l.hidden.clone()), None);
    }

    fn read_series_values(xml: &str) -> Chart {
        let doc = roxmltree::Document::parse(xml).unwrap();
        let theme = crate::theme::Theme::default();
        let colors = Colors::new(&theme, None, &[]);
        crate::chart::read_in(doc.root_element(), &colors, false).unwrap()
    }

    fn numeric_space(layout: &str, values: &[&str], binning: &str) -> String {
        let pts: String = values.iter().enumerate().map(|(i, v)| format!("<cx:pt idx=\"{i}\">{v}</cx:pt>")).collect();
        format!("<cx:chartSpace xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:cx=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"><cx:chartData><cx:data id=\"0\"><cx:numDim type=\"val\"><cx:lvl ptCount=\"{}\" formatCode=\"General\">{pts}</cx:lvl></cx:numDim></cx:data></cx:chartData><cx:chart><cx:plotArea><cx:plotAreaRegion><cx:series layoutId=\"{layout}\"><cx:dataId val=\"0\"/><cx:layoutPr>{binning}</cx:layoutPr></cx:series></cx:plotAreaRegion><cx:axis id=\"0\"><cx:catScaling gapWidth=\"0\"/></cx:axis><cx:axis id=\"1\"><cx:valScaling/></cx:axis></cx:plotArea></cx:chart></cx:chartSpace>", values.len())
    }

    #[test]
    fn histogram_counts_values_in_bins_closed_on_the_right() {
        // 1 2 2 3 3 3 4 4 5 7 in bins of 3 from 1: [1,4] (1 2 2 3 3 3 4 4 = 8), (4,7] (5 7 = 2).
        let xml = numeric_space("clusteredColumn", &["1", "2", "2", "3", "3", "3", "4", "4", "5", "7"], "<cx:binning intervalClosed=\"r\"><cx:binSize val=\"3\"/></cx:binning>");
        let chart = read_series_values(&xml);
        let series = &chart.groups[0].series[0];
        assert_eq!(series.categories, vec!["[1, 4]".to_string(), "(4, 7]".to_string()]);
        assert_eq!(series.values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), vec![Some(8.0), Some(2.0)]);
        assert!(matches!(chart.groups[0].kind, ChartType::Bar { gap_width: 0, .. }));
        // Closed on the left: [1,4) holds 1 2 2 3 3 3 (6), [4,7] 4 4 5 7 (4).
        let xml = numeric_space("clusteredColumn", &["1", "2", "2", "3", "3", "3", "4", "4", "5", "7"], "<cx:binning intervalClosed=\"l\"><cx:binSize val=\"3\"/></cx:binning>");
        let chart = read_series_values(&xml);
        assert_eq!(chart.groups[0].series[0].values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), vec![Some(6.0), Some(4.0)]);
        // A count of bins: 4 bins from 0 to 8.
        let xml = numeric_space("clusteredColumn", &["0", "1", "3", "5", "8"], "<cx:binning><cx:binCount val=\"4\"/></cx:binning>");
        let chart = read_series_values(&xml);
        assert_eq!(chart.groups[0].series[0].categories.len(), 4);
    }

    #[test]
    fn pareto_sorts_the_categories_and_adds_the_running_share() {
        let xml = r#"<cx:chartSpace xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:cx="http://schemas.microsoft.com/office/drawing/2014/chartex"><cx:chartData><cx:data id="0"><cx:strDim type="cat"><cx:lvl ptCount="3"><cx:pt idx="0">A</cx:pt><cx:pt idx="1">B</cx:pt><cx:pt idx="2">C</cx:pt></cx:lvl></cx:strDim><cx:numDim type="val"><cx:lvl ptCount="3" formatCode="General"><cx:pt idx="0">10</cx:pt><cx:pt idx="1">60</cx:pt><cx:pt idx="2">30</cx:pt></cx:lvl></cx:numDim></cx:data></cx:chartData><cx:chart><cx:plotArea><cx:plotAreaRegion><cx:series layoutId="pareto"><cx:dataId val="0"/></cx:series></cx:plotAreaRegion><cx:axis id="0"><cx:catScaling/></cx:axis><cx:axis id="1"><cx:valScaling/></cx:axis></cx:plotArea></cx:chart></cx:chartSpace>"#;
        let chart = read_series_values(xml);
        assert_eq!(chart.groups[0].series[0].categories, vec!["B".to_string(), "C".to_string(), "A".to_string()]);
        let share: Vec<f64> = chart.groups[1].series[0].values.iter().map(|v| v.unwrap().0).collect();
        assert_eq!(share, vec![0.6, 0.9, 1.0]);
        assert_eq!(chart.groups[1].kind, ChartType::Line);
    }

    #[test]
    fn a_level_keeps_its_blank_points_and_drops_an_index_past_the_limit() {
        let xml = r#"<cx:data xmlns:cx="http://schemas.microsoft.com/office/drawing/2014/chartex" id="0"><cx:strDim type="cat"><cx:lvl ptCount="4"><cx:pt idx="0">A</cx:pt><cx:pt idx="2">C</cx:pt></cx:lvl></cx:strDim><cx:numDim type="val"><cx:lvl ptCount="2" formatCode="General"><cx:pt idx="0">10</cx:pt><cx:pt idx="9223372036854775807">20</cx:pt></cx:lvl></cx:numDim></cx:data>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let dims = dims(doc.root_element());
        assert_eq!(dims[0].points, vec![Some("A".to_string()), None, Some("C".to_string()), None]);
        assert_eq!(dims[1].points, vec![Some("10".to_string()), None]);
    }
}
