// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The mock without its window, over a real local socket.

use std::time::Duration;

use ibis_embed_protocol::{HandlerMessage, HostMessage, HostSession, Mode};

fn ask(request: HostMessage) -> HandlerMessage {
    let mut session = HostSession::spawn(env!("CARGO_BIN_EXE_ibis-embed-mock-app"), &[] as &[&str], Mode::Headless, Duration::from_secs(20)).unwrap();
    session.request(&request).unwrap()
}

fn svg(preview: ibis_embed_protocol::Preview) -> String {
    String::from_utf8(preview.svg.unwrap()).unwrap()
}

#[test]
fn new_import_export_render() {
    let HandlerMessage::Created { content, preview, size_emu } = ask(HostMessage::New) else { panic!("not created") };
    assert_eq!(content.prog_id, "IbisOffice.Mock.1");
    assert_eq!(content.stream("CONTENTS"), Some(&b"mock"[..]));
    assert!(svg(preview).contains(">mock<"));
    assert!(size_emu.is_some());

    let HandlerMessage::Created { content, .. } = ask(HostMessage::Import { extension: "txt".into(), data: "a<b\n二行目".into() }) else { panic!("not imported") };
    let HandlerMessage::Exported { data } = ask(HostMessage::Export { content: content.clone(), extension: "txt".into() }) else { panic!("not exported") };
    assert_eq!(data, "a<b\n二行目".as_bytes());
    let HandlerMessage::Rendered { preview, .. } = ask(HostMessage::Render { content }) else { panic!("not rendered") };
    let svg = svg(preview);
    assert!(svg.contains(">a&lt;b<") && svg.contains(">二行目<"));
}
