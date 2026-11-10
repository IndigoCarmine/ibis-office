// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Where the app keeps what is the user's (the recent files, AutoRecover, the backup and chart-template settings, ...):
//! the user's application data directory (`~/Library/Application Support` on macOS, `%APPDATA%` on Windows,
//! `$XDG_DATA_HOME` or `~/.local/share` elsewhere), and `ibisOffice` under it.
//!
//! The scenarios (built with the `scenarios` feature) never touch the user's own: they keep it all under
//! `target/scenario-user-data` (todo-fix #83: the user's recent-files list had the scenarios' test files in it).
//! `IBIS_DATA_DIR` puts it anywhere else.

use std::path::PathBuf;

fn env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The directory the scenarios keep the user's data in (instead of the user's own), if this is a scenario or test build.
pub(crate) fn scenario_dir() -> Option<PathBuf> {
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    {
        return Some(crate::shell::scenario::repo_root().join("target/scenario-user-data"));
    }
    #[cfg(test)]
    {
        return Some(std::env::temp_dir().join("ibisOffice-test-user-data"));
    }
    #[allow(unreachable_code)]
    None
}

/// The directory under which the app's own folder (`ibisOffice`) lies: where the user's data goes.
pub(crate) fn base() -> Option<PathBuf> {
    if let Some(dir) = env("IBIS_DATA_DIR").or_else(scenario_dir) {
        return Some(dir);
    }
    #[cfg(target_os = "macos")]
    let base = env("HOME").map(|home| home.join("Library/Application Support"));
    #[cfg(target_os = "windows")]
    let base = env("APPDATA");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = env("XDG_DATA_HOME").or_else(|| env("HOME").map(|home| home.join(".local/share")));
    base
}
