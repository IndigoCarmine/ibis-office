// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The editor library over real local sockets: objects opened in one
//! window (the second handed over), updated and closed; headless requests.

use std::time::Duration;

use ibis_embed_protocol::editor::{self, Created, EditorEvent, Headless, NoHeadless, ObjectId, Start};
use ibis_embed_protocol::{HandlerArgs, HandlerMessage, HostListener, HostMessage, Mode, OleContent, Preview};

const WAIT: Duration = Duration::from_secs(20);

fn args(listener: &HostListener, mode: Mode) -> HandlerArgs {
    HandlerArgs { socket: listener.name().to_owned(), token: listener.token().to_owned(), mode }
}

fn content(text: &str) -> OleContent {
    let mut c = OleContent { prog_id: "Test.Object.1".into(), ..Default::default() };
    c.set_stream("CONTENTS", text.as_bytes().to_vec());
    c
}

fn open(text: &str) -> HostMessage {
    HostMessage::Open { content: content(text), title: "doc.pptx".into(), locale: "ja".into() }
}

#[test]
fn objects_go_to_one_window() {
    let app = format!("test.editor.{}", std::process::id());
    let first = HostListener::bind().unwrap();
    let Start::Interactive(editor) = editor::start_with(&app, Some(args(&first, Mode::Interactive)), NoHeadless).unwrap() else { panic!("not the window") };
    let mut host1 = first.accept(WAIT, None).unwrap();
    host1.send(&open("one")).unwrap();
    let Some(EditorEvent::Opened { object, content, title, .. }) = editor.next() else { panic!("not opened") };
    assert_eq!((object, content.stream("CONTENTS"), title.as_str()), (ObjectId(0), Some(&b"one"[..]), "doc.pptx"));

    // A change goes to ibisOffice.
    editor.update(ObjectId(0), self::content("one!"), Preview::svg("<svg/>"), Some((10, 20))).unwrap();
    let HandlerMessage::Update { content, size_emu, .. } = host1.recv().unwrap() else { panic!("no update") };
    assert_eq!((content.stream("CONTENTS"), size_emu), (Some(&b"one!"[..]), Some((10, 20))));
    assert!(editor.update(ObjectId(0), self::content("x"), Preview::default(), None).is_err(), "an update needs a picture");

    // A second launch hands its object to this window.
    let second = HostListener::bind().unwrap();
    assert!(matches!(editor::start_with(&app, Some(args(&second, Mode::Interactive)), NoHeadless).unwrap(), Start::Done));
    let mut host2 = second.accept(WAIT, None).unwrap();
    host2.send(&open("two")).unwrap();
    let Some(EditorEvent::Opened { object, .. }) = editor.next() else { panic!("not opened") };
    assert_eq!(object, ObjectId(1));

    // ibisOffice closes the second: the editor hears it and answers.
    host2.send(&HostMessage::Close).unwrap();
    assert!(matches!(editor.next(), Some(EditorEvent::Closed { object: ObjectId(1) })));
    assert_eq!(host2.recv().unwrap(), HandlerMessage::Closed);

    // The window closes: ibisOffice hears of the first.
    drop(editor);
    assert_eq!(host1.recv().unwrap(), HandlerMessage::Closed);
}

struct Texts;

impl Headless for Texts {
    fn new_object(&mut self) -> Result<Created, String> {
        Ok(Created { content: content("new"), preview: Preview::svg("<svg/>"), size_emu: None })
    }
}

#[test]
fn headless_requests_are_answered() {
    let app = format!("test.editor.headless.{}", std::process::id());
    let ask = |request: HostMessage, headless: Box<dyn FnOnce(HandlerArgs) + Send>| {
        let listener = HostListener::bind().unwrap();
        let a = args(&listener, Mode::Headless);
        let editor = std::thread::spawn(move || headless(a));
        let mut host = listener.accept(WAIT, None).unwrap();
        let reply = host.request(&request).unwrap();
        editor.join().unwrap();
        reply
    };
    let app1 = app.clone();
    let reply = ask(HostMessage::New, Box::new(move |a| assert!(matches!(editor::start_with(&app1, Some(a), Texts).unwrap(), Start::Done))));
    let HandlerMessage::Created { content, .. } = reply else { panic!("not created") };
    assert_eq!(content.stream("CONTENTS"), Some(&b"new"[..]));
    // Not implemented: refused, with a message.
    let reply = ask(HostMessage::Import { extension: "txt".into(), data: vec![] }, Box::new(move |a| assert!(matches!(editor::start_with(&app, Some(a), Texts).unwrap(), Start::Done))));
    assert!(matches!(reply, HandlerMessage::Error { .. }));
    assert!(matches!(editor::start_with("x", None, NoHeadless).unwrap(), Start::NotStarted));
}
