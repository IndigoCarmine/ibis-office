// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibis-imgdiff: where two PNGs differ, as rectangles in a TSV.
//!
//!   ibis-imgdiff <a.png> <b.png> [--out regions.tsv] [--outline marked.png]
//!                [--threshold 10] [--cell 8] [--min-ratio 0.05] [--merge 1] [--resize]
//!
//! Exit status: 0 when no region differs, 1 when some do, 2 on errors, so it
//! can gate a test. A summary goes to stderr. `--outline` writes the first
//! picture with the regions outlined; `--resize` scales the second to the
//! first's size.

use std::process::ExitCode;

use ibis_imgdiff::{Options, compare, outline, resize_to};

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(err) => {
            eprintln!("ibis-imgdiff: {err}");
            ExitCode::from(2)
        }
    }
}

/// `Ok(true)` when the pictures match.
fn run() -> Result<bool, String> {
    let mut inputs = Vec::new();
    let mut options = Options::default();
    let (mut out, mut outline_path, mut resize) = (None, None, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        let number = |text: String, name: &str| text.parse::<f32>().map_err(|_| format!("{name} takes a number"));
        match arg.as_str() {
            "--out" => out = Some(value("--out")?),
            "--outline" => outline_path = Some(value("--outline")?),
            "--threshold" => options.threshold = number(value("--threshold")?, "--threshold")?,
            "--cell" => options.cell = number(value("--cell")?, "--cell")? as u32,
            "--min-ratio" => options.min_ratio = number(value("--min-ratio")?, "--min-ratio")?,
            "--merge" => options.merge = number(value("--merge")?, "--merge")? as u32,
            "--resize" => resize = true,
            other if other.starts_with("--") => return Err(format!("unknown option: {other}")),
            _ => inputs.push(arg),
        }
    }
    let [a_path, b_path] = inputs.as_slice() else {
        return Err("usage: ibis-imgdiff <a.png> <b.png> [--out regions.tsv] [--outline marked.png] [--threshold ΔE] [--cell px] [--min-ratio r] [--merge cells] [--resize]".into());
    };
    let load = |path: &str| image::open(path).map(|i| i.to_rgba8()).map_err(|e| format!("{path}: {e}"));
    let a = load(a_path)?;
    let mut b = load(b_path)?;
    if resize {
        // Compare in the first picture's pixels.
        b = resize_to(&b, a.width(), a.height());
    }
    let report = compare(&a, &b, &options)?;

    match &out {
        Some(path) => std::fs::write(path, report.to_tsv()).map_err(|e| format!("{path}: {e}"))?,
        None => print!("{}", report.to_tsv()),
    }
    if let Some(path) = &outline_path {
        outline(&a, &report.regions).save(path).map_err(|e| format!("{path}: {e}"))?;
    }
    eprintln!(
        "{} regions; pixels with ΔE > {}: {:.2}%",
        report.regions.len(),
        options.threshold,
        report.differing_ratio() * 100.0
    );
    Ok(report.regions.is_empty())
}
