// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Arbitrary transition duration and automatic advancement in seconds.
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, SlideId, Transition};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct TimingPanel {
    pub(crate) duration: Entity<InputState>,
    pub(crate) after: Entity<InputState>,
    pub(crate) automatic: bool,
    pub(crate) on_click: bool,
    slide: SlideId,
    snapshot: Option<Transition>,
    error: Option<SharedString>,
}

fn milliseconds(value: &str,max:u32) -> Option<u32> {
    let seconds: f64 = value.trim().parse().ok()?;
    let ms = (seconds * 1000.).round();
    (seconds.is_finite() && seconds >= 0. && ms <= max as f64).then_some(ms as u32)
}

impl SlideEditor {
    pub(crate) fn open_timing_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let snapshot=self.slide().transition.clone();
        let t = snapshot.clone().unwrap_or_else(|| Transition::new(""));
        let slide = self.current_slide;
        let duration = cx.new(|cx| InputState::new(window, cx).default_value(format!("{:.3}", t.duration as f64 / 1000.)));
        let after = cx.new(|cx| InputState::new(window, cx).default_value(format!("{:.3}", t.after.unwrap_or(0) as f64 / 1000.)));
        if let Some(s) = self.pptx_mut() {
            s.section_panel=None;s.gradient_panel = None; s.shadow_panel = None; s.glow_panel = None; s.paragraph_panel = None; s.find = None; s.size_panel = None; s.frame_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None; s.insets_panel = None; s.crop_panel = None;
            s.timing_panel = Some(TimingPanel { duration: duration.clone(), after, automatic: t.after.is_some(), on_click: t.on_click, slide, snapshot, error: None });
        }
        self.sidebar_open = true;
        duration.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn toggle_timing_panel(&mut self, automatic: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.timing_panel.as_mut()) {
            if automatic { p.automatic = !p.automatic; } else { p.on_click = !p.on_click; }
        }
        cx.notify();
    }

    pub(crate) fn apply_timing_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.timing_panel.as_ref()) else { return };
        let duration = milliseconds(p.duration.read(cx).value().as_ref(),u32::MAX);
        let after = if p.automatic { milliseconds(p.after.read(cx).value().as_ref(),i32::MAX as u32).map(Some) } else { Some(None) };
        let (Some(duration), Some(after)) = (duration, after) else {
            if let Some(p) = self.pptx_mut().and_then(|s| s.timing_panel.as_mut()) { p.error = Some(crate::shell::i18n::text("pptx.timing-panel.enter-finite-numbers-duration-0").into()); }
            return cx.notify();
        };
        if self.presentation().slide(p.slide).map(|s|&s.transition)!=Some(&p.snapshot) {
            if let Some(p)=self.pptx_mut().and_then(|s|s.timing_panel.as_mut()) {p.error=Some(crate::shell::i18n::text("pptx.timing-panel.target-slide-transition-settings").into());}
            return cx.notify();
        }
        let (slide, on_click) = (p.slide, p.on_click);
        if let Some(target) = self.presentation().slide(slide) {
            let mut t = target.transition.clone().unwrap_or_else(|| Transition::new(""));
            t.duration = duration; t.after = after; t.on_click = on_click;
            if target.transition.as_ref() != Some(&t) { self.run(vec![Command::SetTransition { slide, transition: Some(t) }], cx); }
        }
        self.close_timing_panel(window, cx);
    }

    pub(crate) fn close_timing_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() { s.timing_panel = None; }
        self.focus_handle.focus(window, cx); cx.notify();
    }

    pub(crate) fn timing_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.timing_panel.as_ref()?;
        let title=self.presentation().slide_index(p.slide).map(|i|crate::shell::i18n::format("pptx.timing-panel.transition-timing-slide-index", &[("index", &(i+1))])).unwrap_or_else(||crate::shell::i18n::text("pptx.timing-panel.timing-deleted-slide").to_string());
        let (duration, after, automatic, click, error) = (p.duration.clone(), p.after.clone(), p.automatic, p.on_click, p.error.clone());
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .child(div().text_size(px(13.)).child(crate::shell::i18n::text("pptx.timing-panel.transition-duration-s"))).child(Input::new(&duration).small())
            .child(Button::new("pptx-timing-click").small().ghost().label(if click { crate::shell::i18n::text("pptx.timing-panel.mouse-click") } else { crate::shell::i18n::text("pptx.timing-panel.mouse-click-2") }).on_click(cx.listener(|e, _, _, cx| e.toggle_timing_panel(false, cx))))
            .child(Button::new("pptx-timing-auto").small().ghost().label(if automatic { crate::shell::i18n::text("pptx.timing-panel.after") } else { crate::shell::i18n::text("pptx.timing-panel.after-2") }).on_click(cx.listener(|e, _, _, cx| e.toggle_timing_panel(true, cx))))
            .child(div().text_size(px(13.)).child(crate::shell::i18n::text("pptx.timing-panel.advance-after-s"))).child(Input::new(&after).small());
        if let Some(error) = error { body = body.child(div().text_size(px(12.)).text_color(rgb(0xb3261e)).child(error)); }
        body = body.child(h_flex().gap_1()
            .child(Button::new("pptx-timing-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|e, _, w, cx| e.apply_timing_panel(w, cx))))
            .child(Button::new("pptx-timing-cancel").small().ghost().label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(|e, _, w, cx| e.close_timing_panel(w, cx)))));
        Some(SidebarPanel { title: title.into(), body: body.into_any_element() })
    }
}
