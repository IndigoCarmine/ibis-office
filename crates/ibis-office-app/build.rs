// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Embeds the bundled icons (icons/material/*.svg) into the binary, so the
//! app serves them from memory on every platform (see `shell/icons.rs`).
//!
//! On Windows it also gives the executable the app icon and its version (see `windows_icon`).
//!
//! It also sets `cfg(headless_export)` where the export (`--export`) is
//! built: with the `export` feature, on macOS, Windows and Linux, where GPUI
//! draws without a window.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    app_version();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/material");
    println!("cargo:rerun-if-changed={}", dir.display());
    // The icons drawn for ibisOffice itself (icons/ibis/*.svg: line samples for the border styles, the merge menu) are
    // named "ibis/<file>".
    let own = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/ibis");
    println!("cargo:rerun-if-changed={}", own.display());
    let list = |dir: &std::path::Path, prefix: &str| -> Vec<(String, std::path::PathBuf)> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .flatten()
            .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".svg")).map(str::to_string))
            .collect();
        names.sort();
        names.into_iter().map(|n| (format!("{prefix}{n}"), dir.join(format!("{n}.svg")))).collect()
    };
    let mut out = String::from("/// The bundled icons, by name (file name without `.svg`; the ones drawn for ibisOffice: `ibis/<file name>`).\npub(crate) static MATERIAL_ICONS: &[(&str, &[u8])] = &[\n");
    for (name, path) in list(&dir, "").into_iter().chain(list(&own, "ibis/")) {
        writeln!(out, "    ({name:?}, include_bytes!({:?})),", path.display().to_string()).unwrap();
    }
    out.push_str("];\n");
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("material_icons.rs");
    std::fs::write(dest, out).unwrap();

    let target = |name: &str| std::env::var(name).unwrap_or_default();
    let os = target("CARGO_CFG_TARGET_OS");
    println!("cargo::rustc-check-cfg=cfg(headless_export)");
    if std::env::var_os("CARGO_FEATURE_EXPORT").is_some() && (os == "macos" || os == "windows" || os == "linux") {
        println!("cargo::rustc-cfg=headless_export");
    }

    // Built for Windows with MSVC: the icon resource goes to its linker.
    #[cfg(windows)]
    if os == "windows" && target("CARGO_CFG_TARGET_ENV") == "msvc" {
        windows_icon();
    }
}

/// The version the app shows (`IBIS_VERSION`), the one version.txt (at the
/// top of the repository: a platform, a tab and its version, per line) gives
/// the platform built for: Windows, macOS, Linux or wasm.
fn app_version() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../version.txt");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let target = |name: &str| std::env::var(name).unwrap_or_default();
    let platform = match (target("CARGO_CFG_TARGET_OS").as_str(), target("CARGO_CFG_TARGET_FAMILY").as_str()) {
        ("windows", _) => "Windows",
        ("macos", _) => "macOS",
        ("linux", _) => "Linux",
        (_, "wasm") => "wasm",
        (os, _) => panic!("version.txt has no line for {os}"),
    };
    let version = text
        .lines()
        .filter_map(|line| line.trim_end_matches('\r').split_once('\t'))
        .find(|(name, _)| name.trim() == platform)
        .map(|(_, version)| version.trim().to_string())
        .unwrap_or_else(|| panic!("version.txt has no {platform} line"));
    println!("cargo:rustc-env=IBIS_VERSION={version}");
}

#[cfg(windows)]
include!("windows/resource.rs");

/// The Windows app icon (windows/), the design without the plate.
#[cfg(windows)]
const WINDOWS_ICON: &str = "AcaudaOffice_transparent.ico";

/// Give the executable the app icon, as icon resource 1 (the one GPUI shows
/// on the window and the taskbar), from the .ico in windows/: its images as
/// RT_ICON resources and its directory as the RT_GROUP_ICON naming them by
/// id, in a resource file (.res) written here and handed to the linker, so
/// no resource compiler is needed.
#[cfg(windows)]
fn windows_icon() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("windows").join(WINDOWS_ICON);
    println!("cargo:rerun-if-changed={}", path.display());
    let ico = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let u16_at = |i: usize| u16::from_le_bytes([ico[i], ico[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([ico[i], ico[i + 1], ico[i + 2], ico[i + 3]]);
    assert!(u16_at(2) == 1, "{}: not an icon file", path.display());
    let count = u16_at(4) as usize;

    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;
    // An empty resource first marks the file as 32-bit.
    let mut res = Vec::new();
    push_resource(&mut res, 0, 0, &[]);
    // The group: the .ico's directory, each entry's offset replaced by an id.
    let mut group = ico[..6].to_vec();
    for i in 0..count {
        let entry = 6 + 16 * i;
        let (len, offset) = (u32_at(entry + 8) as usize, u32_at(entry + 12) as usize);
        let id = i as u16 + 1;
        push_resource(&mut res, RT_ICON, id, &ico[offset..offset + len]);
        group.extend_from_slice(&ico[entry..entry + 12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    push_resource(&mut res, RT_GROUP_ICON, 1, &group);
    // And the version, for the 詳細 tab of the file's properties.
    push_version_resource(&mut res, &windows_version(), "Acauda Office", "AcaudaOffice.exe");

    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("app-icon.res");
    std::fs::write(&out, res).unwrap();
    println!("cargo:rustc-link-arg-bin=AcaudaOffice={}", out.display());
}
