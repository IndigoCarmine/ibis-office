// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 代替テキスト (Alt Text): the selected shapes' title and description for accessibility (`cNvPr/@descr`, `@title`).
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{h_flex, v_flex, Selectable as _, Sizable as _};
use gpui_kit::*;
use ibis_doc_model::{AltText, Command, ShapeId, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct AltTextPanel {
    pub(crate) title_field: Entity<InputState>,
    pub(crate) desc_field: Entity<InputState>,
    pub(crate) target: (SlideId, ShapeId),
    pub(crate) initial: AltText,
    pub(crate) decorative: bool,
    pub(crate) initial_decorative: bool,
}

impl SlideEditor {
    pub(crate) fn open_alt_text_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let slide = self.current_slide;
        let Some(shape_id) = self.selected_shapes().first().map(|s| s.id) else { return };
        let current_alt = self.slide().shape_alt_texts.get(&shape_id).cloned().unwrap_or_default();
        let current_decorative = self.slide().decorative_shapes.contains(&shape_id);
        let initial = current_alt.clone();
        let title_field = cx.new(|cx| InputState::new(window, cx).default_value(current_alt.title));
        let desc_field = cx.new(|cx| InputState::new(window, cx).default_value(current_alt.description));
        if let Some(s) = self.pptx_mut() {
            s.timing_panel = None;
            s.section_panel = None;
            s.gradient_panel = None;
            s.shadow_panel = None;
            s.glow_panel = None;
            s.paragraph_panel = None;
            s.find = None;
            s.size_panel = None;
            s.insets_panel = None;
            s.crop_panel = None;
            s.comments = None;
            s.chart_panel = None;
            s.header_footer = None;
            s.frame_panel = None;
            s.alt_text_panel = Some(AltTextPanel {
                title_field,
                desc_field: desc_field.clone(),
                target: (slide, shape_id),
                initial,
                decorative: current_decorative,
                initial_decorative: current_decorative,
            });
        }
        self.sidebar_open = true;
        desc_field.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn toggle_alt_text_decorative(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.pptx_mut().and_then(|s| s.alt_text_panel.as_mut()) {
            panel.decorative = !panel.decorative;
            cx.notify();
        }
    }

    pub(crate) fn apply_alt_text(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.alt_text_panel.as_ref()) else { return };
        let (slide, shape) = panel.target;
        let title = panel.title_field.read(cx).value().trim().to_string();
        let description = panel.desc_field.read(cx).value().trim().to_string();
        let new_alt = AltText { title, description };
        let new_decorative = panel.decorative;

        let mut commands = Vec::new();
        if new_decorative != panel.initial_decorative {
            commands.push(Command::SetShapeDecorative { slide, shape, decorative: new_decorative });
        }
        if new_alt != panel.initial {
            commands.push(Command::SetShapeAltText { slide, shape, alt_text: new_alt.clone() });
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
        if let Some(panel) = self.pptx_mut().and_then(|s| s.alt_text_panel.as_mut()) {
            panel.initial = new_alt;
            panel.initial_decorative = new_decorative;
        }
    }

    pub(crate) fn close_alt_text_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_alt_text(cx);
        if let Some(s) = self.pptx_mut() {
            s.alt_text_panel = None;
        }
        self.sidebar_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn alt_text_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.pptx()?.alt_text_panel.as_ref()?;
        let t = crate::shell::i18n::text;
        let title_field = panel.title_field.clone();
        let desc_field = panel.desc_field.clone();
        let decorative = panel.decorative;

        let mut body = v_flex()
            .gap_3()
            .p_3()
            .child(
                Button::new("alt-text-decorative")
                    .small()
                    .label(t("pptx.alt-text.decorative"))
                    .selected(decorative)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_alt_text_decorative(cx);
                    })),
            );

        if decorative {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(rgb(0x5f6368))
                    .child(t("pptx.alt-text.decorative.hint")),
            );
        } else {
            body = body
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x5f6368))
                        .child(t("pptx.alt-text.hint")),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(t("pptx.alt-text.title")))
                        .child(Input::new(&title_field)),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(t("pptx.alt-text.description")))
                        .child(Input::new(&desc_field)),
                );
        }

        body = body.child(
            h_flex()
                .gap_2()
                .justify_end()
                .child(
                    Button::new("alt-text-apply")
                        .primary()
                        .label(t("pptx.alt-text.apply"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_alt_text_panel(window, cx);
                        })),
                )
                .child(
                    Button::new("alt-text-close")
                        .ghost()
                        .label(t("_shell.close"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_alt_text_panel(window, cx);
                        })),
                ),
        );

        Some(SidebarPanel {
            title: t("pptx.alt-text"),
            body: body.into_any_element(),
        })
    }
}
