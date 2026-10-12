// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The right button on a cell: Excel for Mac's menu (切り取り, コピー,
//! 貼り付け, 形式を選択して貼り付け, 挿入, 削除, 数式と値のクリア, フィルター,
//! 並べ替え, メモ, セルの書式設定, ハイパーリンク, 名前の定義). The cell
//! pressed is selected unless it is within the selection (the menu then
//! acts on the selection). The pop-up is the header menu's (`MenuItem`,
//! `fill_menu`).
use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;
use gpui_kit::*;
use ibis_doc_model::{ColumnFilter, sheet_command::Axis};

impl SlideEditor {
    /// The right button on the sheet.
    pub(crate) fn on_sheet_right_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.commit_edit(window, cx);
        self.focus_handle.focus(window, cx);
        self.selection = None;
        let Some((column, row)) = self.cell_under(event.position) else { return };
        self.commit_cell_edit(cx);
        let within = self.selected_ranges().iter().any(|r| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column));
        if !within {
            self.set_cell(column, row);
        }
        let entries = self.cell_menu();
        self.open_popup_menu(entries, event.position, window, cx);
    }

    /// The menu's entries, in Excel for Mac's order.
    pub(crate) fn cell_menu(&self) -> Vec<MenuEntry> {
        let menus = self.menus();
        let has_note = self.current_cell().zip(self.current_sheet()).is_some_and(|((c, r), (_, sheet))| sheet.comment_at(r, c).is_some());
        let mut out: Vec<MenuEntry> = vec![
            MenuItem::new("xlsx.cell-cut", "_xlsx.cut").icon("content_cut").shortcut("⌘X").on(|this, _, cx| this.copy_cells(true, cx)).into(),
            MenuItem::new("xlsx.cell-copy", "_shell.copy").icon("content_copy").shortcut("⌘C").on(|this, _, cx| this.copy_cells(false, cx)).into(),
            MenuItem::new("xlsx.cell-paste", "_xlsx.paste").icon("content_paste").shortcut("⌘V").on(|this, _, cx| this.paste_cells(false, cx)).into(),
        ];
        out.extend(menus.find_entry("xlsx.paste-special"));
        out.push(MenuEntry::Separator);
        out.push(
            Submenu::new(
                "xlsx.cell-insert",
                "add",
                "_shell.insert",
                vec![
                    MenuItem::new("xlsx.cell-insert-right", "_shell.insert-cells-shift-cells-right").on(|this, _, cx| this.shift_cells(false, true, cx)).into(),
                    MenuItem::new("xlsx.cell-insert-down", "docx.context-menu.insert-cells-shift-down").on(|this, _, cx| this.shift_cells(true, true, cx)).into(),
                    MenuItem::new("xlsx.cell-insert-row", "_xlsx.insert-row").on(|this, _, cx| this.insert_lines(Axis::Rows, false, cx)).into(),
                    MenuItem::new("xlsx.cell-insert-column", "_xlsx.insert-column").on(|this, _, cx| this.insert_lines(Axis::Columns, false, cx)).into(),
                ],
            )
            .into(),
        );
        out.push(
            Submenu::new(
                "xlsx.cell-delete",
                "delete",
                "_shell.delete",
                vec![
                    MenuItem::new("xlsx.cell-delete-left", "_shell.delete-cells-shift-cells-left").on(|this, _, cx| this.shift_cells(false, false, cx)).into(),
                    MenuItem::new("xlsx.cell-delete-up", "docx.context-menu.delete-cells-shift-up").on(|this, _, cx| this.shift_cells(true, false, cx)).into(),
                    MenuItem::new("xlsx.cell-delete-row", "docx.context-menu.delete-row").on(|this, _, cx| this.delete_lines(Axis::Rows, cx)).into(),
                    MenuItem::new("xlsx.cell-delete-column", "docx.context-menu.delete-column").on(|this, _, cx| this.delete_lines(Axis::Columns, cx)).into(),
                ],
            )
            .into(),
        );
        out.push(MenuItem::new("xlsx.cell-clear", "_xlsx.clear-contents").on(|this, _, cx| this.clear_selected_cells(cx)).into());
        out.push(MenuEntry::Separator);
        out.push(
            Submenu::new(
                "xlsx.cell-filter",
                "filter_alt",
                "_xlsx.filters",
                vec![
                    MenuItem::new("xlsx.cell-filter-toggle", "_xlsx.create-or-remove-filter").on(|this, _, cx| this.toggle_auto_filter(cx)).into(),
                    MenuItem::new("xlsx.cell-filter-value", "_xlsx.filter-by-selected-cell-s-value").on(|this, _, cx| this.filter_by_selected(cx, |text, _| ColumnFilter::Values(vec![text.to_string()]))).into(),
                    MenuItem::new("xlsx.cell-filter-fill", "_xlsx.filter-by-selected-cell-s-color").on(|this, _, cx| this.filter_by_selected(cx, |_, style| ColumnFilter::Color { cell: true, color: style.fill })).into(),
                    MenuItem::new("xlsx.cell-filter-font", "_xlsx.filter-by-selected-cell-s-font-color").on(|this, _, cx| this.filter_by_selected(cx, |_, style| ColumnFilter::Color { cell: false, color: Some(style.color) })).into(),
                ],
            )
            .into(),
        );
        out.push(
            Submenu::new(
                "xlsx.cell-sort",
                "sort",
                "_xlsx.sort-2",
                vec![
                    MenuItem::new("xlsx.cell-sort-ascending", "_xlsx.ascending").on(|this, _, cx| this.sort_by_selected_column(true, cx)).into(),
                    MenuItem::new("xlsx.cell-sort-descending", "_xlsx.descending").on(|this, _, cx| this.sort_by_selected_column(false, cx)).into(),
                ],
            )
            .into(),
        );
        out.push(MenuEntry::Separator);
        // ドロップダウン リストから選択…: a list rule on the cell.
        if self.current_rule().is_some_and(|r| r.kind == "list") {
            out.push(
                MenuItem::new("xlsx.cell-pick-list", "_xlsx.pick-from-drop-down-list")
                    .icon("arrow_drop_down_circle")
                    .on(|this, window, cx| {
                        if let Some((c, r)) = this.current_cell()
                            && let Some(at) = this.cell_center(c, r)
                        {
                            this.open_validation_list(at, window, cx);
                        }
                    })
                    .into(),
            );
        }
        out.push(MenuItem::tr(if has_note { "xlsx.note-edit" } else { "xlsx.note-new" }).icon("note_add").on(|this, window, cx| this.open_note_panel(window, cx)).into());
        if has_note {
            out.push(MenuItem::tr("xlsx.note-delete").icon("delete").on(|this, _, cx| this.delete_notes(cx)).into());
        }
        out.push(MenuItem::new("xlsx.cell-format", "_xlsx.format-cells").icon("tune").on(|this, window, cx| this.open_custom_format(window, cx)).into());
        out.push(MenuItem::new("xlsx.cell-link", "_xlsx.hyperlink").icon("link").shortcut("⌘K").on(|this, window, cx| this.open_link_panel(window, cx)).into());
        out.push(MenuItem::new("xlsx.cell-define-name", "_xlsx.define-name").on(|this, window, cx| this.open_names(window, cx)).into());
        out
    }

    /// The right button on the tab of sheet `id`: the sheet is shown, and
    /// its menu opens (Excel for Mac's: 挿入, 削除, 名前の変更, 移動または
    /// コピー, シートの保護, タブの色, 非表示, 再表示).
    pub(crate) fn on_tab_right_down(&mut self, id: ibis_doc_model::SlideId, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.current_slide != id {
            self.select_slide(id, window, cx);
        }
        let entries = self.tab_menu();
        self.open_popup_menu(entries, event.position, window, cx);
    }

    /// The sheet tab's menu.
    pub(crate) fn tab_menu(&self) -> Vec<MenuEntry> {
        let menus = self.menus();
        let mut out: Vec<MenuEntry> = Vec::new();
        out.extend(menus.find_entry("xlsx.insert-sheet"));
        out.extend(menus.find_entry("xlsx.delete-sheet"));
        out.extend(menus.find_entry("xlsx.rename-sheet"));
        out.extend(menus.find_entry("xlsx.duplicate-sheet"));
        out.extend(menus.find_entry("xlsx.move-sheet-left"));
        out.extend(menus.find_entry("xlsx.move-sheet-right"));
        out.push(MenuEntry::Separator);
        out.push(MenuItem::new("xlsx.tab-protect", "_xlsx.protect-sheet").icon("lock").on(|this, window, cx| this.open_protect_panel(window, cx)).into());
        out.extend(menus.find_entry("xlsx.tab-colors"));
        out.push(MenuEntry::Separator);
        out.extend(menus.find_entry("xlsx.hide-sheet"));
        out.extend(menus.find_entry("xlsx.unhide-sheets"));
        out
    }

    /// The right button on a shape (a chart, picture or drawing) over the
    /// sheet: it is selected (unless it is among those selected) and its menu
    /// opens: 切り取り, コピー, 貼り付け, 削除, 順序, グループ化, 代替テキスト,
    /// サイズとプロパティ.
    pub(crate) fn on_sheet_shape_right_down(&mut self, shape: ibis_doc_model::ShapeId, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.commit_edit(window, cx);
        self.focus_handle.focus(window, cx);
        if !self.selected_ids().contains(&shape) {
            self.set_selection(vec![shape]);
        }
        let entries = self.shape_menu();
        self.open_popup_menu(entries, event.position, window, cx);
        cx.notify();
    }

    /// The menu of a shape over the sheet.
    pub(crate) fn shape_menu(&self) -> Vec<MenuEntry> {
        let menus = self.menus();
        let mut out: Vec<MenuEntry> = Vec::new();
        for id in ["edit.cut", "edit.copy", "edit.paste"] {
            out.extend(menus.find_entry(id));
        }
        out.extend(menus.find_entry("edit.delete"));
        out.push(MenuEntry::Separator);
        for id in ["xlsx.order", "xlsx.group-shapes", "xlsx.alt-text", "xlsx.frame-settings"] {
            out.extend(menus.find_entry(id));
        }
        out
    }

    /// フィルター > 選択したセルの値（色）でフィルター: the table (or the sheet's
    /// filter, made if there is none) shows the rows whose cell in this
    /// column shows what the selected cell shows.
    pub(crate) fn filter_by_selected(&mut self, cx: &mut Context<Self>, make: impl Fn(&str, &ibis_doc_model::CellStyle) -> ColumnFilter) {
        let Some((column, row)) = self.current_cell() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let text = sheet.cell(row, column).map(|c| c.text.clone()).unwrap_or_default();
        let style = sheet.cell(row, column).and_then(|c| sheet.styles.get(c.style as usize)).cloned().unwrap_or_default();
        let filter = make(&text, &style);
        let in_table = sheet.tables.iter().position(|t| t.filter.is_some() && (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column));
        let target = match (in_table, &sheet.auto_filter) {
            (Some(i), _) => Some(super::filter::FilterTarget::Table(i)),
            (None, Some(f)) if (f.range.first_column..=f.range.last_column).contains(&column) => Some(super::filter::FilterTarget::Sheet),
            _ => None,
        };
        let target = match target {
            Some(t) => t,
            None => {
                // No filter over this cell yet: made on the table around it, as フィルター does.
                self.toggle_auto_filter(cx);
                let Some((_, sheet)) = self.current_sheet() else { return };
                match sheet.tables.iter().position(|t| t.filter.is_some() && (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column)) {
                    Some(i) => super::filter::FilterTarget::Table(i),
                    None => super::filter::FilterTarget::Sheet,
                }
            }
        };
        self.filter_column_in(target, column, Some(filter), cx);
    }
}
