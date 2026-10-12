// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 名前付き範囲と名前付き数式: a name typed in the name box for the selected range
//! defines it (as in Excel), the name box and ジャンプ go to a name, and
//! the sidebar's 名前の管理 lists the names, changes what one stands for
//! (to an arbitrary formula, constant, or the selected range), edits names,
//! and deletes names.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The name manager, open in the sidebar.
pub(crate) struct NamesPanel {
    /// The field for a new or edited name.
    pub(crate) new_name: Entity<InputState>,
    /// The field for the formula, constant, or reference.
    pub(crate) new_formula: Entity<InputState>,
    /// Index of the name currently being edited.
    pub(crate) editing: Option<usize>,
}

/// Whether `text` can be a defined name (Excel's rules, roughly): a letter,
/// `_` or `\\` first, then no spaces or operators, and not a cell
/// reference (`A1`, `R1C1`) or TRUE/FALSE.
pub(crate) fn valid_name(text: &str) -> bool {
    let Some(first) = text.chars().next() else { return false };
    if !(first.is_alphabetic() || first == '_' || first == '\\') || text.chars().count() > 255 {
        return false;
    }
    if !text.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?')) {
        return false;
    }
    let upper = text.to_ascii_uppercase();
    if upper == "TRUE" || upper == "FALSE" || upper == "R" || upper == "C" {
        return false;
    }
    // A cell reference: letters then digits within the sheet.
    let letters = upper.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let digits = &upper[letters..];
    if letters > 0 && letters <= 3 && !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // R1C1 forms.
    let r1c1 = upper.starts_with('R') && upper[1..].chars().all(|c| c.is_ascii_digit() || c == 'C');
    !r1c1
}

impl SlideEditor {
    /// The selected range as a name's formula: `Sheet!$A$1:$B$3`.
    fn selection_formula(&self) -> Option<String> {
        let range = self.selected_range()?;
        let name = self.slide().name.clone()?;
        let a = |c: u32, r: u32| format!("${}${}", ibis_doc_model::cell_name(c, 0).trim_end_matches('1'), r + 1);
        let area = if (range.first_row, range.first_column) == (range.last_row, range.last_column) {
            a(range.first_column, range.first_row)
        } else {
            format!("{}:{}", a(range.first_column, range.first_row), a(range.last_column, range.last_row))
        };
        Some(format!("{}!{area}", ibis_calc::formula::quote_sheet(&name)))
    }

    /// Define `name` with a formula or constant (without leading `=`), or fallback to the selected range.
    pub(crate) fn set_named_formula(&mut self, name: &str, formula: &str, cx: &mut Context<Self>) -> bool {
        if !valid_name(name) {
            self.status = Some(crate::shell::i18n::format("_xlsx.this-name-cannot-be-used-name", &[("name", &name)]).into());
            cx.notify();
            return false;
        }
        let clean = formula.trim().trim_start_matches('=').trim().to_string();
        let current_sheet = self.slide().name.clone().unwrap_or_else(|| "Sheet1".to_string());
        let target_formula = if clean.is_empty() {
            self.selection_formula().unwrap_or_default()
        } else {
            let qualified = ibis_calc::formula::rewrite_references(&clean, |area| {
                if area.sheet.is_none() {
                    let mut a = area.clone();
                    a.sheet = Some(ibis_calc::formula::SheetRef { first: current_sheet.clone(), last: None, book: None });
                    a.start.row_fixed = true;
                    a.start.column_fixed = true;
                    a.end.row_fixed = true;
                    a.end.column_fixed = true;
                    Some(a)
                } else {
                    Some(area.clone())
                }
            });
            ibis_calc::formula::strip_formula_spaces(&qualified)
        };
        if target_formula.is_empty() {
            self.status = Some(crate::shell::i18n::text("_xlsx.enter-a-reference-range-or-formula").into());
            cx.notify();
            return false;
        }
        let ok = self.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: name.to_string(), sheet: None, formula: Some(target_formula) })]).is_ok();
        cx.notify();
        ok
    }

    /// Define `name` (for the whole workbook) as the selected range, or
    /// change what it stands for to it.
    pub(crate) fn name_selection(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        let Some(formula) = self.selection_formula() else { return false };
        self.set_named_formula(name, &formula, cx)
    }

    /// Start editing an existing defined name.
    pub(crate) fn edit_name(&mut self, index: usize, name: &str, formula: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut().and_then(|s| s.names.as_mut()) {
            state.editing = Some(index);
            state.new_name.update(cx, |i, cx| i.set_value(name, window, cx));
            state.new_formula.update(cx, |i, cx| i.set_value(format!("={formula}"), window, cx));
        }
        cx.notify();
    }

    /// Delete a name.
    pub(crate) fn delete_name(&mut self, name: &str, sheet: Option<usize>, cx: &mut Context<Self>) {
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: name.to_string(), sheet, formula: None })]) {
            log::warn!("deleting a name rejected: {err}");
        }
        cx.notify();
    }

    /// Open the name manager in the sidebar.
    pub(crate) fn open_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.new-name")));
        let initial_formula = self.selection_formula().map(|f| format!("={f}")).unwrap_or_default();
        let placeholder = crate::shell::i18n::text("_xlsx.refers-to-or-formula-e-g-sum-a1-a5-or-0-1");
        let new_formula = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder(placeholder);
            if !initial_formula.is_empty() {
                state.set_value(initial_formula, window, cx);
            }
            state
        });
        if let Some(state) = self.sheet_state_mut() {
            state.names = Some(NamesPanel { new_name, new_formula, editing: None });
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

    /// The name manager's panel.
    pub(crate) fn names_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let (new_name, new_formula, editing) = {
            let p = self.sheet_state()?.names.as_ref()?;
            (p.new_name.clone(), p.new_formula.clone(), p.editing)
        };
        let names: Vec<(String, Option<usize>, String)> =
            self.presentation().workbook.as_ref().map(|w| w.names.iter().filter(|n| !n.hidden && !n.name.starts_with("_xlnm.")).map(|n| (n.name.clone(), n.sheet, n.formula.clone())).collect()).unwrap_or_default();
        let rows = names.into_iter().enumerate().map(|(i, (name, sheet, formula))| {
            let (n1, n2, n3, f3) = (name.clone(), name.clone(), name.clone(), formula.clone());
            v_flex()
                .gap_1()
                .py(px(4.))
                .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(SharedString::from(name)))
                .child(div().text_size(px(12.)).child(SharedString::from(format!("={formula}"))))
                .child(
                    h_flex()
                        .gap_1()
                        .child(Button::new(("name-edit", i)).small().ghost().label(crate::shell::i18n::text("_xlsx.edit")).on_click(cx.listener(move |this, _, window, cx| {
                            this.edit_name(i, &n3, &f3, window, cx);
                        })))
                        .child(Button::new(("name-set", i)).small().ghost().label(crate::shell::i18n::text("_xlsx.use-selection")).on_click(cx.listener(move |this, _, _, cx| {
                            this.name_selection(&n1, cx);
                        })))
                        .child(Button::new(("name-delete", i)).small().ghost().label(crate::shell::i18n::text("_shell.delete")).on_click(cx.listener(move |this, _, _, cx| this.delete_name(&n2, sheet, cx)))),
                )
        });
        let name_input = new_name.clone();
        let formula_input = new_formula.clone();
        let is_editing = editing.is_some();
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_shell.name")))
            .child(Input::new(&new_name).small())
            .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.refers-to-or-formula")))
            .child(Input::new(&new_formula).small())
            .child({
                let (add_name, add_formula) = (name_input.clone(), formula_input.clone());
                let mut btn_row = h_flex()
                    .gap_1()
                    .child(Button::new("name-add").primary().small().label(if is_editing { crate::shell::i18n::text("_xlsx.update-name") } else { crate::shell::i18n::text("_xlsx.add-name") }).on_click(cx.listener(move |this, _, window, cx| {
                        let name_text = add_name.read(cx).value().trim().to_string();
                        let formula_text = add_formula.read(cx).value().trim().to_string();
                        if this.set_named_formula(&name_text, &formula_text, cx) {
                            add_name.update(cx, |i, cx| i.set_value("", window, cx));
                            let default_formula = this.selection_formula().map(|f| format!("={f}")).unwrap_or_default();
                            add_formula.update(cx, |i, cx| i.set_value(default_formula, window, cx));
                            if let Some(state) = this.sheet_state_mut().and_then(|s| s.names.as_mut()) {
                                state.editing = None;
                            }
                        }
                    })));
                if is_editing {
                    let (cancel_name, cancel_formula) = (name_input.clone(), formula_input.clone());
                    btn_row = btn_row.child(Button::new("name-cancel").small().ghost().label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(move |this, _, window, cx| {
                        cancel_name.update(cx, |i, cx| i.set_value("", window, cx));
                        let default_formula = this.selection_formula().map(|f| format!("={f}")).unwrap_or_default();
                        cancel_formula.update(cx, |i, cx| i.set_value(default_formula, window, cx));
                        if let Some(state) = this.sheet_state_mut().and_then(|s| s.names.as_mut()) {
                            state.editing = None;
                        }
                        cx.notify();
                    })));
                }
                btn_row
            })
            .children(rows);
        Some(SidebarPanel { title: "_xlsx.name-manager".into(), body: body.into_any_element() })
    }
}

#[cfg(test)]
mod tests {
    use super::valid_name;

    #[test]
    fn names() {
        assert!(valid_name("売上"));
        assert!(valid_name("Tax_Rate"));
        assert!(!valid_name("A1"));
        assert!(!valid_name("XFD100"));
        assert!(!valid_name("R1C1"));
        assert!(!valid_name("1st"));
        assert!(!valid_name("my name"));
        assert!(valid_name("ABCD1"));
    }
}
