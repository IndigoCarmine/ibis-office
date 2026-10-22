// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Compact Table Format to TrueType (MicroType Express, sections 4 to 7).
//!
//! The first block is an sfnt whose 'glyf' and 'cvt ' tables are recoded
//! and whose 'loca' is empty; the second holds every glyph's initial push
//! values and the third their remaining instructions. This rebuilds a plain
//! TrueType file. 'hdmx' and 'VDMX' (device metrics caches) and 'DSIG' (no
//! longer valid) are left out; renderers do not need them.

use crate::Error;

fn err(message: &'static str) -> Error {
    Error(message)
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u8(&mut self) -> Result<u8, Error> {
        let v = *self.data.get(self.pos).ok_or(err("CTF のデータが途中で終わっています"))?;
        self.pos += 1;
        Ok(v)
    }

    fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_be_bytes([self.u8()?, self.u8()?]))
    }

    fn i16(&mut self) -> Result<i16, Error> {
        Ok(self.u16()? as i16)
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let s = self.data.get(self.pos..self.pos + n).ok_or(err("CTF のデータが途中で終わっています"))?;
        self.pos += n;
        Ok(s)
    }

    /// 255USHORT (6.1.1).
    fn u255(&mut self) -> Result<u16, Error> {
        Ok(match self.u8()? {
            253 => self.u16()?,
            255 => 253 + self.u8()? as u16,
            254 => 506 + self.u8()? as u16,
            code => code as u16,
        })
    }

    /// 255SHORT (6.1.2).
    fn s255(&mut self) -> Result<i16, Error> {
        let code = self.u8()?;
        if code == 253 {
            return self.i16();
        }
        let (negative, code) = if code == 250 { (true, self.u8()?) } else { (false, code) };
        let value = match code {
            255 => 250 + self.u8()? as i16,
            254 => 500 + self.u8()? as i16,
            code => code as i16,
        };
        Ok(if negative { -value } else { value })
    }
}

fn be16(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_be_bytes());
}

fn be32(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_be_bytes());
}

/// `count` initial push values with their hop codes expanded (6.2.4).
fn read_push_data(r: &mut Reader, count: usize) -> Result<Vec<i16>, Error> {
    const HOP3: u8 = 251;
    const HOP4: u8 = 252;
    let mut values: Vec<i16> = Vec::with_capacity(count);
    while values.len() < count {
        let code = *r.data.get(r.pos).ok_or(err("プッシュ データが途中で終わっています"))?;
        if code == HOP3 || code == HOP4 {
            r.pos += 1;
            let a = *values.len().checked_sub(2).and_then(|i| values.get(i)).ok_or(err("ホップ コードの参照先がありません"))?;
            values.push(a);
            values.push(r.s255()?);
            values.push(a);
            if code == HOP4 {
                values.push(r.s255()?);
                values.push(a);
            }
        } else {
            values.push(r.s255()?);
        }
    }
    values.truncate(count);
    Ok(values)
}

/// TrueType instructions pushing `values` onto the stack.
fn push_instructions(values: &[i16], out: &mut Vec<u8>) {
    for chunk in values.chunks(255) {
        if chunk.iter().all(|v| (0..=255).contains(v)) {
            if chunk.len() <= 8 {
                out.push(0xB0 + chunk.len() as u8 - 1); // PUSHB[n]
            } else {
                out.push(0x40); // NPUSHB
                out.push(chunk.len() as u8);
            }
            out.extend(chunk.iter().map(|v| *v as u8));
        } else {
            if chunk.len() <= 8 {
                out.push(0xB8 + chunk.len() as u8 - 1); // PUSHW[n]
            } else {
                out.push(0x41); // NPUSHW
                out.push(chunk.len() as u8);
            }
            for v in chunk {
                out.extend(v.to_be_bytes());
            }
        }
    }
}

/// One point's (dx, dy) from its flag and data bytes (5.11). The triplet
/// table is the one WOFF 2.0 later adopted.
fn triplet(flag: u8, data: &[u8]) -> (i32, i32) {
    let sign = |bit: u8, v: i32| if bit & 1 != 0 { v } else { -v };
    let f = flag as i32;
    let b = |i: usize| data[i] as i32;
    if f < 10 {
        (0, sign(flag, ((f & 14) << 7) + b(0)))
    } else if f < 20 {
        (sign(flag, (((f - 10) & 14) << 7) + b(0)), 0)
    } else if f < 84 {
        let b0 = f - 20;
        (sign(flag, 1 + (b0 & 0x30) + (b(0) >> 4)), sign(flag >> 1, 1 + ((b0 & 0x0c) << 2) + (b(0) & 0x0f)))
    } else if f < 120 {
        let b0 = f - 84;
        (sign(flag, 1 + ((b0 / 12) << 8) + b(0)), sign(flag >> 1, 1 + (((b0 % 12) >> 2) << 8) + b(1)))
    } else if f < 124 {
        (sign(flag, (b(0) << 4) + (b(1) >> 4)), sign(flag >> 1, ((b(1) & 0x0f) << 8) + b(2)))
    } else {
        (sign(flag, (b(0) << 8) + b(1)), sign(flag >> 1, (b(2) << 8) + b(3)))
    }
}

/// Data bytes after the flag byte, per triplet index.
fn triplet_bytes(flag: u8) -> usize {
    match flag {
        0..=83 => 1,
        84..=119 => 2,
        120..=123 => 3,
        _ => 4,
    }
}

/// Instruction streams shared by all glyphs, read in glyph order.
struct Streams<'a> {
    push: Reader<'a>,
    code: Reader<'a>,
    max_instructions: usize,
}

impl Streams<'_> {
    /// The glyph's instructions: its push burst, then the rest of its code.
    fn instructions(&mut self, glyf: &mut Reader, out: &mut Vec<u8>) -> Result<(), Error> {
        let push_count = glyf.u255()? as usize;
        let code_size = glyf.u255()? as usize;
        let mut program = Vec::new();
        push_instructions(&read_push_data(&mut self.push, push_count)?, &mut program);
        program.extend_from_slice(self.code.bytes(code_size)?);
        if program.len() > u16::MAX as usize {
            return Err(err("グリフの命令が長すぎます"));
        }
        self.max_instructions = self.max_instructions.max(program.len());
        be16(out, program.len() as u16);
        out.extend(program);
        Ok(())
    }
}

fn simple_glyph(contours: i16, glyf: &mut Reader, streams: &mut Streams, out: &mut Vec<u8>) -> Result<(), Error> {
    let (contours, bbox) = if contours == 0x7FFF {
        let n = glyf.i16()?;
        (n, Some([glyf.i16()?, glyf.i16()?, glyf.i16()?, glyf.i16()?]))
    } else {
        (contours, None)
    };
    if contours <= 0 {
        return Err(err("輪郭の数が不正です"));
    }
    // End point of the first contour, then point counts of the others.
    let mut end_points = Vec::with_capacity(contours as usize);
    let mut last: i64 = -1;
    for i in 0..contours {
        let v = glyf.u255()? as i64;
        last = if i == 0 { v } else { last + v };
        end_points.push(last);
    }
    let points = (last + 1).max(0) as usize;
    let flags = glyf.bytes(points)?;
    let mut coords = Vec::with_capacity(points);
    let (mut x, mut y) = (0i32, 0i32);
    for &flag in flags {
        let data = glyf.bytes(triplet_bytes(flag & 0x7f))?;
        let (dx, dy) = triplet(flag & 0x7f, data);
        x += dx;
        y += dy;
        coords.push((x, y, flag & 0x80 == 0));
    }
    let [x_min, y_min, x_max, y_max] = bbox.unwrap_or_else(|| {
        let xs = coords.iter().map(|c| c.0);
        let ys = coords.iter().map(|c| c.1);
        [
            xs.clone().min().unwrap_or(0) as i16,
            ys.clone().min().unwrap_or(0) as i16,
            xs.max().unwrap_or(0) as i16,
            ys.max().unwrap_or(0) as i16,
        ]
    });

    be16(out, contours as u16);
    for v in [x_min, y_min, x_max, y_max] {
        be16(out, v as u16);
    }
    for e in &end_points {
        be16(out, *e as u16);
    }
    streams.instructions(glyf, out)?;
    // Flags and coordinates: each delta as a byte when it fits, else a word.
    let (mut xs, mut ys, mut fl) = (Vec::new(), Vec::new(), Vec::new());
    let (mut px, mut py) = (0i32, 0i32);
    for &(x, y, on_curve) in &coords {
        let (dx, dy) = (x - px, y - py);
        (px, py) = (x, y);
        let mut f = on_curve as u8;
        if dx == 0 {
            f |= 0x10;
        } else if dx.abs() < 256 {
            f |= 0x02 | if dx > 0 { 0x10 } else { 0 };
            xs.push(dx.unsigned_abs() as u8);
        } else {
            xs.extend((dx as i16).to_be_bytes());
        }
        if dy == 0 {
            f |= 0x20;
        } else if dy.abs() < 256 {
            f |= 0x04 | if dy > 0 { 0x20 } else { 0 };
            ys.push(dy.unsigned_abs() as u8);
        } else {
            ys.extend((dy as i16).to_be_bytes());
        }
        fl.push(f);
    }
    out.extend(fl);
    out.extend(xs);
    out.extend(ys);
    Ok(())
}

fn composite_glyph(glyf: &mut Reader, streams: &mut Streams, out: &mut Vec<u8>) -> Result<(), Error> {
    const ARGS_ARE_WORDS: u16 = 0x0001;
    const HAVE_SCALE: u16 = 0x0008;
    const MORE_COMPONENTS: u16 = 0x0020;
    const HAVE_XY_SCALE: u16 = 0x0040;
    const HAVE_2X2: u16 = 0x0080;
    const HAVE_INSTRUCTIONS: u16 = 0x0100;

    be16(out, 0xFFFF);
    out.extend_from_slice(glyf.bytes(8)?); // bounding box
    let mut have_instructions = false;
    loop {
        let start = glyf.pos;
        let flags = glyf.u16()?;
        glyf.u16()?; // glyph index
        let mut extra = if flags & ARGS_ARE_WORDS != 0 { 4 } else { 2 };
        if flags & HAVE_SCALE != 0 {
            extra += 2;
        } else if flags & HAVE_XY_SCALE != 0 {
            extra += 4;
        } else if flags & HAVE_2X2 != 0 {
            extra += 8;
        }
        glyf.bytes(extra)?;
        out.extend_from_slice(&glyf.data[start..glyf.pos]);
        have_instructions |= flags & HAVE_INSTRUCTIONS != 0;
        if flags & MORE_COMPONENTS == 0 {
            break;
        }
    }
    if have_instructions {
        streams.instructions(glyf, out)?;
    }
    Ok(())
}

/// Decode the 'glyf' table; returns it and the long 'loca'.
fn decode_glyf(glyf: &[u8], num_glyphs: usize, push: &[u8], code: &[u8]) -> Result<(Vec<u8>, Vec<u8>, usize), Error> {
    let mut r = Reader::new(glyf);
    let mut streams = Streams { push: Reader::new(push), code: Reader::new(code), max_instructions: 0 };
    let mut out = Vec::with_capacity(glyf.len() * 2);
    let mut loca = Vec::with_capacity((num_glyphs + 1) * 4);
    for _ in 0..num_glyphs {
        be32(&mut loca, out.len() as u32);
        match r.i16()? {
            0 => {} // empty glyph
            -1 => composite_glyph(&mut r, &mut streams, &mut out)?,
            n if n > 0 => simple_glyph(n, &mut r, &mut streams, &mut out)?,
            _ => return Err(err("グリフの種類が不正です")),
        }
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    be32(&mut loca, out.len() as u32);
    Ok((out, loca, streams.max_instructions))
}

/// Decode 'cvt ' (5.2): a count, then values relative to the previous one.
fn decode_cvt(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut r = Reader::new(data);
    let count = r.u16()? as usize;
    let mut out = Vec::with_capacity(count * 2);
    let mut last: u16 = 0;
    for _ in 0..count {
        let code = r.u8()?;
        let delta: i32 = match code {
            0..=237 => code as i32,
            238 => r.i16()? as i32,
            239..=247 => -((r.u8()? as i32) + (code as i32 - 239) * 238),
            248..=255 => (r.u8()? as i32) + (code as i32 - 247) * 238,
        };
        last = last.wrapping_add(delta as u16);
        be16(&mut out, last);
    }
    Ok(out)
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, c| {
        let mut word = [0u8; 4];
        word[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// Rebuild a TrueType file from the three decompressed CTF blocks.
pub(crate) fn to_truetype(block1: &[u8], block2: &[u8], block3: &[u8]) -> Result<Vec<u8>, Error> {
    let mut r = Reader::new(block1);
    let version = u32::from_be_bytes(r.bytes(4)?.try_into().expect("4 bytes"));
    let count = r.u16()? as usize;
    r.bytes(6)?;
    let mut tables: Vec<([u8; 4], &[u8])> = Vec::with_capacity(count);
    for _ in 0..count {
        let tag: [u8; 4] = r.bytes(4)?.try_into().expect("4 bytes");
        r.bytes(4)?; // checksum
        let offset = u32::from_be_bytes(r.bytes(4)?.try_into().expect("4 bytes")) as usize;
        let length = u32::from_be_bytes(r.bytes(4)?.try_into().expect("4 bytes")) as usize;
        let data = block1.get(offset..offset + length).ok_or(err("CTF の表が範囲外です"))?;
        tables.push((tag, data));
    }
    let table = |tag: &[u8; 4]| tables.iter().find(|(t, _)| t == tag).map(|(_, d)| *d);
    let maxp = table(b"maxp").ok_or(err("maxp 表がありません"))?;
    let num_glyphs = u16::from_be_bytes([*maxp.get(4).unwrap_or(&0), *maxp.get(5).unwrap_or(&0)]) as usize;

    let mut out_tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    let mut max_instructions = 0;
    if let Some(glyf) = table(b"glyf") {
        let (glyf, loca, max) = decode_glyf(glyf, num_glyphs, block2, block3)?;
        max_instructions = max;
        out_tables.push((*b"glyf", glyf));
        out_tables.push((*b"loca", loca));
    }
    for (tag, data) in &tables {
        match tag {
            b"glyf" | b"loca" | b"hdmx" | b"VDMX" | b"DSIG" => {}
            b"cvt " => out_tables.push((*tag, decode_cvt(data)?)),
            b"head" => {
                // 'head' is 54 bytes; its fields are written at fixed offsets
                // here and once the font is laid out (checkSumAdjustment).
                if data.len() < 54 {
                    return Err(err("head 表が短すぎます"));
                }
                let mut head = data.to_vec();
                head[8..12].fill(0); // checkSumAdjustment, set below
                head[50..52].copy_from_slice(&1u16.to_be_bytes()); // long 'loca'
                out_tables.push((*tag, head));
            }
            b"maxp" => {
                let mut maxp = data.to_vec();
                if maxp.len() >= 28 {
                    let old = u16::from_be_bytes([maxp[26], maxp[27]]) as usize;
                    let size = old.max(max_instructions).min(u16::MAX as usize) as u16;
                    maxp[26..28].copy_from_slice(&size.to_be_bytes());
                }
                out_tables.push((*tag, maxp));
            }
            _ => out_tables.push((*tag, data.to_vec())),
        }
    }
    out_tables.sort_by_key(|t| t.0);

    let n = out_tables.len() as u16;
    let entry_selector = 15 - n.leading_zeros() as u16;
    let search_range = (1u16 << entry_selector) * 16;
    let mut font = Vec::new();
    be32(&mut font, version);
    be16(&mut font, n);
    be16(&mut font, search_range);
    be16(&mut font, entry_selector);
    be16(&mut font, n * 16 - search_range);
    let mut offset = 12 + 16 * out_tables.len();
    let mut head_at = None;
    for (tag, data) in &out_tables {
        font.extend(tag);
        be32(&mut font, checksum(data));
        be32(&mut font, offset as u32);
        be32(&mut font, data.len() as u32);
        if tag == b"head" {
            head_at = Some(offset);
        }
        offset += data.len().div_ceil(4) * 4;
    }
    for (_, data) in &out_tables {
        font.extend(data);
        while font.len() % 4 != 0 {
            font.push(0);
        }
    }
    if let Some(at) = head_at {
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&font));
        font[at + 8..at + 12].copy_from_slice(&adjustment.to_be_bytes());
    }
    Ok(font)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_length_numbers() {
        let mut r = Reader::new(&[5, 255, 10, 254, 0, 253, 0x12, 0x34]);
        assert_eq!([r.u255().unwrap(), r.u255().unwrap(), r.u255().unwrap(), r.u255().unwrap()], [5, 263, 506, 0x1234]);
        let mut r = Reader::new(&[7, 250, 7, 255, 1, 250, 254, 2, 253, 0xFF, 0xFE]);
        assert_eq!([r.s255().unwrap(), r.s255().unwrap(), r.s255().unwrap(), r.s255().unwrap(), r.s255().unwrap()], [7, -7, 251, -502, -2]);
    }

    #[test]
    fn hop_codes_repeat_the_value_two_back() {
        // A, X1, Hop3, X2 -> A, X1, A, X2, A
        let mut r = Reader::new(&[9, 1, 251, 2]);
        assert_eq!(read_push_data(&mut r, 5).unwrap(), vec![9, 1, 9, 2, 9]);
        let mut r = Reader::new(&[9, 1, 252, 2, 3]);
        assert_eq!(read_push_data(&mut r, 7).unwrap(), vec![9, 1, 9, 2, 9, 3, 9]);
    }

    #[test]
    fn triplets_follow_the_spec_table() {
        // Index 0: dy = -(data), index 3: dy = +(256 + data).
        assert_eq!(triplet(0, &[5]), (0, -5));
        assert_eq!(triplet(3, &[5]), (0, 261));
        // Index 11: dx = +data.
        assert_eq!(triplet(11, &[7]), (7, 0));
        // Index 23: 4-bit x and y, both positive, delta 1: data 0x21.
        assert_eq!(triplet(23, &[0x21]), (3, 2));
        // Index 24: y delta 17, both negative.
        assert_eq!(triplet(24, &[0x00]), (-1, -17));
        // Index 127: two words, both positive.
        assert_eq!(triplet(127, &[0x01, 0x00, 0x00, 0x02]), (256, 2));
    }

    #[test]
    fn push_instructions_choose_byte_or_word() {
        let mut out = Vec::new();
        push_instructions(&[1, 2, 3], &mut out);
        assert_eq!(out, vec![0xB2, 1, 2, 3]);
        let mut out = Vec::new();
        push_instructions(&[-1], &mut out);
        assert_eq!(out, vec![0xB8, 0xFF, 0xFF]);
    }

    #[test]
    fn cvt_values_are_relative() {
        // 10, then +5, then -(238 + 2) = -240.
        let data = [0, 3, 10, 5, 240, 2];
        assert_eq!(decode_cvt(&data).unwrap(), [10u16, 15, (15i16 - 240) as u16].iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<_>>());
    }

    /// A first CTF block: an sfnt directory with `tables`, then their data.
    fn block1(tables: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        be32(&mut out, 0x0001_0000);
        be16(&mut out, tables.len() as u16);
        out.extend([0; 6]);
        let mut offset = 12 + 16 * tables.len();
        for (tag, data) in tables {
            out.extend(*tag);
            be32(&mut out, 0);
            be32(&mut out, offset as u32);
            be32(&mut out, data.len() as u32);
            offset += data.len();
        }
        for (_, data) in tables {
            out.extend(*data);
        }
        out
    }

    #[test]
    fn a_short_head_is_an_error() {
        // Empty 'head' and 'maxp', no 'glyf'.
        assert!(to_truetype(&block1(&[(b"head", &[]), (b"maxp", &[])]), &[], &[]).is_err());
        // A 'head' too short for its fixed fields, before another table.
        assert!(to_truetype(&block1(&[(b"head", &[0; 4]), (b"maxp", &[0, 0, 0x50, 0, 0, 0])]), &[], &[]).is_err());
        assert!(to_truetype(&block1(&[(b"head", &[0; 53]), (b"maxp", &[0; 6])]), &[], &[]).is_err());
    }

    #[test]
    fn head_gets_the_checksum_adjustment() {
        let font = to_truetype(&block1(&[(b"head", &[0xAA; 54]), (b"maxp", &[0, 0, 0x50, 0, 0, 0])]), &[], &[]).unwrap();
        assert_eq!(checksum(&font), 0xB1B0_AFBA);
        // Tables sorted by tag: 'head' first, after the two directory entries.
        let head = &font[12 + 16 * 2..][..54];
        assert_eq!(head[50..52], [0, 1]);
        assert_eq!(head[..8], [0xAA; 8]);
    }
}
