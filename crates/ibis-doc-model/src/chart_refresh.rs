// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The charts of a workbook follow its cells: after an edit each chart that reads cells (`Chart::cell_refs`) takes its
//! series' names, categories and values from them again, as Excel redraws a chart when the cells it plots change.

use std::sync::Arc;

use crate::chart::{Chart, Number, SeriesCells};
use crate::sheet::Sheet;
use crate::sheet_calc::sheet_of_slide;
use crate::{CellValue, Presentation, ShapeKind};

/// A sheet and a range: `Sheet1!$A$2:$A$5`, `'Sheet 1'!B2`.
fn parse(reference: &str) -> Option<(String, (u32, u32, u32, u32))> {
    let (sheet, range) = reference.trim_start_matches('=').rsplit_once('!')?;
    let name = sheet.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')).map_or_else(|| sheet.to_string(), |s| s.replace("''", "'"));
    let (first, last) = range.split_once(':').unwrap_or((range, range));
    let (c0, r0) = address(first)?;
    let (c1, r1) = address(last)?;
    Some((name, (r0.min(r1), c0.min(c1), r0.max(r1), c0.max(c1))))
}

/// A cell address `$B$12` as (column, row), 0-based.
fn address(text: &str) -> Option<(u32, u32)> {
    let text = text.replace('$', "");
    let split = text.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = text.split_at(split);
    if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let column = letters.to_ascii_uppercase().bytes().fold(0u32, |n, b| n * 26 + (b - b'A' + 1) as u32).checked_sub(1)?;
    let row: u32 = digits.parse().ok()?;
    Some((column, row.checked_sub(1)?))
}

/// What the cells of `reference` show, in order (down a column, then across); cells of hidden rows and columns left
/// out when `visible_only`. `None` where the reference names no sheet of the workbook.
fn cells<'a>(sheets: &'a [(String, Arc<Sheet>)], reference: &str, visible_only: bool) -> Option<Vec<Option<&'a crate::sheet::SheetCell>>> {
    let (name, (r0, c0, r1, c1)) = parse(reference)?;
    let sheet = sheets.iter().find(|(n, _)| *n == name || n.to_lowercase() == name.to_lowercase()).map(|(_, s)| s)?;
    if (r1 - r0 + 1) as u64 * (c1 - c0 + 1) as u64 > 200_000 {
        return None;
    }
    let mut out = Vec::new();
    for column in c0..=c1 {
        if visible_only && sheet.hidden_columns.contains_key(&column) {
            continue;
        }
        for row in r0..=r1 {
            if visible_only && (sheet.hidden_rows.contains_key(&row) || sheet.filtered_rows.contains(&row)) {
                continue;
            }
            out.push(sheet.cell(row, column));
        }
    }
    Some(out)
}

fn shown(cell: Option<&crate::sheet::SheetCell>) -> String {
    cell.map(|c| c.text.clone()).unwrap_or_default()
}

fn number(cell: Option<&crate::sheet::SheetCell>) -> Option<Number> {
    match cell.map(|c| &c.value) {
        Some(CellValue::Number(v)) => Some(Number(*v)),
        _ => None,
    }
}

/// `chart` read again from the cells its series name, `None` if nothing changed.
fn refreshed(chart: &Chart, sheets: &[(String, Arc<Sheet>)]) -> Option<Chart> {
    let total: usize = chart.groups.iter().map(|g| g.series.len()).sum();
    if chart.ex.is_some() || chart.pivot_source.is_some() || chart.cell_refs.len() != total {
        return None;
    }
    let mut out = chart.clone();
    let scatter: Vec<bool> = out.groups.iter().flat_map(|g| g.series.iter().map(|_| matches!(g.kind, crate::chart::ChartType::Scatter { .. } | crate::chart::ChartType::Bubble { .. }))).collect();
    let refs: &[SeriesCells] = &chart.cell_refs;
    for ((series, r), is_scatter) in out.groups.iter_mut().flat_map(|g| g.series.iter_mut()).zip(refs).zip(scatter) {
        if let Some(cells) = r.name.as_deref().and_then(|f| cells(sheets, f, false)) {
            series.name = cells.into_iter().map(shown).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
        }
        if let Some(cells) = r.categories.as_deref().and_then(|f| cells(sheets, f, chart.plot_vis_only)) {
            if is_scatter {
                series.x_values = cells.into_iter().map(number).collect();
            } else {
                series.categories = cells.into_iter().map(shown).collect();
            }
        }
        if let Some(cells) = r.values.as_deref().and_then(|f| cells(sheets, f, chart.plot_vis_only)) {
            series.values = cells.into_iter().map(number).collect();
        }
        if let Some(cells) = r.sizes.as_deref().and_then(|f| cells(sheets, f, chart.plot_vis_only)) {
            series.bubble_sizes = cells.into_iter().map(number).collect();
        }
    }
    (out != *chart).then_some(out)
}

/// The cells of `range` of the sheet named (in lower case) `name`: text, number and number format of each.
fn grid(sheets: &[(String, Arc<Sheet>)], name: &str, range: crate::CellRange) -> Option<crate::chart::ExCells> {
    let (_, sheet) = sheets.iter().find(|(n, _)| n.to_lowercase() == name.to_lowercase())?;
    let last_row = range.last_row.min(sheet.cells.iter().map(|c| c.row).max().unwrap_or(0));
    let last_column = range.last_column.min(sheet.cells.iter().map(|c| c.column).max().unwrap_or(0));
    let mut out = Vec::new();
    for row in range.first_row..=last_row {
        let mut line = Vec::new();
        for column in range.first_column..=last_column {
            line.push(match sheet.cell(row, column) {
                Some(c) => (c.text.clone(), if let CellValue::Number(n) = c.value { Some(n) } else { None }, sheet.styles.get(c.style as usize).map(|s| s.number_format.clone()).unwrap_or_else(|| "General".into())),
                None => (String::new(), None, "General".into()),
            });
        }
        out.push(line);
    }
    Some(out)
}

/// A chartEx chart made again from its cells; what the user set on it (title, legend, …) stays.
fn refreshed_ex(chart: &Chart, sheets: &[(String, Arc<Sheet>)]) -> Option<Chart> {
    let source = chart.ex_source.as_ref().filter(|s| s.1)?;
    let new = source.0.rebuild(&|name, range| grid(sheets, name, range))?;
    let mut out = chart.clone();
    out.groups = new.groups;
    out.axes = new.axes;
    out.ex = new.ex;
    (out != *chart).then_some(out)
}

/// The sheets of the workbook with their names.
fn sheets_of(presentation: &Presentation) -> Vec<(String, Arc<Sheet>)> {
    let mut sheets: Vec<(String, Arc<Sheet>)> = Vec::new();
    let Some(workbook) = presentation.workbook.as_ref() else { return sheets };
    for ws in &workbook.sheets {
        let sheet = match ws.slide {
            Some(id) => presentation.slides.iter().find(|s| s.id == id).and_then(sheet_of_slide).cloned(),
            None => ws.hidden.clone(),
        };
        if let Some(sheet) = sheet {
            sheets.push((ws.name.clone(), sheet));
        }
    }
    sheets
}

/// A chartEx `chart` made again from the cells of the workbook now (after its settings changed), as it is.
pub fn rebuilt_ex(presentation: &Presentation, chart: &Chart) -> Chart {
    let sheets = sheets_of(presentation);
    refreshed_ex(chart, &sheets).unwrap_or_else(|| chart.clone())
}

/// Every chart of a sheet of the workbook that reads cells, drawn from them again.
pub fn refresh_charts(presentation: &mut Presentation) {
    if presentation.workbook.is_none() {
        return;
    }
    // Quick out: no chart that follows cells.
    let any = presentation.slides.iter().flat_map(|s| &s.shapes).any(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if !c.cell_refs.is_empty() || c.ex_source.is_some()));
    if !any {
        return;
    }
    let sheets = sheets_of(presentation);
    let mut changes: Vec<(usize, usize, Chart)> = Vec::new();
    for (si, slide) in presentation.slides.iter().enumerate() {
        for (hi, shape) in slide.shapes.iter().enumerate() {
            if let ShapeKind::Chart(chart) = &shape.kind {
                if let Some(new) = refreshed(chart, &sheets).or_else(|| refreshed_ex(chart, &sheets)) {
                    changes.push((si, hi, new));
                }
            }
        }
    }
    for (si, hi, chart) in changes {
        presentation.slides[si].shapes[hi].kind = ShapeKind::Chart(Box::new(chart));
    }
}

/// The charts of a sheet that a file wrote without the data they draw (no `c:numCache` or `c:strCache`, as some programs
/// leave them out), read from the cells they name; the other charts are left as the file has them.
pub fn fill_charts_without_data(presentation: &mut Presentation) {
    if presentation.workbook.is_none() {
        return;
    }
    let empty = |c: &Chart| {
        c.cell_refs.iter().zip(c.groups.iter().flat_map(|g| &g.series)).any(|(r, s)| (r.values.is_some() && s.values.iter().all(Option::is_none)) || (r.categories.is_some() && s.categories.is_empty() && s.x_values.is_empty()))
    };
    let wanted = presentation.slides.iter().flat_map(|s| &s.shapes).any(|sh| matches!(&sh.kind, ShapeKind::Chart(c) if empty(c)));
    if !wanted {
        return;
    }
    let sheets = sheets_of(presentation);
    let mut changes: Vec<(usize, usize, Chart)> = Vec::new();
    for (si, slide) in presentation.slides.iter().enumerate() {
        for (hi, shape) in slide.shapes.iter().enumerate() {
            if let ShapeKind::Chart(chart) = &shape.kind
                && empty(chart)
                && let Some(new) = refreshed(chart, &sheets)
            {
                changes.push((si, hi, new));
            }
        }
    }
    for (si, hi, chart) in changes {
        presentation.slides[si].shapes[hi].kind = ShapeKind::Chart(Box::new(chart));
    }
}

/// Rewrite all chart cell references (`cell_refs` and `ex_source`) in `presentation` using `f`.
pub fn rewrite_chart_references(presentation: &mut Presentation, f: &dyn Fn(&str) -> String) {
    for slide in &mut presentation.slides {
        for shape in &mut slide.shapes {
            if let ShapeKind::Chart(chart) = &mut shape.kind {
                for r in &mut chart.cell_refs {
                    for slot in [&mut r.name, &mut r.categories, &mut r.values, &mut r.sizes] {
                        if let Some(s) = slot.as_mut() {
                            let next = f(s);
                            if next != *s {
                                *s = next;
                            }
                        }
                    }
                }
                if let Some(source) = chart.ex_source.as_ref() {
                    if let Some(mut next_source) = source.0.rewrite_references(f) {
                        next_source.1 = source.1;
                        chart.ex_source = Some(next_source);
                    }
                }
            }
        }
    }
}

/// The charts of the workbook no longer read cells (what they show stays as it is).
pub fn unlink_charts(presentation: &mut Presentation) {
    for slide in &mut presentation.slides {
        for shape in &mut slide.shapes {
            if let ShapeKind::Chart(chart) = &mut shape.kind {
                if !chart.cell_refs.is_empty() {
                    chart.cell_refs.clear();
                }
                // (A chartEx chart keeps its source, for the settings written into its part, but stops following.)
                if let Some(source) = chart.ex_source.as_mut() {
                    source.1 = false;
                }
            }
        }
    }
}
