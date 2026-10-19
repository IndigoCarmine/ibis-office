// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Writes a fictitious EMF+ metafile of shapes for tests: `make_emfplus <out.emf>`.
//! It holds a gradient rectangle, a stroked and filled path, an ellipse, a pie, a spline, a bitmap, a rotated string and a clip.

fn f(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn u(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn rec(kind: u16, flags: u16, data: &[u8]) -> Vec<u8> {
    let mut data = data.to_vec();
    while data.len() % 4 != 0 {
        data.push(0);
    }
    let mut v = kind.to_le_bytes().to_vec();
    v.extend(flags.to_le_bytes());
    v.extend(((data.len() + 12) as u32).to_le_bytes());
    v.extend((data.len() as u32).to_le_bytes());
    v.extend(data);
    v
}

fn object(kind: u16, id: u16, data: &[u8]) -> Vec<u8> {
    rec(0x4008, (kind << 8) | id, data)
}

fn rectf(x: f32, y: f32, w: f32, h: f32) -> Vec<u8> {
    [f(x), f(y), f(w), f(h)].concat()
}

fn solid(argb: u32) -> Vec<u8> {
    [u(0xDBC0_1002), u(0), u(argb)].concat()
}

fn pen(width: f32, argb: u32, extra_flags: u32, extra: &[u8]) -> Vec<u8> {
    [u(0xDBC0_1002), u(0), u(extra_flags), u(2), f(width), extra.to_vec(), solid(argb)].concat()
}

fn linear(rect: [f32; 4], a: u32, b: u32) -> Vec<u8> {
    [u(0xDBC0_1002), u(4), u(0), u(4), rectf(rect[0], rect[1], rect[2], rect[3]), u(a), u(b), u(0), u(0)].concat()
}

fn path(points: &[(f32, f32)], types: &[u8]) -> Vec<u8> {
    let mut d = [u(0xDBC0_1002), u(points.len() as u32), u(0)].concat();
    for (x, y) in points {
        d.extend(f(*x));
        d.extend(f(*y));
    }
    d.extend(types);
    d
}

fn main() {
    let Some(out) = std::env::args().nth(1) else {
        eprintln!("usage: make_emfplus <out.emf>");
        std::process::exit(2);
    };
    let (w, h) = (400u32, 300u32);
    let mut plus: Vec<Vec<u8>> = Vec::new();
    plus.push(rec(0x4001, 0, &[u(0xDBC0_1002), u(0), u(96), u(96)].concat()));
    plus.push(rec(0x4030, 2, &f(1.0)));
    plus.push(rec(0x4009, 0, &u(0xFFFF_FFFF)));
    // 0: gradient brush, 1: solid orange, 2: pen, 3: dashed pen, 4: path, 5: font, 6: bitmap, 7: format
    plus.push(object(1, 0, &linear([20.0, 20.0, 160.0, 80.0], 0xFF2E_6FD8, 0xFFF4_B400)));
    plus.push(object(1, 1, &solid(0xFFE0_5A2B)));
    plus.push(object(2, 2, &pen(4.0, 0xFF12_2B5A, 0x08, &u(2))));
    plus.push(object(2, 3, &pen(2.0, 0xFF28_7D3C, 0x20, &u(1))));
    plus.push(object(3, 4, &path(&[(240.0, 30.0), (360.0, 40.0), (380.0, 110.0), (300.0, 130.0), (250.0, 90.0)], &[0, 1, 1, 1, 0x81])));
    let mut font = [u(0xDBC0_1002), f(20.0), u(0), u(1), u(0), u(5)].concat();
    for c in "Arial".encode_utf16() {
        font.extend(c.to_le_bytes());
    }
    plus.push(object(6, 5, &font));
    let mut bmp = [u(0xDBC0_1002), u(1), u(8), u(8), u(32), u(0x0026_200A), u(0)].concat();
    for y in 0..8u32 {
        for x in 0..8u32 {
            let on = (x + y) % 2 == 0;
            bmp.extend(if on { [0x3C, 0x8C, 0x28, 255] } else { [0xE6, 0xD2, 0xB4, 255] });
        }
    }
    plus.push(object(5, 6, &bmp));
    let format = [u(0xDBC0_1002), u(0), u(0), u(1), u(0), vec![0u8; 40]].concat();
    plus.push(object(7, 7, &format));
    // Gradient rectangle with an outline.
    plus.push(rec(0x400A, 0, &[u(0), u(1), rectf(20.0, 20.0, 160.0, 80.0)].concat()));
    plus.push(rec(0x400B, 2, &[u(1), rectf(20.0, 20.0, 160.0, 80.0)].concat()));
    // The pentagon: fill and outline.
    plus.push(rec(0x4014, 4, &u(1)));
    plus.push(rec(0x4015, 4, &u(2)));
    // An ellipse and a pie.
    plus.push(rec(0x400E, 0, &[u(1), rectf(20.0, 130.0, 120.0, 70.0)].concat()));
    plus.push(rec(0x4011, 3, &[f(20.0), f(250.0), rectf(160.0, 120.0, 90.0, 90.0)].concat()));
    // A spline through four points.
    let pts = [(20.0f32, 250.0f32), (80.0, 220.0), (140.0, 280.0), (200.0, 240.0)];
    let mut curve = [f(0.5), u(0), u(3), u(4)].concat();
    for (x, y) in pts {
        curve.extend(f(x));
        curve.extend(f(y));
    }
    plus.push(rec(0x4018, 3, &curve));
    // The bitmap scaled up, then a clipped copy.
    plus.push(rec(0x401A, 6, &[u(0), u(2), rectf(0.0, 0.0, 8.0, 8.0), rectf(280.0, 150.0, 80.0, 80.0)].concat()));
    plus.push(rec(0x4025, 0, &u(1)));
    plus.push(rec(0x4032, 0x0100, &rectf(270.0, 240.0, 60.0, 40.0)));
    plus.push(rec(0x400A, 0, &[u(1), u(1), rectf(260.0, 230.0, 100.0, 60.0)].concat()));
    plus.push(rec(0x4026, 0, &u(1)));
    // A rotated string.
    plus.push(rec(0x4025, 0, &u(2)));
    plus.push(rec(0x402F, 0, &f(-12.0)));
    let mut text = [u(0xFF1B_1B1B), u(7), u(9)].concat();
    text.extend(rectf(30.0, 200.0, 200.0, 30.0));
    for c in "EMF+ test".encode_utf16() {
        text.extend(c.to_le_bytes());
    }
    plus.push(rec(0x401C, 0x8005, &text));
    plus.push(rec(0x4026, 0, &u(2)));
    plus.push(rec(0x4002, 0, &[]));
    let body: Vec<u8> = plus.concat();
    let mut emf = vec![0u8; 88];
    emf[0..4].copy_from_slice(&1u32.to_le_bytes());
    emf[4..8].copy_from_slice(&88u32.to_le_bytes());
    for (i, v) in [0i32, 0, w as i32 - 1, h as i32 - 1].iter().enumerate() {
        emf[8 + i * 4..12 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    // The frame in 0.01 mm at 96 dpi.
    for (i, v) in [0i32, 0, (w as f64 * 2540.0 / 96.0) as i32, (h as f64 * 2540.0 / 96.0) as i32].iter().enumerate() {
        emf[24 + i * 4..28 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    emf[40..44].copy_from_slice(b" EMF");
    emf[44..48].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    for (at, v) in [(72, 1920i32), (76, 1080), (80, 508), (84, 285)] {
        emf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    emf.extend(70u32.to_le_bytes());
    emf.extend(((16 + body.len()) as u32).to_le_bytes());
    emf.extend(((4 + body.len()) as u32).to_le_bytes());
    emf.extend(b"EMF+");
    emf.extend(body);
    emf.extend(14u32.to_le_bytes());
    emf.extend(20u32.to_le_bytes());
    emf.extend([0u8; 12]);
    let size = emf.len() as u32;
    emf[52..56].copy_from_slice(&3u32.to_le_bytes());
    emf[56..58].copy_from_slice(&1u16.to_le_bytes());
    emf[48..52].copy_from_slice(&size.to_le_bytes());
    std::fs::write(out, emf).expect("write");
}
