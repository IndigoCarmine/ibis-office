// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Tab stops and leader settings panel for docx paragraphs.

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ibis_doc_model::{Emu, FlowDocument, ParaChange, TabAlign, TabChange, TabStop};

use crate::shell::{chrome::SidebarPanel, editor::SlideEditor};

pub(crate) struct TabEditState {
    pub(crate) pos_field: Entity<InputState>,
    pub(crate) align: TabAlign,
    pub(crate) leader: bool,
    pub(crate) stops: Vec<TabStop>,
    pub(crate) original_stops: Vec<TabStop>,
    pub(crate) selected_index: Option<usize>,
    pub(crate) target: (u32, u32),
    pub(crate) document: Arc<FlowDocument>,
    pub(crate) error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

fn emu_to_pt(emu: Emu) -> f64 {
    emu as f64 / 12_700.0
}

fn pt_to_emu(pt: f64) -> Emu {
    (pt * 20.0).round() as i64 * 635
}

fn format_pt(pt: f64) -> String {
    format!("{:.1}", pt)
}

fn align_name(align: TabAlign) -> gpui::SharedString {
    match align {
        TabAlign::Left => crate::shell::i18n::text("docx.page-number.bottom-left"),
        TabAlign::Center => crate::shell::i18n::text("_shell.center-2"),
        TabAlign::Right => crate::shell::i18n::text("docx.page-number.bottom-right"),
        TabAlign::Decimal => crate::shell::i18n::text("_docx.decimal"),
    }
}

impl SlideEditor {
    pub(crate) fn open_tab_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.selected_paragraphs() else { return };
        let Some(document) = self.flow().cloned() else { return };
        let Some(p) = self.paragraph_of(target.0) else { return };
        let original_stops = p.tabs.clone();
        let stops = original_stops.clone();
        let is_empty = p.tabs.is_empty();
        let initial_val = stops.first().map_or("72".to_string(), |s| format_pt(emu_to_pt(s.position)));
        let initial_align = stops.first().map_or(TabAlign::Left, |s| s.align);
        let initial_leader = stops.first().map_or(false, |s| s.leader);

        let pos_field = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(initial_val, window, cx);
            state
        });

        let subscription = cx.subscribe_in(&pos_field, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.set_tab_stop(window, cx);
            }
        });

        if let Some(state) = self.doc_state_mut() {
            state.tab_edit = Some(TabEditState {
                pos_field,
                align: initial_align,
                leader: initial_leader,
                stops,
                original_stops,
                selected_index: if is_empty { None } else { Some(0) },
                target,
                document,
                error: None,
                _subscriptions: vec![subscription],
            });
            state.font_edit = None;
            state.color_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.style_edit = None;
            state.link = None;
            state.find = None;
            state.comments = None;
            state.paragraph_edit = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn choose_tab_align(&mut self, align: TabAlign, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        edit.align = align;
        cx.notify();
    }

    pub(crate) fn choose_tab_leader(&mut self, leader: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        edit.leader = leader;
        cx.notify();
    }

    pub(crate) fn select_tab_stop(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        if let Some(stop) = edit.stops.get(index).copied() {
            edit.selected_index = Some(index);
            edit.align = stop.align;
            edit.leader = stop.leader;
            let val = format_pt(emu_to_pt(stop.position));
            let field = edit.pos_field.clone();
            field.update(cx, |i, cx| i.set_value(val, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn set_tab_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        let text = edit.pos_field.read(cx).value().trim().to_string();
        let Ok(pt) = text.parse::<f64>() else {
            edit.error = Some(crate::shell::i18n::text("_docx.enter-a-number-for-the-tab-position").into());
            cx.notify();
            return;
        };
        if pt <= 0.0 || !pt.is_finite() || pt > 1584.0 {
            edit.error = Some(crate::shell::i18n::text("_docx.enter-a-value-greater-than-0-and-up-to-1584").into());
            cx.notify();
            return;
        };
        let emu = pt_to_emu(pt);
        let align = edit.align;
        let leader = edit.leader;
        edit.stops.retain(|s| s.position != emu);
        edit.stops.push(TabStop { position: emu, align, leader });
        edit.stops.sort_by_key(|s| s.position);
        edit.selected_index = edit.stops.iter().position(|s| s.position == emu);
        edit.error = None;
        let field = edit.pos_field.clone();
        field.update(cx, |i, cx| i.set_value(format_pt(pt), window, cx));
        cx.notify();
    }

    pub(crate) fn clear_tab_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        if let Some(idx) = edit.selected_index && idx < edit.stops.len() {
            edit.stops.remove(idx);
            edit.selected_index = None;
            edit.error = None;
        } else {
            let text = edit.pos_field.read(cx).value().trim().to_string();
            if let Ok(pt) = text.parse::<f64>() {
                let emu = pt_to_emu(pt);
                edit.stops.retain(|s| s.position != emu);
                edit.selected_index = None;
                edit.error = None;
            }
        }
        let field = edit.pos_field.clone();
        field.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        cx.notify();
    }

    pub(crate) fn clear_all_tab_stops(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) else { return };
        edit.stops.clear();
        edit.selected_index = None;
        edit.error = None;
        let field = edit.pos_field.clone();
        field.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        cx.notify();
    }

    pub(crate) fn apply_tab_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state().and_then(|s| s.tab_edit.as_ref()) else { return };
        if self.flow() != Some(&edit.document) {
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) {
                edit.error = Some(crate::shell::i18n::text("_docx.the-document-has-changed-reopen-the-tabs-setting").into());
            }
            cx.notify();
            return;
        }
        let (first, last) = edit.target;
        let original = edit.original_stops.clone();
        let new_stops = edit.stops.clone();

        // Build list of TabChanges
        let mut changes = Vec::new();
        // Clear stops that were removed
        for old in &original {
            if !new_stops.iter().any(|s| s.position == old.position) {
                changes.push(TabChange { position: old.position, stop: None });
            }
        }
        // Set new or updated stops
        for new in &new_stops {
            changes.push(TabChange { position: new.position, stop: Some(*new) });
        }

        let commands = self.paragraph_format_commands(first, last, ParaChange::Tabs(changes));
        if commands.is_empty() || self.run_flow(commands, cx) {
            let document = self.flow().unwrap().clone();
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.tab_edit.as_mut()) {
                edit.original_stops = new_stops;
                edit.document = document;
                edit.error = None;
            }
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn tab_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let edit = self.doc_state()?.tab_edit.as_ref()?;
        let pos_field = edit.pos_field.clone();
        let align = edit.align;
        let leader = edit.leader;
        let stops = edit.stops.clone();
        let selected_idx = edit.selected_index;
        let error = edit.error.clone();

        let choice_align = |id: &'static str, label: &'static str, value: TabAlign| {
            Button::new(id)
                .small()
                .label(crate::shell::i18n::text(label))
                .selected(align == value)
                .on_click(cx.listener(move |this, _, _, cx| this.choose_tab_align(value, cx)))
        };

        let choice_leader = |id: &'static str, label: &'static str, value: bool| {
            Button::new(id)
                .small()
                .label(crate::shell::i18n::text(label))
                .selected(leader == value)
                .on_click(cx.listener(move |this, _, _, cx| this.choose_tab_leader(value, cx)))
        };

        let stop_items: Vec<AnyElement> = stops
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let is_sel = selected_idx == Some(i);
                let label = format!("{} pt  {}  {}", format_pt(emu_to_pt(s.position)), align_name(s.align), crate::shell::i18n::text(if s.leader { "_docx.dotted" } else { "_shell.none" }));
                div()
                    .id(("tab-stop", i))
                    .px(px(8.))
                    .py(px(4.))
                    .text_sm()
                    .cursor_pointer()
                    .when(is_sel, |d| d.bg(rgb(0xd0e2ff)).font_weight(FontWeight::MEDIUM))
                    .when(!is_sel, |d| d.hover(|h| h.bg(rgb(0xeff1f4))))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| this.select_tab_stop(i, window, cx)))
                    .into_any_element()
            })
            .collect();

        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(92.)).text_sm().child(crate::shell::i18n::text("_docx.tab-stop-position-pt")))
                    .child(Input::new(&pos_field).small()),
            )
            .child(
                div()
                    .id("tab-stops-list")
                    .border_1()
                    .border_color(rgb(0xd1d5db))
                    .rounded_sm()
                    .h(px(100.))
                    .overflow_y_scroll()
                    .children(stop_items),
            )
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(crate::shell::i18n::text("_shell.arrange")))
            .child(
                h_flex()
                    .gap_1()
                    .child(choice_align("tab-align-left", "_shell.align-left", TabAlign::Left))
                    .child(choice_align("tab-align-center", "_shell.center-2", TabAlign::Center))
                    .child(choice_align("tab-align-right", "_shell.align-right", TabAlign::Right))
                    .child(choice_align("tab-align-decimal", "_docx.decimal", TabAlign::Decimal)),
            )
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(crate::shell::i18n::text("_docx.leader")))
            .child(
                h_flex()
                    .gap_1()
                    .child(choice_leader("tab-leader-none", "_shell.none", false))
                    .child(choice_leader("tab-leader-dot", "_docx.dotted-2", true)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("tab-set").small().label(crate::shell::i18n::text("fix.pivot.settings")).on_click(cx.listener(|this, _, window, cx| this.set_tab_stop(window, cx))))
                    .child(Button::new("tab-clear").small().label(crate::shell::i18n::text("_xlsx.clear")).on_click(cx.listener(|this, _, window, cx| this.clear_tab_stop(window, cx))))
                    .child(Button::new("tab-clear-all").small().label(crate::shell::i18n::text("fix.validation.clear-all")).on_click(cx.listener(|this, _, window, cx| this.clear_all_tab_stops(window, cx)))),
            )
            .children(error.map(|err| div().text_sm().text_color(rgb(0xcc0000)).child(err)))
            .child(
                Button::new("tab-apply").primary()
                    .small()
                    .label(crate::shell::i18n::text("_shell.apply"))
                    .on_click(cx.listener(|this, _, window, cx| this.apply_tab_edit(window, cx))),
            );

        Some(SidebarPanel { title: crate::shell::i18n::text("_docx.tabs-and-leaders"), body: body.into_any_element() })
    }
}
