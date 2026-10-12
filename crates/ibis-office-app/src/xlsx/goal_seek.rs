// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > What-If 分析 > ゴールシーク…: the cell to change (a constant)
//! that makes a formula's result a value (`ibis_doc_model::goal_seek`). The
//! answer is shown first; 適用 puts it in the cell as one step to undo.
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::goal_seek::{GoalSeekError, goal_seek};
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};

use super::sheet_ui::{Reference, Target, parse_reference};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct GoalSeekPanel {
    /// 数式入力セル.
    pub(crate) formula: Entity<InputState>,
    /// 目標値.
    pub(crate) target: Entity<InputState>,
    /// 変化させるセル.
    pub(crate) changing: Entity<InputState>,
    /// The answer: the cell (row, column) and its value, until applied.
    pub(crate) found: Option<((u32, u32), f64)>,
    pub(crate) message: String,
}

impl SlideEditor {
    pub(crate) fn open_goal_seek(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let here = self.current_cell().map(|(c, r)| ibis_doc_model::cell_name(c, r)).unwrap_or_default();
        let panel = GoalSeekPanel { formula: input(here, window, cx), target: input(String::new(), window, cx), changing: input(String::new(), window, cx), found: None, message: String::new() };
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
        s.panels.goal_seek = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn goal_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.goal_seek.as_mut() {
            p.message = message.to_string();
            p.found = None;
        }
        cx.notify();
    }

    /// 実行: look for the value.
    pub(crate) fn run_goal_seek(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.goal_seek.as_ref()) else { return };
        let (formula_text, target_text, changing_text) = (p.formula.read(cx).value().to_string(), p.target.read(cx).value().to_string(), p.changing.read(cx).value().to_string());
        let cell = |t: &str| match parse_reference(t) {
            Some(Reference { sheet: None, target: Target::Cells(r) }) if r.first_row == r.last_row && r.first_column == r.last_column => Some((r.first_row, r.first_column)),
            _ => None,
        };
        let Some(formula) = cell(&formula_text) else { return self.goal_message(text("_xlsx.set-cell-must-be-a-single-cell-on-this-sheet"), cx) };
        let Some(changing) = cell(&changing_text) else { return self.goal_message(text("_xlsx.by-changing-cell-must-be-a-single-cell-on-this-s"), cx) };
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let target = match ibis_calc::input::parse_input(target_text.trim(), date1904) {
            ibis_calc::input::Typed::Value(CellValue::Number(n), _) => n,
            _ => return self.goal_message(text("_xlsx.enter-a-number-for-to-value"), cx),
        };
        match goal_seek(self.presentation(), self.current_slide, formula, target, changing) {
            Ok(x) => {
                let shown = ibis_calc::eval::number_to_text(x);
                let message = crate::shell::i18n::format("goalseek.found", &[("formula", &ibis_doc_model::cell_name(formula.1, formula.0)), ("target", &ibis_calc::eval::number_to_text(target)), ("cell", &ibis_doc_model::cell_name(changing.1, changing.0)), ("value", &shown)]);
                if let Some(p) = self.wb_mut().panels.goal_seek.as_mut() {
                    p.found = Some((changing, x));
                    p.message = message;
                }
                cx.notify();
            }
            Err(GoalSeekError::ChangingCell) => self.goal_message(text("_xlsx.by-changing-cell-must-be-a-number-cell-without-a"), cx),
            Err(GoalSeekError::NotANumber) => self.goal_message(text("_xlsx.set-cell-must-be-a-cell-with-a-formula-that-retu"), cx),
            Err(GoalSeekError::NotFound) => self.goal_message(text("_xlsx.no-solution-was-found"), cx),
        }
    }

    /// 適用: the answer put in the cell.
    pub(crate) fn apply_goal_seek(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(((row, column), x)) = self.sheet_state().and_then(|s| s.panels.goal_seek.as_ref()).and_then(|p| p.found) else { return };
        let slide = self.current_slide;
        let edit = CellEdit { row, column, input: CellInput::Value(CellValue::Number(x)) };
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![edit] })]) {
            return self.goal_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
        }
        self.wb_mut().panels.goal_seek = None;
        cx.notify();
    }

    pub(crate) fn goal_seek_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.goal_seek.as_ref()?;
        let found = p.found.is_some();
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(div().text_size(px(13.)).child(text("_xlsx.set-cell")))
            .child(Input::new(&p.formula).small())
            .child(div().text_size(px(13.)).child(text("_xlsx.to-value")))
            .child(Input::new(&p.target).small())
            .child(div().text_size(px(13.)).child(text("_xlsx.by-changing-cell")))
            .child(Input::new(&p.changing).small())
            .child(div().text_size(px(12.)).child(SharedString::from(p.message.clone())))
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("goal-run").primary().small().label(text("_xlsx.run")).on_click(cx.listener(|this, _, _, cx| this.run_goal_seek(cx))))
                    .when(found, |b| b.child(Button::new("goal-apply").small().primary().label(text("_shell.apply")).on_click(cx.listener(|this, _, _, cx| this.apply_goal_seek(cx)))))
                    .child(Button::new("goal-close").small().ghost().label(text("_shell.close")).on_click(cx.listener(|this, _, _, cx| {
                        this.wb_mut().panels.goal_seek = None;
                        cx.notify();
                    }))),
            );
        Some(SidebarPanel { title: text("_xlsx.goal-seek").to_string().into(), body: body.into_any_element() })
    }
}
