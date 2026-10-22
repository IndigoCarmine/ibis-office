// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Test fixtures that may be missing: one place that decides whether a test
//! runs or is skipped.
//!
//! The files under `test/editing` (and other private test data) are not in
//! the public repository. A test that needs one asks here; when the file is
//! missing it prints `skipped: <path> が無い` and returns early instead of
//! failing. With `IBIS_REQUIRE_FIXTURES` set (to anything but empty or `0`) a
//! missing file is a failure, so a CI that has the data never skips silently.
//!
//! This file belongs to no crate: every crate whose tests read fixtures takes
//! it in with `#[path]`, because no crate is depended on by all of them and a
//! dev-dependency would change `Cargo.lock`. From `crates/<name>/tests/*.rs`:
//!
//! ```ignore
//! #[path = "../../../test/fixtures.rs"]
//! mod fixtures;
//! ```
//!
//! (`#[path]` is relative to the directory of the file that has the `mod`.)
//! Paths are given from the repository root, as `Scenario::open` takes them,
//! for example `"test/editing/xlsx-sales.xlsx"`.
//!
//! In a libtest test use [`find`], [`read`] or [`read_text`] and return early
//! with `let ... else`. A scenario runner, which counts outcomes from panics,
//! uses [`find_or_skip`] and tells skips apart with [`skipped`].

// Each crate takes in the whole file but uses only part of it.
#![allow(dead_code)]

use std::any::Any;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Set it to make a missing fixture a failure instead of a skip.
pub const REQUIRE_VAR: &str = "IBIS_REQUIRE_FIXTURES";

/// The repository root, from the manifest of the crate that takes this file in
/// (`crates/<name>` or `tools/<name>`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Whether a value of [`REQUIRE_VAR`] requires the fixtures: set, not empty and
/// not `0`.
pub fn required_from(value: Option<&OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != "0")
}

/// Whether this run requires the fixtures (reads [`REQUIRE_VAR`]).
pub fn required() -> bool {
    required_from(std::env::var_os(REQUIRE_VAR).as_deref())
}

fn missing(rel: &str) -> String {
    format!("{rel} が無い")
}

/// The fixture's path, or `None` when it is missing and not required. A
/// missing required fixture panics with its path.
fn locate(rel: &str) -> Option<PathBuf> {
    let path = repo_root().join(rel);
    if path.exists() {
        Some(path)
    } else if required() {
        panic!("{}（{REQUIRE_VAR} が立っているので失敗にする）: {}", missing(rel), path.display())
    } else {
        None
    }
}

/// The path of the fixture `rel` (from the repository root). When it is
/// missing: a panic if required, otherwise `skipped: <rel> が無い` on stderr
/// and `None`.
pub fn find(rel: &str) -> Option<PathBuf> {
    let found = locate(rel);
    if found.is_none() {
        eprintln!("skipped: {}", missing(rel));
    }
    found
}

/// The bytes of the fixture `rel`, missing as in [`find`]. A file that is
/// there but cannot be read panics.
pub fn read(rel: &str) -> Option<Vec<u8>> {
    find(rel).map(|path| std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
}

/// The text of the fixture `rel`, missing as in [`find`]. A file that is there
/// but cannot be read panics.
pub fn read_text(rel: &str) -> Option<String> {
    find(rel).map(|path| std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
}

/// The panic payload of a scenario skipped for a missing fixture; it holds
/// `<rel> が無い`.
#[derive(Debug)]
pub struct Skipped(pub String);

/// The path of the fixture `rel`. When it is missing: an ordinary panic if
/// required, otherwise a panic with [`Skipped`] as its payload.
pub fn find_or_skip(rel: &str) -> PathBuf {
    locate(rel).unwrap_or_else(|| std::panic::panic_any(Skipped(missing(rel))))
}

/// The reason, when a caught panic payload is a [`Skipped`].
pub fn skipped(payload: &(dyn Any + Send)) -> Option<&str> {
    payload.downcast_ref::<Skipped>().map(|s| s.0.as_str())
}
