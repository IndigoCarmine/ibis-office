// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! This crate's only reading of the clock: deadlines while waiting for an
//! editor. `Instant::now` panics on wasm32-unknown-unknown, but the sessions
//! that use it are not built for the browser.
#![allow(clippy::disallowed_methods)]

pub(crate) use std::time::Instant;

pub(crate) fn now() -> Instant {
    Instant::now()
}
