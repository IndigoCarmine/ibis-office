// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Presentation show properties panel (スライド ショーの設定).
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, ShowProperties, ShowSlideRange, ShowType};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShowRangeKind {
    All,
    Range,
    Custom,
}

pub(crate) struct ShowPanel {
    pub(crate) show_type: ShowType,
    pub(crate) loop_continuously: bool,
    pub(crate) show_narration: bool,
    pub(crate) show_animation: bool,
    pub(crate) use_timings: bool,
    pub(crate) all_slides: bool,
    pub(crate) range_kind: ShowRangeKind,
    pub(crate) selected_custom_show: Option<u32>,
    pub(crate) from: Entity<InputState>,
    pub(crate) to: Entity<InputState>,
    pub(crate) error: Option<SharedString>,
}

impl SlideEditor {
    pub(crate) fn open_show_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let p = &self.presentation().show_properties;
        let show_type = p.show_type;
        let loop_continuously = p.loop_continuously;
        let show_narration = p.show_narration;
        let show_animation = p.show_animation;
        let use_timings = p.use_timings;
        let (range_kind, selected_custom_show, start_val, end_val) = match p.slide_range {
            ShowSlideRange::All => (ShowRangeKind::All, self.presentation().custom_shows.first().map(|s| s.id), 1, self.presentation().slides.len().max(1)),
            ShowSlideRange::Range { start, end } => (ShowRangeKind::Range, self.presentation().custom_shows.first().map(|s| s.id), start, end),
            ShowSlideRange::Custom(id) => (ShowRangeKind::Custom, Some(id), 1, self.presentation().slides.len().max(1)),
        };

        let from = cx.new(|cx| InputState::new(window, cx).default_value(start_val.to_string()));
        let to = cx.new(|cx| InputState::new(window, cx).default_value(end_val.to_string()));

        if let Some(s) = self.pptx_mut() {
            s.section_panel = None;
            s.gradient_panel = None;
            s.shadow_panel = None;
            s.glow_panel = None;
            s.paragraph_panel = None;
            s.timing_panel = None;
            s.find = None;
            s.size_panel = None;
            s.frame_panel = None;
            s.comments = None;
            s.chart_panel = None;
            s.header_footer = None;
            s.insets_panel = None;
            s.crop_panel = None;
            s.show_panel = Some(ShowPanel {
                show_type,
                loop_continuously,
                show_narration,
                show_animation,
                use_timings,
                all_slides: range_kind == ShowRangeKind::All,
                range_kind,
                selected_custom_show,
                from: from.clone(),
                to,
                error: None,
            });
        }
        self.sidebar_open = true;
        from.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn set_show_panel_type(&mut self, kind: ShowType, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
            p.show_type = kind;
        }
        cx.notify();
    }

    pub(crate) fn toggle_show_panel_option(&mut self, option: &str, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
            match option {
                "loop" => p.loop_continuously = !p.loop_continuously,
                "narration" => p.show_narration = !p.show_narration,
                "animation" => p.show_animation = !p.show_animation,
                "timings" => p.use_timings = !p.use_timings,
                _ => {}
            }
        }
        cx.notify();
    }

    pub(crate) fn apply_show_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.show_panel.as_ref()) else { return };
        let show_type = p.show_type;
        let loop_continuously = p.loop_continuously;
        let show_narration = p.show_narration;
        let show_animation = p.show_animation;
        let use_timings = p.use_timings;
        let pen_color = self.presentation().show_properties.pen_color;

        let range_kind = if !p.all_slides && p.range_kind == ShowRangeKind::All {
            ShowRangeKind::Range
        } else {
            p.range_kind
        };
        let slide_range = match range_kind {
            ShowRangeKind::All => ShowSlideRange::All,
            ShowRangeKind::Custom => {
                if let Some(id) = p.selected_custom_show {
                    ShowSlideRange::Custom(id)
                } else if let Some(first) = self.presentation().custom_shows.first() {
                    ShowSlideRange::Custom(first.id)
                } else {
                    ShowSlideRange::All
                }
            }
            ShowRangeKind::Range => {
                let from_str = p.from.read(cx).value();
                let to_str = p.to.read(cx).value();
                let start = from_str.trim().parse::<usize>().ok();
                let end = to_str.trim().parse::<usize>().ok();
                match (start, end) {
                    (Some(s), Some(e)) if s >= 1 && e >= 1 => {
                        let total = self.presentation().slides.len().max(1);
                        ShowSlideRange::Range {
                            start: s.min(total),
                            end: e.min(total),
                        }
                    }
                    _ => {
                        if let Some(p) = self.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
                            p.error = Some(crate::shell::i18n::text("pptx.show-panel.please-enter-valid-slide-number").into());
                        }
                        return cx.notify();
                    }
                }
            }
        };

        let new_props = ShowProperties {
            show_type,
            loop_continuously,
            show_narration,
            show_animation,
            use_timings,
            slide_range,
            pen_color,
        };

        if self.presentation().show_properties != new_props {
            self.run(vec![Command::SetShowProperties { properties: new_props }], cx);
        }
        self.close_show_panel(window, cx);
    }

    pub(crate) fn close_show_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.show_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn show_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.show_panel.as_ref()?;
        let title = crate::shell::i18n::text("pptx.show-panel.set-up-slide-show");
        let (show_type, loop_cont, narration, animation, timings, range_kind, selected_custom_show, error) = (
            p.show_type,
            p.loop_continuously,
            p.show_narration,
            p.show_animation,
            p.use_timings,
            p.range_kind,
            p.selected_custom_show,
            p.error.clone(),
        );
        let from = p.from.clone();
        let to = p.to.clone();

        let is_presenter = matches!(show_type, ShowType::Presenter);
        let is_browse = matches!(show_type, ShowType::Browse);
        let is_kiosk = matches!(show_type, ShowType::Kiosk { .. });

        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.))
            // 種類
            .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("pptx.show.type")))
            .child(Button::new("pptx-show-type-presenter").small().ghost().label(if is_presenter { crate::shell::i18n::text("pptx.show-panel.presented-speaker-full-screen") } else { crate::shell::i18n::text("pptx.show-panel.presented-speaker-full-screen-2") }).on_click(cx.listener(|e, _, _, cx| e.set_show_panel_type(ShowType::Presenter, cx))))
            .child(Button::new("pptx-show-type-browse").small().ghost().label(if is_browse { crate::shell::i18n::text("pptx.show-panel.browsed-individual-window") } else { crate::shell::i18n::text("pptx.show-panel.browsed-individual-window-2") }).on_click(cx.listener(|e, _, _, cx| e.set_show_panel_type(ShowType::Browse, cx))))
            .child(Button::new("pptx-show-type-kiosk").small().ghost().label(if is_kiosk { crate::shell::i18n::text("pptx.show-panel.browsed-kiosk-full-screen") } else { crate::shell::i18n::text("pptx.show-panel.browsed-kiosk-full-screen-2") }).on_click(cx.listener(|e, _, _, cx| e.set_show_panel_type(ShowType::Kiosk { restart: None }, cx))))
            // スライドの表示
            .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("pptx.show-panel.show-slides")))
            .child(Button::new("pptx-show-range-all").small().ghost().label(if range_kind == ShowRangeKind::All { crate::shell::i18n::text("pptx.show-panel.all") } else { crate::shell::i18n::text("pptx.show-panel.all-2") }).on_click(cx.listener(|e, _, _, cx| {
                if let Some(p) = e.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
                    p.all_slides = true;
                    p.range_kind = ShowRangeKind::All;
                }
                cx.notify();
            })))
            .child(Button::new("pptx-show-range-custom").small().ghost().label(if range_kind == ShowRangeKind::Range { crate::shell::i18n::text("pptx.show-panel.from") } else { crate::shell::i18n::text("pptx.show-panel.from-2") }).on_click(cx.listener(|e, _, _, cx| {
                if let Some(p) = e.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
                    p.all_slides = false;
                    p.range_kind = ShowRangeKind::Range;
                }
                cx.notify();
            })))
            .child(
                h_flex().gap_2().items_center()
                    .child(div().text_size(px(12.)).child(crate::shell::i18n::text("pptx.animation-ui.from")))
                    .child(div().w(px(60.)).child(Input::new(&from).small()))
                    .child(div().text_size(px(12.)).child(crate::shell::i18n::text("pptx.show-panel.to")))
                    .child(div().w(px(60.)).child(Input::new(&to).small()))
            )
            .child(Button::new("pptx-show-range-custom-show").small().ghost().label(if range_kind == ShowRangeKind::Custom { crate::shell::i18n::text("_pptx.custom-show") } else { crate::shell::i18n::text("_pptx.custom-show") }).on_click(cx.listener(|e, _, _, cx| {
                let first_id = e.presentation().custom_shows.first().map(|s| s.id);
                if let Some(p) = e.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
                    p.all_slides = false;
                    p.range_kind = ShowRangeKind::Custom;
                    if p.selected_custom_show.is_none() {
                        p.selected_custom_show = first_id;
                    }
                }
                cx.notify();
            })));

        if range_kind == ShowRangeKind::Custom {
            let custom_shows = self.presentation().custom_shows.clone();
            if custom_shows.is_empty() {
                body = body.child(div().px(px(8.)).text_size(px(12.)).text_color(rgb(0x888888)).child(crate::shell::i18n::text("_pptx.no-custom-shows-have-been-defined")));
            } else {
                for cs in custom_shows {
                    let cs_id = cs.id;
                    let selected = selected_custom_show == Some(cs_id);
                    let label = format!("{} {}", if selected { "✓" } else { " " }, cs.name);
                    body = body.child(
                        Button::new(ElementId::Name(format!("pptx-custom-show-select-{}", cs_id).into()))
                            .small().ghost().label(label)
                            .on_click(cx.listener(move |e, _, _, cx| {
                                if let Some(p) = e.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
                                    p.range_kind = ShowRangeKind::Custom;
                                    p.selected_custom_show = Some(cs_id);
                                }
                                cx.notify();
                            }))
                    );
                }
            }
        }

        body = body
            // オプション
            .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("pptx.show-panel.show-options")))
            .child(Button::new("pptx-show-opt-loop").small().ghost().label(if loop_cont { crate::shell::i18n::text("pptx.show-panel.loop-continuously-until-esc") } else { crate::shell::i18n::text("pptx.show-panel.loop-continuously-until-esc-2") }).on_click(cx.listener(|e, _, _, cx| e.toggle_show_panel_option("loop", cx))))
            .child(Button::new("pptx-show-opt-anim").small().ghost().label(if !animation { crate::shell::i18n::text("pptx.show-panel.show-without-animation") } else { crate::shell::i18n::text("pptx.show-panel.show-without-animation-2") }).on_click(cx.listener(|e, _, _, cx| e.toggle_show_panel_option("animation", cx))))
            .child(Button::new("pptx-show-opt-narration").small().ghost().label(if !narration { crate::shell::i18n::text("pptx.show-panel.show-without-narration") } else { crate::shell::i18n::text("pptx.show-panel.show-without-narration-2") }).on_click(cx.listener(|e, _, _, cx| e.toggle_show_panel_option("narration", cx))))
            .child(Button::new("pptx-show-opt-timings").small().ghost().label(if timings { crate::shell::i18n::text("pptx.show-panel.use-timings") } else { crate::shell::i18n::text("pptx.show-panel.use-timings-2") }).on_click(cx.listener(|e, _, _, cx| e.toggle_show_panel_option("timings", cx))));

        if let Some(err) = error {
            body = body.child(div().text_size(px(12.)).text_color(rgb(0xb3261e)).child(err));
        }

        body = body.child(
            h_flex().gap_1()
                .child(Button::new("pptx-show-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|e, _, w, cx| e.apply_show_panel(w, cx))))
                .child(Button::new("pptx-show-cancel").small().ghost().label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(|e, _, w, cx| e.close_show_panel(w, cx))))
        );

        Some(SidebarPanel {
            title: title.into(),
            body: body.into_any_element(),
        })
    }
}
