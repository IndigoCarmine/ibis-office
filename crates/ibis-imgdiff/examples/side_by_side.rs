// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Put a reference picture, a candidate and their differences side by side
//! (left: reference, middle: candidate, right: differences outlined), as the
//! coverage crops are laid out (tasks/doc/testRule.md).
//!
//!   side_by_side <reference.png> <candidate.png> <out.png>
//!
//! The candidate is resized to the reference's size. Prints the differing
//! ratio and the number of differing regions.

use ibis_imgdiff::{Options, compare, outline, resize_to};
use image::{Rgba, RgbaImage};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, reference, candidate, out] = &args[..] else {
        eprintln!("usage: side_by_side <reference.png> <candidate.png> <out.png>");
        std::process::exit(2);
    };
    let a = image::open(reference).expect("reference").to_rgba8();
    let b = image::open(candidate).expect("candidate").to_rgba8();
    let (w, h) = a.dimensions();
    let b = if b.dimensions() == (w, h) { b } else { resize_to(&b, w, h) };
    let report = compare(&a, &b, &Options::default()).expect("compare");
    let mut side = RgbaImage::from_pixel(w * 3 + 8, h, Rgba([255, 255, 255, 255]));
    image::imageops::overlay(&mut side, &a, 0, 0);
    image::imageops::overlay(&mut side, &b, (w + 4) as i64, 0);
    image::imageops::overlay(&mut side, &outline(&a, &report.regions), (2 * w + 8) as i64, 0);
    side.save(out).expect("save");
    println!("{}\t{:.5}\t{}", out, report.differing_ratio(), report.regions.len());
}
