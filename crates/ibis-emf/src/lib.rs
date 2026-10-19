// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pictures inside Windows Enhanced Metafiles (EMF).
//!
//! Office often saves pasted pictures as an EMF that does nothing but draw
//! one bitmap, either with EMF+ (GDI+ records in EMF comments) or with a
//! GDI `EMR_STRETCHDIBITS` or `EMR_ALPHABLEND`. [`extract_image`] finds that
//! bitmap and returns it as an ordinary image file, keeping its transparency. Vector drawing is not interpreted: a
//! metafile without a bitmap yields `None`.

mod png;

/// The encoding of an extracted image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Tiff,
}

const EMR_HEADER: u32 = 1;
const EMR_EOF: u32 = 14;
const EMR_COMMENT: u32 = 70;
const EMR_STRETCHDIBITS: u32 = 81;
const EMR_ALPHABLEND: u32 = 114;

/// `BLENDFUNCTION.AlphaFormat`: the source bitmap has per-pixel alpha.
const AC_SRC_ALPHA: u8 = 1;

const EMFPLUS_OBJECT: u16 = 0x4008;
const OBJECT_IMAGE: u16 = 5;
const IMAGE_BITMAP: u32 = 1;
const BITMAP_PIXELS: u32 = 0;
const BITMAP_COMPRESSED: u32 = 1;

const PIXEL_ARGB32: u32 = 0x0026_200a;
const PIXEL_PARGB32: u32 = 0x000e_200b;
const PIXEL_RGB32: u32 = 0x0002_2009;
const PIXEL_RGB24: u32 = 0x0002_1808;

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn i32_at(d: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

/// The largest bitmap the metafile draws, as an image file.
pub fn extract_image(emf: &[u8]) -> Option<(ImageFormat, Vec<u8>)> {
    if u32_at(emf, 0)? != EMR_HEADER || emf.get(40..44)? != b" EMF" {
        return None;
    }
    let mut emf_plus = Vec::new();
    let mut gdi_bitmaps = Vec::new();
    let mut at = 0;
    while let (Some(kind), Some(size)) = (u32_at(emf, at), u32_at(emf, at + 4)) {
        let size = size as usize;
        if size < 8 || at + size > emf.len() {
            break;
        }
        let record = &emf[at..at + size];
        match kind {
            // EMF+ records travel in comments tagged "EMF+".
            EMR_COMMENT if record.get(12..16) == Some(b"EMF+") => {
                let len = u32_at(record, 8)? as usize;
                emf_plus.extend_from_slice(record.get(16..(12 + len).min(size))?);
            }
            EMR_STRETCHDIBITS => gdi_bitmaps.extend(stretch_dibits(record)),
            EMR_ALPHABLEND => gdi_bitmaps.extend(alpha_blend(record)),
            EMR_EOF => break,
            _ => {}
        }
        at += size;
    }
    let mut candidates = emf_plus_bitmaps(&emf_plus);
    candidates.extend(gdi_bitmaps);
    candidates.into_iter().max_by_key(|(area, _)| *area).map(|(_, image)| image)
}

type Candidate = (u64, (ImageFormat, Vec<u8>));

/// Bitmaps defined as EMF+ image objects, with their pixel area.
fn emf_plus_bitmaps(data: &[u8]) -> Vec<Candidate> {
    let mut out = Vec::new();
    // Objects too big for one record are split, the first part announcing
    // the total size; collect the parts per object id.
    let mut pending: Option<(u16, usize, Vec<u8>)> = None;
    let mut at = 0;
    while let (Some(kind), Some(flags), Some(size), Some(data_size)) =
        (u16_at(data, at), u16_at(data, at + 2), u32_at(data, at + 4), u32_at(data, at + 8))
    {
        let (size, data_size) = (size as usize, data_size as usize);
        if size < 12 || at + size > data.len() {
            break;
        }
        if kind == EMFPLUS_OBJECT && (flags >> 8) & 0x7f == OBJECT_IMAGE {
            let body = &data[at + 12..at + 12 + data_size.min(size - 12)];
            let id = flags & 0xff;
            if flags & 0x8000 != 0 {
                let Some(total) = u32_at(body, 0) else { break };
                let part = &body[4..];
                match &mut pending {
                    Some((pid, _, buf)) if *pid == id => buf.extend_from_slice(part),
                    _ => pending = Some((id, total as usize, part.to_vec())),
                }
                if let Some((_, total, buf)) = &pending
                    && buf.len() >= *total
                {
                    out.extend(emf_plus_image(buf));
                    pending = None;
                }
            } else {
                out.extend(emf_plus_image(body));
            }
        }
        at += size;
    }
    out
}

/// An `EmfPlusImage` object holding a bitmap.
fn emf_plus_image(body: &[u8]) -> Option<Candidate> {
    if u32_at(body, 4)? != IMAGE_BITMAP {
        return None;
    }
    let width = i32_at(body, 8)?;
    let height = i32_at(body, 12)?;
    let stride = i32_at(body, 16)?;
    let pixel_format = u32_at(body, 20)?;
    let bitmap_type = u32_at(body, 24)?;
    let pixels = body.get(28..)?;
    let area = width.unsigned_abs() as u64 * height.unsigned_abs() as u64;
    match bitmap_type {
        BITMAP_COMPRESSED => Some((area, (sniff(pixels)?, pixels.to_vec()))),
        BITMAP_PIXELS => {
            let rgba = to_rgba(pixels, width, height, stride, pixel_format, false)?;
            Some((area, (ImageFormat::Png, png::encode(width as u32, height as u32, &rgba))))
        }
        _ => None,
    }
}

/// The DIB of an `EMR_STRETCHDIBITS` record.
fn stretch_dibits(record: &[u8]) -> Option<Candidate> {
    // GDI itself ignores the fourth byte of 32-bit pixels here, which is
    // usually 0; a bitmap that does fill it in carries its transparency there.
    let (width, height, rgba) = dib(record, u32_at(record, 48)?, u32_at(record, 56)?, u32_at(record, 60)?, Alpha::IfPresent)?;
    Some(png_candidate(width, height, &rgba))
}

/// The source bitmap of an `EMR_ALPHABLEND` record, with the blend's
/// per-pixel and constant alpha applied.
fn alpha_blend(record: &[u8]) -> Option<Candidate> {
    let blend = record.get(40..44)?;
    let (constant, per_pixel) = (blend[2], blend[3] & AC_SRC_ALPHA != 0);
    let alpha = if per_pixel { Alpha::Premultiplied } else { Alpha::Opaque };
    let (width, height, mut rgba) = dib(record, u32_at(record, 84)?, u32_at(record, 92)?, u32_at(record, 96)?, alpha)?;
    if constant < 255 {
        for p in rgba.chunks_exact_mut(4) {
            p[3] = ((p[3] as u32 * constant as u32 + 127) / 255) as u8;
        }
    }
    Some(png_candidate(width, height, &rgba))
}

/// `rgba` as a PNG candidate.
fn png_candidate(width: u32, height: u32, rgba: &[u8]) -> Candidate {
    (width as u64 * height as u64, (ImageFormat::Png, png::encode(width, height, rgba)))
}

/// How the fourth byte of a 32-bit DIB pixel is read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Alpha {
    /// Ignored: every pixel is opaque.
    Opaque,
    /// Premultiplied alpha.
    Premultiplied,
    /// Alpha when any pixel has some (premultiplied unless a colour exceeds
    /// its alpha), otherwise ignored.
    IfPresent,
}

/// The width, height and straight RGBA pixels of an uncompressed DIB inside
/// `record`: its `BITMAPINFO` at `bmi_offset` and its pixels at `bits_offset`.
fn dib(record: &[u8], bmi_offset: u32, bits_offset: u32, bits_size: u32, alpha: Alpha) -> Option<(u32, u32, Vec<u8>)> {
    let (bmi_offset, bits_offset, bits_size) = (bmi_offset as usize, bits_offset as usize, bits_size as usize);
    let header = record.get(bmi_offset..)?;
    let width = i32_at(header, 4)?;
    let height = i32_at(header, 8)?;
    let bit_count = u16_at(header, 14)?;
    let compression = u32_at(header, 16)?;
    if compression != 0 {
        return None;
    }
    let stride = ((width.unsigned_abs() * bit_count as u32).div_ceil(32) * 4) as i32;
    let bits = record.get(bits_offset..bits_offset + bits_size)?;
    let format = match (bit_count, alpha) {
        (24, _) => PIXEL_RGB24,
        (32, Alpha::Opaque) => PIXEL_RGB32,
        (32, Alpha::Premultiplied) => PIXEL_PARGB32,
        (32, Alpha::IfPresent) => {
            let pixels = || bits.chunks_exact(4);
            if !pixels().any(|p| p[3] != 0) {
                PIXEL_RGB32
            } else if pixels().any(|p| p[0].max(p[1]).max(p[2]) > p[3]) {
                PIXEL_ARGB32
            } else {
                PIXEL_PARGB32
            }
        }
        _ => return None,
    };
    // DIB rows run bottom-up unless the height is negative.
    let rgba = to_rgba(bits, width, height.abs(), stride, format, height > 0)?;
    Some((width as u32, height.unsigned_abs(), rgba))
}

/// Convert BGR(A) rows to straight RGBA.
fn to_rgba(data: &[u8], width: i32, height: i32, stride: i32, format: u32, bottom_up: bool) -> Option<Vec<u8>> {
    if width <= 0 || height <= 0 || width > 32_768 || height > 32_768 {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    let bytes_per_pixel = match format {
        PIXEL_ARGB32 | PIXEL_PARGB32 | PIXEL_RGB32 => 4,
        PIXEL_RGB24 => 3,
        _ => return None,
    };
    let stride = stride.unsigned_abs() as usize;
    if stride < w * bytes_per_pixel || data.len() < stride * (h - 1) + w * bytes_per_pixel {
        return None;
    }
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = if bottom_up { h - 1 - y } else { y };
        let line = &data[row * stride..row * stride + w * bytes_per_pixel];
        for p in line.chunks_exact(bytes_per_pixel) {
            let (b, g, r) = (p[0], p[1], p[2]);
            let a = if bytes_per_pixel == 4 && format != PIXEL_RGB32 { p[3] } else { 255 };
            if format == PIXEL_PARGB32 && a > 0 && a < 255 {
                // Undo premultiplication.
                let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
                out.extend_from_slice(&[un(r), un(g), un(b), a]);
            } else {
                out.extend_from_slice(&[r, g, b, a]);
            }
        }
    }
    Some(out)
}

/// The format of an image file, from its signature.
fn sniff(data: &[u8]) -> Option<ImageFormat> {
    Some(match data {
        [0x89, b'P', b'N', b'G', ..] => ImageFormat::Png,
        [0xff, 0xd8, ..] => ImageFormat::Jpeg,
        [b'G', b'I', b'F', ..] => ImageFormat::Gif,
        [b'B', b'M', ..] => ImageFormat::Bmp,
        [b'I', b'I', 42, 0, ..] | [b'M', b'M', 0, 42, ..] => ImageFormat::Tiff,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A metafile whose only content is one EMF+ image object.
    fn emf_with_plus_image(image: &[u8]) -> Vec<u8> {
        let mut plus = Vec::new();
        plus.extend(EMFPLUS_OBJECT.to_le_bytes());
        plus.extend((OBJECT_IMAGE << 8).to_le_bytes());
        plus.extend((12 + image.len() as u32).to_le_bytes());
        plus.extend((image.len() as u32).to_le_bytes());
        plus.extend(image);

        let mut emf = vec![0u8; 88];
        emf[0..4].copy_from_slice(&EMR_HEADER.to_le_bytes());
        emf[4..8].copy_from_slice(&88u32.to_le_bytes());
        emf[40..44].copy_from_slice(b" EMF");
        let comment_len = 16 + plus.len();
        emf.extend(EMR_COMMENT.to_le_bytes());
        emf.extend((comment_len as u32).to_le_bytes());
        emf.extend((plus.len() as u32 + 4).to_le_bytes());
        emf.extend(b"EMF+");
        emf.extend(&plus);
        emf.extend(EMR_EOF.to_le_bytes());
        emf.extend(20u32.to_le_bytes());
        emf.extend([0u8; 12]);
        emf
    }

    fn bitmap_object(width: i32, height: i32, format: u32, kind: u32, data: &[u8]) -> Vec<u8> {
        let mut o = Vec::new();
        o.extend(0xdbc0_1002u32.to_le_bytes()); // version
        o.extend(IMAGE_BITMAP.to_le_bytes());
        o.extend(width.to_le_bytes());
        o.extend(height.to_le_bytes());
        o.extend((width * 4).to_le_bytes());
        o.extend(format.to_le_bytes());
        o.extend(kind.to_le_bytes());
        o.extend(data);
        o
    }

    #[test]
    fn extracts_premultiplied_pixels_as_png() {
        // 2×1: opaque red, half-transparent premultiplied blue.
        let pixels = [0, 0, 255, 255, 64, 0, 0, 128];
        let emf = emf_with_plus_image(&bitmap_object(2, 1, PIXEL_PARGB32, BITMAP_PIXELS, &pixels));
        let (format, png) = extract_image(&emf).expect("an image");
        assert_eq!(format, ImageFormat::Png);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(png::decode_rgba_for_test(&png), vec![255, 0, 0, 255, 0, 0, 128, 128]);
    }

    #[test]
    fn passes_compressed_images_through() {
        let jpeg = [0xff, 0xd8, 0xff, 0xe0, 1, 2, 3];
        let emf = emf_with_plus_image(&bitmap_object(1, 1, PIXEL_ARGB32, BITMAP_COMPRESSED, &jpeg));
        assert_eq!(extract_image(&emf), Some((ImageFormat::Jpeg, jpeg.to_vec())));
    }

    /// A metafile drawing one GDI bitmap record of `kind`: a 32-bit
    /// bottom-up DIB of `width` × 1 pixels placed after `fixed` bytes, with
    /// `fill` writing the record's own fields.
    fn emf_with_gdi_bitmap(kind: u32, fixed: usize, offsets: [usize; 3], width: i32, pixels: &[u8], fill: impl FnOnce(&mut [u8])) -> Vec<u8> {
        let mut record = vec![0u8; fixed + 40];
        fill(&mut record);
        record[fixed..fixed + 4].copy_from_slice(&40u32.to_le_bytes());
        record[fixed + 4..fixed + 8].copy_from_slice(&width.to_le_bytes());
        record[fixed + 8..fixed + 12].copy_from_slice(&1i32.to_le_bytes());
        record[fixed + 12..fixed + 14].copy_from_slice(&1u16.to_le_bytes());
        record[fixed + 14..fixed + 16].copy_from_slice(&32u16.to_le_bytes());
        let [bmi, bits, bits_size] = offsets;
        record[bmi..bmi + 4].copy_from_slice(&(fixed as u32).to_le_bytes());
        record[bits..bits + 4].copy_from_slice(&(fixed as u32 + 40).to_le_bytes());
        record[bits_size..bits_size + 4].copy_from_slice(&(pixels.len() as u32).to_le_bytes());
        record.extend(pixels);
        record[0..4].copy_from_slice(&kind.to_le_bytes());
        let size = record.len() as u32;
        record[4..8].copy_from_slice(&size.to_le_bytes());

        let mut emf = vec![0u8; 88];
        emf[0..4].copy_from_slice(&EMR_HEADER.to_le_bytes());
        emf[4..8].copy_from_slice(&88u32.to_le_bytes());
        emf[40..44].copy_from_slice(b" EMF");
        emf.extend(record);
        emf.extend(EMR_EOF.to_le_bytes());
        emf.extend(20u32.to_le_bytes());
        emf.extend([0u8; 12]);
        emf
    }

    fn stretch_dibits_emf(width: i32, pixels: &[u8]) -> Vec<u8> {
        emf_with_gdi_bitmap(EMR_STRETCHDIBITS, 80, [48, 56, 60], width, pixels, |_| {})
    }

    fn pixels_of(emf: &[u8]) -> Vec<u8> {
        let (format, png) = extract_image(emf).expect("an image");
        assert_eq!(format, ImageFormat::Png);
        png::decode_rgba_for_test(&png)
    }

    #[test]
    fn dib_without_alpha_is_opaque() {
        // BGRX with the fourth byte left 0, as GDI usually writes it.
        assert_eq!(pixels_of(&stretch_dibits_emf(2, &[0, 0, 255, 0, 255, 0, 0, 0])), vec![255, 0, 0, 255, 0, 0, 255, 255]);
    }

    #[test]
    fn dib_with_alpha_keeps_its_transparency() {
        // Opaque red, and a clear pixel.
        assert_eq!(pixels_of(&stretch_dibits_emf(2, &[0, 0, 255, 255, 0, 0, 0, 0])), vec![255, 0, 0, 255, 0, 0, 0, 0]);
        // Premultiplied half-transparent blue.
        assert_eq!(pixels_of(&stretch_dibits_emf(1, &[64, 0, 0, 128])), vec![0, 0, 128, 128]);
        // A colour brighter than its alpha cannot be premultiplied: straight.
        assert_eq!(pixels_of(&stretch_dibits_emf(1, &[200, 0, 0, 128])), vec![0, 0, 200, 128]);
    }

    #[test]
    fn alpha_blend_applies_pixel_and_constant_alpha() {
        let blend = |constant: u8, format: u8| move |r: &mut [u8]| r[40..44].copy_from_slice(&[0, 0, constant, format]);
        let pixels = [64, 0, 0, 128, 0, 0, 255, 255];
        let emf = emf_with_gdi_bitmap(EMR_ALPHABLEND, 108, [84, 92, 96], 2, &pixels, blend(255, AC_SRC_ALPHA));
        assert_eq!(pixels_of(&emf), vec![0, 0, 128, 128, 255, 0, 0, 255]);
        // Half the constant alpha on top.
        let emf = emf_with_gdi_bitmap(EMR_ALPHABLEND, 108, [84, 92, 96], 2, &pixels, blend(128, AC_SRC_ALPHA));
        assert_eq!(pixels_of(&emf), vec![0, 0, 128, 64, 255, 0, 0, 128]);
        // Without per-pixel alpha only the constant counts.
        let emf = emf_with_gdi_bitmap(EMR_ALPHABLEND, 108, [84, 92, 96], 1, &[0, 0, 255, 0], blend(51, 0));
        assert_eq!(pixels_of(&emf), vec![255, 0, 0, 51]);
    }

    #[test]
    fn rejects_non_emf() {
        assert_eq!(extract_image(b"not a metafile"), None);
    }
}
