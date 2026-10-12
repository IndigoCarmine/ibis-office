// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > グラフ in a workbook: a chart of the selected cells (or of the
//! table around the selected cell), its series in columns or rows as
//! Excel takes them (more rows than columns: in columns), placed at the
//! selected cell.

use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, EmuRect};
use ibis_pptx::NewChartKind;

use crate::shell::chrome::{MenuEntry, MenuItem};
use crate::shell::editor::SlideEditor;

/// The kinds of chart 挿入 > グラフ offers on a sheet, in menu order: the
/// item's id, its icon and the kind (`None` for a separator).
pub(crate) fn chart_items() -> Vec<MenuEntry> {
    let item = |id: &'static str, icon: &'static str, kind: NewChartKind| -> MenuEntry {
        MenuItem::tr(id).icon(icon).on(move |this: &mut SlideEditor, window, cx| this.insert_sheet_chart(kind, window, cx)).into()
    };
    vec![
        item("pptx.insert-chart.column", "bar_chart", NewChartKind::ClusteredColumn),
        item("pptx.insert-chart.stacked-column", "bar_chart", NewChartKind::StackedColumn),
        item("pptx.insert-chart.percent-stacked-column", "bar_chart", NewChartKind::PercentStackedColumn),
        MenuEntry::Separator,
        item("pptx.insert-chart.bar", "bar_chart", NewChartKind::ClusteredBar),
        item("pptx.insert-chart.stacked-bar", "bar_chart", NewChartKind::StackedBar),
        item("pptx.insert-chart.percent-stacked-bar", "bar_chart", NewChartKind::PercentStackedBar),
        MenuEntry::Separator,
        item("pptx.insert-chart.line", "show_chart", NewChartKind::Line),
        item("pptx.insert-chart.stacked-line", "show_chart", NewChartKind::StackedLine),
        item("pptx.insert-chart.percent-stacked-line", "show_chart", NewChartKind::PercentStackedLine),
        MenuEntry::Separator,
        item("pptx.insert-chart.area", "area_chart", NewChartKind::Area),
        item("pptx.insert-chart.stacked-area", "area_chart", NewChartKind::StackedArea),
        item("pptx.insert-chart.percent-stacked-area", "area_chart", NewChartKind::PercentStackedArea),
        MenuEntry::Separator,
        item("pptx.insert-chart.pie", "pie_chart", NewChartKind::Pie),
        item("pptx.insert-chart.doughnut", "donut_large", NewChartKind::Doughnut),
        MenuEntry::Separator,
        item("pptx.insert-chart.scatter", "polyline", NewChartKind::Scatter),
        item("pptx.insert-chart.scatter-smooth", "polyline", NewChartKind::ScatterSmooth),
        item("pptx.insert-chart.scatter-lines", "polyline", NewChartKind::ScatterLines),
        item("pptx.insert-chart.bubble", "dataset", NewChartKind::Bubble),
    ]
}

impl SlideEditor {
    /// The cells a chart is made of: the selected range, or (a single cell
    /// selected) the table around it.
    fn chart_source_range(&self) -> Option<CellRange> {
        let range = self.selected_range()?;
        if range.first_row == range.last_row && range.first_column == range.last_column {
            return self.current_region();
        }
        Some(range)
    }

    /// 挿入 > グラフ: a chart of `kind` over the selected cells, below them.
    pub(crate) fn insert_sheet_chart(&mut self, kind: NewChartKind, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(source) = self.source.clone() else { return };
        let Some(range) = self.chart_source_range() else { return };
        let slide = self.current_slide;
        let Some(name) = self.presentation().slide(slide).and_then(|s| s.name.clone()) else { return };
        let sheet = ibis_pptx::quote_sheet(&name);
        let current = self.presentation().clone();
        // Excel: more rows than columns, the series in columns.
        let rows = range.last_row - range.first_row;
        let columns = range.last_column - range.first_column;
        let by_columns = rows >= columns;
        let scatter = matches!(kind, NewChartKind::Scatter | NewChartKind::ScatterSmooth | NewChartKind::ScatterLines);
        // A scatter chart: the first column the X values, a heading row of text above numbers.
        let bubble = matches!(kind, NewChartKind::Bubble);
        let refs = if bubble {
            // X, Y and size: three columns of numbers under headings.
            ibis_pptx::bubble_refs_for_range(&current, &sheet, range)
        } else {
            if scatter { ibis_pptx::scatter_refs_for_range(&current, &sheet, range) } else { None }
                .or_else(|| ibis_pptx::refs_for_range(&current, &sheet, range, by_columns))
                .or_else(|| ibis_pptx::refs_for_range(&current, &sheet, range, !by_columns))
        };
        let Some(refs) = refs else {
            self.status = Some(crate::shell::i18n::text("xlsx.chart.no-data").to_string().into());
            cx.notify();
            return;
        };
        // Below the data, as wide as five columns of it can be.
        let (x, _) = self.sheet_insertion_origin();
        let y = self.current_sheet().map_or(952_500, |(shape, sheet)| shape.frame.y + sheet.row_offsets().get(range.last_row as usize + 1).copied().unwrap_or(0) + 190_500);
        // Not on a chart already there: a half inch down and right of it, as Excel cascades them.
        let (mut x, mut y) = (x, y);
        while self.slide().shapes.iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_)) && (s.frame.x - x).abs() < 100_000 && (s.frame.y - y).abs() < 100_000) {
            x += 457_200;
            y += 457_200;
        }
        let frame = EmuRect::new(x, y, 4_572_000, 2_743_200);
        let id = self.document.ids().new_shape_id();
        let shape = match source.new_sheet_chart(&current, slide, id, frame, kind, refs) {
            Ok(shape) => shape,
            Err(err) => {
                log::warn!("Cannot insert the chart: {err}");
                self.status = Some(crate::shell::i18n::format("_xlsx.this-operation-can-t-be-done-err", &[("err", &err.to_string())]).into());
                cx.notify();
                return;
            }
        };
        let index = self.slide().shapes.len();
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.selection = Some(id);
        let _ = window;
        cx.notify();
    }
}
