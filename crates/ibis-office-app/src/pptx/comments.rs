// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コメント: the sidebar panel listing the slide's comments, where a
//! comment is put on the slide or the selected shape (挿入 > コメント,
//! ⌥⌘M), answered, resolved and removed; and their marks on the slide
//! (the editor only, not the slide show, printing or exports).

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Author, Command, CommentReply, EmuPoint, ShapeId, SlideComment};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// What the panel's field writes: a new comment (on a shape, or the
/// slide), or an answer to the comment at an index of the slide's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentTarget {
    New(Option<ShapeId>),
    Reply(usize),
}

/// The panel's field and what it is for.
pub(crate) struct CommentsPanel {
    pub(crate) input: Entity<InputState>,
    pub(crate) target: Option<CommentTarget>,
    _subscription: Subscription,
}

/// Who writes and now, as the panel puts them on a comment (the time as
/// PowerPoint writes it, without a zone letter).
fn author_and_time() -> (Author, String) {
    let name = crate::shell::user_name::user_name().to_string();
    let date = crate::docx::comments::comment_author().date;
    (Author { initials: crate::shell::user_name::initials(&name), name }, date.trim_end_matches('Z').to_string())
}

/// A new GUID for a comment or an answer, from the time and a count.
fn new_guid() -> String {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seed = (crate::clock::unix_millis() as u64) ^ COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let mix = |mut x: u64| {
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        x ^ (x >> 33)
    };
    let (a, b) = (mix(seed), mix(seed ^ 0x5555_5555_5555_5555));
    format!("{{{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}}}", a >> 32, (a >> 16) & 0xffff, a & 0xfff, 0x8000 | (b >> 48) & 0x3fff, b & 0xffff_ffff_ffff)
}

/// Where comment `c`'s mark is on `slide`: its place, else the top right
/// corner of its shape, else the slide's top left.
pub(crate) fn mark_place(slide: &ibis_doc_model::Slide, c: &SlideComment) -> EmuPoint {
    c.position.or_else(|| c.shape.and_then(|id| slide.shape(id)).map(|s| EmuPoint::new(s.frame.right(), s.frame.y))).unwrap_or(EmuPoint::new(0, 0))
}

impl SlideEditor {
    /// 挿入 > コメント: the panel, its field for a comment on the selected
    /// shape (or the slide).
    pub(crate) fn open_slide_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let shape = self.selected_shapes().first().map(|s| s.id);
        if self.pptx().is_some_and(|s| s.comments.is_none()) {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_shell.type-a-comment-and-press-enter")));
            let subscription = cx.subscribe_in(&input, window, |this, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().to_string();
                    this.send_slide_comment(&text, window, cx);
                }
            });
            if let Some(state) = self.pptx_mut() {
                state.comments = Some(CommentsPanel { input, target: None, _subscription: subscription });
            }
        }
        if let Some(s) = self.pptx_mut() {
            s.timing_panel = None; s.section_panel = None; s.gradient_panel = None; s.shadow_panel = None; s.glow_panel = None; s.paragraph_panel = None; s.find = None; s.size_panel = None; s.insets_panel = None; s.crop_panel = None; s.frame_panel = None; s.chart_panel = None; s.header_footer = None;
            if let Some(c) = s.comments.as_mut() {
                c.target = Some(CommentTarget::New(shape));
            }
        }
        if let Some(input) = self.pptx().and_then(|s| s.comments.as_ref()).map(|c| c.input.clone()) {
            input.update(cx, |i, cx| i.focus(window, cx));
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// コメントを表示 off: the panel closed.
    pub(crate) fn close_slide_comments(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.comments = None;
        }
        cx.notify();
    }

    /// The slide's comments changed by `change`, as one step to undo.
    fn change_comments(&mut self, change: impl FnOnce(&mut Vec<SlideComment>), cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut comments = self.slide().comments.clone();
        change(&mut comments);
        if comments != self.slide().comments {
            self.run(vec![Command::SetComments { slide, comments }], cx);
        }
    }

    /// The field's text written as the comment or answer it is for.
    pub(crate) fn send_slide_comment(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        let Some(target) = self.pptx().and_then(|s| s.comments.as_ref()).and_then(|c| c.target) else { return };
        if text.is_empty() {
            return;
        }
        let (author, created) = author_and_time();
        match target {
            CommentTarget::New(shape) => {
                // On a shape its mark follows the shape; else the slide's top left.
                let position = shape.is_none().then(|| EmuPoint::new(0, 0));
                let comment = SlideComment { id: new_guid(), author, created, text, position, shape, resolved: false, replies: Vec::new() };
                self.change_comments(|c| c.push(comment), cx);
            }
            CommentTarget::Reply(index) => {
                let reply = CommentReply { id: new_guid(), author, created, text };
                self.change_comments(|c| { if let Some(c) = c.get_mut(index) { c.replies.push(reply) } }, cx);
            }
        }
        if let Some(c) = self.pptx_mut().and_then(|s| s.comments.as_mut()) {
            c.target = None;
        }
        if let Some(input) = self.pptx().and_then(|s| s.comments.as_ref()).map(|c| c.input.clone()) {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
        cx.notify();
    }

    /// 返信: the field for an answer to comment `index`.
    pub(crate) fn reply_to_slide_comment(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self.pptx_mut().and_then(|s| s.comments.as_mut()) {
            c.target = Some(CommentTarget::Reply(index));
        }
        if let Some(input) = self.pptx().and_then(|s| s.comments.as_ref()).map(|c| c.input.clone()) {
            input.update(cx, |i, cx| i.focus(window, cx));
        }
        cx.notify();
    }

    /// 解決 (or 再開).
    pub(crate) fn resolve_slide_comment(&mut self, index: usize, done: bool, cx: &mut Context<Self>) {
        self.change_comments(|c| { if let Some(c) = c.get_mut(index) { c.resolved = done } }, cx);
    }

    /// 削除: the comment and its answers.
    pub(crate) fn delete_slide_comment(&mut self, index: usize, cx: &mut Context<Self>) {
        self.change_comments(|c| { if index < c.len() { c.remove(index); } }, cx);
    }

    pub(crate) fn slide_comments_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.pptx()?.comments.as_ref()?;
        let (input, target) = (state.input.clone(), state.target);
        let comments = self.slide().comments.clone();
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.));
        let prompt = match target {
            Some(CommentTarget::New(Some(_))) => "pptx.comments.comment-selected-shape",
            Some(CommentTarget::New(None)) => "pptx.comments.comment-slide",
            Some(CommentTarget::Reply(_)) => "_shell.reply",
            None => "pptx.comments.new-comment-insert-comment",
        };
        body = body.child(div().text_sm().child(crate::shell::i18n::text(prompt)));
        if target.is_some() {
            body = body.child(Input::new(&input).small()).child(Button::new("pptx-comment-send").small().label(crate::shell::i18n::text("_shell.comment")).on_click(cx.listener(move |this, _, window, cx| {
                let text = input.read(cx).value().to_string();
                this.send_slide_comment(&text, window, cx);
            })));
        }
        if comments.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("pptx.comments.there-no-comments-slide")));
        }
        for (index, c) in comments.iter().enumerate() {
            let done = c.resolved;
            let mut card = v_flex()
                .gap_1()
                .p(px(8.))
                .rounded(px(8.))
                .bg(if done { rgb(0xf1f3f4) } else { rgb(0xfef7e0) })
                .child(div().text_xs().text_color(rgb(0x5f6368)).child(format!("{}{}", c.author.name, if done { crate::shell::i18n::text("_shell.resolved") } else { "".into() })))
                .child(div().text_sm().child(c.text.clone()));
            for r in &c.replies {
                card = card.child(div().text_sm().pl(px(8.)).child(format!("{}：{}", r.author.name, r.text))); // i18n-ok（区切りの記号）
            }
            card = card.child(
                h_flex()
                    .gap_1()
                    .child(Button::new(("pptx-comment-reply", index)).small().ghost().label(crate::shell::i18n::text("_shell.reply")).on_click(cx.listener(move |this, _, window, cx| this.reply_to_slide_comment(index, window, cx))))
                    .child(Button::new(("pptx-comment-done", index)).small().ghost().label(crate::shell::i18n::text(if done { "pptx.comments.reopen" } else { "_shell.resolve" })).on_click(cx.listener(move |this, _, _, cx| this.resolve_slide_comment(index, !done, cx))))
                    .child(Button::new(("pptx-comment-delete", index)).small().ghost().label(crate::shell::i18n::text("_shell.delete")).on_click(cx.listener(move |this, _, _, cx| this.delete_slide_comment(index, cx)))),
            );
            body = body.child(card);
        }
        Some(SidebarPanel { title: crate::shell::i18n::text("_shell.comment"), body: body.into_any_element() })
    }
}
