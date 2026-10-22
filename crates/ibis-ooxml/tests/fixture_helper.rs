// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The shared fixture helper (`test/fixtures.rs`): a missing fixture is a
//! skip, and a failure once `IBIS_REQUIRE_FIXTURES` is set.
//!
//! Whether the variable is set is tried in a child process (this test binary
//! running one ignored probe), since setting it here would be `unsafe` and
//! would reach the tests running beside.

#[path = "../../../test/fixtures.rs"]
mod fixtures;

use std::ffi::OsStr;
use std::process::{Command, Output};

/// A fixture that is never there, with or without the private data.
const MISSING: &str = "test/editing/fixture-helper-probe-missing.xlsx";
/// A fixture that is in the public repository.
const PRESENT: &str = "test/benchmark/README.md";

#[test]
fn required_only_when_set_to_something_but_zero() {
    assert!(!fixtures::required_from(None));
    assert!(!fixtures::required_from(Some(OsStr::new(""))));
    assert!(!fixtures::required_from(Some(OsStr::new("0"))));
    assert!(fixtures::required_from(Some(OsStr::new("1"))));
    assert!(fixtures::required_from(Some(OsStr::new("true"))));
}

#[test]
fn a_fixture_that_is_there_is_found() {
    let path = fixtures::find(PRESENT).expect("found");
    assert!(path.ends_with(PRESENT), "{}", path.display());
    assert!(fixtures::read(PRESENT).is_some_and(|data| !data.is_empty()));
    assert!(fixtures::read_text(PRESENT).is_some_and(|text| !text.is_empty()));
    assert_eq!(fixtures::find_or_skip(PRESENT), path);
}

#[test]
fn a_skipped_payload_is_told_apart() {
    let caught = std::panic::catch_unwind(|| std::panic::panic_any(fixtures::Skipped("test/editing/a.xlsx が無い".into())));
    assert_eq!(fixtures::skipped(&*caught.unwrap_err()), Some("test/editing/a.xlsx が無い"));
    let caught = std::panic::catch_unwind(|| panic!("an ordinary failure"));
    assert_eq!(fixtures::skipped(&*caught.unwrap_err()), None);
}

#[test]
#[ignore = "run by find_skips_unless_required in a child process"]
fn probe_find() {
    assert_eq!(fixtures::find(MISSING), None);
    assert_eq!(fixtures::read(MISSING), None);
}

#[test]
#[ignore = "run by find_or_skip_skips_unless_required in a child process"]
fn probe_find_or_skip() {
    let caught = std::panic::catch_unwind(|| fixtures::find_or_skip(MISSING));
    assert_eq!(fixtures::skipped(&*caught.unwrap_err()), Some(format!("{MISSING} が無い").as_str()));
}

/// Runs one ignored probe of this binary, with the variable removed or set.
fn run_probe(name: &str, require: Option<&str>) -> (bool, String) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args([name, "--ignored", "--exact", "--nocapture", "--test-threads=1"]);
    match require {
        Some(value) => command.env(fixtures::REQUIRE_VAR, value),
        None => command.env_remove(fixtures::REQUIRE_VAR),
    };
    let Output { status, stdout, stderr } = command.output().unwrap();
    let output = format!("{}{}", String::from_utf8_lossy(&stdout), String::from_utf8_lossy(&stderr));
    // The probe ran, whatever its outcome (an `--exact` filter matching
    // nothing would pass with no test run).
    assert!(output.contains("running 1 test"), "{name} did not run:\n{output}");
    (status.success(), output)
}

#[test]
fn find_skips_unless_required() {
    let (passed, output) = run_probe("probe_find", None);
    assert!(passed, "{output}");
    assert!(output.contains(&format!("skipped: {MISSING} が無い")), "{output}");

    let (passed, output) = run_probe("probe_find", Some("1"));
    assert!(!passed, "{output}");
    assert!(output.contains(MISSING), "{output}");
}

#[test]
fn find_or_skip_skips_unless_required() {
    let (passed, output) = run_probe("probe_find_or_skip", Some("0"));
    assert!(passed, "{output}");

    let (passed, output) = run_probe("probe_find_or_skip", Some("1"));
    assert!(!passed, "{output}");
    assert!(output.contains(MISSING), "{output}");
}
