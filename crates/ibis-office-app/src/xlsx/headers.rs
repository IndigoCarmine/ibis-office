// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The row and column headers as Excel works them: a click selects the
//! whole column or row (Shift extends), a drag on the line between two
//! headers sizes the one before it (to the selected ones, if it is among
//! them; to nothing, it hides it), a double-click there fits the column to
//! its widest content, and the right button opens the header's menu
//! (insert, delete, hide, show, clear formats).

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::*;
use ibis_doc_model::{Command, EMU_PER_POINT, Emu, SheetCommand, sheet_command::Axis};

use gpui_kit::component::menu::PopupMenu;

use crate::shell::chrome::{MenuEntry, MenuItem, fill_menu};
use crate::shell::editor::SlideEditor;

/// How near the line between two headers (in pixels) a press takes it.
const GRIP: f32 = 4.0;

/// Where the headers were last painted, in window pixels: (index, start,
/// end) of each column and row shown, and px per EMU.
#[derive(Clone, Debug, Default)]
pub(crate) struct HeaderGeometry {
    pub(crate) columns: Vec<(u32, f32, f32)>,
    pub(crate) rows: Vec<(u32, f32, f32)>,
    pub(crate) scale: f32,
    /// The top-left corner of the headers (the workspace's).
    pub(crate) origin: (f32, f32),
}

pub(crate) type SharedGeometry = Rc<RefCell<HeaderGeometry>>;

/// A header's line being dragged.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HeaderDrag {
    pub(crate) axis: Axis,
    pub(crate) index: u32,
    /// Where the column or row starts, and where the pointer is, in window pixels.
    pub(crate) start: f32,
    pub(crate) current: f32,
}

/// The header's menu, open.
pub(crate) struct HeaderPopup {
    pub(crate) menu: Entity<PopupMenu>,
    pub(crate) position: Point<Pixels>,
    _dismiss: Subscription,
}

/// What a press on a header is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HeaderHit {
    /// The line after column or row `index` (starting at `start`).
    Line { index: u32, start: f32 },
    /// Column or row `index` itself.
    Header(u32),
}

pub(crate) fn hit(spans: &[(u32, f32, f32)], at: f32) -> Option<HeaderHit> {
    // The line first. Hidden columns' lines are all where the one before
    // them ends: left of it, the visible column is sized; right of it, the
    // last hidden one is dragged out, as in Excel.
    let near: Vec<&(u32, f32, f32)> = spans.iter().filter(|(_, _, end)| (at - end).abs() <= GRIP).collect();
    if let Some(&&(_, _, end)) = near.first() {
        let pick = if at < end { near.iter().filter(|(_, s, e)| e > s).last().or(near.last()) } else { near.last() };
        if let Some(&&(index, start, _)) = pick {
            return Some(HeaderHit::Line { index, start });
        }
    }
    spans.iter().find(|(_, start, end)| (*start..*end).contains(&at)).map(|&(index, _, _)| HeaderHit::Header(index))
}

impl SlideEditor {
    /// The columns (or rows) selected whole, if the selection is that.
    pub(crate) fn selected_lines(&self, axis: Axis) -> Option<(u32, u32)> {
        let (_, sheet) = self.current_sheet()?;
        let r = self.selected_range()?;
        match axis {
            Axis::Columns => (r.first_row == 0 && r.last_row as usize + 1 >= sheet.rows.len()).then_some((r.first_column, r.last_column)),
            Axis::Rows => (r.first_column == 0 && r.last_column as usize + 1 >= sheet.columns.len()).then_some((r.first_row, r.last_row)),
        }
    }

    fn header_geometry(&self) -> HeaderGeometry {
        self.sheet_state().map(|s| s.header_geometry.borrow().clone()).unwrap_or_default()
    }

    /// ⌃Space / ⇧Space: the columns (rows) the selection covers, whole. While
    /// typing, ⇧Space is a space.
    pub(crate) fn select_selection_lines(&mut self, axis: Axis, cx: &mut Context<Self>) {
        if self.cell_edit().is_some() {
            if axis == Axis::Rows {
                self.cell_replace(None, " ", None, cx);
            }
            return;
        }
        let Some(r) = self.selected_range() else { return };
        let (first, last) = match axis {
            Axis::Columns => (r.first_column, r.last_column),
            Axis::Rows => (r.first_row, r.last_row),
        };
        self.select_lines(axis, first, last, cx);
    }

    /// Select whole columns or rows, from `first` to `last` (the anchor at `first`).
    pub(crate) fn select_lines(&mut self, axis: Axis, first: u32, last: u32, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let (rows, columns) = (sheet.rows.len().max(1) as u32 - 1, sheet.columns.len().max(1) as u32 - 1);
        let (anchor, end) = match axis {
            Axis::Columns => ((first, 0), (last, rows)),
            Axis::Rows => ((0, first), (columns, last)),
        };
        let slide = self.current_slide;
        self.selection = None;
        let state = self.wb_mut();
        state.cells.insert(slide, anchor);
        state.cell_ends.insert(slide, end);
        cx.notify();
    }

    /// Press on a header strip.
    pub(crate) fn on_header_down(&mut self, axis: Axis, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.focus_handle.focus(window, cx);
        let geometry = self.header_geometry();
        let (spans, at) = match axis {
            Axis::Columns => (&geometry.columns, f32::from(event.position.x)),
            Axis::Rows => (&geometry.rows, f32::from(event.position.y)),
        };
        match hit(spans, at) {
            Some(HeaderHit::Line { index, start }) => {
                if event.click_count >= 2 && axis == Axis::Columns {
                    return self.fit_columns(index, window, cx);
                }
                self.commit_cell_edit(cx);
                self.wb_mut().header_drag = Some(HeaderDrag { axis, index, start, current: at });
                cx.notify();
            }
            Some(HeaderHit::Header(index)) => {
                let anchor = self.selected_range().map(|r| match axis {
                    Axis::Columns => r.first_column.min(r.last_column),
                    Axis::Rows => r.first_row.min(r.last_row),
                });
                let anchor_cell = self.current_cell().map(|(c, r)| if axis == Axis::Columns { c } else { r });
                match (event.modifiers.shift, anchor_cell.or(anchor)) {
                    (true, Some(a)) => self.select_lines(axis, a, index, cx),
                    // ⌘: the column (row) added to those selected.
                    _ if event.modifiers.platform => {
                        let kept = self.selected_ranges();
                        self.select_lines(axis, index, index, cx);
                        self.keep_with_selection(kept);
                    }
                    _ => self.select_lines(axis, index, index, cx),
                }
                self.wb_mut().header_select = Some(axis);
            }
            None => {}
        }
    }

    /// The right button on a header: the column or row pressed is selected
    /// unless it is already (the menu then acts on the selection).
    pub(crate) fn on_header_right_down(&mut self, axis: Axis, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let geometry = self.header_geometry();
        let (spans, at) = match axis {
            Axis::Columns => (&geometry.columns, f32::from(event.position.x)),
            Axis::Rows => (&geometry.rows, f32::from(event.position.y)),
        };
        let index = match hit(spans, at) {
            Some(HeaderHit::Header(i)) | Some(HeaderHit::Line { index: i, .. }) => i,
            None => return,
        };
        // Within whole columns (rows) selected: the menu is for them.
        let within = self.selected_lines(axis).is_some_and(|(a, b)| (a..=b).contains(&index));
        if !within {
            self.select_lines(axis, index, index, cx);
        }
        self.open_header_menu(axis, event.position, window, cx);
    }

    /// Open the header's menu at `position`.
    pub(crate) fn open_header_menu(&mut self, axis: Axis, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let entries = self.header_menu(axis);
        self.open_popup_menu(entries, position, window, cx);
    }

    /// Open a pop-up menu of `entries` at `position` (the header's, a
    /// cell's, a sheet tab's).
    pub(crate) fn open_popup_menu(&mut self, entries: Vec<MenuEntry>, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let entries = std::rc::Rc::new(entries);
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| fill_menu(menu.min_w(px(240.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(state) = this.sheet_state_mut() {
                state.header_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.wb_mut().header_popup = Some(HeaderPopup { menu, position, _dismiss: dismiss });
        cx.notify();
    }

    /// The pointer moves (over the workspace): a line dragged follows it, a
    /// header selection stretches.
    pub(crate) fn on_header_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button.is_none() {
            let cell = self.cell_under(event.position);
            self.show_comment_card(cell, cx);
        }
        let geometry = self.header_geometry();
        let Some(state) = self.sheet_state_mut() else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            state.header_drag = None;
            state.header_select = None;
            // On a header's line: the resize pointer.
            let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
            let (ox, oy) = geometry.origin;
            let (head_w, head_h) = (super::sheet_ui::ROW_HEADER_WIDTH, super::sheet_ui::COLUMN_HEADER_HEIGHT);
            let hover = if (oy..oy + head_h).contains(&y) && x >= ox + head_w {
                matches!(hit(&geometry.columns, x), Some(HeaderHit::Line { .. })).then_some(Axis::Columns)
            } else if (ox..ox + head_w).contains(&x) && y >= oy + head_h {
                matches!(hit(&geometry.rows, y), Some(HeaderHit::Line { .. })).then_some(Axis::Rows)
            } else {
                None
            };
            if state.header_hover != hover {
                state.header_hover = hover;
                cx.notify();
            }
            return;
        }
        if let Some(drag) = state.header_drag.as_mut() {
            drag.current = match drag.axis {
                Axis::Columns => f32::from(event.position.x),
                Axis::Rows => f32::from(event.position.y),
            };
            return cx.notify();
        }
        if let Some(axis) = state.header_select {
            let geometry = self.header_geometry();
            let (spans, at) = match axis {
                Axis::Columns => (&geometry.columns, f32::from(event.position.x)),
                Axis::Rows => (&geometry.rows, f32::from(event.position.y)),
            };
            let index = spans.iter().find(|(_, s, e)| (*s..*e).contains(&at)).map(|s| s.0);
            if let (Some(index), Some((c, r))) = (index, self.current_cell()) {
                let was_kept = self.selected_ranges().len() > 1;
                self.select_lines(axis, if axis == Axis::Columns { c } else { r }, index, cx);
                self.extra_ranges_follow(was_kept);
            }
        }
    }

    /// The button comes up: a dragged line sets the size.
    pub(crate) fn on_header_up(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.sheet_state_mut() else { return };
        state.header_select = None;
        let Some(drag) = state.header_drag.take() else { return };
        let scale = self.header_geometry().scale;
        if scale <= 0.0 {
            return cx.notify();
        }
        // Less than a couple of pixels hides it, as in Excel.
        let px_size = (drag.current - drag.start).max(0.0);
        let size: Emu = if px_size < 2.0 { 0 } else { ((px_size / scale) as Emu / (EMU_PER_POINT / 4)) * (EMU_PER_POINT / 4) };
        // The selected columns or rows too, if the one dragged is among them.
        let (first, last) = self.selected_lines(drag.axis).filter(|(a, b)| (*a..=*b).contains(&drag.index)).unwrap_or((drag.index, drag.index));
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetSize { slide, axis: drag.axis, first, last, size })]) {
            log::warn!("sizing rejected: {err}");
        }
        cx.notify();
    }

    /// Fit column `index` (and the selected ones, if it is among them) to
    /// the widest text in it, as a double-click on its right line does.
    pub(crate) fn fit_columns(&mut self, index: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let (first, last) = self.selected_lines(Axis::Columns).filter(|(a, b)| (*a..=*b).contains(&index)).unwrap_or((index, index));
        let ts = window.text_system().clone();
        let mut commands = Vec::new();
        let slide = self.current_slide;
        for column in first..=last {
            let mut widest: f32 = 0.0;
            for cell in sheet.cells.iter().filter(|c| c.column == column && !c.text.is_empty()) {
                // A merged cell does not count, as in Excel.
                if sheet.merge_at(cell.row, cell.column).is_some() {
                    continue;
                }
                let style = &sheet.styles[cell.style as usize];
                widest = widest.max(self.excel_text_width(&cell.text, style, &ts));
            }
            if widest <= 0.0 {
                continue;
            }
            let size = fitted_width(widest);
            commands.push(Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Columns, first: column, last: column, size }));
        }
        if commands.is_empty() {
            return;
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("fitting rejected: {err}");
        }
        cx.notify();
    }

    /// How wide Excel draws a cell's text, in points. Excel measures in its
    /// own font, whose digit is the workbook's digit width (0.55 em, rounded,
    /// at the default size): the width as drawn here, scaled so a digit is
    /// that; its slanted (synthesized) italic needs about 2 pt more (tried in Excel).
    pub(crate) fn excel_text_width(&self, text: &str, style: &ibis_doc_model::CellStyle, ts: &TextSystem) -> f32 {
        let default_size = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
        let digit = (default_size * 0.55).round().max(1.0);
        let size = style.font_size as f32 / 100.0;
        let excel_digit = digit * size / default_size;
        let drawn_digit = super::sheet_view::text_width_pt("0", style, &self.resources, ts).max(0.1);
        let width = super::sheet_view::text_width_pt(text, style, &self.resources, ts) * excel_digit / drawn_digit;
        let italic = if style.italic { 2.0 * size / 12.0 } else { 0.0 };
        width + italic + style.indent as f32 * 3.0 * excel_digit
    }

    /// The header's menu (the right button), for the columns or rows selected.
    pub(crate) fn header_menu(&self, axis: Axis) -> Vec<MenuEntry> {
        let (unit, before, after) = match axis {
            Axis::Columns => (crate::shell::i18n::text("_xlsx.column"), crate::shell::i18n::text("_shell.insert-column-left"), crate::shell::i18n::text("_shell.insert-column-right")),
            Axis::Rows => (crate::shell::i18n::text("_xlsx.row"), crate::shell::i18n::text("_shell.insert-row-above"), crate::shell::i18n::text("_shell.insert-row-below")),
        };
        let (cut, copy, paste): (&'static str, &'static str, &'static str) = ("xlsx.header-cut", "xlsx.header-copy", "xlsx.header-paste");
        let id = |c: &'static str, r: &'static str| if axis == Axis::Columns { c } else { r };
        vec![
            MenuItem::new(cut, "_xlsx.cut").icon("content_cut").shortcut("⌘X").on(|this, _, cx| this.copy_cells(true, cx)).into(),
            MenuItem::new(copy, "_shell.copy").icon("content_copy").shortcut("⌘C").on(|this, _, cx| this.copy_cells(false, cx)).into(),
            MenuItem::tr("xlsx.copy-as-picture").icon("image").on(|this, _, cx| this.copy_as_picture(cx)).into(),
            MenuItem::new(paste, "_xlsx.paste").icon("content_paste").shortcut("⌘V").on(|this, _, cx| this.paste_cells(false, cx)).into(),
            MenuEntry::Separator,
            MenuItem::new(id("xlsx.header-insert-left", "xlsx.header-insert-above"), before).icon("add").on(move |this, _, cx| this.insert_lines(axis, false, cx)).into(),
            MenuItem::new(id("xlsx.header-insert-right", "xlsx.header-insert-below"), after).icon("add").on(move |this, _, cx| this.insert_lines(axis, true, cx)).into(),
            MenuItem::new(id("xlsx.header-delete-columns", "xlsx.header-delete-rows"), crate::shell::i18n::format("_xlsx.delete-unit", &[("unit", &unit)])).icon("delete").on(move |this, _, cx| this.delete_lines(axis, cx)).into(),
            MenuItem::tr("xlsx.header-clear").on(|this, _, cx| this.clear_selected_cells(cx)).into(),
            MenuItem::tr("xlsx.header-clear-formats").icon("format_clear").shortcut("⌘\\").on(|this, _, cx| this.clear_cell_formats(cx)).into(),
            MenuEntry::Separator,
            MenuItem::new(id("xlsx.header-fit-columns", "xlsx.header-fit-rows"), if axis == Axis::Columns { crate::shell::i18n::text("_xlsx.autofit-column-width") } else { crate::shell::i18n::text("_xlsx.autofit-row-height") })
                .on(move |this, window, cx| this.fit_selected(axis, window, cx))
                .into(),
            MenuItem::new(id("xlsx.header-hide-columns", "xlsx.header-hide-rows"), crate::shell::i18n::format("_xlsx.hide-unit", &[("unit", &unit)])).icon("visibility_off").shortcut(if axis == Axis::Columns { "⌃0" } else { "⌃9" }).on(move |this, _, cx| this.hide_lines(axis, true, cx)).into(),
            MenuItem::new(id("xlsx.header-unhide-columns", "xlsx.header-unhide-rows"), crate::shell::i18n::format("_xlsx.unhide-unit", &[("unit", &unit)])).icon("visibility").on(move |this, _, cx| this.hide_lines(axis, false, cx)).into(),
        ]
    }

    /// The commands fitting rows `first`–`last` to their contents, as
    /// Excel's 行の高さの自動調整: the sheet's default height for a row of
    /// one line in the default size; a wrapped cell's lines (its text broken
    /// at the column's width) or a larger font make it taller.
    pub(crate) fn fit_rows_commands(&self, first: u32, last: u32, ts: &TextSystem) -> Vec<Command> {
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let default_size = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
        let base = if sheet.default_row > 0 { sheet.default_row as f32 / EMU_PER_POINT as f32 } else { 18.0 };
        let slide = self.current_slide;
        let mut out = Vec::new();
        for row in first..=last {
            let mut height = base;
            for cell in sheet.cells.iter().filter(|c| c.row == row && !c.text.is_empty()) {
                let style = &sheet.styles[cell.style as usize];
                let size = style.font_size as f32 / 100.0;
                let line = base * size / default_size;
                let lines = if style.wrap {
                    let room = sheet.columns.get(cell.column as usize).copied().unwrap_or(0) as f32 / EMU_PER_POINT as f32 - 5.0;
                    // Break where the text is wider than the room (spaces first for Latin text).
                    let mut lines = 0;
                    for paragraph in cell.text.split('\n') {
                        let mut current = String::new();
                        lines += 1;
                        for ch in paragraph.chars() {
                            current.push(ch);
                            if self.excel_text_width(&current, style, ts) > room && current.chars().count() > 1 {
                                lines += 1;
                                current = ch.to_string();
                            }
                        }
                    }
                    lines.max(1)
                } else {
                    1
                };
                // Turned text needs the height of its turned line (not measured
                // against Excel: its width by the angle's sine, its line by the
                // cosine); vertical text a character's height each.
                let turned = match style.rotation {
                    0 => 0.0,
                    255 => line * cell.text.chars().count() as f32,
                    r => {
                        let a = (if r <= 90 { r as f32 } else { r as f32 - 90.0 }).to_radians();
                        self.excel_text_width(&cell.text, style, ts) * a.sin() + line * a.cos() + 4.0
                    }
                };
                height = height.max(line * lines as f32).max(turned);
            }
            let size = ((height * 4.0).round() / 4.0 * EMU_PER_POINT as f32) as Emu;
            if sheet.rows.get(row as usize).is_some_and(|&h| h != size && h > 0) {
                out.push(Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Rows, first: row, last: row, size }));
            }
        }
        out
    }

    /// Fit the selected columns, or give the selected rows the sheet's default height.
    fn fit_selected(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected_range() else { return };
        match axis {
            Axis::Columns => match self.selected_lines(Axis::Columns) {
                Some((a, _)) => self.fit_columns(a, window, cx),
                None => {
                    for c in r.first_column..=r.last_column {
                        self.fit_columns(c, window, cx);
                    }
                }
            },
            Axis::Rows => {
                let ts = window.text_system().clone();
                let commands = self.fit_rows_commands(r.first_row, r.last_row, &ts);
                if !commands.is_empty()
                    && let Err(err) = self.document.execute(commands)
                {
                    log::warn!("sizing rejected: {err}");
                }
                cx.notify();
            }
        }
    }
}

/// The width a column fitted to text `text_pt` wide gets: Excel shows a
/// number from text + 4.2 pt (found by trying widths in Excel; `###`
/// below) and leaves a little more when it fits a column. In quarter points.
pub(crate) fn fitted_width(text_pt: f32) -> Emu {
    let pt = ((text_pt + 5.0) * 4.0).ceil() / 4.0;
    (pt * EMU_PER_POINT as f32) as Emu
}

#[cfg(test)]
mod tests {
    use super::{HeaderHit, hit};

    #[test]
    fn hits() {
        let spans = [(0, 0.0, 50.0), (1, 50.0, 50.0), (2, 50.0, 120.0)];
        assert_eq!(hit(&spans, 20.0), Some(HeaderHit::Header(0)));
        // Where 0 ends (and the hidden 1): left of the line sizes 0, right
        // of it drags out 1.
        assert_eq!(hit(&spans, 49.0), Some(HeaderHit::Line { index: 0, start: 0.0 }));
        assert_eq!(hit(&spans, 51.0), Some(HeaderHit::Line { index: 1, start: 50.0 }));
        assert_eq!(hit(&spans, 118.0), Some(HeaderHit::Line { index: 2, start: 50.0 }));
        assert_eq!(hit(&spans, 80.0), Some(HeaderHit::Header(2)));
        assert_eq!(hit(&spans, 200.0), None);
    }
}
