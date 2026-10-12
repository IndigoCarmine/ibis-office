// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スピーカー ノート: the notes pane under the slide (表示 > スピーカー ノート
//! を表示), a plain text field for the current slide's notes. What is typed
//! is put in the document as it is typed (`Command::SetNotes`; one undo
//! step while the field keeps the focus).

use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Command, SlideId};

use crate::shell::editor::SlideEditor;

/// The notes field and the slide it shows.
pub(crate) struct NotesField {
    pub(crate) slide: SlideId,
    /// The UI language its prompt is in (a new field when it changes).
    language: crate::shell::i18n::Language,
    pub(crate) state: Entity<TextareaState>,
    /// The last command is this field's typing (more joins it).
    pub(crate) typing: bool,
    _subscription: Subscription,
}

/// The pane's height, pixels.
const PANE_HEIGHT: f32 = 110.0;

impl SlideEditor {
    /// Whether the notes pane shows (it does unless hidden from the menu).
    pub(crate) fn notes_shown(&self) -> bool {
        self.pptx().is_some_and(|s| !s.notes_hidden)
    }

    pub(crate) fn toggle_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_notes(window, cx);
        if let Some(state) = self.pptx_mut() {
            state.notes_hidden = !state.notes_hidden;
        }
        cx.notify();
    }

    /// Put what the field holds in the document, if it changed.
    pub(crate) fn commit_notes(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some((slide, text)) = self.pptx().and_then(|s| s.notes_field.as_ref()).map(|f| (f.slide, f.state.read(cx).value().to_string())) else { return };
        let Some(old) = self.presentation().slide(slide).map(|s| s.notes.clone()) else { return };
        // The field's own line ends are `\n`; a pasted `\r\n` is one too.
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text != old {
            let typing = self.pptx().and_then(|s| s.notes_field.as_ref()).is_some_and(|f| f.typing);
            let command = vec![Command::SetNotes { slide, notes: text }];
            let result = if typing { self.document.amend_last(command) } else { self.document.execute(command) };
            if let Err(err) = result {
                log::warn!("notes rejected: {err}");
            }
            if let Some(f) = self.pptx_mut().and_then(|s| s.notes_field.as_mut()) {
                f.typing = true;
            }
            cx.notify();
        }
    }

    /// The notes field is no longer being typed in (it lost the focus, or
    /// another command ran): the next typing is a new undo step.
    pub(crate) fn end_notes_typing(&mut self) {
        if let Some(f) = self.pptx_mut().and_then(|s| s.notes_field.as_mut()) {
            f.typing = false;
        }
    }

    /// The field for the current slide (made, or moved to it), showing its
    /// notes as they are in the document unless they are being typed.
    fn notes_field(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<TextareaState>> {
        let current = self.current_slide;
        let notes = self.presentation().slide(current)?.notes.clone();
        let language = crate::shell::i18n::language();
        let field = self.pptx().and_then(|s| s.notes_field.as_ref()).map(|f| (f.slide, f.language, f.state.clone()));
        match field {
            Some((slide, shown, state)) if slide == current && shown == language => {
                // Undo and redo change the notes under the field.
                let focused = state.read(cx).focus_handle(cx).is_focused(window);
                if !focused && state.read(cx).value().as_ref() != notes {
                    state.update(cx, |s, cx| s.set_value(notes, window, cx));
                }
                Some(state)
            }
            _ => {
                if field.is_some() {
                    self.commit_notes(window, cx);
                }
                let state = cx.new(|cx| TextareaState::new(window, cx).placeholder(crate::shell::i18n::text("pptx.notes.click-add-notes")).default_value(notes));
                let subscription = cx.subscribe_in(&state, window, |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => this.commit_notes(window, cx),
                    InputEvent::Blur => {
                        this.commit_notes(window, cx);
                        this.end_notes_typing();
                    }
                    _ => {}
                });
                if let Some(s) = self.pptx_mut() {
                    s.notes_field = Some(NotesField { slide: current, language, state: state.clone(), typing: false, _subscription: subscription });
                }
                Some(state)
            }
        }
    }

    /// The notes pane, when it shows.
    pub(crate) fn render_notes_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.notes_shown() || !self.pages_are_slides() || !self.has_document {
            return None;
        }
        let state = self.notes_field(window, cx)?;
        let theme = cx.theme();
        Some(
            div()
                .id("speaker-notes")
                .h(px(PANE_HEIGHT))
                .flex_shrink_0()
                .border_t_1()
                .border_color(theme.border)
                .bg(theme.background)
                .px_3()
                .py_2()
                .text_sm()
                .child(Textarea::new(&state).appearance(false).size_full())
                .into_any_element(),
        )
    }

    /// Notes Page View (ノート表示): A full-page layout showing the slide at the top
    /// and its speaker notes editing area at the bottom, mimicking PowerPoint's Notes Page view.
    pub(crate) fn render_notes_page_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let slide = self.slide().clone();
        let size = self.presentation().size_of(&slide);
        let preview_w = 460.0f32;
        let scale = (preview_w / size.width.max(1) as f32).min(300.0 / size.height.max(1) as f32);
        let (width, height) = (size.width as f32 * scale, size.height as f32 * scale);

        let ready = self.resources.preload.slide_ready(&slide);
        let shapes: Vec<AnyElement> = if ready {
            slide
                .shown_shapes()
                .map(|shape| crate::render::shape_view::shape_element(shape, shape.frame, scale, &self.resources, None))
                .collect()
        } else {
            Vec::new()
        };

        // The notes page's own shapes (a picture or a shape put on it), at
        // the scale the page (520 px wide) has to the notes page.
        let notes_size = self.presentation().notes_size;
        let page_scale = 520.0 / notes_size.width.max(1) as f32;
        let page_height = notes_size.height as f32 * page_scale;
        let notes_shapes: Vec<AnyElement> = if ready {
            slide.notes_shapes.iter().map(|shape| crate::render::shape_view::shape_element(shape, shape.frame, page_scale, &self.resources, None)).collect()
        } else {
            Vec::new()
        };

        // The header, date, footer and number the notes pages carry, where the notes master puts them.
        let marks = self.presentation().notes_marks.clone();
        let number = self.presentation().slide_index(self.current_slide).unwrap_or(0) + self.presentation().first_slide_number;
        let mark_text = |kind: &str| -> Option<String> {
            match kind {
                "hdr" => marks.header.clone(),
                "dt" => marks.date.as_ref().map(|d| match d {
                    ibis_doc_model::NotesDate::Auto(t) | ibis_doc_model::NotesDate::Fixed(t) => t.clone(),
                }),
                "ftr" => marks.footer.clone(),
                _ => marks.number.then(|| number.to_string()),
            }
        };
        let mark_elements: Vec<AnyElement> = self
            .presentation()
            .notes_frames
            .iter()
            .filter_map(|(kind, frame)| {
                let text = mark_text(kind).filter(|t| !t.is_empty())?;
                let right = matches!(kind.as_str(), "dt" | "sldNum");
                let bottom = matches!(kind.as_str(), "ftr" | "sldNum");
                let mut d = div()
                    .absolute()
                    .left(px(frame.x as f32 * page_scale))
                    .top(px(frame.y as f32 * page_scale))
                    .w(px(frame.width as f32 * page_scale))
                    .h(px(frame.height as f32 * page_scale))
                    .flex()
                    .px_1()
                    .text_size(px(12.0 * 12_700.0 * page_scale))
                    .text_color(black());
                d = if bottom { d.items_end() } else { d.items_start() };
                d = if right { d.justify_end() } else { d.justify_start() };
                Some(d.child(text).into_any_element())
            })
            .collect();

        let state = self.notes_field(window, cx);
        let theme = cx.theme();

        let slide_index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let total_slides = self.presentation().slides.len();

        let header = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .px_6()
            .py_2()
            .bg(theme.sidebar)
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(crate::shell::i18n::text("pptx.notes-page-view")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(crate::shell::i18n::format("_pptx.slide-n-of-total", &[("n", &(slide_index + 1)), ("total", &total_slides)])),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .id("notes-format-edit")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(theme.border)
                            .text_xs()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.accent))
                            .text_color(theme.foreground)
                            .on_click(cx.listener(|this, _, window, cx| this.open_notes_format(window, cx)))
                            .child(crate::shell::i18n::text("pptx.notes.edit-notes-formatting")),
                    )
                    .child(
                div()
                    .id("exit-notes-view")
                    .px_2p5()
                    .py_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.accent))
                    .text_color(theme.foreground)
                    .on_click(cx.listener(|this, _, window, cx| this.switch_to_normal_view(window, cx)))
                    .child(crate::shell::i18n::text("_pptx.return-to-normal-view")),
                    ),
            );

        let page = v_flex()
            .relative()
            .w(px(520.0))
            .h(px(page_height))
            .bg(white())
            .rounded_md()
            .shadow_lg()
            .border_1()
            .border_color(theme.border)
            .p_6()
            .gap_4()
            .items_center()
            // 1. スライド縮小プレビュー
            .child(
                div()
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .shadow_sm()
                    .bg(white())
                    .child(crate::render::shape_view::slide_backdrop(&slide, scale, &self.resources))
                    .children(shapes),
            )
            // 2. ノート入力・表示領域
            .child(
                v_flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .border_1()
                    .border_color(theme.border)
                    .rounded_sm()
                    .p_3()
                    .bg(theme.background)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .pb_1()
                            .child(crate::shell::i18n::text("_pptx.speaker-notes")),
                    )
                    .when_some(state, |d, st| {
                        d.child(Textarea::new(&st).appearance(false).size_full())
                    }),
            )
            // 3. ノート ページ上の図・図形
            .children(notes_shapes)
            .children(mark_elements);

        v_flex()
            .id("notes-page-workspace")
            .key_context(crate::shell::editor::CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &crate::CancelTool, window, cx| this.switch_to_normal_view(window, cx)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.switch_to_normal_view(window, cx);
                }
            }))
            .size_full()
            .bg(theme.muted)
            .child(header)
            .child(
                v_flex()
                    .id("notes-page-scroll")
                    .flex_1()
                    .size_full()
                    .overflow_y_scroll()
                    .items_center()
                    .py_8()
                    .child(page),
            )
            .into_any_element()
    }
}
