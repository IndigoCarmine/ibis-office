// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Conditional formats drawn: which rules hold for which cells (their
//! values, the formulas Excel writes for the text, blank and date rules,
//! top and bottom ranks, averages, unique and duplicate values), and what
//! they give: the rule's format over the cell's, a colour scale's fill, a
//! data bar, an icon. The sheet is shown as a copy with the formats
//! applied (the model keeps the cells' own formats).

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

use ibis_calc::eval::Book;
use ibis_doc_model::{CellKind, CellRange, CellValue, CfKind, CfStyle, CfValue, Rgb, Sheet, SheetCell};

/// A drawn data bar segment and optional axis line.
#[derive(Clone, Debug)]
pub(crate) struct DataBarMark {
    pub(crate) cell: (u32, u32),
    /// Start fraction (0.0 .. 1.0) of cell width.
    pub(crate) from: f32,
    /// End fraction (0.0 .. 1.0) of cell width.
    pub(crate) to: f32,
    pub(crate) color: Rgb,
    pub(crate) gradient: bool,
    /// If true, bar represents a negative value (fading from right/axis to left).
    pub(crate) is_negative: bool,
    /// Optional vertical axis position (fraction 0.0 .. 1.0) to draw a divider line.
    pub(crate) axis: Option<f32>,
    /// Color of the axis line.
    pub(crate) axis_color: Rgb,
}

/// What the conditional formats draw over the cells, besides their formats.
#[derive(Clone, Debug, Default)]
pub(crate) struct Overlay {
    /// A data bar: the cell, its bounds, color, gradient, and axis.
    pub(crate) bars: Vec<DataBarMark>,
    /// An icon: the cell, its colour and its shape (0 circle, 1 up, 2 side, 3 down).
    pub(crate) icons: Vec<((u32, u32), Rgb, u8)>,
}

/// One sheet as the calculation sees it (formulas of conditional formats
/// refer to their own sheet; other sheets are not seen).
pub(crate) struct OneSheet<'a>(pub(crate) &'a Sheet);

impl Book for OneSheet<'_> {
    fn sheet_count(&self) -> usize {
        1
    }
    fn sheet_name(&self, _: usize) -> &str {
        ""
    }
    fn value(&self, _: usize, row: u32, column: u32) -> CellValue {
        self.0.cell(row, column).map(|c| c.value.clone()).unwrap_or_default()
    }
    fn formula(&self, _: usize, row: u32, column: u32) -> Option<&str> {
        self.0.cell(row, column)?.formula.as_deref()
    }
    fn for_each_cell(&self, _: usize, area: (u32, u32, u32, u32), f: &mut dyn FnMut(u32, u32, &CellValue)) {
        let (r0, c0, r1, c1) = area;
        for c in &self.0.cells {
            if (r0..=r1).contains(&c.row) && (c0..=c1).contains(&c.column) && (!c.value.is_empty() || c.formula.is_some()) {
                f(c.row, c.column, &c.value);
            }
        }
    }
    fn extent(&self, _: usize) -> (u32, u32) {
        self.0.cells.iter().fold((0, 0), |(r, c), x| (r.max(x.row + 1), c.max(x.column + 1)))
    }
    fn defined_name(&self, _: &str, _: Option<usize>) -> Option<String> {
        None
    }
}

fn number(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Number(n) => Some(*n),
        _ => None,
    }
}

fn truthy(v: &CellValue) -> bool {
    match v {
        CellValue::Bool(b) => *b,
        CellValue::Number(n) => *n != 0.0,
        _ => false,
    }
}

/// Compare as Excel's comparison operators do: numbers by value, text
/// without regard to case, a number below any text.
fn compare(a: &CellValue, b: &CellValue) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    let rank = |v: &CellValue| match v {
        CellValue::Number(_) | CellValue::Empty => 0,
        CellValue::Text(_) => 1,
        CellValue::Bool(_) => 2,
        CellValue::Error(_) => 3,
    };
    match (a, b) {
        (CellValue::Error(_), _) | (_, CellValue::Error(_)) => None,
        _ if rank(a) != rank(b) => Some(rank(a).cmp(&rank(b))),
        (CellValue::Text(x), CellValue::Text(y)) => Some(x.to_lowercase().cmp(&y.to_lowercase())),
        (CellValue::Bool(x), CellValue::Bool(y)) => Some(x.cmp(y)),
        _ => {
            let (x, y) = (number(a).unwrap_or(0.0), number(b).unwrap_or(0.0));
            x.partial_cmp(&y).or(Some(Ordering::Equal))
        }
    }
}

/// The cells of a conditional format's ranges (within the sheet's grid).
fn cells_of(sheet: &Sheet, ranges: &[CellRange]) -> Vec<(u32, u32)> {
    let (rows, columns) = (sheet.rows.len() as u32, sheet.columns.len() as u32);
    let mut out = Vec::new();
    for r in ranges {
        for row in r.first_row..=r.last_row.min(rows.saturating_sub(1)) {
            for column in r.first_column..=r.last_column.min(columns.saturating_sub(1)) {
                out.push((row, column));
            }
        }
    }
    out
}

/// A point's value among the numbers `values` (sorted).
fn point(book: &OneSheet, p: &CfValue, values: &[f64], anchor: (u32, u32)) -> Option<f64> {
    let (min, max) = (*values.first()?, *values.last()?);
    let given = || -> Option<f64> {
        let v = p.value.as_deref()?;
        v.parse::<f64>().ok().or_else(|| number(&ibis_calc::recalc::evaluate(book, (0, anchor.0, anchor.1), v.trim_start_matches('=')).0))
    };
    Some(match p.kind.as_str() {
        "min" | "autoMin" => min,
        "max" | "autoMax" => max,
        "percent" => min + (max - min) * given()? / 100.0,
        "percentile" => {
            let k = given()? / 100.0;
            let pos = k * (values.len() - 1) as f64;
            let (i, f) = (pos.floor() as usize, pos.fract());
            values[i] + (values.get(i + 1).copied().unwrap_or(values[i]) - values[i]) * f
        }
        _ => given()?,
    })
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let ch = |shift: u32| {
        let (x, y) = (((a.0 >> shift) & 0xff) as f32, ((b.0 >> shift) & 0xff) as f32);
        ((x + (y - x) * t).round().clamp(0.0, 255.0) as u32) << shift
    };
    Rgb(ch(16) | ch(8) | ch(0))
}

/// The icons of a set, low to high: (colour, shape).
pub(crate) fn icons_of(set: &str) -> Vec<(Rgb, u8)> {
    let red = Rgb(0xC00000);
    let dark_red = Rgb(0x8B0000);
    let orange = Rgb(0xFF7F00);
    let yellow = Rgb(0xFFC000);
    let green = Rgb(0x00B050);
    let light_green = Rgb(0x92D050);
    let gray = Rgb(0x7F7F7F);
    let black = Rgb(0x000000);
    let blue = Rgb(0x418AB3);

    match set {
        "3Arrows" => vec![(red, 3), (yellow, 2), (green, 1)],
        "3ArrowsGray" => vec![(gray, 3), (gray, 2), (gray, 1)],
        "3Flags" => vec![(red, 8), (yellow, 8), (green, 8)],
        "3TrafficLights1" => vec![(red, 0), (yellow, 0), (green, 0)],
        "3TrafficLights2" => vec![(red, 0), (yellow, 6), (green, 0)],
        "3Signs" => vec![(red, 7), (yellow, 6), (green, 0)],
        "3Symbols" => vec![(red, 10), (yellow, 11), (green, 9)],
        "3Symbols2" => vec![(red, 10), (yellow, 11), (green, 9)],
        "4Arrows" => vec![(red, 3), (yellow, 5), (yellow, 4), (green, 1)],
        "4ArrowsGray" => vec![(gray, 3), (gray, 5), (gray, 4), (gray, 1)],
        "4RedToBlack" => vec![(black, 0), (dark_red, 0), (red, 0), (gray, 0)],
        "4Rating" => vec![(gray, 13), (gray, 14), (gray, 15), (gray, 16)],
        "4TrafficLights" => vec![(black, 0), (red, 0), (yellow, 0), (green, 0)],
        "5Arrows" => vec![(red, 3), (orange, 5), (yellow, 2), (light_green, 4), (green, 1)],
        "5ArrowsGray" => vec![(gray, 3), (gray, 5), (gray, 2), (gray, 4), (gray, 1)],
        "5Rating" => vec![(gray, 13), (gray, 14), (gray, 15), (gray, 16), (gray, 17)],
        "5Quarters" => vec![(black, 18), (black, 19), (black, 20), (black, 21), (black, 22)],
        "5Boxes" => vec![(blue, 12), (blue, 12), (blue, 12), (blue, 12), (blue, 12)],
        _ => {
            let n = set.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(3) as usize;
            let arrows = set.contains("Arrows");
            let shape = |i: usize| -> u8 {
                if !arrows {
                    0
                } else if i == 0 {
                    3
                } else if i == n - 1 {
                    1
                } else {
                    2
                }
            };
            let colors: Vec<Rgb> = match n {
                5 => vec![red, orange, yellow, light_green, green],
                4 => vec![red, yellow, light_green, green],
                _ => vec![red, yellow, green],
            };
            (0..n).map(|i| (if set.contains("Gray") { gray } else { colors[i.min(colors.len() - 1)] }, shape(i))).collect()
        }
    }
}

/// Resolve icon color and shape for a threshold slot, respecting custom individual icons and NoIcon.
fn resolve_icon(set: &str, idx: usize, custom_icons: &[Option<ibis_doc_model::CfIcon>]) -> Option<(Rgb, u8)> {
    if let Some(Some(ci)) = custom_icons.get(idx) {
        if ci.is_no_icon() {
            return None;
        }
        let list = icons_of(&ci.set);
        if ci.id >= 0 && (ci.id as usize) < list.len() {
            return Some(list[ci.id as usize]);
        }
    }
    let default_list = icons_of(set);
    default_list.get(idx).copied()
}

/// The sheet with its conditional formats applied, and what they draw
/// besides; `None` when no rule holds anywhere.
pub(crate) fn apply(sheet: &Sheet) -> Option<(Sheet, Overlay)> {
    if sheet.conditional.is_empty() {
        return None;
    }
    let book = OneSheet(sheet);
    // Per cell: the formats of the rules that hold, by priority.
    let mut styles: HashMap<(u32, u32), Vec<(u32, CfStyle)>> = HashMap::new();
    let mut stopped: HashMap<(u32, u32), u32> = HashMap::new();
    let mut overlay = Overlay::default();
    let mut hide_value: std::collections::HashSet<(u32, u32)> = Default::default();
    let mut rules: Vec<(&ibis_doc_model::ConditionalFormat, &ibis_doc_model::CfRule)> = sheet.conditional.iter().flat_map(|cf| cf.rules.iter().map(move |r| (cf, r))).collect();
    rules.sort_by_key(|(_, r)| r.priority);
    for (cf, rule) in rules {
        let cells = cells_of(sheet, &cf.ranges);
        let Some(anchor) = cf.ranges.first().map(|r| (r.first_row, r.first_column)) else { continue };
        let value = |p: (u32, u32)| book.value(0, p.0, p.1);
        // The range's numbers, sorted: only for the rules that rank or scale
        // by them (a rule by a formula or a comparison needs none).
        let numbers: Vec<f64> = if matches!(rule.kind, CfKind::Expression(_) | CfKind::TimePeriod { .. } | CfKind::CellIs { .. }) {
            Vec::new()
        } else {
            let mut numbers: Vec<f64> = cells.iter().filter_map(|&p| number(&value(p))).collect();
            numbers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            numbers
        };
        let shifted = |f: &str, p: (u32, u32)| ibis_calc::formula::shift(f, p.0 as i64 - anchor.0 as i64, p.1 as i64 - anchor.1 as i64);
        // A formula whose references do not move across the columns (all
        // `$D192`-like) gives the same value along a row: worked out once a
        // row (or once a column, for the other way round). A rule over a
        // whole table is otherwise worked out for every cell (the user's
        // sheet: 30 000 cells × 7 rules, 1.4 s before it was drawn).
        // Functions that read the cell they are in (ROW(), COLUMN(), …)
        // differ along a row whatever their references.
        let placed = |f: &str| {
            let up = f.to_uppercase();
            ["ROW(", "COLUMN(", "CELL(", "INDIRECT(", "OFFSET(", "ADDRESS("].iter().any(|n| up.contains(n))
        };
        let moves: std::cell::RefCell<HashMap<String, (bool, bool)>> = Default::default();
        let still = |f: &str| {
            if let Some(m) = moves.borrow().get(f) {
                return *m;
            }
            let base = ibis_calc::formula::shift(f, 0, 0);
            let m = if placed(f) { (false, false) } else { (base == ibis_calc::formula::shift(f, 0, 1), base == ibis_calc::formula::shift(f, 1, 0)) };
            moves.borrow_mut().insert(f.to_string(), m);
            m
        };
        let memo: std::cell::RefCell<HashMap<(String, u32, u32), CellValue>> = Default::default();
        let eval = |f: &str, p: (u32, u32)| {
            let (row_key, col_key) = match still(f) {
                (true, true) => (u32::MAX, u32::MAX),
                (true, false) => (p.0, u32::MAX),
                (false, true) => (u32::MAX, p.1),
                (false, false) => return ibis_calc::recalc::evaluate(&book, (0, p.0, p.1), &shifted(f, p)).0,
            };
            let key = (f.to_string(), row_key, col_key);
            if let Some(v) = memo.borrow().get(&key) {
                return v.clone();
            }
            let v = ibis_calc::recalc::evaluate(&book, (0, p.0, p.1), &shifted(f, p)).0;
            memo.borrow_mut().insert(key, v.clone());
            v
        };
        let holds: Vec<(u32, u32)> = match &rule.kind {
            CfKind::CellIs { operator, formulas } => cells
                .iter()
                .copied()
                .filter(|&p| {
                    let v = value(p);
                    let args: Vec<CellValue> = formulas.iter().map(|f| eval(f, p)).collect();
                    let cmp = |i: usize| args.get(i).and_then(|a| compare(&v, a));
                    use std::cmp::Ordering::*;
                    match operator.as_str() {
                        "lessThan" => cmp(0) == Some(Less),
                        "lessThanOrEqual" => matches!(cmp(0), Some(Less | Equal)),
                        "equal" => cmp(0) == Some(Equal),
                        "notEqual" => cmp(0).is_some_and(|o| o != Equal),
                        "greaterThanOrEqual" => matches!(cmp(0), Some(Greater | Equal)),
                        "greaterThan" => cmp(0) == Some(Greater),
                        "between" | "notBetween" => {
                            let (lo, hi) = (args.first(), args.get(1));
                            let inside = match (lo, hi) {
                                (Some(a), Some(b)) => {
                                    let (a, b) = if compare(a, b) == Some(Greater) { (b, a) } else { (a, b) };
                                    matches!(compare(&v, a), Some(Greater | Equal)) && matches!(compare(&v, b), Some(Less | Equal))
                                }
                                _ => false,
                            };
                            if operator == "between" { inside } else { !inside && !matches!(v, CellValue::Error(_)) }
                        }
                        _ => false,
                    }
                })
                .collect(),
            CfKind::Expression(f) | CfKind::TimePeriod { formula: f, .. } => cells.iter().copied().filter(|&p| truthy(&eval(f, p))).collect(),
            CfKind::Top10 { rank, percent, bottom } => {
                if numbers.is_empty() {
                    Vec::new()
                } else {
                    let n = if *percent { ((numbers.len() as f64 * *rank as f64 / 100.0).floor() as usize).max(1) } else { (*rank as usize).max(1) };
                    let n = n.min(numbers.len());
                    let threshold = if *bottom { numbers[n - 1] } else { numbers[numbers.len() - n] };
                    cells.iter().copied().filter(|&p| number(&value(p)).is_some_and(|v| if *bottom { v <= threshold } else { v >= threshold })).collect()
                }
            }
            CfKind::AboveAverage { above, equal, std_dev } => {
                if numbers.is_empty() {
                    Vec::new()
                } else {
                    let avg = numbers.iter().sum::<f64>() / numbers.len() as f64;
                    let sd = if numbers.len() > 1 { (numbers.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / (numbers.len() - 1) as f64).sqrt() } else { 0.0 };
                    let line = if *above { avg + *std_dev as f64 * sd } else { avg - *std_dev as f64 * sd };
                    cells
                        .iter()
                        .copied()
                        .filter(|&p| {
                            number(&value(p)).is_some_and(|v| match (*above, *equal) {
                                (true, false) => v > line,
                                (true, true) => v >= line,
                                (false, false) => v < line,
                                (false, true) => v <= line,
                            })
                        })
                        .collect()
                }
            }
            CfKind::Unique | CfKind::Duplicate => {
                let key = |v: &CellValue| match v {
                    CellValue::Text(t) => Some(format!("t{}", t.to_lowercase())),
                    CellValue::Number(n) => Some(format!("n{n}")),
                    CellValue::Bool(b) => Some(format!("b{b}")),
                    _ => None,
                };
                let mut counts: HashMap<String, usize> = HashMap::new();
                for &p in &cells {
                    if let Some(k) = key(&value(p)) {
                        *counts.entry(k).or_default() += 1;
                    }
                }
                let dup = matches!(rule.kind, CfKind::Duplicate);
                cells.iter().copied().filter(|&p| key(&value(p)).and_then(|k| counts.get(&k)).is_some_and(|&n| if dup { n > 1 } else { n == 1 })).collect()
            }
            CfKind::ColorScale(points) => {
                let stops: Vec<(f64, Rgb)> = points.iter().filter_map(|(p, c)| Some((point(&book, p, &numbers, anchor)?, *c))).collect();
                if stops.len() >= 2 {
                    for &p in &cells {
                        if stopped.get(&p).is_some_and(|&s| s < rule.priority) {
                            continue;
                        }
                        let Some(v) = number(&value(p)) else { continue };
                        let color = if v <= stops[0].0 {
                            stops[0].1
                        } else if v >= stops[stops.len() - 1].0 {
                            stops[stops.len() - 1].1
                        } else {
                            let i = stops.windows(2).position(|w| v >= w[0].0 && v <= w[1].0).unwrap_or(0);
                            let (a, b) = (stops[i], stops[i + 1]);
                            let t = if b.0 > a.0 { ((v - a.0) / (b.0 - a.0)) as f32 } else { 0.0 };
                            mix(a.1, b.1, t)
                        };
                        styles.entry(p).or_default().push((rule.priority, CfStyle { fill: Some(color), ..Default::default() }));
                        if rule.stop {
                            stopped.entry(p).or_insert(rule.priority);
                        }
                    }
                }
                Vec::new()
            }
            CfKind::DataBar { min, max, color, gradient, negative_color, axis_color, axis_position, min_length, max_length } => {
                if let (Some(lo), Some(hi)) = (point(&book, min, &numbers, anchor), point(&book, max, &numbers, anchor)) {
                    let neg_color = negative_color.unwrap_or(Rgb(0xD00000));
                    let ax_color = axis_color.unwrap_or(Rgb(0x000000));
                    let min_l = min_length.map(|l| (l as f32 / 100.0).clamp(0.0, 1.0)).unwrap_or(0.1);
                    let max_l = max_length.map(|l| (l as f32 / 100.0).clamp(0.0, 1.0)).unwrap_or(0.9);

                    let (has_axis, axis_pos) = match axis_position.unwrap_or_default() {
                        ibis_doc_model::DataBarAxisPosition::None => (false, 0.0f32),
                        ibis_doc_model::DataBarAxisPosition::Midpoint => (true, 0.5f32),
                        ibis_doc_model::DataBarAxisPosition::Automatic => {
                            if lo < 0.0 && hi > 0.0 {
                                let abs_lo = lo.abs();
                                let pos = (abs_lo / (abs_lo + hi)).clamp(0.1, 0.9) as f32;
                                (true, pos)
                            } else if hi <= 0.0 {
                                (true, 1.0f32)
                            } else {
                                (false, 0.0f32)
                            }
                        }
                    };

                    for &p in &cells {
                        if stopped.get(&p).is_some_and(|&s| s < rule.priority) {
                            continue;
                        }
                        let Some(v) = number(&value(p)) else { continue };

                        if !has_axis || axis_pos == 0.0 {
                            let t = if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 } else { 1.0 };
                            let length = min_l + (max_l - min_l) * t;
                            overlay.bars.push(DataBarMark {
                                cell: p,
                                from: 0.0,
                                to: length,
                                color: *color,
                                gradient: *gradient,
                                is_negative: false,
                                axis: None,
                                axis_color: ax_color,
                            });
                        } else if axis_pos >= 1.0 {
                            let abs_lo = lo.abs().max(1e-9);
                            let ratio = (v.abs() / abs_lo).clamp(0.0, 1.0) as f32;
                            let length = min_l + (max_l - min_l) * ratio;
                            overlay.bars.push(DataBarMark {
                                cell: p,
                                from: 1.0 - length,
                                to: 1.0,
                                color: neg_color,
                                gradient: *gradient,
                                is_negative: true,
                                axis: Some(1.0),
                                axis_color: ax_color,
                            });
                        } else {
                            if v >= 0.0 {
                                let pos_span = 1.0 - axis_pos;
                                let ratio = if hi > 0.0 { (v / hi).clamp(0.0, 1.0) as f32 } else { 0.0 };
                                let length = ratio * pos_span * 0.95;
                                overlay.bars.push(DataBarMark {
                                    cell: p,
                                    from: axis_pos,
                                    to: axis_pos + length,
                                    color: *color,
                                    gradient: *gradient,
                                    is_negative: false,
                                    axis: Some(axis_pos),
                                    axis_color: ax_color,
                                });
                            } else {
                                let neg_span = axis_pos;
                                let abs_lo = lo.abs().max(1e-9);
                                let ratio = (v.abs() / abs_lo).clamp(0.0, 1.0) as f32;
                                let length = ratio * neg_span * 0.95;
                                overlay.bars.push(DataBarMark {
                                    cell: p,
                                    from: axis_pos - length,
                                    to: axis_pos,
                                    color: neg_color,
                                    gradient: *gradient,
                                    is_negative: true,
                                    axis: Some(axis_pos),
                                    axis_color: ax_color,
                                });
                            }
                        }

                        if rule.stop {
                            stopped.entry(p).or_insert(rule.priority);
                        }
                    }
                }
                Vec::new()
            }
            CfKind::IconSet { set, values, reverse, show_value, custom: _, icons: custom_icons } => {
                let default_icons = icons_of(set);
                let count = default_icons.len();
                let thresholds: Vec<(f64, bool)> = values.iter().skip(1).filter_map(|p| Some((point(&book, p, &numbers, anchor)?, p.gte))).collect();
                for &p in &cells {
                    if stopped.get(&p).is_some_and(|&s| s < rule.priority) {
                        continue;
                    }
                    let Some(v) = number(&value(p)) else { continue };
                    let mut i = 0;
                    for (t, gte) in &thresholds {
                        if if *gte { v >= *t } else { v > *t } {
                            i += 1;
                        }
                    }
                    let i = i.min(count.saturating_sub(1));
                    let slot = if *reverse { count.saturating_sub(1).saturating_sub(i) } else { i };
                    if let Some((color, shape)) = resolve_icon(set, slot, custom_icons) {
                        overlay.icons.push((p, color, shape));
                    }
                    if !show_value {
                        hide_value.insert(p);
                    }
                    if rule.stop {
                        stopped.entry(p).or_insert(rule.priority);
                    }
                }
                Vec::new()
            }
            CfKind::Other => Vec::new(),
        };
        for p in holds {
            if stopped.get(&p).is_some_and(|&s| s < rule.priority) {
                continue;
            }
            if let Some(style) = &rule.style {
                styles.entry(p).or_default().push((rule.priority, style.clone()));
            }
            if rule.stop {
                stopped.entry(p).or_insert(rule.priority);
            }
        }
    }
    if styles.is_empty() && overlay.bars.is_empty() && overlay.icons.is_empty() {
        return None;
    }
    let mut out = sheet.clone();
    let mut positions: Vec<(u32, u32)> = styles.keys().chain(hide_value.iter()).copied().collect();
    positions.sort();
    positions.dedup();
    // The cells' new formats, by their own format and the rules that hold
    // (most cells of a table share both: worked out once each). The new
    // cells are put in at the end, sorted, rather than one by one.
    let mut made: HashMap<(u32, String), u32> = HashMap::new();
    let mut changed: Vec<SheetCell> = Vec::with_capacity(positions.len());
    for p in positions {
        let cell = out.cell(p.0, p.1).cloned().unwrap_or(SheetCell { runs: None, phonetic: None, row: p.0, column: p.1, value: CellValue::Empty, text: String::new(), kind: CellKind::Empty, formula: None, style: 0 });
        let mut list = styles.remove(&p).unwrap_or_default();
        list.sort_by_key(|(prio, _)| *prio);
        // (the formats themselves: a colour scale's differ by cell)
        let key = (cell.style, format!("{list:?}"));
        if let Some(&index) = made.get(&key) {
            let style = &out.styles[index as usize];
            let text = if hide_value.contains(&p) { String::new() } else { ibis_doc_model::sheet_command::display_text(&cell.value, style, false) };
            changed.push(SheetCell { style: index, text, ..cell });
            continue;
        }
        let mut style = out.styles.get(cell.style as usize).cloned().unwrap_or_default();
        // The first rule (by priority) to set a property sets it.
        for (_, s) in list.iter().rev() {
            if let Some(c) = s.color {
                style.color = c;
            }
            if let Some(b) = s.bold {
                style.bold = b;
            }
            if let Some(i) = s.italic {
                style.italic = i;
            }
            if let Some(u) = s.underline {
                style.underline = u;
            }
            if let Some(x) = s.strike {
                style.strike = x;
            }
            if let Some(f) = s.fill {
                style.fill = Some(f);
                style.pattern = None;
                style.gradient = None;
            }
            if let Some(pat) = &s.pattern {
                style.pattern = Some(pat.clone());
                style.fill = None;
                style.gradient = None;
            }
            if let Some(grad) = &s.gradient {
                style.gradient = Some(grad.clone());
                style.fill = None;
                style.pattern = None;
            }
            if let Some(va) = s.vert_align {
                style.vert_align = va;
            }
            let [l, r, t, b] = &s.borders;
            if l.is_some() {
                style.left = *l;
            }
            if r.is_some() {
                style.right = *r;
            }
            if t.is_some() {
                style.top = *t;
            }
            if b.is_some() {
                style.bottom = *b;
            }
            if let Some(code) = &s.number_format {
                style.number_format = code.clone();
            }
        }
        let text = if hide_value.contains(&p) { String::new() } else { ibis_doc_model::sheet_command::display_text(&cell.value, &style, false) };
        let index = out.style_index(style);
        made.insert(key, index);
        changed.push(SheetCell { style: index, text, ..cell });
    }
    // Merge the changed cells into the sorted cells.
    let mut by_place: HashMap<(u32, u32), SheetCell> = changed.into_iter().map(|c| ((c.row, c.column), c)).collect();
    for c in out.cells.iter_mut() {
        if let Some(n) = by_place.remove(&(c.row, c.column)) {
            *c = n;
        }
    }
    out.cells.extend(by_place.into_values());
    out.cells.sort_by_key(|c| (c.row, c.column));
    Some((out, overlay))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_doc_model::{CellRange, CellStyle, CellValue, CfKind, CfRule, CfStyle, ConditionalFormat, Rgb, Sheet};

    #[test]
    fn overlapping_expression_priority_and_stop_control_property_merging() {
        let mut sheet = Sheet::default();
        sheet.rows = vec![20]; sheet.columns = vec![80];
        sheet.styles = vec![CellStyle::default()];
        sheet.put_cell(0, 0, Some(ibis_doc_model::SheetCell { runs: None, phonetic: None, row: 0, column: 0, value: CellValue::Number(15.0), text: "15".into(), kind: ibis_doc_model::CellKind::Number, formula: None, style: 0 }));
        let rule = |priority, fill, bold, stop| ConditionalFormat {
            source_index: None,
            ranges: vec![CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 0 }],
            rules: vec![CfRule { kind: CfKind::Expression("A1>10".into()), priority, stop,
                style: Some(CfStyle { fill: Some(Rgb(fill)), bold, ..Default::default() }) }],
        };
        // Stored order differs from priority; the lower-priority rule has an
        // independent property, making stopIfTrue observable beyond fill.
        sheet.conditional = vec![rule(2, 0xC6EFCE, Some(true), false), rule(1, 0xFFC7CE, None, true)];
        let drawn_style = |s: &Sheet| {
            let (drawn, _) = apply(s).unwrap();
            drawn.styles[drawn.cell(0, 0).unwrap().style as usize].clone()
        };
        let stopped = drawn_style(&sheet);
        assert_eq!(stopped.fill, Some(Rgb(0xFFC7CE))); assert!(!stopped.bold);
        sheet.conditional[1].rules[0].stop = false;
        let merged = drawn_style(&sheet);
        assert_eq!(merged.fill, Some(Rgb(0xFFC7CE))); assert!(merged.bold);
    }

    #[test]
    fn color_scale_respects_rule_priority_and_merges_with_font_properties() {
        use ibis_doc_model::CfValue;
        let mut sheet = Sheet::default();
        sheet.rows = vec![20]; sheet.columns = vec![80];
        sheet.styles = vec![CellStyle::default()];
        sheet.put_cell(0, 0, Some(ibis_doc_model::SheetCell { runs: None, phonetic: None, row: 0, column: 0, value: CellValue::Number(10.0), text: "10".into(), kind: ibis_doc_model::CellKind::Number, formula: None, style: 0 }));
        let cs_rule = |priority| ConditionalFormat {
            source_index: None,
            ranges: vec![CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 0 }],
            rules: vec![CfRule {
                kind: CfKind::ColorScale(vec![
                    (CfValue { kind: "min".into(), value: None, gte: true }, Rgb(0x00FF00)),
                    (CfValue { kind: "max".into(), value: None, gte: true }, Rgb(0x0000FF)),
                ]),
                priority,
                stop: false,
                style: None,
            }],
        };
        let fill_rule = |priority, fill, bold| ConditionalFormat {
            source_index: None,
            ranges: vec![CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 0 }],
            rules: vec![CfRule {
                kind: CfKind::Expression("A1>0".into()),
                priority,
                stop: false,
                style: Some(CfStyle { fill: Some(Rgb(fill)), bold, ..Default::default() }),
            }],
        };
        // Case 1: ColorScale (prio 1) wins over rule fill (prio 2), but rule bold merges in.
        sheet.conditional = vec![cs_rule(1), fill_rule(2, 0xFF0000, Some(true))];
        let (drawn1, _) = apply(&sheet).unwrap();
        let s1 = &drawn1.styles[drawn1.cell(0, 0).unwrap().style as usize];
        assert_eq!(s1.fill, Some(Rgb(0x00FF00)));
        assert!(s1.bold);

        // Case 2: Rule fill (prio 1) wins over ColorScale (prio 2).
        sheet.conditional = vec![cs_rule(2), fill_rule(1, 0xFF0000, Some(true))];
        let (drawn2, _) = apply(&sheet).unwrap();
        let s2 = &drawn2.styles[drawn2.cell(0, 0).unwrap().style as usize];
        assert_eq!(s2.fill, Some(Rgb(0xFF0000)));
        assert!(s2.bold);
    }

    /// The cells of one colour scale share the rule but not the colour:
    /// each its own (the formats worked out once per own format and rules
    /// must tell them apart).
    #[test]
    fn color_scale_cells_each_get_their_own_colour() {
        use ibis_doc_model::CfValue;
        let mut sheet = Sheet::default();
        sheet.rows = vec![20]; sheet.columns = vec![80, 80, 80];
        sheet.styles = vec![CellStyle::default()];
        for (c, v) in [1.0, 2.0, 3.0].into_iter().enumerate() {
            sheet.put_cell(0, c as u32, Some(ibis_doc_model::SheetCell { runs: None, phonetic: None, row: 0, column: c as u32, value: CellValue::Number(v), text: v.to_string(), kind: ibis_doc_model::CellKind::Number, formula: None, style: 0 }));
        }
        sheet.conditional = vec![ConditionalFormat {
            source_index: None,
            ranges: vec![CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 2 }],
            rules: vec![CfRule {
                kind: CfKind::ColorScale(vec![
                    (CfValue { kind: "min".into(), value: None, gte: true }, Rgb(0x000000)),
                    (CfValue { kind: "max".into(), value: None, gte: true }, Rgb(0xFFFFFF)),
                ]),
                priority: 1,
                stop: false,
                style: None,
            }],
        }];
        let (drawn, _) = apply(&sheet).unwrap();
        let fills: Vec<_> = (0..3).map(|c| drawn.styles[drawn.cell(0, c).unwrap().style as usize].fill).collect();
        assert_eq!(fills[0], Some(Rgb(0x000000)));
        assert_eq!(fills[2], Some(Rgb(0xFFFFFF)));
        assert!(fills[1] != fills[0] && fills[1] != fills[2]);
    }

    #[test]
    fn higher_priority_solid_fill_clears_pattern() {
        let mut sheet = Sheet::default();
        sheet.rows = vec![20]; sheet.columns = vec![80];
        sheet.styles = vec![CellStyle::default()];
        sheet.put_cell(0, 0, Some(ibis_doc_model::SheetCell { runs: None, phonetic: None, row: 0, column: 0, value: CellValue::Number(10.0), text: "10".into(), kind: ibis_doc_model::CellKind::Number, formula: None, style: 0 }));
        let pat_rule = ConditionalFormat {
            source_index: None,
            ranges: vec![CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 0 }],
            rules: vec![CfRule {
                kind: CfKind::Expression("A1>0".into()),
                priority: 2,
                stop: false,
                style: Some(CfStyle { pattern: Some(("gray125".into(), Rgb(0), Rgb(0xFFFFFF))), ..Default::default() }),
            }],
        };
        let solid_rule = ConditionalFormat {
            source_index: None,
            ranges: vec![CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 0 }],
            rules: vec![CfRule {
                kind: CfKind::Expression("A1>5".into()),
                priority: 1,
                stop: false,
                style: Some(CfStyle { fill: Some(Rgb(0xFFC7CE)), ..Default::default() }),
            }],
        };
        sheet.conditional = vec![pat_rule, solid_rule];
        let (drawn, _) = apply(&sheet).unwrap();
        let s = &drawn.styles[drawn.cell(0, 0).unwrap().style as usize];
        assert_eq!(s.fill, Some(Rgb(0xFFC7CE)));
        assert_eq!(s.pattern, None);
    }

    #[test]
    fn cfvo_threshold_types_evaluation() {
        use ibis_doc_model::CfValue;
        let mut sheet = Sheet::default();
        sheet.rows = vec![20; 5]; sheet.columns = vec![80; 3];
        sheet.styles = vec![CellStyle::default()];

        // Data in A1:A5 = [10.0, 20.0, 30.0, 40.0, 50.0]
        let values = [10.0, 20.0, 30.0, 40.0, 50.0];
        for (r, &v) in values.iter().enumerate() {
            sheet.put_cell(r as u32, 0, Some(ibis_doc_model::SheetCell {
                runs: None, phonetic: None, row: r as u32, column: 0,
                value: CellValue::Number(v), text: v.to_string(),
                kind: ibis_doc_model::CellKind::Number, formula: None, style: 0,
            }));
        }

        // Ancillary cell B1 = 25.0 for formula threshold testing
        sheet.put_cell(0, 1, Some(ibis_doc_model::SheetCell {
            runs: None, phonetic: None, row: 0, column: 1,
            value: CellValue::Number(25.0), text: "25".into(),
            kind: ibis_doc_model::CellKind::Number, formula: None, style: 0,
        }));

        let book = OneSheet(&sheet);

        // 1. Min / Max
        let p_min = CfValue { kind: "min".into(), value: None, gte: true };
        assert_eq!(point(&book, &p_min, &values, (0, 0)), Some(10.0));

        let p_max = CfValue { kind: "max".into(), value: None, gte: true };
        assert_eq!(point(&book, &p_max, &values, (0, 0)), Some(50.0));

        // 2. Num
        let p_num = CfValue { kind: "num".into(), value: Some("35".into()), gte: true };
        assert_eq!(point(&book, &p_num, &values, (0, 0)), Some(35.0));

        // 3. Percent (50% of range [10, 50] = 10 + (50 - 10) * 0.5 = 30.0)
        let p_pct = CfValue { kind: "percent".into(), value: Some("50".into()), gte: true };
        assert_eq!(point(&book, &p_pct, &values, (0, 0)), Some(30.0));

        let p_pct25 = CfValue { kind: "percent".into(), value: Some("25".into()), gte: true };
        assert_eq!(point(&book, &p_pct25, &values, (0, 0)), Some(20.0));

        // 4. Percentile (50th percentile of [10, 20, 30, 40, 50] = 30.0, 75th percentile = 40.0)
        let p_ptile50 = CfValue { kind: "percentile".into(), value: Some("50".into()), gte: true };
        assert_eq!(point(&book, &p_ptile50, &values, (0, 0)), Some(30.0));

        let p_ptile75 = CfValue { kind: "percentile".into(), value: Some("75".into()), gte: true };
        assert_eq!(point(&book, &p_ptile75, &values, (0, 0)), Some(40.0));

        // 5. Formula (evaluated dynamically using book context)
        let p_formula_ref = CfValue { kind: "formula".into(), value: Some("=B1".into()), gte: true };
        assert_eq!(point(&book, &p_formula_ref, &values, (0, 0)), Some(25.0));

        let p_formula_avg = CfValue { kind: "formula".into(), value: Some("=AVERAGE(A1:A5)".into()), gte: true };
        assert_eq!(point(&book, &p_formula_avg, &values, (0, 0)), Some(30.0));

        // 6. Test 3-ColorScale rule with percentile thresholds: 10% (Red), 50% (Yellow), 90% (Green)
        sheet.conditional = vec![ConditionalFormat {
            source_index: None,
            ranges: vec![CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 0 }],
            rules: vec![CfRule {
                kind: CfKind::ColorScale(vec![
                    (CfValue { kind: "percentile".into(), value: Some("10".into()), gte: true }, Rgb(0xF8696B)),
                    (CfValue { kind: "percentile".into(), value: Some("50".into()), gte: true }, Rgb(0xFFEB84)),
                    (CfValue { kind: "percentile".into(), value: Some("90".into()), gte: true }, Rgb(0x63BE7B)),
                ]),
                priority: 1,
                stop: false,
                style: None,
            }],
        }];

        let (drawn, _) = apply(&sheet).unwrap();
        // A1 (10.0) should be Red (0xF8696B)
        let f_a1 = drawn.styles[drawn.cell(0, 0).unwrap().style as usize].fill.unwrap();
        assert_eq!(f_a1, Rgb(0xF8696B));

        // A3 (30.0, 50th percentile) should be Yellow (0xFFEB84)
        let f_a3 = drawn.styles[drawn.cell(2, 0).unwrap().style as usize].fill.unwrap();
        assert_eq!(f_a3, Rgb(0xFFEB84));

        // A5 (50.0, 90th+ percentile) should be Green (0x63BE7B)
        let f_a5 = drawn.styles[drawn.cell(4, 0).unwrap().style as usize].fill.unwrap();
        assert_eq!(f_a5, Rgb(0x63BE7B));
    }
}

/// A sheet with its conditional formats applied (`apply`), shared.
pub(crate) type Applied = Arc<Option<(Sheet, Overlay)>>;

/// The sheets with their conditional formats applied lately, by the
/// sheet's address (the sheet held with it, so the address is not reused
/// while the entry lives): the same at every zoom, and worked out ahead on
/// another thread for a workbook just opened (`apply_ahead`), so changing
/// sheets does not wait for it.
struct AppliedCache {
    done: Vec<(usize, Arc<Sheet>, Applied)>,
    /// Sheets being worked out on the other thread.
    working: Vec<usize>,
}

static APPLIED: Mutex<AppliedCache> = Mutex::new(AppliedCache { done: Vec::new(), working: Vec::new() });
static APPLIED_CHANGED: Condvar = Condvar::new();

/// How many sheets to keep (a workbook's, and a few edited).
const APPLIED_KEPT: usize = 16;

fn applied_cache() -> std::sync::MutexGuard<'static, AppliedCache> {
    APPLIED.lock().unwrap_or_else(|e| e.into_inner())
}

fn keep(cache: &mut AppliedCache, sheet: &Arc<Sheet>, applied: Applied) {
    let key = Arc::as_ptr(sheet) as usize;
    cache.done.retain(|(k, _, _)| *k != key);
    if cache.done.len() >= APPLIED_KEPT {
        cache.done.remove(0);
    }
    cache.done.push((key, sheet.clone(), applied));
}

/// `sheet` with its conditional formats applied, as kept, or worked out
/// now (waiting for the other thread when it is working on it).
pub(crate) fn applied(sheet: &Arc<Sheet>) -> Applied {
    let key = Arc::as_ptr(sheet) as usize;
    let mut cache = applied_cache();
    loop {
        if let Some(i) = cache.done.iter().position(|(k, _, _)| *k == key) {
            let entry = cache.done.remove(i);
            let applied = entry.2.clone();
            cache.done.push(entry);
            return applied;
        }
        if !cache.working.contains(&key) {
            break;
        }
        cache = APPLIED_CHANGED.wait(cache).unwrap_or_else(|e| e.into_inner());
    }
    drop(cache);
    let applied: Applied = Arc::new(apply(sheet));
    keep(&mut applied_cache(), sheet, applied.clone());
    applied
}

/// Work out the conditional formats of `sheets` on another thread, the
/// first first (the browser, which has no threads here, works each out
/// when it is first drawn).
pub(crate) fn apply_ahead(sheets: Vec<Arc<Sheet>>) {
    #[cfg(not(target_family = "wasm"))]
    {
        let sheets: Vec<Arc<Sheet>> = sheets.into_iter().filter(|s| !s.conditional.is_empty()).collect();
        if sheets.is_empty() {
            return;
        }
        std::thread::spawn(move || {
            for sheet in sheets {
                let key = Arc::as_ptr(&sheet) as usize;
                {
                    let mut cache = applied_cache();
                    if cache.done.iter().any(|(k, _, _)| *k == key) {
                        continue;
                    }
                    cache.working.push(key);
                }
                // A sheet that cannot be worked out here is left to the
                // drawing, which then works it out itself.
                let applied = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| apply(&sheet)));
                let mut cache = applied_cache();
                cache.working.retain(|k| *k != key);
                if let Ok(applied) = applied {
                    keep(&mut cache, &sheet, Arc::new(applied));
                }
                drop(cache);
                APPLIED_CHANGED.notify_all();
            }
        });
    }
    #[cfg(target_family = "wasm")]
    let _ = sheets;
}
