// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibis-grid-diff: compare the tests of a coverage file one by one — each
//! test's rectangle cut out of the real app's page pictures and out of
//! ibisOffice's, then compared (see `ibis_imgdiff::grid` for the manifest
//! and the coordinates).
//!
//!   ibis-grid-diff --manifest <m.json> --ref <dir> --app <dir>
//!                  [--out <dir>] [--threshold 10] [--cell 8] [--min-ratio 0.05]
//!
//! `--ref` and `--app` are directories of page PNGs (pdftoppm's output,
//! ibisOffice's `--export png`). Prints JSON: {"tests", "passed", "failed",
//! "results": [...]}. `--out` saves, for each failing test,
//! `<id>.ref.png`, `<id>.app.png` and `<id>.diff.png`.
//! Exit status: 0 when every test passes, 1 when some fail, 2 on errors.

use std::process::ExitCode;

use ibis_imgdiff::Options;
use ibis_imgdiff::grid::{Manifest, page_files, run};

fn main() -> ExitCode {
    match main_inner() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("ibis-grid-diff: {e}");
            ExitCode::from(2)
        }
    }
}

fn main_inner() -> Result<bool, String> {
    let (mut manifest, mut reference, mut app, mut out) = (None, None, None, None);
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--manifest" => manifest = Some(value()?),
            "--ref" => reference = Some(value()?),
            "--app" => app = Some(value()?),
            "--out" => out = Some(value()?),
            "--threshold" => options.threshold = value()?.parse().map_err(|_| "--threshold takes a number")?,
            "--cell" => options.cell = value()?.parse().map_err(|_| "--cell takes an integer")?,
            "--min-ratio" => options.min_ratio = value()?.parse().map_err(|_| "--min-ratio takes a number")?,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let (Some(manifest), Some(reference), Some(app)) = (manifest, reference, app) else {
        return Err("usage: ibis-grid-diff --manifest <m.json> --ref <dir> --app <dir> [--out <dir>]".into());
    };
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{manifest}: {e}"))?;
    let manifest = Manifest::parse(&text, &options)?;
    let load = |dir: &str| -> Result<Vec<image::RgbaImage>, String> {
        page_files(std::path::Path::new(dir))?
            .iter()
            .map(|p| image::open(p).map(|i| i.to_rgba8()).map_err(|e| format!("{}: {e}", p.display())))
            .collect()
    };
    let (reference, app) = (load(&reference)?, load(&app)?);
    let outcomes = run(&manifest, &reference, &app, out.is_some());
    if let Some(out) = &out {
        std::fs::create_dir_all(out).map_err(|e| format!("{out}: {e}"))?;
        for o in outcomes.iter().filter(|o| !o.pass) {
            if let Some((r, a, d)) = &o.pictures {
                let safe: String = o.id.chars().map(|c| if c.is_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
                for (suffix, img) in [("ref", r), ("app", a), ("diff", d)] {
                    img.save(format!("{out}/{safe}.{suffix}.png")).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    let passed = outcomes.iter().filter(|o| o.pass).count();
    let failed: Vec<String> = outcomes.iter().filter(|o| !o.pass).map(|o| ibis_imgdiff::json::quote(&o.id)).collect();
    let results: Vec<String> = outcomes.iter().map(|o| o.to_json()).collect();
    println!("{{\"tests\":{},\"passed\":{},\"failed\":[{}],\"results\":[{}]}}", outcomes.len(), passed, failed.join(","), results.join(","));
    Ok(passed == outcomes.len())
}
