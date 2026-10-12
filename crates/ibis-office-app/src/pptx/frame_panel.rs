// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! サイズと位置: the selected shapes' height, width, rotation and place
//! typed as numbers (cm and degrees), as PowerPoint's 図形の書式設定 >
//! サイズとプロパティ. The place is the frame's top-left corner from the
//! slide's, before rotation, as PowerPoint shows it.
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Command, EmuRect, ShapeId, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The fields, in the panel's order: height, width (cm), rotation (°),
/// horizontal and vertical place (cm).
pub(crate) struct FramePanel {
    pub(crate) fields: Vec<Entity<InputState>>,
    targets: Vec<(SlideId, ShapeId)>,
    error: Option<SharedString>,
    /// 縦横比を固定 (all the targets lock their proportions).
    locked: bool,
    /// What the fields said when the panel opened (to tell what was typed).
    initial: Vec<String>,
}

impl FramePanel {
    #[allow(dead_code)]
    pub(crate) fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }
}

/// The largest size and place PowerPoint takes, 1584 cm, in EMU.
const LIMIT: f64 = 570_240_000.;

/// `value` cm in EMU, if a number within ±1584 cm (`signed`) or 0 to 1584 cm.
fn length(value: &str, signed: bool) -> Option<i64> {
    let cm: f64 = value.trim().parse().ok()?;
    let emu = (cm * 360_000.).round();
    (emu.is_finite() && emu <= LIMIT && emu >= if signed { -LIMIT } else { 0. }).then_some(emu as i64)
}

/// `value` degrees in 60 000ths, turned into 0..360°.
fn angle(value: &str) -> Option<i32> {
    let degrees: f64 = value.trim().parse().ok()?;
    if !degrees.is_finite() || degrees.abs() > 1e6 {
        return None;
    }
    Some(((degrees * 60_000.).round() as i64).rem_euclid(21_600_000) as i32)
}

fn cm(emu: i64) -> String {
    let v = format!("{:.2}", emu as f64 / 360_000.);
    v.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl SlideEditor {
    pub(crate) fn open_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let slide = self.current_slide;
        let shapes: Vec<_> = self.selected_shapes().into_iter().map(|s| (s.id, s.frame, s.rotation)).collect();
        let locked = !shapes.is_empty() && shapes.iter().all(|(id, ..)| self.slide().aspect_locked.contains(id));
        let Some(&(_, frame, rotation)) = shapes.first() else { return };
        let degrees = format!("{:.1}", rotation as f64 / 60_000.);
        let values = [cm(frame.height), cm(frame.width), degrees.trim_end_matches('0').trim_end_matches('.').to_string(), cm(frame.x), cm(frame.y)];
        let initial = values.to_vec();
        let fields: Vec<_> = values.into_iter().map(|v| cx.new(|cx| InputState::new(window, cx).default_value(v))).collect();
        let first = fields[0].clone();
        if let Some(s) = self.pptx_mut() {
            s.timing_panel = None; s.section_panel = None; s.gradient_panel = None; s.shadow_panel = None; s.glow_panel = None; s.paragraph_panel = None; s.find = None; s.size_panel = None; s.insets_panel = None; s.crop_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None;
            s.frame_panel = Some(FramePanel { fields, targets: shapes.into_iter().map(|(id, _, _)| (slide, id)).collect(), error: None, locked, initial });
        }
        self.sidebar_open = true;
        first.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn apply_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.frame_panel.as_ref()) else { return };
        let text: Vec<String> = panel.fields.iter().map(|f| f.read(cx).value().to_string()).collect();
        let values = (|| Some((length(&text[0], false)?, length(&text[1], false)?, angle(&text[2])?, length(&text[3], true)?, length(&text[4], true)?)))();
        let Some((height, width, rotation, x, y)) = values else {
            if let Some(p) = self.pptx_mut().and_then(|s| s.frame_panel.as_mut()) {
                p.error = Some(crate::shell::i18n::text("pptx.frame-panel.height-width-must-0-1584"));
            }
            return cx.notify();
        };
        let mut commands = Vec::new();
        let locked = panel.locked;
        let typed = (text[0] != panel.initial[0], text[1] != panel.initial[1]);
        let first_frame = panel.targets.first().and_then(|(slide, shape)| self.presentation().slide(*slide)?.shape(*shape)).map(|s| s.frame);
        for (slide, shape) in panel.targets.clone() {
            let Some(target) = self.presentation().slide(slide).and_then(|s| s.shape(shape)) else { continue };
            let mut frame = EmuRect::new(x, y, width, height);
            // With the proportions kept: the side not typed follows the one that was.
            if locked && let Some(old) = first_frame.filter(|f| f.width > 0 && f.height > 0) {
                let (dh, dw) = typed;
                if dh && !dw {
                    frame.width = (height as f64 * old.width as f64 / old.height as f64).round() as i64;
                } else if dw && !dh {
                    frame.height = (width as f64 * old.height as f64 / old.width as f64).round() as i64;
                }
            }
            if self.presentation().slide(slide).is_some_and(|s| s.aspect_locked.contains(&shape) != locked) {
                commands.push(Command::SetAspectLocked { slide, shape, locked });
            }
            if target.frame != frame {
                commands.push(Command::SetFrame { slide, shape, frame });
            }
            if target.rotation != rotation {
                commands.push(Command::SetTransform { slide, shape, rotation, flip_h: target.flip_h, flip_v: target.flip_v });
            }
        }
        self.run(commands, cx);
        self.close_frame_panel(window, cx);
    }

    pub(crate) fn close_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.frame_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn frame_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.frame_panel.as_ref()?;
        let (fields, error, locked) = (p.fields.clone(), p.error.clone(), p.locked);
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.));
        for (label, field) in ["pptx.frame-panel.height-cm", "pptx.frame-panel.width-cm", "pptx.frame-panel.rotation-degrees", "pptx.frame-panel.horizontal-position-cm-top-left", "pptx.frame-panel.vertical-position-cm-top-left"].into_iter().zip(fields) {
            body = body.child(div().text_size(px(13.)).child(crate::shell::i18n::text(label))).child(Input::new(&field).small());
        }
        body = body.child(
            Button::new("pptx-frame-lock")
                .small()
                .when(!locked, |b| b.ghost())
                .label(format!("{} {}", if locked { "☑" } else { "☐" }, crate::shell::i18n::text("pptx.frame-panel.lock-aspect-ratio")))
                .on_click(cx.listener(|e, _, _, cx| {
                    if let Some(p) = e.pptx_mut().and_then(|s| s.frame_panel.as_mut()) {
                        p.locked = !p.locked;
                    }
                    cx.notify();
                })),
        );
        if let Some(error) = error {
            body = body.child(div().text_size(px(12.)).text_color(rgb(0xb3261e)).child(error));
        }
        body = body.child(
            h_flex()
                .gap_1()
                .child(Button::new("pptx-frame-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|e, _, w, cx| e.apply_frame_panel(w, cx))))
                .child(Button::new("pptx-frame-cancel").small().ghost().label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(|e, _, w, cx| e.close_frame_panel(w, cx)))),
        );
        Some(SidebarPanel { title: crate::shell::i18n::text("pptx.frame-panel.size-position"), body: body.into_any_element() })
    }
}

#[cfg(test)]
mod tests {
    use super::{angle, cm, length};

    #[test]
    fn lengths_and_angles_are_read_as_powerpoint_takes_them() {
        assert_eq!(length("2.54", false), Some(914_400));
        assert_eq!(length("-1", false), None);
        assert_eq!(length("-1", true), Some(-360_000));
        assert_eq!(length("1585", true), None);
        assert_eq!(length("abc", true), None);
        assert_eq!(angle("-90"), Some(16_200_000));
        assert_eq!(angle("450"), Some(5_400_000));
        assert_eq!(angle("NaN"), None);
        assert_eq!(cm(914_400), "2.54");
        assert_eq!(cm(360_000), "1");
    }
}
