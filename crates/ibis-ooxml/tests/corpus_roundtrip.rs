// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Every file of the test corpus (`test/benchmark`, `test/editing`) comes
//! back unchanged through the package: saved untouched it is the same bytes,
//! and with every XML part written from the tree it means the same.

use ibis_ooxml::canon::diff_packages;
use ibis_ooxml::{Package, SaveOptions};

fn corpus() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test");
    let mut files = Vec::new();
    for dir in ["benchmark", "editing"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            // (A password-protected file is a compound file, not a package.)
            if matches!(ext.as_str(), "pptx" | "xlsx" | "docx") && !std::fs::read(&path).is_ok_and(|d| ibis_ooxml::cfb::is_cfb(&d)) {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty(), "no corpus files");
    files
}

#[test]
fn untouched_save_is_identical() {
    for path in corpus() {
        let data = std::fs::read(&path).unwrap();
        let package = Package::open(data.clone()).unwrap();
        let saved = package.save(SaveOptions::default()).unwrap();
        assert!(saved == data, "{} changed", path.display());
    }
}

#[test]
fn regenerated_save_means_the_same() {
    for path in corpus() {
        let data = std::fs::read(&path).unwrap();
        let package = Package::open(data.clone()).unwrap();
        let saved = package.save(SaveOptions { regenerate_xml: true }).unwrap();
        let report = diff_packages(&data, &saved, &[]).unwrap();
        assert!(report.is_same(), "{}: {}", path.display(), report.to_json());
    }
}
