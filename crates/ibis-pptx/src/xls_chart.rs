// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The charts of an old workbook (`.xls`), from the public specification (MS-XLS): the records of a
//! chart's own stream (`BOF` of type chart … `EOF`) read into a plain description, and written as the
//! `c:chartSpace` of an `.xlsx` (with the values and texts of the cells the series name, as its cache:
//! the program draws a chart from them).
//!
//! What is read: the chart groups (bars and columns, lines, areas, pies and doughnuts, scatters and
//! radars; a group's grouping, gap and overlap), the series (name, values, categories or x values, the
//! colour of the series and of single points, a line's marker and smoothing), the titles (chart, category
//! axis, value axis), the legend and its place, and the axes' scale (minimum, maximum, major unit,
//! logarithmic, reversed) and gridlines. Left out: a secondary axis (its groups are drawn on the
//! first), 3-D views, data labels, trendlines, error bars and the text's fonts.

use crate::xls::{u16_at, Record};

/// A range of cells a series reads (an `AI` record's formula: a 3-D area or reference).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ref {
    /// The index of the `EXTERNSHEET` entry (which sheet).
    pub ixti: u16,
    pub rows: (u16, u16),
    pub columns: (u16, u16),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) enum Source {
    #[default]
    None,
    Ref(Ref),
    Text(String),
    /// Values the chart holds itself (a document's chart with its own data table).
    Lit(Vec<Cell>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Bar,
    Line,
    Area,
    Pie,
    Scatter,
    Radar,
}

#[derive(Clone, Debug)]
pub(crate) struct Group {
    pub kind: Kind,
    /// Bars: horizontal. Bars, lines, areas: stacked, or stacked to 100 %.
    pub horizontal: bool,
    pub stacked: bool,
    pub percent: bool,
    pub overlap: i16,
    pub gap: u16,
    /// Pies: the hole (percent; 0 is a pie) and the first slice's angle.
    pub hole: u16,
    pub angle: u16,
    /// Radars: filled.
    pub filled: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Series {
    pub name: Source,
    pub values: Source,
    /// Categories; a scatter's x values.
    pub categories: Source,
    pub group: usize,
    pub fill: Option<u32>,
    pub line: Option<u32>,
    pub line_none: bool,
    pub marker: Option<u16>,
    pub marker_color: Option<u32>,
    pub smooth: bool,
    /// The colours of single points (a pie's slices).
    pub points: Vec<(u16, u32)>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Axis {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub major: Option<f64>,
    pub log: bool,
    pub reversed: bool,
    pub grid: bool,
    pub title: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Chart {
    pub title: Option<String>,
    pub groups: Vec<Group>,
    pub series: Vec<Series>,
    /// The legend's place (`b`, `tr`, `t`, `r`, `l`), if there is one.
    pub legend: Option<&'static str>,
    /// The category (x) axis and the value (y) axis.
    pub category: Axis,
    pub value: Axis,
}

fn rgb(d: &[u8], at: usize) -> Option<u32> {
    Some(((*d.get(at)? as u32) << 16) | ((*d.get(at + 1)? as u32) << 8) | *d.get(at + 2)? as u32)
}

fn f64_at(d: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

/// The first reference of an `AI` record's formula.
fn first_ref(rgce: &[u8]) -> Source {
    match rgce.first() {
        Some(0x3A | 0x5A | 0x7A) => {
            let (Some(ixti), Some(row), Some(col)) = (u16_at(rgce, 1), u16_at(rgce, 3), u16_at(rgce, 5)) else { return Source::None };
            let col = col & 0x3FFF;
            Source::Ref(Ref { ixti, rows: (row, row), columns: (col, col) })
        }
        Some(0x3B | 0x5B | 0x7B) => {
            let (Some(ixti), Some(r1), Some(r2), Some(c1), Some(c2)) = (u16_at(rgce, 1), u16_at(rgce, 3), u16_at(rgce, 5), u16_at(rgce, 7), u16_at(rgce, 9)) else { return Source::None };
            Source::Ref(Ref { ixti, rows: (r1, r2), columns: (c1 & 0x3FFF, c2 & 0x3FFF) })
        }
        _ => Source::None,
    }
}

/// A short unicode string (`SeriesText`: an id, then a length byte, flags and the characters).
fn series_text(d: &[u8]) -> Option<String> {
    let n = *d.get(2)? as usize;
    let wide = d.get(3)? & 1 != 0;
    let body = d.get(4..)?;
    Some(if wide {
        char::decode_utf16(body.chunks_exact(2).take(n).map(|c| u16::from_le_bytes([c[0], c[1]]))).map(|c| c.unwrap_or('\u{FFFD}')).collect()
    } else {
        body.iter().take(n).map(|&b| b as char).collect()
    })
}

/// What is being read inside a `BEGIN`…`END` pair.
#[derive(Clone, Copy, PartialEq)]
enum Block {
    Other,
    Series,
    Text,
    Axis(u16),
    DataFormat(u16, usize),
}

/// Reads a chart's records (those between its `BOF` and `EOF`).
pub(crate) fn read_chart(records: &[&Record<'_>]) -> Option<Chart> {
    let mut chart = Chart::default();
    let mut stack: Vec<Block> = Vec::new();
    // The block a `BEGIN` will open, set by the record before it.
    let mut next = Block::Other;
    let mut text: Option<String> = None;
    let mut link: Option<u16> = None;
    let mut axis_kind: Option<u16> = None;
    let mut grid_for: Option<u16> = None;
    let mut line_format_for_axis = false;
    for r in records {
        let d = r.data;
        let top = stack.last().copied().unwrap_or(Block::Other);
        match r.id {
            0x1033 => stack.push(std::mem::replace(&mut next, Block::Other)),
            0x1034 => {
                if let Some(Block::Text) = stack.pop() {
                    // A title's text is known now: where it belongs is its object link.
                    if let (Some(t), Some(l)) = (text.take(), link.take()) {
                        match l {
                            1 => chart.title = Some(t),
                            2 => chart.value.title = Some(t),
                            3 => chart.category.title = Some(t),
                            _ => {}
                        }
                    }
                    link = None;
                }
            }
            0x1003 => {
                chart.series.push(Series { group: 0, ..Series::default() });
                next = Block::Series;
            }
            0x1045 if matches!(top, Block::Series) => {
                if let (Some(s), Some(g)) = (chart.series.last_mut(), u16_at(d, 0)) {
                    s.group = g as usize;
                }
            }
            0x1051 if matches!(top, Block::Series) => {
                // AI: id (0 name, 1 values, 2 categories), the kind of link (2: a formula), …, the formula.
                let (Some(&id), Some(&rt)) = (d.first(), d.get(1)) else { continue };
                let cce = u16_at(d, 6).unwrap_or(0) as usize;
                let source = if rt == 2 { first_ref(d.get(8..8 + cce).unwrap_or(&[])) } else { Source::None };
                if let Some(s) = chart.series.last_mut() {
                    match id {
                        0 => s.name = source,
                        1 => s.values = source,
                        2 => s.categories = source,
                        _ => {}
                    }
                }
            }
            0x100D => {
                let t = series_text(d);
                if matches!(top, Block::Text) {
                    text = t;
                } else if matches!(top, Block::Series)
                    && let (Some(s), Some(t)) = (chart.series.last_mut(), t)
                    && s.name == Source::None
                {
                    s.name = Source::Text(t);
                }
            }
            0x1006 => {
                // DATAFORMAT: the point (0xFFFF: the series) and the series it formats.
                let (point, series) = (u16_at(d, 0).unwrap_or(0xFFFF), u16_at(d, 2).unwrap_or(0) as usize);
                next = Block::DataFormat(point, series);
            }
            0x100A => {
                // AREAFORMAT: foreground colour, pattern, automatic.
                if let Block::DataFormat(point, series) = top {
                    let auto = u16_at(d, 10).unwrap_or(1) & 1 != 0;
                    let pattern = u16_at(d, 8).unwrap_or(0);
                    if !auto
                        && pattern != 0
                        && let Some(c) = rgb(d, 0)
                        && let Some(s) = chart.series.get_mut(series)
                    {
                        if point == 0xFFFF {
                            s.fill = Some(c);
                        } else {
                            s.points.push((point, c));
                        }
                    }
                }
            }
            0x1007 => {
                // LINEFORMAT: colour, style (5: none), weight, automatic.
                if let Block::DataFormat(point, series) = top
                    && point == 0xFFFF
                    && let Some(s) = chart.series.get_mut(series)
                {
                    let auto = u16_at(d, 8).unwrap_or(1) & 1 != 0;
                    if u16_at(d, 4) == Some(5) {
                        s.line_none = true;
                    } else if !auto {
                        s.line = rgb(d, 0);
                    }
                }
            }
            0x1009 => {
                // MARKERFORMAT: the kind of marker (0: none).
                if let Block::DataFormat(point, series) = top
                    && point == 0xFFFF
                    && let Some(s) = chart.series.get_mut(series)
                {
                    s.marker = u16_at(d, 8);
                    s.marker_color = if u16_at(d, 10).unwrap_or(1) & 1 == 0 { rgb(d, 0) } else { None };
                }
            }
            0x105D => {
                // SERFMT: smoothed line.
                if let Some(s) = chart.series.last_mut() {
                    s.smooth = d.first().is_some_and(|b| b & 1 != 0);
                }
            }
            0x1014 => {} // CHARTFORMAT: its first record of a kind starts the group below.
            0x1017 => chart.groups.push(Group {
                kind: Kind::Bar,
                horizontal: d.get(4).is_some_and(|b| b & 1 != 0),
                stacked: d.get(4).is_some_and(|b| b & 2 != 0),
                percent: d.get(4).is_some_and(|b| b & 4 != 0),
                overlap: u16_at(d, 0).unwrap_or(0) as i16,
                gap: u16_at(d, 2).unwrap_or(150),
                hole: 0,
                angle: 0,
                filled: false,
            }),
            0x1018 | 0x101A => chart.groups.push(Group {
                kind: if r.id == 0x1018 { Kind::Line } else { Kind::Area },
                horizontal: false,
                stacked: d.first().is_some_and(|b| b & 1 != 0),
                percent: d.first().is_some_and(|b| b & 2 != 0),
                overlap: 0,
                gap: 0,
                hole: 0,
                angle: 0,
                filled: false,
            }),
            0x1019 => chart.groups.push(Group { kind: Kind::Pie, horizontal: false, stacked: false, percent: false, overlap: 0, gap: 0, hole: u16_at(d, 2).unwrap_or(0), angle: u16_at(d, 0).unwrap_or(0), filled: false }),
            0x101B => chart.groups.push(Group { kind: Kind::Scatter, horizontal: false, stacked: false, percent: false, overlap: 0, gap: 0, hole: 0, angle: 0, filled: false }),
            0x103E | 0x1040 => chart.groups.push(Group { kind: Kind::Radar, horizontal: false, stacked: false, percent: false, overlap: 0, gap: 0, hole: 0, angle: 0, filled: r.id == 0x1040 }),
            0x1015 => {
                // LEGEND: where it is docked.
                chart.legend = Some(match d.get(16) {
                    Some(0) => "b",
                    Some(1) => "tr",
                    Some(2) => "t",
                    Some(4) => "l",
                    _ => "r",
                });
            }
            0x1025 => {
                next = Block::Text;
                text = None;
                link = None;
            }
            0x1027 if matches!(top, Block::Text) => link = u16_at(d, 0),
            0x101D => {
                // AXIS: 0 category, 1 value.
                axis_kind = u16_at(d, 0);
                next = Block::Axis(axis_kind.unwrap_or(9));
            }
            0x101F => {
                // VALUERANGE: minimum, maximum, major unit, minor unit, crossing, flags.
                if let Block::Axis(kind) = top {
                    let flags = u16_at(d, 40).unwrap_or(0xFF);
                    let axis = if kind == 0 { &mut chart.category } else { &mut chart.value };
                    if flags & 1 == 0 {
                        axis.min = f64_at(d, 0);
                    }
                    if flags & 2 == 0 {
                        axis.max = f64_at(d, 8);
                    }
                    if flags & 4 == 0 {
                        axis.major = f64_at(d, 16).filter(|m| *m > 0.0);
                    }
                    axis.log = flags & 0x20 != 0;
                    axis.reversed = flags & 0x40 != 0;
                }
            }
            0x1021 => {
                // AXISLINEFORMAT: 0 the axis line, 1 major gridlines.
                grid_for = if let Block::Axis(kind) = top { u16_at(d, 0).filter(|&i| i == 1).map(|_| kind) } else { None };
                line_format_for_axis = true;
                if let Some(kind) = grid_for {
                    let axis = if kind == 0 { &mut chart.category } else { &mut chart.value };
                    axis.grid = true;
                }
            }
            _ => {}
        }
    }
    let _ = (axis_kind, grid_for, line_format_for_axis);
    chart.series.retain(|s| s.values != Source::None);
    if chart.groups.is_empty() || chart.series.is_empty() {
        return None;
    }
    Some(chart)
}

/// The cells a [`Ref`] covers, as the caller reads them.
pub(crate) trait Cells {
    /// The sheet's name and the values of the range: row by row, a number, a text or nothing.
    fn range(&self, r: &Ref) -> Option<(String, Vec<Cell>)>;
    /// A colour of the workbook's palette (the old charts' automatic colours are some of them: 16 and on
    /// for bars, areas and pie slices, 24 and on for lines).
    fn color(&self, index: usize) -> u32;
    /// The text of a single cell reference.
    fn text(&self, r: &Ref) -> Option<String> {
        match self.range(r)?.1.into_iter().next()? {
            Cell::Text(t) => Some(t),
            Cell::Number(n) => Some(ibis_calc::number_to_xml(n)),
            Cell::Empty => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Cell {
    Number(f64),
    Text(String),
    Empty,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn column_name(mut c: u32) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    s
}

fn formula_of(name: &str, r: &Ref) -> String {
    let sheet = if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty() { name.to_string() } else { format!("'{}'", name.replace('\'', "''")) };
    let (a, b) = (format!("${}${}", column_name(r.columns.0 as u32), r.rows.0 as u32 + 1), format!("${}${}", column_name(r.columns.1 as u32), r.rows.1 as u32 + 1));
    if a == b { format!("{sheet}!{a}") } else { format!("{sheet}!{a}:{b}") }
}

fn rich(text: &str) -> String {
    format!("<c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/>", esc(text))
}

fn solid(color: u32) -> String {
    format!("<a:solidFill><a:srgbClr val=\"{color:06X}\"/></a:solidFill>")
}

fn num_cache(cells: &[Cell]) -> String {
    let mut pts = String::new();
    for (i, c) in cells.iter().enumerate() {
        match c {
            Cell::Number(n) => pts += &format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", ibis_calc::number_to_xml(*n)),
            Cell::Text(t) => {
                if let Ok(n) = t.trim().parse::<f64>() {
                    pts += &format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", ibis_calc::number_to_xml(n));
                }
            }
            Cell::Empty => {}
        }
    }
    format!("<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>{pts}</c:numCache>", cells.len())
}

fn str_cache(cells: &[Cell]) -> String {
    let mut pts = String::new();
    for (i, c) in cells.iter().enumerate() {
        let t = match c {
            Cell::Number(n) => ibis_calc::number_to_xml(*n),
            Cell::Text(t) => t.clone(),
            Cell::Empty => continue,
        };
        pts += &format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", esc(&t));
    }
    format!("<c:strCache><c:ptCount val=\"{}\"/>{pts}</c:strCache>", cells.len())
}

fn axis_scaling(a: &Axis, value: bool) -> String {
    let mut s = String::from("<c:scaling>");
    if a.log {
        s += "<c:logBase val=\"10\"/>";
    }
    s += &format!("<c:orientation val=\"{}\"/>", if a.reversed { "maxMin" } else { "minMax" });
    if value {
        if let Some(m) = a.max {
            s += &format!("<c:max val=\"{}\"/>", ibis_calc::number_to_xml(m));
        }
        if let Some(m) = a.min {
            s += &format!("<c:min val=\"{}\"/>", ibis_calc::number_to_xml(m));
        }
    }
    s + "</c:scaling>"
}

fn marker_symbol(imk: u16) -> &'static str {
    match imk {
        0 => "none",
        1 => "square",
        2 => "diamond",
        3 => "triangle",
        4 => "x",
        5 => "star",
        6 => "dot",
        7 => "dash",
        8 => "circle",
        9 => "plus",
        _ => "auto",
    }
}

/// The `c:chartSpace` of a chart; `cells` reads the cells its series name, `default_name` gives the name of
/// a series with none (its number from 1).
pub(crate) fn chart_xml(chart: &Chart, cells: &dyn Cells, default_name: &dyn Fn(usize) -> String) -> String {
    let scatter = chart.groups.iter().any(|g| g.kind == Kind::Scatter);
    let pie = chart.groups.iter().all(|g| g.kind == Kind::Pie);
    let mut plot = String::new();
    for (gi, g) in chart.groups.iter().enumerate() {
        let mut sers = String::new();
        for (si, s) in chart.series.iter().enumerate().filter(|(_, s)| s.group.min(chart.groups.len() - 1) == gi) {
            let mut x = format!("<c:ser><c:idx val=\"{si}\"/><c:order val=\"{si}\"/>");
            // The name.
            let name_xml = match &s.name {
                Source::Ref(r) => match (cells.range(r), cells.text(r)) {
                    (Some((sheet, _)), Some(t)) => format!("<c:tx><c:strRef><c:f>{}</c:f>{}</c:strRef></c:tx>", esc(&formula_of(&sheet, r)), str_cache(&[Cell::Text(t)])),
                    _ => String::new(),
                },
                Source::Text(t) => format!("<c:tx><c:v>{}</c:v></c:tx>", esc(t)),
                Source::Lit(v) => format!("<c:tx><c:v>{}</c:v></c:tx>", esc(&v.iter().find_map(|c| match c { Cell::Text(t) => Some(t.clone()), Cell::Number(n) => Some(ibis_calc::number_to_xml(*n)), Cell::Empty => None }).unwrap_or_default())),
                Source::None => format!("<c:tx><c:v>{}</c:v></c:tx>", esc(&default_name(si + 1))),
            };
            x += &name_xml;
            // The look: the colours the file names, else the automatic ones of Excel 97-2003.
            let mut sp = String::new();
            let line_like = matches!(g.kind, Kind::Line | Kind::Scatter);
            let area_color = s.fill.unwrap_or_else(|| cells.color(16 + si % 16));
            let line_color = s.line.unwrap_or_else(|| cells.color(24 + si % 16));
            if line_like {
                if s.line_none {
                    sp += "<a:ln><a:noFill/></a:ln>";
                } else {
                    sp += &format!("<a:ln w=\"28575\" cap=\"rnd\">{}<a:round/></a:ln>", solid(line_color));
                }
            } else if g.kind == Kind::Radar && !g.filled {
                sp += &format!("<a:ln w=\"28575\">{}</a:ln>", solid(line_color));
            } else if g.kind != Kind::Pie {
                sp += &solid(area_color);
            }
            if !sp.is_empty() {
                x += &format!("<c:spPr>{sp}</c:spPr>");
            }
            if line_like || g.kind == Kind::Radar && !g.filled {
                let symbol = s.marker.map(marker_symbol).unwrap_or(if g.kind == Kind::Scatter { "auto" } else { "none" });
                let marker_color = s.marker_color.or(s.line).unwrap_or_else(|| cells.color(24 + si % 16));
                if symbol == "none" {
                    x += "<c:marker><c:symbol val=\"none\"/></c:marker>";
                } else {
                    x += &format!("<c:marker><c:symbol val=\"{symbol}\"/><c:size val=\"7\"/><c:spPr>{}</c:spPr></c:marker>", solid(marker_color));
                }
            }
            if g.kind == Kind::Pie {
                let count = match &s.values {
                    Source::Ref(r) => cells.range(r).map_or(0, |(_, v)| v.len()),
                    _ => 0,
                };
                let count = if let Source::Lit(v) = &s.values { v.len() } else { count };
                for p in 0..count {
                    let c = s.points.iter().find(|(i, _)| *i as usize == p).map_or_else(|| cells.color(16 + p % 16), |(_, c)| *c);
                    x += &format!("<c:dPt><c:idx val=\"{p}\"/><c:bubble3D val=\"0\"/><c:spPr>{}</c:spPr></c:dPt>", solid(c));
                }
            }
            // The categories (or a scatter's x values) and the values.
            let numbers = |r: &Ref| -> String {
                match cells.range(r) {
                    Some((sheet, v)) => format!("<c:numRef><c:f>{}</c:f>{}</c:numRef>", esc(&formula_of(&sheet, r)), num_cache(&v)),
                    None => String::new(),
                }
            };
            let strings = |r: &Ref| -> String {
                match cells.range(r) {
                    Some((sheet, v)) => format!("<c:strRef><c:f>{}</c:f>{}</c:strRef>", esc(&formula_of(&sheet, r)), str_cache(&v)),
                    None => String::new(),
                }
            };
            let num_lit = |v: &[Cell]| format!("<c:numLit>{}</c:numLit>", num_cache(v).trim_start_matches("<c:numCache>").trim_end_matches("</c:numCache>"));
            let str_lit = |v: &[Cell]| format!("<c:strLit>{}</c:strLit>", str_cache(v).trim_start_matches("<c:strCache>").trim_end_matches("</c:strCache>"));
            if let Source::Lit(values) = &s.values {
                if g.kind == Kind::Scatter {
                    if let Source::Lit(cat) = &s.categories {
                        x += &format!("<c:xVal>{}</c:xVal>", num_lit(cat));
                    }
                    x += &format!("<c:yVal>{}</c:yVal>", num_lit(values));
                } else {
                    if let Source::Lit(cat) = &s.categories {
                        x += &format!("<c:cat>{}</c:cat>", str_lit(cat));
                    }
                    x += &format!("<c:val>{}</c:val>", num_lit(values));
                }
                x += "</c:ser>";
                sers += &x;
                continue;
            }
            let Source::Ref(values) = &s.values else { continue };
            if g.kind == Kind::Scatter {
                if let Source::Ref(cat) = &s.categories {
                    x += &format!("<c:xVal>{}</c:xVal>", numbers(cat));
                }
                x += &format!("<c:yVal>{}</c:yVal>", numbers(values));
                if s.smooth {
                    x += "<c:smooth val=\"1\"/>";
                }
            } else {
                if let Source::Ref(cat) = &s.categories {
                    x += &format!("<c:cat>{}</c:cat>", strings(cat));
                }
                x += &format!("<c:val>{}</c:val>", numbers(values));
                if g.kind == Kind::Line && s.smooth {
                    x += "<c:smooth val=\"1\"/>";
                }
            }
            x += "</c:ser>";
            sers += &x;
        }
        let grouping = if g.percent { "percentStacked" } else if g.stacked { "stacked" } else { "standard" };
        let axes = "<c:axId val=\"111\"/><c:axId val=\"222\"/>";
        plot += &match g.kind {
            Kind::Bar => {
                let overlap = if g.stacked || g.percent { 100 } else { g.overlap.clamp(-100, 100) };
                format!("<c:barChart><c:barDir val=\"{}\"/><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>{sers}<c:gapWidth val=\"{}\"/><c:overlap val=\"{overlap}\"/>{axes}</c:barChart>", if g.horizontal { "bar" } else { "col" }, if g.percent { "percentStacked" } else if g.stacked { "stacked" } else { "clustered" }, g.gap.min(500))
            }
            Kind::Line => format!("<c:lineChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{sers}<c:marker val=\"1\"/>{axes}</c:lineChart>"),
            Kind::Area => format!("<c:areaChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{sers}{axes}</c:areaChart>"),
            Kind::Pie if g.hole > 0 => format!("<c:doughnutChart><c:varyColors val=\"1\"/>{sers}<c:firstSliceAng val=\"{}\"/><c:holeSize val=\"{}\"/></c:doughnutChart>", g.angle % 360, g.hole.clamp(10, 90)),
            Kind::Pie => format!("<c:pieChart><c:varyColors val=\"1\"/>{sers}<c:firstSliceAng val=\"{}\"/></c:pieChart>", g.angle % 360),
            Kind::Scatter => format!("<c:scatterChart><c:scatterStyle val=\"lineMarker\"/><c:varyColors val=\"0\"/>{sers}{axes}</c:scatterChart>"),
            Kind::Radar => format!("<c:radarChart><c:radarStyle val=\"{}\"/><c:varyColors val=\"0\"/>{sers}{axes}</c:radarChart>", if g.filled { "filled" } else { "marker" }),
        };
    }
    let mut axes = String::new();
    if !pie {
        let horizontal = chart.groups.iter().any(|g| g.kind == Kind::Bar && g.horizontal);
        let title = |a: &Axis| a.title.as_ref().map(|t| format!("<c:title>{}</c:title>", rich(t))).unwrap_or_default();
        let grid = |a: &Axis| if a.grid { "<c:majorGridlines/>" } else { "" };
        let (cat_pos, val_pos) = if horizontal { ("l", "b") } else { ("b", "l") };
        if scatter {
            axes += &format!("<c:valAx><c:axId val=\"111\"/>{}<c:delete val=\"0\"/><c:axPos val=\"{cat_pos}\"/>{}{}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"222\"/><c:crosses val=\"autoZero\"/></c:valAx>", axis_scaling(&chart.category, true), grid(&chart.category), title(&chart.category));
        } else {
            axes += &format!("<c:catAx><c:axId val=\"111\"/>{}<c:delete val=\"0\"/><c:axPos val=\"{cat_pos}\"/>{}{}<c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"222\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/></c:catAx>", axis_scaling(&chart.category, false), grid(&chart.category), title(&chart.category));
        }
        let major = chart.value.major.map(|m| format!("<c:majorUnit val=\"{}\"/>", ibis_calc::number_to_xml(m))).unwrap_or_default();
        axes += &format!("<c:valAx><c:axId val=\"222\"/>{}<c:delete val=\"0\"/><c:axPos val=\"{val_pos}\"/>{}{}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"111\"/><c:crosses val=\"autoZero\"/>{major}</c:valAx>", axis_scaling(&chart.value, true), grid(&chart.value), title(&chart.value));
    }
    let title = chart.title.as_ref().map(|t| format!("<c:title>{}</c:title>", rich(t))).unwrap_or_default();
    let legend = chart.legend.map(|p| format!("<c:legend><c:legendPos val=\"{p}\"/><c:overlay val=\"0\"/></c:legend>")).unwrap_or_default();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:chart>{title}<c:autoTitleDeleted val=\"{}\"/><c:plotArea><c:layout/>{plot}{axes}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart></c:chartSpace>",
        if chart.title.is_some() { 0 } else { 1 }
    )
}
