// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Conditional formatting: formula and cell-value rules on a captured selection. Other
//! imported rule types remain listed and keep their original XML.
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::Disableable as _;
use ibis_doc_model::{CellRange, CfKind, CfRule, CfStyle, Command, ConditionalFormat, Rgb, SheetCommand, SlideId};
use crate::shell::chrome::{SidebarPanel, MenuEntry, MenuItem, fill_menu};
use crate::shell::editor::SlideEditor;

const OPERATORS: [(&str, &str); 8] = [
    ("lessThan", "_xlsx.less-than"), ("lessThanOrEqual", "_xlsx.less-than-or-equal-to"), ("equal", "_xlsx.equal-to"), ("notEqual", "_xlsx.not-equal-to"),
    ("greaterThanOrEqual", "_xlsx.greater-than-or-equal-to"), ("greaterThan", "_xlsx.greater-than"), ("between", "_xlsx.between"), ("notBetween", "_xlsx.not-between"),
];

pub(crate) struct ConditionalPanel {
    pub(crate) range: Entity<InputState>,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
    pub(crate) formula: Entity<InputState>,
    pub(crate) formula2: Entity<InputState>,
    kind: Option<usize>,
    pub(crate) fill: Entity<InputState>,
    slide: SlideId,
    ranges: Vec<CellRange>,
    // A copy guards against undo, deletion or sheet changes while the panel is open.
    editing: Option<(usize, ConditionalFormat)>,
    stop: bool,
    error: String,
}

impl SlideEditor {
    pub(crate) fn open_conditional(&mut self, index: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let old = index.and_then(|i| sheet.conditional.get(i).cloned().map(|f| (i, f)));
        if index.is_some() && old.is_none() { return; }
        let (text, second, kind, color, ranges) = match &old {
            Some((_, f)) if f.rules.len() == 1 => match &f.rules[0].kind {
                CfKind::Expression(text) | CfKind::TimePeriod { formula: text, .. } => (text.clone(), String::new(), None, f.rules[0].style.as_ref().and_then(|s| s.fill).unwrap_or(Rgb(0xFFC7CE)), f.ranges.clone()),
                CfKind::CellIs { operator, formulas } => {
                    let Some(kind) = OPERATORS.iter().position(|(name,_)| *name==operator) else { return; };
                    if formulas.len()!=if kind>=6 {2}else{1} { return; }
                    (formulas[0].clone(), formulas.get(1).cloned().unwrap_or_default(), Some(kind), f.rules[0].style.as_ref().and_then(|s| s.fill).unwrap_or(Rgb(0xFFC7CE)), f.ranges.clone())
                },
                _ => return,
            },
            Some(_) => return,
            None => (format!("{}>0", ibis_doc_model::cell_name(range.first_column, range.first_row)), String::new(), None, Rgb(0xFFC7CE), vec![range]),
        };
        let range_input = cx.new(|cx| InputState::new(window, cx).placeholder("A1:B10"));
        range_input.update(cx, |i, cx| i.set_value(ranges.iter().map(|r| format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row))).collect::<Vec<_>>().join(" "), window, cx));
        let formula = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.comparison-value-or-formula-e-g-10-a1-0")));
        let formula2 = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.second-comparison-value-or-formula")));
        formula2.update(cx, |i,cx| i.set_value(second,window,cx));
        let fill = cx.new(|cx| InputState::new(window, cx).placeholder("FFC7CE"));
        formula.update(cx, |i, cx| { i.set_value(text, window, cx); i.focus(window, cx); });
        fill.update(cx, |i, cx| i.set_value(format!("{:06X}", color.0), window, cx));
        let stop = old.as_ref().is_some_and(|(_, f)| f.rules[0].stop);
        let panel = ConditionalPanel { range: range_input, ui: Default::default(), formula, formula2, kind, fill, slide: self.current_slide, ranges, editing: old, stop, error: String::new() };
        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// The open panel's comparison (an `OPERATORS` index; `None`: a
    /// formula) and, if given, its formula.
    pub(crate) fn set_conditional_choice(&mut self, kind: Option<usize>, formula: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().conditional_edit.as_mut() else { return };
        p.kind = kind;
        p.error.clear();
        let input = p.formula.clone();
        if let Some(f) = formula {
            input.update(cx, |i, cx| i.set_value(f, window, cx));
        } else if kind.is_some() {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
        cx.notify();
    }

    pub(crate) fn apply_conditional(&mut self, cx: &mut Context<Self>) {
        self.apply_conditional_internal(false, cx);
    }

    pub(crate) fn apply_conditional_as_copy(&mut self, cx: &mut Context<Self>) {
        self.apply_conditional_internal(true, cx);
    }

    fn apply_conditional_internal(&mut self, as_copy: bool, cx: &mut Context<Self>) {
        let Some(p) = self.wb().conditional_edit.as_ref() else { return };
        let formula = p.formula.read(cx).value().trim().trim_start_matches('=').to_string();
        let second = p.formula2.read(cx).value().trim().trim_start_matches('=').to_string();
        let pair = p.kind.is_some_and(|kind| kind>=6);
        let hex = p.fill.read(cx).value().trim().trim_start_matches('#').to_string();
        let color = u32::from_str_radix(&hex, 16).ok().filter(|_| hex.len() == 6);
        let ranges = parse_ranges(&p.range.read(cx).value());
        let valid = ranges.is_some() && !formula.is_empty() && ibis_calc::formula::parser::parse(&formula).is_ok() && color.is_some() && (!pair || !second.is_empty() && ibis_calc::formula::parser::parse(&second).is_ok());
        let unchanged = as_copy || (p.slide == self.current_slide && p.editing.as_ref().is_none_or(|(i, old)| self.current_sheet().is_some_and(|(_, s)| s.conditional.get(*i).is_some_and(|f|f==old && f.source_index==old.source_index))));
        if !valid || !unchanged {
            self.wb_mut().conditional_edit.as_mut().unwrap().error = if !unchanged { crate::shell::i18n::text("_xlsx.the-sheet-or-rule-has-changed-reopen-it") } else { crate::shell::i18n::text("_xlsx.enter-a-valid-range-or-formula-and-a-six-digit-f") }.into();
            cx.notify();
            return;
        }
        let mut format = if as_copy {
            ConditionalFormat {
                source_index: None,
                ranges: ranges.clone().unwrap(),
                rules: vec![CfRule { kind: CfKind::Expression(formula.clone()), priority: 1, stop: p.stop, style: None }],
            }
        } else {
            p.editing.as_ref().map(|(_, f)| f.clone()).unwrap_or(ConditionalFormat {
                source_index: None,
                ranges: p.ranges.clone(), rules: vec![CfRule { kind: CfKind::Expression(formula.clone()), priority: 1, stop: false, style: None }],
            })
        };
        format.ranges = ranges.unwrap();
        format.rules[0].stop = p.stop;
        format.rules[0].kind = match p.kind { None => CfKind::Expression(formula), Some(kind) => CfKind::CellIs { operator: OPERATORS[kind].0.into(), formulas: if pair {vec![formula, second]}else{vec![formula]} } };
        // A new rule: Excel's 濃い赤の文字 with the fill chosen.
        let new_rule = format.rules[0].style.is_none();
        let style = format.rules[0].style.get_or_insert_with(CfStyle::default);
        style.fill = Some(Rgb(color.unwrap()));
        if new_rule {
            style.color = Some(Rgb(0x9C0006));
        }
        let command = if as_copy {
            SheetCommand::AddConditional { slide: p.slide, format }
        } else {
            match &p.editing {
                Some((index, _)) => SheetCommand::UpdateConditional { slide: p.slide, index: *index, format },
                None => SheetCommand::AddConditional { slide: p.slide, format },
            }
        };
        if let Err(err) = self.document.execute(vec![Command::Sheet(command)]) {
            self.wb_mut().conditional_edit.as_mut().unwrap().error = err.to_string();
        } else {
            // Require reopening before applying again, so one click is one edit.
            self.wb_mut().conditional_edit = None;
        }
        cx.notify();
    }

    fn duplicate_conditional_rule(&mut self, index: usize, rule: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb().conditional_edit.as_ref() else { return };
        if p.slide != self.current_slide { return; }
        let command = SheetCommand::DuplicateConditionalRule { slide: p.slide, index, rule };
        match self.document.execute(vec![Command::Sheet(command)]) {
            Ok(()) => self.open_conditional(None, window, cx),
            Err(err) => {
                if let Some(p) = self.wb_mut().conditional_edit.as_mut() {
                    p.error = err.to_string();
                }
                cx.notify();
            }
        }
    }

    fn delete_conditional_rule(&mut self, index: usize, rule: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb().conditional_edit.as_ref() else { return };
        if p.slide != self.current_slide { return; }
        let command = SheetCommand::DeleteConditionalRule { slide: p.slide, index, rule };
        match self.document.execute(vec![Command::Sheet(command)]) {
            Ok(()) => self.open_conditional(None, window, cx),
            Err(err) => {
                if let Some(p) = self.wb_mut().conditional_edit.as_mut() {
                    p.error = err.to_string();
                }
                cx.notify();
            }
        }
    }

    fn manage_conditional(&mut self, index: usize, rule: usize, up: Option<bool>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb().conditional_edit.as_ref() else { return };
        if p.slide != self.current_slide { return; }
        let Some(old) = self.current_sheet().and_then(|(_, s)| s.conditional.get(index)).and_then(|f| f.rules.get(rule)) else { return };
        let command = match up {
            Some(up) => SheetCommand::MoveConditionalRule { slide: p.slide, index, rule, up },
            None => SheetCommand::SetConditionalStop { slide: p.slide, index, rule, stop: !old.stop },
        };
        match self.document.execute(vec![Command::Sheet(command)]) {
            Ok(()) => self.open_conditional(None, window, cx),
            Err(err) => { self.wb_mut().conditional_edit.as_mut().unwrap().error = err.to_string(); cx.notify(); },
        }
    }

    pub(crate) fn clear_conditional_selection(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let command = SheetCommand::ClearConditional { slide: self.current_slide, ranges: vec![range] };
        if let Err(err) = self.document.execute(vec![Command::Sheet(command)]) {
            self.status = Some(crate::shell::i18n::format("_xlsx.there-are-no-rules-to-delete-err", &[("err", &err)]).into());
        }
        self.wb_mut().conditional_edit = None;
        cx.notify();
    }

    pub(crate) fn conditional_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.conditional_edit.as_ref()?;
        let range = p.ranges.iter().map(|r| format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row))).collect::<Vec<_>>().join(" ");
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div().relative().child(child).child(canvas(|_, _, _| {}, move |bounds, _, _, _| { ui.borrow_mut().insert(id.clone(), bounds); }).absolute().top_0().left_0().size_full()).into_any_element()
        };
        let editor=cx.entity().downgrade();
        let mut choices:Vec<MenuEntry>=vec![MenuItem::tr("conditional.expression").checked(p.kind.is_none()).on(|this,_,cx|{if let Some(p)=this.wb_mut().conditional_edit.as_mut(){p.kind=None;p.error.clear();}cx.notify();}).into()];
        let choice_ids=["conditional.less-than","conditional.less-equal","conditional.equal","conditional.not-equal","conditional.greater-equal","conditional.greater-than","conditional.between","conditional.not-between"];
        for (kind,(_,label)) in OPERATORS.iter().enumerate() {
            choices.push(MenuItem::new(choice_ids[kind],crate::shell::i18n::format("_xlsx.cell-value-label", &[("label", &label)])).checked(p.kind==Some(kind)).on(move|this,_,cx|{if let Some(p)=this.wb_mut().conditional_edit.as_mut(){p.kind=Some(kind);p.error.clear();}cx.notify();}).into());
        }
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .child(div().text_size(px(12.)).child(SharedString::from(crate::shell::i18n::format("_xlsx.applies-to-range", &[("range", &range)]))))
            .child(mark("kind", Button::new("conditional-kind").small().label(p.kind.map_or_else(|| crate::shell::i18n::text("_xlsx.formula-is-true-2").to_string(), |k| crate::shell::i18n::format("_xlsx.cell-value-label", &[("label", &OPERATORS[k].1)]))).dropdown_menu(move|m,window,cx|fill_menu(m.min_w(px(200.)),&choices,&editor,window,cx)).into_any_element()))
            .child(mark("range", Input::new(&p.range).small().into_any_element()))
            .child(mark("formula", Input::new(&p.formula).small().into_any_element()))
            .when(p.kind.is_some_and(|k|k>=6), |d| d.child(mark("formula2",Input::new(&p.formula2).small().into_any_element())))
            .child(crate::shell::i18n::text("_xlsx.fill-color-rgb"))
            .child(mark("fill", Input::new(&p.fill).small().into_any_element()))
            .child(mark("stop", Button::new("conditional-stop").small().label(if p.stop { crate::shell::i18n::text("_xlsx.stop-if-true-on") } else { crate::shell::i18n::text("_xlsx.stop-if-true-off") }).on_click(cx.listener(|this, _, _, cx| {
                if let Some(p) = this.wb_mut().conditional_edit.as_mut() { p.stop = !p.stop; }
                cx.notify();
            })).into_any_element()))
            .child(div().text_size(px(12.)).child(SharedString::from(p.error.clone())))
            .child(h_flex().gap_1()
                .child(mark("apply", Button::new("conditional-apply").primary().small().label(if p.editing.is_some() { crate::shell::i18n::text("_xlsx.update-rule") } else { crate::shell::i18n::text("_xlsx.add-rule") }).on_click(cx.listener(|this, _, _, cx| this.apply_conditional(cx))).into_any_element()))
                .when(p.editing.is_some(), |d| d.child(mark("duplicate-apply", Button::new("conditional-duplicate-apply").small().label(crate::shell::i18n::text("_xlsx.add-as-copy")).on_click(cx.listener(|this, _, _, cx| this.apply_conditional_as_copy(cx))).into_any_element())))
            )
            .child(mark("clear", Button::new("conditional-clear").small().label(crate::shell::i18n::text("_xlsx.delete-rules-in-selection")).on_click(cx.listener(|this, _, _, cx| this.clear_conditional_selection(cx))).into_any_element()));
        if let Some((_, sheet)) = self.current_sheet() {
            let mut order: Vec<_> = sheet.conditional.iter().enumerate().flat_map(|(i, f)| f.rules.iter().enumerate().map(move |(j, r)| (i, j, r))).collect();
            order.sort_by_key(|(_, _, r)| r.priority);
            for (at, &(index, rule, r)) in order.iter().enumerate() {
                let editable = sheet.conditional[index].rules.len() == 1 && match &r.kind { CfKind::Expression(_) | CfKind::TimePeriod { .. } => true, CfKind::CellIs {operator, formulas} => OPERATORS.iter().position(|(n,_)|n==operator).is_some_and(|k|formulas.len()==if k>=6 {2}else{1}), _=>false };
                body = body.child(div().text_size(px(12.)).child(crate::shell::i18n::format("_xlsx.priority-priority-rule-group-number", &[("priority", &r.priority), ("group", &(index + 1)), ("number", &(rule + 1))])))
                    .child(h_flex().gap_1()
                        .child(mark(&format!("edit-{index}"), Button::new(("conditional-edit", at)).small().label(crate::shell::i18n::text("_xlsx.edit")).disabled(!editable).on_click(cx.listener(move |this, _, window, cx| this.open_conditional(Some(index), window, cx))).into_any_element()))
                        .child(mark(&format!("duplicate-{index}-{rule}"), Button::new(("conditional-duplicate", at)).small().label(crate::shell::i18n::text("_shell.duplicate")).on_click(cx.listener(move |this, _, window, cx| this.duplicate_conditional_rule(index, rule, window, cx))).into_any_element()))
                        .child(mark(&format!("delete-{index}-{rule}"), Button::new(("conditional-delete", at)).small().label(crate::shell::i18n::text("_shell.delete")).on_click(cx.listener(move |this, _, window, cx| this.delete_conditional_rule(index, rule, window, cx))).into_any_element()))
                        .child(mark(&format!("up-{index}-{rule}"), Button::new(("conditional-up", at)).small().label("↑").disabled(at == 0).on_click(cx.listener(move |this, _, window, cx| this.manage_conditional(index, rule, Some(true), window, cx))).into_any_element()))
                        .child(mark(&format!("down-{index}-{rule}"), Button::new(("conditional-down", at)).small().label("↓").disabled(at + 1 == order.len()).on_click(cx.listener(move |this, _, window, cx| this.manage_conditional(index, rule, Some(false), window, cx))).into_any_element()))
                        .child(mark(&format!("rule-stop-{index}-{rule}"), Button::new(("conditional-rule-stop", at)).small().label(if r.stop { crate::shell::i18n::text("_xlsx.stop-on") } else { crate::shell::i18n::text("_xlsx.stop-off") }).on_click(cx.listener(move |this, _, window, cx| this.manage_conditional(index, rule, None, window, cx))).into_any_element())));
            }
        }
        Some(SidebarPanel { title: "xlsx.conditional".into(), body: body.into_any_element() })
    }
}

fn parse_ranges(text: &str) -> Option<Vec<CellRange>> {
    let ranges: Option<Vec<_>> = text.split_whitespace().map(|word| {
        let mut ends = word.split(':');
        let a = ibis_calc::formula::parser::parse_cell(ends.next()?)?;
        let b = match ends.next() { Some(s) => ibis_calc::formula::parser::parse_cell(s)?, None => a.clone() };
        if ends.next().is_some() || a.row > b.row || a.column > b.column || b.row >= 1_048_576 || b.column >= 16_384 { return None; }
        Some(CellRange { first_row: a.row, first_column: a.column, last_row: b.row, last_column: b.column })
    }).collect();
    ranges.filter(|r| !r.is_empty())
}
