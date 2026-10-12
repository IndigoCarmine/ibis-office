// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The time is read through the `clock` modules only:
//! `std::time::Instant::now` and `SystemTime::now` panic on
//! wasm32-unknown-unknown (the web version showed a blank page for a .docx
//! until 2026-10-06). clippy.toml refuses them too, but clippy is not run
//! on every change, so this looks for them in the source with a plain
//! search. Files named clock.rs, and tests/ (not built for the browser),
//! are left out.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name != "tests" && name != "target" {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") && name != "clock.rs" {
            out.push(path);
        }
    }
}

/// Whether `line` (not a comment) reads the std clock.
fn reads_std_clock(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") {
        return false;
    }
    code.contains("SystemTime::now")
        || code.contains("std::time::Instant")
        || (code.contains("Instant::now") && !code.contains("clock::Instant::now"))
}

#[test]
fn no_std_clock_outside_clock_modules() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    rust_files(crates, &mut files);
    assert!(files.len() > 50, "found only {} files under {}", files.len(), crates.display());
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            if reads_std_clock(line) {
                found.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(found.is_empty(), "std の時計を直接読んでいる（wasm で panic する。clock を使う）:\n{}", found.join("\n"));
}

#[test]
fn the_search_finds_them() {
    assert!(reads_std_clock("    let t = std::time::Instant::now();"));
    assert!(reads_std_clock("let t = Instant::now();"));
    assert!(reads_std_clock("SystemTime::now().duration_since(UNIX_EPOCH)"));
    assert!(!reads_std_clock("let t = crate::clock::Instant::now();"));
    assert!(!reads_std_clock("    // std::time::Instant::now panics on wasm"));
}
