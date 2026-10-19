// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The baseline of text comes from the metrics of the font that draws it (`set_font_metrics`).

fn u(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn f(v: f32) -> Vec<u8> {
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

fn string_emf(face: &str) -> Vec<u8> {
    let header = rec(0x4001, 0, &[u(0xDBC0_1002), u(0), u(96), u(96)].concat());
    let mut font = [u(0xDBC0_1002), f(10.0), u(2), u(0), u(0), u(face.encode_utf16().count() as u32)].concat();
    for c in face.encode_utf16() {
        font.extend(c.to_le_bytes());
    }
    let font = rec(0x4008, (6 << 8) | 0, &font);
    let mut text = [u(0xFF00_0000), u(0xFFFF_FFFF), u(2)].concat();
    for v in [0.0f32, 20.0, 100.0, 30.0] {
        text.extend(f(v));
    }
    for c in "Hi".encode_utf16() {
        text.extend(c.to_le_bytes());
    }
    let string = rec(0x401C, 0x8000, &text);
    let body = [header, font, string].concat();
    let mut h = vec![0u8; 88];
    h[0..4].copy_from_slice(&1u32.to_le_bytes());
    h[4..8].copy_from_slice(&88u32.to_le_bytes());
    for (i, v) in [0i32, 0, 99, 99].iter().enumerate() {
        h[8 + i * 4..12 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    h[40..44].copy_from_slice(b" EMF");
    for (at, v) in [(72, 100i32), (76, 100), (80, 26), (84, 26)] {
        h[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    let mut out = h;
    out.extend(70u32.to_le_bytes());
    out.extend(((16 + body.len()) as u32).to_le_bytes());
    out.extend(((4 + body.len()) as u32).to_le_bytes());
    out.extend(b"EMF+");
    out.extend(body);
    out.extend(14u32.to_le_bytes());
    out.extend(20u32.to_le_bytes());
    out.extend([0u8; 12]);
    out
}

fn baseline(svg: &str) -> f64 {
    let at = svg.find(" y=\"").expect("a text") + 4;
    svg[at..].split('"').next().unwrap().parse().unwrap()
}

#[test]
fn the_baseline_follows_the_metrics_of_the_font() {
    // Before the hook: the proportions of a common sans-serif (the em is 10 px, the text starts at y = 20).
    let before = ibis_emf::emf_to_svg(&string_emf("Tall Face")).expect("drawn");
    assert!((baseline(&before) - 29.05).abs() < 0.01, "{before}");
    ibis_emf::set_font_metrics(|face, _, _| (face == "Tall Face").then_some((1.5, 0.5)));
    let tall = ibis_emf::emf_to_svg(&string_emf("Tall Face")).expect("drawn");
    assert!((baseline(&tall) - 35.0).abs() < 0.01, "{tall}");
    // A font the hook does not know keeps the default.
    let other = ibis_emf::emf_to_svg(&string_emf("Other")).expect("drawn");
    assert!((baseline(&other) - 29.05).abs() < 0.01, "{other}");
}
