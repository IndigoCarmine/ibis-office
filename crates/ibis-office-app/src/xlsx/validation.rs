// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データの入力規則: a list's drop-down by the selected cell, the message
//! when a cell with one is selected, a value that breaks the rule refused
//! (or warned about), and the sidebar's panel to set a list (and a
//! message) on the selected range or take the rule off.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand, Sheet, Validation};

use crate::shell::chrome::{MenuEntry, MenuItem, SidebarPanel, fill_menu};
use crate::shell::editor::SlideEditor;

/// The panel's fields, as Excel's dialog: 設定 (the kind of value, the
/// comparison, its values, blanks, the drop-down), 入力時メッセージ and
/// エラー メッセージ.
pub(crate) struct ValidationPanel {
    /// `none` (すべての値), `whole`, `decimal`, `list`, `date`, `time`,
    /// `textLength`, `custom`.
    pub(crate) kind: &'static str,
    pub(crate) operator: &'static str,
    /// A list's items or range, a bound, a custom formula.
    pub(crate) items: Entity<InputState>,
    pub(crate) second: Entity<InputState>,
    pub(crate) allow_blank: bool,
    pub(crate) dropdown: bool,
    pub(crate) show_prompt: bool,
    pub(crate) prompt_title: Entity<InputState>,
    pub(crate) prompt: Entity<InputState>,
    pub(crate) show_error: bool,
    /// `stop`, `warning`, `information`.
    pub(crate) style: &'static str,
    pub(crate) error_title: Entity<InputState>,
    pub(crate) error_text: Entity<InputState>,
    pub(crate) message: String,
    /// The tab shown: 0 設定, 1 入力時メッセージ, 2 エラー メッセージ.
    pub(crate) tab: usize,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

/// The kinds of value, as Excel's 入力値の種類 lists them.
pub(crate) const KINDS: [(&str, &str); 8] = [
    ("none", "fix.validation.any-value"),
    ("whole", "fix.validation.whole-number"),
    ("decimal", "fix.validation.decimal"),
    ("list", "fix.validation.list"),
    ("date", "docx.field.date"),
    ("time", "docx.field.time"),
    ("textLength", "fix.validation.text-length"),
    ("custom", "_shell.custom"),
];

/// The comparisons (データ).
pub(crate) const OPERATORS: [(&str, &str); 8] = [
    ("between", "fix.validation.between"),
    ("notBetween", "fix.validation.not-between"),
    ("equal", "fix.validation.equal"),
    ("notEqual", "fix.validation.not-equal"),
    ("greaterThan", "fix.validation.greater-than"),
    ("lessThan", "fix.validation.less-than"),
    ("greaterThanOrEqual", "fix.validation.greater-than-equal"),
    ("lessThanOrEqual", "fix.validation.less-than-equal"),
];

/// The error message's styles (スタイル).
pub(crate) const STYLES: [(&str, &str); 3] = [("stop", "fix.validation.stop"), ("warning", "fix.validation.warning"), ("information", "fix.validation.information")];

fn name_of(list: &[(&'static str, &'static str)], key: &str) -> &'static str {
    list.iter().find(|(k, _)| *k == key).map_or("", |(_, n)| n)
}

/// A list's drop-down (or another small menu by the cells), open.
pub(crate) struct ListPopup {
    pub(crate) menu: Entity<PopupMenu>,
    pub(crate) position: Point<Pixels>,
    pub(crate) _dismiss: Subscription,
}

/// The rule over a cell (the last one listed wins, as Excel keeps one).
pub(crate) fn rule_at(sheet: &Sheet, row: u32, column: u32) -> Option<&Validation> {
    sheet.validations.iter().rev().find(|v| v.ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column)))
}

/// A list rule's items: the text in quotes split at commas, or the
/// values of a range on the same sheet.
pub(crate) fn list_items(sheet: &Sheet, rule: &Validation) -> Vec<String> {
    let Some(f) = rule.formula1.as_deref() else { return Vec::new() };
    let f = f.trim();
    if let Some(inner) = f.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        return inner.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    }
    let area = f.rsplit('!').next().unwrap_or(f).replace('$', "");
    let Some(r) = parse_area(&area) else { return Vec::new() };
    let mut out = Vec::new();
    for row in r.0..=r.2 {
        for column in r.1..=r.3 {
            if let Some(c) = sheet.cell(row, column)
                && !c.text.is_empty()
            {
                out.push(c.text.clone());
            }
        }
    }
    out
}

/// `A1` or `A1:B3` as (first row, first column, last row, last column).
fn parse_area(text: &str) -> Option<(u32, u32, u32, u32)> {
    let one = |t: &str| -> Option<(u32, u32)> {
        let split = t.find(|c: char| c.is_ascii_digit())?;
        let column = ibis_calc::formula::parser::column_index(&t[..split])?;
        let row: u32 = t[split..].parse().ok()?;
        Some((row.checked_sub(1)?, column))
    };
    let (a, b) = text.split_once(':').unwrap_or((text, text));
    let (r0, c0) = one(a)?;
    let (r1, c1) = one(b)?;
    Some((r0.min(r1), c0.min(c1), r0.max(r1), c0.max(c1)))
}

/// Whether `value` keeps the rule.
pub(crate) fn keeps(sheet: &Sheet, rule: &Validation, value: &CellValue) -> bool {
    if value.is_empty() {
        return rule.allow_blank || rule.kind == "none";
    }
    let bound = |f: &Option<String>| -> Option<f64> {
        let f = f.as_deref()?;
        f.trim().parse::<f64>().ok().or_else(|| {
            // A cell of the same sheet.
            let area = f.rsplit('!').next().unwrap_or(f).replace('$', "");
            let (r, c, _, _) = parse_area(&area)?;
            match sheet.cell(r, c).map(|x| &x.value) {
                Some(CellValue::Number(n)) => Some(*n),
                _ => None,
            }
        })
    };
    let compare = |v: f64| -> bool {
        let (a, b) = (bound(&rule.formula1), bound(&rule.formula2));
        match (rule.operator.as_str(), a) {
            ("between", Some(a)) => b.is_some_and(|b| v >= a.min(b) && v <= a.max(b)),
            ("notBetween", Some(a)) => b.is_some_and(|b| v < a.min(b) || v > a.max(b)),
            ("equal", Some(a)) => v == a,
            ("notEqual", Some(a)) => v != a,
            ("greaterThan", Some(a)) => v > a,
            ("lessThan", Some(a)) => v < a,
            ("greaterThanOrEqual", Some(a)) => v >= a,
            ("lessThanOrEqual", Some(a)) => v <= a,
            _ => true,
        }
    };
    match rule.kind.as_str() {
        "list" => {
            let text = match value {
                CellValue::Number(n) => ibis_calc::eval::number_to_text(*n),
                CellValue::Text(t) => t.clone(),
                CellValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
                CellValue::Error(e) => e.as_str().to_string(),
                CellValue::Empty => String::new(),
            };
            list_items(sheet, rule).iter().any(|i| i.eq_ignore_ascii_case(&text))
        }
        "whole" => matches!(value, CellValue::Number(n) if n.fract() == 0.0 && compare(*n)),
        "decimal" | "date" | "time" => matches!(value, CellValue::Number(n) if compare(*n)),
        "textLength" => {
            let len = match value {
                CellValue::Text(t) => t.chars().count(),
                CellValue::Number(n) => ibis_calc::eval::number_to_text(*n).chars().count(),
                _ => 0,
            };
            compare(len as f64)
        }
        _ => true,
    }
}

/// Whether `value` put into (`row`, `column`) keeps the rule: a custom
/// rule's formula worked out with the value in the cell (its references
/// moved from the rule's first cell to this one), true unless it gives
/// FALSE or 0 (as Excel).
pub(crate) fn keeps_at(sheet: &Sheet, rule: &Validation, value: &CellValue, row: u32, column: u32) -> bool {
    if rule.kind != "custom" {
        return keeps(sheet, rule, value);
    }
    if value.is_empty() && rule.allow_blank {
        return true;
    }
    let (Some(formula), Some(first)) = (rule.formula1.as_deref(), rule.ranges.first()) else { return true };
    let mut with = sheet.clone();
    let mut cell = with.cell(row, column).cloned().unwrap_or(ibis_doc_model::SheetCell { runs: None, phonetic: None, row, column, value: CellValue::Empty, text: String::new(), kind: ibis_doc_model::CellKind::Empty, formula: None, style: 0 });
    cell.value = value.clone();
    cell.formula = None;
    with.put_cell(row, column, Some(cell));
    let shifted = ibis_calc::formula::shift(formula, row as i64 - first.first_row as i64, column as i64 - first.first_column as i64);
    let book = super::conditional::OneSheet(&with);
    match ibis_calc::recalc::evaluate(&book, (0, row, column), &shifted).0 {
        CellValue::Bool(b) => b,
        CellValue::Number(n) => n != 0.0,
        CellValue::Error(_) => false,
        _ => true,
    }
}

impl SlideEditor {
    /// The rule over the selected cell.
    pub(crate) fn current_rule(&self) -> Option<Validation> {
        let (column, row) = self.current_cell()?;
        let (_, sheet) = self.current_sheet()?;
        rule_at(sheet, row, column).cloned()
    }

    /// Whether `value` may go into (`row`, `column`): `Err` with the
    /// message when the rule refuses it (style `stop`); `Ok` with a warning
    /// to show for the other styles.
    pub(crate) fn check_entry(&self, row: u32, column: u32, value: &CellValue) -> Result<Option<String>, String> {
        let Some((_, sheet)) = self.current_sheet() else { return Ok(None) };
        let Some(rule) = rule_at(sheet, row, column) else { return Ok(None) };
        let Some((style, title, text)) = &rule.error else { return Ok(None) };
        if keeps_at(sheet, rule, value, row, column) {
            return Ok(None);
        }
        let message = if text.is_empty() { crate::shell::i18n::text("fix.validation.value-does-not-match-data").to_string() } else { text.clone() };
        let message = if title.is_empty() { message } else { format!("{title}：{message}") }; // i18n-ok（区切りの記号）
        if style == "stop" { Err(message) } else { Ok(Some(message)) }
    }

    /// Open the selected cell's list.
    pub(crate) fn open_validation_list(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rule) = self.current_rule() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some((column, row)) = self.current_cell() else { return };
        let items = list_items(sheet, &rule);
        let slide = self.current_slide;
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let entries: std::rc::Rc<Vec<MenuEntry>> = std::rc::Rc::new(
            items
                .into_iter()
                .map(|item| {
                    let text = item.clone();
                    MenuItem::new("xlsx.validation-item", item)
                        .on(move |this, _, cx| {
                            let input = match ibis_calc::input::parse_input(&text, date1904) {
                                ibis_calc::input::Typed::Value(v, _) => CellInput::Value(v),
                                ibis_calc::input::Typed::Formula(_) => CellInput::Value(CellValue::Text(text.clone())),
                            };
                            if let Err(err) = this.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row, column, input }] })]) {
                                log::warn!("choosing from the list rejected: {err}");
                            }
                            if let Some(s) = this.sheet_state_mut() {
                                s.list_popup = None;
                            }
                            cx.notify();
                        })
                        .into()
                })
                .collect(),
        );
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| fill_menu(menu.min_w(px(160.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(s) = this.sheet_state_mut() {
                s.list_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.wb_mut().list_popup = Some(ListPopup { menu, position, _dismiss: dismiss });
        cx.notify();
    }

    /// Open the panel (データの入力規則), with the selected cell's rule.
    pub(crate) fn open_validation_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rule = self.current_rule();
        let input = |placeholder: &str, text: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text(placeholder)));
            state.update(cx, |i, cx| i.set_value(text, window, cx));
            state
        };
        let r = rule.clone().unwrap_or(Validation { kind: "none".into(), operator: "between".into(), allow_blank: true, dropdown: true, ..Default::default() });
        // Bounds of dates and times shown as typed (a serial number read back).
        let shown = |f: &Option<String>| f.as_deref().unwrap_or("").trim_matches('"').to_string();
        let kind = KINDS.iter().map(|(k, _)| *k).find(|k| *k == r.kind).unwrap_or("none");
        let operator = OPERATORS.iter().map(|(k, _)| *k).find(|k| *k == r.operator).unwrap_or("between");
        let (style, error_title, error_text) = r.error.clone().unwrap_or(("stop".into(), String::new(), String::new()));
        let (prompt_title, prompt_text) = r.prompt.clone().unwrap_or_default();
        let panel = ValidationPanel {
            kind,
            operator,
            items: input("fix.validation.value-range-formula-list-comma", shown(&r.formula1), window, cx),
            second: input("fix.validation.second-value-between", shown(&r.formula2), window, cx),
            allow_blank: r.allow_blank,
            dropdown: r.dropdown,
            show_prompt: r.prompt.is_some(),
            prompt_title: input("_shell.title", prompt_title, window, cx),
            prompt: input("fix.validation.input-message-optional", prompt_text, window, cx),
            show_error: r.error.is_some() || rule.is_none(),
            style: STYLES.iter().map(|(k, _)| *k).find(|k| *k == style).unwrap_or("stop"),
            error_title: input("_shell.title", error_title, window, cx),
            error_text: input("fix.validation.error-alert", error_text, window, cx),
            message: String::new(),
            tab: 0,
            ui: Default::default(),
        };
        if let Some(s) = self.sheet_state_mut() {
            s.validation = Some(panel);
            s.page_setup = false;
            s.custom_format = None;
            s.panels.close();
        }
        self.sidebar_open = true;
        cx.notify();
    }

    fn validation_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.validation.as_mut()) {
            p.message = text;
        }
        cx.notify();
    }

    /// The panel's rule over the selected ranges (OK).
    pub(crate) fn apply_validation_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.validation.as_ref()) else { return };
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let value = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let (first, second) = (value(&p.items), value(&p.second));
        // A bound as stored: a number (dates and times as their serial
        // numbers), a reference or formula as typed (without `=`).
        let bound = |text: &str| -> Option<String> {
            if text.is_empty() {
                return None;
            }
            if let Some(f) = text.strip_prefix('=') {
                return Some(f.to_string());
            }
            match ibis_calc::input::parse_input(text, date1904) {
                ibis_calc::input::Typed::Value(CellValue::Number(n), _) => Some(ibis_calc::eval::number_to_text(n)),
                _ => Some(text.to_string()),
            }
        };
        let kind = p.kind;
        let two = matches!(p.operator, "between" | "notBetween");
        let (formula1, formula2) = match kind {
            "none" => (None, None),
            "list" => {
                if first.is_empty() {
                    return self.validation_message(crate::shell::i18n::text("fix.validation.enter-source").into(), cx);
                }
                // A range (=A1:A5) or items.
                let f = match first.strip_prefix('=') {
                    Some(range) => range.to_string(),
                    None => format!("\"{}\"", first.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(",")),
                };
                (Some(f), None)
            }
            "custom" => {
                if first.is_empty() {
                    return self.validation_message(crate::shell::i18n::text("fix.validation.enter-formula").into(), cx);
                }
                (Some(first.trim_start_matches('=').to_string()), None)
            }
            _ => {
                let (a, b) = (bound(&first), bound(&second));
                if a.is_none() || (two && b.is_none()) {
                    return self.validation_message(crate::shell::i18n::text("_xlsx.enter-a-value").into(), cx);
                }
                (a, if two { b } else { None })
            }
        };
        let rule = Validation {
            kind: kind.into(),
            operator: if matches!(kind, "list" | "custom" | "none") { "between".into() } else { p.operator.into() },
            formula1,
            formula2,
            allow_blank: p.allow_blank,
            dropdown: p.dropdown,
            prompt: p.show_prompt.then(|| (value(&p.prompt_title), value(&p.prompt))).filter(|(t, m)| !t.is_empty() || !m.is_empty()),
            error: p.show_error.then(|| (p.style.to_string(), value(&p.error_title), value(&p.error_text))),
            ..Default::default()
        };
        let (slide, ranges) = (self.current_slide, self.selected_ranges());
        if ranges.is_empty() {
            return;
        }
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetValidation { slide, ranges, rule: Some(rule) })]) {
            return self.validation_message(err.to_string(), cx);
        }
        self.validation_message(crate::shell::i18n::text("fix.validation.validation-rule-set").into(), cx);
    }

    /// A list over the selected range (items `a,b,c`), with a message if given.
    pub(crate) fn set_list_validation(&mut self, items: &str, prompt: &str, cx: &mut Context<Self>) {
        let Some(range) = self.selected_range() else { return };
        let items: Vec<&str> = items.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        if items.is_empty() {
            return;
        }
        let rule = Validation {
            kind: "list".into(),
            operator: "between".into(),
            formula1: Some(format!("\"{}\"", items.join(","))),
            allow_blank: true,
            dropdown: true,
            prompt: (!prompt.is_empty()).then(|| (String::new(), prompt.to_string())),
            error: Some(("stop".into(), String::new(), String::new())),
            ..Default::default()
        };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetValidation { slide, ranges: vec![range], rule: Some(rule) })]) {
            log::warn!("validation rejected: {err}");
        }
        cx.notify();
    }

    /// すべてクリア: the rules taken off the selected ranges.
    pub(crate) fn clear_validation(&mut self, cx: &mut Context<Self>) {
        let ranges = self.selected_ranges();
        if ranges.is_empty() {
            return;
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetValidation { slide, ranges, rule: None })]) {
            log::warn!("validation rejected: {err}");
        }
        self.validation_message(crate::shell::i18n::text("fix.validation.validation-rules-cleared").into(), cx);
    }

    /// 入力規則のルールに違反しているセル (col, row) の一覧を取得する。
    pub(crate) fn invalid_cells(&self) -> Vec<(u32, u32)> {
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let mut invalid = Vec::new();
        for rule in &sheet.validations {
            for range in &rule.ranges {
                for r in range.first_row..=range.last_row {
                    for c in range.first_column..=range.last_column {
                        let value = sheet.cell(r, c).map_or(&CellValue::Empty, |cell| &cell.value);
                        if !keeps_at(sheet, rule, value, r, c) {
                            invalid.push((c, r));
                        }
                    }
                }
            }
        }
        invalid.sort_unstable();
        invalid.dedup();
        invalid
    }

    /// 無効データのマーク表示 (Circle Invalid Data) を有効化する。
    pub(crate) fn circle_invalid_data(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.validation_circles = true;
        }
        cx.notify();
    }

    /// マークのクリア (Clear Validation Circles) を実行する。
    pub(crate) fn clear_validation_circles(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.validation_circles = false;
        }
        cx.notify();
    }

    pub(crate) fn validation_circles(&self) -> bool {
        self.sheet_state().is_some_and(|s| s.validation_circles)
    }

    /// ワークスペース上に無効データのマーク（赤い角丸楕円枠）を描画する。
    pub(crate) fn render_validation_circles(&self, scale: f32) -> Option<Div> {
        if !self.validation_circles() {
            return None;
        }
        let (shape, sheet) = self.current_sheet()?;
        let invalid = self.invalid_cells();
        if invalid.is_empty() {
            return None;
        }
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let at = |offsets: &[i64], i: u32| offsets[(i as usize).min(offsets.len() - 1)];
        let x = |c: u32| (shape.frame.x + at(&xs, c)) as f32 * scale;
        let y = |r: u32| (shape.frame.y + at(&ys, r)) as f32 * scale;

        let red = rgb(0xED1C24);
        let mut circles = div().absolute().top_0().left_0().size_full();
        for (c, r) in invalid {
            let (x0, x1, y0, y1) = (x(c), x(c + 1), y(r), y(r + 1));
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let pad = 2.0 * scale.max(1.0);
            circles = circles.child(
                div()
                    .absolute()
                    .left(px(x0 - pad))
                    .top(px(y0 - pad))
                    .w(px((x1 - x0) + pad * 2.0))
                    .h(px((y1 - y0) + pad * 2.0))
                    .border_2()
                    .border_color(red)
                    .rounded(px(10.0 * scale.max(1.0))),
            );
        }
        Some(circles)
    }

    pub(crate) fn validation_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        use crate::shell::i18n::text;
        use gpui_kit::component::h_flex;
        use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
        let p = self.sheet_state()?.validation.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let entity = cx.entity().downgrade();
        // A drop-down of `list`, setting the field `set` picks.
        let choose = |id: &'static str, list: &'static [(&'static str, &'static str)], current: &str, set: fn(&mut ValidationPanel, &'static str)| {
            let entity = entity.clone();
            mark(
                id,
                Button::new(id)
                    .small()
                    .label(text(name_of(list, current)))
                    .dropdown_menu(move |menu, _, _| {
                        list.iter().fold(menu, |menu, &(key, name)| {
                            let entity = entity.clone();
                            menu.item(PopupMenuItem::new(text(name)).on_click(move |_, _, cx| {
                                entity
                                    .update(cx, |this, cx| {
                                        if let Some(p) = this.sheet_state_mut().and_then(|s| s.validation.as_mut()) {
                                            set(p, key);
                                        }
                                        cx.notify();
                                    })
                                    .ok();
                            }))
                        })
                    })
                    .into_any_element(),
            )
        };
        type Flip = fn(&mut ValidationPanel);
        let toggle = |id: &'static str, on: bool, label: &str, flip: Flip| {
            mark(
                id,
                Button::new(id)
                    .small()
                    .ghost()
                    .label(format!("{} {}", if on { "☑" } else { "☐" }, text(label)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.sheet_state_mut().and_then(|s| s.validation.as_mut()) {
                            flip(p);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        let compared = !matches!(p.kind, "none" | "list" | "custom");
        // The dialog's three tabs.
        let tabs = h_flex().gap_1().flex_wrap().children(["fix.pivot.settings", "fix.validation.input-message", "fix.validation.error-alert"].into_iter().enumerate().map(|(i, name)| {
            mark(
                &format!("tab-{i}"),
                Button::new(("validation-tab", i))
                    .small()
                    .when(p.tab == i, |b| b.primary())
                    .when(p.tab != i, |b| b.ghost())
                    .label(text(name))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.sheet_state_mut().and_then(|s| s.validation.as_mut()) {
                            p.tab = i;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        }));
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).child(tabs);
        if p.tab == 0 {
        body = body
            .child(div().text_size(px(13.)).child(text("fix.validation.allow")))
            .child(choose("validation-kind", &KINDS, p.kind, |p, k| p.kind = k));
        if compared {
            body = body.child(div().text_size(px(13.)).child(text("fix.pivot-options.data"))).child(choose("validation-operator", &OPERATORS, p.operator, |p, k| p.operator = k));
        }
        if p.kind != "none" {
            let label = match p.kind {
                "list" => "fix.validation.source",
                "custom" => "_xlsx.formula-2",
                _ if matches!(p.operator, "between" | "notBetween") => "xlsx.chart-axis-min",
                _ => "chart.labels.show-value",
            };
            body = body.child(div().text_size(px(13.)).child(text(label))).child(mark("items", Input::new(&p.items).small().into_any_element()));
            if compared && matches!(p.operator, "between" | "notBetween") {
                body = body.child(div().text_size(px(13.)).child(text("_xlsx.max"))).child(mark("second", Input::new(&p.second).small().into_any_element()));
            }
            body = body.child(toggle("validation-blank", p.allow_blank, "fix.validation.ignore-blank", |p| p.allow_blank = !p.allow_blank));
            if p.kind == "list" {
                body = body.child(toggle("validation-dropdown", p.dropdown, "fix.validation.cell-dropdown", |p| p.dropdown = !p.dropdown));
            }
        }
        }
        if p.tab == 1 {
        body = body
            .child(toggle("validation-show-prompt", p.show_prompt, "fix.validation.show-input-message-when-cell", |p| p.show_prompt = !p.show_prompt));
        if p.show_prompt {
            body = body.child(mark("prompt-title", Input::new(&p.prompt_title).small().into_any_element())).child(mark("prompt", Input::new(&p.prompt).small().into_any_element()));
        }
        }
        if p.tab == 2 {
        body = body
            .child(toggle("validation-show-error", p.show_error, "fix.validation.show-error-alert-after-invalid", |p| p.show_error = !p.show_error));
        if p.show_error {
            body = body
                .child(h_flex().gap_2().items_center().child(div().text_size(px(13.)).child(text("_shell.styles"))).child(choose("validation-style", &STYLES, p.style, |p, k| p.style = k)))
                .child(mark("error-title", Input::new(&p.error_title).small().into_any_element()))
                .child(mark("error-text", Input::new(&p.error_text).small().into_any_element()));
        }
        }
        body = body
            .child(div().text_size(px(12.)).child(SharedString::from(p.message.clone())))
            .child(
                h_flex().gap_2().children([
                    mark("apply", Button::new("validation-set").small().label(text("fix.pivot.settings")).on_click(cx.listener(|this, _, _, cx| this.apply_validation_panel(cx))).into_any_element()),
                    mark("clear", Button::new("validation-clear").small().ghost().label(text("fix.validation.clear-all")).on_click(cx.listener(|this, _, _, cx| this.clear_validation(cx))).into_any_element()),
                ])
            )
            .child(
                h_flex().gap_2().children([
                    mark("circle-invalid", Button::new("validation-circle").small().ghost().label(text("_xlsx.circle-invalid-data")).on_click(cx.listener(|this, _, _, cx| this.circle_invalid_data(cx))).into_any_element()),
                    mark("clear-circles", Button::new("validation-clear-circles").small().ghost().label(text("_xlsx.clear-validation-circles")).on_click(cx.listener(|this, _, _, cx| this.clear_validation_circles(cx))).into_any_element()),
                ])
            );
        Some(SidebarPanel { title: crate::shell::i18n::text("_shell.data-validation").into(), body: body.into_any_element() })
    }
}

#[cfg(test)]
mod tests {
    use super::{CellValue, Sheet, Validation, keeps};

    #[test]
    fn lists_and_numbers() {
        let sheet = Sheet::default();
        let list = Validation { kind: "list".into(), formula1: Some(r#""りんご,みかん""#.into()), ..Default::default() };
        assert!(keeps(&sheet, &list, &CellValue::Text("みかん".into())));
        assert!(!keeps(&sheet, &list, &CellValue::Text("ぶどう".into())));
        let whole = Validation { kind: "whole".into(), operator: "between".into(), formula1: Some("1".into()), formula2: Some("10".into()), ..Default::default() };
        assert!(keeps(&sheet, &whole, &CellValue::Number(5.0)));
        assert!(!keeps(&sheet, &whole, &CellValue::Number(5.5)));
        assert!(!keeps(&sheet, &whole, &CellValue::Number(11.0)));
    }
}
