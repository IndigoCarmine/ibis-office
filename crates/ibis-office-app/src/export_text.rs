// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A line of text drawn on a page picture (the header, date, footer and
//! number of a printed handout), with the system's own font and its fallback
//! for the other scripts (CoreText, drawn into a CoreGraphics bitmap).

use core_foundation::attributed_string::CFMutableAttributedString;
use core_foundation::base::{CFRange, TCFType};
use core_foundation::string::CFString;
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::CGContext;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_graphics::path::CGPath;
use core_text::font::new_ui_font_for_language;
use core_text::framesetter::CTFramesetter;
use core_text::line::CTLine;
use core_text::string_attributes::kCTFontAttributeName;
use image::{Rgba, RgbaImage};

/// Where a line sits against the `x` given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Center,
    Right,
}

/// `text`, `size` pixels high, in black on `image`: its left (or right) end at `x`, the top of its letters
/// at `top`.
pub(crate) fn draw_text(image: &mut RgbaImage, text: &str, x: f32, top: f32, size: f32, align: Align) {
    if text.is_empty() {
        return;
    }
    // The system's own UI font (kCTFontUIFontSystem = 2): the other scripts come from its fallback.
    let font = new_ui_font_for_language(2, size as f64, None);
    let mut string = CFMutableAttributedString::new();
    let cf = CFString::new(text);
    string.replace_str(&cf, CFRange::init(0, 0));
    let length = string.char_len();
    string.set_attribute(CFRange::init(0, length), unsafe { kCTFontAttributeName }, &font);
    let line = CTLine::new_with_attributed_string(string.as_concrete_TypeRef());
    let bounds = line.get_typographic_bounds();
    let width = bounds.width.ceil() as usize + 4;
    let height = (bounds.ascent + bounds.descent + bounds.leading).ceil() as usize + 4;
    // Drawn in black on transparent: CoreGraphics' rows go from the bottom.
    let mut buffer = vec![0u8; width * height * 4];
    {
        let space = CGColorSpace::create_device_rgb();
        // kCGImageAlphaPremultipliedLast
        let context = CGContext::create_bitmap_context(Some(buffer.as_mut_ptr() as *mut std::ffi::c_void), width, height, 8, width * 4, &space, 1);
        context.set_rgb_fill_color(0.0, 0.0, 0.0, 1.0);
        context.set_text_position(2.0, 2.0 + bounds.descent);
        line.draw(&context);
    }
    let left = match align {
        Align::Left => x,
        Align::Center => x - (width as f32 - 4.0) / 2.0,
        Align::Right => x - (width as f32 - 4.0),
    };
    let (ox, oy) = ((left - 2.0).round() as i64, (top - 2.0).round() as i64);
    for row in 0..height {
        // The buffer's first row is the top of the picture.
        let source = row * width * 4;
        for column in 0..width {
            let alpha = buffer[source + column * 4 + 3] as u32;
            if alpha == 0 {
                continue;
            }
            let (px, py) = (ox + column as i64, oy + row as i64);
            if px < 0 || py < 0 || px >= image.width() as i64 || py >= image.height() as i64 {
                continue;
            }
            // Black with alpha over what is there.
            let under = image.get_pixel(px as u32, py as u32).0;
            let mix = |c: u8| ((c as u32 * (255 - alpha)) / 255) as u8;
            image.put_pixel(px as u32, py as u32, Rgba([mix(under[0]), mix(under[1]), mix(under[2]), 255]));
        }
    }
}

/// The notes of one slide as one text to lay out: paragraphs (a bullet, a level's indent) of
/// runs (bold, size, colour), broken into lines, and over as many boxes (pages) as it takes.
pub(crate) struct NoteFlow {
    setter: CTFramesetter,
    // The text the framesetter was made of.
    _string: CFMutableAttributedString,
    length: isize,
    at: isize,
}

impl NoteFlow {
    /// `None` for notes with no text. A size is `base_pt` unless a run has its own (`px` per point).
    pub(crate) fn new(text: &crate::export_print::NoteText, px_per_pt: f32) -> Option<Self> {
        use core_graphics::color::CGColor;
        use core_text::string_attributes::kCTForegroundColorAttributeName;
        let mut plain = String::new();
        // (UTF-16 start, length, run) of each run, once the text is in.
        let mut spans: Vec<(isize, isize, &crate::export_print::NoteRun)> = Vec::new();
        let mut at = 0isize;
        let mut push = |plain: &mut String, text: &str, at: &mut isize| -> (isize, isize) {
            let start = *at;
            plain.push_str(text);
            let n = text.encode_utf16().count() as isize;
            *at += n;
            (start, n)
        };
        let mut first = true;
        for paragraph in &text.paragraphs {
            if !first {
                push(&mut plain, "\n", &mut at);
            }
            first = false;
            let lead = format!("{}{}", "    ".repeat(paragraph.level as usize), paragraph.bullet.as_ref().map(|b| format!("{b} ")).unwrap_or_default());
            push(&mut plain, &lead, &mut at);
            for run in &paragraph.runs {
                let (start, n) = push(&mut plain, &run.text, &mut at);
                spans.push((start, n, run));
            }
        }
        if plain.trim().is_empty() {
            return None;
        }
        let mut string = CFMutableAttributedString::new();
        string.replace_str(&CFString::new(&plain), CFRange::init(0, 0));
        let length = string.char_len();
        let base = new_ui_font_for_language(2, 12.0 * px_per_pt as f64, None);
        string.set_attribute(CFRange::init(0, length), unsafe { kCTFontAttributeName }, &base);
        for (start, n, run) in spans {
            if n == 0 {
                continue;
            }
            let size = run.size_pt.unwrap_or(12.0) as f64 * px_per_pt as f64;
            // 2 is the UI's text font, 3 its emphasized (bold) one.
            let font = new_ui_font_for_language(if run.bold { 3 } else { 2 }, size, None);
            let range = CFRange::init(start, n);
            string.set_attribute(range, unsafe { kCTFontAttributeName }, &font);
            if let Some((r, g, b)) = run.color {
                let color = CGColor::rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0);
                string.set_attribute(range, unsafe { kCTForegroundColorAttributeName }, &color);
            }
        }
        let setter = CTFramesetter::new_with_attributed_string(string.as_concrete_TypeRef());
        Some(Self { setter, _string: string, length, at: 0 })
    }

    /// Whether there is text not drawn yet.
    pub(crate) fn has_more(&self) -> bool {
        self.at < self.length
    }

    /// The text from where the last box ended, in the box `(x, y, width, height)` of `image`, as much
    /// as fits (the next box takes the rest).
    pub(crate) fn draw_next(&mut self, image: &mut RgbaImage, (x, y, width, height): (f32, f32, f32, f32)) {
        let (w, h) = (width.round() as usize, height.round() as usize);
        if !self.has_more() || w < 8 || h < 8 {
            return;
        }
        let remaining = CFRange::init(self.at, self.length - self.at);
        let (_, fit) = self.setter.suggest_frame_size_with_constraints(remaining, std::ptr::null(), CGSize::new(w as f64, h as f64));
        // Always on: a box too small for a line takes a piece, so the text ends.
        let take = if fit.length > 0 { fit.length } else { remaining.length.min(40) };
        let range = CFRange::init(self.at, take);
        self.at += take;
        let path = CGPath::from_rect(CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(w as f64, h as f64)), None);
        let frame = self.setter.create_frame(range, &path);
        let mut buffer = vec![0u8; w * h * 4];
        {
            let space = CGColorSpace::create_device_rgb();
            let context = CGContext::create_bitmap_context(Some(buffer.as_mut_ptr() as *mut std::ffi::c_void), w, h, 8, w * 4, &space, 1);
            context.set_rgb_fill_color(0.0, 0.0, 0.0, 1.0);
            frame.draw(&context);
        }
        let (ox, oy) = (x.round() as i64, y.round() as i64);
        for row in 0..h {
            for column in 0..w {
                let o = (row * w + column) * 4;
                let alpha = buffer[o + 3] as u32;
                if alpha == 0 {
                    continue;
                }
                let (px, py) = (ox + column as i64, oy + row as i64);
                if px < 0 || py < 0 || px >= image.width() as i64 || py >= image.height() as i64 {
                    continue;
                }
                // The buffer holds premultiplied colour over transparent: over what is there.
                let under = image.get_pixel(px as u32, py as u32).0;
                let mix = |u: u8, c: u8| (((u as u32 * (255 - alpha)) / 255) + c as u32).min(255) as u8;
                image.put_pixel(px as u32, py as u32, Rgba([mix(under[0], buffer[o]), mix(under[1], buffer[o + 1]), mix(under[2], buffer[o + 2]), 255]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{draw_text, Align, NoteFlow};
    use image::{Rgba, RgbaImage};

    fn dark(image: &RgbaImage) -> Vec<(u32, u32)> {
        image.enumerate_pixels().filter(|(_, _, p)| p.0[0] < 128).map(|(x, y, _)| (x, y)).collect()
    }

    #[test]
    fn notes_are_broken_into_lines_and_boxes() {
        use crate::export_print::{NotePara, NoteRun, NoteText};
        let run = |text: &str, bold: bool, size: Option<f32>, color: Option<(u8, u8, u8)>| NoteRun { text: text.into(), bold, size_pt: size, color };
        let text = NoteText {
            paragraphs: vec![
                NotePara { level: 0, bullet: None, runs: vec![run("The quick brown fox jumps over the lazy dog and runs on and on across the whole field. ", false, None, None), run("二つ目の段落です。", true, Some(16.0), Some((200, 0, 0)))] },
                NotePara { level: 1, bullet: Some("•".into()), runs: vec![run("A bulleted line, one level in, with more words to fill a few lines of the box.", false, None, None)] },
            ],
        };
        let mut flow = NoteFlow::new(&text, 1.0).unwrap();
        // A box with room for part of it: the rest is for the next box.
        let mut first = RgbaImage::from_pixel(300, 200, Rgba([255, 255, 255, 255]));
        flow.draw_next(&mut first, (20.0, 20.0, 200.0, 40.0));
        let marks = dark(&first);
        assert!(marks.len() > 100 && flow.has_more(), "part of it fits: {} {}", marks.len(), flow.has_more());
        let max_y = marks.iter().map(|m| m.1).max().unwrap();
        assert!(max_y <= 62, "inside the box: {max_y}");
        let mut second = RgbaImage::from_pixel(300, 200, Rgba([255, 255, 255, 255]));
        flow.draw_next(&mut second, (20.0, 20.0, 200.0, 150.0));
        assert!(!flow.has_more(), "all of it is drawn");
        assert!(dark(&second).len() > 100);
        // The red bold run: red pixels somewhere in the two.
        let red = |i: &RgbaImage| i.pixels().filter(|p| p[0] > 150 && p[1] < 80 && p[2] < 80).count();
        assert!(red(&first) + red(&second) > 20, "coloured");
        assert!(NoteFlow::new(&NoteText::default(), 1.0).is_none());
    }

    #[test]
    fn text_is_drawn_where_it_is_put() {
        let mut page = RgbaImage::from_pixel(400, 100, Rgba([255, 255, 255, 255]));
        draw_text(&mut page, "Header 123", 20.0, 30.0, 24.0, Align::Left);
        let marks = dark(&page);
        assert!(marks.len() > 100, "{}", marks.len());
        let (min_x, max_x) = (marks.iter().map(|m| m.0).min().unwrap(), marks.iter().map(|m| m.0).max().unwrap());
        assert!((15..=30).contains(&min_x), "left {min_x}");
        assert!(max_x < 200, "right {max_x}");
        let (min_y, max_y) = (marks.iter().map(|m| m.1).min().unwrap(), marks.iter().map(|m| m.1).max().unwrap());
        assert!(min_y >= 25 && max_y < 75, "{min_y}..{max_y}");
        // Right-aligned: it ends at the x given; Japanese is drawn too.
        let mut right = RgbaImage::from_pixel(400, 100, Rgba([255, 255, 255, 255]));
        draw_text(&mut right, "社外秘 2026/10/11", 380.0, 30.0, 24.0, Align::Right);
        let marks = dark(&right);
        assert!(marks.len() > 100);
        let max_x = marks.iter().map(|m| m.0).max().unwrap();
        assert!((370..=384).contains(&max_x), "{max_x}");
        // The right way up: a T has its bar at the top.
        let mut t = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        draw_text(&mut t, "T", 40.0, 30.0, 40.0, Align::Left);
        let marks = dark(&t);
        let (top, bottom) = (marks.iter().map(|m| m.1).min().unwrap(), marks.iter().map(|m| m.1).max().unwrap());
        let third = (bottom - top) / 3;
        let wide = |from: u32, to: u32| marks.iter().filter(|m| m.1 >= from && m.1 < to).count();
        assert!(wide(top, top + third) > 2 * wide(bottom - third, bottom + 1), "the bar of the T is at the top");
        // Nothing for nothing.
        let mut empty = RgbaImage::from_pixel(10, 10, Rgba([255, 255, 255, 255]));
        draw_text(&mut empty, "", 0.0, 0.0, 12.0, Align::Left);
        assert!(dark(&empty).is_empty());
    }
}
