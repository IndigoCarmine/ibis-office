// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! アニメーション: effects put on the selected shapes from the toolbar's
//! アニメーション menu (each choice takes the place of their effects, as
//! PowerPoint's gallery does), when they start and how long they take, and
//! taking them away. They play in the slide show and are saved as the
//! slide's timing (`save/pptx_timing.rs`).

use ibis_doc_model::{Animation, AnimationAfterEffect, AnimationClass as C, AnimationLook as L, AnimationRepeat, AnimationSide, AnimationStart, AnimationTrigger, Command, ShapeId};

use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;
use gpui_kit::Context;

/// An effect PowerPoint names by its preset (`presetID`, `presetSubtype`),
/// with its usual time (milliseconds): as the file is read back.
pub(crate) fn effect(shape: ShapeId, class: C, look: L) -> Animation {
    let side = |s: AnimationSide| match s {
        AnimationSide::Top => 1,
        AnimationSide::Right => 2,
        AnimationSide::Bottom => 4,
        AnimationSide::Left => 8,
    };
    let (preset, duration) = match look {
        L::Appear => ((1, 0), 1),
        L::Fade => ((10, 0), 500),
        L::Fly(s) => ((2, side(s)), 500),
        L::Spin(_) => ((8, 0), 2000),
        L::Scale(..) => ((6, 0), 2000),
        L::Motion(x, y) => {
            let subtype = if x == 0 && y > 0 {
                4
            } else if x < 0 && y == 0 {
                8
            } else if x > 0 && y == 0 {
                2
            } else if x == 0 && y < 0 {
                1
            } else {
                0
            };
            ((61, subtype), 2000)
        }
        L::Other => ((0, 0), 500),
    };
    Animation {
        shape,
        class,
        start: AnimationStart::OnClick,
        look,
        delay: 0,
        duration,
        preset,
        repeat_count: AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: AnimationAfterEffect::Hold,
        trigger: AnimationTrigger::None,
        paragraph: None, path: Vec::new(),
    }
}

/// アニメーション効果のオプション詳細設定パネル (Effect Options Panel / Dialog)
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnimationOptionsPanel {
    pub(crate) index: usize,
    pub(crate) shape: ShapeId,
    pub(crate) class: C,
    pub(crate) look: L,
    pub(crate) start: AnimationStart,
    pub(crate) delay_ms: u32,
    pub(crate) duration_ms: u32,
    pub(crate) repeat_count: AnimationRepeat,
    pub(crate) auto_reverse: bool,
    pub(crate) rewind: bool,
    pub(crate) accel: u32,
    pub(crate) decel: u32,
    pub(crate) after_effect: AnimationAfterEffect,
    pub(crate) trigger: AnimationTrigger,
}

/// Detailed timeline information for an animation on the slide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TimelineItem {
    pub click_group: usize,
    pub start_time_ms: u32,
    pub end_time_ms: u32,
    pub delay_ms: u32,
    pub duration_ms: u32,
}

/// Compute the visual start and end times (in milliseconds) and click step groups
/// for all animations on the slide, in accordance with PowerPoint's timeline sequence.
pub(crate) fn compute_timeline(animations: &[Animation]) -> (Vec<TimelineItem>, u32) {
    let mut items = Vec::with_capacity(animations.len());
    let mut current_click_start = 0u32;
    let mut current_group = 1usize;
    let mut prev_start = 0u32;
    let mut prev_end = 0u32;
    let mut max_end_overall = 0u32;

    for (i, a) in animations.iter().enumerate() {
        let repeat_factor = match a.repeat_count {
            AnimationRepeat::None => 1,
            AnimationRepeat::Count(n) => n.max(1),
            AnimationRepeat::UntilNextClick | AnimationRepeat::UntilEndOfSlide => 1,
        };
        let anim_dur = a.duration * repeat_factor;
        let (start_time, end_time) = match a.start {
            AnimationStart::OnClick => {
                if i > 0 {
                    current_group += 1;
                    current_click_start = max_end_overall;
                }
                let s = current_click_start + a.delay;
                let e = s + anim_dur;
                (s, e)
            }
            AnimationStart::WithPrevious => {
                let s = if i == 0 { a.delay } else { prev_start + a.delay };
                let e = s + anim_dur;
                (s, e)
            }
            AnimationStart::AfterPrevious => {
                let s = if i == 0 { a.delay } else { prev_end + a.delay };
                let e = s + anim_dur;
                (s, e)
            }
        };
        prev_start = start_time;
        prev_end = end_time;
        max_end_overall = max_end_overall.max(end_time);

        items.push(TimelineItem {
            click_group: current_group,
            start_time_ms: start_time,
            end_time_ms: end_time,
            delay_ms: a.delay,
            duration_ms: anim_dur,
        });
    }

    let total_ms = max_end_overall.max(1000);
    (items, total_ms)
}

impl SlideEditor {
    /// The selected shapes' effects changed by `change` (all of the
    /// slide's effects, and the selected shapes), as one step to undo.
    fn change_animations(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Vec<Animation>, &[ShapeId])) {
        let slide = self.current_slide;
        let selected = self.selected_ids();
        if selected.is_empty() {
            return;
        }
        let mut animations = self.slide().animations.clone();
        change(&mut animations, &selected);
        if animations != self.slide().animations {
            self.run(vec![Command::SetAnimations { slide, animations }], cx);
        }
    }

    /// One effect for each selected shape, in the place of theirs (where
    /// the first of them was, else at the end).
    pub(crate) fn set_shape_effect(&mut self, class: C, look: L, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            let at = all.iter().position(|a| selected.contains(&a.shape)).unwrap_or(all.len());
            let kept_before = all[..at].len();
            all.retain(|a| !selected.contains(&a.shape));
            let at = kept_before.min(all.len());
            for (i, shape) in selected.iter().enumerate() {
                // Several shapes at once (a group's, too) come together, with the first one's click.
                let mut each = effect(*shape, class, look);
                if i > 0 {
                    each.start = AnimationStart::WithPrevious;
                }
                all.insert(at + i, each);
            }
        });
    }

    /// 効果の追加: an effect more for each selected shape, after the slide's
    /// others (theirs kept).
    pub(crate) fn add_shape_effect(&mut self, class: C, look: L, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for (i, shape) in selected.iter().enumerate() {
                let mut each = effect(*shape, class, look);
                if i > 0 {
                    each.start = AnimationStart::WithPrevious;
                }
                all.push(each);
            }
        });
    }

    /// The effect at `index` of the slide's list one place up (or down).
    pub(crate) fn move_effect(&mut self, index: usize, up: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        let other = if up { index.checked_sub(1) } else { Some(index + 1).filter(|i| *i < all.len()) };
        let Some(other) = other else { return };
        if index >= all.len() {
            return;
        }
        all.swap(index, other);
        self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
    }

    /// The effect at `index` of the slide's list taken away.
    pub(crate) fn remove_effect(&mut self, index: usize, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if index < all.len() {
            all.remove(index);
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 開始タイミングを設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_start_at(&mut self, index: usize, start: AnimationStart, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.start = start;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 開始タイミングを次のモードへサイクル切り替え (OnClick -> WithPrevious -> AfterPrevious -> OnClick).
    pub(crate) fn cycle_effect_start_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| a.start);
        if let Some(start) = current {
            let next = match start {
                AnimationStart::OnClick => AnimationStart::WithPrevious,
                AnimationStart::WithPrevious => AnimationStart::AfterPrevious,
                AnimationStart::AfterPrevious => AnimationStart::OnClick,
            };
            self.set_effect_start_at(index, next, cx);
        }
    }

    /// 継続時間を設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_duration_at(&mut self, index: usize, ms: u32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            if a.look != L::Appear {
                a.duration = ms;
                self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
            }
        }
    }

    /// 遅延時間を設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_delay_at(&mut self, index: usize, ms: u32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.delay = ms;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 継続時間を増減 (delta_ms: 例 500ms).
    pub(crate) fn adjust_effect_duration_at(&mut self, index: usize, delta_ms: i32, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| a.duration);
        if let Some(dur) = current {
            let new_dur = (dur as i32 + delta_ms).clamp(100, 60_000) as u32;
            self.set_effect_duration_at(index, new_dur, cx);
        }
    }

    /// 遅延時間を増減 (delta_ms: 例 500ms).
    pub(crate) fn adjust_effect_delay_at(&mut self, index: usize, delta_ms: i32, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| a.delay);
        if let Some(delay) = current {
            let new_delay = (delay as i32 + delta_ms).clamp(0, 60_000) as u32;
            self.set_effect_delay_at(index, new_delay, cx);
        }
    }

    /// 繰り返しを設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_repeat_at(&mut self, index: usize, repeat: AnimationRepeat, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.repeat_count = repeat;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 繰り返しを次のモードへサイクル切り替え (None -> Count(2) -> Count(3) -> Count(5) -> Count(10) -> UntilNextClick -> UntilEndOfSlide -> None).
    pub(crate) fn cycle_effect_repeat_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| a.repeat_count);
        if let Some(r) = current {
            let next = match r {
                AnimationRepeat::None => AnimationRepeat::Count(2),
                AnimationRepeat::Count(2) => AnimationRepeat::Count(3),
                AnimationRepeat::Count(3) => AnimationRepeat::Count(5),
                AnimationRepeat::Count(5) => AnimationRepeat::Count(10),
                AnimationRepeat::Count(_) => AnimationRepeat::UntilNextClick,
                AnimationRepeat::UntilNextClick => AnimationRepeat::UntilEndOfSlide,
                AnimationRepeat::UntilEndOfSlide => AnimationRepeat::None,
            };
            self.set_effect_repeat_at(index, next, cx);
        }
    }

    /// 自動反転をトグル切り替え (特定のインデックスの効果).
    pub(crate) fn toggle_effect_autoreverse_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.auto_reverse = !a.auto_reverse;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 滑らかさ (accel, decel) を設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_smooth_at(&mut self, index: usize, accel: u32, decel: u32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.accel = accel;
            a.decel = decel;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 滑らかさを次のモードへサイクル切り替え (なし -> 開始 -> 終了 -> 両方 -> なし).
    pub(crate) fn cycle_effect_smooth_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| (a.accel > 0, a.decel > 0));
        if let Some(smooth) = current {
            let (next_accel, next_decel) = match smooth {
                (false, false) => (50_000, 0),        // 開始
                (true, false) => (0, 50_000),         // 終了
                (false, true) => (50_000, 50_000),    // 両方
                (true, true) => (0, 0),               // なし
            };
            self.set_effect_smooth_at(index, next_accel, next_decel, cx);
        }
    }

    /// 滑らかに開始をトグル切り替え (特定のインデックスの効果).
    pub(crate) fn toggle_effect_smooth_start_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| (a.accel, a.decel));
        if let Some((accel, decel)) = current {
            let next_accel = if accel > 0 { 0 } else { 50_000 };
            self.set_effect_smooth_at(index, next_accel, decel, cx);
        }
    }

    /// 滑らかに終了をトグル切り替え (特定のインデックスの効果).
    pub(crate) fn toggle_effect_smooth_end_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| (a.accel, a.decel));
        if let Some((accel, decel)) = current {
            let next_decel = if decel > 0 { 0 } else { 50_000 };
            self.set_effect_smooth_at(index, accel, next_decel, cx);
        }
    }

    /// 効果終了時の動作を設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_after_at(&mut self, index: usize, after: AnimationAfterEffect, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.after_effect = after;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 効果終了時の動作を次のモードへサイクル切り替え (Hold -> Hide -> Dim -> HideOnNextClick -> Hold).
    pub(crate) fn cycle_effect_after_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| a.after_effect);
        if let Some(after) = current {
            let next = match after {
                AnimationAfterEffect::Hold => AnimationAfterEffect::Hide,
                AnimationAfterEffect::Hide => AnimationAfterEffect::Dim,
                AnimationAfterEffect::Dim => AnimationAfterEffect::HideOnNextClick,
                AnimationAfterEffect::HideOnNextClick => AnimationAfterEffect::Hold,
            };
            self.set_effect_after_at(index, next, cx);
        }
    }

    /// 再生終了後に巻き戻すをトグル切り替え (特定のインデックスの効果).
    pub(crate) fn toggle_effect_rewind_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.rewind = !a.rewind;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// トリガー条件を設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_trigger_at(&mut self, index: usize, trigger: AnimationTrigger, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.trigger = trigger;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 効果のオプション詳細設定パネルを開く.
    pub(crate) fn open_animation_options_panel(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(a) = self.slide().animations.get(index).cloned() else { return };
        if let Some(s) = self.pptx_mut() {
            s.section_panel = None;
            s.gradient_panel = None;
            s.shadow_panel = None;
            s.glow_panel = None;
            s.paragraph_panel = None;
            s.find = None;
            s.size_panel = None;
            s.frame_panel = None;
            s.comments = None;
            s.chart_panel = None;
            s.header_footer = None;
            s.insets_panel = None;
            s.crop_panel = None;
            s.timing_panel = None;
            s.alt_text_panel = None;
            s.properties_panel = None;
            s.inspect_panel = None;
            s.accessibility_panel = None;
            s.animation_options_panel = Some(AnimationOptionsPanel {
                index,
                shape: a.shape,
                class: a.class,
                look: a.look,
                start: a.start,
                delay_ms: a.delay,
                duration_ms: a.duration,
                repeat_count: a.repeat_count,
                auto_reverse: a.auto_reverse,
                rewind: a.rewind,
                accel: a.accel,
                decel: a.decel,
                after_effect: a.after_effect,
                trigger: a.trigger.clone(),
            });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 選択中の図形のアニメーションで効果のオプション詳細設定パネルを開く.
    pub(crate) fn open_selected_animation_options_panel(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let index = self.slide().animations.iter().position(|a| selected.contains(&a.shape));
        if let Some(i) = index {
            self.open_animation_options_panel(i, cx);
        }
    }

    /// 効果のオプション詳細設定パネルを閉じる.
    pub(crate) fn close_animation_options_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.animation_options_panel = None;
        }
        cx.notify();
    }

    /// 効果のオプション詳細設定パネルの設定を適用して保存.
    pub(crate) fn apply_animation_options_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.animation_options_panel.as_ref()).cloned() else { return };
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(p.index) {
            a.look = p.look;
            a.start = p.start;
            a.delay = p.delay_ms;
            a.duration = p.duration_ms;
            a.repeat_count = p.repeat_count;
            a.auto_reverse = p.auto_reverse;
            a.rewind = p.rewind;
            a.accel = p.accel;
            a.decel = p.decel;
            a.after_effect = p.after_effect;
            a.trigger = p.trigger;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
        self.close_animation_options_panel(cx);
    }

    pub(crate) fn set_panel_trigger(&mut self, trigger: AnimationTrigger, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.trigger = trigger;
        }
        cx.notify();
    }

    pub(crate) fn set_panel_start(&mut self, start: AnimationStart, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.start = start;
        }
        cx.notify();
    }

    pub(crate) fn adjust_panel_duration(&mut self, delta_ms: i32, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.duration_ms = (p.duration_ms as i32 + delta_ms).clamp(100, 60_000) as u32;
        }
        cx.notify();
    }

    pub(crate) fn adjust_panel_delay(&mut self, delta_ms: i32, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.delay_ms = (p.delay_ms as i32 + delta_ms).clamp(0, 60_000) as u32;
        }
        cx.notify();
    }

    pub(crate) fn set_panel_repeat(&mut self, repeat: AnimationRepeat, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.repeat_count = repeat;
        }
        cx.notify();
    }

    pub(crate) fn cycle_panel_repeat(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.repeat_count = match p.repeat_count {
                AnimationRepeat::None => AnimationRepeat::Count(2),
                AnimationRepeat::Count(2) => AnimationRepeat::Count(3),
                AnimationRepeat::Count(3) => AnimationRepeat::Count(5),
                AnimationRepeat::Count(5) => AnimationRepeat::Count(10),
                AnimationRepeat::Count(_) => AnimationRepeat::UntilNextClick,
                AnimationRepeat::UntilNextClick => AnimationRepeat::UntilEndOfSlide,
                AnimationRepeat::UntilEndOfSlide => AnimationRepeat::None,
            };
        }
        cx.notify();
    }

    pub(crate) fn toggle_panel_auto_reverse(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.auto_reverse = !p.auto_reverse;
        }
        cx.notify();
    }

    pub(crate) fn toggle_panel_rewind(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.rewind = !p.rewind;
        }
        cx.notify();
    }

    pub(crate) fn toggle_panel_smooth_start(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.accel = if p.accel > 0 { 0 } else { 50_000 };
        }
        cx.notify();
    }

    pub(crate) fn toggle_panel_smooth_end(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.decel = if p.decel > 0 { 0 } else { 50_000 };
        }
        cx.notify();
    }

    pub(crate) fn set_panel_after_effect(&mut self, after: AnimationAfterEffect, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.after_effect = after;
        }
        cx.notify();
    }

    pub(crate) fn cycle_panel_after_effect(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.after_effect = match p.after_effect {
                AnimationAfterEffect::Hold => AnimationAfterEffect::Hide,
                AnimationAfterEffect::Hide => AnimationAfterEffect::Dim,
                AnimationAfterEffect::Dim => AnimationAfterEffect::HideOnNextClick,
                AnimationAfterEffect::HideOnNextClick => AnimationAfterEffect::Hold,
            };
        }
        cx.notify();
    }

    pub(crate) fn set_panel_fly_side(&mut self, side: AnimationSide, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            if matches!(p.look, L::Fly(_)) {
                p.look = L::Fly(side);
            }
        }
        cx.notify();
    }

    pub(crate) fn set_panel_motion_dir(&mut self, x: i32, y: i32, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            if matches!(p.look, L::Motion(..)) {
                p.look = L::Motion(x, y);
            }
        }
        cx.notify();
    }

    /// アニメーション ウィンドウ: shown or hidden.
    pub(crate) fn toggle_animation_pane(&mut self, cx: &mut Context<Self>) {
        let shown = self.pptx().is_some_and(|s| s.animation_pane);
        if let Some(s) = self.pptx_mut() {
            s.animation_pane = !shown;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The pane: the slide's effects in order, each with its shape, look,
    /// timeline bar, timing controls, to move up or down, adjust, or remove.
    pub(crate) fn animation_pane(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        use gpui_kit::component::button::{Button, ButtonVariants as _};
        use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
        use gpui_kit::*;
        if !self.pptx()?.animation_pane {
            return None;
        }
        let t = crate::shell::i18n::text;
        let slide = self.slide().clone();
        let selected_ids = self.selected_ids();
        let mut body = v_flex().gap_2().px(px(12.)).py(px(8.));
        if slide.animations.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.animation-ui.there-no-animations-slide")));
            return Some(crate::shell::chrome::SidebarPanel { title: t("pptx.animation-ui.animation-pane"), body: body.into_any_element() });
        }

        let (timeline_items, max_time_ms) = compute_timeline(&slide.animations);
        let scale_ms = (((max_time_ms + 499) / 500) * 500).max(1000);
        let scale_sec = scale_ms as f32 / 1000.0;
        let timeline_width = 160.0f32;

        let play_btn = Button::new("anim-play-all")
            .small()
            .label(t("pptx.anim.play-all"))
            .on_click(cx.listener(|e, _, window, cx| e.start_slideshow(window, cx)));

        let mut scale_labels = h_flex().w(px(timeline_width)).justify_between().text_xs().text_color(rgb(0x757575));
        let steps = if scale_sec <= 2.0 { 4 } else if scale_sec <= 5.0 { (scale_sec as usize).max(2) } else { 5 };
        for step in 0..=steps {
            let sec = (scale_sec * step as f32) / steps as f32;
            scale_labels = scale_labels.child(div().child(format!("{:.1}s", sec)));
        }

        let header_bar = h_flex()
            .items_center()
            .justify_between()
            .pb_1()
            .border_b_1()
            .border_color(rgb(0xe0e0e0))
            .child(play_btn)
            .child(
                v_flex()
                    .gap_0p5()
                    .child(div().text_xs().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("_pptx.timeline")))
                    .child(scale_labels),
            );
        body = body.child(header_bar);

        let count = slide.animations.len();
        for (i, a) in slide.animations.iter().enumerate() {
            let item = timeline_items.get(i).copied().unwrap_or(TimelineItem {
                click_group: 1,
                start_time_ms: 0,
                end_time_ms: a.duration,
                delay_ms: a.delay,
                duration_ms: a.duration,
            });

            let is_selected_shape = selected_ids.contains(&a.shape);
            let shape = slide.shape(a.shape);
            let name = shape
                .and_then(|s| s.text_body())
                .map(|b| b.text().chars().take(12).collect::<String>())
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| crate::shell::i18n::format("pptx.selection-pane.shape-n", &[("n", &(slide.shape_index(a.shape).map_or(0, |x| x + 1)))]));

            let start_label = if a.is_triggered() {
                crate::shell::i18n::text("pptx.anim.trigger")
            } else {
                match a.start {
                    AnimationStart::OnClick => crate::shell::i18n::text("pptx.anim.start.click"),
                    AnimationStart::WithPrevious => crate::shell::i18n::text("_pptx.with-previous"),
                    AnimationStart::AfterPrevious => crate::shell::i18n::text("_pptx.after-previous"),
                }
            };

            let (class_color, class_bg, class_name) = match a.class {
                C::Entrance => (rgb(0x2e7d32), rgb(0xe8f5e9), crate::shell::i18n::text("_pptx.entrance")),
                C::Emphasis => (rgb(0xed6c02), rgb(0xfff3e0), crate::shell::i18n::text("_pptx.emphasis")),
                C::Exit => (rgb(0xd32f2f), rgb(0xffebee), crate::shell::i18n::text("_pptx.exit")),
                C::Path => (rgb(0x1976d2), rgb(0xe3f2fd), crate::shell::i18n::text("_pptx.motion-path")),
            };

            let left_offset = (item.start_time_ms as f32 / scale_ms as f32) * timeline_width;
            let bar_width = ((item.duration_ms as f32 / scale_ms as f32) * timeline_width).max(24.0);

            let shape_id = a.shape;
            let mut card = v_flex()
                .gap_1()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(if is_selected_shape { rgb(0x1976d2) } else { rgb(0xe0e0e0) })
                .bg(if is_selected_shape { rgb(0xf0f7ff) } else { rgb(0xffffff) });

            let trigger_badge = match &a.trigger {
                AnimationTrigger::None => None,
                AnimationTrigger::OnShapeClick(trig_shape) => {
                    let trig_name = slide.shape(*trig_shape)
                        .and_then(|s| s.text_body())
                        .map(|b| b.text().chars().take(8).collect::<String>())
                        .filter(|t| !t.trim().is_empty())
                        .unwrap_or_else(|| crate::shell::i18n::format("_pptx.shape-n", &[("n", &(slide.shape_index(*trig_shape).map_or(0, |x| x + 1)))]));
                    Some(div().px_1p5().py_0p5().rounded_sm().bg(rgb(0xfff8e1)).text_xs().text_color(rgb(0xf57f17)).child(format!("⚡️{}", trig_name)))
                }
                AnimationTrigger::OnMediaBookmark { bookmark, .. } => {
                    Some(div().px_1p5().py_0p5().rounded_sm().bg(rgb(0xe0f7fa)).text_xs().text_color(rgb(0x00838f)).child(format!("🔖{}", bookmark)))
                }
            };

            let mut row_top = h_flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .rounded_full()
                        .bg(rgb(0xf0f0f0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(rgb(0x424242))
                        .child(format!("{}", i + 1)),
                )
                .child(
                    Button::new(("anim-select", i))
                        .ghost()
                        .small()
                        .label(name)
                        .on_click(cx.listener(move |e, _, _, cx| {
                            e.set_selection(vec![shape_id]);
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(class_bg)
                        .text_xs()
                        .text_color(class_color)
                        .child(format!("{}・{}", class_name, t(look_id(a.class, a.look)))), // i18n-ok（区切りの「・」だけで、訳す語がない（class と look の間））
                );
            if let Some(tb) = trigger_badge {
                row_top = row_top.child(tb);
            }
            let row_top = row_top
                .child(div().flex_1())
                .child(Button::new(("anim-options-detail", i)).small().ghost().label("⚙️").on_click(cx.listener(move |e, _, _, cx| e.open_animation_options_panel(i, cx))))
                .child(Button::new(("anim-up", i)).small().ghost().label("↑").disabled(i == 0).on_click(cx.listener(move |e, _, _, cx| e.move_effect(i, true, cx))))
                .child(Button::new(("anim-down", i)).small().ghost().label("↓").disabled(i + 1 >= count).on_click(cx.listener(move |e, _, _, cx| e.move_effect(i, false, cx))))
                .child(Button::new(("anim-remove", i)).small().ghost().label(t("_shell.delete")).on_click(cx.listener(move |e, _, _, cx| e.remove_effect(i, cx))));

            let repeat_label = match a.repeat_count {
                AnimationRepeat::None => crate::shell::i18n::text("_pptx.once").to_string(),
                AnimationRepeat::Count(n) => crate::shell::i18n::format("_pptx.n-times", &[("n", &n)]),
                AnimationRepeat::UntilNextClick => crate::shell::i18n::text("_pptx.until-next-click").to_string(),
                AnimationRepeat::UntilEndOfSlide => crate::shell::i18n::text("_pptx.until-end-of-slide").to_string(),
            };
            let autorev_label = if a.auto_reverse { crate::shell::i18n::text("_pptx.reverse") } else { crate::shell::i18n::text("_pptx.none") };
            let rewind_label = if a.rewind { crate::shell::i18n::text("_pptx.rewind") } else { crate::shell::i18n::text("_pptx.none-2") };
            let smooth_label = match (a.accel > 0, a.decel > 0) {
                (true, true) => crate::shell::i18n::text("_pptx.both"),
                (true, false) => crate::shell::i18n::text("_pptx.start"),
                (false, true) => crate::shell::i18n::text("_pptx.end"),
                (false, false) => crate::shell::i18n::text("_pptx.constant"),
            };
            let after_label = match a.after_effect {
                AnimationAfterEffect::Hold => crate::shell::i18n::text("_pptx.hold"),
                AnimationAfterEffect::Hide => crate::shell::i18n::text("_pptx.hide"),
                AnimationAfterEffect::Dim => crate::shell::i18n::text("_pptx.dim"),
                AnimationAfterEffect::HideOnNextClick => crate::shell::i18n::text("_pptx.hide-on-next-click"),
            };

            let row_controls = h_flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new(("anim-start", i))
                        .small()
                        .label(start_label)
                        .on_click(cx.listener(move |e, _, _, cx| e.cycle_effect_start_at(i, cx))),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(rgb(0x616161))
                        .child(div().child(crate::shell::i18n::text("_pptx.time")))
                        .child(Button::new(("anim-dur-minus", i)).small().ghost().label("-").on_click(cx.listener(move |e, _, _, cx| e.adjust_effect_duration_at(i, -500, cx))))
                        .child(div().w(px(30.)).text_center().child(format!("{:.1}s", a.duration as f32 / 1000.0)))
                        .child(Button::new(("anim-dur-plus", i)).small().ghost().label("+").on_click(cx.listener(move |e, _, _, cx| e.adjust_effect_duration_at(i, 500, cx)))),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(rgb(0x616161))
                        .child(div().child(crate::shell::i18n::text("_pptx.delay")))
                        .child(Button::new(("anim-del-minus", i)).small().ghost().label("-").disabled(a.delay == 0).on_click(cx.listener(move |e, _, _, cx| e.adjust_effect_delay_at(i, -500, cx))))
                        .child(div().w(px(30.)).text_center().child(format!("{:.1}s", a.delay as f32 / 1000.0)))
                        .child(Button::new(("anim-del-plus", i)).small().ghost().label("+").on_click(cx.listener(move |e, _, _, cx| e.adjust_effect_delay_at(i, 500, cx)))),
                )
                .child(
                    Button::new(("anim-repeat", i))
                        .small()
                        .ghost()
                        .label(repeat_label)
                        .on_click(cx.listener(move |e, _, _, cx| e.cycle_effect_repeat_at(i, cx))),
                )
                .child(
                    Button::new(("anim-autorev", i))
                        .small()
                        .ghost()
                        .label(autorev_label)
                        .on_click(cx.listener(move |e, _, _, cx| e.toggle_effect_autoreverse_at(i, cx))),
                )
                .child(
                    Button::new(("anim-rewind", i))
                        .small()
                        .ghost()
                        .label(rewind_label)
                        .on_click(cx.listener(move |e, _, _, cx| e.toggle_effect_rewind_at(i, cx))),
                )
                .child(
                    Button::new(("anim-smooth", i))
                        .small()
                        .ghost()
                        .label(smooth_label)
                        .on_click(cx.listener(move |e, _, _, cx| e.cycle_effect_smooth_at(i, cx))),
                )
                .child(
                    Button::new(("anim-after", i))
                        .small()
                        .ghost()
                        .label(after_label)
                        .on_click(cx.listener(move |e, _, _, cx| e.cycle_effect_after_at(i, cx))),
                );

            let mut timeline_track = div()
                .relative()
                .w(px(timeline_width))
                .h(px(16.))
                .rounded_sm()
                .bg(rgb(0xf5f5f5))
                .overflow_hidden();

            if a.delay > 0 {
                let delay_left = ((item.start_time_ms - item.delay_ms) as f32 / scale_ms as f32) * timeline_width;
                let delay_width = (item.delay_ms as f32 / scale_ms as f32) * timeline_width;
                timeline_track = timeline_track.child(
                    div()
                        .absolute()
                        .left(px(delay_left))
                        .w(px(delay_width))
                        .h_full()
                        .bg(rgb(0xeeeeee))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(rgb(0x9e9e9e))
                        .child(format!("+{}", format!("{:.1}s", a.delay as f32 / 1000.0))),
                );
            }

            let bar_element = div()
                .absolute()
                .left(px(left_offset))
                .w(px(bar_width))
                .h_full()
                .rounded_sm()
                .bg(class_color)
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(rgb(0xffffff))
                .child(format!("{:.1}s", a.duration as f32 / 1000.0));
            timeline_track = timeline_track.child(bar_element);

            let row_timeline = h_flex()
                .items_center()
                .justify_end()
                .gap_2()
                .pt_1()
                .child(div().text_xs().text_color(rgb(0x757575)).child(format!("{:.1}s〜{:.1}s", item.start_time_ms as f32 / 1000.0, item.end_time_ms as f32 / 1000.0))) // i18n-ok（範囲の記号）
                .child(timeline_track);

            card = card.child(row_top).child(row_controls).child(row_timeline);
            body = body.child(card);
        }

        Some(crate::shell::chrome::SidebarPanel { title: t("pptx.animation-ui.animation-pane"), body: body.into_any_element() })
    }

    /// 効果のオプション詳細設定パネル (Effect Options Panel / Dialog) の描画.
    pub(crate) fn animation_options_panel(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        use gpui_kit::component::button::{Button, ButtonVariants as _};
        use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
        use gpui_kit::*;

        let p = self.pptx()?.animation_options_panel.as_ref()?.clone();
        let t = crate::shell::i18n::text;

        let title = t("pptx.anim.options-dialog");

        let (class_name, class_color, class_bg) = match p.class {
            C::Entrance => (crate::shell::i18n::text("_pptx.entrance"), rgb(0x2e7d32), rgb(0xe8f5e9)),
            C::Emphasis => (crate::shell::i18n::text("_pptx.emphasis"), rgb(0xed6c02), rgb(0xfff3e0)),
            C::Exit => (crate::shell::i18n::text("_pptx.exit"), rgb(0xd32f2f), rgb(0xffebee)),
            C::Path => (crate::shell::i18n::text("_pptx.motion-path"), rgb(0x1976d2), rgb(0xe3f2fd)),
        };

        let mut body = v_flex().gap_3().px(px(16.)).py(px(12.));

        // 1. 対象アニメーション情報バナー
        let banner = h_flex()
            .items_center()
            .justify_between()
            .p_2()
            .rounded_md()
            .bg(rgb(0xf5f5f5))
            .child(
                h_flex().items_center().gap_1p5().child(
                    div()
                        .w(px(22.))
                        .h(px(22.))
                        .rounded_full()
                        .bg(rgb(0xe0e0e0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .child(format!("{}", p.index + 1)),
                )
                .child(
                    div()
                        .px_2()
                        .py_0p5()
                        .rounded_sm()
                        .bg(class_bg)
                        .text_xs()
                        .text_color(class_color)
                        .child(format!("{}・{}", class_name, t(look_id(p.class, p.look)))), // i18n-ok（区切りの「・」だけで、訳す語がない）
                ),
            );
        body = body.child(banner);

        // 2. 「効果」セクション
        let mut effect_sec = v_flex()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0xe0e0e0))
            .bg(rgb(0xffffff));

        effect_sec = effect_sec.child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x212121))
                .child(t("pptx.anim.panel.effect")),
        );

        // 方向（Fly の場合）
        if let L::Fly(current_side) = p.look {
            let sides = [
                (AnimationSide::Bottom, crate::shell::i18n::text("transition.dir.down")),
                (AnimationSide::Left, crate::shell::i18n::text("transition.dir.left")),
                (AnimationSide::Right, crate::shell::i18n::text("transition.dir.right")),
                (AnimationSide::Top, crate::shell::i18n::text("transition.dir.up")),
            ];
            let mut dir_row = h_flex().items_center().gap_1().flex_wrap();
            dir_row = dir_row.child(div().text_xs().text_color(rgb(0x616161)).child(crate::shell::i18n::text("_pptx.direction")));
            for (s, label) in sides {
                let is_cur = current_side == s;
                dir_row = dir_row.child(
                    Button::new(("anim-fly-side", s as usize))
                        .small()
                        .ghost()
                        .label(if is_cur { format!("✓ {}", label) } else { label.to_string() })
                        .on_click(cx.listener(move |e, _, _, cx| e.set_panel_fly_side(s, cx))),
                );
            }
            effect_sec = effect_sec.child(dir_row);
        }

        // 方向（Motion の場合）
        if let L::Motion(cur_x, cur_y) = p.look {
            let dirs = [
                (0, 25_000, crate::shell::i18n::text("docx.find.dir.down")),
                (-25_000, 0, crate::shell::i18n::text("pptx.anim.opt.path-left")),
                (25_000, 0, crate::shell::i18n::text("pptx.anim.opt.path-right")),
                (0, -25_000, crate::shell::i18n::text("docx.find.dir.up")),
            ];
            let mut motion_row = h_flex().items_center().gap_1().flex_wrap();
            motion_row = motion_row.child(div().text_xs().text_color(rgb(0x616161)).child(crate::shell::i18n::text("_pptx.direction")));
            for (idx, (x, y, label)) in dirs.into_iter().enumerate() {
                let is_cur = cur_x == x && cur_y == y;
                motion_row = motion_row.child(
                    Button::new(("anim-motion-dir", idx))
                        .small()
                        .ghost()
                        .label(if is_cur { format!("✓ {}", label) } else { label.to_string() })
                        .on_click(cx.listener(move |e, _, _, cx| e.set_panel_motion_dir(x, y, cx))),
                );
            }
            effect_sec = effect_sec.child(motion_row);
        }

        // 滑らかさ
        let smooth_row = h_flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("anim-opt-smooth-start")
                    .small()
                    .ghost()
                    .label(if p.accel > 0 { crate::shell::i18n::text("_pptx.smooth-start") } else { crate::shell::i18n::text("pptx.anim.smooth-start") })
                    .on_click(cx.listener(|e, _, _, cx| e.toggle_panel_smooth_start(cx))),
            )
            .child(
                Button::new("anim-opt-smooth-end")
                    .small()
                    .ghost()
                    .label(if p.decel > 0 { crate::shell::i18n::text("_pptx.smooth-end") } else { crate::shell::i18n::text("pptx.anim.smooth-end") })
                    .on_click(cx.listener(|e, _, _, cx| e.toggle_panel_smooth_end(cx))),
            );
        effect_sec = effect_sec.child(smooth_row);

        // 自動反転
        let autorev_btn = Button::new("anim-opt-autorev")
            .small()
            .ghost()
            .label(if p.auto_reverse { crate::shell::i18n::text("_pptx.auto-reverse") } else { crate::shell::i18n::text("_pptx.auto-reverse-2") })
            .on_click(cx.listener(|e, _, _, cx| e.toggle_panel_auto_reverse(cx)));
        effect_sec = effect_sec.child(autorev_btn);

        // アニメーション後の動作
        let after_label = match p.after_effect {
            AnimationAfterEffect::Hold => crate::shell::i18n::text("_pptx.don-t-hide"),
            AnimationAfterEffect::Hide => crate::shell::i18n::text("pptx.anim.after.hide"),
            AnimationAfterEffect::Dim => crate::shell::i18n::text("_pptx.dim-2"),
            AnimationAfterEffect::HideOnNextClick => crate::shell::i18n::text("pptx.anim.after.hide-next"),
        };
        let after_row = h_flex()
            .items_center()
            .gap_1p5()
            .child(div().text_xs().text_color(rgb(0x616161)).child(crate::shell::i18n::text("_pptx.after-animation")))
            .child(
                Button::new("anim-opt-after")
                    .small()
                    .label(after_label)
                    .on_click(cx.listener(|e, _, _, cx| e.cycle_panel_after_effect(cx))),
            );
        effect_sec = effect_sec.child(after_row);
        body = body.child(effect_sec);

        // 3. 「タイミング」セクション
        let mut timing_sec = v_flex()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(0xe0e0e0))
            .bg(rgb(0xffffff));

        timing_sec = timing_sec.child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x212121))
                .child(t("pptx.anim.panel.timing")),
        );

        // 開始タイミング
        let starts = [
            (AnimationStart::OnClick, crate::shell::i18n::text("pptx.anim.start.click")),
            (AnimationStart::WithPrevious, crate::shell::i18n::text("_pptx.with-previous-2")),
            (AnimationStart::AfterPrevious, crate::shell::i18n::text("_pptx.after-previous-2")),
        ];
        let mut start_row = h_flex().items_center().gap_1().flex_wrap();
        start_row = start_row.child(div().text_xs().text_color(rgb(0x616161)).child(crate::shell::i18n::text("pptx.animation-ui.from")));
        for (st, label) in starts {
            let is_cur = p.start == st;
            start_row = start_row.child(
                Button::new(("anim-opt-start", st as usize))
                    .small()
                    .ghost()
                    .label(if is_cur { format!("✓ {}", label) } else { label.to_string() })
                    .on_click(cx.listener(move |e, _, _, cx| e.set_panel_start(st, cx))),
            );
        }
        timing_sec = timing_sec.child(start_row);

        // 継続時間
        let dur_row = h_flex()
            .items_center()
            .gap_2()
            .child(div().text_xs().text_color(rgb(0x616161)).child(crate::shell::i18n::text("_pptx.duration")))
            .child(Button::new("anim-opt-dur-minus").small().ghost().label("-").on_click(cx.listener(|e, _, _, cx| e.adjust_panel_duration(-500, cx))))
            .child(div().w(px(36.)).text_center().text_xs().child(crate::shell::i18n::format("_pptx.value-s", &[("value", &format!("{:.1}", p.duration_ms as f32 / 1000.0))])))
            .child(Button::new("anim-opt-dur-plus").small().ghost().label("+").on_click(cx.listener(|e, _, _, cx| e.adjust_panel_duration(500, cx))));
        timing_sec = timing_sec.child(dur_row);

        // 遅延時間
        let del_row = h_flex()
            .items_center()
            .gap_2()
            .child(div().text_xs().text_color(rgb(0x616161)).child(crate::shell::i18n::text("_pptx.delay")))
            .child(Button::new("anim-opt-del-minus").small().ghost().label("-").disabled(p.delay_ms == 0).on_click(cx.listener(|e, _, _, cx| e.adjust_panel_delay(-500, cx))))
            .child(div().w(px(36.)).text_center().text_xs().child(crate::shell::i18n::format("_pptx.value-s", &[("value", &format!("{:.1}", p.delay_ms as f32 / 1000.0))])))
            .child(Button::new("anim-opt-del-plus").small().ghost().label("+").on_click(cx.listener(|e, _, _, cx| e.adjust_panel_delay(500, cx))));
        timing_sec = timing_sec.child(del_row);

        // 繰り返し
        let repeat_label = match p.repeat_count {
            AnimationRepeat::None => crate::shell::i18n::text("_shell.none"),
            AnimationRepeat::Count(2) => crate::shell::i18n::text("_pptx.2-times"),
            AnimationRepeat::Count(3) => crate::shell::i18n::text("_pptx.3-times"),
            AnimationRepeat::Count(5) => crate::shell::i18n::text("_pptx.5-times"),
            AnimationRepeat::Count(10) => crate::shell::i18n::text("_pptx.10-times"),
            AnimationRepeat::Count(_) => crate::shell::i18n::text("_pptx.multiple-times"),
            AnimationRepeat::UntilNextClick => crate::shell::i18n::text("pptx.anim.repeat.until-click"),
            AnimationRepeat::UntilEndOfSlide => crate::shell::i18n::text("pptx.anim.repeat.until-end"),
        };
        let rep_row = h_flex()
            .items_center()
            .gap_2()
            .child(div().text_xs().text_color(rgb(0x616161)).child(crate::shell::i18n::text("_pptx.repeat")))
            .child(
                Button::new("anim-opt-repeat")
                    .small()
                    .label(repeat_label)
                    .on_click(cx.listener(|e, _, _, cx| e.cycle_panel_repeat(cx))),
            );
        timing_sec = timing_sec.child(rep_row);

        // 巻き戻し (Rewind when done playing)
        let rewind_btn = Button::new("anim-opt-rewind")
            .small()
            .ghost()
            .label(if p.rewind { crate::shell::i18n::text("_pptx.rewind-when-done-playing") } else { crate::shell::i18n::text("_pptx.rewind-when-done-playing-2") })
            .on_click(cx.listener(|e, _, _, cx| e.toggle_panel_rewind(cx)));
        timing_sec = timing_sec.child(rewind_btn);

        // トリガー (Trigger)
        let trig_desc = match &p.trigger {
            AnimationTrigger::None => crate::shell::i18n::text("_pptx.normal-click-or-auto").to_string(),
            AnimationTrigger::OnShapeClick(s) => {
                let s_name = self.slide().shape(*s)
                    .and_then(|sh| sh.text_body())
                    .map(|b| b.text().chars().take(10).collect::<String>())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| crate::shell::i18n::format("_pptx.shape-n", &[("n", &(self.slide().shape_index(*s).map_or(0, |x| x + 1)))]));
                crate::shell::i18n::format("_pptx.on-click-name", &[("name", &s_name)])
            }
            AnimationTrigger::OnMediaBookmark { bookmark, .. } => crate::shell::i18n::format("_pptx.on-bookmark-bookmark", &[("bookmark", &bookmark)]),
        };
        let is_trig = !p.trigger.is_none();
        let trig_row = h_flex()
            .items_center()
            .justify_between()
            .gap_1()
            .pt_1()
            .border_t_1()
            .border_color(rgb(0xf0f0f0))
            .child(
                h_flex()
                    .items_center()
                    .gap_1p5()
                    .child(div().text_xs().text_color(rgb(0x616161)).child(crate::shell::i18n::text("_pptx.trigger-2")))
                    .child(div().text_xs().text_color(rgb(0x212121)).child(trig_desc)),
            )
            .child(
                Button::new("anim-opt-trig-clear")
                    .small()
                    .ghost()
                    .label(crate::shell::i18n::text("_pptx.clear"))
                    .disabled(!is_trig)
                    .on_click(cx.listener(|e, _, _, cx| e.set_panel_trigger(AnimationTrigger::None, cx))),
            );
        timing_sec = timing_sec.child(trig_row);
        body = body.child(timing_sec);

        // 4. フッターボタン
        let footer = h_flex()
            .gap_2()
            .pt_2()
            .child(Button::new("pptx-anim-opt-apply").primary().small().label(t("_shell.apply")).on_click(cx.listener(|e, _, _, cx| e.apply_animation_options_panel(cx))))
            .child(Button::new("pptx-anim-opt-cancel").small().ghost().label(t("_shell.cancel")).on_click(cx.listener(|e, _, _, cx| e.close_animation_options_panel(cx))));
        body = body.child(footer);

        Some(crate::shell::chrome::SidebarPanel {
            title: title.into(),
            body: body.into_any_element(),
        })
    }

    /// 開始のタイミング of the selected shapes' effects.
    pub(crate) fn set_effect_start(&mut self, start: AnimationStart, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.start = start;
            }
        });
    }

    /// 継続時間 of the selected shapes' effects (not an appear, which is at once).
    pub(crate) fn set_effect_duration(&mut self, ms: u32, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape) && a.look != L::Appear) {
                a.duration = ms;
            }
        });
    }

    /// 開始遅延 of the selected shapes' effects.
    pub(crate) fn set_effect_delay(&mut self, ms: u32, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.delay = ms;
            }
        });
    }

    /// 繰り返し of the selected shapes' effects.
    pub(crate) fn set_effect_repeat(&mut self, repeat: AnimationRepeat, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.repeat_count = repeat;
            }
        });
    }

    /// 自動反転 of the selected shapes' effects.
    pub(crate) fn toggle_effect_autoreverse(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let all_rev = self.slide().animations.iter().filter(|a| selected.contains(&a.shape)).all(|a| a.auto_reverse);
        let new_rev = !all_rev;
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.auto_reverse = new_rev;
            }
        });
    }

    /// 再生終了後に巻き戻す (rewind) of the selected shapes' effects.
    pub(crate) fn toggle_effect_rewind(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let all_rew = self.slide().animations.iter().filter(|a| selected.contains(&a.shape)).all(|a| a.rewind);
        let new_rew = !all_rew;
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.rewind = new_rew;
            }
        });
    }

    /// 再生終了後に巻き戻す (rewind) of the selected shapes' effects.
    pub(crate) fn set_effect_rewind(&mut self, rewind: bool, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.rewind = rewind;
            }
        });
    }

    /// 滑らかさ of the selected shapes' effects.
    pub(crate) fn set_effect_smooth(&mut self, accel: u32, decel: u32, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.accel = accel;
                a.decel = decel;
            }
        });
    }

    /// 滑らかに開始 of the selected shapes' effects.
    pub(crate) fn toggle_effect_smooth_start(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let all_smooth = self.slide().animations.iter().filter(|a| selected.contains(&a.shape)).all(|a| a.accel > 0);
        let new_accel = if all_smooth { 0 } else { 50_000 };
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.accel = new_accel;
            }
        });
    }

    /// 滑らかに終了 of the selected shapes' effects.
    pub(crate) fn toggle_effect_smooth_end(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let all_smooth = self.slide().animations.iter().filter(|a| selected.contains(&a.shape)).all(|a| a.decel > 0);
        let new_decel = if all_smooth { 0 } else { 50_000 };
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.decel = new_decel;
            }
        });
    }

    /// 効果終了時の動作 of the selected shapes' effects.
    pub(crate) fn set_effect_after(&mut self, after: AnimationAfterEffect, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.after_effect = after;
            }
        });
    }

    /// トリガー (OnShapeClick, OnMediaBookmark, None) of the selected shapes' effects.
    pub(crate) fn set_selected_effects_trigger(&mut self, trigger: AnimationTrigger, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                a.trigger = trigger.clone();
            }
        });
    }

    /// トリガーの解除 (通常進行に戻す) of the selected shapes' effects.
    pub(crate) fn clear_selected_effects_trigger(&mut self, cx: &mut Context<Self>) {
        self.set_selected_effects_trigger(AnimationTrigger::None, cx);
    }

    /// アニメーションの削除: the selected shapes' effects.
    pub(crate) fn remove_shape_effects(&mut self, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| all.retain(|a| !selected.contains(&a.shape)));
    }

    /// 効果のオプション of the selected shapes' effects.
    pub(crate) fn set_shape_effect_option(&mut self, new_look: L, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            let side_subtype = |side: AnimationSide| match side {
                AnimationSide::Top => 1,
                AnimationSide::Right => 2,
                AnimationSide::Bottom => 4,
                AnimationSide::Left => 8,
            };
            for a in all.iter_mut().filter(|a| selected.contains(&a.shape)) {
                match (a.look, new_look) {
                    (L::Fly(_), L::Fly(side)) => {
                        a.look = L::Fly(side);
                        a.preset.1 = side_subtype(side);
                    }
                    (L::Spin(_), L::Spin(by)) => {
                        a.look = L::Spin(by);
                        a.preset = (8, 0);
                    }
                    (L::Scale(..), L::Scale(x, y)) => {
                        a.look = L::Scale(x, y);
                        a.preset = (6, 0);
                    }
                    (L::Motion(..), L::Motion(x, y)) => {
                        let subtype = if x == 0 && y > 0 {
                            4
                        } else if x < 0 && y == 0 {
                            8
                        } else if x > 0 && y == 0 {
                            2
                        } else if x == 0 && y < 0 {
                            1
                        } else {
                            0
                        };
                        a.look = L::Motion(x, y);
                        a.preset = (61, subtype);
                    }
                    _ => {}
                }
            }
        });
    }

    /// テキストの動き: the selected shapes' effects on the shape as a whole
    /// (`by_paragraph` false) or one for each paragraph of its text (each
    /// started by a click, the first as the effect was).
    pub(crate) fn set_effect_text_build(&mut self, by_paragraph: bool, cx: &mut Context<Self>) {
        let counts: std::collections::HashMap<ShapeId, usize> = self.selected_shapes().into_iter().filter_map(|s| Some((s.id, s.text_body()?.paragraphs.len()))).collect();
        self.change_animations(cx, |all, selected| {
            for &shape in selected {
                if by_paragraph {
                    let n = counts.get(&shape).copied().unwrap_or(0);
                    if n < 2 {
                        continue;
                    }
                    let mut out = Vec::with_capacity(all.len() + n);
                    for a in all.drain(..) {
                        if a.shape == shape && a.paragraph.is_none() && a.trigger.is_none() {
                            for k in 0..n {
                                let mut each = a.clone();
                                each.paragraph = Some((k as u32, k as u32));
                                if k > 0 {
                                    each.start = AnimationStart::OnClick;
                                    each.delay = 0;
                                }
                                out.push(each);
                            }
                        } else {
                            out.push(a);
                        }
                    }
                    *all = out;
                } else {
                    let mut seen = false;
                    all.retain_mut(|a| {
                        if a.shape != shape || a.paragraph.is_none() {
                            return true;
                        }
                        let keep = !seen;
                        seen = true;
                        a.paragraph = None;
                        keep
                    });
                }
            }
        });
    }

    /// アニメーションのコピー: copy the animation effects of the selected shape.
    pub(crate) fn copy_shape_animations(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let Some(&shape_id) = selected.first() else { return };
        let animations: Vec<ibis_doc_model::Animation> = self.slide().animations.iter().filter(|a| a.shape == shape_id).cloned().collect();
        if animations.is_empty() {
            return;
        }
        if let Some(s) = self.pptx_mut() {
            s.animation_copy = Some(animations);
        }
        self.set_status(crate::shell::i18n::text("pptx.anim.copied").to_string(), cx);
        cx.notify();
    }

    /// アニメーションの貼り付け: paste the copied animation effects onto the selected shapes.
    pub(crate) fn paste_shape_animations(&mut self, cx: &mut Context<Self>) {
        let copied = self.pptx().and_then(|s| s.animation_copy.clone());
        let Some(copied) = copied else { return };
        if copied.is_empty() {
            return;
        }
        self.change_animations(cx, |all, selected| {
            for &shape in selected {
                all.retain(|a| a.shape != shape);
                for template in &copied {
                    let mut anim = template.clone();
                    anim.shape = shape;
                    all.push(anim);
                }
            }
        });
        self.set_status(crate::shell::i18n::text("pptx.anim.pasted").to_string(), cx);
    }

    /// Whether animation effects can be copied from the current selection.
    pub(crate) fn can_copy_animations(&self) -> bool {
        let selected = self.selected_ids();
        let Some(&shape_id) = selected.first() else { return false };
        self.slide().animations.iter().any(|a| a.shape == shape_id)
    }

    /// Whether copied animation effects can be pasted onto the current selection.
    pub(crate) fn can_paste_animations(&self) -> bool {
        let has_copied = self.pptx().is_some_and(|s| s.animation_copy.as_ref().is_some_and(|c| !c.is_empty()));
        has_copied && !self.selected_ids().is_empty()
    }

    /// アニメーションの書式設定（Animation Painter）ボタンのトグル動作.
    pub(crate) fn toggle_animation_painter(&mut self, cx: &mut Context<Self>) {
        if self.can_copy_animations() {
            self.copy_shape_animations(cx);
        } else if self.can_paste_animations() {
            self.paste_shape_animations(cx);
        }
    }
}

/// The menu id (and label) of an effect's look.
fn look_id(class: C, look: L) -> &'static str {
    match (class, look) {
        (C::Entrance, L::Appear) => "pptx.anim.in.appear",
        (C::Entrance, L::Fade) => "pptx.anim.in.fade",
        (C::Entrance, L::Fly(AnimationSide::Bottom)) => "pptx.anim.in.fly-bottom",
        (C::Entrance, L::Fly(AnimationSide::Left)) => "pptx.anim.in.fly-left",
        (C::Entrance, L::Fly(AnimationSide::Right)) => "pptx.anim.in.fly-right",
        (C::Entrance, L::Fly(AnimationSide::Top)) => "pptx.anim.in.fly-top",
        (C::Exit, L::Appear) => "pptx.anim.out.disappear",
        (C::Exit, L::Fade) => "pptx.anim.out.fade",
        (C::Exit, L::Fly(AnimationSide::Bottom)) => "pptx.anim.out.fly-bottom",
        (C::Exit, L::Fly(AnimationSide::Left)) => "pptx.anim.out.fly-left",
        (C::Exit, L::Fly(AnimationSide::Right)) => "pptx.anim.out.fly-right",
        (C::Exit, L::Fly(AnimationSide::Top)) => "pptx.anim.out.fly-top",
        (C::Emphasis, L::Spin(_)) => "pptx.anim.em.spin",
        (C::Emphasis, L::Scale(..)) => "pptx.anim.em.grow",
        (C::Path, L::Motion(x, y)) if x == 0 && y > 0 => "pptx.anim.path.down",
        (C::Path, L::Motion(x, y)) if x < 0 && y == 0 => "pptx.anim.path.left",
        (C::Path, L::Motion(x, y)) if x > 0 && y == 0 => "pptx.anim.path.right",
        (C::Path, L::Motion(x, y)) if x == 0 && y < 0 => "pptx.anim.path.up",
        (C::Path, _) => "pptx.anim.path",
        _ => "pptx.anim.other",
    }
}

/// Whether the selected shapes have an animation supporting effect options (direction, rotation, scale, or details panel).
pub(crate) fn has_animation_options(editor: &SlideEditor) -> bool {
    let selected = editor.selected_ids();
    editor.slide().animations.iter().any(|a| selected.contains(&a.shape))
}

/// The options menu for the currently selected shape's animation (direction, rotation, scale, or details panel).
pub(crate) fn animation_option_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let selected = editor.selected_ids();
    let mine: Vec<&Animation> = editor.slide().animations.iter().filter(|a| selected.contains(&a.shape)).collect();
    let first = mine.first();
    let mut items: Vec<MenuEntry> = match first.map(|a| a.look) {
        Some(L::Fly(current_side)) => {
            vec![
                MenuItem::tr("pptx.anim.opt.fly-bottom").checked(current_side == AnimationSide::Bottom).on(|e, _, cx| e.set_shape_effect_option(L::Fly(AnimationSide::Bottom), cx)).into(),
                MenuItem::tr("pptx.anim.opt.fly-left").checked(current_side == AnimationSide::Left).on(|e, _, cx| e.set_shape_effect_option(L::Fly(AnimationSide::Left), cx)).into(),
                MenuItem::tr("pptx.anim.opt.fly-right").checked(current_side == AnimationSide::Right).on(|e, _, cx| e.set_shape_effect_option(L::Fly(AnimationSide::Right), cx)).into(),
                MenuItem::tr("pptx.anim.opt.fly-top").checked(current_side == AnimationSide::Top).on(|e, _, cx| e.set_shape_effect_option(L::Fly(AnimationSide::Top), cx)).into(),
            ]
        }
        Some(L::Spin(current_by)) => {
            vec![
                MenuItem::tr("pptx.anim.opt.spin-360-cw").checked(current_by == 21_600_000).on(|e, _, cx| e.set_shape_effect_option(L::Spin(21_600_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.spin-360-ccw").checked(current_by == -21_600_000).on(|e, _, cx| e.set_shape_effect_option(L::Spin(-21_600_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.spin-180").checked(current_by == 10_800_000).on(|e, _, cx| e.set_shape_effect_option(L::Spin(10_800_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.spin-720").checked(current_by == 43_200_000).on(|e, _, cx| e.set_shape_effect_option(L::Spin(43_200_000), cx)).into(),
            ]
        }
        Some(L::Scale(current_x, _)) => {
            vec![
                MenuItem::tr("pptx.anim.opt.scale-150").checked(current_x == 150_000).on(|e, _, cx| e.set_shape_effect_option(L::Scale(150_000, 150_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.scale-200").checked(current_x == 200_000).on(|e, _, cx| e.set_shape_effect_option(L::Scale(200_000, 200_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.scale-50").checked(current_x == 50_000).on(|e, _, cx| e.set_shape_effect_option(L::Scale(50_000, 50_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.scale-25").checked(current_x == 25_000).on(|e, _, cx| e.set_shape_effect_option(L::Scale(25_000, 25_000), cx)).into(),
            ]
        }
        Some(L::Motion(current_x, current_y)) => {
            vec![
                MenuItem::tr("pptx.anim.opt.path-down").checked(current_x == 0 && current_y > 0).on(|e, _, cx| e.set_shape_effect_option(L::Motion(0, 25_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.path-left").checked(current_x < 0 && current_y == 0).on(|e, _, cx| e.set_shape_effect_option(L::Motion(-25_000, 0), cx)).into(),
                MenuItem::tr("pptx.anim.opt.path-right").checked(current_x > 0 && current_y == 0).on(|e, _, cx| e.set_shape_effect_option(L::Motion(25_000, 0), cx)).into(),
                MenuItem::tr("pptx.anim.opt.path-up").checked(current_x == 0 && current_y < 0).on(|e, _, cx| e.set_shape_effect_option(L::Motion(0, -25_000), cx)).into(),
            ]
        }
        _ => Vec::new(),
    };
    if !mine.is_empty() {
        if !items.is_empty() {
            items.push(MenuEntry::Separator);
        }
        items.push(MenuItem::tr("pptx.anim.options-dialog").on(|e, _, cx| e.open_selected_animation_options_panel(cx)).into());
    }
    if items.is_empty() {
        // Nothing to set (no effect selected): the item is shown grey, as Office does.
        items.push(MenuItem::tr("pptx.anim.options-dialog").into());
    }
    items
}

/// The toolbar's アニメーション menu, for the selection.
pub(crate) fn animation_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let selected = editor.selected_ids();
    let mine: Vec<&Animation> = editor.slide().animations.iter().filter(|a| selected.contains(&a.shape)).collect();
    let has = !mine.is_empty();
    let options = animation_option_items(editor);
    let start = mine.first().map(|a| a.start);
    let duration = mine.iter().find(|a| a.look != L::Appear).map(|a| a.duration);
    let delay = mine.first().map(|a| a.delay);
    let effect_item = |id: &'static str, class: C, look: L| -> MenuEntry {
        let on = mine.len() == selected.len() && mine.iter().all(|a| a.class == class && a.look == look);
        MenuItem::tr(id).checked(on).on(move |e, _, cx| e.set_shape_effect(class, look, cx)).into()
    };
    let entrance = vec![
        effect_item("pptx.anim.in.appear", C::Entrance, L::Appear),
        effect_item("pptx.anim.in.fade", C::Entrance, L::Fade),
        effect_item("pptx.anim.in.fly-bottom", C::Entrance, L::Fly(AnimationSide::Bottom)),
        effect_item("pptx.anim.in.fly-left", C::Entrance, L::Fly(AnimationSide::Left)),
        effect_item("pptx.anim.in.fly-right", C::Entrance, L::Fly(AnimationSide::Right)),
        effect_item("pptx.anim.in.fly-top", C::Entrance, L::Fly(AnimationSide::Top)),
    ];
    let emphasis = vec![effect_item("pptx.anim.em.spin", C::Emphasis, L::Spin(21_600_000)), effect_item("pptx.anim.em.grow", C::Emphasis, L::Scale(150_000, 150_000))];
    let exit = vec![
        effect_item("pptx.anim.out.disappear", C::Exit, L::Appear),
        effect_item("pptx.anim.out.fade", C::Exit, L::Fade),
        effect_item("pptx.anim.out.fly-bottom", C::Exit, L::Fly(AnimationSide::Bottom)),
        effect_item("pptx.anim.out.fly-left", C::Exit, L::Fly(AnimationSide::Left)),
        effect_item("pptx.anim.out.fly-right", C::Exit, L::Fly(AnimationSide::Right)),
        effect_item("pptx.anim.out.fly-top", C::Exit, L::Fly(AnimationSide::Top)),
    ];
    let path = vec![
        effect_item("pptx.anim.path.down", C::Path, L::Motion(0, 25_000)),
        effect_item("pptx.anim.path.left", C::Path, L::Motion(-25_000, 0)),
        effect_item("pptx.anim.path.right", C::Path, L::Motion(25_000, 0)),
        effect_item("pptx.anim.path.up", C::Path, L::Motion(0, -25_000)),
    ];
    let path = path.into_iter().chain([MenuEntry::Separator, MenuItem::tr("pptx.anim.path.custom").disabled(!editor.can_draw_path()).on(|e, _, cx| e.begin_path_draw(cx)).into()]).collect::<Vec<_>>();
    let starts = [("pptx.anim.start.click", AnimationStart::OnClick), ("pptx.anim.start.with", AnimationStart::WithPrevious), ("pptx.anim.start.after", AnimationStart::AfterPrevious)]
        .into_iter()
        .map(|(id, s)| MenuItem::tr(id).disabled(!has).checked(start == Some(s)).on(move |e, _, cx| e.set_effect_start(s, cx)).into())
        .collect();
    let durations = [("pptx.anim.duration.500", 500), ("pptx.anim.duration.1000", 1000), ("pptx.anim.duration.2000", 2000)]
        .into_iter()
        .map(|(id, ms)| MenuItem::tr(id).disabled(!has).checked(duration == Some(ms)).on(move |e, _, cx| e.set_effect_duration(ms, cx)).into())
        .collect();
    let delays = [("pptx.anim.delay.0", 0), ("pptx.anim.delay.500", 500), ("pptx.anim.delay.1000", 1000), ("pptx.anim.delay.2000", 2000)]
        .into_iter()
        .map(|(id, ms)| MenuItem::tr(id).disabled(!has).checked(delay == Some(ms)).on(move |e, _, cx| e.set_effect_delay(ms, cx)).into())
        .collect();
    let repeat = mine.first().map(|a| a.repeat_count);
    let auto_rev = mine.first().map(|a| a.auto_reverse).unwrap_or(false);
    let rewind = mine.first().map(|a| a.rewind).unwrap_or(false);
    let repeats = [
        ("pptx.anim.repeat.none", AnimationRepeat::None),
        ("pptx.anim.repeat.2", AnimationRepeat::Count(2)),
        ("pptx.anim.repeat.3", AnimationRepeat::Count(3)),
        ("pptx.anim.repeat.4", AnimationRepeat::Count(4)),
        ("pptx.anim.repeat.5", AnimationRepeat::Count(5)),
        ("pptx.anim.repeat.10", AnimationRepeat::Count(10)),
        ("pptx.anim.repeat.until-click", AnimationRepeat::UntilNextClick),
        ("pptx.anim.repeat.until-end", AnimationRepeat::UntilEndOfSlide),
    ]
    .into_iter()
    .map(|(id, r)| MenuItem::tr(id).disabled(!has).checked(repeat == Some(r)).on(move |e, _, cx| e.set_effect_repeat(r, cx)).into())
    .collect();
    let accel = mine.first().map(|a| a.accel).unwrap_or(0);
    let decel = mine.first().map(|a| a.decel).unwrap_or(0);
    let smooths = [
        ("pptx.anim.smooth.none", 0, 0),
        ("pptx.anim.smooth.start", 50_000, 0),
        ("pptx.anim.smooth.end", 0, 50_000),
        ("pptx.anim.smooth.both", 50_000, 50_000),
    ]
    .into_iter()
    .map(|(id, ac, dc)| {
        MenuItem::tr(id)
            .disabled(!has)
            .checked(accel == ac && decel == dc)
            .on(move |e, _, cx| e.set_effect_smooth(ac, dc, cx))
            .into()
    })
    .collect();
    let by_paragraph = has && mine.iter().all(|a| a.paragraph.is_some());
    let builds = vec![
        MenuItem::tr("pptx.anim.build.whole").disabled(!has).checked(has && mine.iter().all(|a| a.paragraph.is_none())).on(|e, _, cx| e.set_effect_text_build(false, cx)).into(),
        MenuItem::tr("pptx.anim.build.paragraph")
            .disabled(!has || !editor.selected_shapes().iter().any(|s| s.text_body().is_some_and(|b| b.paragraphs.len() > 1)))
            .checked(by_paragraph)
            .on(|e, _, cx| e.set_effect_text_build(true, cx))
            .into(),
    ];
    let after_effect = mine.first().map(|a| a.after_effect);
    let after_effects = [
        ("pptx.anim.after.hold", AnimationAfterEffect::Hold),
        ("pptx.anim.after.hide", AnimationAfterEffect::Hide),
        ("pptx.anim.after.dim", AnimationAfterEffect::Dim),
        ("pptx.anim.after.hide-next", AnimationAfterEffect::HideOnNextClick),
    ]
    .into_iter()
    .map(|(id, af)| {
        MenuItem::tr(id)
            .disabled(!has)
            .checked(after_effect == Some(af))
            .on(move |e, _, cx| e.set_effect_after(af, cx))
            .into()
    })
    .collect();
    let add_item = |id: &'static str, class: C, look: L| -> MenuEntry { MenuItem::new(id, crate::shell::i18n::text(&id["add.".len()..])).on(move |e, _, cx| e.add_shape_effect(class, look, cx)).into() };
    let add = vec![
        add_item("add.pptx.anim.in.appear", C::Entrance, L::Appear),
        add_item("add.pptx.anim.in.fade", C::Entrance, L::Fade),
        add_item("add.pptx.anim.in.fly-bottom", C::Entrance, L::Fly(AnimationSide::Bottom)),
        add_item("add.pptx.anim.em.spin", C::Emphasis, L::Spin(21_600_000)),
        add_item("add.pptx.anim.em.grow", C::Emphasis, L::Scale(150_000, 150_000)),
        add_item("add.pptx.anim.out.disappear", C::Exit, L::Appear),
        add_item("add.pptx.anim.out.fade", C::Exit, L::Fade),
        add_item("add.pptx.anim.out.fly-bottom", C::Exit, L::Fly(AnimationSide::Bottom)),
        add_item("add.pptx.anim.path.down", C::Path, L::Motion(0, 25_000)),
        add_item("add.pptx.anim.path.left", C::Path, L::Motion(-25_000, 0)),
        add_item("add.pptx.anim.path.right", C::Path, L::Motion(25_000, 0)),
        add_item("add.pptx.anim.path.up", C::Path, L::Motion(0, -25_000)),
    ];
    vec![
        MenuItem::tr("pptx.anim.preview").disabled(editor.slide().animations.is_empty()).on(|e, w, cx| e.start_slideshow(w, cx)).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.anim.entrance", "animation", entrance).into(),
        Submenu::tr("pptx.anim.emphasis", "animation", emphasis).into(),
        Submenu::tr("pptx.anim.exit", "animation", exit).into(),
        Submenu::tr("pptx.anim.path", "conversion_path", path).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.anim.options", "tune", options).into(),
        Submenu::tr("pptx.anim.start", "schedule", starts).into(),
        Submenu::tr("pptx.anim.duration", "schedule", durations).into(),
        Submenu::tr("pptx.anim.delay", "schedule", delays).into(),
        Submenu::tr("pptx.anim.repeat", "repeat", repeats).into(),
        MenuItem::tr("pptx.anim.autoreverse").disabled(!has).checked(auto_rev).on(|e, _, cx| e.toggle_effect_autoreverse(cx)).into(),
        MenuItem::tr("pptx.anim.rewind").disabled(!has).checked(rewind).on(|e, _, cx| e.toggle_effect_rewind(cx)).into(),
        Submenu::tr("pptx.anim.smooth", "water", smooths).into(),
        Submenu::tr("pptx.anim.after", "visibility_off", after_effects).into(),
        Submenu::tr("pptx.anim.build", "format_list_bulleted", builds).into(),
        Submenu::tr("pptx.anim.trigger", "touch_app", {
            let slide = editor.slide();
            let mut shape_click_items = Vec::new();
            for sh in &slide.shapes {
                let sh_id = sh.id;
                let label = sh.text_body()
                    .map(|b| b.text().chars().take(12).collect::<String>())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| crate::shell::i18n::format("pptx.selection-pane.shape-n", &[("n", &slide.shape_index(sh_id).map_or(0, |x| x + 1))]).to_string());
                let is_checked = has && mine.iter().all(|a| matches!(a.trigger, AnimationTrigger::OnShapeClick(s) if s == sh_id));
                shape_click_items.push(
                    MenuItem::new("pptx.anim.trigger.shape-item", label)
                        .disabled(!has)
                        .checked(is_checked)
                        .on(move |e, _, cx| e.set_selected_effects_trigger(AnimationTrigger::OnShapeClick(sh_id), cx))
                        .into(),
                );
            }

            let mut bookmark_items = Vec::new();
            for sh in &slide.shapes {
                let sh_id = sh.id;
                if let Some(clip) = SlideEditor::clip_of(sh) {
                    for bmk in &clip.options.bookmarks {
                        let bmk_name = bmk.name.clone();
                        let label = format!("{}: {:.1}s", bmk_name, bmk.time_ms as f32 / 1000.0);
                        let bmk_check = bmk_name.clone();
                        let is_checked = has && mine.iter().all(|a| matches!(&a.trigger, AnimationTrigger::OnMediaBookmark { shape, bookmark } if *shape == sh_id && bookmark == &bmk_check));
                        let bmk_set = bmk_name.clone();
                        bookmark_items.push(
                            MenuItem::new("pptx.anim.trigger.bookmark-item", label)
                                .disabled(!has)
                                .checked(is_checked)
                                .on(move |e, _, cx| e.set_selected_effects_trigger(AnimationTrigger::OnMediaBookmark { shape: sh_id, bookmark: bmk_set.clone() }, cx))
                                .into(),
                        );
                    }
                }
            }

            let mut trigger_sub: Vec<MenuEntry> = Vec::new();
            if shape_click_items.is_empty() {
                trigger_sub.push(MenuItem::tr("pptx.anim.trigger.click").disabled(true).into());
            } else {
                trigger_sub.push(Submenu::tr("pptx.anim.trigger.click", "touch_app", shape_click_items).into());
            }
            if bookmark_items.is_empty() {
                trigger_sub.push(MenuItem::tr("pptx.anim.trigger.bookmark").disabled(true).into());
            } else {
                trigger_sub.push(Submenu::tr("pptx.anim.trigger.bookmark", "bookmark", bookmark_items).into());
            }
            trigger_sub.push(MenuEntry::Separator);
            let has_any_trigger = mine.iter().any(|a| !a.trigger.is_none());
            trigger_sub.push(
                MenuItem::tr("pptx.anim.trigger.none")
                    .disabled(!has || !has_any_trigger)
                    .on(|e, _, cx| e.clear_selected_effects_trigger(cx))
                    .into(),
            );
            trigger_sub
        }).into(),
        MenuItem::tr("pptx.anim.options-dialog").disabled(!has).on(|e, _, cx| e.open_selected_animation_options_panel(cx)).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.anim.add", "add", add).into(),
        MenuItem::tr("pptx.anim.remove").disabled(!has).on(|e, _, cx| e.remove_shape_effects(cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.anim.copy").shortcut("⌥⇧⌘C").disabled(!editor.can_copy_animations()).on(|e, _, cx| e.copy_shape_animations(cx)).into(),
        MenuItem::tr("pptx.anim.paste").shortcut("⌥⇧⌘V").disabled(!editor.can_paste_animations()).on(|e, _, cx| e.paste_shape_animations(cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.anim.pane").checked(editor.pptx().is_some_and(|s| s.animation_pane)).on(|e, _, cx| e.toggle_animation_pane(cx)).into(),
    ]
}
