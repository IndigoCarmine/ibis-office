// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! テキスト ボックス (Word's 挿入 > テキスト ボックス): inserting a text box
//! at the caret, and the sidebar panel that edits the text of the text box
//! at the caret.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{FlowCommand, FlowPos, FlowRunKind, Rgb, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// What the panel shows of a text box: where it is, its text, size, place and text format.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextBoxInfo {
    pub(crate) at: FlowPos,
    pub(crate) text: String,
    /// Width, height and offset (points, as the panel's fields hold them).
    pub(crate) geometry: [String; 4],
    pub(crate) font_size: String,
    pub(crate) color: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) floating: bool,
    /// Whether it holds text (a text box); a drawn shape does not.
    pub(crate) has_text: bool,
    /// The fill and the line's colours (`RRGGBB`, empty for none) and the line's width (points).
    pub(crate) fill: String,
    pub(crate) line: String,
    pub(crate) line_width: String,
    /// A straight line (it has arrow ends, no fill).
    pub(crate) is_line: bool,
    /// The outline as it is (the line's, or the shape's), for a change to start from.
    pub(crate) stroke: Option<ibis_doc_model::Stroke>,
    /// The dash pattern of the outline, whether the fill is a gradient, whether the shape has a shadow.
    pub(crate) dash: ibis_doc_model::LineDash,
    pub(crate) gradient: bool,
    pub(crate) shadow: bool,
    /// A glow, soft edges, a reflection.
    pub(crate) glow: bool,
    pub(crate) soft_edge: bool,
    pub(crate) reflection: bool,
    /// Vertical anchor of the text (top, middle, bottom).
    pub(crate) anchor: ibis_doc_model::VerticalAnchor,
}

/// The panel's fields, and the text box they edit.
pub(crate) struct TextBoxState {
    pub(crate) text: Entity<TextareaState>,
    /// Width, height, left and top (points).
    pub(crate) geometry: [Entity<InputState>; 4],
    pub(crate) font_size: Entity<InputState>,
    pub(crate) color: Entity<InputState>,
    pub(crate) fill: Entity<InputState>,
    pub(crate) line: Entity<InputState>,
    pub(crate) line_width: Entity<InputState>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// A line's arrows at its start and its end.
    pub(crate) head: bool,
    pub(crate) tail: bool,
    /// The outline's dash pattern, a gradient fill, a shadow.
    pub(crate) dash: ibis_doc_model::LineDash,
    pub(crate) gradient: bool,
    pub(crate) shadow: bool,
    pub(crate) glow: bool,
    pub(crate) soft_edge: bool,
    pub(crate) reflection: bool,
    pub(crate) anchor: ibis_doc_model::VerticalAnchor,
    /// What was shown when the panel opened or last applied: the changes are those from it.
    pub(crate) initial: TextBoxInfo,
}

/// The outline's dash patterns the panel offers, in the order its button goes through them.
const DASHES: [ibis_doc_model::LineDash; 4] = [ibis_doc_model::LineDash::Solid, ibis_doc_model::LineDash::Dash, ibis_doc_model::LineDash::Dot, ibis_doc_model::LineDash::DashDot];

fn next_dash(now: ibis_doc_model::LineDash) -> ibis_doc_model::LineDash {
    DASHES.iter().position(|d| *d == now).map_or(DASHES[0], |i| DASHES[(i + 1) % DASHES.len()])
}

fn dash_label(dash: ibis_doc_model::LineDash) -> &'static str {
    match dash {
        ibis_doc_model::LineDash::Solid => "docx.text-box-panel.line-solid",
        ibis_doc_model::LineDash::Dash | ibis_doc_model::LineDash::LgDash | ibis_doc_model::LineDash::SysDash => "docx.text-box-panel.line-dash",
        ibis_doc_model::LineDash::Dot | ibis_doc_model::LineDash::SysDot => "docx.text-box-panel.line-dot",
        _ => "docx.text-box-panel.line-dash-dot",
    }
}

/// Points as the fields show them (a whole number without the decimals).
fn points(emu: i64) -> String {
    let v = emu as f64 / 12_700.0;
    if (v - v.round()).abs() < 0.005 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

fn emu(text: &str) -> Option<i64> {
    text.trim().parse::<f64>().ok().filter(|v| v.is_finite() && v.abs() < 10_000.0).map(|v| (v * 12_700.0).round() as i64)
}

/// What a drag of a floating text box does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DragMode {
    Move,
    /// A handle: which side it takes along, each axis -1 (the left / top), 0 or 1 (the right / bottom).
    Resize(i8, i8),
}

/// The wrapping a floating shape's panel offers.
const WRAPS: [(&str, &str, ibis_doc_model::PictureWrapType); 5] = [
    ("shape-wrap-square", "docx.picture.wrap.square", ibis_doc_model::PictureWrapType::Square),
    ("shape-wrap-tight", "docx.picture.wrap.tight", ibis_doc_model::PictureWrapType::Tight),
    ("shape-wrap-top-bottom", "docx.picture.wrap.top-and-bottom", ibis_doc_model::PictureWrapType::TopAndBottom),
    ("shape-wrap-behind", "docx.picture.wrap.behind-text", ibis_doc_model::PictureWrapType::BehindText),
    ("shape-wrap-front", "docx.picture.wrap.in-front-of-text", ibis_doc_model::PictureWrapType::InFrontOfText),
];

/// A drag moving or resizing a floating text box.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShapeDrag {
    pub(crate) at: FlowPos,
    pub(crate) mode: DragMode,
    pub(crate) page: usize,
    /// Where the box was on its page when the drag began (points).
    pub(crate) frame: (f32, f32, f32, f32),
    /// Where the mouse went down on the page, and how far it has moved (points).
    pub(crate) start: (f32, f32),
    pub(crate) delta: (f32, f32),
    /// The box's offset from its anchor when it began (EMU).
    pub(crate) offset: (i64, i64),
}

impl ShapeDrag {
    /// The box as the drag now has it (points): moved, or with its sides taken along.
    pub(crate) fn frame_now(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = self.frame;
        let (dx, dy) = self.delta;
        match self.mode {
            DragMode::Move => (x + dx, y + dy, w, h),
            DragMode::Resize(hx, hy) => {
                let (mut nx, mut nw) = (x, w);
                match hx {
                    -1 => {
                        nw = (w - dx).max(12.0);
                        nx = x + w - nw;
                    }
                    1 => nw = (w + dx).max(12.0),
                    _ => {}
                }
                let (mut ny, mut nh) = (y, h);
                match hy {
                    -1 => {
                        nh = (h - dy).max(12.0);
                        ny = y + h - nh;
                    }
                    1 => nh = (h + dy).max(12.0),
                    _ => {}
                }
                (nx, ny, nw, nh)
            }
        }
    }
}

/// The handles of a selected text box: the corners and the middles of the sides.
const HANDLES: &[(i8, i8)] = &[(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// A new text box's size: 6 cm × 3 cm.
const NEW_SIZE: (i64, i64) = (2_160_000, 1_080_000);

/// The shape styles: fill colour, line colour and line width (pt); an empty colour is none.
pub(crate) const SHAPE_STYLES: [(&str, &str, &str); 8] = [
    ("FFFFFF", "4472C4", "1"),
    ("D9E2F3", "4472C4", "1"),
    ("4472C4", "2F528F", "1"),
    ("ED7D31", "AE5A21", "1"),
    ("A5A5A5", "787878", "1"),
    ("FFC000", "BC8C00", "1"),
    ("70AD47", "507E32", "1"),
    ("000000", "000000", "2.25"),
];

impl SlideEditor {
    /// The text box (a shape with text) at the caret, or just before it.
    pub(crate) fn text_box_at_caret(&self) -> Option<TextBoxInfo> {
        let (start, _) = self.ordered_selection()?;
        let p = self.paragraph_of(start.para)?;
        let mut offset = 0;
        let mut previous = None;
        for run in &p.runs {
            if let FlowRunKind::Shape(shape) = &run.kind {
                let body = match &shape.kind {
                    ShapeKind::Auto(a) => a.text.as_ref(),
                    ShapeKind::TextBox(body) => Some(body),
                    _ => None,
                };
                if matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_) | ShapeKind::Line(_)) {
                    let default_body = ibis_doc_model::TextBody::default();
                    let text_body = body.unwrap_or(&default_body);
                    let first = text_body.paragraphs.iter().flat_map(|p| &p.runs).find(|r| !r.text.is_empty());
                    let bold = first.and_then(|r| r.style.bold).unwrap_or(text_body.style.bold);
                    let italic = first.and_then(|r| r.style.italic).unwrap_or(text_body.style.italic);
                    let size = first.and_then(|r| r.style.font_size).unwrap_or(text_body.style.font_size);
                    let color = first.and_then(|r| r.style.color).unwrap_or(text_body.style.color);
                    let (x, y) = shape.floating.as_ref().map_or((0, 0), |f| (f.offset_x, f.offset_y));
                    let (fill, line, line_width) = match &shape.kind {
                        ShapeKind::Auto(a) => (
                            match &a.fill {
                                ibis_doc_model::Fill::Solid(c) => format!("{:06X}", c.rgb.0 & 0xffffff),
                                _ => String::new(),
                            },
                            a.stroke.as_ref().map_or(String::new(), |s| format!("{:06X}", s.color.rgb.0 & 0xffffff)),
                            a.stroke.as_ref().map_or("1".to_string(), |s| points(s.width)),
                        ),
                        ShapeKind::Line(s) => (String::new(), format!("{:06X}", s.color.rgb.0 & 0xffffff), points(s.width)),
                        _ => (String::new(), String::new(), "1".to_string()),
                    };
                    let stroke = match &shape.kind {
                        ShapeKind::Auto(a) => a.stroke.clone(),
                        ShapeKind::Line(s) => Some(s.clone()),
                        _ => None,
                    };
                    let (gradient, shadow, glow, soft_edge, reflection) = match &shape.kind {
                        ShapeKind::Auto(a) => (matches!(a.fill, ibis_doc_model::Fill::Linear { .. }), a.shadow.is_some(), a.glow.is_some(), a.soft_edge.is_some(), a.reflection.is_some()),
                        _ => (false, false, false, false, false),
                    };
                    let dash = stroke.as_ref().map_or(ibis_doc_model::LineDash::Solid, |s| s.dash);
                    let found = Some(TextBoxInfo {
                        at: FlowPos::new(p.id, offset),
                        text: text_body.text(),
                        geometry: [points(shape.frame.width), points(shape.frame.height), points(x), points(y)],
                        font_size: format!("{}", size as f32 / 100.0),
                        color: format!("{:06X}", color.0 & 0xffffff),
                        bold,
                        italic,
                        floating: shape.floating.is_some(),
                        has_text: matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)),
                        fill,
                        line,
                        line_width,
                        is_line: matches!(shape.kind, ShapeKind::Line(_)),
                        stroke,
                        dash,
                        gradient,
                        shadow,
                        glow,
                        soft_edge,
                        reflection,
                        anchor: text_body.anchor,
                    });
                    if offset == start.offset {
                        return found;
                    }
                    if offset + 1 == start.offset {
                        previous = found;
                    }
                }
            }
            offset += run.len();
        }
        previous
    }

    /// The floating text box under the point (`x`, `y` points) of page `page`, in front of the
    /// text: where it is anchored and its offset.
    pub(crate) fn floating_text_box_at(&self, page: usize, x: f32, y: f32) -> Option<(FlowPos, (i64, i64), (f32, f32, f32, f32))> {
        let doc = self.flow()?;
        let laid = super::flow_layout::pages(doc, &self.resources);
        let page = laid.pages.get(page)?;
        for item in page.items.iter().rev() {
            let super::flow_layout::Item::Shape { shape, x: sx, y: sy, w, h } = item else { continue };
            let has_text = matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_) | ShapeKind::Line(_));
            // A line or ink stroke has little thickness: 4 points about it are on it.
            let slack = if matches!(shape.kind, ShapeKind::Line(_)) || super::ink::is_docx_ink_shape(shape) { 4.0 } else { 0.0 };
            if !has_text || x < *sx - slack || x > sx + w + slack || y < *sy - slack || y > sy + h + slack {
                continue;
            }
            for block in &doc.body {
                let ibis_doc_model::Block::Paragraph(p) = block else { continue };
                let mut offset = 0;
                for run in &p.runs {
                    if let FlowRunKind::Shape(own) = &run.kind
                        && own.id == shape.id
                        && let Some(anchor) = own.floating.as_ref().filter(|a| !a.behind_doc)
                    {
                        return Some((FlowPos::new(p.id, offset), (anchor.offset_x, anchor.offset_y), (*sx, *sy, *w, *h)));
                    }
                    offset += run.len();
                }
            }
        }
        None
    }

    /// The text box at the caret (the one selected): its page, frame (points), where it is anchored
    /// and its offset.
    pub(crate) fn selected_text_box_frame(&self) -> Option<(usize, (f32, f32, f32, f32), FlowPos, (i64, i64))> {
        let at = self.text_box_at_caret()?.at;
        let doc = self.flow()?;
        let paragraph = self.paragraph_of(at.para)?;
        let mut offset = 0;
        let mut found = None;
        for run in &paragraph.runs {
            if offset == at.offset
                && let FlowRunKind::Shape(shape) = &run.kind
                && let Some(anchor) = shape.floating.as_ref().filter(|a| !a.behind_doc)
            {
                found = Some((shape.id, (anchor.offset_x, anchor.offset_y)));
            }
            offset += run.len();
        }
        let (id, anchor_offset) = found?;
        let laid = super::flow_layout::pages(doc, &self.resources);
        for (page, pg) in laid.pages.iter().enumerate() {
            for item in &pg.items {
                if let super::flow_layout::Item::Shape { shape, x, y, w, h } = item
                    && shape.id == id
                {
                    return Some((page, (*x, *y, *w, *h), at, anchor_offset));
                }
            }
        }
        None
    }

    /// The handle of the selected text box under the point (`x`, `y` points) of page `page`.
    fn text_box_handle_at(&self, page: usize, x: f32, y: f32) -> Option<(i8, i8)> {
        let (p, (fx, fy, fw, fh), _, _) = self.selected_text_box_frame()?;
        if p != page {
            return None;
        }
        HANDLES.iter().copied().find(|&(hx, hy)| {
            let (cx, cy) = (fx + (hx + 1) as f32 * fw / 2.0, fy + (hy + 1) as f32 * fh / 2.0);
            (x - cx).abs() <= 5.0 && (y - cy).abs() <= 5.0
        })
    }

    /// A press on a floating text box: on a handle of the selected one it begins resizing, else it selects the box
    /// (the caret goes before it) and starts moving it. Whether the press was on one.
    pub(crate) fn begin_shape_drag(&mut self, page: usize, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        if let Some((hx, hy)) = self.text_box_handle_at(page, x, y)
            && let Some((_, frame, at, offset)) = self.selected_text_box_frame()
        {
            if let Some(state) = self.doc_state_mut() {
                state.shape_drag = Some(ShapeDrag { at, mode: DragMode::Resize(hx, hy), page, frame, start: (x, y), delta: (0.0, 0.0), offset });
            }
            cx.notify();
            return true;
        }
        let Some((at, offset, frame)) = self.floating_text_box_at(page, x, y) else { return false };
        self.set_caret(at, false, cx);
        if let Some(state) = self.doc_state_mut() {
            state.shape_drag = Some(ShapeDrag { at, mode: DragMode::Move, page, frame, start: (x, y), delta: (0.0, 0.0), offset });
        }
        cx.notify();
        true
    }

    /// The mouse moved to (`x`, `y`) points of the page with the box held.
    pub(crate) fn drag_shape(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if let Some(drag) = self.doc_state_mut().and_then(|s| s.shape_drag.as_mut()) {
            drag.delta = (x - drag.start.0, y - drag.start.1);
            cx.notify();
        }
    }

    /// The mouse went up: the box moves or changes size by what it was dragged (a click that moved less than
    /// 2 points only selects).
    pub(crate) fn end_shape_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.doc_state_mut().and_then(|s| s.shape_drag.take()) else { return };
        if drag.delta.0.abs() < 2.0 && drag.delta.1.abs() < 2.0 {
            cx.notify();
            return;
        }
        let (nx, ny, nw, nh) = drag.frame_now();
        let (ox, oy, _, _) = drag.frame;
        let emu = |v: f32| (v * 12_700.0) as i64;
        let offset = (drag.offset.0 + emu(nx - ox), drag.offset.1 + emu(ny - oy));
        let size = (drag.mode != DragMode::Move).then(|| ibis_doc_model::EmuSize::new(emu(nw), emu(nh)));
        let keep = self.text_selection();
        if self.run_flow(vec![FlowCommand::SetShapeGeometry { at: drag.at, size, offset: Some(offset) }, FlowCommand::RerouteConnectors], cx)
            && let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut())
        {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
        cx.notify();
    }

    /// The selected text box's handles, and the outline of the box while it is dragged, on page `index`
    /// (`k` pixels to the point).
    pub(crate) fn text_box_overlay(&self, index: usize, k: f32) -> Vec<AnyElement> {
        let mut out = Vec::new();
        out.extend(self.draw_table_overlay(index, k));
        out.extend(self.wrap_points_overlay(index, k));
        out.extend(self.docx_ink_overlay(index, k));
        let blue = rgb(0x1a73e8);
        if let Some(drag) = self.doc_state().and_then(|s| s.shape_drag).filter(|d| d.page == index && (d.delta.0 != 0.0 || d.delta.1 != 0.0)) {
            let (x, y, w, h) = drag.frame_now();
            out.push(div().absolute().left(px(x * k)).top(px(y * k)).w(px(w * k)).h(px(h * k)).border_1().border_color(blue).bg(hsla(0.6, 0.8, 0.5, 0.12)).into_any_element());
            return out;
        }
        let Some((page, (x, y, w, h), _, _)) = self.selected_text_box_frame() else { return out };
        if page != index {
            return out;
        }
        out.push(div().absolute().left(px(x * k)).top(px(y * k)).w(px(w * k)).h(px(h * k)).border_1().border_color(blue).into_any_element());
        for &(hx, hy) in HANDLES {
            let (cx, cy) = (x + (hx + 1) as f32 * w / 2.0, y + (hy + 1) as f32 * h / 2.0);
            out.push(div().absolute().left(px(cx * k - 3.0)).top(px(cy * k - 3.0)).w(px(6.0)).h(px(6.0)).bg(rgb(0xffffff)).border_1().border_color(blue).into_any_element());
        }
        out
    }

    /// 挿入 > テキスト ボックス: an empty text box at the caret, and its
    /// panel for the text.
    pub(crate) fn doc_insert_text_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let size = ibis_doc_model::EmuSize::new(NEW_SIZE.0, NEW_SIZE.1);
        if self.run_flow(vec![FlowCommand::InsertTextBox { at: caret, size, text: String::new() }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            self.open_text_box(window, cx);
        }
    }

    /// 挿入 > 図形: a floating shape of preset `preset` at the caret (Word's blue with a darker line), selected.
    /// The command that inserts a preset shape (`size` in EMU) at `caret`.
    pub(crate) fn shape_command(preset: &str, caret: FlowPos, size: ibis_doc_model::EmuSize) -> Option<FlowCommand> {
        let geometry = ibis_pptx::preset_geometry(preset)?;
        Some(FlowCommand::InsertShape { at: caret, size, geometry, fill: Some(Rgb(0x4472C4)), line: Some((Rgb(0x2F528F), 12_700)) })
    }
    pub(crate) fn doc_insert_shape(&mut self, preset: &str, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(command) = Self::shape_command(preset, caret, ibis_doc_model::EmuSize::new(1_440_000, 1_080_000)) else { return };
        if self.run_flow(vec![command], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
        }
    }
    /// The command that inserts a connector (a line when `geometry` is `None`) at `caret`.
    pub(crate) fn connector_command(geometry: Option<ibis_doc_model::Geometry>, arrow: bool, caret: FlowPos, size: ibis_doc_model::EmuSize, flip_h: bool, flip_v: bool, glue: ibis_doc_model::Glue) -> FlowCommand {
        let mut stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(Rgb(0x2F528F)), 19_050);
        if arrow {
            stroke.tail = ibis_doc_model::LineEnd::parse(Some("triangle"), None, None);
        }
        FlowCommand::InsertConnector { at: caret, size, stroke, geometry, flip_h, flip_v, glue }
    }
    /// 挿入 > 図形 > 直線・コネクタ clicked on the page: at the usual size, at the caret.
    pub(crate) fn doc_insert_connector(&mut self, preset: Option<&str>, arrow: bool, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let geometry = preset.and_then(ibis_pptx::preset_geometry);
        let size = if geometry.is_some() { ibis_doc_model::EmuSize::new(2_160_000, 1_080_000) } else { ibis_doc_model::EmuSize::new(2_160_000, 0) };
        let command = Self::connector_command(geometry, arrow, caret, size, false, false, Default::default());
        if self.run_flow(vec![command], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
        }
    }

    /// Replace the text of the text box at `at`.
    pub(crate) fn set_text_box_text(&mut self, at: FlowPos, text: &str, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetShapeText { at, text: text.to_string() }], cx);
        cx.notify();
    }

    /// 挿入 > テキスト ボックスの文字…: the panel, with the text box at the caret.
    pub(crate) fn open_text_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(info) = self.text_box_at_caret() else {
            self.status = Some(crate::shell::i18n::text("docx.text-box-panel.place-cursor-before-after-text"));
            cx.notify();
            return;
        };
        let text = cx.new(|cx| TextareaState::new(window, cx));
        text.update(cx, |a, cx| {
            a.set_value(info.text.clone(), window, cx);
            a.focus(window, cx);
        });
        let field = |value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let value = value.to_string();
            cx.new(|cx| {
                let mut s = InputState::new(window, cx);
                s.set_value(value, window, cx);
                s
            })
        };
        let geometry = [field(&info.geometry[0], window, cx), field(&info.geometry[1], window, cx), field(&info.geometry[2], window, cx), field(&info.geometry[3], window, cx)];
        let font_size = field(&info.font_size, window, cx);
        let color = field(&info.color, window, cx);
        let fill = field(&info.fill, window, cx);
        let line = field(&info.line, window, cx);
        let line_width = field(&info.line_width, window, cx);
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.comments = None;
            state.ruby = None;
            state.helper = Default::default();
            state.text_box = Some(TextBoxState { text, geometry, font_size, color, fill, line, line_width, bold: info.bold, italic: info.italic, head: info.stroke.as_ref().is_some_and(|s| s.head != ibis_doc_model::LineEnd::None), tail: info.stroke.as_ref().is_some_and(|s| s.tail != ibis_doc_model::LineEnd::None), dash: info.dash, gradient: info.gradient, shadow: info.shadow, glow: info.glow, soft_edge: info.soft_edge, reflection: info.reflection, anchor: info.anchor, initial: info });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The panel's changes made to its text box, as one step: the text, the size and place, the
    /// text's size, colour, bold and italic.
    pub(crate) fn apply_text_box(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.text_box.as_ref()) else { return };
        let value = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let text = state.text.read(cx).value().to_string();
        let geometry = [value(&state.geometry[0]), value(&state.geometry[1]), value(&state.geometry[2]), value(&state.geometry[3])];
        let (font_size, color) = (value(&state.font_size), value(&state.color));
        let (bold, italic) = (state.bold, state.italic);
        let initial = state.initial.clone();
        let at = initial.at;
        let mut commands = Vec::new();
        if text != initial.text {
            commands.push(FlowCommand::SetShapeText { at, text: text.clone() });
        }
        let size = (geometry[0] != initial.geometry[0] || geometry[1] != initial.geometry[1])
            .then(|| emu(&geometry[0]).zip(emu(&geometry[1])).filter(|(w, h)| *w > 0 && *h > 0).map(|(w, h)| ibis_doc_model::EmuSize::new(w, h)))
            .flatten();
        let offset = (initial.floating && (geometry[2] != initial.geometry[2] || geometry[3] != initial.geometry[3]))
            .then(|| emu(&geometry[2]).zip(emu(&geometry[3])))
            .flatten();
        if size.is_some() || offset.is_some() {
            commands.push(FlowCommand::SetShapeGeometry { at, size, offset });
            commands.push(FlowCommand::RerouteConnectors);
        }
        let new_size = (font_size != initial.font_size).then(|| font_size.parse::<f32>().ok().filter(|v| (1.0..=1638.0).contains(v)).map(|v| (v * 2.0).round() as u32 * 50)).flatten();
        let new_color = (color != initial.color).then(|| u32::from_str_radix(color.trim_start_matches('#'), 16).ok().filter(|_| color.trim_start_matches('#').len() == 6).map(Rgb)).flatten();
        let (new_bold, new_italic) = ((bold != initial.bold).then_some(bold), (italic != initial.italic).then_some(italic));
        // The fill and the line (an empty colour: none).
        let hex = |t: &str| (t.trim_start_matches('#').len() == 6).then(|| u32::from_str_radix(t.trim_start_matches('#'), 16).ok()).flatten().map(Rgb);
        let (fill_now, line_now, width_now) = (value(&state.fill), value(&state.line), value(&state.line_width));
        let new_fill = (fill_now != initial.fill).then(|| if fill_now.is_empty() { Some(None) } else { hex(&fill_now).map(Some) }).flatten();
        let width = emu(&width_now).filter(|w| *w > 0).unwrap_or(12_700);
        let new_line = (line_now != initial.line || width_now != initial.line_width).then(|| if line_now.is_empty() { Some(None) } else { hex(&line_now).map(|c| Some((c, width))) }).flatten();
        let (head_now, tail_now) = (state.head, state.tail);
        let ends_before = (initial.stroke.as_ref().is_some_and(|s| s.head != ibis_doc_model::LineEnd::None), initial.stroke.as_ref().is_some_and(|s| s.tail != ibis_doc_model::LineEnd::None));
        if initial.is_line {
            // A line: its whole outline, with the colour, width and arrows the panel says.
            if let Some(mut stroke) = initial.stroke.clone() {
                let changed = line_now != initial.line || width_now != initial.line_width || (head_now, tail_now) != ends_before;
                if changed {
                    if let Some(c) = hex(&line_now) {
                        stroke.color = ibis_doc_model::Color::opaque(c);
                    }
                    stroke.width = width;
                    let arrow = |on: bool, was: ibis_doc_model::LineEnd| if !on { ibis_doc_model::LineEnd::None } else if was != ibis_doc_model::LineEnd::None { was } else { ibis_doc_model::LineEnd::parse(Some("triangle"), None, None) };
                    stroke.head = arrow(head_now, stroke.head);
                    stroke.tail = arrow(tail_now, stroke.tail);
                    commands.push(FlowCommand::SetShapeStroke { at, stroke: Some(stroke) });
                }
            }
        } else if new_fill.is_some() || new_line.is_some() {
            commands.push(FlowCommand::SetShapeStyle { at, fill: new_fill, line: new_line });
        }
        // The dash pattern of the outline, a gradient fill, a shadow (after the fill and the line, which start them again).
        if state.dash != initial.dash && let Some(base) = initial.stroke.clone().or_else(|| hex(&line_now).map(|c| ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(c), width))) {
            let mut stroke = base;
            if let Some(c) = hex(&line_now) {
                stroke.color = ibis_doc_model::Color::opaque(c);
            }
            stroke.width = width;
            commands.push(FlowCommand::SetShapeStroke { at, stroke: Some(stroke.with_dash(state.dash)) });
        }
        let effects_changed = state.shadow != initial.shadow || state.glow != initial.glow || state.soft_edge != initial.soft_edge || state.reflection != initial.reflection;
        if !initial.is_line && (state.gradient != initial.gradient || effects_changed) {
            let colour = hex(&fill_now).or_else(|| hex(&initial.fill)).unwrap_or(Rgb(0x4472C4));
            let fill = (state.gradient != initial.gradient).then(|| {
                if state.gradient {
                    // From the colour to a lighter one, top to bottom.
                    let lighter = Rgb(((colour.0 >> 16 & 255) + (255 - (colour.0 >> 16 & 255)) / 2) << 16 | ((colour.0 >> 8 & 255) + (255 - (colour.0 >> 8 & 255)) / 2) << 8 | ((colour.0 & 255) + (255 - (colour.0 & 255)) / 2));
                    ibis_doc_model::Fill::Linear { angle: 5_400_000, scaled: false, stops: vec![ibis_doc_model::GradientStop { position: 0, color: ibis_doc_model::Color::opaque(colour) }, ibis_doc_model::GradientStop { position: 100_000, color: ibis_doc_model::Color::opaque(lighter) }] }
                } else {
                    ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(colour))
                }
            });
            let shadow = (state.shadow != initial.shadow).then(|| state.shadow.then(|| ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb: Rgb(0), alpha: 102 }, 50_800, 38_100, 2_700_000)));
            let glow = (state.glow != initial.glow).then(|| state.glow.then(|| (ibis_doc_model::Color { rgb: colour, alpha: 102 }, 63_500)));
            let soft_edge = (state.soft_edge != initial.soft_edge).then(|| state.soft_edge.then_some(63_500));
            let reflection = (state.reflection != initial.reflection).then(|| state.reflection.then(|| ibis_doc_model::Reflection::standard(0, 45_500)));
            commands.push(FlowCommand::SetShapeEffects { at, fill, shadow, glow, soft_edge, reflection });
        }
        if new_size.is_some() || new_color.is_some() || new_bold.is_some() || new_italic.is_some() {
            commands.push(FlowCommand::SetShapeTextFormat { at, bold: new_bold, italic: new_italic, size: new_size, color: new_color });
        }
        if state.anchor != initial.anchor {
            commands.push(FlowCommand::SetShapeAnchor { at, anchor: state.anchor });
        }
        if commands.is_empty() {
            return;
        }
        let keep = self.text_selection();
        if self.run_flow(commands, cx) {
            if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
            }
            if let Some(info) = self.text_box_at_caret()
                && let Some(state) = self.doc_state_mut().and_then(|s| s.text_box.as_mut())
            {
                state.initial = info;
            }
        }
        cx.notify();
    }

    /// The panel's font size, colour, bold and italic given to the characters selected in its text only (DOCX-IMAGE-015); the
    /// rest of the text keeps its formatting. One step of undo.
    pub(crate) fn apply_text_box_selection(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.text_box.as_ref()) else { return };
        let text = state.text.read(cx).value().to_string();
        let range = state.text.read(cx).selected_range();
        let (from, to) = (text[..range.start.min(text.len())].chars().count(), text[..range.end.min(text.len())].chars().count());
        if from >= to {
            return;
        }
        let value = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let (font_size, color) = (value(&state.font_size), value(&state.color));
        let (bold, italic) = (state.bold, state.italic);
        let initial = state.initial.clone();
        let at = initial.at;
        let size = font_size.parse::<f32>().ok().filter(|v| (1.0..=1638.0).contains(v)).map(|v| (v * 2.0).round() as u32 * 50);
        let color = (color.trim_start_matches('#').len() == 6).then(|| u32::from_str_radix(color.trim_start_matches('#'), 16).ok().map(Rgb)).flatten();
        let mut commands = Vec::new();
        if text != initial.text {
            commands.push(FlowCommand::SetShapeText { at, text: text.clone() });
        }
        commands.push(FlowCommand::SetShapeRangeFormat { at, from, to, bold: Some(bold), italic: Some(italic), size, color });
        let keep = self.text_selection();
        if self.run_flow(commands, cx) {
            if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
            }
            if let Some(info) = self.text_box_at_caret()
                && let Some(state) = self.doc_state_mut().and_then(|s| s.text_box.as_mut())
            {
                state.initial = info;
            }
        }
        cx.notify();
    }

    /// 文字列の折り返し of the text box or shape at the caret (it floats: not inline).
    pub(crate) fn set_shape_wrap(&mut self, wrap: ibis_doc_model::PictureWrapType, cx: &mut Context<Self>) {
        let Some(info) = self.text_box_at_caret() else { return };
        if self.run_flow(vec![FlowCommand::SetPictureWrap { at: info.at, wrap }], cx) {
            cx.notify();
        }
    }

    /// 図形のスタイル (DOCX-IMAGE-024): a choice of fill, line colour and line width; given to the panel's fields and applied in
    /// one step.
    pub(crate) fn apply_shape_style(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(&(fill, line, width)) = SHAPE_STYLES.get(index) else { return };
        let Some(state) = self.doc_state().and_then(|s| s.text_box.as_ref()) else { return };
        let (fill_input, line_input, width_input) = (state.fill.clone(), state.line.clone(), state.line_width.clone());
        fill_input.update(cx, |i, cx| i.set_value(fill, window, cx));
        line_input.update(cx, |i, cx| i.set_value(line, window, cx));
        width_input.update(cx, |i, cx| i.set_value(width, window, cx));
        self.apply_text_box(cx);
    }

    pub(crate) fn text_box_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.text_box.as_ref()?;
        let (text, geometry, font_size, color, bold, italic, floating) = (state.text.clone(), state.geometry.clone(), state.font_size.clone(), state.color.clone(), state.bold, state.italic, state.initial.floating);
        let (fill, line, line_width, has_text, is_line) = (state.fill.clone(), state.line.clone(), state.line_width.clone(), state.initial.has_text, state.initial.is_line);
        let (head, tail) = (state.head, state.tail);
        let (dash, gradient, shadow) = (state.dash, state.gradient, state.shadow);
        let (glow, soft_edge, reflection) = (state.glow, state.soft_edge, state.reflection);
        let row = |label: &'static str, input: &Entity<InputState>| h_flex().gap_2().child(div().w(px(64.)).text_sm().child(crate::shell::i18n::text(label))).child(Input::new(input).small());
        let toggle = |id: &'static str, label: &'static str, on: bool, f: fn(&mut TextBoxState)| {
            Button::new(id).small().label(crate::shell::i18n::text(label)).selected(on).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(state) = this.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                    f(state);
                }
                cx.notify();
            }))
        };
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .children(has_text.then(|| Textarea::new(&text).h(px(120.))))
            .child(row("docx.picture-panel.width-pt", &geometry[0]))
            .child(row("docx.picture-panel.height-pt", &geometry[1]))
            .children(floating.then(|| row("docx.text-box-panel.left-pt", &geometry[2])))
            .children(floating.then(|| row("docx.text-box-panel.top-pt", &geometry[3])))
            .children(floating.then(|| h_flex().flex_wrap().gap_1().children(WRAPS.iter().map(|&(id, label, wrap)| Button::new(id).small().label(crate::shell::i18n::text(label)).on_click(cx.listener(move |this, _, _, cx| this.set_shape_wrap(wrap, cx)))))))
            .children((!is_line).then(|| row("docx.text-box-panel.fill-color", &fill)))
            .child(row("docx.text-box-panel.outline-color", &line))
            .child(row("docx.text-box-panel.line-width-pt", &line_width))
            .child(h_flex().gap_1().child(Button::new("line-dash").small().label(format!("{}: {}", crate::shell::i18n::text("docx.text-box-panel.line-style"), crate::shell::i18n::text(dash_label(dash)))).on_click(cx.listener(|this, _, _, cx| {
                if let Some(state) = this.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                    state.dash = next_dash(state.dash);
                }
                cx.notify();
            }))))
            .children((!is_line).then(|| h_flex().gap_1().child(toggle("shape-gradient", "docx.text-box-panel.gradient", gradient, |s| s.gradient = !s.gradient)).child(toggle("shape-shadow", "docx.text-box-panel.shadow", shadow, |s| s.shadow = !s.shadow))))
            .children((!is_line).then(|| h_flex().gap_1().child(toggle("shape-glow", "docx.text-box-panel.glow", glow, |s| s.glow = !s.glow)).child(toggle("shape-soft-edge", "docx.text-box-panel.soft-edge", soft_edge, |s| s.soft_edge = !s.soft_edge)).child(toggle("shape-reflection", "docx.text-box-panel.reflection", reflection, |s| s.reflection = !s.reflection))))
            .children((!is_line).then(|| v_flex().gap_1().child(div().text_sm().child(crate::shell::i18n::text("docx.text-box-panel.shape-styles"))).child(h_flex().gap_1().flex_wrap().children(SHAPE_STYLES.iter().enumerate().map(|(i, &(f, l, _))| {
                let hex = |c: &str| u32::from_str_radix(c, 16).unwrap_or(0);
                div().id(("shape-style", i)).w(px(26.)).h(px(20.)).rounded_sm().border_2().border_color(rgb(hex(l))).bg(rgb(hex(f))).cursor_pointer().on_click(cx.listener(move |this, _, window, cx| this.apply_shape_style(i, window, cx)))
            }))))
            )
            .children(is_line.then(|| h_flex().gap_1().child(toggle("line-head", "docx.text-box-panel.arrow-start", head, |s| s.head = !s.head)).child(toggle("line-tail", "docx.text-box-panel.arrow-end", tail, |s| s.tail = !s.tail))))
            .children(has_text.then(|| row("docx.text-box-panel.font-size", &font_size)))
            .children(has_text.then(|| row("docx.text-box-panel.text-color", &color)))
            .children(has_text.then(|| {
                let anchor = state.anchor;
                h_flex().gap_1().children([
                    (ibis_doc_model::VerticalAnchor::Top, "docx.shape.align-text.top"),
                    (ibis_doc_model::VerticalAnchor::Middle, "docx.shape.align-text.middle"),
                    (ibis_doc_model::VerticalAnchor::Bottom, "docx.shape.align-text.bottom"),
                ].into_iter().map(|(a, label)| {
                    Button::new(label).small().label(crate::shell::i18n::text(label)).selected(anchor == a).on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                            state.anchor = a;
                        }
                        this.set_shape_anchor(a, cx);
                    }))
                }))
            }))
            .children(has_text.then(|| h_flex().gap_1().child(toggle("text-box-bold", "_shell.bold", bold, |s| s.bold = !s.bold)).child(toggle("text-box-italic", "docx.common.italic", italic, |s| s.italic = !s.italic))))
            .children(has_text.then(|| h_flex().gap_1().child(Button::new("text-box-apply-selection").small().label(crate::shell::i18n::text("docx.text-box-panel.apply-selection")).on_click(cx.listener(|this, _, _, cx| this.apply_text_box_selection(cx))))))
            .child(h_flex().gap_1().child(Button::new("text-box-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|this, _, _, cx| this.apply_text_box(cx)))));
        Some(SidebarPanel { title: crate::shell::i18n::text("_shell.text-box"), body: body.into_any_element() })
    }
}
