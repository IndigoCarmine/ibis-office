// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! リンク (X1): the sidebar panel that makes the selection a link to an
//! address, or removes the link (⌘K, 挿入 > リンク, the toolbar's button).

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{FlowCommand, Link};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The panel's address field.
pub(crate) struct LinkState {
    pub(crate) address: Entity<InputState>,
    _subscription: Subscription,
}

impl SlideEditor {
    /// The link of the selection's first character.
    pub(crate) fn link_at_selection(&self) -> Option<Link> {
        let (start, end) = self.ordered_selection()?;
        let p = ibis_doc_model::flow_command::paragraph(self.flow()?, start.para)?;
        let offset = if end != start { start.offset } else { start.offset.saturating_sub(1) };
        let mut at = 0;
        for run in &p.runs {
            let len = run.len();
            if offset < at + len {
                return run.link.clone();
            }
            at += len;
        }
        None
    }

    /// Make the selection (in one paragraph) a link to `address`, or no
    /// link (an empty address). When adding a link, apply the Word
    /// `Hyperlink` character style; when removing it, clear the style.
    pub(crate) fn set_link(&mut self, address: &str, cx: &mut Context<Self>) {
        let Some((start, end)) = self.ordered_selection() else { return };
        if start == end || start.para != end.para {
            return;
        }
        let address = address.trim();
        let link = if address.is_empty() {
            None
        } else if let Some(bookmark) = address.strip_prefix('#') {
            Some(Link::Anchor(bookmark.to_string()))
        } else {
            Some(Link::Url(address.to_string()))
        };
        let keep = self.text_selection();
        let Some(doc) = self.flow() else { return };
        let mut commands = Vec::new();
        match link {
            Some(link) => {
                // Word applies the Hyperlink character style to newly linked text.
                let style_id = doc
                    .styles
                    .styles
                    .iter()
                    .find(|(_, s)| s.kind == ibis_doc_model::StyleKind::Character && s.name.eq_ignore_ascii_case("hyperlink"))
                    .map(|(id, _)| id.clone())
                    .or_else(|| doc.styles.styles.get("Hyperlink").map(|_| "Hyperlink".to_string()))
                    .unwrap_or_else(|| {
                        let id = "Hyperlink".to_string();
                        let style = ibis_doc_model::FlowStyle {
                            kind: ibis_doc_model::StyleKind::Character,
                            name: "Hyperlink".to_string(),
                            based_on: Some("DefaultParagraphFont".to_string()),
                            run: ibis_doc_model::RunFormat {
                                color: Some(ibis_doc_model::Rgb(0x0563C1)),
                                underline: Some(true),
                                underline_style: Some(ibis_doc_model::UnderlineStyle::Single),
                                ..Default::default()
                            },
                            ..Default::default()
                        };
                        commands.push(FlowCommand::SetStyle { id: id.clone(), style: Some(style) });
                        id
                    });
                commands.push(FlowCommand::SetLink { from: start, to: end, link: Some(link) });
                commands.push(FlowCommand::SetCharStyle { from: start, to: end, style: Some(style_id) });
            }
            None => {
                commands.push(FlowCommand::SetLink { from: start, to: end, link: None });
                // If the selection had a Hyperlink character style, remove it.
                if let Some(p) = ibis_doc_model::flow_command::paragraph(doc, start.para) {
                    let mut at = 0;
                    let mut has_hyperlink = false;
                    for run in &p.runs {
                        let len = run.len();
                        let run_end = at + len;
                        if start.offset < run_end && at < end.offset
                            && run.char_style.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("hyperlink") || doc.styles.styles.get(s).is_some_and(|st| st.name.eq_ignore_ascii_case("hyperlink")))
                        {
                            has_hyperlink = true;
                            break;
                        }
                        at = run_end;
                    }
                    if has_hyperlink {
                        commands.push(FlowCommand::SetCharStyle { from: start, to: end, style: None });
                    }
                }
            }
        }
        self.run_flow(commands, cx);
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
    }

    /// ⌘K: the panel, with the selection's link's address.
    pub(crate) fn open_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = match self.link_at_selection() {
            Some(Link::Url(u)) => u,
            Some(Link::Anchor(a)) => format!("#{a}"),
            None => String::new(),
        };
        if self.doc_state().is_some_and(|s| s.link.is_none()) {
            let address = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.link-panel.https-start-bookmark")));
            let subscription = cx.subscribe_in(&address, window, |this, input, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().to_string();
                    this.set_link(&text, cx);
                }
            });
            if let Some(state) = self.doc_state_mut() {
                state.link = Some(LinkState { address, _subscription: subscription });
                state.find = None;
            }
        }
        if let Some(address) = self.doc_state().and_then(|s| s.link.as_ref()).map(|l| l.address.clone()) {
            address.update(cx, |a, cx| {
                a.set_value(current, window, cx);
                a.focus(window, cx);
            });
        }
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.style_edit = None;
            state.font_edit=None; state.color_edit=None; state.paragraph_edit = None; state.picture_edit = None; state.table_size = None;
            state.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn link_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let address = self.doc_state()?.link.as_ref()?.address.clone();
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(Input::new(&address).small())
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("link-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|this, _, _, cx| {
                        let text = this.doc_state().and_then(|s| s.link.as_ref()).map(|l| l.address.read(cx).value().to_string()).unwrap_or_default();
                        this.set_link(&text, cx);
                    })))
                    .child(Button::new("link-remove").small().ghost().label(crate::shell::i18n::text("_shell.remove-link")).on_click(cx.listener(|this, _, _, cx| this.set_link("", cx)))),
            );
        Some(SidebarPanel { title: crate::shell::i18n::text("_shell.link"), body: body.into_any_element() })
    }
}
