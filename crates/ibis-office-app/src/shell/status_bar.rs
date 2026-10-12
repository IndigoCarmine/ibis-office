// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The status bar: the page, the format's items, hints, and the zoom
//! (− slider + percentage ▲ fit) at the bottom right, as in the mock.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarHandle};
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::editor::{MAX_ZOOM, MIN_ZOOM, SlideEditor, Tool, zoom_levels};
use crate::shell::format::with_format;

impl SlideEditor {
    /// The status bar's texts, left to right: the page, the format's items,
    /// the hint (the zoom is not among them). For the scenario tests too.
    pub(crate) fn status_texts(&self) -> Vec<SharedString> {
        let slides = &self.presentation().slides;
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let hint: SharedString = match (self.tool, self.editing.is_some(), &self.status) {
            (Tool::TextBox, _, _) => crate::shell::i18n::text("status.hint.textbox"),
            (_, true, _) => crate::shell::i18n::text("status.hint.esc"),
            (_, _, Some(status)) => status.clone(),
            _ => "".into(),
        };
        // In the UI's language (the tables' texts by the Japanese one).
        let tr = |t: &str| crate::shell::i18n::text(t);
        let mut texts = vec![SharedString::from(format!("{} {} / {}", tr(with_format!(self, PAGE_NOUN)), index + 1, slides.len()))];
        texts.extend(with_format!(self, status_items(self)).into_iter().map(|t| tr(&t)));
        texts.push(tr(&hint));
        texts
    }

    pub(crate) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let mut texts = self.status_texts();
        let hint = texts.pop().unwrap_or_default();
        let layout = self.status_layout.clone();
        // A workbook's sheet tabs scroll with a scrollbar here, from the
        // tabs' left edge to the zoom slider.
        let tab_scrollbar = self.sheet_state().and_then(|state| {
            let scroll = state.tab_scroll.clone();
            let [bar_left, bar_right, zoom_left] = layout.get();
            let left = f32::from(scroll.bounds().left()) - bar_left;
            let right = zoom_left - bar_left - 8.0;
            // Only while the tabs scroll, and where there is room.
            let visible = bar_right > bar_left && right - left > 24.0 && scroll.max_offset().x > px(0.5);
            visible.then(|| {
                let width = px(right - left);
                let wheel = scroll.clone();
                div().absolute().top_0().h(px(12.)).left(px(left)).w(width)
                // The wheel over the bar scrolls the tabs sideways, a vertical
                // one too (as over the tabs themselves).
                .on_scroll_wheel(move |event: &ScrollWheelEvent, window, cx| {
                    let delta = event.delta.pixel_delta(px(20.));
                    let amount = if delta.x.abs() >= delta.y.abs() { delta.x } else { delta.y };
                    let (offset, max) = (wheel.offset(), wheel.max_offset().x);
                    let x = (offset.x + amount).clamp(-max, px(0.));
                    wheel.set_offset(point(x, offset.y));
                    window.refresh();
                    cx.stop_propagation();
                })
                .child(
                    div().absolute().inset_0().child(
                        Scrollbar::new(&TabsScroll { handle: scroll, width })
                            .id("sheet-tabs-scrollbar")
                            .axis(ScrollbarAxis::Horizontal)
                            .viewport_from_layout(),
                    ),
                )
            })
        });
        h_flex()
            .relative()
            .h(px(26.))
            .flex_shrink_0()
            .px_3()
            .gap_4()
            .items_center()
            .text_xs()
            .text_color(theme.muted_foreground)
            .bg(theme.title_bar)
            .border_t_1()
            .border_color(theme.title_bar_border)
            .child(crate::shell::editor::bounds_observer({
                let layout = layout.clone();
                move |bounds, window, _| {
                    let mut now = layout.get();
                    let was = now;
                    (now[0], now[1]) = (f32::from(bounds.left()), f32::from(bounds.right()));
                    layout.set(now);
                    // Drawn again once with a measure that changed (a window
                    // resized), so the scrollbar is not a frame behind.
                    if (was[0] - now[0]).abs() > 0.5 || (was[1] - now[1]).abs() > 0.5 {
                        window.refresh();
                    }
                }
            }))
            .children(texts)
            .child(div().flex_1().child(hint))
            .child(self.render_zoom(cx))
            .children(tab_scrollbar)
    }

    /// Zoom of the status bar: − / slider / +, the percentage with ▲ for
    /// the list of zooms, and fitting the slide to the window.
    fn render_zoom(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.current_zoom();
        // A little slack, so a zoom rounded onto a limit still counts as there.
        let (at_min, at_max) = (zoom <= MIN_ZOOM * 1.001, zoom >= MAX_ZOOM * 0.999);
        let layout = self.status_layout.clone();
        h_flex()
            .relative()
            .gap_1()
            .items_center()
            // Where the zoom controls begin (the − button's left edge): the
            // sheet tabs' scrollbar ends before it.
            .child(crate::shell::editor::bounds_observer(move |bounds, window, _| {
                let mut now = layout.get();
                let was = now[2];
                now[2] = f32::from(bounds.left());
                layout.set(now);
                if (was - now[2]).abs() > 0.5 {
                    window.refresh();
                }
            }))
            .child(
                Button::new("zoom-out")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Minus)
                    .tooltip(crate::shell::i18n::text("view.zoom-out"))
                    .disabled(at_min)
                    .on_click(cx.listener(|this, _, window, cx| this.step_zoom(false, window, cx))),
            )
            .child(div().w(px(120.)).child(Slider::new(&self.zoom_slider)))
            .child(
                Button::new("zoom-in")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .tooltip(crate::shell::i18n::text("view.zoom-in"))
                    .disabled(at_max)
                    .on_click(cx.listener(|this, _, window, cx| this.step_zoom(true, window, cx))),
            )
            .child(div().w(px(36.)).ml_1().text_right().child(SharedString::from(format!("{:.0}%", zoom * 100.0))))
            .child(self.render_zoom_menu(cx))
            .child(
                Button::new("zoom-fit")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Fullscreen)
                    .tooltip(crate::shell::i18n::text(with_format!(self, FIT_TOOLTIP)))
                    .selected(self.zoom.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.set_zoom(None, window, cx))),
            )
    }

    /// ▲ by the percentage, and the list of zooms it opens upwards: the
    /// zooms of `zoom_levels`, 「指定」 (a zoom typed in), and 「画面にフィット」
    /// (not for a worksheet, which is not fitted from here).
    fn render_zoom_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = cx.entity().downgrade();
        let shown = (self.current_zoom() * 100.0).round() as u32;
        let fitted = self.zoom.is_none();
        let can_fit = with_format!(self, CAN_FIT);
        let input = self.zoom_input.clone();
        let on_open_change = {
            let editor = editor.clone();
            move |open: &bool, window: &mut Window, cx: &mut App| {
                editor.update(cx, |this, cx| this.set_zoom_menu_open(*open, window, cx)).ok();
            }
        };
        Popover::new("zoom-menu")
            .anchor(Anchor::BottomRight)
            .open(self.zoom_menu_open)
            .on_open_change(on_open_change)
            .trigger(Button::new("zoom-menu-button").ghost().xsmall().label("▲").tooltip(crate::shell::i18n::text("view.zoom-menu")))
            .p_1()
            .min_w(px(132.))
            .content(move |_, _, cx| {
                let theme = cx.theme();
                let (hover, muted) = (theme.accent, theme.muted_foreground);
                // A row of the list, with a check mark if it is the zoom shown.
                let row = |id: ElementId, checked: bool| {
                    h_flex()
                        .id(id)
                        .h(px(24.))
                        .px_2()
                        .gap_2()
                        .items_center()
                        .rounded_sm()
                        .text_sm()
                        .child(div().w(px(12.)).child(if checked { "✓" } else { "" }))
                };
                let choose = |zoom: Option<f32>| {
                    let editor = editor.clone();
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        editor
                            .update(cx, |this, cx| {
                                this.set_zoom(zoom, window, cx);
                                this.set_zoom_menu_open(false, window, cx);
                            })
                            .ok();
                    }
                };
                let levels = zoom_levels().into_iter().map(|percent| {
                    row(("zoom-level", percent as usize).into(), !fitted && percent == shown)
                        .cursor_pointer()
                        .hover(|s| s.bg(hover))
                        .child(SharedString::from(format!("{percent}%")))
                        .on_click(choose(Some(percent as f32 / 100.0)))
                });
                v_flex()
                    .children(levels)
                    .child(
                        row("zoom-custom".into(), false)
                            .child(crate::shell::i18n::text("view.zoom-custom"))
                            .child(Input::new(&input).xsmall().w(px(56.)))
                            .child(div().text_color(muted).child("%")),
                    )
                    .when(can_fit, |d| {
                        d.child(
                            row("zoom-fit-item".into(), fitted)
                                .cursor_pointer()
                                .hover(|s| s.bg(hover))
                                .child(crate::shell::i18n::text("view.zoom-fit"))
                                .on_click(choose(None)),
                        )
                    })
            })
    }

    /// Open or close the list of zooms; opening puts the zoom shown in 「指定」.
    pub(crate) fn set_zoom_menu_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open {
            let percent = format!("{:.0}", self.current_zoom() * 100.0);
            self.zoom_input.update(cx, |input, cx| input.set_value(percent, window, cx));
        } else if self.zoom_menu_open {
            self.focus_handle.focus(window, cx);
        }
        self.zoom_menu_open = open;
        cx.notify();
    }
}

/// `n` with a comma every three digits, as in `12,345`.
pub(crate) fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::thousands;

    #[test]
    fn thousands_groups_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}

/// The sheet tabs' scroll handle as the scrollbar in the status bar sees it:
/// the scrollbar is `width` wide (it ends before the zoom controls), which is
/// the viewport the thumb travels in, so the content is the tabs' scrolling
/// range plus that width (the handle's own viewport is wider: the thumb would
/// stop short of the end).
#[derive(Clone)]
struct TabsScroll {
    handle: gpui::ScrollHandle,
    width: Pixels,
}

impl ScrollbarHandle for TabsScroll {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        let bounds = self.handle.bounds();
        Bounds::new(bounds.origin, gpui_kit::size(self.width, bounds.size.height))
    }

    fn offset(&self) -> Point<Pixels> {
        self.handle.offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.handle.set_offset(offset);
    }

    fn content_size(&self) -> Size<Pixels> {
        gpui_kit::size(self.width + self.handle.max_offset().x, self.handle.bounds().size.height)
    }
}
