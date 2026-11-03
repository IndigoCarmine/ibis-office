// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The scripts under scripts/ are LF files. `scripts/cargo-shim/cargo` starts
//! with `#!/usr/bin/env python3`, and with CRLF the interpreter is looked up
//! as `python3\r`, so the shim does not run on a Mac. Since e9ab888a3 the shim
//! and the python files in scripts/corpus/ have been committed with CRLF.
//! The .ps1 files are left out (PowerShell reads CRLF).

use std::path::{Path, PathBuf};

fn script_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            script_files(&path, out);
        } else if is_checked_script(&path) {
            out.push(path);
        }
    }
}

fn is_checked_script(path: &Path) -> bool {
    let is_shim = path.ends_with("cargo-shim/cargo");
    is_shim || path.extension().is_some_and(|e| e == "py" || e == "sh")
}

#[test]
fn scripts_have_lf_line_endings() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let mut files = Vec::new();
    script_files(&root.join("scripts"), &mut files);
    assert!(files.len() > 5, "found only {} files under {}/scripts", files.len(), root.display());
    let mut with_cr: Vec<String> = files
        .iter()
        .filter(|file| std::fs::read(file).unwrap_or_default().contains(&b'\r'))
        .map(|file| file.strip_prefix(root).unwrap().display().to_string())
        .collect();
    with_cr.sort();
    assert!(
        with_cr.is_empty(),
        "Mac で shim が `python3\\r` になって動かない。改行が CRLF のファイル:\n{}",
        with_cr.join("\n")
    );
}
