// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スライサー (slicers): a box of buttons, one for each item of a table's
//! column or a pivot table's field, that filters it (ECMA-376 has no slicer;
//! MS-XLSX §2.6.x `slicer`, `slicerCacheDefinition`, and MS-ODRAWXML
//! `sle:slicer` for the drawing).
//!
//! A slicer is a floating shape on a sheet ([`crate::ShapeKind::Slicer`]).
//! What it filters is found by name in the whole workbook, and its items
//! are a snapshot of that source's state that [`refresh_slicers`] makes
//! again after every command, so the slicer shows what the filter keeps.

use std::collections::HashSet;

use crate::model::{Presentation, ShapeKind};
use crate::sheet::{AutoFilter, CellValue, ColumnFilter, Sheet};

/// Where a slicer's items come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlicerSource {
    /// A table's column (`x15:tableSlicerCache`): the table's name and the
    /// column's heading (`sourceName`).
    Table { table: String, field: String },
    /// A field of pivot tables (`pivotTables` of the cache): the field's
    /// name and the tables (by name) the slicer filters together.
    Pivot { field: String, pivots: Vec<String> },
}

/// A button of a slicer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlicerItem {
    pub label: String,
    /// Kept by the filter (a selected button).
    pub selected: bool,
    /// Some row or record with the item is left by the other filters (else
    /// the button is dimmed).
    pub has_data: bool,
}

/// A slicer (`x14:slicer`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slicer {
    /// Its name (`name`), the one the drawing names too.
    pub name: String,
    /// The heading (`caption`).
    pub caption: String,
    /// Its cache's name (`cache`): slicers with one cache keep one selection.
    pub cache: String,
    /// Its style (`style`, `SlicerStyleLight1` …); empty: the default.
    pub style: String,
    /// The buttons across (`columnCount`).
    pub columns: u32,
    /// A button's height in EMU (`rowHeight`).
    pub row_height: i64,
    /// Whether the heading shows (`showCaption`).
    pub show_caption: bool,
    pub source: SlicerSource,
    pub items: Vec<SlicerItem>,
}

impl Slicer {
    /// Excel's button height when the file gives none (19 pt).
    pub const DEFAULT_ROW_HEIGHT: i64 = 241_300;

    pub fn button_height(&self) -> i64 {
        if self.row_height > 0 { self.row_height } else { Self::DEFAULT_ROW_HEIGHT }
    }
}

fn sheets(presentation: &Presentation) -> impl Iterator<Item = &Sheet> {
    presentation.slides.iter().flat_map(|s| s.shapes.iter()).filter_map(|sh| match &sh.kind {
        ShapeKind::Sheet(sheet) => Some(&**sheet),
        _ => None,
    })
}

/// The table named `name` (formulas are not case sensitive) and its sheet.
pub fn find_table<'a>(presentation: &'a Presentation, name: &str) -> Option<(&'a Sheet, usize)> {
    sheets(presentation).find_map(|s| s.tables.iter().position(|t| t.name.eq_ignore_ascii_case(name)).map(|i| (s, i)))
}

/// The pivot table named `name`, its sheet, and its place on it.
pub fn find_pivot<'a>(presentation: &'a Presentation, name: &str) -> Option<(&'a Sheet, usize)> {
    sheets(presentation).find_map(|s| s.pivots.iter().position(|p| p.name == name).map(|i| (s, i)))
}

/// The slide of the sheet holding the table `name`, and its index there.
pub fn locate_table(presentation: &Presentation, name: &str) -> Option<(crate::SlideId, usize)> {
    presentation.slides.iter().find_map(|slide| {
        slide.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Sheet(s) => s.tables.iter().position(|t| t.name.eq_ignore_ascii_case(name)).map(|i| (slide.id, i)),
            _ => None,
        })
    })
}

/// The slide of the sheet holding the pivot table `name`, and its index there.
pub fn locate_pivot(presentation: &Presentation, name: &str) -> Option<(crate::SlideId, usize)> {
    presentation.slides.iter().find_map(|slide| {
        slide.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Sheet(s) => s.pivots.iter().position(|p| p.name == name).map(|i| (slide.id, i)),
            _ => None,
        })
    })
}

/// Which items a click on item `clicked` leaves selected, given the
/// selection `now`: a plain click selects that item alone (or all again if
/// it is the only one selected); with ⌘/Ctrl it toggles the item (none left
/// selected means all); with Shift it selects the items from the first
/// selected one to the clicked one.
pub fn select_after_click(now: &[bool], clicked: usize, toggle: bool, range: bool) -> Vec<bool> {
    let all = vec![true; now.len()];
    if clicked >= now.len() {
        return now.to_vec();
    }
    let out: Vec<bool> = if range {
        let from = now.iter().position(|&s| s).filter(|_| now.iter().any(|&s| !s)).unwrap_or(clicked);
        let (a, b) = (from.min(clicked), from.max(clicked));
        (0..now.len()).map(|i| (a..=b).contains(&i)).collect()
    } else if toggle {
        let mut v = now.to_vec();
        v[clicked] = !v[clicked];
        v
    } else if now.iter().filter(|&&s| s).count() == 1 && now[clicked] {
        all.clone()
    } else {
        (0..now.len()).map(|i| i == clicked).collect()
    };
    if out.iter().any(|&s| s) { out } else { all }
}

/// The items of `source` as the filter has them now.
pub fn slicer_items(presentation: &Presentation, source: &SlicerSource) -> Vec<SlicerItem> {
    let d1904 = presentation.workbook.as_ref().is_some_and(|w| w.date1904);
    match source {
        SlicerSource::Table { table, field } => {
            let Some((sheet, i)) = find_table(presentation, table) else { return Vec::new() };
            let Some(column) = table_column(&sheet.tables[i], field) else { return Vec::new() };
            table_items(sheet, i, column, d1904)
        }
        SlicerSource::Pivot { field, pivots } => {
            let Some((sheet, i)) = pivots.iter().find_map(|n| find_pivot(presentation, n)) else { return Vec::new() };
            pivot_items(&sheet.pivots[i], field)
        }
    }
}

/// The sheet's column of the table's column headed `field`.
pub fn table_column(table: &crate::sheet::SheetTable, field: &str) -> Option<u32> {
    table.columns.iter().position(|c| c == field).map(|i| table.range.first_column + i as u32)
}

fn table_items(sheet: &Sheet, index: usize, column: u32, date1904: bool) -> Vec<SlicerItem> {
    let t = &sheet.tables[index];
    let range = crate::CellRange { last_row: t.range.last_row.saturating_sub(t.totals_rows), ..t.range };
    let filter = t.filter.clone().unwrap_or(AutoFilter { range, columns: Vec::new() });
    let others = AutoFilter { range: filter.range, columns: filter.columns.iter().filter(|(c, _)| *c != column).cloned().collect() };
    let hidden_by_others = crate::sheet_command::filtered_rows(sheet, &others, date1904);
    let current = filter.columns.iter().find(|(c, _)| *c == column).map(|(_, f)| f);
    let last = filter.range.last_row.min(sheet.rows.len().saturating_sub(1) as u32);
    let mut seen: HashSet<String> = HashSet::new();
    // (rank, number, sort key, text, has data, kept)
    let mut keyed: Vec<(u8, f64, String, String, bool, bool)> = Vec::new();
    for r in (filter.range.first_row + 1)..=last {
        let cell = sheet.cell(r, column);
        let text = cell.map_or(String::new(), |c| c.text.clone());
        let (rank, n) = match cell.map(|c| &c.value) {
            Some(CellValue::Number(n)) => (0, *n),
            _ if text.is_empty() => (3, 0.0),
            Some(CellValue::Bool(_)) => (2, 0.0),
            _ => (1, 0.0),
        };
        let has_data = !hidden_by_others.contains(&r);
        let kept = match current {
            None => true,
            Some(ColumnFilter::Values(v)) => v.iter().any(|x| x.to_lowercase() == text.to_lowercase()),
            Some(_) => !sheet.filtered_rows.contains(&r),
        };
        if seen.insert(text.clone()) {
            keyed.push((rank, n, ibis_calc::text_order::sort_key(&text, None), text, has_data, kept));
        } else if let Some(k) = keyed.iter_mut().find(|k| k.3 == text) {
            k.4 |= has_data;
        }
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
    keyed.into_iter().map(|k| SlicerItem { label: k.3, selected: k.5, has_data: k.4 }).collect()
}

fn pivot_items(p: &crate::pivot::PivotTable, field: &str) -> Vec<SlicerItem> {
    let Some(f) = p.fields.iter().find(|f| f.name == field) else { return Vec::new() };
    f.items
        .iter()
        .filter(|it| it.kind.is_none())
        .filter_map(|it| {
            let v = f.shared.get(it.x? as usize)?;
            let label = crate::sheet_command::display_text(v, &Default::default(), false);
            Some(SlicerItem { label, selected: !it.hidden, has_data: true })
        })
        .collect()
}

/// Where a slicer's parts are, in EMU from its top-left corner.
pub struct SlicerLayout {
    /// The heading's height (0: none).
    pub header: i64,
    /// The area of the buttons: left, top, width, height.
    pub body: (i64, i64, i64, i64),
    pub gap: i64,
    pub columns: u32,
    pub button_width: i64,
    pub button_height: i64,
}

pub const PAD: i64 = 3 * crate::geometry::EMU_PER_POINT;
pub const HEADER: i64 = 24 * crate::geometry::EMU_PER_POINT;

pub fn layout(s: &Slicer, width: i64, height: i64) -> SlicerLayout {
    let header = if s.show_caption { HEADER } else { 0 };
    let gap = crate::geometry::EMU_PER_POINT * 3 / 2;
    let columns = s.columns.max(1);
    let body = (PAD, header + PAD, (width - 2 * PAD).max(0), (height - header - 2 * PAD).max(0));
    let button_width = ((body.2 - gap * (columns as i64 - 1)) / columns as i64).max(0);
    SlicerLayout { header, body, gap, columns, button_width, button_height: s.button_height() }
}

/// Whether the point is on the heading's clear-filter icon (the heading's
/// right end).
pub fn clear_icon_at(s: &Slicer, width: i64, x: i64, y: i64) -> bool {
    s.show_caption && y >= 0 && y < HEADER && x >= width - HEADER && x < width
}

/// The button at the point (`x`, `y` in EMU from the slicer's top-left
/// corner), as an index into the items.
pub fn button_at(s: &Slicer, width: i64, height: i64, x: i64, y: i64) -> Option<usize> {
    let l = layout(s, width, height);
    let (bx, by) = (x - l.body.0, y - l.body.1);
    if bx < 0 || by < 0 || bx >= l.body.2 || by >= l.body.3 {
        return None;
    }
    let col = bx / (l.button_width + l.gap).max(1);
    let row = by / (l.button_height + l.gap).max(1);
    if col >= l.columns as i64 || bx % (l.button_width + l.gap).max(1) >= l.button_width || by % (l.button_height + l.gap).max(1) >= l.button_height {
        return None;
    }
    let i = (row * l.columns as i64 + col) as usize;
    (i < s.items.len()).then_some(i)
}


/// Make the items of every slicer again from its source.
pub fn refresh_slicers(presentation: &mut Presentation) {
    if !presentation.slides.iter().any(|s| s.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Slicer(_)))) {
        return;
    }
    let mut updates: Vec<(usize, usize, Vec<SlicerItem>)> = Vec::new();
    for (si, slide) in presentation.slides.iter().enumerate() {
        for (hi, shape) in slide.shapes.iter().enumerate() {
            if let ShapeKind::Slicer(s) = &shape.kind {
                let items = slicer_items(presentation, &s.source);
                if items != s.items {
                    updates.push((si, hi, items));
                }
            }
        }
    }
    for (si, hi, items) in updates {
        if let ShapeKind::Slicer(s) = &mut presentation.slides[si].shapes[hi].kind {
            std::sync::Arc::make_mut(s).items = items;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slicer(columns: u32, n: usize) -> Slicer {
        Slicer {
            name: "地域".into(),
            caption: "地域".into(),
            cache: "Slicer_地域".into(),
            style: String::new(),
            columns,
            row_height: 0,
            show_caption: true,
            source: SlicerSource::Table { table: "T".into(), field: "地域".into() },
            items: (0..n).map(|i| SlicerItem { label: format!("項目{i}"), selected: true, has_data: true }).collect(),
        }
    }

    #[test]
    fn clicks_select_items_as_excel_does() {
        let all = [true; 4];
        assert_eq!(select_after_click(&all, 1, false, false), [false, true, false, false]);
        // Clicking the only selected item shows everything again.
        assert_eq!(select_after_click(&[false, true, false, false], 1, false, false), all);
        // Another item alone replaces the selection.
        assert_eq!(select_after_click(&[false, true, false, false], 3, false, false), [false, false, false, true]);
        // ⌘-click adds or takes away one.
        assert_eq!(select_after_click(&[false, true, false, false], 3, true, false), [false, true, false, true]);
        assert_eq!(select_after_click(&[false, true, false, true], 1, true, false), [false, false, false, true]);
        assert_eq!(select_after_click(&[false, true, false, false], 1, true, false), all);
        assert_eq!(select_after_click(&all, 2, true, false), [true, true, false, true]);
        // Shift-click: from the first selected to the clicked one.
        assert_eq!(select_after_click(&[false, true, false, false], 3, false, true), [false, true, true, true]);
    }

    #[test]
    fn a_click_finds_the_button_under_it() {
        let s = slicer(2, 5);
        let (w, h) = (2_000_000, 2_000_000);
        let l = layout(&s, w, h);
        for i in 0..5usize {
            let (col, row) = (i as i64 % 2, i as i64 / 2);
            let x = l.body.0 + col * (l.button_width + l.gap) + l.button_width / 2;
            let y = l.body.1 + row * (l.button_height + l.gap) + l.button_height / 2;
            assert_eq!(button_at(&s, w, h, x, y), Some(i));
        }
        // The heading, the gap between buttons and the empty place after the last.
        assert_eq!(button_at(&s, w, h, 100_000, 100_000), None);
        assert_eq!(button_at(&s, w, h, l.body.0 + l.button_width + l.gap / 2, l.body.1 + 10), None);
        let x = l.body.0 + l.button_width + l.gap + 10;
        let y = l.body.1 + 2 * (l.button_height + l.gap) + 10;
        assert_eq!(button_at(&s, w, h, x, y), None);
    }
}
