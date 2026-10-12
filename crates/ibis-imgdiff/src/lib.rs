// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Where two pictures differ: pixel colour differences grouped into rectangles.
//!
//! Each pixel's difference is the CIE76 colour distance (ΔE in L\*a\*b\*)
//! after compositing both pictures over white. Pixels over a threshold are
//! counted per square cell; cells with enough of them are marked, marked
//! cells within a merge distance are joined, and each group becomes one
//! rectangle. A rectangle is a place worth looking at, not a pixel-exact
//! outline.

use image::{Rgba, RgbaImage};

#[derive(Clone, Debug)]
pub struct Options {
    /// ΔE above which a pixel differs. About 2.3 is just noticeable; 10 is
    /// clearly different.
    pub threshold: f32,
    /// Side of the cells differing pixels are counted in, in pixels.
    pub cell: u32,
    /// Fraction of a cell's pixels that must differ for the cell to count.
    pub min_ratio: f32,
    /// Marked cells up to this many cells apart are joined into one rectangle.
    pub merge: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self { threshold: 10.0, cell: 8, min_ratio: 0.05, merge: 1 }
    }
}

/// A region where the pictures differ, in pixels of the first picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Pixels in the region over the threshold.
    pub differing_pixels: u64,
    /// Mean and largest ΔE of those pixels.
    pub mean_delta_e: f32,
    pub max_delta_e: f32,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub width: u32,
    pub height: u32,
    /// Largest regions (by differing pixels) first.
    pub regions: Vec<Region>,
    /// Pixels over the threshold in the whole picture.
    pub differing_pixels: u64,
}

impl Report {
    /// Fraction of all pixels over the threshold.
    pub fn differing_ratio(&self) -> f64 {
        self.differing_pixels as f64 / (self.width as f64 * self.height as f64).max(1.0)
    }

    /// The regions as tab-separated values with a header line.
    pub fn to_tsv(&self) -> String {
        let mut out = String::from("x\ty\twidth\theight\tdiffering_pixels\tmean_delta_e\tmax_delta_e\n");
        for r in &self.regions {
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.1}\n",
                r.x, r.y, r.width, r.height, r.differing_pixels, r.mean_delta_e, r.max_delta_e
            ));
        }
        out
    }
}

/// Compare `a` and `b`, which must be the same size (see [`resize_to`]).
pub fn compare(a: &RgbaImage, b: &RgbaImage, options: &Options) -> Result<Report, String> {
    let (w, h) = a.dimensions();
    if b.dimensions() != (w, h) {
        return Err(format!("sizes differ: {}×{} and {}×{}", w, h, b.width(), b.height()));
    }
    let delta: Vec<f32> = a.pixels().zip(b.pixels()).map(|(p, q)| delta_e(lab(*p), lab(*q))).collect();

    let cell = options.cell.max(1);
    let (cols, rows) = (w.div_ceil(cell), h.div_ceil(cell));
    let mut counts = vec![0u32; (cols * rows) as usize];
    let mut differing_pixels = 0u64;
    for y in 0..h {
        for x in 0..w {
            if delta[(y * w + x) as usize] > options.threshold {
                counts[((y / cell) * cols + x / cell) as usize] += 1;
                differing_pixels += 1;
            }
        }
    }
    let marked: Vec<bool> = (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (r, c)))
        .map(|(r, c)| {
            let cw = cell.min(w - c * cell);
            let ch = cell.min(h - r * cell);
            let n = counts[(r * cols + c) as usize];
            n > 0 && n as f32 >= (cw * ch) as f32 * options.min_ratio
        })
        .collect();

    // Group marked cells: those within `merge` cells of each other join.
    let reach = options.merge as i64;
    let mut group = vec![usize::MAX; marked.len()];
    let mut boxes: Vec<(u32, u32, u32, u32)> = Vec::new();
    for start in 0..marked.len() {
        if !marked[start] || group[start] != usize::MAX {
            continue;
        }
        let id = boxes.len();
        let (sc, sr) = ((start as u32) % cols, (start as u32) / cols);
        let mut bounds = (sc, sr, sc, sr);
        group[start] = id;
        let mut stack = vec![start];
        while let Some(i) = stack.pop() {
            let (c, r) = ((i as u32 % cols) as i64, (i as u32 / cols) as i64);
            bounds = (bounds.0.min(c as u32), bounds.1.min(r as u32), bounds.2.max(c as u32), bounds.3.max(r as u32));
            for dr in -reach - 1..=reach + 1 {
                for dc in -reach - 1..=reach + 1 {
                    let (nc, nr) = (c + dc, r + dr);
                    if nc < 0 || nr < 0 || nc >= cols as i64 || nr >= rows as i64 {
                        continue;
                    }
                    let j = (nr as u32 * cols + nc as u32) as usize;
                    if marked[j] && group[j] == usize::MAX {
                        group[j] = id;
                        stack.push(j);
                    }
                }
            }
        }
        boxes.push(bounds);
    }

    let mut regions: Vec<Region> = boxes
        .into_iter()
        .map(|(c0, r0, c1, r1)| {
            let (x, y) = (c0 * cell, r0 * cell);
            let (x1, y1) = (((c1 + 1) * cell).min(w), ((r1 + 1) * cell).min(h));
            let (mut n, mut sum, mut max) = (0u64, 0f64, 0f32);
            for py in y..y1 {
                for px in x..x1 {
                    let d = delta[(py * w + px) as usize];
                    if d > options.threshold {
                        n += 1;
                        sum += d as f64;
                        max = max.max(d);
                    }
                }
            }
            Region {
                x,
                y,
                width: x1 - x,
                height: y1 - y,
                differing_pixels: n,
                mean_delta_e: if n > 0 { (sum / n as f64) as f32 } else { 0.0 },
                max_delta_e: max,
            }
        })
        .collect();
    regions.sort_by(|a, b| b.differing_pixels.cmp(&a.differing_pixels).then(a.y.cmp(&b.y)).then(a.x.cmp(&b.x)));
    Ok(Report { width: w, height: h, regions, differing_pixels })
}

/// `image` scaled to `width` × `height` (bilinear), for comparing pictures
/// rendered at slightly different sizes.
pub fn resize_to(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    if image.dimensions() == (width, height) {
        return image.clone();
    }
    image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle)
}

/// `image` with the regions outlined in red, for looking at a report.
pub fn outline(image: &RgbaImage, regions: &[Region]) -> RgbaImage {
    let mut out = image.clone();
    let red = Rgba([255, 0, 0, 255]);
    let (w, h) = out.dimensions();
    for r in regions {
        for t in 0..2 {
            let (x0, y0) = (r.x.saturating_sub(t), r.y.saturating_sub(t));
            let (x1, y1) = ((r.x + r.width + t).min(w) - 1, (r.y + r.height + t).min(h) - 1);
            for x in x0..=x1 {
                out.put_pixel(x, y0, red);
                out.put_pixel(x, y1, red);
            }
            for y in y0..=y1 {
                out.put_pixel(x0, y, red);
                out.put_pixel(x1, y, red);
            }
        }
    }
    out
}

/// sRGB (composited over white) to CIE L\*a\*b\* (D65).
fn lab(p: Rgba<u8>) -> [f32; 3] {
    let a = p[3] as f32 / 255.0;
    let lin = |c: u8| {
        let c = (c as f32 / 255.0) * a + (1.0 - a);
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (lin(p[0]), lin(p[1]), lin(p[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f32| if t > 0.008_856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn delta_e(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]))
    }

    fn paint(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
        for py in y..y + h {
            for px in x..x + w {
                img.put_pixel(px, py, Rgba(color));
            }
        }
    }

    #[test]
    fn identical_pictures_have_no_regions() {
        let a = white(64, 48);
        let report = compare(&a, &a.clone(), &Options::default()).unwrap();
        assert!(report.regions.is_empty());
        assert_eq!(report.differing_pixels, 0);
    }

    #[test]
    fn a_changed_block_becomes_one_region_on_the_cell_grid() {
        let a = white(64, 48);
        let mut b = a.clone();
        paint(&mut b, 20, 10, 10, 6, [0, 0, 0, 255]);
        let report = compare(&a, &b, &Options::default()).unwrap();
        assert_eq!(report.regions.len(), 1);
        let r = &report.regions[0];
        assert_eq!((r.x, r.y, r.width, r.height), (16, 8, 16, 8));
        assert_eq!(r.differing_pixels, 60);
        assert!(r.max_delta_e > 99.0);
    }

    #[test]
    fn nearby_blocks_merge_and_distant_ones_do_not() {
        let a = white(128, 32);
        let mut b = a.clone();
        paint(&mut b, 0, 0, 4, 4, [255, 0, 0, 255]);
        paint(&mut b, 16, 0, 4, 4, [255, 0, 0, 255]); // one empty cell between: merges
        paint(&mut b, 100, 0, 4, 4, [255, 0, 0, 255]); // far away: separate
        let report = compare(&a, &b, &Options::default()).unwrap();
        assert_eq!(report.regions.len(), 2);
        assert_eq!((report.regions[0].x, report.regions[0].width), (0, 24));
        assert_eq!(report.regions[1].x, 96);
    }

    #[test]
    fn small_differences_are_ignored() {
        let a = white(16, 16);
        let mut b = a.clone();
        paint(&mut b, 0, 0, 16, 16, [250, 250, 250, 255]); // ΔE ≈ 2
        assert!(compare(&a, &b, &Options::default()).unwrap().regions.is_empty());
    }

    #[test]
    fn transparent_pixels_count_as_white() {
        let a = white(8, 8);
        let b = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0]));
        assert!(compare(&a, &b, &Options::default()).unwrap().regions.is_empty());
    }

    #[test]
    fn tsv_has_a_header_and_one_line_per_region() {
        let a = white(32, 32);
        let mut b = a.clone();
        paint(&mut b, 0, 0, 8, 8, [0, 0, 0, 255]);
        let tsv = compare(&a, &b, &Options::default()).unwrap().to_tsv();
        let lines: Vec<&str> = tsv.lines().collect();
        assert_eq!(lines[0], "x\ty\twidth\theight\tdiffering_pixels\tmean_delta_e\tmax_delta_e");
        assert_eq!(lines[1], "0\t0\t8\t8\t64\t100.0\t100.0");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn different_sizes_are_an_error() {
        assert!(compare(&white(8, 8), &white(8, 9), &Options::default()).is_err());
    }
}
pub mod grid;
pub mod json;
