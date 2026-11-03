// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! End to end against the `ibis-embed-mock` editor in tools/, over real local
//! sockets. The sessions on their own are tested in `acauda-embed`.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use ibis_embed_protocol::{Error, HandlerMessage, HostMessage, HostSession, Mode, OleContent, Preview};

const WAIT: Duration = Duration::from_secs(20);

/// Builds tools/ibis-embed-mock (another package, so `CARGO_BIN_EXE_*` does
/// not reach it) into the same target directory and returns its path.
fn mock_exe() -> &'static PathBuf {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| {
        // This test runs from <target>/<profile>/deps/.
        let profile_dir = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_owned();
        let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        cmd.args(["build", "--quiet", "-p", "ibis-embed-mock"]).current_dir(env!("CARGO_MANIFEST_DIR"));
        if profile_dir.ends_with("release") {
            cmd.arg("--release");
        }
        assert!(cmd.status().expect("run cargo").success(), "building ibis-embed-mock failed");
        let exe = profile_dir.join(format!("ibis-embed-mock{}", std::env::consts::EXE_SUFFIX));
        assert!(exe.is_file(), "{} was not built", exe.display());
        exe
    })
}

fn spawn_mock(mode: Mode) -> HostSession {
    HostSession::spawn(mock_exe(), &[] as &[&str], mode, WAIT).unwrap()
}

fn text(content: &OleContent) -> &str {
    std::str::from_utf8(content.stream("CONTENTS").unwrap()).unwrap()
}

fn svg_text(preview: &Preview) -> String {
    String::from_utf8(preview.svg.clone().expect("an SVG preview")).unwrap()
}

/// Headless: one reply, then the editor exits and the connection closes.
fn headless(request: HostMessage) -> HandlerMessage {
    let mut host = spawn_mock(Mode::Headless);
    let reply = host.request(&request).unwrap();
    assert!(matches!(host.recv(), Err(Error::Disconnected)));
    let status = host.process().unwrap().wait_timeout(WAIT).expect("mock exited");
    assert!(status.success());
    reply
}

#[test]
fn mock_headless_new_import_export_render() {
    let HandlerMessage::Created { content, preview, size_emu } = headless(HostMessage::New) else {
        panic!("New did not create")
    };
    assert_eq!(text(&content), "mock");
    assert!(svg_text(&preview).contains(">mock<"));
    assert!(size_emu.is_some());

    let HandlerMessage::Created { content, .. } =
        headless(HostMessage::Import { extension: "txt".into(), data: b"a<b".to_vec() })
    else {
        panic!("Import did not create")
    };
    assert_eq!(text(&content), "a<b");

    let reply = headless(HostMessage::Export { content: content.clone(), extension: "txt".into() });
    assert_eq!(reply, HandlerMessage::Exported { data: b"a<b".to_vec() });

    let HandlerMessage::Rendered { preview, .. } = headless(HostMessage::Render { content }) else {
        panic!("Render did not render")
    };
    assert!(svg_text(&preview).contains(">a&lt;b<"));
}

#[test]
fn mock_interactive_open_update_close() {
    let mut host = spawn_mock(Mode::Interactive);
    let mut content = OleContent { prog_id: "ChemDraw.Document.6.0".into(), ..Default::default() };
    content.set_stream("CONTENTS", b"mock".to_vec());
    content.set_stream("Other/Data", vec![1, 2, 3]);
    host.send(&HostMessage::Open { content, title: "Slide 1".into(), locale: "en".into() }).unwrap();

    let HandlerMessage::Update { content, preview, .. } = host.recv().unwrap() else {
        panic!("expected Update")
    };
    assert_eq!(text(&content), "mock+edited");
    assert_eq!(content.prog_id, "ChemDraw.Document.6.0");
    assert_eq!(content.stream("Other/Data"), Some(&[1, 2, 3][..]));
    assert!(svg_text(&preview).contains("mock+edited"));

    host.send(&HostMessage::Ping).unwrap();
    host.send(&HostMessage::Close).unwrap();
    assert_eq!(host.recv().unwrap(), HandlerMessage::Closed);
    assert!(matches!(host.recv(), Err(Error::Disconnected)));
    assert!(host.process().unwrap().wait_timeout(WAIT).expect("mock exited").success());
}

#[test]
fn mock_killed_mid_session_is_a_disconnect() {
    let mut host = spawn_mock(Mode::Interactive);
    host.process().unwrap().kill();
    assert!(matches!(host.recv(), Err(Error::Disconnected)));
    assert!(matches!(host.send(&HostMessage::Ping), Err(Error::Disconnected) | Err(Error::Io(_))));
}
