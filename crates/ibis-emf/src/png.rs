// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A minimal PNG encoder for 8-bit RGBA pixels.

const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend(kind);
    out.extend(data);
    let crc = crc32(&out[start..]);
    out.extend(crc.to_be_bytes());
}

/// Encode `rgba` (`width * height * 4` bytes, rows top to bottom).
pub(crate) fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let row = width as usize * 4;
    // Each row starts with filter type 0 (none).
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgba.chunks_exact(row) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut header = Vec::with_capacity(13);
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([8, 6, 0, 0, 0]); // 8-bit, RGBA, deflate, adaptive filtering, no interlace

    let mut out = SIGNATURE.to_vec();
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// The pixels of a PNG written by [`encode`].
#[cfg(test)]
pub(crate) fn decode_rgba_for_test(png: &[u8]) -> Vec<u8> {
    let mut at = SIGNATURE.len();
    let mut idat = Vec::new();
    let mut width = 0;
    while at + 8 <= png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        let kind = &png[at + 4..at + 8];
        let data = &png[at + 8..at + 8 + len];
        assert_eq!(crc32(&png[at + 4..at + 8 + len]), u32::from_be_bytes(png[at + 8 + len..at + 12 + len].try_into().unwrap()));
        match kind {
            b"IHDR" => width = u32::from_be_bytes(data[0..4].try_into().unwrap()) as usize,
            b"IDAT" => idat.extend_from_slice(data),
            _ => {}
        }
        at += 12 + len;
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&idat).unwrap();
    raw.chunks_exact(width * 4 + 1).flat_map(|line| line[1..].to_vec()).collect()
}
