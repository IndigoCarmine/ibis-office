// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! エラー チェック: a cell marked with the green triangle shows a button
//! at its left when it is the selected cell; the button opens Excel's menu
//! (the message, 数値に変換, エラーを無視, 数式バーで編集).
use crate::shell::chrome::{MenuEntry, MenuItem};
use crate::shell::editor::SlideEditor;
use gpui_kit::*;
use ibis_calc::input::{Typed, parse_input};
use ibis_doc_model::error_check::{ErrorKind, errors_at, message};
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, SheetCommand, StyleChange};

impl SlideEditor {
    /// The first error of the selected cell, if it has one not ignored.
    pub(crate) fn current_error(&self) -> Option<ErrorKind> {
        let (column, row) = self.current_cell()?;
        let (_, sheet) = self.current_sheet()?;
        errors_at(sheet, row, column).first().copied()
    }

    /// The menu of the button: the message, and what can be done.
    pub(crate) fn error_menu(&self) -> Vec<MenuEntry> {
        let Some(kind) = self.current_error() else { return Vec::new() };
        let mut out: Vec<MenuEntry> = vec![MenuItem::new("xlsx.error-message", message(kind)).disabled(true).into(), MenuEntry::Separator];
        if kind == ErrorKind::NumberAsText {
            out.push(MenuItem::new("xlsx.error-convert", "_xlsx.convert-to-number").on(|this, _, cx| this.convert_text_numbers(cx)).into());
        }
        out.push(MenuItem::new("xlsx.error-ignore", "_xlsx.ignore-error").on(|this, _, cx| this.ignore_error(cx)).into());
        out.push(MenuItem::new("xlsx.error-edit", "_xlsx.edit-in-formula-bar").on(|this, _, cx| this.begin_cell_edit(super::cell_edit::EditMode::Edit, true, cx)).into());
        out
    }

    pub(crate) fn open_error_menu(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let entries = self.error_menu();
        if !entries.is_empty() {
            self.open_popup_menu(entries, position, window, cx);
        }
    }

    /// エラーを無視: the selected cell's kind of error ignored in the selection.
    pub(crate) fn ignore_error(&mut self, cx: &mut Context<Self>) {
        let Some(kind) = self.current_error() else { return };
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::IgnoreErrors { slide, range, kind })], cx);
    }

    /// 数値に変換: the text constants of the selection that read as numbers
    /// become those numbers (with the format typing them gives).
    pub(crate) fn convert_text_numbers(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(range) = self.selected_range() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let mut edits = Vec::new();
        let mut formats: Vec<(CellRange, String)> = Vec::new();
        for row in range.first_row..=range.last_row {
            for column in range.first_column..=range.last_column {
                if !errors_at(sheet, row, column).contains(&ErrorKind::NumberAsText) {
                    continue;
                }
                let Some(CellValue::Text(t)) = sheet.cell(row, column).map(|c| c.value.clone()) else { continue };
                if let Typed::Value(CellValue::Number(n), format) = parse_input(t.trim(), date1904) {
                    if let Some(code) = format {
                        formats.push((CellRange { first_row: row, last_row: row, first_column: column, last_column: column }, code.to_string()));
                    }
                    edits.push(CellEdit { row, column, input: CellInput::Value(CellValue::Number(n)) });
                }
            }
        }
        if edits.is_empty() {
            return;
        }
        let slide = self.current_slide;
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits })];
        for (range, code) in formats {
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::NumberFormat(code) }));
        }
        self.run_sheet(commands, cx);
    }
}
