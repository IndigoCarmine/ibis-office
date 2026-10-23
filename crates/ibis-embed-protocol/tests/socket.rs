// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Sessions over real local sockets, and end to end against the
//! `ibis-embed-mock` editor in tools/.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use ibis_embed_protocol::{
    Error, HandlerArgs, HandlerMessage, HandlerSession, HostListener, HostMessage, HostSession,
    Mode, OleContent, Preview,
};

const WAIT: Duration = Duration::from_secs(20);

fn args(listener: &HostListener, token: &str) -> HandlerArgs {
    HandlerArgs { socket: listener.name().to_owned(), token: token.to_owned(), mode: Mode::Interactive }
}

#[test]
fn wrong_token_is_dropped_and_waiting_goes_on() {
    let listener = HostListener::bind().unwrap();
    let bad = args(&listener, "00000000000000000000000000000000");
    let good = args(&listener, listener.token());
    let handler = thread::spawn(move || {
        let mut intruder = HandlerSession::connect(&bad).unwrap();
        // The host hangs up on it without a word.
        assert!(matches!(intruder.recv(), Err(Error::Disconnected)));
        let mut session = HandlerSession::connect(&good).unwrap();
        session.send(&HandlerMessage::Closed).unwrap();
    });
    let mut host = listener.accept(WAIT, None).unwrap();
    assert_eq!(host.recv().unwrap(), HandlerMessage::Closed);
    handler.join().unwrap();
}

#[test]
fn only_wrong_tokens_time_out() {
    let listener = HostListener::bind().unwrap();
    let bad = args(&listener, "not-the-token");
    let handler = thread::spawn(move || {
        let mut s = HandlerSession::connect(&bad).unwrap();
        let _ = s.recv();
    });
    assert!(matches!(listener.accept(Duration::from_millis(500), None), Err(Error::Timeout)));
    handler.join().unwrap();
}

#[test]
fn disconnect_is_detected_both_ways() {
    // Handler goes away.
    let listener = HostListener::bind().unwrap();
    let a = args(&listener, listener.token());
    let handler = thread::spawn(move || drop(HandlerSession::connect(&a).unwrap()));
    let mut host = listener.accept(WAIT, None).unwrap();
    handler.join().unwrap();
    assert!(matches!(host.recv(), Err(Error::Disconnected)));

    // Host goes away.
    let listener = HostListener::bind().unwrap();
    let a = args(&listener, listener.token());
    let handler = thread::spawn(move || {
        let mut s = HandlerSession::connect(&a).unwrap();
        s.recv()
    });
    drop(listener.accept(WAIT, None).unwrap());
    assert!(matches!(handler.join().unwrap(), Err(Error::Disconnected)));
}

#[test]
fn split_halves_work_from_different_threads() {
    let listener = HostListener::bind().unwrap();
    let a = args(&listener, listener.token());
    let handler = thread::spawn(move || {
        let (mut tx, mut rx) = HandlerSession::connect(&a).unwrap().split();
        assert_eq!(rx.recv().unwrap(), HostMessage::Ping);
        assert_eq!(rx.recv().unwrap(), HostMessage::Close);
        tx.send(&HandlerMessage::Closed).unwrap();
    });
    let (mut tx, mut rx, process) = listener.accept(WAIT, None).unwrap().split();
    assert!(process.is_none());
    let reader = thread::spawn(move || rx.recv());
    tx.send(&HostMessage::Ping).unwrap();
    tx.send(&HostMessage::Close).unwrap();
    assert_eq!(reader.join().unwrap().unwrap(), HandlerMessage::Closed);
    handler.join().unwrap();
}

#[test]
fn recv_timeout_gives_timeout() {
    let listener = HostListener::bind().unwrap();
    let a = args(&listener, listener.token());
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let handler = thread::spawn(move || {
        let _s = HandlerSession::connect(&a).unwrap();
        let _ = done_rx.recv();
    });
    let mut host = listener.accept(WAIT, None).unwrap();
    assert!(matches!(host.recv_timeout(Duration::from_millis(100)), Err(Error::Timeout)));
    done_tx.send(()).unwrap();
    handler.join().unwrap();
    assert!(matches!(host.recv(), Err(Error::Disconnected)));
}

#[cfg(unix)]
#[test]
fn editor_that_exits_without_connecting() {
    let err = HostSession::spawn("/bin/sh", &["-c", "exit 3", "sh"], Mode::Headless, WAIT).err().unwrap();
    assert!(matches!(err, Error::ChildExited(Some(3))), "{err:?}");
}

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

#[test]
fn a_launcher_that_hands_over_and_exits_is_waited_on() {
    // The editor's launcher exits with success before connecting (it handed
    // the session to a running instance); the instance connects a little later.
    let listener = HostListener::bind().unwrap();
    let mut launcher = if cfg!(windows) { Command::new("cmd").args(["/C", "exit 0"]).spawn().unwrap() } else { Command::new("true").spawn().unwrap() };
    let good = args(&listener, listener.token());
    let instance = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        let mut session = HandlerSession::connect(&good).unwrap();
        session.send(&HandlerMessage::Closed).unwrap();
    });
    let mut host = listener.accept(WAIT, Some(&mut launcher)).unwrap();
    assert_eq!(host.recv().unwrap(), HandlerMessage::Closed);
    instance.join().unwrap();

    // One that fails is not waited on.
    let listener = HostListener::bind().unwrap();
    let mut failing = if cfg!(windows) { Command::new("cmd").args(["/C", "exit 3"]).spawn().unwrap() } else { Command::new("false").spawn().unwrap() };
    assert!(matches!(listener.accept(WAIT, Some(&mut failing)), Err(Error::ChildExited(_))));
}
