// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 検索と置換 in the sidebar: the next cell holding the text (in what
//! cells show, or in formulas too), and replacing in one cell or all.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct FindPanel {
    pub(crate) query: Entity<InputState>,
    pub(crate) replacement: Entity<InputState>,
    /// Look in formulas too (Excel's 検索対象: 数式); otherwise only text
    /// values are replaced in, and formulas keep theirs.
    pub(crate) formulas: bool,
}

/// Replace `query` in `text` without regard to case (as Excel's default).
pub(crate) fn replace_text(text: &str, query: &str, with: &str) -> Option<String> {
    if query.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    let q = query.to_lowercase();
    if !lower.contains(&q) || lower.len() != text.len() {
        // Case folding changed the length: compare as is.
        return text.contains(query).then(|| text.replace(query, with));
    }
    let mut out = String::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find(&q) {
        out.push_str(&text[at..at + i]);
        out.push_str(with);
        at += i + q.len();
    }
    out.push_str(&text[at..]);
    Some(out)
}

impl SlideEditor {
    pub(crate) fn open_sheet_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.find-what")));
        let replacement = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.replace-with")));
        query.update(cx, |q, cx| q.focus(window, cx));
        if let Some(s) = self.sheet_state_mut() {
            s.names = None;
            s.validation = None;
            s.find = Some(FindPanel { query, replacement, formulas: false });
        }
        if let Some(s) = self.sheet_state_mut() {
            s.page_setup = false;
        }
        if let Some(s) = self.sheet_state_mut() {
            s.custom_format = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// What a cell is searched in: its formula (`=…`) when looking in
    /// formulas, else what it shows.
    fn searched(&self, row: u32, column: u32, formulas: bool) -> Option<String> {
        let (_, sheet) = self.current_sheet()?;
        let c = sheet.cell(row, column)?;
        match (&c.formula, formulas) {
            (Some(f), true) => Some(format!("={f}")),
            _ => (!c.text.is_empty()).then(|| c.text.clone()),
        }
    }

    /// Select the next cell (row by row, after the selected one, wrapping
    /// round) holding `query`. Whether one was found.
    pub(crate) fn find_next_cell(&mut self, query: &str, formulas: bool, cx: &mut Context<Self>) -> bool {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return false };
        let q = query.to_lowercase();
        if q.is_empty() {
            return false;
        }
        let at = self.current_cell().map_or((0, 0), |(c, r)| (r, c));
        let mut hits: Vec<(u32, u32)> = sheet.cells.iter().map(|c| (c.row, c.column)).filter(|&(r, c)| self.searched(r, c, formulas).is_some_and(|t| t.to_lowercase().contains(&q))).collect();
        hits.sort();
        let next = hits.iter().find(|&&p| p > at).or(hits.first()).copied();
        match next {
            Some((row, column)) => {
                let slide = self.current_slide;
                self.selection = None;
                let s = self.wb_mut();
                s.cells.insert(slide, (column, row));
                s.cell_ends.remove(&slide);
                self.status = None;
                cx.notify();
                true
            }
            None => {
                self.status = Some(crate::shell::i18n::format("_xlsx.cannot-find-query", &[("query", &query)]).into());
                cx.notify();
                false
            }
        }
    }

    /// The edit replacing `query` in the cell (`None` if nothing to replace).
    fn replacement_for(&self, row: u32, column: u32, query: &str, with: &str, formulas: bool) -> Option<CellEdit> {
        let (_, sheet) = self.current_sheet()?;
        let c = sheet.cell(row, column)?;
        let input = match (&c.formula, &c.value) {
            (Some(f), _) if formulas => CellInput::Formula(replace_text(f, query, with)?),
            (Some(_), _) => return None,
            (None, CellValue::Text(t)) => CellInput::Value(CellValue::Text(replace_text(t, query, with)?)),
            (None, v @ (CellValue::Number(_) | CellValue::Bool(_))) => {
                // A number or TRUE replaced in as text, read again as typed.
                let text = replace_text(&c.text, query, with)?;
                let _ = v;
                match ibis_calc::input::parse_input(&text, false) {
                    ibis_calc::input::Typed::Value(v, _) => CellInput::Value(v),
                    ibis_calc::input::Typed::Formula(f) => CellInput::Formula(f),
                }
            }
            _ => return None,
        };
        Some(CellEdit { row, column, input })
    }

    /// Replace in the selected cell, then go to the next.
    pub(crate) fn replace_one(&mut self, query: &str, with: &str, formulas: bool, cx: &mut Context<Self>) {
        if let Some((column, row)) = self.current_cell()
            && let Some(edit) = self.replacement_for(row, column, query, with, formulas)
        {
            let slide = self.current_slide;
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![edit] })]) {
                log::warn!("replace rejected: {err}");
            }
        }
        self.find_next_cell(query, formulas, cx);
    }

    /// Replace in every cell of the sheet (one step); how many.
    pub(crate) fn replace_all_cells(&mut self, query: &str, with: &str, formulas: bool, cx: &mut Context<Self>) -> usize {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return 0 };
        let positions: Vec<(u32, u32)> = sheet.cells.iter().map(|c| (c.row, c.column)).collect();
        let edits: Vec<CellEdit> = positions.into_iter().filter_map(|(r, c)| self.replacement_for(r, c, query, with, formulas)).collect();
        let n = edits.len();
        if n > 0 {
            let slide = self.current_slide;
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })]) {
                log::warn!("replace all rejected: {err}");
            }
        }
        self.status = Some(crate::shell::i18n::format("_xlsx.replaced-n-occurrence-s", &[("n", &n)]).into());
        cx.notify();
        n
    }

    pub(crate) fn sheet_find_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.find.as_ref()?;
        let (query, replacement, formulas) = (panel.query.clone(), panel.replacement.clone(), panel.formulas);
        let values = |this: &SlideEditor, cx: &App| -> (String, String, bool) {
            let p = this.sheet_state().and_then(|s| s.find.as_ref());
            (p.map(|p| p.query.read(cx).value().to_string()).unwrap_or_default(), p.map(|p| p.replacement.read(cx).value().to_string()).unwrap_or_default(), p.is_some_and(|p| p.formulas))
        };
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(Input::new(&query).small())
            .child(Input::new(&replacement).small())
            .child(Checkbox::new("find-formulas").label(crate::shell::i18n::text("_xlsx.include-formulas")).checked(formulas).on_click(cx.listener(|this, checked: &bool, _, cx| {
                if let Some(p) = this.sheet_state_mut().and_then(|s| s.find.as_mut()) {
                    p.formulas = *checked;
                }
                cx.notify();
            })))
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("xfind-next").small().ghost().label(crate::shell::i18n::text("_xlsx.find-next")).on_click(cx.listener(move |this, _, _, cx| {
                        let (q, _, f) = values(this, cx);
                        this.find_next_cell(&q, f, cx);
                    })))
                    .child(Button::new("xreplace-one").small().label(crate::shell::i18n::text("_shell.replace")).on_click(cx.listener(move |this, _, _, cx| {
                        let (q, r, f) = values(this, cx);
                        this.replace_one(&q, &r, f, cx);
                    })))
                    .child(Button::new("xreplace-all").small().label(crate::shell::i18n::text("_shell.replace-all")).on_click(cx.listener(move |this, _, _, cx| {
                        let (q, r, f) = values(this, cx);
                        this.replace_all_cells(&q, &r, f, cx);
                    }))),
            );
        Some(SidebarPanel { title: "_shell.find-and-replace".into(), body: body.into_any_element() })
    }
}

#[cfg(test)]
mod tests {
    use super::replace_text;

    #[test]
    fn replacing() {
        assert_eq!(replace_text("北町店の店舗", "店舗", "支店").as_deref(), Some("北町店の支店"));
        assert_eq!(replace_text("Apple apple", "APPLE", "pear").as_deref(), Some("pear pear"));
        assert_eq!(replace_text("abc", "x", "y"), None);
    }
}
