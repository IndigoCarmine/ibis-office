// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A reference external editor for ibisOffice embedded objects. It accepts
//! any ProgID and keeps its "document" as text in the CONTENTS stream:
//!
//! - headless `New` makes "mock"; `Import` takes the file's bytes as the
//!   text; `Export` gives the text back; `Render` draws it;
//! - interactive `Open` appends "+edited", sends one `Update`, then waits
//!   for `Close`.
//!
//! Real editors follow the same shape; see `ibis_embed_protocol::HandlerSession`.

use std::process::ExitCode;

use ibis_embed_protocol::{
    Clsid, Error, HandlerMessage, HandlerSession, HostMessage, Mode, OleContent, Preview,
};

const PROG_ID: &str = "IbisOffice.Mock.1";
const STREAM: &str = "CONTENTS";
/// 2 × 1 inches.
const SIZE_EMU: (i64, i64) = (2 * 914_400, 914_400);

fn main() -> ExitCode {
    let mut session = match HandlerSession::from_env() {
        Ok(Some(session)) => session,
        Ok(None) => {
            eprintln!("ibis-embed-mock: run by ibisOffice with --ibis-embed <socket> --token <hex> --mode interactive|headless");
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("ibis-embed-mock: cannot connect: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = match session.mode() {
        Mode::Headless => headless(&mut session),
        Mode::Interactive => interactive(&mut session),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ibis-embed-mock: {e}");
            ExitCode::FAILURE
        }
    }
}

fn headless(session: &mut HandlerSession) -> Result<(), Error> {
    let reply = match session.recv()? {
        HostMessage::New => created(new_content(b"mock".to_vec())),
        HostMessage::Import { data, .. } => created(new_content(data)),
        HostMessage::Export { content, .. } => {
            HandlerMessage::Exported { data: content.stream(STREAM).unwrap_or_default().to_vec() }
        }
        HostMessage::Render { content } => {
            HandlerMessage::Rendered { preview: preview(&content), size_emu: Some(SIZE_EMU) }
        }
        other => HandlerMessage::Error { message: format!("not a headless request: {other:?}") },
    };
    session.send(&reply)
}

fn interactive(session: &mut HandlerSession) -> Result<(), Error> {
    loop {
        match session.recv() {
            Ok(HostMessage::Open { mut content, .. }) => {
                let mut text = content.stream(STREAM).unwrap_or_default().to_vec();
                text.extend_from_slice(b"+edited");
                content.set_stream(STREAM, text);
                let preview = preview(&content);
                session.send(&HandlerMessage::Update { content, preview, size_emu: Some(SIZE_EMU) })?;
            }
            Ok(HostMessage::Ping) => {}
            Ok(HostMessage::Close) => return session.send(&HandlerMessage::Closed),
            Ok(other) => session.send(&HandlerMessage::Error {
                message: format!("not an interactive request: {other:?}"),
            })?,
            // The host went away: nothing to save into, just quit.
            Err(Error::Disconnected) => return Ok(()),
            Err(e) => return Err(e),
        }
    }
}

fn new_content(text: Vec<u8>) -> OleContent {
    let mut content = OleContent {
        clsid: Clsid::default(),
        prog_id: PROG_ID.into(),
        user_type: "Mock Object".into(),
        streams: Vec::new(),
    };
    content.set_stream(STREAM, text);
    content
}

fn created(content: OleContent) -> HandlerMessage {
    let preview = preview(&content);
    HandlerMessage::Created { content, preview, size_emu: Some(SIZE_EMU) }
}

fn preview(content: &OleContent) -> Preview {
    let text = String::from_utf8_lossy(content.stream(STREAM).unwrap_or_default());
    let escaped = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    Preview::svg(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="192" height="96" viewBox="0 0 192 96"><rect x="1" y="1" width="190" height="94" fill="#fff" stroke="#333"/><text x="96" y="54" font-family="sans-serif" font-size="16" text-anchor="middle">{escaped}</text></svg>"##
    ))
}
