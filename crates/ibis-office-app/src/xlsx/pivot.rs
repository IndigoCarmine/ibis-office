// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pivot tables (ピボットテーブル) in the editor: the columns of a table
//! built on opening fitted to it, as Excel fits them when it builds one.

use std::sync::Arc;

use gpui_kit::{Context, TextSystem};
use ibis_doc_model::{CellStyle, Command, EMU_PER_POINT, Presentation, Shape, ShapeId, ShapeKind, Sheet, SheetCommand, SlideId};

use crate::render::shape_view::Resources;
use crate::shell::chrome::{MenuEntry, MenuItem};
use crate::shell::editor::SlideEditor;

/// The room Excel leaves beside a fitted pivot column's widest text, and
/// for a pivot button (▼), in points (measured against Excel's widths of
/// XPVT-01 and 02: "第1四半期" in 62 pt, "行ラベル" with its button in 71).
const PADDING: f32 = 12.0;
const BUTTON: f32 = 15.0;

/// A text's width as Excel measures it for fitting, in points: the drawn
/// width scaled to Excel's digit, and its indent.
pub(crate) fn excel_width(text: &str, style: &CellStyle, default_size: f32, res: &Resources, ts: &TextSystem) -> f32 {
    let digit = (default_size * 0.55).round().max(1.0);
    let size = style.font_size as f32 / 100.0;
    let excel_digit = digit * size / default_size;
    let drawn_digit = super::sheet_view::text_width_pt("0", style, res, ts).max(0.1);
    let width = super::sheet_view::text_width_pt(text, style, res, ts) * excel_digit / drawn_digit;
    let italic = if style.italic { 2.0 * size / 12.0 } else { 0.0 };
    width + italic + style.indent as f32 * 3.0 * excel_digit
}

/// The columns of each pivot table built on opening (`refreshOnLoad`)
/// fitted to its cells, as Excel does after building one: the widest text
/// of the column within the table and its report filters (not the corner
/// caption), with room for a pivot button. The style's bold is not counted
/// (Excel measures the cells' own font).
pub(crate) fn fit_built_pivots(presentation: &mut Presentation, res: &Resources, ts: &TextSystem) {
    let default_size = presentation.workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
    for slide in &mut presentation.slides {
        for shape in &mut slide.shapes {
            let ShapeKind::Sheet(sheet) = &mut shape.kind else { continue };
            if !sheet.pivots.iter().any(|p| p.refresh_on_load) {
                continue;
            }
            let widths = fitted(sheet, default_size, res, ts);
            let sheet = Arc::make_mut(sheet);
            for (column, width) in widths {
                if let Some(w) = sheet.columns.get_mut(column as usize)
                    && *w > 0
                {
                    *w = width;
                }
            }
        }
    }
}

fn fitted(sheet: &Sheet, default_size: f32, res: &Resources, ts: &TextSystem) -> Vec<(u32, ibis_doc_model::Emu)> {
    let built: Vec<usize> = (0..sheet.pivots.len()).filter(|&i| sheet.pivots[i].refresh_on_load).collect();
    fitted_for(sheet, &built, default_size, res, ts)
}

fn fitted_for(sheet: &Sheet, which: &[usize], default_size: f32, res: &Resources, ts: &TextSystem) -> Vec<(u32, ibis_doc_model::Emu)> {
    let mut out = Vec::new();
    for p in which.iter().filter_map(|&i| sheet.pivots.get(i)) {
        let loc = p.location;
        let pads: std::collections::HashMap<(u32, u32), f32> = p.label_pads().into_iter().map(|(r, c, pt)| ((r, c), pt)).collect();
        let pages = p.page_fields.len() as u32;
        let top = if pages > 0 { loc.first_row.saturating_sub(pages + 1) } else { loc.first_row };
        for column in loc.first_column..=loc.last_column {
            let mut widest: f32 = 0.0;
            for cell in sheet.cells.iter().filter(|c| c.column == column && (top..=loc.last_row).contains(&c.row) && !c.text.is_empty()) {
                if p.corner == Some((cell.row, cell.column)) {
                    continue;
                }
                let mut style = sheet.styles[cell.style as usize].clone();
                style.bold = false;
                let button = if p.buttons.contains(&(cell.row, cell.column)) { BUTTON } else { 0.0 };
                // A row label's own indent in place of its cell's.
                let pad = pads.get(&(cell.row, cell.column)).copied();
                if pad.is_some() {
                    style.indent = 0;
                }
                let button = button + pad.unwrap_or(0.0);
                widest = widest.max(excel_width(&cell.text, &style, default_size, res, ts) + button);
            }
            // Excel keeps the first label column of a table not in コンパクト
            // form (XPVT-03 and 04 keep column A's width).
            let compact = p.row_fields.first().is_none_or(|&f| p.fields.get(f.max(0) as usize).is_none_or(|f| f.compact && f.outline));
            if column == loc.first_column && !compact {
                continue;
            }
            if widest > 0.0 {
                // Whole points, as Excel's widths of whole pixels.
                let pt = (widest + PADDING).ceil();
                out.push((column, (pt * EMU_PER_POINT as f32) as ibis_doc_model::Emu));
            }
        }
    }
    out
}

impl SlideEditor {
    /// The pivot table the selected cell is in (or in its report filters).
    pub(crate) fn pivot_at_selection(&self) -> Option<usize> {
        let (_, sheet) = self.current_sheet()?;
        let (column, row) = self.current_cell()?;
        sheet.pivots.iter().position(|p| {
            let loc = p.location;
            let pages = p.page_fields.len() as u32;
            let top = if pages > 0 { loc.first_row.saturating_sub(pages + 1) } else { loc.first_row };
            (top..=loc.last_row).contains(&row) && (loc.first_column..=loc.last_column).contains(&column)
        })
    }

    /// ピボットテーブルの更新 (`all`: すべて更新, every pivot table of the
    /// book): each built again from its source, in its style, its columns
    /// fitted as Excel fits them; one step to undo.
    pub(crate) fn refresh_pivots(&mut self, all: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let targets: Vec<(SlideId, usize)> = if all {
            self.presentation()
                .slides
                .iter()
                .flat_map(|s| {
                    let count = s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(sheet) = &sh.kind { Some(sheet.pivots.len()) } else { None }).unwrap_or(0);
                    (0..count).map(move |i| (s.id, i))
                })
                .collect()
        } else {
            match self.pivot_at_selection() {
                Some(i) => vec![(self.current_slide, i)],
                None => {
                    self.status = Some(crate::shell::i18n::text("_xlsx.select-a-cell-in-a-pivottable").into());
                    return cx.notify();
                }
            }
        };
        if targets.is_empty() {
            return;
        }
        // Built one after another on a copy (several on one sheet).
        let mut work = self.presentation().clone();
        let default_size = work.workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
        let ts = cx.text_system().clone();
        let mut changed: Vec<SlideId> = Vec::new();
        for (slide, index) in targets {
            let Some(mut sheet) = ibis_pptx::refreshed_pivot(&work, slide, index) else { continue };
            let before = work.slide(slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None }));
            if before.is_some_and(|old| covers_others(&old, &sheet, index)) {
                return self.refuse_overlap(cx);
            }
            for (column, width) in fitted_for(&sheet, &[index], default_size, &self.resources, &ts) {
                if let Some(w) = sheet.columns.get_mut(column as usize)
                    && *w > 0
                {
                    *w = width;
                }
            }
            if let Some(s) = work.slide_mut(slide)
                && let Some(ShapeKind::Sheet(target)) = s.shapes.iter_mut().map(|sh| &mut sh.kind).find(|k| matches!(k, ShapeKind::Sheet(_)))
            {
                *target = Arc::new(sheet);
            }
            if !changed.contains(&slide) {
                changed.push(slide);
            }
        }
        let mut commands: Vec<Command> = changed
            .into_iter()
            .filter_map(|slide| {
                let sheet = work.slide(slide)?.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some((**s).clone()) } else { None })?;
                Some(Command::Sheet(SheetCommand::ReplaceSheet { slide, sheet: Box::new(sheet) }))
            })
            .collect();
        // Also sync any PivotCharts linked to refreshed pivots
        for slide in self.presentation().slides.iter() {
            let sheet = work.slide(slide.id).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(sheet) = &sh.kind { Some((**sheet).clone()) } else { None }));
            if let Some(sheet) = sheet {
                for pivot in &sheet.pivots {
                    if let Some(data) = ibis_pptx::pivot_source(self.presentation(), &sheet, pivot) {
                        let built = pivot.build(&data);
                        for sh in &slide.shapes {
                            if let ShapeKind::Chart(c) = &sh.kind {
                                if c.pivot_source.as_deref() == Some(&pivot.name) {
                                    let mut updated_chart = (**c).clone();
                                    ibis_doc_model::pivot::update_chart_from_pivot(&mut updated_chart, pivot, &built);
                                    commands.push(Command::SetKind {
                                        slide: slide.id,
                                        shape: sh.id,
                                        kind: Box::new(ShapeKind::Chart(Box::new(updated_chart))),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        self.status = match self.document.execute(commands) {
            Ok(_) => None,
            Err(err) => Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into()),
        };
        cx.notify();
    }

    /// ピボットテーブルのデータセルに対して詳細データを展開する（Show Details / Drill Down）。
    /// 成功した場合は新規展開シートに切り替えて true を返す。
    pub(crate) fn show_pivot_details(&mut self, column: u32, row: u32, cx: &mut Context<Self>) -> bool {
        self.commit_cell_edit(cx);
        let before_slides: Vec<SlideId> = self.presentation().slides.iter().map(|s| s.id).collect();
        let cmd = Command::Sheet(SheetCommand::ShowPivotDetails {
            slide: self.current_slide,
            column,
            row,
        });
        match self.document.execute(vec![cmd]) {
            Ok(_) => {
                let after_slides: Vec<SlideId> = self.presentation().slides.iter().map(|s| s.id).collect();
                if let Some(&new_slide) = after_slides.iter().find(|id| !before_slides.contains(id)) {
                    self.current_slide = new_slide;
                    self.set_cell(0, 0);
                }
                self.status = Some(crate::shell::i18n::text("_xlsx.expanded-the-detail-data").into());
                cx.notify();
                true
            }
            Err(_) => false,
        }
    }

    /// メニュー（データ > 詳細データの表示）からの実行。
    pub(crate) fn show_pivot_details_at_selection(&mut self, cx: &mut Context<Self>) {
        let Some((column, row)) = self.current_cell() else {
            self.status = Some(crate::shell::i18n::text("_xlsx.select-a-data-cell-in-the-pivottable").into());
            return cx.notify();
        };
        if !self.show_pivot_details(column, row, cx) {
            self.status = Some(crate::shell::i18n::text("_xlsx.select-a-data-cell-in-the-pivottable").into());
            cx.notify();
        }
    }
}

/// データ > ピボットテーブルの更新, すべて更新.
pub(crate) fn menu_items() -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("xlsx.pivot-fields").on(|this, window, cx| this.open_pivot_fields(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-value-settings").on(|this, window, cx| this.open_pivot_value_settings(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-options").on(|this, window, cx| this.open_pivot_options(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-show-details").on(|this, _, cx| this.show_pivot_details_at_selection(cx)).into(),
        MenuItem::tr("xlsx.pivot-calculated-field").on(|this, window, cx| this.open_pivot_calc_field(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-calculated-item").on(|this, window, cx| this.open_pivot_calc_item(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-group").on(|this, window, cx| this.open_pivot_group(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-ungroup").on(|this, _, cx| this.pivot_ungroup(cx)).into(),
        MenuItem::tr("xlsx.pivot-refresh").on(|this, _, cx| this.refresh_pivots(false, cx)).into(),
        MenuItem::tr("xlsx.pivot-refresh-all").on(|this, _, cx| this.refresh_pivots(true, cx)).into(),
    ]
}

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{AppContext as _, Entity, FontWeight, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window, div, px};
use ibis_doc_model::{PivotAxis, PivotSource, PivotTable};

use crate::shell::chrome::SidebarPanel;
use crate::shell::i18n::text;

/// 挿入 > ピボットテーブル: where its data is and where it goes; then the
/// table's fields (ピボットテーブルのフィールド).
pub(crate) struct PivotPanel {
    pub(crate) create: Option<CreatePivot>,
    /// The table whose fields are shown (its sheet and index).
    pub(crate) target: Option<(SlideId, usize)>,
    /// The value whose summary is being chosen.
    pub(crate) value: Option<usize>,
    /// The custom name input for the value field being edited.
    pub(crate) value_name: Option<Entity<InputState>>,
    /// Selected row/col field for field settings (subtotals, layout).
    pub(crate) field_selected: Option<(PivotAxis, usize)>,
    pub(crate) message: String,
    /// 空白セルに表示する値 and エラー値に表示する値.
    pub(crate) missing: Option<Entity<InputState>>,
    pub(crate) error: Option<Entity<InputState>>,
}

pub(crate) struct CreatePivot {
    pub(crate) source: Entity<InputState>,
    pub(crate) new_sheet: bool,
    pub(crate) at: Entity<InputState>,
}

const AREAS: [(PivotAxis, &str); 4] = [(PivotAxis::Page, "_xlsx.filters"), (PivotAxis::Column, "_xlsx.columns"), (PivotAxis::Row, "_xlsx.rows"), (PivotAxis::Values, "_xlsx.values")];

impl SlideEditor {
    fn sheet_name_of(&self, slide: SlideId) -> Option<String> {
        self.presentation().slide(slide).and_then(|s| s.name.clone())
    }

    fn slide_named(&self, name: &str) -> Option<SlideId> {
        self.presentation().slides.iter().find(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name))).map(|s| s.id)
    }

    fn show_pivot_panel(&mut self, panel: PivotPanel, cx: &mut Context<Self>) {
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.panels.close();
        s.panels.pivot = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// 挿入 > ピボットテーブル: the panel, its data the table around the
    /// selected cell (or the selection), on a new sheet.
    pub(crate) fn open_create_pivot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let range = self.selected_range().filter(|r| r.first_row != r.last_row || r.first_column != r.last_column).or_else(|| self.current_region());
        let sheet = self.sheet_name_of(self.current_slide).unwrap_or_default();
        let source = range.map(|r| format!("{}!{}", quote_sheet(&sheet), absolute(r))).unwrap_or_default();
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let at = self.current_cell().map(|(c, r)| format!("{}!{}", quote_sheet(&sheet), ibis_doc_model::cell_name(c, r))).unwrap_or_default();
        let create = CreatePivot { source: input(source, window, cx), new_sheet: true, at: input(at, window, cx) };
        self.show_pivot_panel(PivotPanel { create: Some(create), target: None, value: None, value_name: None, field_selected: None, message: String::new(), missing: None, error: None }, cx);
    }

    /// データ > ピボットテーブルのフィールド: the panel of the table the
    /// selected cell is in.
    pub(crate) fn open_pivot_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = Some((self.current_slide, index));
        let pivot = self.presentation().slide(self.current_slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(index).cloned() } else { None }));
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let missing = input(pivot.as_ref().and_then(|p| p.missing_caption.clone()).unwrap_or_default(), window, cx);
        let error = input(pivot.as_ref().and_then(|p| p.error_caption.clone()).unwrap_or_default(), window, cx);
        self.show_pivot_panel(PivotPanel { create: None, target, value: None, value_name: None, field_selected: None, message: String::new(), missing: Some(missing), error: Some(error) }, cx);
    }

    /// データ > 値フィールドの設定…: the panel of the table the selected cell is in,
    /// focused on the value field.
    pub(crate) fn open_pivot_value_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, index);
        let pivot = self.presentation().slide(self.current_slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(index).cloned() } else { None }));
        let Some(pivot) = pivot else { return };
        if pivot.data_fields.is_empty() {
            self.status = Some(text("_xlsx.there-are-no-value-fields").into());
            return cx.notify();
        }
        let val_pos = if pivot.data_fields.len() > 1 {
            if let Some((col, _)) = self.current_cell() {
                let first_val_col = pivot.location.first_column + pivot.first_data_col;
                if col >= first_val_col {
                    ((col - first_val_col) as usize) % pivot.data_fields.len()
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        };
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let missing = input(pivot.missing_caption.clone().unwrap_or_default(), window, cx);
        let error = input(pivot.error_caption.clone().unwrap_or_default(), window, cx);
        let val_name = input(pivot.data_fields.get(val_pos).map(|d| d.name.clone()).unwrap_or_default(), window, cx);
        self.show_pivot_panel(PivotPanel {
            create: None,
            target: Some(target),
            value: Some(val_pos),
            value_name: Some(val_name),
            field_selected: None,
            message: String::new(),
            missing: Some(missing),
            error: Some(error),
        }, cx);
    }

    fn current_sheet_of(&self, slide: SlideId) -> Option<Arc<Sheet>> {
        self.presentation().slide(slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None }))
    }

    fn refuse_overlap(&mut self, cx: &mut Context<Self>) {
        let message = text("_xlsx.the-pivottable-would-overlap-other-cells-or-anot");
        self.status = Some(message.clone());
        self.pivot_message(message, cx);
    }

    fn pivot_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.pivot.as_mut() {
            p.message = message.to_string();
        }
        cx.notify();
    }

    /// The panel's 作成: the table made where it says, with nothing placed,
    /// and its fields shown; one step to undo.
    pub(crate) fn create_pivot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.create.as_ref()) else { return };
        let (source_text, new_sheet, at_text) = (c.source.read(cx).value().to_string(), c.new_sheet, c.at.read(cx).value().to_string());
        let current = self.sheet_name_of(self.current_slide).unwrap_or_default();
        let parsed_ref = super::sheet_ui::parse_reference(&source_text);
        let (source_sheet, range, source_name) = match parsed_ref {
            Some(super::sheet_ui::Reference { sheet, target: super::sheet_ui::Target::Cells(range) }) => {
                (sheet.unwrap_or_else(|| current.clone()), range, None)
            }
            Some(super::sheet_ui::Reference { target: super::sheet_ui::Target::Name(_), .. }) | None if super::sheet_ui::is_name(source_text.trim()) => {
                let name = source_text.trim();
                let mut found_table = None;
                for slide in &self.presentation().slides {
                    let sh_name = slide.name.clone().unwrap_or_default();
                    if let Some(sh) = slide.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None }) {
                        if let Some(tbl) = sh.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name)) {
                            let r = ibis_doc_model::CellRange {
                                first_row: tbl.range.first_row,
                                last_row: tbl.range.last_row.saturating_sub(tbl.totals_rows),
                                first_column: tbl.range.first_column,
                                last_column: tbl.range.last_column,
                            };
                            found_table = Some((sh_name, r, Some(tbl.name.clone())));
                            break;
                        }
                    }
                }
                let found_name = if found_table.is_none() {
                    self.presentation().workbook.as_ref().and_then(|wb| {
                        wb.names.iter().find(|n| n.name.eq_ignore_ascii_case(name)).and_then(|dn| {
                            let trimmed = dn.formula.trim().trim_start_matches('=');
                            let (sh_n, cell_part) = if let Some((s, c)) = trimmed.split_once('!') {
                                (s.trim().trim_matches('\'').to_string(), c)
                            } else {
                                (current.clone(), trimmed)
                            };
                            let r = super::sheet_ui::parse_reference(cell_part).and_then(|re| match re.target {
                                super::sheet_ui::Target::Cells(c) => Some(c),
                                _ => None,
                            })?;
                            Some((sh_n, r, Some(dn.name.clone())))
                        })
                    })
                } else {
                    None
                };

                if let Some(t) = found_table.or(found_name) {
                    t
                } else {
                    return self.pivot_message(text("_xlsx.the-data-range-is-not-valid"), cx);
                }
            }
            _ => return self.pivot_message(text("_xlsx.the-data-range-is-not-valid"), cx),
        };
        let Some(source_slide) = self.slide_named(&source_sheet) else { return self.pivot_message(text("_xlsx.the-data-range-is-not-valid"), cx) };
        let Some(data_sheet) = self.presentation().slide(source_slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None })) else { return };
        let data = ibis_doc_model::pivot::source_data(&data_sheet, range);
        if range.last_row <= range.first_row || data.headings.iter().any(|h| h.trim().is_empty()) {
            return self.pivot_message(text("_xlsx.select-a-range-of-two-or-more-rows-with-a-headin"), cx);
        }
        let count: usize = self.presentation().slides.iter().filter_map(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.pivots.len()) } else { None })).sum();
        let name = format!("{}{}", text("_xlsx.pivottable"), count + 1);
        let source = PivotSource { sheet: Some(source_sheet), range: Some(range), name: source_name };
        let (slide, row, column) = if new_sheet {
            // A new sheet before the current one, the table at A3, as Excel puts it.
            let Some(wb) = self.presentation().workbook.clone() else { return };
            let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide)).unwrap_or(0);
            let mut n = wb.sheets.len() + 1;
            let sheet_name = loop {
                let name = format!("Sheet{n}");
                if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, None) {
                    break name;
                }
                n += 1;
            };
            let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::AddSheet { index, name: sheet_name, slide, shape })]) {
                return self.pivot_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
            }
            (slide, 2, 0)
        } else {
            let Some(super::sheet_ui::Reference { sheet, target: super::sheet_ui::Target::Cells(at) }) = super::sheet_ui::parse_reference(&at_text) else {
                return self.pivot_message(text("_xlsx.the-location-is-not-valid"), cx);
            };
            let Some(slide) = self.slide_named(sheet.as_deref().unwrap_or(&current)) else { return self.pivot_message(text("_xlsx.the-location-is-not-valid"), cx) };
            (slide, at.first_row, at.first_column)
        };
        let pivot = PivotTable::new(&name, source, &data, row, column);
        let index = self.presentation().slide(slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.pivots.len()) } else { None })).unwrap_or(0);
        let Some(sheet) = ibis_pptx::rebuilt_pivot(self.presentation(), slide, index, Some(pivot)) else { return };
        if !new_sheet && self.current_sheet_of(slide).is_some_and(|old| covers_others(&old, &sheet, index) || old.cell(row, column).is_some_and(|c| !c.value.is_empty())) {
            return self.refuse_overlap(cx);
        }
        let command = vec![Command::Sheet(SheetCommand::ReplaceSheet { slide, sheet: Box::new(sheet) })];
        let result = if new_sheet { self.document.amend_last(command) } else { self.document.execute(command) };
        if let Err(err) = result {
            return self.pivot_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
        }
        if slide != self.current_slide {
            self.select_slide(slide, window, cx);
        }
        let s = self.wb_mut();
        s.cells.insert(slide, (column, row));
        s.cell_ends.remove(&slide);
        if let Some(p) = s.panels.pivot.as_mut() {
            p.create = None;
            p.target = Some((slide, index));
            p.message = String::new();
        }
        cx.notify();
    }

    /// The table `target` changed by `change`, built again; one step to undo.
    pub(crate) fn change_pivot(&mut self, target: (SlideId, usize), change: impl FnOnce(&mut PivotTable), cx: &mut Context<Self>) {
        let (slide, index) = target;
        let Some(mut pivot) = self.presentation().slide(slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(index).cloned() } else { None })) else { return };
        if let Some(data) = ibis_pptx::pivot_data(self.presentation(), slide, index) {
            pivot.fill_shared(&data);
        }
        change(&mut pivot);
        let Some(mut sheet) = ibis_pptx::rebuilt_pivot(self.presentation(), slide, index, Some(pivot)) else { return };
        if self.current_sheet_of(slide).is_some_and(|old| covers_others(&old, &sheet, index)) {
            return self.refuse_overlap(cx);
        }
        if sheet.pivots.get(index).is_none_or(|p| p.auto_fit_columns) {
            let default_size = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
            let ts = cx.text_system().clone();
            for (column, width) in fitted_for(&sheet, &[index], default_size, &self.resources, &ts) {
                if let Some(w) = sheet.columns.get_mut(column as usize)
                    && *w > 0
                {
                    *w = width;
                }
            }
        }
        let mut commands = vec![Command::Sheet(SheetCommand::ReplaceSheet { slide, sheet: Box::new(sheet.clone()) })];
        if let (Some(s), Some(p)) = (self.presentation().slide(slide), sheet.pivots.get(index)) {
            if let Some(data) = ibis_pptx::pivot_source(self.presentation(), &sheet, p) {
                let built = p.build(&data);
                for sh in &s.shapes {
                    if let ShapeKind::Chart(c) = &sh.kind {
                        if c.pivot_source.as_deref() == Some(&p.name) {
                            let mut updated_chart = (**c).clone();
                            ibis_doc_model::pivot::update_chart_from_pivot(&mut updated_chart, p, &built);
                            commands.push(Command::SetKind {
                                slide,
                                shape: sh.id,
                                kind: Box::new(ShapeKind::Chart(Box::new(updated_chart))),
                            });
                        }
                    }
                }
            }
        }
        self.status = match self.document.execute(commands) {
            Ok(_) => None,
            Err(err) => Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into()),
        };
        let message = self.status.clone().unwrap_or_default();
        self.pivot_message(message, cx);
    }

    /// Insert a PivotChart linked to the pivot table at `target` (slide, pivot_index).
    pub(crate) fn insert_pivot_chart(&mut self, target: (SlideId, usize), cx: &mut Context<Self>) -> Option<ShapeId> {
        let (slide, index) = target;
        let s = self.presentation().slide(slide)?;
        let pivot = s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(index).cloned() } else { None })?;
        let sheet = s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s) } else { None })?;
        let data = ibis_pptx::pivot_source(self.presentation(), sheet, &pivot)?;
        let built = pivot.build(&data);
        let chart = ibis_doc_model::pivot::chart_from_pivot(&pivot, &built);

        let (x, y) = self.sheet_insertion_origin();
        let frame = ibis_doc_model::EmuRect::new(x, y, 4_500_000, 3_000_000);
        let id = self.document.ids().new_shape_id();
        let shape_index = self.slide().shapes.len();
        let shape = Shape {
            id,
            frame,
            rotation: 0,
            flip_h: false,
            flip_v: false,
            kind: ShapeKind::Chart(Box::new(chart)),
            floating: None,
        };
        self.run_sheet(vec![Command::InsertShape { slide, index: shape_index, shape }], cx);
        self.selection = Some(id);
        self.status = Some(crate::shell::i18n::text("xlsx.pivot-chart-inserted").into());
        cx.notify();
        Some(id)
    }

    /// 挿入 > ピボットグラフ: create and insert a PivotChart linked to the selected or active pivot table.
    pub(crate) fn open_insert_pivot_chart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        if let Some(index) = self.pivot_at_selection() {
            self.insert_pivot_chart((self.current_slide, index), cx);
            return;
        }
        // If there are pivot tables on the current sheet, pick the first one
        let first_pivot = self.slide().shapes.iter().find_map(|sh| {
            if let ShapeKind::Sheet(s) = &sh.kind {
                if !s.pivots.is_empty() {
                    Some(0)
                } else {
                    None
                }
            } else {
                None
            }
        });
        if let Some(index) = first_pivot {
            self.insert_pivot_chart((self.current_slide, index), cx);
        } else {
            // No pivot table exists, prompt create pivot dialog
            self.open_create_pivot(window, cx);
        }
    }

    /// A field's check box: on, to the rows (a field of numbers to the
    /// values), as Excel does; off, out of the table.
    pub(crate) fn toggle_pivot_field(&mut self, target: (SlideId, usize), f: usize, cx: &mut Context<Self>) {
        self.change_pivot(target, |p| {
            if p.place_of(f).is_some() {
                p.place(f, None, 0);
            } else {
                let numbers = p.fields[f].shared.iter().filter(|v| !matches!(v, ibis_doc_model::CellValue::Empty)).all(|v| matches!(v, ibis_doc_model::CellValue::Number(_)));
                let area = if numbers { PivotAxis::Values } else { PivotAxis::Row };
                p.place(f, Some(area), usize::MAX);
            }
        }, cx);
    }

    pub(crate) fn pivot_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.pivot.as_ref()?;
        let message = p.message.clone();
        if let Some(c) = &p.create {
            let new_sheet = c.new_sheet;
            let radio = |id: &'static str, on: bool, label: &str, value: bool| {
                Button::new(id).small().label(format!("{} {}", if on { "◉" } else { "○" }, text(label))).on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(c) = this.wb_mut().panels.pivot.as_mut().and_then(|p| p.create.as_mut()) {
                        c.new_sheet = value;
                    }
                    cx.notify();
                }))
            };
            let body = v_flex()
                .gap_1()
                .px(px(18.))
                .py(px(8.))
                .child(text("_xlsx.table-or-range"))
                .child(Input::new(&c.source).small())
                .child(text("_xlsx.where-to-place-the-pivottable"))
                .child(radio("pivot-new-sheet", new_sheet, "_xlsx.new-worksheet", true))
                .child(radio("pivot-existing", !new_sheet, "_xlsx.existing-worksheet", false))
                .child(Input::new(&c.at).small())
                .child(div().text_size(px(12.)).child(SharedString::from(message)))
                .child(Button::new("pivot-create").primary().small().label(text("_xlsx.create")).on_click(cx.listener(|this, _, window, cx| this.create_pivot(window, cx))));
            return Some(SidebarPanel { title: "_xlsx.create-pivottable".into(), body: body.into_any_element() });
        }
        let target = p.target?;
        let selected_value = p.value;
        let selected_field = p.field_selected;
        let pivot = self.presentation().slide(target.0).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(target.1).cloned() } else { None }))?;
        let mut body = v_flex().gap_1().px(px(18.)).py(px(8.)).child(text("_xlsx.choose-fields-to-add-to-the-report"));
        for (i, f) in pivot.fields.iter().enumerate() {
            let on = pivot.place_of(i).is_some();
            body = body.child(
                Button::new(SharedString::from(format!("pivot-field-{i}")))
                    .small()
                    .label(format!("{} {}", if on { "☑" } else { "☐" }, f.name))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_pivot_field(target, i, cx))),
            );
        }
        for (area, label) in AREAS {
            body = body.child(div().pt(px(6.)).child(text(label)));
            let entries: Vec<(usize, String)> = match area {
                PivotAxis::Row => pivot.row_fields.iter().filter(|&&f| f >= 0).map(|&f| (f as usize, pivot.fields[f as usize].name.clone())).collect(),
                PivotAxis::Column => pivot.col_fields.iter().map(|&f| if f < 0 { (usize::MAX, text("_xlsx.values-2").to_string()) } else { (f as usize, pivot.fields[f as usize].name.clone()) }).collect(),
                PivotAxis::Page => pivot.page_fields.iter().map(|p| (p.field as usize, pivot.fields[p.field as usize].name.clone())).collect(),
                PivotAxis::Values => pivot.data_fields.iter().map(|d| (d.field as usize, d.name.clone())).collect(),
            };
            for (pos, (f, name)) in entries.into_iter().enumerate() {
                if f == usize::MAX {
                    body = body.child(div().pl(px(8.)).text_size(px(12.)).child(SharedString::from(name)));
                    continue;
                }
                let id = |s: &str| SharedString::from(format!("pivot-{label}-{pos}-{s}"));
                let mut row = h_flex().gap_1().flex_wrap().pl(px(8.)).child(div().text_size(px(12.)).child(SharedString::from(name)));
                if area == PivotAxis::Values {
                    row = row.child(Button::new(id("fn")).small().label(text("fix.pivot.settings")).on_click(cx.listener(move |this, _, window, cx| {
                        let current_name = if let Some(target) = this.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.target) {
                            this.presentation().slide(target.0).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(target.1).and_then(|p| p.data_fields.get(pos)).map(|d| d.name.clone()) } else { None })).unwrap_or_default()
                        } else {
                            String::new()
                        };
                        if let Some(p) = this.wb_mut().panels.pivot.as_mut() {
                            if p.value == Some(pos) {
                                p.value = None;
                                p.value_name = None;
                            } else {
                                p.value = Some(pos);
                                p.field_selected = None;
                                let state = cx.new(|cx| InputState::new(window, cx));
                                state.update(cx, |i, cx| i.set_value(current_name, window, cx));
                                p.value_name = Some(state);
                            }
                        }
                        cx.notify();
                    })));
                } else if area == PivotAxis::Row || area == PivotAxis::Column {
                    row = row.child(Button::new(id("field-fn")).small().label(text("fix.pivot.settings")).on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.pivot.as_mut() {
                            p.value = None;
                            p.value_name = None;
                            p.field_selected = if p.field_selected == Some((area, pos)) { None } else { Some((area, pos)) };
                        }
                        cx.notify();
                    })));
                }
                row = row
                    .child(Button::new(id("up")).small().label("▲").on_click(cx.listener(move |this, _, _, cx| this.change_pivot(target, |p| p.place(f, Some(area), pos.saturating_sub(1)), cx))))
                    .child(Button::new(id("down")).small().label("▼").on_click(cx.listener(move |this, _, _, cx| this.change_pivot(target, |p| p.place(f, Some(area), pos + 1), cx))));
                for (other, other_label) in AREAS {
                    if other == area || (area == PivotAxis::Values) != (other == PivotAxis::Values) && area == PivotAxis::Values {
                        continue;
                    }
                    row = row.child(Button::new(id(other_label)).small().label(format!("→{}", text(other_label))).on_click(cx.listener(move |this, _, _, cx| {
                        this.change_pivot(target, |p| {
                            if area == PivotAxis::Values {
                                p.place(f, None, 0);
                            }
                            p.place(f, Some(other), usize::MAX)
                        }, cx)
                    })));
                }
                row = row.child(Button::new(id("remove")).small().label("✕").on_click(cx.listener(move |this, _, _, cx| {
                    this.change_pivot(target, |p| {
                        if area == PivotAxis::Values {
                            p.data_fields.retain(|d| d.field != f as u32);
                            p.place(f, p.place_of(f).map(|(a, _)| a), usize::MAX);
                        } else {
                            p.place(f, None, 0);
                        }
                    }, cx)
                })));
                body = body.child(row);
                if area == PivotAxis::Values && selected_value == Some(pos) {
                    let current_df = pivot.data_fields.get(pos);
                    let current_fn = current_df.map(|d| d.function.as_str()).unwrap_or("sum");
                    let current_fmt = current_df.and_then(|d| d.number_format.as_deref()).unwrap_or("General");
                    let current_show = current_df.and_then(|d| d.show_as.as_deref()).unwrap_or("normal");

                    let mut settings = v_flex().gap_1().pl(px(16.)).pr(px(8.)).py(px(4.));

                    // 1. 名前 (指定した名前)
                    settings = settings.child(div().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.name-custom-name")));
                    if let Some(input_state) = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.value_name.as_ref()) {
                        let name_row = h_flex().gap_1().child(Input::new(input_state).small()).child(
                            Button::new(id("apply-name")).small().label(text("_shell.apply")).on_click(cx.listener(move |this, _, _, cx| {
                                let name = this.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.value_name.as_ref()).map(|st| st.read(cx).value().to_string()).unwrap_or_default();
                                this.change_pivot(target, |p| p.set_data_field_name(pos, &name), cx);
                            }))
                        );
                        settings = settings.child(name_row);
                    }

                    // 2. 計算の種類 (集計方法)
                    settings = settings.child(div().pt(px(2.)).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.summarize-values-by")));
                    let mut fn_choices = h_flex().gap_1().flex_wrap();
                    for (function, label) in ibis_doc_model::pivot::FUNCTIONS {
                        let is_active = current_fn == function;
                        fn_choices = fn_choices.child(
                            Button::new(id(function))
                                .small()
                                .label(format!("{} {}", if is_active { "◉" } else { "○" }, text(label)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.change_pivot(target, |p| p.set_function(pos, function), cx);
                                    let new_name = this.presentation().slide(target.0)
                                        .and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(target.1) } else { None }))
                                        .and_then(|p| p.data_fields.get(pos).map(|df| df.name.clone()));
                                    if let Some(name) = new_name {
                                        if let Some(inp) = this.wb_mut().panels.pivot.as_mut().and_then(|p| p.value_name.as_ref()) {
                                            inp.update(cx, |i, cx| i.set_value(name, window, cx));
                                        }
                                    }
                                }))
                        );
                    }
                    settings = settings.child(fn_choices);

                    // 3. 表示形式
                    settings = settings.child(div().pt(px(2.)).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.show-values-as")));
                    const PIVOT_NUM_FORMATS: &[(&str, Option<&str>)] = &[
                        ("_xlsx.general", None),
                        ("_xlsx.number", Some("#,##0")),
                        ("_xlsx.number-2-decimals", Some("#,##0.00")),
                        ("_xlsx.currency", Some("\"¥\"#,##0;\"¥\"\\-#,##0")),
                        ("_xlsx.percentage", Some("0.0%")),
                        ("_xlsx.percentage-2-decimals", Some("0.00%")),
                    ];
                    let mut fmt_choices = h_flex().gap_1().flex_wrap();
                    for (label, fmt_code) in PIVOT_NUM_FORMATS {
                        let is_active = match fmt_code {
                            None => current_fmt == "General" || current_df.and_then(|d| d.number_format.as_deref()).is_none(),
                            Some(c) => current_fmt == *c,
                        };
                        let code_owned = fmt_code.map(str::to_string);
                        fmt_choices = fmt_choices.child(
                            Button::new(id(&format!("fmt-{}", label)))
                                .small()
                                .label(format!("{} {}", if is_active { "◉" } else { "○" }, text(label)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let code = code_owned.clone();
                                    this.change_pivot(target, |p| p.set_number_format(pos, code), cx);
                                }))
                        );
                    }
                    settings = settings.child(fmt_choices);

                    // 4. 計算の種類 (値の表示方法 / 比率)
                    settings = settings.child(div().pt(px(2.)).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.calculation-type-show-values-as")));
                    const SHOW_AS_OPTIONS: &[(&str, Option<&str>)] = &[
                        ("_xlsx.no-calculation", None),
                        ("_xlsx.of-grand-total", Some("percentOfTotal")),
                        ("_xlsx.of-column-total", Some("percentOfCol")),
                        ("_xlsx.of-row-total", Some("percentOfRow")),
                    ];
                    let mut show_choices = h_flex().gap_1().flex_wrap();
                    for (label, show_mode) in SHOW_AS_OPTIONS {
                        let is_active = match show_mode {
                            None => current_show == "normal" || current_df.and_then(|d| d.show_as.as_deref()).is_none(),
                            Some(m) => current_show == *m,
                        };
                        let mode_owned = show_mode.map(str::to_string);
                        show_choices = show_choices.child(
                            Button::new(id(&format!("show-{}", label)))
                                .small()
                                .label(format!("{} {}", if is_active { "◉" } else { "○" }, text(label)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let mode = mode_owned.clone();
                                    this.change_pivot(target, |p| p.set_show_as(pos, mode), cx);
                                }))
                        );
                    }
                    settings = settings.child(show_choices);

                    body = body.child(settings);
                }
                if (area == PivotAxis::Row || area == PivotAxis::Column) && selected_field == Some((area, pos)) {
                    if let Some(target_field) = pivot.fields.get(f) {
                        let mut field_settings = v_flex().gap_1().pl(px(16.)).pr(px(8.)).py(px(4.));

                        // 1. 小計設定
                        field_settings = field_settings.child(div().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.subtotals")));
                        let subtotal_on = target_field.default_subtotal;
                        let subtotal_top = target_field.subtotal_top;
                        field_settings = field_settings.child(
                            Button::new(id("toggle-subtotal"))
                                .small()
                                .label(format!("{} {}", if subtotal_on { "☑" } else { "☐" }, text("_xlsx.show-subtotals")))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.change_pivot(target, |p| {
                                        if let Some(fld) = p.fields.get_mut(f) {
                                            fld.default_subtotal = !fld.default_subtotal;
                                        }
                                    }, cx);
                                }))
                        );
                        if subtotal_on {
                            field_settings = field_settings.child(
                                Button::new(id("toggle-subtotal-top"))
                                    .small()
                                    .label(format!("{} {}", if subtotal_top { "☑" } else { "☐" }, text("_xlsx.show-subtotals-at-top-of-group")))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.change_pivot(target, |p| {
                                            if let Some(fld) = p.fields.get_mut(f) {
                                                fld.subtotal_top = !fld.subtotal_top;
                                            }
                                        }, cx);
                                    }))
                            );
                        }

                        // 2. レイアウト形式
                        field_settings = field_settings.child(div().pt(px(2.)).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.layout")));
                        let is_compact = target_field.compact;
                        let is_outline = target_field.outline;
                        let layout_kind = if is_compact {
                            ReportLayout::Compact
                        } else if is_outline {
                            ReportLayout::Outline
                        } else {
                            ReportLayout::Tabular
                        };

                        let mut layout_choices = h_flex().gap_1().flex_wrap();
                        for (lid, llabel, lkind) in [
                            ("compact", "_xlsx.compact-form", ReportLayout::Compact),
                            ("outline", "_xlsx.outline-form", ReportLayout::Outline),
                            ("tabular", "_xlsx.tabular-form", ReportLayout::Tabular),
                        ] {
                            let active = layout_kind == lkind;
                            layout_choices = layout_choices.child(
                                Button::new(id(lid))
                                    .small()
                                    .label(format!("{} {}", if active { "◉" } else { "○" }, text(llabel)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.change_pivot(target, |p| {
                                            if let Some(fld) = p.fields.get_mut(f) {
                                                fld.compact = lkind == ReportLayout::Compact;
                                                fld.outline = lkind != ReportLayout::Tabular;
                                            }
                                        }, cx);
                                    }))
                            );
                        }
                        field_settings = field_settings.child(layout_choices);

                        body = body.child(field_settings);
                    }
                }
            }
        }
        body = body.child(self.pivot_design_section(&pivot, target, cx));
        body = body.child(div().text_size(px(12.)).child(SharedString::from(message))).child(Button::new("pivot-close").small().label(text("_shell.close")).on_click(cx.listener(|this, _, _, cx| {
            this.wb_mut().panels.pivot = None;
            cx.notify();
        })));
        Some(SidebarPanel { title: "xlsx.pivot-fields".into(), body: body.into_any_element() })
    }
}

/// レポートのレイアウト.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportLayout {
    Compact,
    Outline,
    Tabular,
}

fn layout_of(p: &PivotTable) -> ReportLayout {
    let first = p.row_fields.first().and_then(|&f| p.fields.get(f.max(0) as usize));
    match first {
        Some(f) if f.compact && f.outline => ReportLayout::Compact,
        Some(f) if f.outline => ReportLayout::Outline,
        Some(_) => ReportLayout::Tabular,
        None if p.compact && p.outline => ReportLayout::Compact,
        None if p.outline => ReportLayout::Outline,
        None => ReportLayout::Tabular,
    }
}

/// The subtotals the row fields show: none, at the bottom of each group, or at its top.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Subtotals {
    Off,
    Bottom,
    Top,
}

fn subtotals_of(p: &PivotTable) -> Subtotals {
    let first = p.row_fields.first().and_then(|&f| p.fields.get(f.max(0) as usize));
    match first {
        Some(f) if !f.default_subtotal => Subtotals::Off,
        Some(f) if f.subtotal_top => Subtotals::Top,
        _ => Subtotals::Bottom,
    }
}

impl SlideEditor {
    /// デザイン: レポートのレイアウト, 小計, 総計, 項目ラベルの繰り返し, and the captions of empty and error values.
    fn pivot_design_section(&mut self, pivot: &PivotTable, target: (SlideId, usize), cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let mark = |on: bool| if on { "◉" } else { "○" };
        let check = |on: bool| if on { "☑" } else { "☐" };
        let layout = layout_of(pivot);
        let subtotals = subtotals_of(pivot);
        let mut body = v_flex().gap_1().pt(px(8.)).child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(text("_shell.layout")));
        let mut layouts = h_flex().gap_1().flex_wrap();
        for (id, label, kind) in [("pivot-layout-compact", "_xlsx.show-in-compact-form", ReportLayout::Compact), ("pivot-layout-outline", "_xlsx.show-in-outline-form", ReportLayout::Outline), ("pivot-layout-tabular", "_xlsx.show-in-tabular-form", ReportLayout::Tabular)] {
            layouts = layouts.child(Button::new(id).small().label(format!("{} {}", mark(layout == kind), text(label))).on_click(cx.listener(move |this, _, _, cx| {
                this.change_pivot(target, |p| {
                    p.compact = kind == ReportLayout::Compact;
                    p.outline = kind != ReportLayout::Tabular;
                    for f in &mut p.fields {
                        f.compact = p.compact;
                        f.outline = p.outline;
                    }
                }, cx)
            })));
        }
        body = body.child(layouts);
        let mut subs = h_flex().gap_1().flex_wrap();
        for (id, label, kind) in [("pivot-subtotal-off", "_xlsx.do-not-show-subtotals", Subtotals::Off), ("pivot-subtotal-bottom", "_xlsx.show-all-subtotals-at-bottom-of-group", Subtotals::Bottom), ("pivot-subtotal-top", "_xlsx.show-all-subtotals-at-top-of-group", Subtotals::Top)] {
            subs = subs.child(Button::new(id).small().label(format!("{} {}", mark(subtotals == kind), text(label))).on_click(cx.listener(move |this, _, _, cx| {
                this.change_pivot(target, |p| {
                    for f in &mut p.fields {
                        f.default_subtotal = kind != Subtotals::Off;
                        f.subtotal_top = kind == Subtotals::Top;
                    }
                }, cx)
            })));
        }
        body = body.child(subs);
        body = body
            .child(Button::new("pivot-grand-rows").small().label(format!("{} {}", check(pivot.row_grand_totals), text("_xlsx.show-grand-total-row"))).on_click(cx.listener(move |this, _, _, cx| this.change_pivot(target, |p| p.row_grand_totals = !p.row_grand_totals, cx))))
            .child(Button::new("pivot-grand-cols").small().label(format!("{} {}", check(pivot.col_grand_totals), text("_xlsx.show-grand-total-column"))).on_click(cx.listener(move |this, _, _, cx| this.change_pivot(target, |p| p.col_grand_totals = !p.col_grand_totals, cx))))
            .child(Button::new("pivot-fill-down").small().label(format!("{} {}", check(pivot.fill_down_labels), text("_xlsx.repeat-all-item-labels"))).on_click(cx.listener(move |this, _, _, cx| this.change_pivot(target, |p| p.fill_down_labels = !p.fill_down_labels, cx))));
        let inputs = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).map(|p| (p.missing.clone(), p.error.clone()));
        if let Some((Some(missing), Some(error))) = inputs {
            body = body
                .child(text("_xlsx.for-empty-cells-show"))
                .child(Input::new(&missing).small())
                .child(text("_xlsx.for-error-values-show"))
                .child(Input::new(&error).small())
                .child(Button::new("pivot-captions").small().label(text("_xlsx.apply-captions")).on_click(cx.listener(move |this, _, _, cx| {
                    let read = |this: &Self, pick: fn(&PivotPanel) -> Option<Entity<InputState>>, cx: &Context<Self>| this.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(pick).map(|e| e.read(cx).value().to_string()).unwrap_or_default();
                    let (m, e) = (read(this, |p| p.missing.clone(), cx), read(this, |p| p.error.clone(), cx));
                    this.change_pivot(target, move |p| {
                        p.missing_caption = (!m.is_empty()).then_some(m);
                        p.error_caption = (!e.is_empty()).then_some(e);
                    }, cx)
                })));
        }
        body.into_any_element()
    }
}

/// The cells a pivot table covers: its place and its report filters'.
fn pivot_area(p: &ibis_doc_model::PivotTable) -> Vec<ibis_doc_model::CellRange> {
    let loc = p.location;
    let pages = p.page_fields.len() as u32;
    let mut out = vec![loc];
    if pages > 0 {
        out.push(ibis_doc_model::CellRange { first_row: loc.first_row.saturating_sub(pages + 1), last_row: loc.first_row.saturating_sub(1), first_column: loc.first_column, last_column: loc.first_column + 1 });
    }
    out
}

/// Whether table `index` as built on `new` would cover cells of `old`
/// with something in them that are not its own (Excel will not write a
/// pivot table over them).
fn covers_others(old: &Sheet, new: &Sheet, index: usize) -> bool {
    let Some(p) = new.pivots.get(index) else { return false };
    let inside = |areas: &[ibis_doc_model::CellRange], r: u32, c: u32| areas.iter().any(|a| (a.first_row..=a.last_row).contains(&r) && (a.first_column..=a.last_column).contains(&c));
    let area = pivot_area(p);
    let own = old.pivots.get(index).map(pivot_area).unwrap_or_default();
    let others: Vec<ibis_doc_model::CellRange> = old.pivots.iter().enumerate().filter(|(i, _)| *i != index).flat_map(|(_, q)| pivot_area(q)).collect();
    if others.iter().any(|o| area.iter().any(|a| a.first_row <= o.last_row && o.first_row <= a.last_row && a.first_column <= o.last_column && o.first_column <= a.last_column)) {
        return true;
    }
    old.cells.iter().any(|c| (!c.value.is_empty() || c.formula.is_some()) && inside(&area, c.row, c.column) && !inside(&own, c.row, c.column))
}

/// A sheet's name as a reference writes it (quoted when it must be).
fn quote_sheet(name: &str) -> String {
    if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// `$A$1:$G$41`.
fn absolute(r: ibis_doc_model::CellRange) -> String {
    let cell = |c: u32, row: u32| {
        let name = ibis_doc_model::cell_name(c, row);
        let split = name.find(|ch: char| ch.is_ascii_digit()).unwrap_or(name.len());
        format!("${}${}", &name[..split], &name[split..])
    };
    format!("{}:{}", cell(r.first_column, r.first_row), cell(r.last_column, r.last_row))
}

/// A pivot button's (▼) panel: a field's items to show (ticked) and its
/// order, or a report filter's one item.
pub(crate) struct PivotPopup {
    pub(crate) target: (SlideId, usize),
    /// The fields it is for (行ラベル: the row fields), and the one shown.
    pub(crate) fields: Vec<usize>,
    pub(crate) field: usize,
    /// A report filter: its place among them.
    pub(crate) page: Option<usize>,
    pub(crate) position: gpui_kit::Point<gpui_kit::Pixels>,
    /// Each item of the field (by its place in its items): ticked to show,
    /// or (a report filter) the one chosen (`None`: すべて).
    pub(crate) ticked: Vec<bool>,
    pub(crate) chosen: Option<usize>,
    /// 検索.
    pub(crate) search: Entity<InputState>,
    /// ラベル フィルター ▸ / 値フィルター ▸ opened.
    pub(crate) section: Option<PopupSection>,
    /// The filter being asked for (its value, or the トップテン's).
    pub(crate) dialog: Option<PopupDialog>,
}

/// A part of the panel opened under its heading, as the オートフィルター's.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopupSection {
    Label,
    Value,
    Sort,
}

/// ラベル フィルター / 値フィルター / トップテン: what is asked.
pub(crate) struct PopupDialog {
    pub(crate) label: bool,
    /// The filter's type (`captionEqual`, …, `count`).
    pub(crate) kind: &'static str,
    /// The value filtered by.
    pub(crate) measure: usize,
    pub(crate) values: [Entity<InputState>; 2],
    /// トップテン: the top, or the bottom; 項目・パーセント・合計.
    pub(crate) top: bool,
    pub(crate) top_kind: &'static str,
    pub(crate) error: Option<SharedString>,
}

/// The label filters of the panel, as Excel lists them, and the value filters.
const LABEL_FILTERS: &[(&str, &str)] = &[
    ("_xlsx.equal-to-2", "captionEqual"),
    ("_xlsx.not-equal-to-2", "captionNotEqual"),
    ("_xlsx.begins-with-2", "captionBeginsWith"),
    ("_xlsx.does-not-begin-with-2", "captionNotBeginsWith"),
    ("_xlsx.ends-with-2", "captionEndsWith"),
    ("_xlsx.does-not-end-with-2", "captionNotEndsWith"),
    ("_xlsx.contains-2", "captionContains"),
    ("_xlsx.does-not-contain-2", "captionNotContains"),
    ("_xlsx.greater-than-3", "captionGreaterThan"),
    ("_xlsx.greater-than-or-equal-to-3", "captionGreaterThanOrEqual"),
    ("_xlsx.less-than-3", "captionLessThan"),
    ("_xlsx.less-than-or-equal-to-3", "captionLessThanOrEqual"),
    ("_xlsx.between-2", "captionBetween"),
    ("_xlsx.not-between-2", "captionNotBetween"),
];
const VALUE_FILTERS: &[(&str, &str)] = &[
    ("_xlsx.equal-to-2", "valueEqual"),
    ("_xlsx.not-equal-to-2", "valueNotEqual"),
    ("_xlsx.greater-than-3", "valueGreaterThan"),
    ("_xlsx.greater-than-or-equal-to-3", "valueGreaterThanOrEqual"),
    ("_xlsx.less-than-3", "valueLessThan"),
    ("_xlsx.less-than-or-equal-to-3", "valueLessThanOrEqual"),
    ("_xlsx.between-2", "valueBetween"),
    ("_xlsx.not-between-2", "valueNotBetween"),
    ("_xlsx.top-10", "count"),
];

/// A field's items as listed: their place among its items and their text.
fn item_labels(p: &PivotTable, f: usize) -> Vec<(usize, String)> {
    let Some(field) = p.fields.get(f) else { return Vec::new() };
    field
        .items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.kind.is_none())
        .filter_map(|(i, it)| {
            let v = field.shared.get(it.x? as usize)?;
            let t = ibis_doc_model::sheet_command::display_text(v, &Default::default(), false);
            Some((i, if t.is_empty() { crate::shell::i18n::text("_xlsx.blank-2").to_string() } else { t }))
        })
        .collect()
}

impl SlideEditor {
    pub(crate) fn pivot_def(&self, target: (SlideId, usize)) -> Option<PivotTable> {
        self.current_sheet_of(target.0).and_then(|s| s.pivots.get(target.1).cloned())
    }

    /// A pivot button pressed: its panel, for `button`.
    pub(crate) fn open_pivot_popup(&mut self, target: (SlideId, usize), button: ibis_doc_model::pivot::PivotButton, position: gpui_kit::Point<gpui_kit::Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        use ibis_doc_model::pivot::PivotButton as B;
        self.commit_cell_edit(cx);
        let Some(p) = self.pivot_def(target) else { return };
        let real = |list: &[i32]| list.iter().filter(|&&f| f >= 0).map(|&f| f as usize).collect::<Vec<_>>();
        let (fields, page) = match button {
            B::Rows => (real(&p.row_fields), None),
            B::Columns => (real(&p.col_fields), None),
            B::Field(f) => (vec![f], None),
            B::Page(i) => (p.page_fields.get(i).map(|pf| vec![pf.field as usize]).unwrap_or_default(), Some(i)),
        };
        let Some(&field) = fields.first() else { return };
        let search = cx.new(|cx| InputState::new(window, cx));
        let mut popup = PivotPopup { target, fields, field, page, position, ticked: Vec::new(), chosen: None, search, section: None, dialog: None };
        self.fill_popup(&p, &mut popup);
        self.wb_mut().panels.pivot_popup = Some(popup);
        cx.notify();
    }

    fn fill_popup(&self, p: &PivotTable, popup: &mut PivotPopup) {
        let field = &p.fields[popup.field];
        popup.ticked = field.items.iter().map(|it| !it.hidden).collect();
        popup.chosen = popup.page.and_then(|i| p.page_fields.get(i)).and_then(|pf| pf.item).map(|i| i as usize);
    }

    /// Another field of 行ラベル's (列ラベル's) panel shown.
    pub(crate) fn pivot_popup_field(&mut self, field: usize, cx: &mut Context<Self>) {
        let Some(target) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()).map(|p| p.target) else { return };
        let Some(p) = self.pivot_def(target) else { return };
        let mut popup = self.wb_mut().panels.pivot_popup.take().expect("open");
        popup.field = field;
        self.fill_popup(&p, &mut popup);
        self.wb_mut().panels.pivot_popup = Some(popup);
        cx.notify();
    }

    pub(crate) fn close_pivot_popup(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.panels.pivot_popup = None;
        }
        cx.notify();
    }

    /// Tick or untick item `i` (`None`: すべて選択).
    pub(crate) fn pivot_popup_tick(&mut self, i: Option<usize>, on: bool, cx: &mut Context<Self>) {
        // With something searched for, すべて選択 is of the items found.
        let shown = self.sheet_state().and_then(|st| st.panels.pivot_popup.as_ref()).and_then(|popup| {
            let query = popup.search.read(cx).value().trim().to_lowercase();
            let p = self.pivot_def(popup.target)?;
            (!query.is_empty()).then(|| item_labels(&p, popup.field).into_iter().filter(|(_, l)| l.to_lowercase().contains(&query)).map(|(i, _)| i).collect::<Vec<_>>())
        });
        if let Some(p) = self.wb_mut().panels.pivot_popup.as_mut() {
            match i {
                Some(i) => {
                    if let Some(t) = p.ticked.get_mut(i) {
                        *t = on;
                    }
                }
                None => match shown {
                    Some(shown) => shown.iter().for_each(|&i| {
                        if let Some(t) = p.ticked.get_mut(i) {
                            *t = on;
                        }
                    }),
                    None => p.ticked.iter_mut().for_each(|t| *t = on),
                },
            }
        }
        cx.notify();
    }

    /// A report filter's item chosen (`None`: すべて).
    pub(crate) fn pivot_popup_choose(&mut self, i: Option<usize>, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.pivot_popup.as_mut() {
            p.chosen = i;
        }
        cx.notify();
    }

    /// 昇順・降順: the field's items sorted by label (the table built again).
    pub(crate) fn pivot_popup_sort(&mut self, ascending: bool, cx: &mut Context<Self>) {
        let Some((target, f)) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()).map(|p| (p.target, p.field)) else { return };
        self.close_pivot_popup(cx);
        self.change_pivot(target, move |p| p.sort_field_by_label(f, ascending), cx);
    }

    /// 値で並べ替え: the field's items sorted by data field `df_idx` (the table built again).
    pub(crate) fn pivot_popup_sort_by_value(&mut self, df_idx: usize, ascending: bool, cx: &mut Context<Self>) {
        let Some((target, f)) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()).map(|p| (p.target, p.field)) else { return };
        self.close_pivot_popup(cx);
        self.change_pivot(target, move |p| p.sort_field_by_value(f, df_idx, ascending), cx);
    }

    /// OK: the items ticked shown (at least one), or the report filter's
    /// item kept.
    pub(crate) fn pivot_popup_ok(&mut self, cx: &mut Context<Self>) {
        let Some(popup) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()) else { return };
        let (target, f, page, chosen, ticked) = (popup.target, popup.field, popup.page, popup.chosen, popup.ticked.clone());
        let Some(p) = self.pivot_def(target) else { return };
        if page.is_none() && !p.fields[f].items.iter().zip(&ticked).any(|(it, &t)| it.kind.is_none() && t) {
            return;
        }
        self.close_pivot_popup(cx);
        self.change_pivot(target, move |p| match page {
            Some(i) => {
                if let Some(pf) = p.page_fields.get_mut(i) {
                    pf.item = chosen.map(|c| c as u32);
                }
            }
            None => {
                for (it, t) in p.fields[f].items.iter_mut().zip(ticked) {
                    if it.kind.is_none() {
                        it.hidden = !t;
                    }
                }
            }
        }, cx);
    }

    pub(crate) fn pivot_popup_section(&mut self, section: PopupSection, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.pivot_popup.as_mut() {
            p.section = if p.section == Some(section) { None } else { Some(section) };
        }
        cx.notify();
    }

    /// ラベル フィルター / 値フィルター の 1 つを選んだ: its value asked for (what the field has now in it).
    pub(crate) fn pivot_popup_ask(&mut self, label: bool, kind: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, field)) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()).map(|p| (p.target, p.field)) else { return };
        let Some(p) = self.pivot_def(target) else { return };
        let current = p.filters.iter().find(|f| f.field as usize == field && f.is_label() == label).cloned();
        let same = current.as_ref().filter(|f| f.kind == kind || (kind == "count" && matches!(f.kind.as_str(), "count" | "percent" | "sum")));
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let values = [input(same.map(|f| f.value.clone()).unwrap_or_else(|| if kind == "count" { "10".into() } else { String::new() }), window, cx), input(same.and_then(|f| f.value2.clone()).unwrap_or_default(), window, cx)];
        let top_kind = match same.map(|f| f.kind.as_str()) {
            Some("percent") => "percent",
            Some("sum") => "sum",
            _ => "count",
        };
        let dialog = PopupDialog { label, kind, measure: same.map_or(0, |f| f.measure as usize), values, top: same.is_none_or(|f| f.top), top_kind, error: None };
        if let Some(popup) = self.wb_mut().panels.pivot_popup.as_mut() {
            popup.dialog = Some(dialog);
        }
        cx.notify();
    }

    pub(crate) fn close_pivot_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.pivot_popup.as_mut() {
            p.dialog = None;
        }
        cx.notify();
    }

    pub(crate) fn with_pivot_dialog(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut PopupDialog)) {
        if let Some(d) = self.wb_mut().panels.pivot_popup.as_mut().and_then(|p| p.dialog.as_mut()) {
            f(d);
        }
        cx.notify();
    }

    /// OK of the ラベル フィルター / 値フィルター / トップテン dialog: the filter set on the field (the table built again).
    pub(crate) fn pivot_popup_dialog_ok(&mut self, cx: &mut Context<Self>) {
        let Some(popup) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()) else { return };
        let Some(d) = popup.dialog.as_ref() else { return };
        let (target, field) = (popup.target, popup.field as u32);
        let (label, mut kind, measure, top) = (d.label, d.kind, d.measure as u32, d.top);
        let (a, b) = (d.values[0].read(cx).value().trim().to_string(), d.values[1].read(cx).value().trim().to_string());
        if kind == "count" {
            kind = d.top_kind;
        }
        let between = kind.ends_with("Between") || kind.ends_with("NotBetween");
        let numeric = !label || matches!(kind, "count" | "percent" | "sum");
        let bad = |t: &str| t.is_empty() || (numeric && t.parse::<f64>().is_err());
        let error = if bad(&a) || (between && bad(&b)) {
            Some(text(if numeric { "_xlsx.enter-a-number" } else { "_xlsx.enter-a-value" }))
        } else if matches!(kind, "count" | "percent" | "sum") && a.parse::<f64>().is_ok_and(|n| n <= 0.0) {
            Some(text("_xlsx.enter-a-number-greater-than-0"))
        } else {
            None
        };
        if let Some(e) = error {
            return self.with_pivot_dialog(cx, |d| d.error = Some(e));
        }
        self.close_pivot_popup(cx);
        let filter = ibis_doc_model::PivotFilter { field, kind: kind.to_string(), measure, value: a, value2: between.then_some(b), top };
        self.change_pivot(target, move |p| {
            p.filters.retain(|f| !(f.field == field && f.is_label() == label));
            p.filters.push(filter);
        }, cx);
    }

    /// "…" からフィルターをクリア: the field's filters and hidden items gone.
    pub(crate) fn pivot_popup_clear(&mut self, cx: &mut Context<Self>) {
        let Some((target, f)) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()).map(|p| (p.target, p.field)) else { return };
        self.close_pivot_popup(cx);
        self.change_pivot(target, move |p| {
            p.filters.retain(|x| x.field as usize != f);
            for it in &mut p.fields[f].items {
                it.hidden = false;
            }
        }, cx);
    }

    /// The pivot buttons of the current sheet, over their cells' right ends.
    pub(crate) fn pivot_buttons(&self, cx: &mut Context<Self>) -> Vec<gpui_kit::AnyElement> {
        use gpui_kit::{InteractiveElement as _, MouseButton, MouseDownEvent, rgb};
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let Some(g) = self.sheet_state().map(|s| s.header_geometry.borrow().clone()) else { return Vec::new() };
        let (ox, oy) = g.origin;
        let slide = self.current_slide;
        let mut out = Vec::new();
        for (index, p) in sheet.pivots.iter().enumerate() {
            for (row, column, button) in p.button_places() {
                let Some(&(_, top, bottom)) = g.rows.iter().find(|r| r.0 == row) else { continue };
                let Some(&(_, _, right)) = g.columns.iter().find(|c| c.0 == column) else { continue };
                let side = (bottom - top).clamp(12.0, 18.0);
                let filtered = match button {
                    ibis_doc_model::pivot::PivotButton::Page(i) => p.page_fields.get(i).is_some_and(|pf| pf.item.is_some()),
                    _ => {
                        let fields: Vec<usize> = match button {
                            ibis_doc_model::pivot::PivotButton::Rows => p.row_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).collect(),
                            ibis_doc_model::pivot::PivotButton::Columns => p.col_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).collect(),
                            ibis_doc_model::pivot::PivotButton::Field(f) => vec![f],
                            ibis_doc_model::pivot::PivotButton::Page(_) => Vec::new(),
                        };
                        fields.iter().any(|&f| p.fields.get(f).is_some_and(|fl| fl.items.iter().any(|i| i.hidden)) || p.filters.iter().any(|x| x.field as usize == f))
                    }
                };
                out.push(
                    div()
                        .id(SharedString::from(format!("pivot-button-{index}-{row}-{column}")))
                        .absolute()
                        .left(px(right - ox - side - 2.0))
                        .top(px(bottom - oy - side - 2.0))
                        .size(px(side))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_color(rgb(0xA6A6A6))
                        .text_size(px(side * 0.55))
                        .text_color(if filtered { rgb(0x217346) } else { rgb(0x404040) })
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_pivot_popup((slide, index), button, e.position, window, cx);
                        }))
                        .child(if filtered { "▼●" } else { "▼" })
                        .into_any_element(),
                );
            }
        }
        out
    }

    /// ラベル フィルター / 値フィルター / トップテン: the value asked for, as the オートフィルターの 指定の値… dialog.
    fn render_pivot_filter_dialog(&self, p: &PivotTable, field: usize, d: &PopupDialog, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        use gpui_kit::component::ActiveTheme as _;
        let theme = cx.theme().clone();
        let name = p.fields[field].name.clone();
        let title = if d.label { "_xlsx.label-filters" } else { "_xlsx.value-filters" };
        let top = d.kind == "count";
        let mut body = v_flex().gap_2().child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(SharedString::from(format!("{} ({name})", text(title)))));
        if !d.label && p.data_fields.len() > 1 {
            let mut choose = h_flex().gap_1().flex_wrap();
            for (i, df) in p.data_fields.iter().enumerate() {
                choose = choose.child(Button::new(SharedString::from(format!("pivot-dialog-measure-{i}"))).small().label(format!("{} {}", if d.measure == i { "◉" } else { "○" }, df.name)).on_click(cx.listener(move |this, _, _, cx| this.with_pivot_dialog(cx, |d| d.measure = i))));
            }
            body = body.child(div().child(text("_xlsx.values"))).child(choose);
        }
        if top {
            let kinds: [(&'static str, &str); 3] = [("count", "pptx.pivot.category"), ("percent", "_xlsx.percent-2"), ("sum", "_xlsx.sum")];
            let mut unit = h_flex().gap_1();
            for (kind, label) in kinds {
                unit = unit.child(Button::new(SharedString::from(format!("pivot-dialog-{kind}"))).small().label(format!("{} {}", if d.top_kind == kind { "◉" } else { "○" }, text(label))).on_click(cx.listener(move |this, _, _, cx| this.with_pivot_dialog(cx, |d| d.top_kind = kind))));
            }
            body = body
                .child(h_flex().gap_1().child(Button::new("pivot-dialog-top").small().label(format!("{} {}", if d.top { "◉" } else { "○" }, text("_xlsx.top"))).on_click(cx.listener(|this, _, _, cx| this.with_pivot_dialog(cx, |d| d.top = true)))).child(Button::new("pivot-dialog-bottom").small().label(format!("{} {}", if d.top { "○" } else { "◉" }, text("_xlsx.bottom"))).on_click(cx.listener(|this, _, _, cx| this.with_pivot_dialog(cx, |d| d.top = false)))))
                .child(Input::new(&d.values[0]).small())
                .child(unit);
        } else {
            let label = LABEL_FILTERS.iter().chain(VALUE_FILTERS).find(|(_, k)| *k == d.kind).map_or("", |(l, _)| l.trim_end_matches('…'));
            body = body.child(div().child(text(label))).child(Input::new(&d.values[0]).small());
            if d.kind.ends_with("Between") {
                body = body.child(div().child(text("_xlsx.and"))).child(Input::new(&d.values[1]).small());
            }
            if d.label {
                body = body.child(div().text_color(theme.muted_foreground).child(text("_xlsx.use-to-represent-any-single-character-use-to-rep")));
            }
        }
        body.when_some(d.error.clone(), |b, e| b.child(div().text_color(theme.danger).child(e)))
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .pt_1()
                    .child(Button::new("pivot-dialog-ok").primary().small().label("OK").on_click(cx.listener(|this, _, _, cx| this.pivot_popup_dialog_ok(cx))))
                    .child(Button::new("pivot-dialog-cancel").small().label(text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| this.close_pivot_dialog(cx)))),
            )
            .into_any_element()
    }

    /// The open pivot button's panel.
    pub(crate) fn render_pivot_popup(&self, cx: &mut Context<Self>) -> Option<gpui_kit::AnyElement> {
        use gpui_kit::component::ActiveTheme as _;
        use gpui_kit::component::checkbox::Checkbox;
        use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _, anchored, deferred};
        let popup = self.sheet_state()?.panels.pivot_popup.as_ref()?;
        let p = self.pivot_def(popup.target)?;
        let theme = cx.theme().clone();
        let items = item_labels(&p, popup.field);
        let mut body = v_flex().gap_1();
        if popup.fields.len() > 1 {
            let mut choose = h_flex().gap_1().flex_wrap().child(text("_xlsx.select-field"));
            for &f in &popup.fields {
                let on = f == popup.field;
                choose = choose.child(Button::new(SharedString::from(format!("pivot-popup-field-{f}"))).small().label(format!("{} {}", if on { "◉" } else { "○" }, p.fields[f].name)).on_click(cx.listener(move |this, _, _, cx| this.pivot_popup_field(f, cx))));
            }
            body = body.child(choose);
        }
        if let Some(_) = popup.page {
            let chosen = popup.chosen;
            body = body.child(Button::new("pivot-popup-all").small().label(format!("{} {}", if chosen.is_none() { "◉" } else { "○" }, text("_xlsx.all-2"))).on_click(cx.listener(|this, _, _, cx| this.pivot_popup_choose(None, cx))));
            for (i, label) in items {
                body = body.child(Button::new(SharedString::from(format!("pivot-popup-item-{i}"))).small().label(format!("{} {}", if chosen == Some(i) { "◉" } else { "○" }, label)).on_click(cx.listener(move |this, _, _, cx| this.pivot_popup_choose(Some(i), cx))));
            }
        } else if popup.dialog.is_none() {
            // As the オートフィルター's panel: sorting, clearing, the label and value filters, the search box and the items.
            let name = p.fields[popup.field].name.clone();
            let filtered = p.filters.iter().any(|f| f.field as usize == popup.field) || p.fields[popup.field].items.iter().any(|it| it.hidden);
            let arrow = |open: bool| if open { " ▾" } else { " ▸" };
            let mut menu = v_flex()
                .child(self.panel_row("pivot-popup-asc", "_xlsx.sort-ascending", true, Rc::new(|this, _, cx| this.pivot_popup_sort(true, cx)), cx))
                .child(self.panel_row("pivot-popup-desc", "_xlsx.sort-descending", true, Rc::new(|this, _, cx| this.pivot_popup_sort(false, cx)), cx));
            if !p.data_fields.is_empty() {
                menu = menu.child(self.panel_row("pivot-popup-sort-value", format!("{}{}", text("_xlsx.sort-by-value"), arrow(popup.section == Some(PopupSection::Sort))), true, Rc::new(|this, _, cx| this.pivot_popup_section(PopupSection::Sort, cx)), cx));
                if popup.section == Some(PopupSection::Sort) {
                    let mut sort_list = v_flex().pl_3();
                    for (df_idx, df) in p.data_fields.iter().enumerate() {
                        let df_name = &df.name;
                        let asc_label = crate::shell::i18n::format("_xlsx.df-name-ascending", &[("df_name", &df_name)]);
                        let desc_label = crate::shell::i18n::format("_xlsx.df-name-descending", &[("df_name", &df_name)]);
                        sort_list = sort_list
                            .child(self.panel_row(format!("pivot-popup-sort-val-asc-{df_idx}"), asc_label, true, Rc::new(move |this, _, cx| this.pivot_popup_sort_by_value(df_idx, true, cx)), cx))
                            .child(self.panel_row(format!("pivot-popup-sort-val-desc-{df_idx}"), desc_label, true, Rc::new(move |this, _, cx| this.pivot_popup_sort_by_value(df_idx, false, cx)), cx));
                    }
                    menu = menu.child(sort_list);
                }
            }
            menu = menu
                .child(div().h(px(1.)).my_1().bg(theme.border))
                .child(self.panel_row("pivot-popup-clear", crate::shell::i18n::format("_xlsx.clear-filter-from-name", &[("name", &name)]), filtered, Rc::new(|this, _, cx| this.pivot_popup_clear(cx)), cx))
                .child(self.panel_row("pivot-popup-label-filters", format!("{}{}", text("_xlsx.label-filters"), arrow(popup.section == Some(PopupSection::Label))), true, Rc::new(|this, _, cx| this.pivot_popup_section(PopupSection::Label, cx)), cx));
            if popup.section == Some(PopupSection::Label) {
                let mut list = v_flex().pl_3();
                for &(label, kind) in LABEL_FILTERS {
                    list = list.child(self.panel_row(format!("pivot-popup-{kind}"), label, true, Rc::new(move |this, window, cx| this.pivot_popup_ask(true, kind, window, cx)), cx));
                }
                menu = menu.child(list);
            }
            menu = menu.child(self.panel_row("pivot-popup-value-filters", format!("{}{}", text("_xlsx.value-filters"), arrow(popup.section == Some(PopupSection::Value))), !p.data_fields.is_empty(), Rc::new(|this, _, cx| this.pivot_popup_section(PopupSection::Value, cx)), cx));
            if popup.section == Some(PopupSection::Value) && !p.data_fields.is_empty() {
                let mut list = v_flex().pl_3();
                for &(label, kind) in VALUE_FILTERS {
                    list = list.child(self.panel_row(format!("pivot-popup-{kind}"), label, true, Rc::new(move |this, window, cx| this.pivot_popup_ask(false, kind, window, cx)), cx));
                }
                menu = menu.child(list);
            }
            let query = popup.search.read(cx).value().trim().to_lowercase();
            let searching = !query.is_empty();
            let shown: Vec<(usize, String)> = items.iter().filter(|(_, l)| !searching || l.to_lowercase().contains(&query)).cloned().collect();
            let all = shown.iter().all(|(i, _)| popup.ticked.get(*i).copied().unwrap_or(true));
            let mut list = v_flex().child(Checkbox::new("pivot-popup-all").label(text(if searching { "_xlsx.select-all-search-results" } else { "_xlsx.select-all" })).checked(all).on_click(cx.listener(|this, on: &bool, _, cx| this.pivot_popup_tick(None, *on, cx))));
            for (i, label) in shown {
                let on = popup.ticked.get(i).copied().unwrap_or(true);
                list = list.child(Checkbox::new(SharedString::from(format!("pivot-popup-item-{i}"))).label(label).checked(on).on_click(cx.listener(move |this, on: &bool, _, cx| this.pivot_popup_tick(Some(i), *on, cx))));
            }
            body = body
                .child(menu)
                .child(div().h(px(1.)).my_1().bg(theme.border))
                .child(Input::new(&popup.search).small())
                .child(div().id("pivot-popup-items").h(px(200.)).overflow_y_scroll().border_1().border_color(theme.border).rounded_sm().child(list));
        }
        if let Some(d) = popup.dialog.as_ref() {
            body = body.child(self.render_pivot_filter_dialog(&p, popup.field, d, cx));
        } else {
            body = body.child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .pt_1()
                    .child(Button::new("pivot-popup-ok").primary().small().label("OK").on_click(cx.listener(|this, _, _, cx| this.pivot_popup_ok(cx))))
                    .child(Button::new("pivot-popup-cancel").small().label(text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| this.close_pivot_popup(cx)))),
            );
        }
        let panel = v_flex()
            .id("pivot-popup")
            .w(px(if popup.dialog.is_some() { 360. } else { 280. }))
            .max_h(px(560.))
            .overflow_y_scroll()
            .p_2()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .shadow_lg()
            .text_size(px(12.))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_pivot_popup(cx)))
            .child(body);
        Some(deferred(anchored().position(popup.position).snap_to_window_with_margin(px(8.)).child(panel)).with_priority(2).into_any_element())
    }
}
