// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The protocol between ibisOffice and external editors of embedded (OLE)
//! objects. The protocol itself is the `acauda-embed` crate, re-exported
//! here; what is ibisOffice's own is [`host`] (feature `host`).

pub use acauda_embed::*;

#[cfg(feature = "host")]
pub mod host;
