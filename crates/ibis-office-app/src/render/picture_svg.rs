// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Rotated and flipped pictures.
//!
//! GPUI draws pictures upright only. A picture that is rotated or flipped
//! is wrapped in an SVG document that embeds it with the rotation, flips and
//! crop applied; GPUI rasterizes that SVG once (with resvg) and draws the
//! result like any picture. The SVG covers the rotated frame's bounding box,
//! so the corners outside the picture stay transparent.

use gpui_kit::SMOOTH_SVG_SCALE_FACTOR;
use ibis_doc_model::{Crop, EmuRect};

/// The encoding of a picture that can be embedded in SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Encoding {
    Png,
    Jpeg,
    Gif,
    Webp,
    Svg,
}

impl Encoding {
    fn mime(self) -> &'static str {
        match self {
            Encoding::Png => "image/png",
            Encoding::Jpeg => "image/jpeg",
            Encoding::Gif => "image/gif",
            Encoding::Webp => "image/webp",
            Encoding::Svg => "image/svg+xml",
        }
    }
}

/// Largest rasterized size, in pixels; bigger pictures are drawn smaller.
const MAX_PIXELS: f64 = 16_000_000.0;
/// Width the frame gets when the picture's own size is unknown.
const FALLBACK_WIDTH: f64 = 2048.0;

/// The size an SVG says it is, for `pixels` rasterized: GPUI rasterizes an
/// SVG picture at twice its size (`SvgRenderer::render_single_frame`, by
/// `SMOOTH_SVG_SCALE_FACTOR`), so it says half. Else a picture takes four
/// times its pixels (seconds for a large photo).
fn stated(pixels: f64) -> f64 {
    ((pixels / SMOOTH_SVG_SCALE_FACTOR as f64) * 1000.0).round().max(500.0) / 1000.0
}

/// A rotated picture ready to draw: the SVG and where it goes on the slide.
pub(crate) struct RotatedPicture {
    pub(crate) svg: Vec<u8>,
    /// The rotated frame's bounding box, in EMU.
    pub(crate) bounds: EmuRect,
}

/// Wrap a picture drawn in `frame`, rotated clockwise by `rotation`
/// (60 000ths of a degree) about the frame's centre and flipped as given.
pub(crate) fn rotated_picture(
    data: &[u8],
    encoding: Encoding,
    frame: EmuRect,
    rotation: i32,
    flip_h: bool,
    flip_v: bool,
    crop: Crop,
) -> RotatedPicture {
    let degrees = rotation as f64 / 60_000.0;
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (fw_emu, fh_emu) = (frame.width.max(1) as f64, frame.height.max(1) as f64);
    let bw_emu = fw_emu * cos.abs() + fh_emu * sin.abs();
    let bh_emu = fw_emu * sin.abs() + fh_emu * cos.abs();
    let (cx_emu, cy_emu) = (frame.x as f64 + fw_emu / 2.0, frame.y as f64 + fh_emu / 2.0);
    let bounds = EmuRect::new(
        (cx_emu - bw_emu / 2.0).round() as i64,
        (cy_emu - bh_emu / 2.0).round() as i64,
        bw_emu.round() as i64,
        bh_emu.round() as i64,
    );

    // Rasterize at the picture's own resolution across the frame.
    let frac = |v: i32| v as f64 / 100_000.0;
    let visible_w = (1.0 - frac(crop.left) - frac(crop.right)).max(0.01);
    let visible_h = (1.0 - frac(crop.top) - frac(crop.bottom)).max(0.01);
    let frame_px = pixel_size(data, encoding).map(|(w, _)| w as f64 * visible_w).unwrap_or(FALLBACK_WIDTH).max(16.0);
    let mut k = frame_px / fw_emu; // pixels per EMU
    let pixels = bw_emu * k * bh_emu * k;
    if pixels > MAX_PIXELS {
        k *= (MAX_PIXELS / pixels).sqrt();
    }
    let (fw, fh, bw, bh) = (fw_emu * k, fh_emu * k, bw_emu * k, bh_emu * k);

    // The whole picture, placed so the cropped part fills the frame.
    let (ix, iy) = (-frac(crop.left) / visible_w * fw, -frac(crop.top) / visible_h * fh);
    let (iw, ih) = (fw / visible_w, fh / visible_h);
    let (sx, sy) = (if flip_h { -1.0 } else { 1.0 }, if flip_v { -1.0 } else { 1.0 });
    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" "#,
            r#"width="{w}" height="{h}" viewBox="{vx:.3} {vy:.3} {bw:.3} {bh:.3}">"#,
            r#"<defs><clipPath id="frame"><rect x="0" y="0" width="{fw:.3}" height="{fh:.3}"/></clipPath></defs>"#,
            // Flip, then rotate, about the frame's centre (OOXML's order).
            r#"<g transform="translate({cx:.3} {cy:.3}) rotate({deg}) scale({sx} {sy}) translate({ncx:.3} {ncy:.3})">"#,
            r#"<g clip-path="url(#frame)"><image x="{ix:.3}" y="{iy:.3}" width="{iw:.3}" height="{ih:.3}" preserveAspectRatio="none" xlink:href="data:{mime};base64,"#,
        ),
        w = stated(bw.round().max(1.0)),
        h = stated(bh.round().max(1.0)),
        vx = fw / 2.0 - bw / 2.0,
        vy = fh / 2.0 - bh / 2.0,
        bw = bw,
        bh = bh,
        fw = fw,
        fh = fh,
        cx = fw / 2.0,
        cy = fh / 2.0,
        deg = degrees,
        sx = sx,
        sy = sy,
        ncx = -fw / 2.0,
        ncy = -fh / 2.0,
        ix = ix,
        iy = iy,
        iw = iw,
        ih = ih,
        mime = encoding.mime(),
    );
    let mut out = svg.into_bytes();
    base64(data, &mut out);
    out.extend_from_slice(br#""/></g></g></svg>"#);
    RotatedPicture { svg: out, bounds }
}

/// Wrap a picture that fills a shape `width` × `height` EMU, cut to the
/// shape's outline: `subpaths` are the outline's closed parts, as points
/// relative to the frame in EMU. The SVG covers the frame.
pub(crate) fn clipped_picture(data: &[u8], encoding: Encoding, width: i64, height: i64, subpaths: &[Vec<(f64, f64)>]) -> Vec<u8> {
    let (fw_emu, fh_emu) = (width.max(1) as f64, height.max(1) as f64);
    let frame_px = pixel_size(data, encoding).map(|(w, _)| w as f64).unwrap_or(FALLBACK_WIDTH).max(16.0);
    let mut k = frame_px / fw_emu;
    let pixels = fw_emu * k * fh_emu * k;
    if pixels > MAX_PIXELS {
        k *= (MAX_PIXELS / pixels).sqrt();
    }
    let (fw, fh) = (fw_emu * k, fh_emu * k);
    let mut d = String::new();
    for points in subpaths {
        for (i, (x, y)) in points.iter().enumerate() {
            d.push_str(&format!("{}{:.2} {:.2} ", if i == 0 { 'M' } else { 'L' }, x * k, y * k));
        }
        d.push_str("Z ");
    }
    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" "#,
            r#"width="{w}" height="{h}" viewBox="0 0 {fw:.3} {fh:.3}">"#,
            r#"<defs><clipPath id="shape"><path d="{d}"/></clipPath></defs>"#,
            r#"<image clip-path="url(#shape)" x="0" y="0" width="{fw:.3}" height="{fh:.3}" preserveAspectRatio="none" xlink:href="data:{mime};base64,"#,
        ),
        w = stated(fw.round().max(1.0)),
        h = stated(fh.round().max(1.0)),
        fw = fw,
        fh = fh,
        d = d,
        mime = encoding.mime(),
    );
    let mut out = svg.into_bytes();
    base64(data, &mut out);
    out.extend_from_slice(br#""/></svg>"#);
    out
}

/// Wrap a picture recoloured as a duotone: its luminance maps from `dark`
/// (black) to `light` (white). The SVG has the picture's own size (at most
/// `MAX_PIXELS`).
pub(crate) fn duotone_picture(data: &[u8], encoding: Encoding, dark: (u8, u8, u8), light: (u8, u8, u8)) -> Vec<u8> {
    let (w, h) = own_size(data, encoding);
    let table = |a: u8, b: u8| format!("{:.4} {:.4}", a as f64 / 255.0, b as f64 / 255.0);
    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{sw}" height="{sh}" viewBox="0 0 {w} {h}">"#,
            r#"<defs><filter id="duo" color-interpolation-filters="sRGB">"#,
            r#"<feColorMatrix type="matrix" values="0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0 0 0 1 0"/>"#,
            r#"<feComponentTransfer><feFuncR type="table" tableValues="{r}"/><feFuncG type="table" tableValues="{g}"/><feFuncB type="table" tableValues="{b}"/></feComponentTransfer>"#,
            r#"</filter></defs><image filter="url(#duo)" x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="none" xlink:href="data:{mime};base64,"#,
        ),
        w = w,
        h = h,
        sw = stated(w),
        sh = stated(h),
        r = table(dark.0, light.0),
        g = table(dark.1, light.1),
        b = table(dark.2, light.2),
        mime = encoding.mime(),
    );
    let mut out = svg.into_bytes();
    base64(data, &mut out);
    out.extend_from_slice(br#""/></svg>"#);
    out
}

/// Wrap a PNG picture of an art page border (`w` × `h` pixels) to be
/// rasterized at `raster_w` × `raster_h` pixels (the zip's strips are 20 000
/// pixels long, more than a texture takes). With `tint`, it is recoloured as
/// Word colours its grey art borders: black becomes `tint`, white stays
/// white, and the greys between go by their lightness (the coverage tests'
/// references from Word: birds' grey #cccccc comes out #f2cccc in C00000).
pub(crate) fn art_border_picture(png: &[u8], w: u32, h: u32, raster_w: f64, raster_h: f64, tint: Option<(u8, u8, u8)>) -> Vec<u8> {
    let colour = tint.map_or(String::new(), |(r, g, b)| {
        let table = |c: u8| format!("{:.4} 1", c as f64 / 255.0);
        format!(
            concat!(
                r#"<feColorMatrix type="matrix" values="0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0 0 0 1 0"/>"#,
                r#"<feComponentTransfer><feFuncR type="table" tableValues="{r}"/><feFuncG type="table" tableValues="{g}"/><feFuncB type="table" tableValues="{b}"/></feComponentTransfer>"#,
            ),
            r = table(r),
            g = table(g),
            b = table(b),
        )
    });
    let filtered = !colour.is_empty();
    let filter = if filtered { format!(r#"<defs><filter id="tint" color-interpolation-filters="sRGB">{colour}</filter></defs>"#) } else { String::new() };
    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{sw}" height="{sh}" viewBox="0 0 {w} {h}">"#,
            "{filter}",
            r#"<image{use_filter} x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="none" xlink:href="data:image/png;base64,"#,
        ),
        sw = stated(raster_w),
        sh = stated(raster_h),
        w = w,
        h = h,
        filter = filter,
        use_filter = if filtered { r#" filter="url(#tint)""# } else { "" },
    );
    let mut out = svg.into_bytes();
    base64(png, &mut out);
    out.extend_from_slice(br#""/></svg>"#);
    out
}

/// Wrap a picture with the colour `clear` made transparent: a pixel's
/// opacity falls with how near its colour is to `clear` (fully clear at
/// it, opaque from three levels off). The SVG has the picture's own size (at
/// most `MAX_PIXELS`).
pub(crate) fn clear_colour_picture(data: &[u8], encoding: Encoding, clear: (u8, u8, u8)) -> Vec<u8> {
    let (w, h) = own_size(data, encoding);
    // The distance, summed over the channels, taken towards the side each
    // channel can move from the clear colour (exact for white, black and
    // other colours whose channels are all 0 or 255).
    let k = 85.0;
    let term = |c: u8| if c >= 128 { (-k, k * c as f64 / 255.0) } else { (k, -k * c as f64 / 255.0) };
    let (r, g, b) = (term(clear.0), term(clear.1), term(clear.2));
    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{sw}" height="{sh}" viewBox="0 0 {w} {h}">"#,
            r#"<defs><filter id="clear" color-interpolation-filters="sRGB">"#,
            r#"<feColorMatrix in="SourceGraphic" type="matrix" result="mask" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 {rk} {gk} {bk} 0 {c}"/>"#,
            r#"<feComposite in="SourceGraphic" in2="mask" operator="in"/>"#,
            r#"</filter></defs><image filter="url(#clear)" x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="none" xlink:href="data:{mime};base64,"#,
        ),
        w = w,
        h = h,
        sw = stated(w),
        sh = stated(h),
        rk = r.0,
        gk = g.0,
        bk = b.0,
        c = r.1 + g.1 + b.1,
        mime = encoding.mime(),
    );
    let mut out = svg.into_bytes();
    base64(data, &mut out);
    out.extend_from_slice(br#""/></svg>"#);
    out
}

fn base64(data: &[u8], out: &mut Vec<u8>) {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.reserve(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63]);
        out.push(TABLE[(n >> 12) as usize & 63]);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] } else { b'=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] } else { b'=' });
    }
}

/// Width and height in pixels, from the file's header.
/// The size an SVG of the picture's own size gets: the picture's, down to
/// `MAX_PIXELS` (a photo of 57 million pixels takes seconds to rasterize).
fn own_size(data: &[u8], encoding: Encoding) -> (f64, f64) {
    let (w, h) = pixel_size(data, encoding).map(|(w, h)| (w as f64, h as f64)).unwrap_or((FALLBACK_WIDTH, FALLBACK_WIDTH));
    let k = (MAX_PIXELS / (w * h).max(1.0)).sqrt().min(1.0);
    if k < 1.0 { ((w * k).round().max(1.0), (h * k).round().max(1.0)) } else { (w, h) }
}

fn pixel_size(data: &[u8], encoding: Encoding) -> Option<(u32, u32)> {
    let be16 = |at: usize| Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?) as u32);
    let be32 = |at: usize| Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?));
    let le16 = |at: usize| Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?) as u32);
    match encoding {
        Encoding::Png => Some((be32(16)?, be32(20)?)),
        Encoding::Gif => Some((le16(6)?, le16(8)?)),
        Encoding::Jpeg => {
            // The frame header (SOFn) holds the size; skip other segments.
            let mut at = 2;
            while at + 9 < data.len() {
                if data[at] != 0xFF {
                    return None;
                }
                let marker = data[at + 1];
                let length = be16(at + 2)? as usize;
                if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                    return Some((be16(at + 7)?, be16(at + 5)?));
                }
                at += 2 + length;
            }
            None
        }
        // A vector picture has no pixel size; the fallback width applies.
        Encoding::Svg => None,
        Encoding::Webp => {
            let le24 = |at: usize| Some(le16(at)? | (*data.get(at + 2)? as u32) << 16);
            match data.get(12..16)? {
                b"VP8X" => Some((le24(24)? + 1, le24(27)? + 1)),
                b"VP8 " => Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff)),
                b"VP8L" => {
                    let bits = u32::from_le_bytes(data.get(21..25)?.try_into().ok()?);
                    Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
                }
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads() {
        let mut out = Vec::new();
        base64(b"Man", &mut out);
        base64(b"Ma", &mut out);
        base64(b"M", &mut out);
        assert_eq!(out, b"TWFuTWE=TQ==");
    }

    #[test]
    fn quarter_turn_swaps_the_bounds() {
        let png = [0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0, 200, 0, 0, 0, 100];
        let p = rotated_picture(&png, Encoding::Png, EmuRect::new(0, 0, 2000, 1000), 5_400_000, false, false, Crop::default());
        assert_eq!(p.bounds, EmuRect::new(500, -500, 1000, 2000));
        let svg = String::from_utf8(p.svg).unwrap();
        // Rasterized at the picture's width across the frame: 200 px for
        // 2000 EMU (stated half: GPUI rasterizes an SVG at twice its size).
        assert!(svg.contains(r#"width="50" height="100""#), "{svg}");
        assert!(svg.contains("rotate(90)"));
    }

    #[test]
    fn jpeg_size_from_frame_header() {
        // SOI, an APP0 segment of length 4, then SOF0 with 16 × 32.
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 17, 8, 0, 32, 0, 16, 3];
        assert_eq!(pixel_size(&jpeg, Encoding::Jpeg), Some((16, 32)));
    }
}
