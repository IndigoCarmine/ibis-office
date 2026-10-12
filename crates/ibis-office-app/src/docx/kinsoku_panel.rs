// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示形式 > 禁則文字の設定…: the characters a line may not start with and may not end with (Word's 禁則文字の設定),
//! for the whole document; the standard ones until they are changed.
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::FlowCommand;

use super::help_panels::HelpPanels;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct KinsokuPanel {
    pub(crate) start: Entity<InputState>,
    pub(crate) end: Entity<InputState>,
}

impl SlideEditor {
    /// The lists now: what the document sets, else the standard ones.
    pub(crate) fn kinsoku_lists(&self) -> (String, String) {
        let doc = self.flow();
        (
            doc.and_then(|d| d.no_line_start.clone()).unwrap_or_else(|| super::flow_layout::NO_LINE_START.to_string()),
            doc.and_then(|d| d.no_line_end.clone()).unwrap_or_else(|| super::flow_layout::NO_LINE_END.to_string()),
        )
    }

    /// Set the lists (`None`: the standard ones) in one step of undo.
    pub(crate) fn set_kinsoku_lists(&mut self, start: Option<String>, end: Option<String>, cx: &mut Context<Self>) {
        // A list equal to the standard one is not kept.
        let start = start.filter(|s| s != super::flow_layout::NO_LINE_START);
        let end = end.filter(|s| s != super::flow_layout::NO_LINE_END);
        self.run_flow(vec![FlowCommand::SetKinsokuLists { start, end }], cx);
    }

    pub(crate) fn open_kinsoku_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (start, end) = self.kinsoku_lists();
        let panel = KinsokuPanel {
            start: cx.new(|cx| InputState::new(window, cx).default_value(start)),
            end: cx.new(|cx| InputState::new(window, cx).default_value(end)),
        };
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.kinsoku = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_kinsoku_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.helper.kinsoku = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// 適用: the two lists as typed (an empty one is the standard list).
    pub(crate) fn apply_kinsoku_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.kinsoku.as_ref()) else { return };
        let (start, end) = (p.start.read(cx).value().to_string(), p.end.read(cx).value().to_string());
        let list = |v: String| if v.trim().is_empty() { None } else { Some(v.chars().filter(|c| !c.is_whitespace()).collect::<String>()) };
        self.set_kinsoku_lists(list(start), list(end), cx);
        self.close_kinsoku_panel(cx);
    }

    /// 標準に戻す.
    pub(crate) fn reset_kinsoku_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_kinsoku_lists(None, None, cx);
        let (start, end) = self.kinsoku_lists();
        if let Some(p) = self.doc_state().and_then(|s| s.helper.kinsoku.as_ref()) {
            p.start.update(cx, |i, cx| i.set_value(&start, window, cx));
            p.end.update(cx, |i, cx| i.set_value(&end, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn kinsoku_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.kinsoku.as_ref()?;
        let theme = cx.theme();
        let label = |key: &'static str| div().text_size(px(12.)).text_color(theme.muted_foreground).child(text(key));
        let body = v_flex()
            .gap_3()
            .p_3()
            .child(label("docx.kinsoku.description"))
            .child(v_flex().gap_1().child(label("docx.kinsoku.start")).child(Input::new(&p.start).small()))
            .child(v_flex().gap_1().child(label("docx.kinsoku.end")).child(Input::new(&p.end).small()))
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("kinsoku-apply").primary().small().label(text("docx.kinsoku.apply")).on_click(cx.listener(|this, _, _, cx| this.apply_kinsoku_panel(cx))))
                    .child(Button::new("kinsoku-cancel").ghost().small().label(text("docx.kinsoku.cancel")).on_click(cx.listener(|this, _, _, cx| this.close_kinsoku_panel(cx)))),
            )
            .child(Button::new("kinsoku-reset").ghost().small().label(text("docx.kinsoku.reset")).on_click(cx.listener(|this, _, window, cx| this.reset_kinsoku_panel(window, cx))));
        Some(SidebarPanel { title: text("docx.kinsoku.title").into(), body: body.into_any_element() })
    }
}

#[cfg(test)]
mod tests {
    use super::super::flow_layout::may_break;
    use ibis_doc_model::FlowDocument;

    #[test]
    fn the_documents_lists_replace_the_standard_ones() {
        let mut doc = FlowDocument::default();
        // Standard: a line may not start with 、 nor end with 「.
        assert!(!may_break(Some('あ'), Some('、'), true, true, &doc));
        assert!(!may_break(Some('「'), Some('あ'), true, true, &doc));
        assert!(may_break(Some('あ'), Some('い'), true, true, &doc));
        doc.no_line_start = Some("あ".to_string());
        doc.no_line_end = Some("い".to_string());
        // The document's lists: 、 may start a line now, あ may not; い may not end one, 「 may.
        assert!(may_break(Some('う'), Some('、'), true, true, &doc));
        assert!(!may_break(Some('う'), Some('あ'), true, true, &doc));
        assert!(!may_break(Some('い'), Some('う'), true, true, &doc));
        assert!(may_break(Some('「'), Some('う'), true, true, &doc));
        // Without 禁則処理 nothing is held.
        assert!(may_break(Some('う'), Some('あ'), false, true, &doc));
    }
}
