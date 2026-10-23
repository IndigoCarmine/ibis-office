// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The scenario tests (`src/shell/scenario.rs`): every registered scenario,
//! run in turn on the main thread.
//!
//!   cargo test --release -p ibis-office-app --test scenarios [-- <part of a name> | !<part of a name to leave out>]
//!
//! The scenario driver (`run_scenarios`) is built only on macOS; elsewhere
//! this target still builds and says it skipped them.

#[cfg(target_os = "macos")]
fn main() {
    let filter: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect();
    std::process::exit(ibis_office_app::run_scenarios(&filter));
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("skipped: the scenario tests run only on macOS (src/shell/scenario.rs)");
}
