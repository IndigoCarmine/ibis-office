// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Many tests laid out on the pages of one file, compared one by one.
//!
//! A coverage test file holds hundreds of small tests in a grid (a border
//! style per cell, a dash style per shape). Both the real Office app's PDF
//! and ibisOffice's drawing are rendered to page pictures at the same
//! resolution; a *manifest* says where each test is on each side, and each
//! test's rectangle is cut out of both and compared on its own.
//!
//! Coordinates: points (1/72 inch) from the top-left corner of the page
//! (a PDF page, a slide, a document page, or — for ibisOffice's drawing of a
//! worksheet — the top-left corner of cell A1). Pages are rendered at
//! `dpi` pixels per inch, so a point is `dpi / 72` pixels on both sides.
//! See `scripts/office-check/README.md` for how to render the two sides.
//!
//! The manifest (JSON):
//!
//! ```json
//! {
//!   "version": 1,
//!   "dpi": 144,
//!   "pad_pt": 1,
//!   "tests": [
//!     {"id": "border-thin", "ref": {"page": 1, "rect_pt": [72, 90, 60, 20]},
//!                           "app": {"page": 1, "rect_pt": [0, 18, 60, 20]}},
//!     {"id": "dash-sysDot", "ref": {"page": 2, "rect_pt": [100, 100, 80, 40]}}
//!   ]
//! }
//! ```
//!
//! `ref` is the real app's side, `app` ibisOffice's; `app` may be left out
//! when it is the same. `page` is 1-based. `pad_pt` widens every rectangle
//! on all sides. A test may carry its own `threshold`, `cell` and
//! `min_ratio` (see [`crate::Options`]).

use image::RgbaImage;

use crate::json::{self, Json};
use crate::{Options, compare, resize_to};

/// Where one side of a test is.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    /// 1-based page.
    pub page: usize,
    /// x, y, width, height in points.
    pub rect_pt: [f64; 4],
}

#[derive(Clone, Debug)]
pub struct Test {
    pub id: String,
    pub reference: Place,
    pub app: Place,
    pub options: Options,
}

#[derive(Clone, Debug)]
pub struct Manifest {
    pub dpi: f64,
    pub tests: Vec<Test>,
}

impl Manifest {
    /// Read a manifest; `defaults` are the comparison options of tests that
    /// set none.
    pub fn parse(text: &str, defaults: &Options) -> Result<Self, String> {
        let root = json::parse(text)?;
        let dpi = root.get("dpi").and_then(Json::as_f64).ok_or("dpi is missing")?;
        if dpi <= 0.0 {
            return Err("dpi must be a positive number".into());
        }
        let pad = root.get("pad_pt").and_then(Json::as_f64).unwrap_or(0.0);
        let place = |v: &Json, id: &str| -> Result<Place, String> {
            let page = v.get("page").and_then(Json::as_f64).ok_or(format!("{id}: page is missing"))?;
            let rect = v.get("rect_pt").and_then(Json::as_array).ok_or(format!("{id}: rect_pt is missing"))?;
            let r: Vec<f64> = rect.iter().filter_map(Json::as_f64).collect();
            let [x, y, w, h] = r[..] else { return Err(format!("{id}: rect_pt is [x, y, width, height]")) };
            if page < 1.0 || w <= 0.0 || h <= 0.0 {
                return Err(format!("{id}: page starts at 1, width and height are positive numbers"));
            }
            Ok(Place { page: page as usize, rect_pt: [x - pad, y - pad, w + 2.0 * pad, h + 2.0 * pad] })
        };
        let mut tests = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for t in root.get("tests").and_then(Json::as_array).ok_or("tests is missing")? {
            let id = t.get("id").and_then(Json::as_str).ok_or("a test has no id")?.to_string();
            if !seen.insert(id.clone()) {
                return Err(format!("{id}: duplicate id"));
            }
            let reference = place(t.get("ref").ok_or(format!("{id}: ref is missing"))?, &id)?;
            let app = match t.get("app") {
                Some(v) => place(v, &id)?,
                None => reference.clone(),
            };
            let mut options = defaults.clone();
            if let Some(v) = t.get("threshold").and_then(Json::as_f64) {
                options.threshold = v as f32;
            }
            if let Some(v) = t.get("cell").and_then(Json::as_f64) {
                options.cell = (v as u32).max(1);
            }
            if let Some(v) = t.get("min_ratio").and_then(Json::as_f64) {
                options.min_ratio = v as f32;
            }
            tests.push(Test { id, reference, app, options });
        }
        Ok(Self { dpi, tests })
    }
}

/// The pixel rectangle of `rect_pt` at `dpi`, or an error if it is not on the page.
pub fn pixel_rect(rect_pt: [f64; 4], dpi: f64, width: u32, height: u32) -> Result<[u32; 4], String> {
    let s = dpi / 72.0;
    let x0 = (rect_pt[0] * s).round();
    let y0 = (rect_pt[1] * s).round();
    let x1 = ((rect_pt[0] + rect_pt[2]) * s).round();
    let y1 = ((rect_pt[1] + rect_pt[3]) * s).round();
    if x0 < 0.0 || y0 < 0.0 || x1 > width as f64 || y1 > height as f64 || x1 <= x0 || y1 <= y0 {
        return Err(format!("the rectangle {rect_pt:?} pt is outside the page ({width}×{height} px)"));
    }
    Ok([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
}

pub fn crop(image: &RgbaImage, r: [u32; 4]) -> RgbaImage {
    image::imageops::crop_imm(image, r[0], r[1], r[2], r[3]).to_image()
}

/// The result of one test.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub id: String,
    pub pass: bool,
    pub regions: usize,
    pub differing_pixels: u64,
    pub ref_px: Option<[u32; 4]>,
    pub app_px: Option<[u32; 4]>,
    /// The two crops differed in size; ibisOffice's was scaled to the other's.
    pub resized: bool,
    pub error: Option<String>,
    /// The crops and the reference crop with differences outlined, for saving.
    pub pictures: Option<(RgbaImage, RgbaImage, RgbaImage)>,
}

impl Outcome {
    pub fn to_json(&self) -> String {
        let rect = |r: &Option<[u32; 4]>| r.map_or("null".to_string(), |r| format!("[{},{},{},{}]", r[0], r[1], r[2], r[3]));
        format!(
            "{{\"id\":{},\"pass\":{},\"regions\":{},\"differing_pixels\":{},\"ref_px\":{},\"app_px\":{},\"resized\":{},\"error\":{}}}",
            json::quote(&self.id),
            self.pass,
            self.regions,
            self.differing_pixels,
            rect(&self.ref_px),
            rect(&self.app_px),
            self.resized,
            self.error.as_deref().map_or("null".to_string(), json::quote)
        )
    }
}

/// Compare every test of `manifest`: `reference` and `app` are the page
/// pictures of each side, in page order.
pub fn run(manifest: &Manifest, reference: &[RgbaImage], app: &[RgbaImage], keep_pictures: bool) -> Vec<Outcome> {
    manifest
        .tests
        .iter()
        .map(|test| {
            let mut outcome = Outcome { id: test.id.clone(), pass: false, regions: 0, differing_pixels: 0, ref_px: None, app_px: None, resized: false, error: None, pictures: None };
            let side = |pages: &[RgbaImage], place: &Place, name: &str| -> Result<([u32; 4], RgbaImage), String> {
                let page = pages.get(place.page - 1).ok_or(format!("{name} has no page {} ({} pages)", place.page, pages.len()))?;
                let r = pixel_rect(place.rect_pt, manifest.dpi, page.width(), page.height()).map_err(|e| format!("{name}: {e}"))?;
                Ok((r, crop(page, r)))
            };
            let result = (|| -> Result<(), String> {
                let (rr, rc) = side(reference, &test.reference, "ref")?;
                let (ar, mut ac) = side(app, &test.app, "app")?;
                outcome.ref_px = Some(rr);
                outcome.app_px = Some(ar);
                if (ac.width(), ac.height()) != (rc.width(), rc.height()) {
                    ac = resize_to(&ac, rc.width(), rc.height());
                    outcome.resized = true;
                }
                let report = compare(&rc, &ac, &test.options)?;
                outcome.regions = report.regions.len();
                outcome.differing_pixels = report.differing_pixels;
                outcome.pass = report.regions.is_empty();
                if keep_pictures {
                    let marked = crate::outline(&rc, &report.regions);
                    outcome.pictures = Some((rc, ac, marked));
                }
                Ok(())
            })();
            if let Err(e) = result {
                outcome.error = Some(e);
            }
            outcome
        })
        .collect()
}

/// The PNG page pictures in `dir`, in page order: file names sorted by the
/// last number in them (`ref-1.png`, `ref-02.png`, `deck-10.png`), so
/// pdftoppm's and ibisOffice's names both work. Files named `*.layout*`
/// or starting with `diff` are skipped.
pub fn page_files(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut files: Vec<(u64, std::path::PathBuf)> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            !name.contains(".layout") && !name.starts_with("diff")
        })
        .map(|p| {
            let stem = p.file_stem().and_then(|n| n.to_str()).unwrap_or("").to_string();
            let digits: String = stem.chars().rev().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
            let n = digits.chars().rev().collect::<String>().parse().unwrap_or(0);
            (n, p)
        })
        .collect();
    files.sort();
    Ok(files.into_iter().map(|(_, p)| p).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn page(w: u32, h: u32, marks: &[(u32, u32)]) -> RgbaImage {
        let mut img = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
        for &(x, y) in marks {
            for dy in 0..10 {
                for dx in 0..10 {
                    img.put_pixel(x + dx, y + dy, Rgba([0, 0, 0, 255]));
                }
            }
        }
        img
    }

    const MANIFEST: &str = r#"{"version": 1, "dpi": 72, "tests": [
        {"id": "a", "ref": {"page": 1, "rect_pt": [0, 0, 20, 20]}},
        {"id": "b", "ref": {"page": 1, "rect_pt": [40, 0, 20, 20]}, "app": {"page": 1, "rect_pt": [40, 40, 20, 20]}},
        {"id": "c", "ref": {"page": 1, "rect_pt": [70, 70, 40, 40]}}
    ]}"#;

    #[test]
    fn each_test_is_compared_on_its_own() {
        let manifest = Manifest::parse(MANIFEST, &Options { cell: 1, min_ratio: 0.0, ..Options::default() }).unwrap();
        // Test a: the same. Test b: at another place on the app's side, the
        // same. Test c: off the page.
        let reference = vec![page(100, 100, &[(5, 5), (45, 5)])];
        let app = vec![page(100, 100, &[(5, 5), (45, 45)])];
        let out = run(&manifest, &reference, &app, false);
        assert!(out[0].pass, "{:?}", out[0]);
        assert!(out[1].pass, "{:?}", out[1]);
        assert!(!out[2].pass && out[2].error.is_some());
    }

    #[test]
    fn a_one_pixel_shift_fails() {
        let manifest = Manifest::parse(MANIFEST, &Options::default()).unwrap();
        let reference = vec![page(100, 100, &[(5, 5), (45, 5)])];
        let app = vec![page(100, 100, &[(6, 5), (45, 45)])];
        let out = run(&manifest, &reference, &app, false);
        assert!(!out[0].pass, "{:?}", out[0]);
        assert!(out[1].pass);
    }

    #[test]
    fn points_scale_with_dpi() {
        assert_eq!(pixel_rect([10.0, 20.0, 30.0, 40.0], 144.0, 1000, 1000).unwrap(), [20, 40, 60, 80]);
        assert!(pixel_rect([0.0, 0.0, 600.0, 10.0], 144.0, 1000, 1000).is_err());
    }
}
