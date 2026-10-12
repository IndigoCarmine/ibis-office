// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Extract the picture of each EMF file given, next to it.
//!
//!   cargo run -p ibis-emf --example extract -- <file.emf>...

fn main() {
    let mut failed = false;
    for path in std::env::args().skip(1) {
        let data = std::fs::read(&path).expect("readable file");
        match ibis_emf::extract_image(&data) {
            Some((format, image)) => {
                let ext = format!("{format:?}").to_lowercase();
                let out = format!("{path}.{ext}");
                std::fs::write(&out, image).expect("writable output");
                println!("{out}");
            }
            None => {
                eprintln!("{path}: no picture");
                failed = true;
            }
        }
    }
    std::process::exit(failed as i32);
}
