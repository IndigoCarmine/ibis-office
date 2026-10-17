// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shapes, text boxes, and pictures on a worksheet: insertion, ordering, and manipulation.

use gpui_kit::*;
use ibis_doc_model::{
    CellValue, Color, Command, Emu, EmuRect, Fill, LineDash, Paragraph, Run, RunStyle,
    Shadow, Shape, ShapeId, ShapeKind, Stroke, TextBody, TextStyle, TileAlign,
};
use crate::shell::editor::SlideEditor;
use crate::shell::editor::sheet_of;

/// Default preset shapes available in the worksheet Insert > Shapes menu:
/// (preset, item id, label, icon).
pub(crate) const SHEET_SHAPES: &[(&str, &str, &str, &str)] = &[
    ("rect", "xlsx.shape.rect", "", "rectangle"),
    ("roundRect", "xlsx.shape.round-rect", "", "rounded_corner"),
    ("ellipse", "xlsx.shape.ellipse", "", "circle"),
    ("triangle", "xlsx.shape.triangle", "", "change_history"),
    ("rightArrow", "xlsx.shape.right-arrow", "", "arrow_forward"),
    ("star5", "xlsx.shape.star5", "", "star"),
    ("wedgeRectCallout", "xlsx.shape.callout", "", "chat_bubble"),
    ("heart", "xlsx.shape.heart", "", "favorite"),
];

impl SlideEditor {
    /// Calculate insertion coordinates (x, y in EMU) on the worksheet.
    /// Prefers the top-left of the currently selected cell, or a default visible offset.
    pub(crate) fn sheet_insertion_origin(&self) -> (i64, i64) {
        if let Some(((col, row), (shape, sheet))) = self.current_cell().zip(self.current_sheet()) {
            let xs = sheet.column_offsets();
            let ys = sheet.row_offsets();
            let left = shape.frame.x + xs.get(col as usize).copied().unwrap_or(0);
            let top = shape.frame.y + ys.get(row as usize).copied().unwrap_or(0);
            (left, top)
        } else {
            (952_500, 952_500) // ~100px from top-left (100 * 9525 EMU)
        }
    }

    /// Insert a text box on the worksheet at the current selection.
    pub(crate) fn insert_sheet_textbox(&mut self, window: &mut Window, cx: &mut Context<Self>) -> ShapeId {
        let (x, y) = self.sheet_insertion_origin();
        let frame = EmuRect::new(x, y, 2_000_000, 1_000_000);
        let id = self.document.ids().new_shape_id();
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        let shape = Shape::text_box(id, frame, "");
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.selection = Some(id);
        self.begin_edit(id, true, window, cx);
        cx.notify();
        id
    }

    /// Insert an auto shape preset on the worksheet.
    pub(crate) fn insert_sheet_shape(&mut self, preset: &str, cx: &mut Context<Self>) -> ShapeId {
        let (x, y) = self.sheet_insertion_origin();
        let frame = EmuRect::new(x, y, 2_000_000, 1_200_000);
        let id = self.document.ids().new_shape_id();
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        let shape = crate::pptx::editing::new_shape(id, preset, frame);
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.selection = Some(id);
        cx.notify();
        id
    }

    /// Insert a line or arrow line on the worksheet.
    pub(crate) fn insert_sheet_line(&mut self, arrow: bool, cx: &mut Context<Self>) -> ShapeId {
        let (x, y) = self.sheet_insertion_origin();
        let frame = EmuRect::new(x, y, 2_000_000, 0);
        let id = self.document.ids().new_shape_id();
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        let shape = crate::pptx::editing::new_line(id, frame, false, false, arrow);
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.selection = Some(id);
        cx.notify();
        id
    }

    /// Insert an image file on the worksheet as a floating picture.
    pub(crate) fn insert_sheet_picture(&mut self, name: &str, data: Vec<u8>, cx: &mut Context<Self>) -> Option<ShapeId> {
        let format = ibis_doc_model::MediaFormat::from_path(name);
        let (w, h) = crate::pptx::editing::validated_picture_size(format, &data)?;
        let (x, y) = self.sheet_insertion_origin();
        // 96 DPI: 1 px = 9,525 EMU. Cap initial dimensions to 4,000,000 EMU (~420px).
        let max_dim = 4_000_000i64;
        let mut w_emu = w as i64 * 9_525;
        let mut h_emu = h as i64 * 9_525;
        if w_emu > max_dim || h_emu > max_dim {
            let scale = (max_dim as f64 / w_emu as f64).min(max_dim as f64 / h_emu as f64);
            w_emu = (w_emu as f64 * scale) as i64;
            h_emu = (h_emu as f64 * scale) as i64;
        }
        let frame = EmuRect::new(x, y, w_emu, h_emu);
        let media = self.document.ids().add_media(format, data.clone());
        self.resources.add_picture(media, format, &data);
        let id = self.document.ids().new_shape_id();
        let picture = ibis_doc_model::Picture {
            media,
            crop: Default::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: None,
            alt_text: Default::default(),
            decorative: false,
            name: String::new(),
            shadow: None,
            reflection: None,
            soft_edge: None,
            ole: None,
        };
        let shape = Shape::new(id, frame, ShapeKind::Picture(picture));
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.selection = Some(id);
        cx.notify();
        Some(id)
    }

    /// Pick a picture file asynchronously and insert it onto the worksheet.
    pub(crate) fn pick_sheet_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_shell.insert").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let name = path.to_string_lossy().into_owned();
                this.update(cx, |this, cx| {
                    this.insert_sheet_picture(&name, data, cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
                this.update(cx, |this, cx| {
                    this.insert_sheet_picture(&name, data, cx);
                })
                .ok();
            })
            .detach();
        }
    }

    /// Order a selected worksheet shape (front, back, forward, backward).
    pub(crate) fn order_sheet_shape(&mut self, order: crate::pptx::editing::Order, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let slide = self.current_slide;
        let shapes = &self.slide().shapes;
        let Some(pos) = shapes.iter().position(|s| s.id == shape_id) else { return };
        // shapes[0] is the sheet itself. Drawing shapes occupy positions 1..shapes.len().
        if shapes.len() <= 2 {
            return; // At most one floating shape; ordering changes nothing.
        }
        let new_pos = match order {
            crate::pptx::editing::Order::Front => shapes.len() - 1,
            crate::pptx::editing::Order::Back => 1,
            crate::pptx::editing::Order::Forward => (pos + 1).min(shapes.len() - 1),
            crate::pptx::editing::Order::Backward => if pos > 1 { pos - 1 } else { 1 },
        };
        if new_pos != pos {
            self.run_sheet(vec![Command::MoveShape { slide, shape: shape_id, index: new_pos }], cx);
        }
    }

    /// Align a selected worksheet shape (Left, Center, Right, Top, Middle, Bottom).
    pub(crate) fn align_sheet_shape(&mut self, align: crate::pptx::editing::Align, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let slide = self.current_slide;
        let shapes = &self.slide().shapes;
        let Some(target) = shapes.iter().find(|s| s.id == shape_id) else { return };

        // Floating shapes are shapes[1..]
        let floating: Vec<&Shape> = shapes.iter().skip(1).collect();
        if floating.is_empty() {
            return;
        }

        let bounds = if floating.len() > 1 {
            let min_x = floating.iter().map(|s| s.frame.x).min().unwrap_or(0);
            let min_y = floating.iter().map(|s| s.frame.y).min().unwrap_or(0);
            let max_r = floating.iter().map(|s| s.frame.right()).max().unwrap_or(0);
            let max_b = floating.iter().map(|s| s.frame.bottom()).max().unwrap_or(0);
            EmuRect::new(min_x, min_y, (max_r - min_x).max(0), (max_b - min_y).max(0))
        } else {
            let (origin_x, origin_y) = self.sheet_insertion_origin();
            EmuRect::new(origin_x, origin_y, target.frame.width, target.frame.height)
        };

        let mut new_frame = target.frame;
        match align {
            crate::pptx::editing::Align::Left => new_frame.x = bounds.x,
            crate::pptx::editing::Align::Center => new_frame.x = bounds.x + (bounds.width - target.frame.width) / 2,
            crate::pptx::editing::Align::Right => new_frame.x = bounds.right() - target.frame.width,
            crate::pptx::editing::Align::Top => new_frame.y = bounds.y,
            crate::pptx::editing::Align::Middle => new_frame.y = bounds.y + (bounds.height - target.frame.height) / 2,
            crate::pptx::editing::Align::Bottom => new_frame.y = bounds.bottom() - target.frame.height,
        }

        if new_frame != target.frame {
            self.run_sheet(vec![Command::SetFrame { slide, shape: shape_id, frame: new_frame }], cx);
        }
    }

    /// Distribute worksheet shapes evenly (horizontal or vertical) across 3 or more shapes.
    pub(crate) fn distribute_sheet_shapes(&mut self, horizontal: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut floating: Vec<Shape> = self.slide().shapes.iter().skip(1).cloned().collect();
        if floating.len() < 3 {
            return;
        }

        if horizontal {
            floating.sort_by_key(|s| s.frame.x);
            let first_x = floating[0].frame.x;
            let last_r = floating.last().map(|s| s.frame.right()).unwrap_or(first_x);
            let total_w: i64 = floating.iter().map(|s| s.frame.width).sum();
            let gap = (last_r - first_x - total_w) as f64 / (floating.len() - 1) as f64;

            let mut at = first_x as f64;
            let mut commands = Vec::new();
            for shape in floating {
                let target_x = at.round() as i64;
                if shape.frame.x != target_x {
                    let mut frame = shape.frame;
                    frame.x = target_x;
                    commands.push(Command::SetFrame { slide, shape: shape.id, frame });
                }
                at += shape.frame.width as f64 + gap;
            }
            if !commands.is_empty() {
                self.run_sheet(commands, cx);
            }
        } else {
            floating.sort_by_key(|s| s.frame.y);
            let first_y = floating[0].frame.y;
            let last_b = floating.last().map(|s| s.frame.bottom()).unwrap_or(first_y);
            let total_h: i64 = floating.iter().map(|s| s.frame.height).sum();
            let gap = (last_b - first_y - total_h) as f64 / (floating.len() - 1) as f64;

            let mut at = first_y as f64;
            let mut commands = Vec::new();
            for shape in floating {
                let target_y = at.round() as i64;
                if shape.frame.y != target_y {
                    let mut frame = shape.frame;
                    frame.y = target_y;
                    commands.push(Command::SetFrame { slide, shape: shape.id, frame });
                }
                at += shape.frame.height as f64 + gap;
            }
            if !commands.is_empty() {
                self.run_sheet(commands, cx);
            }
        }
    }

    /// 選択中の図形の塗りつぶしを変更.
    pub(crate) fn fill_sheet_shape(&mut self, fill: Fill, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(shape_id) else { return };
        if !matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_)) {
            return;
        }
        self.run_sheet(vec![Command::SetFill { slide, shape: shape_id, fill }], cx);
    }

    /// 選択中の図形の枠線の色を変更（None で枠線なし）.
    pub(crate) fn stroke_sheet_shape_color(&mut self, color: Option<Color>, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(shape_id) else { return };
        let current_stroke = match &shape.kind {
            ShapeKind::Auto(a) => a.stroke.clone(),
            ShapeKind::Picture(p) => p.stroke.clone(),
            ShapeKind::Line(l) => Some(l.clone()),
            _ => None,
        };
        let new_stroke = match (current_stroke, color) {
            (_, None) => None,
            (Some(mut s), Some(c)) => {
                s.color = c;
                Some(s)
            }
            (None, Some(c)) => Some(Stroke::solid(c, Stroke::DEFAULT_WIDTH)),
        };
        self.run_sheet(vec![Command::SetStroke { slide, shape: shape_id, stroke: new_stroke }], cx);
    }

    /// 選択中の図形の枠線の太さを変更.
    pub(crate) fn stroke_sheet_shape_width(&mut self, width: Emu, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(shape_id) else { return };
        let current_stroke = match &shape.kind {
            ShapeKind::Auto(a) => a.stroke.clone(),
            ShapeKind::Picture(p) => p.stroke.clone(),
            ShapeKind::Line(l) => Some(l.clone()),
            _ => None,
        };
        let mut s = current_stroke.unwrap_or_else(|| Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH));
        s.width = width;
        self.run_sheet(vec![Command::SetStroke { slide, shape: shape_id, stroke: Some(s) }], cx);
    }

    /// 選択中の図形の枠線のダッシュパターン（実線・破線・点線）を変更.
    pub(crate) fn stroke_sheet_shape_dash(&mut self, dash: LineDash, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(shape_id) else { return };
        let current_stroke = match &shape.kind {
            ShapeKind::Auto(a) => a.stroke.clone(),
            ShapeKind::Picture(p) => p.stroke.clone(),
            ShapeKind::Line(l) => Some(l.clone()),
            _ => None,
        };
        let s = current_stroke.unwrap_or_else(|| Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH)).with_dash(dash);
        self.run_sheet(vec![Command::SetStroke { slide, shape: shape_id, stroke: Some(s) }], cx);
    }

    /// 選択中の図形の影（Shadow）効果を設定.
    pub(crate) fn shadow_sheet_shape(&mut self, shadow: Option<Shadow>, cx: &mut Context<Self>) {
        let Some(shape_id) = self.selected_sheet_shape() else { return };
        let slide = self.current_slide;
        let Some(shape) = self.slide().shape(shape_id) else { return };
        let ShapeKind::Auto(auto) = &shape.kind else { return };
        if auto.shadow == shadow {
            return;
        }
        let mut new_auto = auto.clone();
        new_auto.shadow = shadow;
        self.run_sheet(vec![Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Auto(new_auto)) }], cx);
    }

    /// 単一の図形を選択.
    pub(crate) fn select_sheet_shape(&mut self, id: ShapeId) {
        self.set_selection(vec![id]);
    }

    /// 複数の図形を一括選択.
    #[allow(dead_code)]
    pub(crate) fn select_sheet_shapes(&mut self, ids: &[ShapeId]) {
        self.set_selection(ids.to_vec());
    }

    /// 図形の選択状態をトグル（Shift+クリック等用）.
    pub(crate) fn toggle_select_sheet_shape(&mut self, id: ShapeId) {
        let mut ids = self.selected_ids();
        if ids.contains(&id) {
            ids.retain(|&x| x != id);
        } else {
            ids.push(id);
        }
        self.set_selection(ids);
    }

    /// 図形選択を全解除.
    #[allow(dead_code)]
    pub(crate) fn clear_sheet_shape_selection(&mut self) {
        self.set_selection(Vec::new());
    }

    /// テキストボックス・図形内の文字の太字をトグル.
    pub(crate) fn format_sheet_text_bold(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_characters(window, cx, |b, r| b.with_run(r).bold, |on| ibis_doc_model::CharacterFormat { bold: Some(on), ..Default::default() });
    }

    /// テキストボックス・図形内の文字の斜体をトグル.
    pub(crate) fn format_sheet_text_italic(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_characters(window, cx, |b, r| b.with_run(r).italic, |on| ibis_doc_model::CharacterFormat { italic: Some(on), ..Default::default() });
    }

    /// テキストボックス・図形内の文字の下線をトグル.
    pub(crate) fn format_sheet_text_underline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_characters(window, cx, |_, r| r.underline, |on| ibis_doc_model::CharacterFormat { underline: Some(on), ..Default::default() });
    }

    /// テキストボックス・図形内の文字の取り消し線をトグル.
    pub(crate) fn format_sheet_text_strikethrough(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.strike_characters(window, cx);
    }

    /// テキストボックス・図形内の文字色を変更.
    pub(crate) fn format_sheet_text_color(&mut self, color: ibis_doc_model::Rgb, window: &mut Window, cx: &mut Context<Self>) {
        self.format_characters(window, cx, ibis_doc_model::CharacterFormat { color: Some(color), ..Default::default() });
    }

    /// テキストボックス・図形内のフォントサイズを変更 (pt 単位).
    pub(crate) fn format_sheet_text_size(&mut self, size_pt: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.format_characters(window, cx, ibis_doc_model::CharacterFormat { font_size: Some(size_pt * 100), ..Default::default() });
    }

    /// テキストボックス・図形内の段落配置を変更.
    pub(crate) fn format_sheet_text_align(&mut self, align: ibis_doc_model::TextAlign, window: &mut Window, cx: &mut Context<Self>) {
        self.format_paragraphs(window, cx, ibis_doc_model::ParagraphFormat { align: Some(align), ..Default::default() });
    }

    /// テキストボックス・図形の上下方向（垂直）の配置を変更.
    pub(crate) fn format_sheet_text_vertical(&mut self, anchor: ibis_doc_model::VerticalAnchor, cx: &mut Context<Self>) {
        self.format_text_body(|b| b.anchor = anchor, cx);
    }

    /// テキストボックス・図形の文字列の方向を変更.
    pub(crate) fn format_sheet_text_direction(&mut self, vertical: bool, sideways: i32, cx: &mut Context<Self>) {
        self.format_text_body(|b| {
            b.vertical = vertical;
            if vertical {
                b.vertical_kind = ibis_doc_model::VerticalKind::EastAsian;
            }
            b.sideways = sideways;
        }, cx);
    }

    /// テキストボックス・図形の内余白を変更.
    pub(crate) fn format_sheet_text_insets(&mut self, insets: ibis_doc_model::Insets, cx: &mut Context<Self>) {
        self.format_text_body(|b| b.insets = insets, cx);
    }

    /// テキストボックス・図形内のテキスト折り返しをトグル.
    pub(crate) fn format_sheet_text_wrap(&mut self, cx: &mut Context<Self>) {
        self.format_text_body(|b| b.wrap = !b.wrap, cx);
    }

    /// 選択中の図形・画像・グラフのセルに対する配置方式を変更.
    pub(crate) fn set_sheet_shape_placement(&mut self, placement: ibis_doc_model::ObjectPlacement, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut ids = self.selected_ids();
        if ids.is_empty() {
            if let Some(id) = self.selected_sheet_shape() {
                ids.push(id);
            }
        }
        if ids.is_empty() { return; }
        let commands = ids.into_iter().map(|shape| ibis_doc_model::Command::SetShapePlacement { slide, shape, placement }).collect();
        self.run_sheet(commands, cx);
    }

    /// 選択中の図形・画像・グラフの印刷指定を変更.
    pub(crate) fn set_sheet_shape_print(&mut self, print: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut ids = self.selected_ids();
        if ids.is_empty() {
            if let Some(id) = self.selected_sheet_shape() {
                ids.push(id);
            }
        }
        if ids.is_empty() { return; }
        let commands = ids.into_iter().map(|shape| ibis_doc_model::Command::SetShapePrint { slide, shape, print }).collect();
        self.run_sheet(commands, cx);
    }

    /// 選択中の図形・画像・グラフの印刷指定をトグル.
    pub(crate) fn toggle_sheet_shape_print(&mut self, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut ids = self.selected_ids();
        if ids.is_empty() {
            if let Some(id) = self.selected_sheet_shape() {
                ids.push(id);
            }
        }
        if ids.is_empty() { return; }
        let cur_print = self.slide().shape_prints.get(&ids[0]).copied().unwrap_or(true);
        let new_print = !cur_print;
        let commands = ids.into_iter().map(|shape| ibis_doc_model::Command::SetShapePrint { slide, shape, print: new_print }).collect();
        self.run_sheet(commands, cx);
    }

    /// 選択中の図形の形状（プリセット geometry）を変更する (XLSX-OBJECT-002).
    pub(crate) fn change_sheet_shape(&mut self, preset: &str, cx: &mut Context<Self>) {
        let Some(geometry) = ibis_pptx::preset_geometry(preset) else { return };
        let slide = self.current_slide;
        let mut ids = self.selected_ids();
        if ids.is_empty() {
            if let Some(id) = self.selected_sheet_shape() {
                ids.push(id);
            }
        }
        let mut commands = Vec::new();
        for id in ids {
            let Some(shape) = self.slide().shape(id) else { continue };
            match &shape.kind {
                ShapeKind::Auto(a) if a.geometry != geometry => {
                    let mut next = a.clone();
                    next.geometry = geometry.clone();
                    commands.push(Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Auto(next)) });
                }
                ShapeKind::TextBox(tb) => {
                    let auto = ibis_doc_model::AutoShape {
                        geometry: geometry.clone(),
                        text: Some(tb.clone()),
                        fill: Fill::None,
                        stroke: None,
                        shadow: None,
                        tilt: None,
                        glow: None,
                        inner_shadow: None,
                        soft_edge: None,
                        reflection: None,
                        bevel: None,
                    };
                    commands.push(Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Auto(auto)) });
                }
                _ => {}
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
    }

    /// 選択中の図形・テキストボックスにセル参照数式（textlink）を設定（または解除）する。
    pub(crate) fn set_selected_shape_formula(&mut self, formula: Option<&str>, cx: &mut Context<Self>) {
        let selected_ids = self.selected_ids();
        let Some(&shape_id) = selected_ids.first() else { return };
        let slide_id = self.current_slide;
        let formula_string = formula.map(|f| {
            let trimmed = f.trim();
            if trimmed.starts_with('=') { trimmed[1..].trim().to_string() } else { trimmed.to_string() }
        }).filter(|s| !s.is_empty());

        let mut commands = vec![Command::SetShapeFormula {
            slide: slide_id,
            shape: shape_id,
            formula: formula_string.clone(),
        }];

        if let Some(ref f) = formula_string {
            if let Some((sheet_name_opt, col, row)) = parse_shape_formula(f) {
                let p = self.presentation();
                let target_slide = match sheet_name_opt {
                    Some(ref name) => p.slides.iter().find(|s| s.name.as_deref().unwrap_or("").eq_ignore_ascii_case(name)),
                    None => p.slide(slide_id),
                };
                if let Some(target_slide) = target_slide {
                    if let Some((_, sheet)) = sheet_of(target_slide) {
                        let text = cell_display_text(sheet.cell(row, col));
                        let body = ibis_doc_model::TextBody::plain(&text, ibis_doc_model::TextStyle::default());
                        commands.push(Command::SetTextBody {
                            slide: slide_id,
                            shape: shape_id,
                            body,
                        });
                    }
                }
            }
        }

        self.run_sheet(commands, cx);
    }

    /// 選択中の図形を現在選択されているセルと連動させる。
    pub(crate) fn link_selected_shape_to_current_cell(&mut self, cx: &mut Context<Self>) {
        let Some((col, row)) = self.current_cell() else { return };
        let ref_name = ibis_doc_model::cell_name(col, row);
        self.set_selected_shape_formula(Some(&ref_name), cx);
    }

    /// 選択中の図形のセル連動を解除する。
    pub(crate) fn clear_selected_shape_formula(&mut self, cx: &mut Context<Self>) {
        self.set_selected_shape_formula(None, cx);
    }

    /// ワークシート上の全図形の連動式（shape_formulas）を最新のセル値と同期する。
    pub(crate) fn sync_shape_formulas(&mut self, cx: &mut Context<Self>) {
        let p = self.presentation();
        let slide_id = self.current_slide;
        let Some(slide) = p.slide(slide_id) else { return };
        if slide.shape_formulas.is_empty() {
            return;
        }

        let mut updates: Vec<(ibis_doc_model::ShapeId, String)> = Vec::new();
        for (&shape_id, formula) in &slide.shape_formulas {
            let Some((sheet_name_opt, col, row)) = parse_shape_formula(formula) else { continue };
            let target_slide = match sheet_name_opt {
                Some(ref name) => p.slides.iter().find(|s| s.name.as_deref().unwrap_or("").eq_ignore_ascii_case(name)),
                None => Some(slide),
            };
            let Some(target_slide) = target_slide else { continue };
            let Some((_, sheet)) = sheet_of(target_slide) else { continue };
            let text = cell_display_text(sheet.cell(row, col));
            updates.push((shape_id, text));
        }

        if updates.is_empty() {
            return;
        }

        let mut commands = Vec::new();
        for (shape_id, text) in updates {
            let Some(shape) = slide.shape(shape_id) else { continue };
            let current_text = shape.text_body().map(|b| b.text()).unwrap_or_default();
            if current_text != text {
                let body = ibis_doc_model::TextBody::plain(&text, ibis_doc_model::TextStyle::default());
                commands.push(Command::SetTextBody {
                    slide: slide_id,
                    shape: shape_id,
                    body,
                });
            }
        }

        if !commands.is_empty() {
            self.run_sheet(commands, cx);
        }
    }

    /// ワードアート (WordArt) をワークシートに挿入する。
    pub(crate) fn insert_sheet_wordart(&mut self, style_idx: usize, text: Option<&str>, cx: &mut Context<Self>) -> ShapeId {
        let preset = crate::pptx::editing::WORDART_PRESETS.get(style_idx).copied().unwrap_or(crate::pptx::editing::WORDART_PRESETS[0]);
        let id = self.document.ids().new_shape_id();
        let (x, y) = self.sheet_insertion_origin();
        let width = 3_810_000i64;
        let height = 952_500i64;
        let frame = EmuRect::new(x, y, width, height);

        let default_text = crate::shell::i18n::text("_xlsx.wordart-default-text");
        let initial_text = text.unwrap_or(&default_text);
        let mut style = TextStyle::default().with_font_size(3600);
        style.color = preset.color;
        style.bold = preset.bold;

        let shadow = preset.shadow.map(|(dir, dist, blur)| Shadow {
            color: Color { rgb: ibis_doc_model::Rgb(0), alpha: 102 },
            blur,
            distance: dist,
            direction: dir,
            sx: 100_000,
            sy: 100_000,
            kx: 0,
            ky: 0,
            align: TileAlign::TopLeft,
            rot_with_shape: true,
        });

        let run_style = RunStyle {
            font_size: Some(3600),
            bold: Some(preset.bold),
            color: Some(preset.color),
            outline: preset.outline,
            shadow,
            ..Default::default()
        };

        let mut body = TextBody::default();
        body.style = style;
        body.paragraphs = vec![Paragraph {
            runs: vec![Run {
                text: initial_text.to_string(),
                style: run_style,
            }],
            ..Default::default()
        }];

        let shape = Shape::new(id, frame, ShapeKind::TextBox(body));
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }], cx);
        self.selection = Some(id);
        cx.notify();
        id
    }

    /// 選択中のテキストまたは図形にワードアートスタイルを適用する。
    pub(crate) fn set_selected_sheet_wordart_style(&mut self, style_idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.set_selected_text_wordart_style(style_idx, window, cx);
        self.sync_workbook_peers(cx);
    }

    /// 文字の変形 (`a:prstTxWarp`) を設定・解除する。
    pub(crate) fn set_sheet_text_warp(&mut self, warp: Option<&str>, cx: &mut Context<Self>) {
        self.set_text_warp(warp, cx);
        self.sync_workbook_peers(cx);
    }

    /// 選択中のテキストボックス・図形の文字変形プリセット名を取得する。
    pub(crate) fn selected_sheet_text_warp(&self) -> Option<String> {
        self.selected_text_warp()
    }
}

fn cell_display_text(cell: Option<&ibis_doc_model::SheetCell>) -> String {
    cell.map_or(String::new(), |c| {
        if !c.text.is_empty() {
            c.text.clone()
        } else {
            match c.value {
                CellValue::Text(ref t) => t.clone(),
                CellValue::Number(n) => ibis_calc::eval::number_to_text(n),
                CellValue::Bool(b) => if b { "TRUE".into() } else { "FALSE".into() },
                CellValue::Error(e) => e.as_str().to_string(),
                _ => String::new(),
            }
        }
    })
}

/// Presets for changing shapes in the worksheet Format > Change Shape menu:
/// (preset, item id, label, icon).
pub(crate) const CHANGE_SHAPES: &[(&str, &str, &str, &str)] = &[
    ("rect", "xlsx.change-shape.rect", "", "rectangle"),
    ("roundRect", "xlsx.change-shape.round-rect", "", "rounded_corner"),
    ("ellipse", "xlsx.change-shape.ellipse", "", "circle"),
    ("triangle", "xlsx.change-shape.triangle", "", "change_history"),
    ("rtTriangle", "xlsx.change-shape.rt-triangle", "", "change_history"),
    ("diamond", "xlsx.change-shape.diamond", "", "diamond"),
    ("pentagon", "xlsx.change-shape.pentagon", "", "pentagon"),
    ("hexagon", "xlsx.change-shape.hexagon", "", "hexagon"),
    ("rightArrow", "xlsx.change-shape.right-arrow", "", "arrow_forward"),
    ("leftArrow", "xlsx.change-shape.left-arrow", "", "arrow_back"),
    ("upArrow", "xlsx.change-shape.up-arrow", "", "arrow_upward"),
    ("downArrow", "xlsx.change-shape.down-arrow", "", "arrow_downward"),
    ("star5", "xlsx.change-shape.star5", "", "star"),
    ("wedgeRectCallout", "xlsx.change-shape.callout", "", "chat_bubble"),
    ("cloud", "xlsx.change-shape.cloud", "", "cloud"),
    ("heart", "xlsx.change-shape.heart", "", "favorite"),
];

/// セル参照数式文字列（`=A1`, `Sheet1!$B$3`, `A1` 等）を解析する。
/// 返り値: (Option<シート名>, column: u32, row: u32)
pub(crate) fn parse_shape_formula(raw: &str) -> Option<(Option<String>, u32, u32)> {
    let s = raw.strip_prefix('=').unwrap_or(raw).trim();
    if s.is_empty() {
        return None;
    }
    if let Some((sheet_part, cell_part)) = s.split_once('!') {
        let sheet_name = sheet_part.trim().trim_matches('\'').trim().to_string();
        let corner = ibis_calc::formula::parser::parse_cell(cell_part.trim())?;
        Some((Some(sheet_name), corner.column as u32, corner.row as u32))
    } else {
        let corner = ibis_calc::formula::parser::parse_cell(s)?;
        Some((None, corner.column as u32, corner.row as u32))
    }
}


