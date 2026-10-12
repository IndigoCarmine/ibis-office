// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing a page of a flowing document, as `flow_layout` laid it out.

use std::sync::Arc;

use gpui_kit::*;
use ibis_doc_model::{EMU_PER_POINT, EmuRect, FlowDocument};

use crate::docx::flow_layout::{Item, pages};
use crate::render::shape_view::{Resources, shape_element};

/// Page `index` of `document`, filling `frame` (frame-relative).
pub(crate) fn page_element(document: &Arc<FlowDocument>, index: usize, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    let (w, h) = (frame.width as f32 * scale, frame.height as f32 * scale);
    let pagination = pages(document, res);
    let k = scale * EMU_PER_POINT as f32;
    let mut behind_pictures = Vec::new();
    let mut front_pictures = Vec::new();
    for item in pagination.pages.get(index).into_iter().flat_map(|p| &p.items) {
        match item {
            Item::Picture { media, x, y, w, h } => {
                if let Some(image) = res.picture(*media) {
                    front_pictures.push(
                        img(image).absolute().left(px(x * k)).top(px(y * k)).w(px(w * k)).h(px(h * k)).into_any_element(),
                    );
                }
            }
            Item::Art { art, part, x, y, w, h, image_w, image_h, tint } => {
                let device = crate::docx::art_border::device_px(image_w.min(*image_h), k * crate::docx::art_border::device_scale());
                if let Some(image) = crate::docx::art_border::image(*art, *part, *tint, device) {
                    behind_pictures.push(
                        div()
                            .absolute()
                            .left(px(x * k))
                            .top(px(y * k))
                            .w(px(w * k))
                            .h(px(h * k))
                            .overflow_hidden()
                            .child(img(res.svg_picture(image)).absolute().left_0().top_0().w(px(image_w * k)).h(px(image_h * k)).object_fit(ObjectFit::Fill))
                            .into_any_element(),
                    );
                }
            }
            Item::Shape { shape, x, y, w, h } => {
                let s_frame = EmuRect::new(
                    (x * EMU_PER_POINT as f32).round() as i64,
                    (y * EMU_PER_POINT as f32).round() as i64,
                    (w * EMU_PER_POINT as f32).round() as i64,
                    (h * EMU_PER_POINT as f32).round() as i64,
                );
                let elem = shape_element(shape, s_frame, scale, res, None);
                let is_behind = shape.floating.as_ref().map_or(false, |f| f.behind_doc);
                if is_behind {
                    behind_pictures.push(elem);
                } else {
                    front_pictures.push(elem);
                }
            }
            _ => {}
        }
    }
    let print = res.print;
    // ページの色: on the screen only (Word for Mac prints none) unless print_backgrounds is enabled.
    let paper = document.page_color.filter(|_| !print || res.print_backgrounds).map_or(0xffffff, |c| c.0);
    div().absolute().left_0().top_0().w(px(w)).h(px(h)).bg(rgb(paper)).children(behind_pictures).child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let o = bounds.origin;
                // The art borders' pictures are made for the window's scale
                // (an export sets its own, its pictures made ahead for it).
                if !print && crate::docx::art_border::set_device_scale(window.scale_factor()) {
                    window.refresh();
                }
                // Art borders' pictures on their way (the web version): draw
                // again when they may have come.
                if crate::docx::art_border::pending() {
                    window.request_animation_frame();
                }
                let Some(page) = pagination.pages.get(index) else { return };
                let visible = window.content_mask().bounds;
                for item in &page.items {
                    match item {
                        Item::Rect { x, y, w, h, color } => {
                            // At least a device pixel, so thin rules show.
                            let min = 1.0 / window.scale_factor();
                            let r = Bounds::new(point(o.x + px(x * k), o.y + px(y * k)), size(px((w * k).max(min)), px((h * k).max(min))));
                            if r.intersects(&visible) {
                                window.paint_quad(fill(r, *color));
                            }
                        }
                        Item::Text { x, y, text, offsets, font, size: font_size, color } => {
                            let size_px = font_size * k;
                            let top = o.y + px(y * k - size_px);
                            if top > visible.origin.y + visible.size.height || top + px(size_px * 2.0) < visible.origin.y {
                                continue;
                            }
                            for ((i, c), dx) in text.char_indices().zip(offsets.iter()) {
                                let s = &text[i..i + c.len_utf8()];
                                if s.trim().is_empty() {
                                    continue;
                                }
                                let run = TextRun { len: s.len(), font: font.clone(), color: *color, background_color: None, underline: None, strikethrough: None };
                                let shaped = window.text_system().shape_line(SharedString::from(s.to_string()), px(size_px), &[run], None);
                                // The line box's top: the baseline less the ascent.
                                let ascent = f32::from(shaped.ascent);
                                let origin = point(o.x + px((x + dx) * k), o.y + px(y * k - ascent));
                                shaped.paint(origin, shaped.ascent + shaped.descent, TextAlign::Left, None, window, cx).ok();
                            }
                        }
                        Item::Turned { x, y, text, font, size: font_size, color, turns } => {
                            // Each glyph moved where turning the line about its
                            // start takes it, and turned about itself there.
                            let run = TextRun { len: text.len(), font: font.clone(), color: *color, background_color: None, underline: None, strikethrough: None };
                            let shaped = window.text_system().shape_line(text.clone(), px(font_size * k), &[run], None);
                            let angle = f32::from(*turns) * std::f32::consts::FRAC_PI_2;
                            let (sin, cos) = angle.sin_cos();
                            let start = point(o.x + px(x * k), o.y + px(y * k));
                            let scale = window.scale_factor();
                            for r in &shaped.runs {
                                for glyph in &r.glyphs {
                                    let (dx, dy) = (f32::from(glyph.position.x), f32::from(glyph.position.y));
                                    let turned = point(start.x + px(dx * cos - dy * sin), start.y + px(dx * sin + dy * cos));
                                    let pivot = point(ScaledPixels(f32::from(turned.x) * scale), ScaledPixels(f32::from(turned.y) * scale));
                                    let transformation = TransformationMatrix::unit()
                                        .translate(pivot)
                                        .rotate(Radians(angle))
                                        .translate(point(ScaledPixels(-pivot.x.0), ScaledPixels(-pivot.y.0)));
                                    let _ = window.paint_glyph_transformed(turned, r.font_id, glyph.id, px(font_size * k), *color, transformation);
                                }
                            }
                        }
                        Item::Stroke { x, y, x2, y2, w, color } => {
                            let mut path = PathBuilder::stroke(px((w * k).max(1.0 / window.scale_factor())));
                            path.move_to(point(o.x + px(x * k), o.y + px(y * k)));
                            path.line_to(point(o.x + px(x2 * k), o.y + px(y2 * k)));
                            if let Ok(path) = path.build() {
                                window.paint_path(path, *color);
                            }
                        }
                        Item::Picture { .. } | Item::Art { .. } | Item::Shape { .. } | Item::Line { .. } | Item::Shading { .. } => {}
                    }
                }
            },
        )
        .absolute()
        .size_full(),
    )
    .children(front_pictures)
}
