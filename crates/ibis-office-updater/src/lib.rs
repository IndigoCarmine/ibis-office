// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Automatic update (todo-fix #35). `ibisOfficeUpdater` (src/bin) is the
//! program that, after ibisOffice has downloaded and unpacked a newer
//! version, waits for ibisOffice to quit, swaps the installed copy for the
//! new one (put back if anything fails), starts ibisOffice and removes what
//! is left. This library is what it and the app share: the log it writes
//! (`updater.log`, one line per step), where files go, and version.txt.
//!
//! Only std: it must work with nothing else installed, and never runs on
//! the web.

pub mod log;
pub mod paths;
pub mod process;
pub mod swap;
pub mod version;
