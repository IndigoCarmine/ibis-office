// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pictures of an old workbook (`.xls`), from the public specification (MS-XLS, MS-ODRAW): the
//! images of the workbook's drawing group (`MSODRAWINGGROUP`: the BLIP store) and, on each sheet, the
//! shapes of its drawing (`MSODRAWING`) that show one, with their anchors.

use crate::xls::u16_at;

/// An image of the BLIP store: its file extension and bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub ext: &'static str,
    pub data: Vec<u8>,
}

/// A shape of a sheet's drawing that shows a picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Picture {
    /// The image, 1 for the first of the store.
    pub blip: usize,
    /// The anchor: first column, offset in 1/1024 of its width, first row, offset in 1/256 of its height,
    /// then the same for the last.
    pub anchor: [u16; 8],
}

struct Record<'a> {
    version: u16,
    instance: u16,
    kind: u16,
    data: &'a [u8],
}

/// The records of an OfficeArt stream, one level.
fn records(mut d: &[u8]) -> Vec<Record<'_>> {
    let mut out = Vec::new();
    while d.len() >= 8 {
        let head = u16_at(d, 0).unwrap_or(0);
        let kind = u16_at(d, 2).unwrap_or(0);
        let len = u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as usize;
        let Some(data) = d.get(8..8 + len) else { break };
        out.push(Record { version: head & 0xF, instance: head >> 4, kind, data });
        d = &d[8 + len..];
    }
    out
}

/// A DIB (the bitmap without a file header) as a `.bmp` file.
fn dib_to_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    let size = u32::from_le_bytes(dib.get(0..4)?.try_into().ok()?) as usize;
    let bits = u16_at(dib, 14)? as u32;
    let colors = u32::from_le_bytes(dib.get(32..36)?.try_into().ok()?);
    let palette = if bits <= 8 { (if colors == 0 { 1u32 << bits } else { colors }) * 4 } else { 0 };
    let offset = 14 + size as u32 + palette;
    let mut out = Vec::with_capacity(14 + dib.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(14 + dib.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}

/// The images of the drawing group, in the BLIP store's order (`None` for one this does not read: a metafile,
/// or one stored outside).
pub(crate) fn read_blips(group: &[u8]) -> Vec<Option<Image>> {
    let mut out = Vec::new();
    for dgg in records(group).into_iter().filter(|r| r.kind == 0xF000) {
        for store in records(dgg.data).into_iter().filter(|r| r.kind == 0xF001) {
            for entry in records(store.data) {
                out.push(blip_of(&entry));
            }
        }
    }
    out
}

fn blip_of(entry: &Record<'_>) -> Option<Image> {
    // A file block: a BSE (header of 36 bytes, a name, the BLIP) or a BLIP itself.
    let blip = if entry.kind == 0xF007 {
        let name = *entry.data.get(33)? as usize;
        records(entry.data.get(36 + name..)?).into_iter().next()?
    } else {
        Record { version: entry.version, instance: entry.instance, kind: entry.kind, data: entry.data }
    };
    // Two identifiers when the instance's low bit is set.
    let uids = if blip.instance & 1 == 1 { 32 } else { 16 };
    let body = blip.data.get(uids + 1..)?;
    match blip.kind {
        0xF01D | 0xF02A => Some(Image { ext: "jpeg", data: body.to_vec() }),
        0xF01E => Some(Image { ext: "png", data: body.to_vec() }),
        0xF01F => dib_to_bmp(body).map(|data| Image { ext: "bmp", data }),
        0xF029 => Some(Image { ext: "tiff", data: body.to_vec() }),
        _ => None,
    }
}

/// The pictures of a sheet's drawing, in order.
pub(crate) fn read_pictures(drawing: &[u8]) -> Vec<Picture> {
    let mut out = Vec::new();
    fn walk(d: &[u8], out: &mut Vec<Picture>) {
        for r in records(d) {
            match r.kind {
                0xF004 => {
                    let mut blip = None;
                    let mut anchor = None;
                    for part in records(r.data) {
                        match part.kind {
                            // The shape's properties: a 6-byte entry each (id, value); id 0x0104 is the picture.
                            0xF00B => {
                                for k in 0..part.instance as usize {
                                    let (Some(id), Some(v)) = (u16_at(part.data, k * 6), part.data.get(k * 6 + 2..k * 6 + 6)) else { break };
                                    if id & 0x3FFF == 0x0104 {
                                        blip = Some(u32::from_le_bytes([v[0], v[1], v[2], v[3]]) as usize);
                                    }
                                }
                            }
                            0xF010 => {
                                if part.data.len() >= 18 {
                                    let at = |k: usize| u16_at(part.data, 2 + k * 2).unwrap_or(0);
                                    anchor = Some([at(0), at(1), at(2), at(3), at(4), at(5), at(6), at(7)]);
                                }
                            }
                            _ => {}
                        }
                    }
                    if let (Some(blip), Some(anchor)) = (blip, anchor)
                        && blip > 0
                    {
                        out.push(Picture { blip, anchor });
                    }
                }
                _ if r.version == 0xF => walk(r.data, out),
                _ => {}
            }
        }
    }
    walk(drawing, &mut out);
    out
}

/// The anchor of the last shape of a drawing piece: first column, offset, first row, offset, then the last.
/// A piece may stop inside a container (the first one holds the start of the group, whose length covers
/// every shape of the sheet): what is there is read.
pub(crate) fn last_anchor(drawing: &[u8]) -> Option<[u16; 8]> {
    fn walk(mut d: &[u8], out: &mut Option<[u16; 8]>) {
        while d.len() >= 8 {
            let head = u16_at(d, 0).unwrap_or(0);
            let kind = u16_at(d, 2).unwrap_or(0);
            let len = u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as usize;
            let container = head & 0xF == 0xF;
            let end = (8 + len).min(d.len());
            let body = &d[8..end];
            if kind == 0xF010 && body.len() >= 18 {
                let at = |k: usize| u16_at(body, 2 + k * 2).unwrap_or(0);
                *out = Some([at(0), at(1), at(2), at(3), at(4), at(5), at(6), at(7)]);
            } else if container {
                walk(body, out);
            }
            d = &d[end..];
        }
    }
    let mut out = None;
    walk(drawing, &mut out);
    out
}
