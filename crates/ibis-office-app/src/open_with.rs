// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Puts ibisOffice in the Explorer's "Open with" list of the text tables (`.csv`, `.tsv`).
//!
//! The app is delivered as a folder (no installer), so it registers itself, for the user
//! only (`HKCU\Software\Classes`, no administrator needed), each time it starts from a
//! folder it is not yet registered for. It does not take the files over: only the
//! "Open with" list gets the entry, the default program stays as the user set it.

use std::os::windows::process::CommandExt;
use std::process::Command;

const KEY: &str = r"HKCU\Software\Classes\Applications\AcaudaOffice.exe";
const EXTENSIONS: &[&str] = &[".csv", ".tsv"];
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn reg(args: &[&str]) -> Option<String> {
    let out = Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Registers this executable (in the background; a failure is ignored: the entry is a convenience).
pub fn register() {
    let Ok(exe) = std::env::current_exe() else { return };
    let command = format!("\"{}\" \"%1\"", exe.display());
    std::thread::spawn(move || {
        let command_key = format!(r"{KEY}\shell\open\command");
        // Already registered for this folder.
        if reg(&["query", &command_key, "/ve"]).is_some_and(|out| out.contains(&command)) {
            return;
        }
        reg(&["add", &command_key, "/ve", "/d", &command, "/f"]);
        reg(&["add", KEY, "/v", "FriendlyAppName", "/d", "Acauda Office", "/f"]);
        for extension in EXTENSIONS {
            reg(&["add", &format!(r"{KEY}\SupportedTypes"), "/v", extension, "/d", "", "/f"]);
            reg(&["add", &format!(r"HKCU\Software\Classes\{extension}\OpenWithList\AcaudaOffice.exe"), "/f"]);
        }
    });
}
