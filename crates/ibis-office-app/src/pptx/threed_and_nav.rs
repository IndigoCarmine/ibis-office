// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 3D モデルの設定（PPTX-THREED-003〜012）、録画プレビュー（PPTX-RECORD-009）、
//! スクリーンリーダー操作（PPTX-ACCESS-013）、リボンとクイックアクセスの設定（PPTX-FILE-013）、
//! セキュリティ センターと信頼設定（PPTX-FILE-015）。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{
    AnimationClass, AnimationLook, AnimationSide, AutoShape, CameraPreset, Command, EmuRect, Fill,
    Insets, ShapeId, ShapeKind, SlideId, Tilt, Transition,
};

use crate::pptx::animation_ui;
use crate::shell::chrome::{MenuId, MenuItem, Menus, SidebarPanel};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

// =============================================================================
// 1. PPTX-THREED-003..012: 3Dモデルの設定パネル (ThreeDModelPanel)
// =============================================================================

/// PowerPoint の定義済み 3D モデル ビュー（PPTX-THREED-004）.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThreeDModelView {
    DefaultFront,
    Above,
    Below,
    Left,
    Right,
    IsoTopLeft,
    IsoTopRight,
    IsoBottomLeft,
    IsoBottomRight,
    FrontOrtho,
}

impl ThreeDModelView {
    pub(crate) const ALL: [Self; 10] = [
        Self::DefaultFront,
        Self::Above,
        Self::Below,
        Self::Left,
        Self::Right,
        Self::IsoTopLeft,
        Self::IsoTopRight,
        Self::IsoBottomLeft,
        Self::IsoBottomRight,
        Self::FrontOrtho,
    ];

    pub(crate) fn camera_preset(self) -> CameraPreset {
        match self {
            Self::DefaultFront => CameraPreset::PerspectiveFront,
            Self::Above => CameraPreset::PerspectiveAbove,
            Self::Below => CameraPreset::PerspectiveBelow,
            Self::Left => CameraPreset::PerspectiveLeft,
            Self::Right => CameraPreset::PerspectiveRight,
            Self::IsoTopLeft => CameraPreset::IsometricTopUp,
            Self::IsoTopRight => CameraPreset::IsometricTopDown,
            Self::IsoBottomLeft => CameraPreset::IsometricBottomUp,
            Self::IsoBottomRight => CameraPreset::IsometricBottomDown,
            Self::FrontOrtho => CameraPreset::OrthographicFront,
        }
    }

    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::DefaultFront => "pptx.3d-model.view.default",
            Self::Above => "pptx.3d-model.view.above",
            Self::Below => "pptx.3d-model.view.below",
            Self::Left => "pptx.3d-model.view.left",
            Self::Right => "pptx.3d-model.view.right",
            Self::IsoTopLeft => "pptx.3d-model.view.iso-top-left",
            Self::IsoTopRight => "pptx.3d-model.view.iso-top-right",
            Self::IsoBottomLeft => "pptx.3d-model.view.iso-bottom-left",
            Self::IsoBottomRight => "pptx.3d-model.view.iso-bottom-right",
            Self::FrontOrtho => "pptx.3d-model.view.front-ortho",
        }
    }
}

/// PowerPoint の 3D モデル専用アニメーション効果（PPTX-THREED-011）.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThreeDAnimationKind {
    Arrive,
    Turntable,
    Swing,
    JumpAndTurn,
    Leave,
}

impl ThreeDAnimationKind {
    pub(crate) const ALL: [Self; 5] = [
        Self::Arrive,
        Self::Turntable,
        Self::Swing,
        Self::JumpAndTurn,
        Self::Leave,
    ];

    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::Arrive => "pptx.3d-model.anim.arrive",
            Self::Turntable => "pptx.3d-model.anim.turntable",
            Self::Swing => "pptx.3d-model.anim.swing",
            Self::JumpAndTurn => "pptx.3d-model.anim.jump-turn",
            Self::Leave => "pptx.3d-model.anim.leave",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ThreeDModelPanel {
    pub(crate) open: bool,
    pub(crate) playing: bool,
}

/// 1% ズームあたりの Z 軸リフト量（EMU, 1 pt = 12,700 EMU）.
const ZOOM_EMU_PER_PCT: i64 = 12_700;

fn normalize_oo_angle(v: i32) -> i32 {
    let full = 21_600_000;
    ((v % full) + full) % full
}

fn auto_shape_of(kind: &ShapeKind) -> Option<AutoShape> {
    match kind {
        ShapeKind::Auto(auto) => Some(auto.clone()),
        ShapeKind::TextBox(body) => Some(AutoShape::text_box(Fill::None, body.clone())),
        _ => None,
    }
}

impl SlideEditor {
    pub(crate) fn open_3d_model_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            let mut p = s.threed_panel.take().unwrap_or_default();
            p.open = true;
            s.threed_panel = Some(p);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_3d_model_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            if let Some(p) = &mut s.threed_panel {
                p.open = false;
                p.playing = false;
            }
        }
        cx.notify();
    }

    pub(crate) fn is_3d_model_panel_open(&self) -> bool {
        self.pptx().and_then(|s| s.threed_panel.as_ref()).is_some_and(|p| p.open)
    }

    /// 選択中（またはスライド上の 3D モデル）図形の ID を返す.
    pub(crate) fn target_3d_model_shape_id(&self) -> Option<ShapeId> {
        if let Some(&id) = self.selected_ids().first() {
            if self.slide().shape(id).is_some() {
                return Some(id);
            }
        }
        self.slide()
            .shapes
            .iter()
            .find(|sh| sh.text_body().is_some_and(|b| b.text().contains('📦')))
            .map(|sh| sh.id)
    }

    /// PPTX-THREED-003: 3D モデルの 3 軸回転角度（度: X, Y, Z）を取得.
    pub(crate) fn selected_3d_model_rotation_deg(&self) -> (i32, i32, i32) {
        let Some(id) = self.target_3d_model_shape_id() else {
            return (0, 0, 0);
        };
        let Some(shape) = self.slide().shape(id) else {
            return (0, 0, 0);
        };
        if let ShapeKind::Auto(auto) = &shape.kind {
            if let Some(tilt) = auto.tilt {
                return (tilt.lat / 60_000, tilt.lon / 60_000, tilt.rev / 60_000);
            }
        }
        (0, 0, 0)
    }

    /// PPTX-THREED-003: 3D モデルの 3 軸回転角度（度: X, Y, Z）を直接設定.
    pub(crate) fn set_selected_3d_model_rotation(&mut self, x_deg: i32, y_deg: i32, z_deg: i32, cx: &mut Context<Self>) {
        let Some(id) = self.target_3d_model_shape_id() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(id) else { return };
        let Some(mut new_auto) = auto_shape_of(&shape.kind) else { return };
        let mut tilt = new_auto.tilt.unwrap_or_else(|| Tilt::new(0, 0, 0));
        tilt.lat = normalize_oo_angle(x_deg * 60_000);
        tilt.lon = normalize_oo_angle(y_deg * 60_000);
        tilt.rev = normalize_oo_angle(z_deg * 60_000);
        if tilt.camera_preset.is_none() {
            tilt.camera_preset = Some(CameraPreset::PerspectiveFront);
        }
        new_auto.tilt = Some(tilt);
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Auto(new_auto)) }], cx);
    }

    /// PPTX-THREED-003: 3D モデルの 3 軸回転を相対角度（度）だけ回転.
    pub(crate) fn rotate_selected_3d_model(&mut self, dx_deg: i32, dy_deg: i32, dz_deg: i32, cx: &mut Context<Self>) {
        let (x, y, z) = self.selected_3d_model_rotation_deg();
        self.set_selected_3d_model_rotation(x + dx_deg, y + dy_deg, z + dz_deg, cx);
    }

    /// PPTX-THREED-004: 定義済みのモデル ビューを適用.
    pub(crate) fn set_selected_3d_model_view(&mut self, view: ThreeDModelView, cx: &mut Context<Self>) {
        let Some(id) = self.target_3d_model_shape_id() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(id) else { return };
        let Some(mut new_auto) = auto_shape_of(&shape.kind) else { return };
        let prev_depth = new_auto.tilt.map(|t| t.depth).unwrap_or(0);
        let mut tilt = Tilt::from_preset(view.camera_preset());
        tilt.depth = prev_depth;
        new_auto.tilt = Some(tilt);
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Auto(new_auto)) }], cx);
    }

    /// PPTX-THREED-005: 3D モデルのパン（水平・垂直オフセット EMU）とズーム倍率（%）を取得.
    pub(crate) fn selected_3d_model_pan_and_zoom(&self) -> (i64, i64, i32) {
        let Some(id) = self.target_3d_model_shape_id() else {
            return (0, 0, 100);
        };
        let Some(shape) = self.slide().shape(id) else {
            return (0, 0, 100);
        };
        let def = Insets::default();
        let (pan_x, pan_y) = shape
            .text_body()
            .map(|b| (b.insets.left - def.left, b.insets.top - def.top))
            .unwrap_or((0, 0));
        let zoom_delta = match &shape.kind {
            ShapeKind::Auto(auto) => auto.tilt.map(|t| (t.depth / ZOOM_EMU_PER_PCT) as i32).unwrap_or(0),
            _ => 0,
        };
        (pan_x, pan_y, (100 + zoom_delta).clamp(25, 400))
    }

    /// PPTX-THREED-005: 3D モデルをパン（水平・垂直 EMU オフセット）.
    pub(crate) fn pan_selected_3d_model(&mut self, dx_emu: i64, dy_emu: i64, cx: &mut Context<Self>) {
        let Some(id) = self.target_3d_model_shape_id() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(id) else { return };
        let Some(mut new_auto) = auto_shape_of(&shape.kind) else { return };
        if let Some(body) = &mut new_auto.text {
            body.insets.left = (body.insets.left + dx_emu).max(0);
            body.insets.top = (body.insets.top + dy_emu).max(0);
        }
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Auto(new_auto)) }], cx);
    }

    /// PPTX-THREED-005: 3D モデルをズーム（% 増減、50%〜300%）.
    pub(crate) fn zoom_selected_3d_model(&mut self, delta_pct: i32, cx: &mut Context<Self>) {
        let Some(id) = self.target_3d_model_shape_id() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(id) else { return };
        let Some(mut new_auto) = auto_shape_of(&shape.kind) else { return };
        let mut tilt = new_auto.tilt.unwrap_or_else(|| Tilt::from_preset(CameraPreset::PerspectiveFront));
        let cur_pct = 100 + (tilt.depth / ZOOM_EMU_PER_PCT) as i32;
        let next_pct = (cur_pct + delta_pct).clamp(50, 300);
        tilt.depth = (next_pct - 100) as i64 * ZOOM_EMU_PER_PCT;
        new_auto.tilt = Some(tilt);
        self.run(vec![Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Auto(new_auto)) }], cx);
    }

    /// PPTX-THREED-006: 3D モデルの位置とサイズ（倍率スケーリング）.
    pub(crate) fn scale_selected_3d_model_frame(&mut self, w_factor: f64, h_factor: f64, cx: &mut Context<Self>) {
        let Some(id) = self.target_3d_model_shape_id() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(id) else { return };
        let mut frame = shape.frame;
        frame.width = ((frame.width as f64 * w_factor).round() as i64).clamp(360_000, 12_000_000);
        frame.height = ((frame.height as f64 * h_factor).round() as i64).clamp(360_000, 12_000_000);
        self.run(vec![Command::SetFrame { slide, shape: id, frame }], cx);
    }

    /// PPTX-THREED-007: 3D モデルの初期化（回転・ビュー・パン・ズーム・シーンを初期化し、オプションでサイズも初期値に戻す）.
    pub(crate) fn reset_selected_3d_model(&mut self, reset_size: bool, cx: &mut Context<Self>) {
        let Some(id) = self.target_3d_model_shape_id() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(id) else { return };
        let mut cmds = Vec::new();
        if let Some(mut new_auto) = auto_shape_of(&shape.kind) {
            new_auto.tilt = None;
            if let Some(body) = &mut new_auto.text {
                body.insets = Insets::default();
            }
            cmds.push(Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Auto(new_auto)) });
        }
        if self.slide().shape_names.contains_key(&id) {
            cmds.push(Command::SetShapeName { slide, shape: id, name: String::new() });
        }
        if reset_size {
            let (w, h) = (3_000_000, 1_500_000);
            let (slide_w, slide_h) = (self.presentation().slide_size.width, self.presentation().slide_size.height);
            let frame = EmuRect::new((slide_w - w) / 2, (slide_h - h) / 2, w, h);
            cmds.push(Command::SetFrame { slide, shape: id, frame });
        }
        if let Some(s) = self.pptx_mut() {
            if let Some(p) = &mut s.threed_panel {
                p.playing = false;
            }
        }
        if !cmds.is_empty() {
            self.run(cmds, cx);
        }
    }

    /// PPTX-THREED-008 / PPTX-THREED-009: 3D モデルの現在選択されているシーン番号（1〜4）を取得.
    pub(crate) fn selected_3d_model_scene(&self) -> u8 {
        let Some(id) = self.target_3d_model_shape_id() else { return 1 };
        if let Some(name) = self.slide().shape_names.get(&id) {
            for idx in 1..=4u8 {
                if name.contains(&format!("[Scene {idx}]")) {
                    return idx;
                }
            }
        }
        1
    }

    /// PPTX-THREED-008 / PPTX-THREED-009: 3D モデルのシーン（1〜4）を選択・保存（cNvPr/@name に保持）.
    pub(crate) fn select_3d_model_scene(&mut self, scene_idx: u8, cx: &mut Context<Self>) {
        let Some(id) = self.target_3d_model_shape_id() else { return };
        let slide = self.current_slide;
        let idx = scene_idx.clamp(1, 4);
        let name = format!("3D Model [Scene {idx}]");
        self.run(vec![Command::SetShapeName { slide, shape: id, name }], cx);
    }

    /// PPTX-THREED-008 / PPTX-THREED-010: アニメーション付き 3D モデルの再生・一時停止をトグル.
    pub(crate) fn toggle_3d_model_playback(&mut self, cx: &mut Context<Self>) -> bool {
        let playing = if let Some(s) = self.pptx_mut() {
            let mut p = s.threed_panel.take().unwrap_or_default();
            p.playing = !p.playing;
            let now = p.playing;
            s.threed_panel = Some(p);
            now
        } else {
            false
        };
        if playing {
            // Advance turntable yaw by 15° on play start so animated preview is immediately visible
            self.rotate_selected_3d_model(0, 15, 0, cx);
        } else {
            cx.notify();
        }
        playing
    }

    #[allow(dead_code)]
    pub(crate) fn is_3d_model_playing(&self) -> bool {
        self.pptx().and_then(|s| s.threed_panel.as_ref()).is_some_and(|p| p.playing)
    }

    /// PPTX-THREED-011: 3D モデルへのアニメーション効果（到着・ターンテーブル・スイング・ジャンプしてターン・退場）を適用.
    pub(crate) fn apply_3d_model_animation(&mut self, kind: ThreeDAnimationKind, cx: &mut Context<Self>) {
        let Some(id) = self.target_3d_model_shape_id() else { return };
        let slide = self.current_slide;
        let mut anim = match kind {
            ThreeDAnimationKind::Arrive => {
                let mut a = animation_ui::effect(id, AnimationClass::Entrance, AnimationLook::Float(AnimationSide::Bottom));
                a.decel = 50_000;
                a
            }
            ThreeDAnimationKind::Turntable => {
                animation_ui::effect(id, AnimationClass::Emphasis, AnimationLook::Spin(21_600_000))
            }
            ThreeDAnimationKind::Swing => {
                let mut a = animation_ui::effect(id, AnimationClass::Emphasis, AnimationLook::Spin(5_400_000));
                a.auto_reverse = true;
                a
            }
            ThreeDAnimationKind::JumpAndTurn => {
                let mut a = animation_ui::effect(id, AnimationClass::Emphasis, AnimationLook::Scale(125_000, 125_000));
                a.auto_reverse = true;
                a
            }
            ThreeDAnimationKind::Leave => {
                animation_ui::effect(id, AnimationClass::Exit, AnimationLook::Float(AnimationSide::Top))
            }
        };
        anim.duration = 1500;
        let mut animations = self.slide().animations.clone();
        animations.retain(|a| a.shape != id);
        animations.push(anim);
        self.run(vec![Command::SetAnimations { slide, animations }], cx);
    }

    pub(crate) fn threed_model_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.threed_panel.clone()?;
        if !p.open {
            return None;
        }
        let (rx, ry, rz) = self.selected_3d_model_rotation_deg();
        let (pan_x, pan_y, zoom_pct) = self.selected_3d_model_pan_and_zoom();
        let cur_scene = self.selected_3d_model_scene();

        let mut views_grid = h_flex().gap_1().flex_wrap();
        for v in ThreeDModelView::ALL {
            views_grid = views_grid.child(
                Button::new(SharedString::from(format!("pptx-3d-view-{v:?}")))
                    .small()
                    .ghost()
                    .label(text(v.label_key()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_selected_3d_model_view(v, cx);
                    })),
            );
        }

        let rot_controls = v_flex()
            .gap_1()
            .child(div().text_xs().font_weight(FontWeight::BOLD).child(text("pptx.3d-model.rotation")))
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(div().w(px(72.)).text_xs().child(text("pptx.3d-model.rot-x")))
                    .child(Button::new("3d-rx-minus").small().label("-15°").on_click(cx.listener(|this, _, _, cx| this.rotate_selected_3d_model(-15, 0, 0, cx))))
                    .child(div().w(px(44.)).text_xs().text_center().child(format!("{rx}°")))
                    .child(Button::new("3d-rx-plus").small().label("+15°").on_click(cx.listener(|this, _, _, cx| this.rotate_selected_3d_model(15, 0, 0, cx)))),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(div().w(px(72.)).text_xs().child(text("pptx.3d-model.rot-y")))
                    .child(Button::new("3d-ry-minus").small().label("-15°").on_click(cx.listener(|this, _, _, cx| this.rotate_selected_3d_model(0, -15, 0, cx))))
                    .child(div().w(px(44.)).text_xs().text_center().child(format!("{ry}°")))
                    .child(Button::new("3d-ry-plus").small().label("+15°").on_click(cx.listener(|this, _, _, cx| this.rotate_selected_3d_model(0, 15, 0, cx)))),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(div().w(px(72.)).text_xs().child(text("pptx.3d-model.rot-z")))
                    .child(Button::new("3d-rz-minus").small().label("-15°").on_click(cx.listener(|this, _, _, cx| this.rotate_selected_3d_model(0, 0, -15, cx))))
                    .child(div().w(px(44.)).text_xs().text_center().child(format!("{rz}°")))
                    .child(Button::new("3d-rz-plus").small().label("+15°").on_click(cx.listener(|this, _, _, cx| this.rotate_selected_3d_model(0, 0, 15, cx)))),
            );

        let pan_zoom_controls = v_flex()
            .gap_1()
            .child(div().text_xs().font_weight(FontWeight::BOLD).child(format!("{} ({zoom_pct}%, ΔX={}pt, ΔY={}pt)", text("pptx.3d-model.pan-zoom"), pan_x / 12_700, pan_y / 12_700)))
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .child(Button::new("3d-pan-l").small().ghost().label(text("pptx.3d-model.pan-left")).on_click(cx.listener(|this, _, _, cx| this.pan_selected_3d_model(-63_500, 0, cx))))
                    .child(Button::new("3d-pan-r").small().ghost().label(text("pptx.3d-model.pan-right")).on_click(cx.listener(|this, _, _, cx| this.pan_selected_3d_model(63_500, 0, cx))))
                    .child(Button::new("3d-pan-u").small().ghost().label(text("pptx.3d-model.pan-up")).on_click(cx.listener(|this, _, _, cx| this.pan_selected_3d_model(0, -63_500, cx))))
                    .child(Button::new("3d-pan-d").small().ghost().label(text("pptx.3d-model.pan-down")).on_click(cx.listener(|this, _, _, cx| this.pan_selected_3d_model(0, 63_500, cx)))),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("3d-zoom-out").small().ghost().label(text("pptx.3d-model.zoom-out")).on_click(cx.listener(|this, _, _, cx| this.zoom_selected_3d_model(-25, cx))))
                    .child(Button::new("3d-zoom-in").small().ghost().label(text("pptx.3d-model.zoom-in")).on_click(cx.listener(|this, _, _, cx| this.zoom_selected_3d_model(25, cx)))),
            );

        let size_reset_controls = v_flex()
            .gap_1()
            .child(div().text_xs().font_weight(FontWeight::BOLD).child(text("pptx.3d-model.pos-size")))
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .child(Button::new("3d-w-minus").small().ghost().label(text("pptx.3d-model.width-minus")).on_click(cx.listener(|this, _, _, cx| this.scale_selected_3d_model_frame(0.9, 1.0, cx))))
                    .child(Button::new("3d-w-plus").small().ghost().label(text("pptx.3d-model.width-plus")).on_click(cx.listener(|this, _, _, cx| this.scale_selected_3d_model_frame(1.1, 1.0, cx))))
                    .child(Button::new("3d-h-minus").small().ghost().label(text("pptx.3d-model.height-minus")).on_click(cx.listener(|this, _, _, cx| this.scale_selected_3d_model_frame(1.0, 0.9, cx))))
                    .child(Button::new("3d-h-plus").small().ghost().label(text("pptx.3d-model.height-plus")).on_click(cx.listener(|this, _, _, cx| this.scale_selected_3d_model_frame(1.0, 1.1, cx)))),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("3d-reset").small().ghost().label(text("pptx.3d-model.reset")).on_click(cx.listener(|this, _, _, cx| this.reset_selected_3d_model(false, cx))))
                    .child(Button::new("3d-reset-size").small().ghost().label(text("pptx.3d-model.reset-size")).on_click(cx.listener(|this, _, _, cx| this.reset_selected_3d_model(true, cx)))),
            );

        let mut scene_row = h_flex().gap_1().flex_wrap();
        for idx in 1..=4u8 {
            let key = match idx {
                1 => "pptx.3d-model.scene.1",
                2 => "pptx.3d-model.scene.2",
                3 => "pptx.3d-model.scene.3",
                _ => "pptx.3d-model.scene.4",
            };
            let mut btn = Button::new(SharedString::from(format!("3d-scene-{idx}"))).small().label(text(key));
            if cur_scene == idx {
                btn = btn.primary();
            } else {
                btn = btn.ghost();
            }
            scene_row = scene_row.child(btn.on_click(cx.listener(move |this, _, _, cx| {
                this.select_3d_model_scene(idx, cx);
            })));
        }

        let play_label = if p.playing { text("pptx.3d-model.pause") } else { text("pptx.3d-model.play") };
        let scene_controls = v_flex()
            .gap_1()
            .child(div().text_xs().font_weight(FontWeight::BOLD).child(text("pptx.3d-model.scenes")))
            .child(scene_row)
            .child(
                Button::new("3d-toggle-play")
                    .small()
                    .primary()
                    .label(play_label)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let _ = this.toggle_3d_model_playback(cx);
                    })),
            );

        let mut anim_row = h_flex().gap_1().flex_wrap();
        for ak in ThreeDAnimationKind::ALL {
            anim_row = anim_row.child(
                Button::new(SharedString::from(format!("3d-anim-{ak:?}")))
                    .small()
                    .ghost()
                    .label(text(ak.label_key()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_3d_model_animation(ak, cx);
                    })),
            );
        }

        let body = v_flex()
            .gap_3()
            .p_3()
            .child(div().text_xs().font_weight(FontWeight::BOLD).child(text("pptx.3d-model.views")))
            .child(views_grid)
            .child(rot_controls)
            .child(pan_zoom_controls)
            .child(size_reset_controls)
            .child(scene_controls)
            .child(div().text_xs().font_weight(FontWeight::BOLD).child(text("pptx.3d-model.animations")))
            .child(anim_row)
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(text("pptx.3d-model.web-note")))
            .child(
                Button::new("3d-close-btn")
                    .small()
                    .ghost()
                    .label(text("pptx.common.close"))
                    .on_click(cx.listener(|this, _, _, cx| this.close_3d_model_panel(cx))),
            );

        Some(SidebarPanel {
            title: text("pptx.3d-model.title"),
            body: body.into_any_element(),
        })
    }
}

// =============================================================================
// 2. PPTX-RECORD-009: 録画プレビュー (RecordPreviewPanel)
// =============================================================================

#[derive(Clone, Debug, Default)]
pub(crate) struct RecordPreviewPanel {
    pub(crate) open: bool,
    pub(crate) playing: bool,
}

impl SlideEditor {
    pub(crate) fn open_record_preview_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.record_preview = Some(RecordPreviewPanel { open: true, playing: false });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_record_preview_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            if let Some(rp) = &mut s.record_preview {
                rp.open = false;
                rp.playing = false;
            }
        }
        cx.notify();
    }

    pub(crate) fn is_record_preview_open(&self) -> bool {
        self.pptx().and_then(|s| s.record_preview.as_ref()).is_some_and(|rp| rp.open)
    }

    pub(crate) fn is_record_preview_playing(&self) -> bool {
        self.pptx().and_then(|s| s.record_preview.as_ref()).is_some_and(|rp| rp.open && rp.playing)
    }

    pub(crate) fn record_preview_toggle_play(&mut self, cx: &mut Context<Self>) -> bool {
        let now = if let Some(s) = self.pptx_mut() {
            let mut rp = s.record_preview.take().unwrap_or_default();
            rp.open = true;
            rp.playing = !rp.playing;
            let playing = rp.playing;
            s.record_preview = Some(rp);
            playing
        } else {
            false
        };
        cx.notify();
        now
    }

    /// プレゼンテーション全体の記録済みスライド時間合計（ミリ秒、未設定スライドは既定 3000ms として試算）.
    pub(crate) fn record_preview_total_duration_ms(&self) -> u32 {
        self.presentation()
            .slides
            .iter()
            .map(|sl| sl.transition.as_ref().and_then(|t| t.after).unwrap_or(3000))
            .sum()
    }

    /// 現在のスライドの記録済み時間（ミリ秒）を設定（Undo/Redo 対応）.
    pub(crate) fn record_preview_set_slide_timing_ms(&mut self, after_ms: Option<u32>, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut t = self.slide().transition.clone().unwrap_or_else(|| Transition::new(""));
        t.after = after_ms;
        self.run(vec![Command::SetTransition { slide, transition: Some(t) }], cx);
    }

    pub(crate) fn record_preview_next_slide(&mut self, cx: &mut Context<Self>) -> bool {
        let slides = &self.presentation().slides;
        let Some(cur_idx) = slides.iter().position(|s| s.id == self.current_slide) else { return false };
        if cur_idx + 1 < slides.len() {
            self.current_slide = slides[cur_idx + 1].id;
            self.selection = None;
            cx.notify();
            true
        } else {
            false
        }
    }

    pub(crate) fn record_preview_prev_slide(&mut self, cx: &mut Context<Self>) -> bool {
        let slides = &self.presentation().slides;
        let Some(cur_idx) = slides.iter().position(|s| s.id == self.current_slide) else { return false };
        if cur_idx > 0 {
            self.current_slide = slides[cur_idx - 1].id;
            self.selection = None;
            cx.notify();
            true
        } else {
            false
        }
    }

    pub(crate) fn record_preview_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let rp = self.pptx()?.record_preview.clone()?;
        if !rp.open {
            return None;
        }
        let total_ms = self.record_preview_total_duration_ms();
        let cur_idx = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let slide_count = self.presentation().slides.len();
        let cur_after_ms = self.slide().transition.as_ref().and_then(|t| t.after);
        let clip_count = self.slide().shapes.iter().filter(|sh| SlideEditor::clip_of(sh).is_some()).count();
        let anim_count = self.slide().animations.len();

        let summary = format!(
            "Slide {}/{} • Timing: {:.1}s • Total: {:.1}s • Clips: {} • Animations: {}",
            cur_idx + 1,
            slide_count,
            cur_after_ms.unwrap_or(0) as f32 / 1000.0,
            total_ms as f32 / 1000.0,
            clip_count,
            anim_count
        );

        let play_label = if rp.playing {
            text("pptx.record-preview.pause")
        } else {
            text("pptx.record-preview.play")
        };

        let body = v_flex()
            .gap_3()
            .p_3()
            .child(div().p_2().rounded_md().bg(rgb(0xf1f3f4)).text_xs().child(summary))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("rec-prev-play")
                            .small()
                            .primary()
                            .label(play_label)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.record_preview_toggle_play(cx);
                            })),
                    )
                    .child(
                        Button::new("rec-prev-prev")
                            .small()
                            .ghost()
                            .label(text("pptx.record-preview.prev-slide"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.record_preview_prev_slide(cx);
                            })),
                    )
                    .child(
                        Button::new("rec-prev-next")
                            .small()
                            .ghost()
                            .label(text("pptx.record-preview.next-slide"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.record_preview_next_slide(cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("rec-prev-set-5s")
                            .small()
                            .ghost()
                            .label(text("pptx.record-preview.set-5s"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.record_preview_set_slide_timing_ms(Some(5000), cx);
                            })),
                    )
                    .child(
                        Button::new("rec-prev-clear")
                            .small()
                            .ghost()
                            .label(text("pptx.record-preview.clear-timing"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.record_preview_set_slide_timing_ms(None, cx);
                            })),
                    ),
            )
            .child(
                Button::new("rec-prev-start-show")
                    .small()
                    .ghost()
                    .label(text("pptx.record-preview.start-show"))
                    .on_click(cx.listener(|this, _, w, cx| {
                        this.start_slideshow(w, cx);
                    })),
            )
            .child(
                Button::new("rec-prev-close")
                    .small()
                    .ghost()
                    .label(text("pptx.common.close"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_record_preview_panel(cx);
                    })),
            );

        Some(SidebarPanel {
            title: text("pptx.record-preview.title"),
            body: body.into_any_element(),
        })
    }
}

// =============================================================================
// 3. PPTX-ACCESS-013: スクリーンリーダー操作 (PptxScreenReaderState)
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum PptxLandmarkKind {
    #[default]
    All,
    Title,
    Text,
    Table,
    Chart,
    Media,
    Notes,
}

impl PptxLandmarkKind {
    pub(crate) const ALL: [Self; 7] = [
        Self::All,
        Self::Title,
        Self::Text,
        Self::Table,
        Self::Chart,
        Self::Media,
        Self::Notes,
    ];

    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::All => "pptx.screen-reader.filter.all",
            Self::Title => "pptx.screen-reader.filter.title",
            Self::Text => "pptx.screen-reader.filter.text",
            Self::Table => "pptx.screen-reader.filter.table",
            Self::Chart => "pptx.screen-reader.filter.chart",
            Self::Media => "pptx.screen-reader.filter.media",
            Self::Notes => "pptx.screen-reader.filter.notes",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PptxLandmark {
    pub(crate) slide_index: usize,
    pub(crate) slide_id: SlideId,
    pub(crate) shape_id: Option<ShapeId>,
    pub(crate) kind: PptxLandmarkKind,
    pub(crate) label: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PptxScreenReaderState {
    pub(crate) open: bool,
    pub(crate) filter: PptxLandmarkKind,
    pub(crate) current_index: usize,
    pub(crate) reading_aloud: bool,
    pub(crate) last_announcement: String,
}

impl SlideEditor {
    pub(crate) fn open_pptx_screen_reader_panel(&mut self, cx: &mut Context<Self>) {
        let announcement = self.pptx_screen_reader_announce_current();
        if let Some(s) = self.pptx_mut() {
            let mut sr = s.screen_reader.take().unwrap_or_default();
            sr.open = true;
            sr.last_announcement = announcement;
            s.screen_reader = Some(sr);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_pptx_screen_reader_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            if let Some(sr) = &mut s.screen_reader {
                sr.open = false;
                sr.reading_aloud = false;
            }
        }
        cx.notify();
    }

    pub(crate) fn is_pptx_screen_reader_open(&self) -> bool {
        self.pptx().and_then(|s| s.screen_reader.as_ref()).is_some_and(|sr| sr.open)
    }

    pub(crate) fn set_pptx_screen_reader_filter(&mut self, filter: PptxLandmarkKind, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            let mut sr = s.screen_reader.take().unwrap_or_default();
            sr.filter = filter;
            sr.current_index = 0;
            s.screen_reader = Some(sr);
        }
        cx.notify();
    }

    /// プレゼンテーション内の全ランドマークを読み上げ順（装飾オブジェクトを除く）で抽出.
    pub(crate) fn pptx_screen_reader_landmarks(&self, filter: Option<PptxLandmarkKind>) -> Vec<PptxLandmark> {
        let target_filter = filter.unwrap_or(PptxLandmarkKind::All);
        let mut out = Vec::new();
        for (s_idx, slide) in self.presentation().slides.iter().enumerate() {
            let slide_num = s_idx + 1;
            for shape in &slide.shapes {
                if slide.hidden_shapes.contains(&shape.id) || slide.decorative_shapes.contains(&shape.id) {
                    continue;
                }
                let is_title = slide
                    .placeholders
                    .get(&shape.id)
                    .is_some_and(|sp| sp.placeholder.normalized() == "title");
                let alt = slide.shape_alt_texts.get(&shape.id);
                let alt_desc = alt
                    .and_then(|a| {
                        if !a.description.trim().is_empty() {
                            Some(a.description.trim().to_string())
                        } else if !a.title.trim().is_empty() {
                            Some(a.title.trim().to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();

                let (kind, label, detail) = match &shape.kind {
                    ShapeKind::Auto(_) | ShapeKind::TextBox(_) => {
                        let txt = shape.text_body().map(|b| b.text().trim().to_string()).unwrap_or_default();
                        if is_title {
                            let t = if txt.is_empty() {
                                slide.name.clone().unwrap_or_else(|| format!("Slide {slide_num}"))
                            } else {
                                txt
                            };
                            (PptxLandmarkKind::Title, format!("Slide {slide_num} Title: {t}"), "Title".to_string())
                        } else if txt.contains('📦') {
                            (PptxLandmarkKind::Media, format!("Slide {slide_num} 3D Model: {txt}"), "3D Model".to_string())
                        } else if !txt.is_empty() {
                            (PptxLandmarkKind::Text, format!("Slide {slide_num} Text: {txt}"), "Text".to_string())
                        } else if !alt_desc.is_empty() {
                            (PptxLandmarkKind::Text, format!("Slide {slide_num} Shape: {alt_desc}"), "Shape".to_string())
                        } else {
                            continue;
                        }
                    }
                    ShapeKind::Table(tbl) => {
                        let rows = tbl.rows.len();
                        let cols = tbl.columns.len();
                        (
                            PptxLandmarkKind::Table,
                            format!("Slide {slide_num} Table ({rows}x{cols})"),
                            format!("{rows} rows, {cols} cols"),
                        )
                    }
                    ShapeKind::Chart(ch) => {
                        let title = ch
                            .title
                            .as_ref()
                            .map(|ct| ct.text.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| "Chart".to_string());
                        (PptxLandmarkKind::Chart, format!("Slide {slide_num} Chart: {title}"), "Chart".to_string())
                    }
                    ShapeKind::Picture(pic) => {
                        let role = if pic.clip.is_some() { "Media Clip" } else { "Picture" };
                        let desc = if !alt_desc.is_empty() {
                            alt_desc
                        } else {
                            slide.shape_names.get(&shape.id).cloned().unwrap_or_else(|| role.to_string())
                        };
                        (PptxLandmarkKind::Media, format!("Slide {slide_num} {role}: {desc}"), role.to_string())
                    }
                    _ => continue,
                };

                if target_filter == PptxLandmarkKind::All || target_filter == kind {
                    out.push(PptxLandmark {
                        slide_index: s_idx,
                        slide_id: slide.id,
                        shape_id: Some(shape.id),
                        kind,
                        label,
                        detail,
                    });
                }
            }

            if !slide.notes.trim().is_empty()
                && (target_filter == PptxLandmarkKind::All || target_filter == PptxLandmarkKind::Notes)
            {
                out.push(PptxLandmark {
                    slide_index: s_idx,
                    slide_id: slide.id,
                    shape_id: None,
                    kind: PptxLandmarkKind::Notes,
                    label: format!("Slide {slide_num} Notes: {}", slide.notes.trim()),
                    detail: "Speaker Notes".to_string(),
                });
            }
        }
        out
    }

    /// 現在選択されているスライド・図形のスクリーンリーダー読み上げ案内文を生成.
    pub(crate) fn pptx_screen_reader_announce_current(&self) -> String {
        let slide_idx = self.presentation().slide_index(self.current_slide).unwrap_or(0) + 1;
        if let Some(&shape_id) = self.selected_ids().first() {
            if let Some(shape) = self.slide().shape(shape_id) {
                if let Some(txt) = shape.text_body().map(|b| b.text()) {
                    let trimmed = txt.trim();
                    if !trimmed.is_empty() {
                        return format!("Slide {slide_idx}: {trimmed}");
                    }
                }
                if let Some(alt) = self.slide().shape_alt_texts.get(&shape_id) {
                    if !alt.description.trim().is_empty() {
                        return format!("Slide {slide_idx}: {}", alt.description.trim());
                    }
                }
                let name = self.slide().shape_names.get(&shape_id).cloned().unwrap_or_else(|| "Shape".to_string());
                return format!("Slide {slide_idx}: {name}");
            }
        }
        let slide = self.slide();
        let title = slide
            .shapes
            .iter()
            .find(|sh| {
                slide
                    .placeholders
                    .get(&sh.id)
                    .is_some_and(|sp| sp.placeholder.normalized() == "title")
            })
            .and_then(|sh| sh.text_body().map(|b| b.text()))
            .filter(|t| !t.trim().is_empty())
            .or_else(|| slide.name.clone())
            .unwrap_or_default();
        if title.trim().is_empty() {
            format!("Slide {slide_idx} of {}", self.presentation().slides.len())
        } else {
            format!("Slide {slide_idx}: {}", title.trim())
        }
    }

    pub(crate) fn pptx_screen_reader_next_landmark(&mut self, cx: &mut Context<Self>) -> Option<PptxLandmark> {
        let filter = self.pptx().and_then(|s| s.screen_reader.as_ref().map(|sr| sr.filter)).unwrap_or_default();
        let landmarks = self.pptx_screen_reader_landmarks(Some(filter));
        if landmarks.is_empty() {
            return None;
        }
        let cur = self.pptx().and_then(|s| s.screen_reader.as_ref().map(|sr| sr.current_index)).unwrap_or(0);
        let next_idx = (cur + 1) % landmarks.len();
        let lm = landmarks[next_idx].clone();
        self.current_slide = lm.slide_id;
        self.selection = lm.shape_id;
        if let Some(s) = self.pptx_mut() {
            let mut sr = s.screen_reader.take().unwrap_or_default();
            sr.open = true;
            sr.current_index = next_idx;
            sr.last_announcement = lm.label.clone();
            s.screen_reader = Some(sr);
        }
        cx.notify();
        Some(lm)
    }

    pub(crate) fn pptx_screen_reader_prev_landmark(&mut self, cx: &mut Context<Self>) -> Option<PptxLandmark> {
        let filter = self.pptx().and_then(|s| s.screen_reader.as_ref().map(|sr| sr.filter)).unwrap_or_default();
        let landmarks = self.pptx_screen_reader_landmarks(Some(filter));
        if landmarks.is_empty() {
            return None;
        }
        let cur = self.pptx().and_then(|s| s.screen_reader.as_ref().map(|sr| sr.current_index)).unwrap_or(0);
        let prev_idx = if cur == 0 { landmarks.len() - 1 } else { cur - 1 };
        let lm = landmarks[prev_idx].clone();
        self.current_slide = lm.slide_id;
        self.selection = lm.shape_id;
        if let Some(s) = self.pptx_mut() {
            let mut sr = s.screen_reader.take().unwrap_or_default();
            sr.open = true;
            sr.current_index = prev_idx;
            sr.last_announcement = lm.label.clone();
            s.screen_reader = Some(sr);
        }
        cx.notify();
        Some(lm)
    }

    pub(crate) fn pptx_screen_reader_toggle_read_aloud(&mut self, cx: &mut Context<Self>) -> bool {
        let announcement = self.pptx_screen_reader_announce_current();
        let now = if let Some(s) = self.pptx_mut() {
            let mut sr = s.screen_reader.take().unwrap_or_default();
            sr.open = true;
            sr.reading_aloud = !sr.reading_aloud;
            if sr.last_announcement.is_empty() {
                sr.last_announcement = announcement;
            }
            let r = sr.reading_aloud;
            s.screen_reader = Some(sr);
            r
        } else {
            false
        };
        cx.notify();
        now
    }

    pub(crate) fn pptx_screen_reader_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let sr = self.pptx()?.screen_reader.clone()?;
        if !sr.open {
            return None;
        }
        let landmarks = self.pptx_screen_reader_landmarks(Some(sr.filter));
        let announcement = if sr.last_announcement.is_empty() {
            self.pptx_screen_reader_announce_current()
        } else {
            sr.last_announcement.clone()
        };

        let mut filter_row = h_flex().gap_1().flex_wrap();
        for f in PptxLandmarkKind::ALL {
            let mut btn = Button::new(SharedString::from(format!("pptx-sr-filter-{f:?}")))
                .small()
                .label(text(f.label_key()));
            if sr.filter == f {
                btn = btn.primary();
            } else {
                btn = btn.ghost();
            }
            filter_row = filter_row.child(btn.on_click(cx.listener(move |this, _, _, cx| {
                this.set_pptx_screen_reader_filter(f, cx);
            })));
        }

        let mut list = v_flex().gap_1();
        for (idx, lm) in landmarks.into_iter().enumerate() {
            let slide_id = lm.slide_id;
            let shape_id = lm.shape_id;
            let lbl = lm.label.clone();
            list = list.child(
                h_flex()
                    .id(("pptx-sr-lm", idx))
                    .p_2()
                    .rounded_md()
                    .bg(if idx == sr.current_index { rgb(0xe8f0fe) } else { rgb(0xf8f9fa) })
                    .cursor_pointer()
                    .justify_between()
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).child(lm.label))
                    .child(div().text_xs().text_color(rgb(0x5f6368)).child(lm.detail))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.current_slide = slide_id;
                        this.selection = shape_id;
                        if let Some(s) = this.pptx_mut() {
                            if let Some(sr) = &mut s.screen_reader {
                                sr.current_index = idx;
                                sr.last_announcement = lbl.clone();
                            }
                        }
                        cx.notify();
                    })),
            );
        }

        let read_label = if sr.reading_aloud {
            text("pptx.screen-reader.stop-aloud")
        } else {
            text("pptx.screen-reader.read-aloud")
        };

        let body = v_flex()
            .gap_3()
            .p_3()
            .child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(0xe8f0fe))
                    .child(div().text_xs().font_weight(FontWeight::BOLD).child(text("pptx.screen-reader.current")))
                    .child(div().text_xs().child(announcement)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("pptx-sr-prev")
                            .small()
                            .label(text("pptx.screen-reader.prev"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.pptx_screen_reader_prev_landmark(cx);
                            })),
                    )
                    .child(
                        Button::new("pptx-sr-next")
                            .small()
                            .primary()
                            .label(text("pptx.screen-reader.next"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.pptx_screen_reader_next_landmark(cx);
                            })),
                    )
                    .child(
                        Button::new("pptx-sr-aloud")
                            .small()
                            .ghost()
                            .label(read_label)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.pptx_screen_reader_toggle_read_aloud(cx);
                            })),
                    ),
            )
            .child(filter_row)
            .child(list)
            .child(
                Button::new("pptx-sr-close")
                    .small()
                    .ghost()
                    .label(text("pptx.common.close"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_pptx_screen_reader_panel(cx);
                    })),
            );

        Some(SidebarPanel {
            title: text("pptx.screen-reader.title"),
            body: body.into_any_element(),
        })
    }
}

// =============================================================================
// 4. PPTX-FILE-013: リボンとクイックアクセスの設定 (PptxRibbonQatPanel)
// =============================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PptxToolbarGroups {
    pub(crate) slides: bool,
    pub(crate) font: bool,
    pub(crate) paragraph: bool,
    pub(crate) insert: bool,
    pub(crate) drawing: bool,
    pub(crate) arrange: bool,
}

impl Default for PptxToolbarGroups {
    fn default() -> Self {
        Self {
            slides: true,
            font: true,
            paragraph: true,
            insert: true,
            drawing: true,
            arrange: true,
        }
    }
}

const PPTX_GROUPS_KEY: &str = "pptx.toolbar.groups";

impl PptxToolbarGroups {
    pub(crate) fn load() -> Self {
        let Some(raw) = crate::shell::settings::get(PPTX_GROUPS_KEY) else {
            return Self::default();
        };
        let mut g = Self::default();
        for part in raw.split(',') {
            if let Some((k, v)) = part.split_once('=') {
                let on = v.trim() != "0";
                match k.trim() {
                    "slides" => g.slides = on,
                    "font" => g.font = on,
                    "paragraph" => g.paragraph = on,
                    "insert" => g.insert = on,
                    "drawing" => g.drawing = on,
                    "arrange" => g.arrange = on,
                    _ => {}
                }
            }
        }
        g
    }

    pub(crate) fn save(&self) {
        let b = |on: bool| if on { "1" } else { "0" };
        let s = format!(
            "slides={},font={},paragraph={},insert={},drawing={},arrange={}",
            b(self.slides),
            b(self.font),
            b(self.paragraph),
            b(self.insert),
            b(self.drawing),
            b(self.arrange)
        );
        crate::shell::settings::set(PPTX_GROUPS_KEY, &s);
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PptxRibbonQatState {
    pub(crate) open: bool,
    pub(crate) groups: PptxToolbarGroups,
}

impl SlideEditor {
    pub(crate) fn open_pptx_ribbon_qat_panel(&mut self, cx: &mut Context<Self>) {
        let groups = PptxToolbarGroups::load();
        if let Some(s) = self.pptx_mut() {
            s.ribbon_qat = Some(PptxRibbonQatState { open: true, groups });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_pptx_ribbon_qat_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            if let Some(rq) = &mut s.ribbon_qat {
                rq.open = false;
            }
        }
        cx.notify();
    }

    pub(crate) fn is_pptx_ribbon_qat_open(&self) -> bool {
        self.pptx().and_then(|s| s.ribbon_qat.as_ref()).is_some_and(|rq| rq.open)
    }

    pub(crate) fn pptx_toolbar_groups(&self) -> PptxToolbarGroups {
        self.pptx()
            .and_then(|s| s.ribbon_qat.as_ref().map(|rq| rq.groups.clone()))
            .unwrap_or_else(PptxToolbarGroups::load)
    }

    pub(crate) fn set_pptx_toolbar_group_visible(&mut self, group: &str, visible: bool, cx: &mut Context<Self>) {
        let mut g = self.pptx_toolbar_groups();
        match group {
            "slides" => g.slides = visible,
            "font" => g.font = visible,
            "paragraph" => g.paragraph = visible,
            "insert" => g.insert = visible,
            "drawing" => g.drawing = visible,
            "arrange" => g.arrange = visible,
            _ => {}
        }
        g.save();
        if let Some(s) = self.pptx_mut() {
            let mut rq = s.ribbon_qat.take().unwrap_or_default();
            rq.groups = g;
            s.ribbon_qat = Some(rq);
        }
        cx.notify();
    }

    pub(crate) fn reset_pptx_toolbar_groups(&mut self, cx: &mut Context<Self>) {
        let g = PptxToolbarGroups::default();
        g.save();
        if let Some(s) = self.pptx_mut() {
            let mut rq = s.ribbon_qat.take().unwrap_or_default();
            rq.groups = g;
            s.ribbon_qat = Some(rq);
        }
        cx.notify();
    }

    pub(crate) fn pptx_ribbon_qat_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let rq = self.pptx()?.ribbon_qat.clone()?;
        if !rq.open {
            return None;
        }
        let g = rq.groups;
        let groups_spec = [
            ("slides", "pptx.customize-ribbon.group.slides", g.slides),
            ("font", "pptx.customize-ribbon.group.font", g.font),
            ("paragraph", "pptx.customize-ribbon.group.paragraph", g.paragraph),
            ("insert", "pptx.customize-ribbon.group.insert", g.insert),
            ("drawing", "pptx.customize-ribbon.group.drawing", g.drawing),
            ("arrange", "pptx.customize-ribbon.group.arrange", g.arrange),
        ];
        let mut group_col = v_flex().gap_1();
        for (name, key, checked) in groups_spec {
            group_col = group_col.child(
                Checkbox::new(SharedString::from(format!("pptx-rg-{name}")))
                    .label(text(key))
                    .checked(checked)
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        this.set_pptx_toolbar_group_visible(name, *on, cx);
                    })),
            );
        }

        let qat_candidates = [
            "file.save",
            "file.export.pdf",
            "pptx.new-slide",
            "pptx.duplicate-slide",
            "pptx.selection-pane",
            "pptx.reading-order",
            "pptx.check-accessibility",
            "pptx.3d-model",
            "pptx.record-preview",
            "pptx.screen-reader",
        ];
        let chosen = crate::shell::qat::commands();
        let mut qat_col = v_flex().gap_1();
        for id in qat_candidates {
            let on = chosen.iter().any(|c| c == id);
            qat_col = qat_col.child(
                Checkbox::new(SharedString::from(format!("pptx-qat-{id}")))
                    .label(text(id))
                    .checked(on)
                    .on_click(cx.listener(move |_, _, _, cx| {
                        crate::shell::qat::toggle(id);
                        cx.refresh_windows();
                        cx.notify();
                    })),
            );
        }

        let body = v_flex()
            .gap_3()
            .p_3()
            .child(div().text_xs().font_weight(FontWeight::BOLD).child(text("pptx.customize-ribbon.groups")))
            .child(group_col)
            .child(
                Button::new("pptx-rg-reset")
                    .small()
                    .ghost()
                    .label(text("pptx.customize-ribbon.reset-groups"))
                    .on_click(cx.listener(|this, _, _, cx| this.reset_pptx_toolbar_groups(cx))),
            )
            .child(div().text_xs().font_weight(FontWeight::BOLD).child(text("pptx.customize-ribbon.qat")))
            .child(qat_col)
            .child(
                Button::new("pptx-qat-reset")
                    .small()
                    .ghost()
                    .label(text("pptx.customize-ribbon.qat-reset"))
                    .on_click(cx.listener(|_, _, _, cx| {
                        crate::shell::qat::reset();
                        cx.refresh_windows();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("pptx-rq-close")
                    .small()
                    .ghost()
                    .label(text("pptx.common.close"))
                    .on_click(cx.listener(|this, _, _, cx| this.close_pptx_ribbon_qat_panel(cx))),
            );

        Some(SidebarPanel {
            title: text("pptx.customize-ribbon.title"),
            body: body.into_any_element(),
        })
    }
}

// =============================================================================
// 5. PPTX-FILE-015: セキュリティ センターと信頼設定 (PptxTrustCenterState)
// =============================================================================

pub(crate) const EXTERNAL_MEDIA_WARNING_KEY: &str = "trust.external-media-warning";
pub(crate) const PROTECTED_VIEW_KEY: &str = "trust.protected-view";

pub(crate) fn external_media_warning_enabled() -> bool {
    crate::shell::settings::get(EXTERNAL_MEDIA_WARNING_KEY).as_deref() != Some("0")
}

pub(crate) fn set_external_media_warning_enabled(on: bool) {
    crate::shell::settings::set(EXTERNAL_MEDIA_WARNING_KEY, if on { "1" } else { "0" });
}

pub(crate) fn protected_view_enabled() -> bool {
    crate::shell::settings::get(PROTECTED_VIEW_KEY).as_deref() != Some("0")
}

pub(crate) fn set_protected_view_enabled(on: bool) {
    crate::shell::settings::set(PROTECTED_VIEW_KEY, if on { "1" } else { "0" });
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PptxTrustCenterState {
    pub(crate) open: bool,
}

impl SlideEditor {
    pub(crate) fn open_pptx_trust_center_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.trust_center = Some(PptxTrustCenterState { open: true });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_pptx_trust_center_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            if let Some(tc) = &mut s.trust_center {
                tc.open = false;
            }
        }
        cx.notify();
    }

    pub(crate) fn is_pptx_trust_center_open(&self) -> bool {
        self.pptx().and_then(|s| s.trust_center.as_ref()).is_some_and(|tc| tc.open)
    }

    pub(crate) fn pptx_trust_center_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let tc = self.pptx()?.trust_center.clone()?;
        if !tc.open {
            return None;
        }
        let links_warn = crate::shell::settings_window::links_warning_enabled();
        let media_warn = external_media_warning_enabled();
        let prot_view = protected_view_enabled();

        let body = v_flex()
            .gap_3()
            .p_3()
            .child(
                Checkbox::new("pptx-trust-links")
                    .label(text("pptx.trust-center.links-warning"))
                    .checked(links_warn)
                    .on_click(cx.listener(|_, on: &bool, _, cx| {
                        crate::shell::settings::set(crate::shell::settings_window::LINKS_WARNING_KEY, if *on { "1" } else { "0" });
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("pptx-trust-media")
                    .label(text("pptx.trust-center.media-warning"))
                    .checked(media_warn)
                    .on_click(cx.listener(|_, on: &bool, _, cx| {
                        set_external_media_warning_enabled(*on);
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("pptx-trust-protected-view")
                    .label(text("pptx.trust-center.protected-view"))
                    .checked(prot_view)
                    .on_click(cx.listener(|_, on: &bool, _, cx| {
                        set_protected_view_enabled(*on);
                        cx.notify();
                    })),
            )
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(text("pptx.trust-center.macro-note")))
            .child(
                Button::new("pptx-trust-close")
                    .small()
                    .ghost()
                    .label(text("pptx.common.close"))
                    .on_click(cx.listener(|this, _, _, cx| this.close_pptx_trust_center_panel(cx))),
            );

        Some(SidebarPanel {
            title: text("pptx.trust-center.title"),
            body: body.into_any_element(),
        })
    }
}

/// Append menu items to `Menus` without modifying `pptx/chrome.rs`.
pub(crate) fn append_menus(_editor: &SlideEditor, menus: &mut Menus) {
    menus.items_mut(MenuId::Format).push(
        MenuItem::tr("pptx.3d-model.panel")
            .on(|this, _, cx| {
                this.open_3d_model_panel(cx);
            })
            .into(),
    );
    menus.items_mut(MenuId::Slide).push(
        MenuItem::tr("pptx.record-preview")
            .on(|this, _, cx| {
                this.open_record_preview_panel(cx);
            })
            .into(),
    );
    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("pptx.screen-reader")
            .on(|this, _, cx| {
                this.open_pptx_screen_reader_panel(cx);
            })
            .into(),
    );
    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("pptx.customize-ribbon")
            .on(|this, _, cx| {
                this.open_pptx_ribbon_qat_panel(cx);
            })
            .into(),
    );
    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("pptx.trust-center")
            .on(|this, _, cx| {
                this.open_pptx_trust_center_panel(cx);
            })
            .into(),
    );
}
