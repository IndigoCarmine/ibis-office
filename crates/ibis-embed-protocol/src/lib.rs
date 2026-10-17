// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The protocol between ibisOffice and external editors of embedded (OLE)
//! objects.
//!
//! ibisOffice knows nothing about what an embedded object contains. For each
//! kind it has a [`HandlerManifest`] naming an editor program; it launches
//! that program with `--ibis-embed <socket> --token <hex> --mode
//! interactive|headless` and the two talk over a local socket:
//!
//! - framing: 4-byte little-endian length + JSON ([`frame`]); binary fields
//!   are base64 strings;
//! - the editor connects and first sends [`HandlerMessage::Hello`] with the
//!   token; the host drops connections whose token does not match;
//! - interactive mode (editing): the host sends [`HostMessage::Open`], the
//!   editor sends [`HandlerMessage::Update`] whenever the user saves, until
//!   [`HostMessage::Close`] / [`HandlerMessage::Closed`];
//! - headless mode: one request (`New`, `Import`, `Export`, `Render`), one
//!   reply, then the editor exits.
//!
//! [`HostSession`] is the host side and [`HandlerSession`] the editor side;
//! both are blocking and meant to run on a background thread. An editor that
//! shows every object in one window uses [`claim_instance`].
//!
//! **To make an editor**, use [`editor`]: [`editor::start`] at launch, then
//! take [`editor::EditorEvent`]s in the UI's loop and send
//! [`editor::Editor::update`] at each change. It does the rest (headless
//! requests, one window for every object, a connection per object).

#[cfg(not(target_arch = "wasm32"))]
mod clock;
#[cfg(not(target_arch = "wasm32"))]
pub mod editor;
mod error;
pub mod frame;
#[cfg(feature = "host")]
pub mod host;
#[cfg(not(target_arch = "wasm32"))]
mod instance;
mod manifest;
mod message;
#[cfg(not(target_arch = "wasm32"))]
mod session;

pub use error::{Error, Result};
pub use frame::{MAX_FRAME_LEN, MemoryTransport, StreamTransport, Transport};
pub use manifest::{HandlerManifest, LocalizedName, find_handler, load_manifest_dir};
pub use message::{
    Clsid, HandlerMessage, HostMessage, OleContent, OleStream, PROTOCOL_VERSION, Preview,
};
#[cfg(not(target_arch = "wasm32"))]
pub use instance::{Instance, InstanceRequests, claim_instance};
#[cfg(not(target_arch = "wasm32"))]
pub use session::{
    EditorProcess, HandlerArgs, HandlerReceiver, HandlerSender, HandlerSession, HostListener,
    HostReceiver, HostSender, HostSession, Mode, MsgReceiver, MsgSender,
};
