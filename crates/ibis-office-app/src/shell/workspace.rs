// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The workspace in the middle of the window: the slide, its shapes, the
//! selection frame with resize handles, and the in-place text field.

use gpui_kit::component::input::Textarea;
use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::component::{ActiveTheme as _, Size as ControlSize};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{EMU_PER_POINT, EmuRect, ResizeHandle, Shape, ShapeKind, TextBody};

use crate::shell::editor::{SlideEditor, SlideLayout, Tool, WorkspaceContent};
use crate::render::shape_view::{LINE_HEIGHT, align_text, body_font, place, shape_element, slide_backdrop};

/// Side of a resize handle, in pixels.
const HANDLE_SIZE: f32 = 8.0;

/// Padding `Textarea` puts around its text. It always renders at the default
/// control size, so this is that size's input padding.
fn text_field_padding() -> Point<Pixels> {
    point(ControlSize::Medium.input_px(), ControlSize::Medium.input_py())
}

impl SlideEditor {
    pub(crate) fn render_workspace(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        // The scrolling in bounds before anything reads it: the wheel adds
        // its turn unchecked, and GPUI holds the scrolled view in bounds
        // only as it lays it out, after the frozen panes and the headers
        // were placed by the turn (past the top, the left pane went down
        // and back while the sheet beside it stayed: #5).
        self.clamp_workspace_scroll();
        if self.is_workbook() && self.sync_scrolling {
            self.sync_scroll_to_peer(cx);
        }
        let theme = cx.theme();
        let layout = self.slide_layout();
        let content = self.workspace_content();

        // The outer element is the workspace (focus, pointer, its bounds for
        // the layout); the inner one scrolls a slide larger than it.
        div()
            .id("workspace")
            .key_context(if self.is_workbook() { crate::shell::editor::SHEET_CONTEXT } else { crate::shell::editor::WORKSPACE_CONTEXT })
            .track_focus(&self.focus_handle)
            .on_scroll_wheel(cx.listener(Self::on_workspace_scroll))
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(theme.muted)
            .when(self.tool == Tool::TextBox, |d| d.cursor_crosshair())
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_workspace_down))
            .child(
                div()
                    .id("workspace-scroll")
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&self.workspace_scroll)
                    .when_some(layout.zip(content), |d, (layout, content)| {
                        d.child(
                            div()
                                .relative()
                                .flex_none()
                                .w(content.size.width)
                                .h(content.size.height)
                                .child(self.render_slide(layout, content, cx)),
                        )
                    }),
            )
            // Frozen panes: the sheet again, over the scrolled one, held
            // still on their axis.
            .when_some(layout.zip(content), |d, (layout, content)| d.children(self.render_frozen_panes(layout, content, cx)))
            // Under the scrollbars, which stay usable at the headers' ends.
            .when_some(content.and_then(|content| self.render_sheet_headers(content, cx)), |d, headers| {
                d.child(headers)
            })
            .children(self.render_loading(cx))
            .child(crate::shell::editor::wheel_zoom_listener(cx))
            .child(crate::shell::editor::bounds_observer(move |bounds, window, cx| {
                let changed = entity.update(cx, |this, cx| {
                    if this.workspace_bounds != Some(bounds) {
                        this.workspace_bounds = Some(bounds);
                        cx.notify();
                        true
                    } else {
                        false
                    }
                });
                if changed {
                    window.on_next_frame(move |window, _| window.refresh());
                }
            }))
            .scrollbar(&self.workspace_scroll, ScrollbarAxis::Both)
    }

    /// The frozen rows (which scroll only sideways), the frozen columns
    /// (only up and down) and their corner (not at all), each a copy of the
    /// page clipped to its part of the workspace, with a line along their
    /// edge, as Excel draws ウィンドウ枠の固定.
    fn render_frozen_panes(&self, layout: SlideLayout, content: WorkspaceContent, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some((split_x, split_y)) = self.frozen_split(content) else { return Vec::new() };
        let offset = self.workspace_scroll.offset();
        let mut panes = Vec::new();
        // Split panes (表示 > 分割) are scrolled on their own (`pane_shift`),
        // by the wheel over them; frozen ones not at all.
        let pane_shift = self.pane_shift();
        let split = self.split_panes().is_some();
        let pane = |this: &Self, id: &'static str, width: Option<f32>, height: Option<f32>, shift: Point<Pixels>, cx: &mut Context<Self>| {
            div()
                .id(id)
                .absolute()
                .top_0()
                .left_0()
                .map(|d| match width {
                    Some(w) => d.w(px(w)),
                    None => d.w_full(),
                })
                .map(|d| match height {
                    Some(h) => d.h(px(h)),
                    None => d.h_full(),
                })
                .overflow_hidden()
                // The sheet scrolled under it gets no presses, but the wheel
                // still scrolls it.
                .block_mouse_except_scroll()
                .when(split, |d| {
                    let (across, down) = (width.is_some(), height.is_some());
                    d.on_scroll_wheel(cx.listener(move |this, e: &ScrollWheelEvent, _, cx| {
                        if this.scroll_split_pane(e, across, down, cx) {
                            cx.stop_propagation();
                        }
                    }))
                })
                .child(
                    div()
                        .absolute()
                        .left(shift.x)
                        .top(shift.y)
                        .w(content.size.width)
                        .h(content.size.height)
                        .child(this.render_slide(layout, content, cx)),
                )
                .into_any_element()
        };
        if let Some(h) = split_y {
            panes.push(pane(self, "frozen-rows", None, Some(h), point(offset.x, pane_shift.y), cx));
        }
        if let Some(w) = split_x {
            panes.push(pane(self, "frozen-columns", Some(w), None, point(pane_shift.x, offset.y), cx));
        }
        if let (Some(w), Some(h)) = (split_x, split_y) {
            panes.push(pane(self, "frozen-corner", Some(w), Some(h), pane_shift, cx));
        }
        let line = rgb(crate::xlsx::sheet_ui::FROZEN_LINE);
        if let Some(h) = split_y {
            panes.push(div().absolute().left_0().top(px(h - 1.0)).w_full().h(px(1.)).bg(line).into_any_element());
        }
        if let Some(w) = split_x {
            panes.push(div().absolute().top_0().left(px(w - 1.0)).h_full().w(px(1.)).bg(line).into_any_element());
        }
        panes
    }

    fn render_slide(&self, layout: SlideLayout, content: WorkspaceContent, cx: &mut Context<Self>) -> Div {
        let slide = self.slide();
        let mut size = self.presentation().size_of(slide);
        // A worksheet's page is its whole grid, past the cells it uses.
        if let Some((shape, _)) = self.current_sheet() {
            size.width = size.width.max(shape.frame.right());
            size.height = size.height.max(shape.frame.bottom());
        }
        let scale = layout.scale;

        let mut shapes: Vec<AnyElement> =
            slide.shown_shapes().map(|shape| self.render_shape(shape, scale, cx)).collect();
        // The selected cell is drawn over the worksheet and under the charts
        // and pictures on it, as in Excel.
        if let Some(index) = slide.shown_shapes().position(|s| matches!(s.kind, ShapeKind::Sheet(_))) {
            // Each overlay goes right over the sheet (so the one put in last is lowest);
            // `overlays` counts them, to put the page lines over all of them.
            let mut overlays = 0;
            if let Some(cursor) = self.render_cell_cursor(scale, cx) {
                shapes.insert(index + 1, cursor.into_any_element());
                overlays += 1;
            }
            // Page break preview greys the sheet outside the print area,
            // under the selected cell.
            if let Some(preview) = self.render_page_break_preview(scale, false) {
                shapes.insert(index + 1, preview.into_any_element());
                overlays += 1;
            }
            // Validation circles over invalid data.
            if let Some(circles) = self.render_validation_circles(scale) {
                shapes.insert(index + 1, circles.into_any_element());
                overlays += 1;
            }
            // Trace arrows over cells (参照元・参照先のトレース).
            if let Some(arrows) = self.render_trace_arrows(scale) {
                shapes.insert(index + 1, arrows.into_any_element());
                overlays += 1;
            }
            // A chart's data ranges (データの編集), over the cells.
            if let Some(frames) = self.render_chart_ranges(scale) {
                shapes.insert(index + 1, frames.into_any_element());
                overlays += 1;
            }
            // The page lines and numbers over the cells' borders and the selected cell
            // (a manual break must show).
            if let Some(lines) = self.render_page_break_preview(scale, true) {
                shapes.insert(index + 1 + overlays, lines.into_any_element());
            }
        }

        div()
            .absolute()
            .left(content.slide_origin.x)
            .top(content.slide_origin.y)
            .w(layout.to_px(size.width))
            .h(layout.to_px(size.height))
            .when(!self.is_workbook(), |d| d.shadow_md())
            .child(slide_backdrop(slide, scale, &self.resources))
            .children(shapes)
            .when_some(self.render_selection(scale, cx), |d, sel| d.child(sel))
            .when_some(self.creation_rect(), |d, rect| {
                d.child(
                    place(div(), rect, scale)
                        .border_1()
                        .border_color(cx.theme().primary),
                )
            })
    }

    fn render_shape(&self, shape: &Shape, scale: f32, cx: &mut Context<Self>) -> AnyElement {
        let id = shape.id;
        if let ShapeKind::Sheet(_) = shape.kind {
            // The grid is the page itself: a press selects a cell, and the
            // shape is never moved, resized or removed.
            return place(div().id(("shape", id.0)), shape.frame, scale)
                .when(self.tool == Tool::Select, |d| d.cursor(CursorStyle::Crosshair))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::on_sheet_down))
                .on_mouse_down(MouseButton::Right, cx.listener(Self::on_sheet_right_down))
                .child(shape_element(shape, EmuRect { x: 0, y: 0, ..shape.frame }, scale, &self.resources, None))
                .into_any_element();
        }
        let frame = self.display_frame(shape);
        let editing = self.editing.as_ref().filter(|e| e.shape == id);
        let body = shape.text_body();
        let empty_text_box = matches!(shape.kind, ibis_doc_model::ShapeKind::TextBox(_))
            && body.is_some_and(TextBody::is_empty)
            && editing.is_none();
        let field = editing
            .zip(body)
            .map(|(edit, body)| self.render_text_field(&edit.state, body, scale, cx).into_any_element());
        // Drawn at the wrapper's origin; the wrapper takes the pointer.
        let local = EmuRect { x: 0, y: 0, ..frame };

        place(div().id(("shape", id.0)), frame, scale)
            .when(self.tool == Tool::Select && editing.is_none(), |d| d.cursor_move())
            .when(empty_text_box, |d| d.border_1().border_dashed().border_color(cx.theme().border))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| this.on_shape_down(id, event, window, cx)),
            )
            .when(self.is_workbook(), |d| d.on_mouse_down(MouseButton::Right, cx.listener(move |this, event, window, cx| this.on_sheet_shape_right_down(id, event, window, cx))))
            .child(shape_element(shape, local, scale, &self.resources, field))
            .into_any_element()
    }

    /// The text field laid over a shape while its text is edited. Its font
    /// and insets match the drawn text, and it is moved out by its own
    /// padding so the text stays in place.
    fn render_text_field(
        &self,
        state: &Entity<gpui_kit::component::input::TextareaState>,
        body: &TextBody,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let style = &body.style;
        let px_per_point = scale * EMU_PER_POINT as f32;
        let pad = text_field_padding();
        let font = body_font(style, &self.resources);
        let insets = body.insets;
        div()
            .absolute()
            .left(px(insets.left as f32 * scale) - pad.x)
            .right(px(insets.right as f32 * scale) - pad.x)
            .top(px(insets.top as f32 * scale) - pad.y)
            .bottom(px(insets.bottom as f32 * scale) - pad.y)
            .cursor_text()
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.commit_edit(window, cx);
                }
            }))
            .child(
                Textarea::new(state)
                    .appearance(false)
                    .size_full()
                    .font_family(font.family)
                    .font_weight(font.weight)
                    .text_size(px(style.font_size_pt() * body.font_scale as f32 / 100_000.0 * px_per_point))
                    .text_color(crate::render::shape_view::hsla(ibis_doc_model::Color::opaque(style.color)))
                    .line_height(relative(LINE_HEIGHT))
                    .when(style.italic, |t| t.italic())
                    .map(|t| align_text(t, style.align)),
            )
    }

    /// Selection frame and the eight resize handles.
    fn render_selection(&self, scale: f32, cx: &mut Context<Self>) -> Option<Div> {
        let shape = self.selected_shape()?;
        let id = shape.id;
        let frame = self.display_frame(shape);
        let editing = self.editing.as_ref().is_some_and(|e| e.shape == id);
        let theme = cx.theme();
        let (primary, background) = (theme.primary, theme.background);

        let outline = place(div(), frame, scale)
            .border_1()
            .border_color(primary)
            .when(editing, |d| d.border_dashed());

        let handles = ResizeHandle::ALL.map(|handle| {
            let (ax, ay) = handle.anchor();
            let x = frame.x as f32 * scale + frame.width as f32 * scale * ax as f32 / 2.0;
            let y = frame.y as f32 * scale + frame.height as f32 * scale * ay as f32 / 2.0;
            div()
                .id(("resize-handle", handle as u64))
                .absolute()
                .left(px(x - HANDLE_SIZE / 2.0))
                .top(px(y - HANDLE_SIZE / 2.0))
                .size(px(HANDLE_SIZE))
                .rounded_full()
                .border_1()
                .border_color(primary)
                .bg(background)
                .map(|d| match handle {
                    ResizeHandle::TopLeft | ResizeHandle::BottomRight => d.cursor_nwse_resize(),
                    ResizeHandle::TopRight | ResizeHandle::BottomLeft => d.cursor_nesw_resize(),
                    ResizeHandle::Top | ResizeHandle::Bottom => d.cursor_ns_resize(),
                    ResizeHandle::Left | ResizeHandle::Right => d.cursor_ew_resize(),
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event, window, cx| {
                        this.on_handle_down(id, handle, event, window, cx)
                    }),
                )
        });

        // The outline is a sibling of the handles, so it never takes clicks
        // meant for the shape beneath it.
        Some(div().absolute().size_full().top_0().left_0().child(outline).children(handles))
    }
}
