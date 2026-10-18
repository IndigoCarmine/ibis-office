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
    let mut items = vec![
        MenuItem::tr("xlsx.recommended-charts").icon("auto_graph").on(|this: &mut SlideEditor, window, cx| this.open_recommended_charts(window, cx)).into(),
        MenuEntry::Separator,
    ];
    items.extend(chart_items_with(SlideEditor::insert_sheet_chart));
    // The kinds of Excel 2016 (chartEx), a workbook's only: over the selected cells.
    let item = |id: &'static str, icon: &'static str, kind: NewChartKind| -> MenuEntry {
        MenuItem::tr(id).icon(icon).on(move |this: &mut SlideEditor, window, cx| this.insert_sheet_chart(kind, window, cx)).into()
    };
    items.push(MenuEntry::Separator);
    items.push(item("pptx.insert-chart.waterfall", "waterfall_chart", NewChartKind::Waterfall));
    items.push(item("pptx.insert-chart.histogram", "bar_chart", NewChartKind::Histogram));
    items.push(item("pptx.insert-chart.pareto", "bar_chart", NewChartKind::Pareto));
    items.push(item("pptx.insert-chart.funnel", "filter_alt", NewChartKind::Funnel));
    items.push(item("pptx.insert-chart.box-whisker", "bar_chart", NewChartKind::BoxWhisker));
    items.push(item("pptx.insert-chart.treemap", "grid_view", NewChartKind::Treemap));
    items.push(item("pptx.insert-chart.sunburst", "donut_large", NewChartKind::Sunburst));
    items
}

/// The same menu, each item calling `insert` with its kind (a document's, a sheet's).
pub(crate) fn chart_items_with(insert: fn(&mut SlideEditor, NewChartKind, &mut Window, &mut Context<SlideEditor>)) -> Vec<MenuEntry> {
    let item = move |id: &'static str, icon: &'static str, kind: NewChartKind| -> MenuEntry {
        MenuItem::tr(id).icon(icon).on(move |this: &mut SlideEditor, window, cx| insert(this, kind, window, cx)).into()
    };
    vec![
        item("pptx.insert-chart.column", "bar_chart", NewChartKind::ClusteredColumn),
        item("pptx.insert-chart.stacked-column", "bar_chart", NewChartKind::StackedColumn),
        item("pptx.insert-chart.percent-stacked-column", "bar_chart", NewChartKind::PercentStackedColumn),
        item("pptx.insert-chart.3d-clustered-column", "bar_chart", NewChartKind::ClusteredColumn3D),
        item("pptx.insert-chart.3d-stacked-column", "bar_chart", NewChartKind::StackedColumn3D),
        item("pptx.insert-chart.3d-percent-stacked-column", "bar_chart", NewChartKind::PercentStackedColumn3D),
        item("pptx.insert-chart.3d-column", "bar_chart", NewChartKind::Column3D),
        MenuEntry::Separator,
        item("pptx.insert-chart.bar", "bar_chart", NewChartKind::ClusteredBar),
        item("pptx.insert-chart.stacked-bar", "bar_chart", NewChartKind::StackedBar),
        item("pptx.insert-chart.percent-stacked-bar", "bar_chart", NewChartKind::PercentStackedBar),
        item("pptx.insert-chart.3d-clustered-bar", "bar_chart", NewChartKind::ClusteredBar3D),
        item("pptx.insert-chart.3d-stacked-bar", "bar_chart", NewChartKind::StackedBar3D),
        item("pptx.insert-chart.3d-percent-stacked-bar", "bar_chart", NewChartKind::PercentStackedBar3D),
        MenuEntry::Separator,
        item("pptx.insert-chart.line", "show_chart", NewChartKind::Line),
        item("pptx.insert-chart.stacked-line", "show_chart", NewChartKind::StackedLine),
        item("pptx.insert-chart.percent-stacked-line", "show_chart", NewChartKind::PercentStackedLine),
        item("pptx.insert-chart.3d-line", "show_chart", NewChartKind::Line3D),
        MenuEntry::Separator,
        item("pptx.insert-chart.area", "area_chart", NewChartKind::Area),
        item("pptx.insert-chart.stacked-area", "area_chart", NewChartKind::StackedArea),
        item("pptx.insert-chart.percent-stacked-area", "area_chart", NewChartKind::PercentStackedArea),
        item("pptx.insert-chart.3d-area", "area_chart", NewChartKind::Area3D),
        item("pptx.insert-chart.3d-stacked-area", "area_chart", NewChartKind::StackedArea3D),
        item("pptx.insert-chart.3d-percent-stacked-area", "area_chart", NewChartKind::PercentStackedArea3D),
        MenuEntry::Separator,
        item("pptx.insert-chart.pie", "pie_chart", NewChartKind::Pie),
        item("pptx.insert-chart.3d-pie", "pie_chart", NewChartKind::Pie3D),
        item("pptx.insert-chart.doughnut", "donut_large", NewChartKind::Doughnut),
        item("pptx.insert-chart.pie-of-pie", "pie_chart", NewChartKind::PieOfPie),
        item("pptx.insert-chart.bar-of-pie", "pie_chart", NewChartKind::BarOfPie),
        MenuEntry::Separator,
        item("pptx.insert-chart.scatter", "polyline", NewChartKind::Scatter),
        item("pptx.insert-chart.scatter-smooth", "polyline", NewChartKind::ScatterSmooth),
        item("pptx.insert-chart.scatter-lines", "polyline", NewChartKind::ScatterLines),
        item("pptx.insert-chart.bubble", "dataset", NewChartKind::Bubble),
        MenuEntry::Separator,
        item("pptx.chrome.radar", "show_chart", NewChartKind::Radar),
        item("pptx.chrome.filled-radar", "area_chart", NewChartKind::RadarFilled),
        MenuEntry::Separator,
        item("pptx.insert-chart.stock-hlc", "show_chart", NewChartKind::StockHLC),
        item("pptx.insert-chart.stock-ohlc", "show_chart", NewChartKind::StockOHLC),
        item("pptx.insert-chart.stock-vhlc", "show_chart", NewChartKind::StockVHLC),
        item("pptx.insert-chart.stock-vohlc", "show_chart", NewChartKind::StockVOHLC),
        MenuEntry::Separator,
        item("pptx.insert-chart.surface-3d", "area_chart", NewChartKind::Surface3D),
        item("pptx.insert-chart.surface-3d-wireframe", "area_chart", NewChartKind::Surface3DWireframe),
        item("pptx.insert-chart.contour", "area_chart", NewChartKind::Contour),
        item("pptx.insert-chart.contour-wireframe", "area_chart", NewChartKind::ContourWireframe),
        MenuEntry::Separator,
        item("pptx.insert-chart.combo-column-line", "bar_chart", NewChartKind::ComboColumnLine),
        item("pptx.insert-chart.combo-column-line-secondary", "bar_chart", NewChartKind::ComboColumnLineSecondaryAxis),
        item("pptx.insert-chart.combo-area-column", "area_chart", NewChartKind::ComboAreaColumn),
    ]
}

/// A chartEx chart over one column: its heading (if text) names the series, the numbers below are its values.
fn ex_refs_of_column(current: &ibis_doc_model::Presentation, sheet: &str, range: CellRange) -> Option<Vec<ibis_pptx::SeriesRefs>> {
    if range.last_column != range.first_column {
        return None;
    }
    let model = ibis_pptx::sheet_of(current, sheet)?;
    let heading = matches!(model.cell(range.first_row, range.first_column).map(|c| &c.value), Some(ibis_doc_model::CellValue::Text(_)));
    let first = range.first_row + u32::from(heading);
    if first > range.last_row {
        return None;
    }
    let one = |r0: u32, r1: u32| ibis_pptx::format_ref(sheet, CellRange { first_row: r0, last_row: r1, ..range });
    Some(vec![ibis_pptx::SeriesRefs { name: heading.then(|| one(range.first_row, range.first_row)), categories: None, values: Some(one(first, range.last_row)), ..Default::default() }])
}

/// A hierarchy for a treemap or a sunburst: a column of text for each level, the last column the sizes, under a
/// heading row if the sizes' heading is text. The categories are all the columns before the last.
fn ex_refs_of_hierarchy(current: &ibis_doc_model::Presentation, sheet: &str, range: CellRange) -> Option<Vec<ibis_pptx::SeriesRefs>> {
    if range.last_column <= range.first_column {
        return None;
    }
    let model = ibis_pptx::sheet_of(current, sheet)?;
    let size_column = range.last_column;
    let heading = matches!(model.cell(range.first_row, size_column).map(|c| &c.value), Some(ibis_doc_model::CellValue::Text(_)));
    let first = range.first_row + u32::from(heading);
    if first > range.last_row {
        return None;
    }
    let sizes_are_numbers = (first..=range.last_row).all(|r| matches!(model.cell(r, size_column).map(|c| &c.value), Some(ibis_doc_model::CellValue::Number(_)) | None));
    if !sizes_are_numbers {
        return None;
    }
    let part = |r0: u32, c0: u32, r1: u32, c1: u32| ibis_pptx::format_ref(sheet, CellRange { first_row: r0, first_column: c0, last_row: r1, last_column: c1 });
    Some(vec![ibis_pptx::SeriesRefs {
        name: heading.then(|| part(range.first_row, size_column, range.first_row, size_column)),
        categories: Some(part(first, range.first_column, range.last_row, size_column - 1)),
        values: Some(part(first, size_column, range.last_row, size_column)),
        ..Default::default()
    }])
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
        let is_stock = matches!(kind, NewChartKind::StockHLC | NewChartKind::StockOHLC | NewChartKind::StockVHLC | NewChartKind::StockVOHLC);
        let refs = if matches!(kind, NewChartKind::Treemap | NewChartKind::Sunburst) {
            // A hierarchy: the columns of text are its levels, the last column the sizes.
            ex_refs_of_hierarchy(&current, &sheet, range)
        } else if kind.is_chart_ex() {
            // One series: the categories (not for a histogram) and the values; or one column of numbers.
            ibis_pptx::refs_for_range(&current, &sheet, range, true).or_else(|| ex_refs_of_column(&current, &sheet, range))
        } else if bubble {
            ibis_pptx::bubble_refs_for_range(&current, &sheet, range, by_columns)
                .or_else(|| ibis_pptx::bubble_refs_for_range(&current, &sheet, range, !by_columns))
        } else if is_stock {
            ibis_pptx::stock_refs_for_range(&current, &sheet, range, kind, by_columns)
                .or_else(|| ibis_pptx::stock_refs_for_range(&current, &sheet, range, kind, !by_columns))
        } else {
            if scatter {
                ibis_pptx::scatter_refs_for_range(&current, &sheet, range, by_columns)
                    .or_else(|| ibis_pptx::scatter_refs_for_range(&current, &sheet, range, !by_columns))
            } else {
                None
            }
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

    /// 挿入 > おすすめグラフ (xlsx.recommended-charts):
    /// Analyzes the selected data and inserts the recommended chart type (clustered column, line, or pie).
    pub(crate) fn open_recommended_charts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(range) = self.chart_source_range() else {
            self.status = Some(crate::shell::i18n::text("xlsx.chart.no-data").to_string().into());
            cx.notify();
            return;
        };
        let rows = (range.last_row - range.first_row).max(1);
        let cols = (range.last_column - range.first_column).max(1);
        let kind = if rows <= 1 || cols <= 1 {
            NewChartKind::Pie
        } else if rows >= 6 && cols <= 3 {
            NewChartKind::Line
        } else {
            NewChartKind::ClusteredColumn
        };
        self.insert_sheet_chart(kind, window, cx);
        self.status = Some(crate::shell::i18n::text("xlsx.recommended-chart-inserted").into());
        cx.notify();
    }
}
