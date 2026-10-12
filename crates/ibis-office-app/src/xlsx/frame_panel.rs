// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! サイズと位置 (書式 > サイズと位置…): 選択中の図形の高さ・幅・回転・横位置・縦位置の
//! 数値入力および縦横比固定の設定を行うサイドバーパネル。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Command, EmuRect, ShapeId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The fields, in the panel's order: height, width (cm), rotation (°),
/// horizontal and vertical place (cm).
pub(crate) struct SheetFramePanel {
    pub(crate) shape_id: ShapeId,
    pub(crate) fields: Vec<Entity<InputState>>,
    pub(crate) error: Option<SharedString>,
    pub(crate) locked: bool,
    pub(crate) placement: ibis_doc_model::ObjectPlacement,
    pub(crate) print: bool,
    pub(crate) initial: Vec<String>,
}

/// The largest size and place Excel takes, in EMU.
const LIMIT: f64 = 570_240_000.;

fn length(value: &str, signed: bool) -> Option<i64> {
    let cm: f64 = value.trim().parse().ok()?;
    let emu = (cm * 360_000.).round();
    (emu.is_finite() && emu <= LIMIT && emu >= if signed { -LIMIT } else { 0. }).then_some(emu as i64)
}

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
    /// 選択中の図形のサイズと位置パネルを開く.
    pub(crate) fn open_sheet_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let Some(shape) = self.slide().shape(shape_id).cloned() else { return };
        let locked = self.slide().aspect_locked.contains(&shape_id);

        let height_str = cm(shape.frame.height);
        let width_str = cm(shape.frame.width);
        let rotation_str = format!("{:.1}", shape.rotation as f64 / 60_000.).trim_end_matches('0').trim_end_matches('.').to_string();
        let x_str = cm(shape.frame.x);
        let y_str = cm(shape.frame.y);

        let initial = vec![height_str.clone(), width_str.clone(), rotation_str.clone(), x_str.clone(), y_str.clone()];
        let fields = vec![
            cx.new(|cx| InputState::new(window, cx).default_value(height_str)),
            cx.new(|cx| InputState::new(window, cx).default_value(width_str)),
            cx.new(|cx| InputState::new(window, cx).default_value(rotation_str)),
            cx.new(|cx| InputState::new(window, cx).default_value(x_str)),
            cx.new(|cx| InputState::new(window, cx).default_value(y_str)),
        ];

        let placement = self.slide().shape_placements.get(&shape_id).copied().unwrap_or_default();
        let print = self.slide().shape_prints.get(&shape_id).copied().unwrap_or(true);

        let state = self.wb_mut();
        state.panels.close();
        state.panels.frame = Some(SheetFramePanel {
            shape_id,
            fields,
            error: None,
            locked,
            placement,
            print,
            initial,
        });
        self.sidebar_open = true;
        cx.notify();
    }

    /// サイズと位置パネルを閉じる.
    pub(crate) fn close_sheet_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.wb_mut().panels.frame = None;
        self.sidebar_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// サイズと位置パネルの設定を適用.
    pub(crate) fn apply_sheet_frame_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.wb().panels.frame.as_ref() else { return };
        let shape_id = panel.shape_id;
        let locked = panel.locked;
        let values: Vec<String> = panel.fields.iter().map(|f| f.read(cx).value().trim().to_string()).collect();
        let Some(shape) = self.slide().shape(shape_id).cloned() else { return };

        let Some(height) = length(&values[0], false) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-positive-number-for-the-height").into());
            }
            cx.notify();
            return;
        };
        let Some(width) = length(&values[1], false) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-positive-number-for-the-width").into());
            }
            cx.notify();
            return;
        };
        let Some(rotation) = angle(&values[2]) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-the-rotation-as-a-number").into());
            }
            cx.notify();
            return;
        };
        let Some(x) = length(&values[3], true) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-number-for-the-horizontal-position").into());
            }
            cx.notify();
            return;
        };
        let Some(y) = length(&values[4], true) else {
            if let Some(p) = self.wb_mut().panels.frame.as_mut() {
                p.error = Some(crate::shell::i18n::text("_xlsx.enter-a-number-for-the-vertical-position").into());
            }
            cx.notify();
            return;
        };

        let mut new_frame = EmuRect::new(x, y, width, height);
        if locked && shape.frame.height > 0 && shape.frame.width > 0 {
            let height_changed = values[0] != panel.initial[0];
            let width_changed = values[1] != panel.initial[1];
            if height_changed && !width_changed {
                new_frame.width = (height as f64 * shape.frame.width as f64 / shape.frame.height as f64).round() as i64;
            } else if width_changed && !height_changed {
                new_frame.height = (width as f64 * shape.frame.height as f64 / shape.frame.width as f64).round() as i64;
            }
        }

        let slide = self.current_slide;
        let mut commands = Vec::new();
        if self.slide().aspect_locked.contains(&shape_id) != locked {
            commands.push(Command::SetAspectLocked { slide, shape: shape_id, locked });
        }
        if shape.frame != new_frame {
            commands.push(Command::SetFrame { slide, shape: shape_id, frame: new_frame });
        }
        if shape.rotation != rotation {
            commands.push(Command::SetTransform { slide, shape: shape_id, rotation, flip_h: shape.flip_h, flip_v: shape.flip_v });
        }
        let cur_placement = self.slide().shape_placements.get(&shape_id).copied().unwrap_or_default();
        if cur_placement != panel.placement {
            commands.push(Command::SetShapePlacement { slide, shape: shape_id, placement: panel.placement });
        }
        let cur_print = self.slide().shape_prints.get(&shape_id).copied().unwrap_or(true);
        if cur_print != panel.print {
            commands.push(Command::SetShapePrint { slide, shape: shape_id, print: panel.print });
        }

        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
        self.close_sheet_frame_panel(window, cx);
    }

    /// サイズと位置パネルのサイドバーUIを生成.
    pub(crate) fn sheet_frame_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.panels.frame.as_ref()?;
        let (fields, error, locked, placement, print) = (panel.fields.clone(), panel.error.clone(), panel.locked, panel.placement, panel.print);

        let mut body = v_flex().gap_2().px(px(16.)).py(px(12.));
        for (label, field) in [
            crate::shell::i18n::text("pptx.frame-panel.height-cm"),
            crate::shell::i18n::text("pptx.frame-panel.width-cm"),
            crate::shell::i18n::text("pptx.frame-panel.rotation-degrees"),
            crate::shell::i18n::text("pptx.frame-panel.horizontal-position-cm-top-left"),
            crate::shell::i18n::text("pptx.frame-panel.vertical-position-cm-top-left"),
        ]
        .into_iter()
        .zip(fields)
        {
            body = body.child(div().text_size(px(13.)).child(label)).child(Input::new(&field).small());
        }

        body = body.child(
            Button::new("xlsx-frame-lock")
                .small()
                .when(!locked, |b| b.ghost())
                .label(format!("{} {}", if locked { "☑" } else { "☐" }, crate::shell::i18n::text("pptx.frame-panel.lock-aspect-ratio")))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.frame.as_mut() {
                        p.locked = !p.locked;
                    }
                    cx.notify();
                })),
        );

        body = body.child(div().pt(px(8.)).text_size(px(13.)).font_weight(gpui::FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.properties")));
        body = body.child(div().text_size(px(12.)).text_color(rgb(0x666666)).child(crate::shell::i18n::text("_xlsx.object-positioning")));
        for (p, label, btn_id) in [
            (ibis_doc_model::ObjectPlacement::MoveAndSize, "xlsx.placement.move-and-size", "xlsx-placement-move-and-size"),
            (ibis_doc_model::ObjectPlacement::MoveOnly, "xlsx.placement.move-only", "xlsx-placement-move-only"),
            (ibis_doc_model::ObjectPlacement::Free, "xlsx.placement.free", "xlsx-placement-free"),
        ] {
            let is_selected = placement == p;
            body = body.child(
                Button::new(btn_id)
                    .small()
                    .when(!is_selected, |b| b.ghost())
                    .label(format!("{} {}", if is_selected { "◉" } else { "○" }, crate::shell::i18n::text(label)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(panel) = this.wb_mut().panels.frame.as_mut() {
                            panel.placement = p;
                        }
                        cx.notify();
                    })),
            );
        }

        body = body.child(
            Button::new("xlsx-frame-print")
                .small()
                .when(!print, |b| b.ghost())
                .label(format!("{} {}", if print { "☑" } else { "☐" }, crate::shell::i18n::text("_xlsx.print-object")))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(panel) = this.wb_mut().panels.frame.as_mut() {
                        panel.print = !panel.print;
                    }
                    cx.notify();
                })),
        );

        if let Some(error) = error {
            body = body.child(div().text_size(px(12.)).text_color(rgb(0xb3261e)).child(error));
        }

        body = body.child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("xlsx-frame-apply").primary()
                        .small()
                        .label(crate::shell::i18n::text("_shell.apply"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.apply_sheet_frame_panel(window, cx);
                        })),
                )
                .child(
                    Button::new("xlsx-frame-cancel")
                        .small()
                        .ghost()
                        .label(crate::shell::i18n::text("_shell.cancel"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_sheet_frame_panel(window, cx);
                        })),
                ),
        );

        Some(SidebarPanel {
            title: crate::shell::i18n::text("pptx.frame-panel.size-position"),
            body: body.into_any_element(),
        })
    }
}
