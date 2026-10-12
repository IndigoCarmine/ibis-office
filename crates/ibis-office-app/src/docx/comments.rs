// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コメント (X5): the sidebar panel listing the comments, where a comment
//! is put on the selection (挿入 > コメント, ⌥⌘M), answered, resolved and
//! removed.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::flow_command::{self, CommentAuthor};
use ibis_doc_model::{Block, FlowPos};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// What the panel's field writes: a new comment on a range, or an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentTarget {
    New { from: FlowPos, to: FlowPos },
    Reply(u32),
}

/// The panel's field and what it is for.
pub(crate) struct CommentState {
    pub(crate) input: Entity<InputState>,
    pub(crate) target: Option<CommentTarget>,
    _subscription: Subscription,
}

/// Who writes (the user's name, shell::user_name) and now, in UTC.
pub(crate) fn comment_author() -> CommentAuthor {
    // The same name as the other formats' comments (custom override,
    // else the OS's full name, else the account name).
    let author = crate::shell::user_name::user_name();
    let initials = crate::shell::user_name::user_initials();
    CommentAuthor { author, initials, date: now_utc() }
}

/// The time now as `w:date` writes it (`2026-10-01T09:00:00Z`).
fn now_utc() -> String {
    let secs = (crate::clock::unix_millis() / 1000) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// A comment's text.
fn comment_text(blocks: &[Block]) -> String {
    blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect::<Vec<_>>().join("\n")
}

impl SlideEditor {
    /// 挿入 > コメント: the panel, its field for a comment on the selection.
    pub(crate) fn open_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.ordered_selection().filter(|(a, b)| a != b).map(|(from, to)| CommentTarget::New { from, to });
        if self.doc_state().is_some_and(|s| s.comments.is_none()) {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_shell.type-a-comment-and-press-enter")));
            let subscription = cx.subscribe_in(&input, window, |this, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().to_string();
                    this.send_comment(&text, window, cx);
                }
            });
            if let Some(state) = self.doc_state_mut() {
                state.comments = Some(CommentState { input, target: None, _subscription: subscription });
            }
        }
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit=None; state.color_edit=None; state.paragraph_edit = None; state.picture_edit = None; state.table_size = None;
            if let Some(c) = state.comments.as_mut() {
                c.target = target;
            }
        }
        if target.is_some()
            && let Some(input) = self.doc_state().and_then(|s| s.comments.as_ref()).map(|c| c.input.clone())
        {
            input.update(cx, |i, cx| i.focus(window, cx));
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The field's text written as the comment or answer it is for.
    pub(crate) fn send_comment(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim();
        let Some(target) = self.doc_state().and_then(|s| s.comments.as_ref()).and_then(|c| c.target) else { return };
        if text.is_empty() {
            return;
        }
        let who = comment_author();
        let Some(doc) = self.flow() else { return };
        let commands = match target {
            CommentTarget::New { from, to } => flow_command::add_comment(doc, from, to, &who, text),
            CommentTarget::Reply(id) => flow_command::reply_comment(doc, id, &who, text),
        };
        let Some(commands) = commands else { return };
        let keep = self.text_selection();
        if self.run_flow(commands, cx)
            && let Some(state) = self.doc_state_mut()
        {
            if let Some(c) = state.comments.as_mut() {
                c.target = None;
            }
            // The marks moved the selection's end along.
            if let Some((_, c)) = keep {
                state.anchor = None;
                state.caret = Some(c);
            }
        }
        if let Some(input) = self.doc_state().and_then(|s| s.comments.as_ref()).map(|c| c.input.clone()) {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
        cx.notify();
    }

    /// 返信: the field for an answer to comment `id`.
    pub(crate) fn reply_to(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self.doc_state_mut().and_then(|s| s.comments.as_mut()) {
            c.target = Some(CommentTarget::Reply(id));
        }
        if let Some(input) = self.doc_state().and_then(|s| s.comments.as_ref()).map(|c| c.input.clone()) {
            input.update(cx, |i, cx| i.focus(window, cx));
        }
        cx.notify();
    }

    /// 解決 (or 再開): the comment and its answers resolved or opened again.
    pub(crate) fn resolve(&mut self, id: u32, done: bool, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let command = flow_command::resolve_comment(doc, id, done);
        self.run_flow(vec![command], cx);
    }

    /// 削除: the comment, its answers and their marks.
    pub(crate) fn delete_comment(&mut self, id: u32, cx: &mut Context<Self>) {
        let Some(command) = self.flow().and_then(|d| flow_command::delete_comment(d, id)) else { return };
        self.run_flow(vec![command], cx);
    }

    pub(crate) fn comments_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.comments.as_ref()?;
        let (input, target) = (state.input.clone(), state.target);
        let doc = self.flow()?.clone();
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.));
        let prompt = match target {
            Some(CommentTarget::New { .. }) => "docx.comments.comment-selection",
            Some(CommentTarget::Reply(_)) => "_shell.reply",
            None => "docx.comments.select-text-then-insert-comment",
        };
        body = body.child(div().text_sm().child(crate::shell::i18n::text(prompt)));
        if target.is_some() {
            body = body.child(Input::new(&input).small()).child(Button::new("comment-send").small().label(crate::shell::i18n::text("_shell.comment")).on_click(cx.listener(move |this, _, window, cx| {
                let text = input.read(cx).value().to_string();
                this.send_comment(&text, window, cx);
            })));
        }
        for c in doc.comments.iter().filter(|c| c.parent.is_none()) {
            let id = c.id;
            let replies: Vec<String> = doc.comments.iter().filter(|r| r.parent == Some(id)).map(|r| format!("{}：{}", r.author, comment_text(&r.blocks))).collect(); // i18n-ok（区切りの記号）
            let done = c.done;
            let mut card = v_flex()
                .gap_1()
                .p(px(8.))
                .rounded(px(8.))
                .bg(if done { rgb(0xf1f3f4) } else { rgb(0xfef7e0) })
                .child(div().text_xs().text_color(rgb(0x5f6368)).child(format!("{}{}", c.author, if done { crate::shell::i18n::text("_shell.resolved") } else { "".into() })))
                .child(div().text_sm().child(comment_text(&c.blocks)));
            for r in replies {
                card = card.child(div().text_sm().pl(px(8.)).child(r));
            }
            card = card.child(
                h_flex()
                    .gap_1()
                    .child(Button::new(("comment-reply", id as usize)).small().ghost().label(crate::shell::i18n::text("_shell.reply")).on_click(cx.listener(move |this, _, window, cx| this.reply_to(id, window, cx))))
                    .child(Button::new(("comment-done", id as usize)).small().ghost().label(crate::shell::i18n::text(if done { "pptx.comments.reopen" } else { "_shell.resolve" })).on_click(cx.listener(move |this, _, _, cx| this.resolve(id, !done, cx))))
                    .child(Button::new(("comment-delete", id as usize)).small().ghost().label(crate::shell::i18n::text("_shell.delete")).on_click(cx.listener(move |this, _, _, cx| this.delete_comment(id, cx)))),
            );
            body = body.child(card);
        }
        Some(SidebarPanel { title: crate::shell::i18n::text("_shell.comment"), body: body.into_any_element() })
    }
}
