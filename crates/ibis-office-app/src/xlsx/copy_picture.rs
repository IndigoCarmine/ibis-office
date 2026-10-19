// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Copy as picture (XLSX-IMAGE-019): render selected cell range, shape, picture,
//! or chart as a PNG image and place it onto the system clipboard.

use gpui_kit::*;
use ibis_doc_model::{
    CellAlign, CellRange, CellValue, Command, EmuRect, Fill, Geometry, Shape, ShapeId, ShapeKind, SlideId, TextBody,
};

use crate::shell::editor::SlideEditor;

/// In-memory RGBA pixel buffer for rasterizing sheet elements to PNG.
#[derive(Clone, Debug)]
pub(crate) struct ImageBuffer {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>, // RGBA8: width * height * 4
}

impl ImageBuffer {
    pub(crate) fn new(width: u32, height: u32, bg: [u8; 4]) -> Self {
        let size = (width * height * 4) as usize;
        let mut pixels = Vec::with_capacity(size);
        for _ in 0..(width * height) {
            pixels.extend_from_slice(&bg);
        }
        Self { width, height, pixels }
    }

    #[inline]
    pub(crate) fn set_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        let a = color[3] as u32;
        if a == 255 {
            self.pixels[idx..idx + 4].copy_from_slice(&color);
        } else if a > 0 {
            let inv = 255 - a;
            let r = (color[0] as u32 * a + self.pixels[idx] as u32 * inv) / 255;
            let g = (color[1] as u32 * a + self.pixels[idx + 1] as u32 * inv) / 255;
            let b = (color[2] as u32 * a + self.pixels[idx + 2] as u32 * inv) / 255;
            let out_a = (a + self.pixels[idx + 3] as u32 * inv / 255).min(255);
            self.pixels[idx] = r as u8;
            self.pixels[idx + 1] = g as u8;
            self.pixels[idx + 2] = b as u8;
            self.pixels[idx + 3] = out_a as u8;
        }
    }

    pub(crate) fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
        let x_end = (x + w).min(self.width);
        let y_end = (y + h).min(self.height);
        for py in y..y_end {
            for px in x..x_end {
                self.set_pixel(px, py, color);
            }
        }
    }

    pub(crate) fn draw_rect_outline(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 4], line_width: u32) {
        if w == 0 || h == 0 || line_width == 0 {
            return;
        }
        self.draw_hline(x, y, w, color, line_width, false);
        self.draw_hline(x, (y + h).saturating_sub(line_width), w, color, line_width, false);
        self.draw_vline(x, y, h, color, line_width, false);
        self.draw_vline((x + w).saturating_sub(line_width), y, h, color, line_width, false);
    }

    pub(crate) fn draw_hline(&mut self, x: u32, y: u32, w: u32, color: [u8; 4], line_width: u32, dashed: bool) {
        let x_end = (x + w).min(self.width);
        let y_end = (y + line_width).min(self.height);
        for py in y..y_end {
            for px in x..x_end {
                if dashed && (px / 4) % 2 != 0 {
                    continue;
                }
                self.set_pixel(px, py, color);
            }
        }
    }

    pub(crate) fn draw_vline(&mut self, x: u32, y: u32, h: u32, color: [u8; 4], line_width: u32, dashed: bool) {
        let x_end = (x + line_width).min(self.width);
        let y_end = (y + h).min(self.height);
        for py in y..y_end {
            if dashed && (py / 4) % 2 != 0 {
                continue;
            }
            for px in x..x_end {
                self.set_pixel(px, py, color);
            }
        }
    }

    pub(crate) fn draw_ellipse(&mut self, cx: u32, cy: u32, rx: u32, ry: u32, fill: Option<[u8; 4]>, stroke: Option<([u8; 4], u32)>) {
        let x0 = cx.saturating_sub(rx);
        let x1 = (cx + rx).min(self.width);
        let y0 = cy.saturating_sub(ry);
        let y1 = (cy + ry).min(self.height);
        let rx_f = rx as f64;
        let ry_f = ry as f64;
        if rx_f == 0.0 || ry_f == 0.0 {
            return;
        }
        for py in y0..y1 {
            let dy = (py as f64 - cy as f64) / ry_f;
            for px in x0..x1 {
                let dx = (px as f64 - cx as f64) / rx_f;
                let d2 = dx * dx + dy * dy;
                if d2 <= 1.0 {
                    if let Some(fc) = fill {
                        self.set_pixel(px, py, fc);
                    }
                }
                if let Some((sc, lw)) = stroke {
                    let inner_rx = (rx_f - lw as f64).max(0.0) / rx_f;
                    let inner_ry = (ry_f - lw as f64).max(0.0) / ry_f;
                    let inner_d2 = (dx / inner_rx) * (dx / inner_rx) + (dy / inner_ry) * (dy / inner_ry);
                    if d2 <= 1.0 && inner_d2 >= 1.0 {
                        self.set_pixel(px, py, sc);
                    }
                }
            }
        }
    }

    pub(crate) fn text_width(&self, text: &str) -> u32 {
        text.chars().count() as u32 * 6
    }

    pub(crate) fn draw_text(&mut self, x: u32, y: u32, text: &str, color: [u8; 4]) {
        self.draw_text_clipped(x, y, text, color, 0, 0, self.width, self.height);
    }

    pub(crate) fn draw_text_clipped(&mut self, x: u32, y: u32, text: &str, color: [u8; 4], cx0: u32, cy0: u32, cw: u32, ch: u32) {
        let cx1 = (cx0 + cw).min(self.width);
        let cy1 = (cy0 + ch).min(self.height);
        let mut cur_x = x;
        for c in text.chars() {
            if cur_x + 5 > cx1 {
                break;
            }
            if cur_x >= cx0 {
                for r in 0..7 {
                    let py = y + r as u32;
                    if py >= cy0 && py < cy1 {
                        let bits = glyph_row_bits(c, r);
                        for col in 0..5 {
                            if (bits & (1 << (4 - col))) != 0 {
                                let px = cur_x + col as u32;
                                if px >= cx0 && px < cx1 {
                                    self.set_pixel(px, py, color);
                                }
                            }
                        }
                    }
                }
            }
            cur_x += 6;
        }
    }

    /// Convert image buffer to RFC 2083 standard PNG format.
    pub(crate) fn to_png(&self) -> Vec<u8> {
        rgba_to_png(self.width, self.height, &self.pixels)
    }
}

/// 5x7 bitmap font glyph table for ASCII printable characters (32..=126).
fn glyph_row_bits(ch: char, row: usize) -> u8 {
    if row >= 7 {
        return 0;
    }
    match ch {
        ' ' => 0b00000,
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100][row],
        '"' => [0b01010, 0b01010, 0b01010, 0b00000, 0b00000, 0b00000, 0b00000][row],
        '#' => [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010][row],
        '$' => [0b00100, 0b01111, 0b10100, 0b01110, 0b00101, 0b11110, 0b00100][row],
        '%' => [0b11001, 0b11010, 0b00100, 0b01000, 0b01011, 0b10011, 0b00000][row],
        '&' => [0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101][row],
        '\'' => [0b00100, 0b00100, 0b00010, 0b00000, 0b00000, 0b00000, 0b00000][row],
        '(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010][row],
        ')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000][row],
        '*' => [0b00000, 0b00100, 0b10101, 0b01110, 0b10101, 0b00100, 0b00000][row],
        '+' => [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000][row],
        ',' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100, 0b01000][row],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000][row],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00110][row],
        '/' => [0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b00000, 0b00000][row],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110][row],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110][row],
        '2' => [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111][row],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110][row],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010][row],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110][row],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110][row],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000][row],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110][row],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100][row],
        ':' => [0b00000, 0b00110, 0b00110, 0b00000, 0b00110, 0b00110, 0b00000][row],
        ';' => [0b00000, 0b00110, 0b00110, 0b00000, 0b00110, 0b00100, 0b01000][row],
        '<' => [0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010][row],
        '=' => [0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000][row],
        '>' => [0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000][row],
        '?' => [0b01110, 0b10001, 0b00010, 0b00100, 0b00100, 0b00000, 0b00100][row],
        '@' => [0b01110, 0b10001, 0b00001, 0b01101, 0b10101, 0b10110, 0b01100][row],
        'A' | 'a' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001][row],
        'B' | 'b' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110][row],
        'C' | 'c' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110][row],
        'D' | 'd' => [0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100][row],
        'E' | 'e' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111][row],
        'F' | 'f' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000][row],
        'G' | 'g' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110][row],
        'H' | 'h' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001][row],
        'I' | 'i' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110][row],
        'J' | 'j' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100][row],
        'K' | 'k' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001][row],
        'L' | 'l' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111][row],
        'M' | 'm' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001][row],
        'N' | 'n' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001][row],
        'O' | 'o' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110][row],
        'P' | 'p' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000][row],
        'Q' | 'q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101][row],
        'R' | 'r' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001][row],
        'S' | 's' => [0b01110, 0b10001, 0b10000, 0b01110, 0b00001, 0b10001, 0b01110][row],
        'T' | 't' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100][row],
        'U' | 'u' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110][row],
        'V' | 'v' => [0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b01010, 0b00100][row],
        'W' | 'w' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001][row],
        'X' | 'x' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001][row],
        'Y' | 'y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100][row],
        'Z' | 'z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111][row],
        '[' => [0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110][row],
        '\\' => [0b10000, 0b01000, 0b00100, 0b00010, 0b00001, 0b00000, 0b00000][row],
        ']' => [0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110][row],
        '^' => [0b00100, 0b01010, 0b10001, 0b00000, 0b00000, 0b00000, 0b00000][row],
        '_' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111][row],
        '`' => [0b00100, 0b00010, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000][row],
        '{' => [0b00011, 0b00100, 0b00100, 0b01000, 0b00100, 0b00100, 0b00011][row],
        '|' => [0b00100, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00100][row],
        '}' => [0b11000, 0b00100, 0b00100, 0b00010, 0b00100, 0b00100, 0b11000][row],
        '~' => [0b00000, 0b01001, 0b10110, 0b00000, 0b00000, 0b00000, 0b00000][row],
        '¥' => [0b10001, 0b01010, 0b11111, 0b00100, 0b11111, 0b00100, 0b00100][row],
        _ => [0b11111, 0b10001, 0b10101, 0b10101, 0b10001, 0b11111, 0b00000][row], // fallback glyph
    }
}

/// Pure-Rust RFC 2083 PNG encoder.
pub(crate) fn rgba_to_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.push(8); // bit depth = 8
    ihdr.push(6); // color type = 6 (RGBA)
    ihdr.push(0); // compression = deflate
    ihdr.push(0); // filter = standard
    ihdr.push(0); // interlace = none
    chunk(&mut out, b"IHDR", &ihdr);

    let row_len = (w * 4) as usize;
    let mut raw = Vec::with_capacity((h as usize) * (row_len + 1));
    for y in 0..h as usize {
        raw.push(0); // filter: None
        let start = y * row_len;
        if start + row_len <= rgba.len() {
            raw.extend_from_slice(&rgba[start..start + row_len]);
        } else {
            raw.resize(raw.len() + row_len, 0);
        }
    }

    // zlib stream with non-compressed deflate blocks
    let mut z = vec![0x78, 0x01];
    for (i, block) in raw.chunks(65_535).enumerate() {
        let last = (i + 1) * 65_535 >= raw.len();
        z.push(last as u8);
        z.extend((block.len() as u16).to_le_bytes());
        z.extend((!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    // Adler-32
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend(((b << 16) | a).to_be_bytes());

    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Calculate the total size in EMU of a cell range on a sheet.
pub(crate) fn sheet_range_emu_size(sheet: &ibis_doc_model::Sheet, range: CellRange) -> (i64, i64) {
    let mut w_emu = 0i64;
    for col in range.first_column..=range.last_column {
        let emu = sheet.columns.get(col as usize).copied().unwrap_or(sheet.default_column);
        w_emu += emu;
    }
    let mut h_emu = 0i64;
    for row in range.first_row..=range.last_row {
        let emu = sheet.rows.get(row as usize).copied().unwrap_or(sheet.default_row);
        h_emu += emu;
    }
    (w_emu, h_emu)
}

/// Render a cell range of a worksheet to an in-memory PNG byte vector.
pub(crate) fn render_sheet_range_to_png(sheet: &ibis_doc_model::Sheet, range: CellRange) -> Option<Vec<u8>> {
    let mut col_widths = Vec::new();
    for col in range.first_column..=range.last_column {
        let emu = sheet.columns.get(col as usize).copied().unwrap_or(sheet.default_column);
        let px = ((emu as f64) / 9525.0).round().clamp(24.0, 320.0) as u32;
        col_widths.push(px);
    }
    let mut row_heights = Vec::new();
    for row in range.first_row..=range.last_row {
        let emu = sheet.rows.get(row as usize).copied().unwrap_or(sheet.default_row);
        let px = ((emu as f64) / 9525.0).round().clamp(16.0, 160.0) as u32;
        row_heights.push(px);
    }
    let total_w = (col_widths.iter().sum::<u32>() + 1).min(3840);
    let total_h = (row_heights.iter().sum::<u32>() + 1).min(2160);
    if total_w == 0 || total_h == 0 {
        return None;
    }

    let mut buf = ImageBuffer::new(total_w, total_h, [255, 255, 255, 255]);
    let grid_color = [218, 222, 228, 255];

    // Draw grid lines
    let mut cur_y = 0;
    for &h in &row_heights {
        buf.draw_hline(0, cur_y, total_w, grid_color, 1, false);
        cur_y += h;
    }
    buf.draw_hline(0, total_h.saturating_sub(1), total_w, grid_color, 1, false);

    let mut cur_x = 0;
    for &w in &col_widths {
        buf.draw_vline(cur_x, 0, total_h, grid_color, 1, false);
        cur_x += w;
    }
    buf.draw_vline(total_w.saturating_sub(1), 0, total_h, grid_color, 1, false);

    // Draw cell contents
    let mut y0 = 0;
    for (r_idx, row) in (range.first_row..=range.last_row).enumerate() {
        let cell_h = row_heights[r_idx];
        let mut x0 = 0;
        for (c_idx, col) in (range.first_column..=range.last_column).enumerate() {
            let cell_w = col_widths[c_idx];
            if let Some(cell) = sheet.cell(row, col) {
                if let Some(style) = sheet.styles.get(cell.style as usize) {
                    // Cell fill
                    if let Some(fill_rgb) = style.fill {
                        buf.fill_rect(x0 + 1, y0 + 1, cell_w.saturating_sub(1), cell_h.saturating_sub(1), [fill_rgb.r(), fill_rgb.g(), fill_rgb.b(), 255]);
                    }
                    // Cell borders
                    if let Some(border) = &style.top {
                        buf.draw_hline(x0, y0, cell_w, [border.color.r(), border.color.g(), border.color.b(), 255], 1, false);
                    }
                    if let Some(border) = &style.bottom {
                        buf.draw_hline(x0, (y0 + cell_h).saturating_sub(1), cell_w, [border.color.r(), border.color.g(), border.color.b(), 255], 1, false);
                    }
                    if let Some(border) = &style.left {
                        buf.draw_vline(x0, y0, cell_h, [border.color.r(), border.color.g(), border.color.b(), 255], 1, false);
                    }
                    if let Some(border) = &style.right {
                        buf.draw_vline((x0 + cell_w).saturating_sub(1), y0, cell_h, [border.color.r(), border.color.g(), border.color.b(), 255], 1, false);
                    }
                    // Cell text
                    if !cell.text.is_empty() {
                        let text_color = [style.color.r(), style.color.g(), style.color.b(), 255];
                        let tw = buf.text_width(&cell.text);
                        let tx = match style.align {
                            CellAlign::Right => {
                                if cell_w > tw + 6 { x0 + cell_w - tw - 6 } else { x0 + 2 }
                            }
                            CellAlign::Center => {
                                if cell_w > tw { x0 + (cell_w - tw) / 2 } else { x0 + 2 }
                            }
                            _ => {
                                if matches!(cell.value, CellValue::Number(_)) {
                                    if cell_w > tw + 6 { x0 + cell_w - tw - 6 } else { x0 + 2 }
                                } else {
                                    x0 + 5
                                }
                            }
                        };
                        let ty = y0 + (cell_h.saturating_sub(7)) / 2;
                        buf.draw_text_clipped(tx, ty, &cell.text, text_color, x0 + 1, y0 + 1, cell_w.saturating_sub(2), cell_h.saturating_sub(2));
                    }
                }
            }
            x0 += cell_w;
        }
        y0 += cell_h;
    }

    Some(buf.to_png())
}

impl SlideEditor {
    /// Render selected cell range as a PNG image.
    pub(crate) fn render_range_as_picture(&self, range: CellRange) -> Option<Vec<u8>> {
        let (_, sheet) = self.current_sheet()?;
        render_sheet_range_to_png(sheet, range)
    }

    /// Render a worksheet shape, picture, or chart as a PNG image.
    pub(crate) fn render_shape_as_picture(&self, shape_id: ShapeId) -> Option<Vec<u8>> {
        let shape = self.slide().shapes.iter().find(|s| s.id == shape_id)?;
        let frame = shape.frame;
        let w_px = ((frame.width as f64) / 9525.0).round().clamp(32.0, 2048.0) as u32;
        let h_px = ((frame.height as f64) / 9525.0).round().clamp(32.0, 2048.0) as u32;

        match &shape.kind {
            ShapeKind::Picture(pic) => {
                if let Some(media) = self.presentation().media(pic.media) {
                    if media.format == ibis_doc_model::MediaFormat::Png {
                        return Some(media.data.to_vec());
                    }
                }
                let mut buf = ImageBuffer::new(w_px, h_px, [245, 247, 250, 255]);
                buf.draw_rect_outline(0, 0, w_px, h_px, [180, 190, 205, 255], 2);
                let label = if pic.name.is_empty() { "Picture" } else { &pic.name };
                let tw = buf.text_width(label);
                let tx = if w_px > tw { (w_px - tw) / 2 } else { 4 };
                let ty = h_px.saturating_sub(7) / 2;
                buf.draw_text(tx, ty, label, [100, 115, 135, 255]);
                Some(buf.to_png())
            }
            ShapeKind::Chart(chart) => {
                let mut buf = ImageBuffer::new(w_px, h_px, [255, 255, 255, 255]);
                buf.draw_rect_outline(0, 0, w_px, h_px, [210, 215, 225, 255], 1);

                let title_text = chart.title.as_ref().map(|t| t.text.as_str()).unwrap_or("Chart");
                let tw = buf.text_width(title_text);
                let tx = if w_px > tw { (w_px - tw) / 2 } else { 8 };
                buf.draw_text(tx, 10, title_text, [40, 50, 65, 255]);

                let px0 = 40;
                let py0 = 32;
                let pw = w_px.saturating_sub(px0 + 50);
                let ph = h_px.saturating_sub(py0 + 35);
                if pw > 20 && ph > 20 {
                    buf.fill_rect(px0, py0, pw, ph, [250, 252, 255, 255]);
                    buf.draw_rect_outline(px0, py0, pw, ph, [220, 225, 235, 255], 1);
                    for i in 1..4 {
                        let gy = py0 + (ph * i) / 4;
                        buf.draw_hline(px0, gy, pw, [235, 238, 245, 255], 1, true);
                    }

                    let colors = [
                        [68, 114, 196, 255],  // Office Blue
                        [237, 125, 49, 255],  // Orange
                        [165, 165, 165, 255], // Gray
                        [255, 192, 0, 255],   // Yellow
                    ];
                    let bar_w = (pw / 12).clamp(4, 24);
                    let gap = bar_w / 2;
                    let mut bx = px0 + gap + 4;
                    for (i, group) in chart.groups.iter().enumerate() {
                        let color = colors[i % colors.len()];
                        for (si, _series) in group.series.iter().take(4).enumerate() {
                            let bh = (ph * (si as u32 * 2 + 3) / 12).clamp(8, ph.saturating_sub(4));
                            let by = py0 + ph.saturating_sub(bh);
                            buf.fill_rect(bx, by, bar_w, bh, color);
                            bx += bar_w + gap;
                            if bx + bar_w > px0 + pw {
                                break;
                            }
                        }
                    }
                }

                if let Some(_legend) = &chart.legend {
                    let lx = w_px.saturating_sub(45);
                    let ly = 40;
                    buf.fill_rect(lx, ly, 8, 8, [68, 114, 196, 255]);
                    buf.draw_text(lx + 12, ly + 1, "S1", [80, 90, 105, 255]);
                }

                Some(buf.to_png())
            }
            ShapeKind::Auto(auto) => {
                let fill_color = match &auto.fill {
                    Fill::Solid(color) => [color.rgb.r(), color.rgb.g(), color.rgb.b(), color.alpha],
                    _ => [68, 114, 196, 255], // Default blue
                };
                let stroke_color = match &auto.stroke {
                    Some(s) => [s.color.rgb.r(), s.color.rgb.g(), s.color.rgb.b(), s.color.alpha],
                    None => [47, 84, 150, 255],
                };
                let mut buf = ImageBuffer::new(w_px, h_px, [0, 0, 0, 0]); // Transparent background
                if matches!(auto.geometry, Geometry::Ellipse) {
                    buf.draw_ellipse(w_px / 2, h_px / 2, w_px / 2 - 1, h_px / 2 - 1, Some(fill_color), Some((stroke_color, 2)));
                } else {
                    buf.fill_rect(0, 0, w_px, h_px, fill_color);
                    buf.draw_rect_outline(0, 0, w_px, h_px, stroke_color, 2);
                }

                if let Some(text) = &auto.text {
                    draw_text_body(&mut buf, text, w_px, h_px, [255, 255, 255, 255]);
                }
                Some(buf.to_png())
            }
            ShapeKind::TextBox(body) => {
                let mut buf = ImageBuffer::new(w_px, h_px, [255, 255, 255, 255]);
                buf.draw_rect_outline(0, 0, w_px, h_px, [200, 205, 215, 255], 1);
                draw_text_body(&mut buf, body, w_px, h_px, [0, 0, 0, 255]);
                Some(buf.to_png())
            }
            ShapeKind::Line(stroke) => {
                let stroke_color = [stroke.color.rgb.r(), stroke.color.rgb.g(), stroke.color.rgb.b(), stroke.color.alpha];
                let mut buf = ImageBuffer::new(w_px, h_px, [0, 0, 0, 0]);
                buf.draw_hline(0, h_px / 2, w_px, stroke_color, 2, stroke.dashed);
                Some(buf.to_png())
            }
            _ => None,
        }
    }

    /// 画像としてコピー (xlsx.copy-as-picture): copy selected cell range, shape,
    /// picture, or chart onto clipboard as a standard PNG image.
    pub(crate) fn copy_as_picture(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);

        // 1. If a worksheet floating shape, picture, or chart is selected:
        if let Some(shape_id) = self.selected_sheet_shape() {
            let kind_name = self.slide().shapes.iter().find(|s| s.id == shape_id).map(|s| match &s.kind {
                ShapeKind::Picture(_) => crate::shell::i18n::text("pptx.chrome.picture").to_string(),
                ShapeKind::Chart(_) => crate::shell::i18n::text("insert.chart").to_string(),
                _ => crate::shell::i18n::text("_xlsx.object").to_string(),
            }).unwrap_or_else(|| crate::shell::i18n::text("_xlsx.object").to_string());

            if let Some(png) = self.render_shape_as_picture(shape_id) {
                let image = gpui_kit::Image {
                    format: gpui_kit::ImageFormat::Png,
                    bytes: png,
                    id: 0,
                };
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_image(&image));
                self.status = Some(crate::shell::i18n::format("_xlsx.copied-kind-name-as-a-picture", &[("kind_name", &kind_name)]).into());
                return cx.notify();
            }
        }

        // 2. If a cell range is selected:
        if let Some(range) = self.selected_range() {
            if let Some(png) = self.render_range_as_picture(range) {
                let image = gpui_kit::Image {
                    format: gpui_kit::ImageFormat::Png,
                    bytes: png,
                    id: 0,
                };
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_image(&image));
                self.status = Some(crate::shell::i18n::text("_xlsx.copied-the-selection-as-a-picture").into());
                return cx.notify();
            }
        }

        self.status = Some(crate::shell::i18n::text("_xlsx.select-a-cell-or-an-object-to-copy").into());
        cx.notify();
    }

    /// 形式を選択して貼り付け > 図 (linked = false) / リンクされた図 (linked = true) (XLSX-IMAGE-020)
    pub(crate) fn paste_as_picture(&mut self, linked: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((target_col, target_row)) = self.current_cell() else { return };
        let slide_id = self.current_slide;

        // 1. Determine source range and slide from clipboard clip or selection
        let (source_range, source_slide) = if let Some(clip) = self.sheet_state().and_then(|s| s.clip.clone()) {
            (clip.range, clip.slide)
        } else if let Some(range) = self.selected_range() {
            (range, slide_id)
        } else {
            self.status = Some(crate::shell::i18n::text("_xlsx.to-use-paste-special-copy-cells-first").into());
            return cx.notify();
        };

        self.insert_range_as_picture(source_range, source_slide, target_col, target_row, linked, cx);
    }

    /// カメラ機能 (xlsx.camera-tool):
    /// 現在の選択範囲をリンクされた図として配置する (XLSX-IMAGE-020)。
    pub(crate) fn insert_camera_picture(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide_id = self.current_slide;
        let Some(source_range) = self.selected_range() else {
            self.status = Some(crate::shell::i18n::text("_xlsx.select-a-cell-or-an-object-to-copy").into());
            return cx.notify();
        };
        let (target_col, target_row) = self.current_cell().unwrap_or((source_range.last_column + 1, source_range.last_row + 1));
        self.insert_range_as_picture(source_range, slide_id, target_col, target_row, true, cx);
    }

    fn insert_range_as_picture(
        &mut self,
        source_range: CellRange,
        source_slide: SlideId,
        target_col: u32,
        target_row: u32,
        linked: bool,
        cx: &mut Context<Self>,
    ) {
        let slide_id = self.current_slide;

        // 2. Obtain source sheet and its name
        let p = self.presentation();
        let source_sheet_info = p.slides.iter().find(|s| s.id == source_slide).and_then(|s| {
            let name = s.name.clone();
            crate::shell::editor::sheet_of(s).map(|(_, sheet)| (name, sheet.clone()))
        });
        let Some((source_sheet_name, source_sheet)) = source_sheet_info else { return };

        // 3. Render range to PNG
        let Some(png) = render_sheet_range_to_png(&source_sheet, source_range) else { return };

        // 4. Calculate frame at destination cell
        let (x, y) = if let Some((shape, sheet)) = self.current_sheet() {
            let xs = sheet.column_offsets();
            let ys = sheet.row_offsets();
            let left = shape.frame.x + xs.get(target_col as usize).copied().unwrap_or(0);
            let top = shape.frame.y + ys.get(target_row as usize).copied().unwrap_or(0);
            (left, top)
        } else {
            self.sheet_insertion_origin()
        };
        let (w_emu, h_emu) = sheet_range_emu_size(&source_sheet, source_range);
        let frame = EmuRect::new(x, y, w_emu, h_emu);

        // 5. Register media and shape
        let media = self.document.ids().add_media(ibis_doc_model::MediaFormat::Png, png.clone());
        self.resources.add_picture(media, ibis_doc_model::MediaFormat::Png, &png);
        let shape_id = self.document.ids().new_shape_id();
        let picture = ibis_doc_model::Picture::new(media);
        let shape = Shape::new(shape_id, frame, ShapeKind::Picture(picture));

        let mut commands = vec![
            Command::InsertShape { slide: slide_id, index: self.slide().shapes.len(), shape },
        ];

        if linked {
            let start_name = format!("${}${}", super::dedupe_edit::column_name(source_range.first_column), source_range.first_row + 1);
            let end_name = format!("${}${}", super::dedupe_edit::column_name(source_range.last_column), source_range.last_row + 1);
            let range_ref = if source_range.first_row == source_range.last_row && source_range.first_column == source_range.last_column {
                start_name
            } else {
                format!("{start_name}:{end_name}")
            };
            let formula_str = match (&source_sheet_name, source_slide != slide_id) {
                (Some(name), true) => format!("={}!{range_ref}", ibis_calc::formula::quote_sheet(name)),
                _ => format!("={range_ref}"),
            };
            commands.push(Command::SetShapeFormula {
                slide: slide_id,
                shape: shape_id,
                formula: Some(formula_str),
            });
        }

        self.run_sheet(commands, cx);
        self.selection = Some(shape_id);
        self.status = Some(crate::shell::i18n::text(if linked { "xlsx.pasted-linked-picture" } else { "xlsx.pasted-picture" }).into());
        cx.notify();
    }
}

fn draw_text_body(buf: &mut ImageBuffer, text: &TextBody, w_px: u32, h_px: u32, color: [u8; 4]) {
    let mut lines = Vec::new();
    for p in &text.paragraphs {
        let mut line = String::new();
        for r in &p.runs {
            line.push_str(&r.text);
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        return;
    }
    let line_h = 10;
    let total_text_h = lines.len() as u32 * line_h;
    let start_y = if h_px > total_text_h { (h_px - total_text_h) / 2 } else { 4 };
    for (idx, line) in lines.iter().enumerate() {
        let tw = buf.text_width(line);
        let tx = if w_px > tw { (w_px - tw) / 2 } else { 4 };
        let ty = start_y + idx as u32 * line_h;
        buf.draw_text_clipped(tx, ty, line, color, 2, 2, w_px.saturating_sub(4), h_px.saturating_sub(4));
    }
}
