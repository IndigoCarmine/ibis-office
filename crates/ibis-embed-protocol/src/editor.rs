// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Everything an editor needs to edit ibisOffice's embedded objects, with
//! no UI of its own: what it was started for (a headless request is
//! answered by [`Headless`] there and then), one instance for every object
//! (a later launch hands its object to the running one and exits), a
//! connection per object on a thread of its own, and the objects' news as
//! [`EditorEvent`]s for the UI to take in its own loop.
//!
//! ```no_run
//! use ibis_embed_protocol::editor::{self, EditorEvent, Headless, Start};
//! use ibis_embed_protocol::{OleContent, Preview};
//!
//! struct Mine;
//! impl Headless for Mine {}  // New, Import, Export, Render: refused unless implemented
//!
//! match editor::start("com.example.editor", Mine)? {
//!     Start::NotStarted => { /* run on its own, as usual */ }
//!     Start::Done => {}       // a headless request answered, or handed to the running window
//!     Start::Interactive(editor) => {
//!         // In the UI's loop (a timer, an idle callback…):
//!         while let Some(event) = editor.try_next() {
//!             match event {
//!                 EditorEvent::Opened { object, content, .. } => { /* show it */ }
//!                 EditorEvent::Closed { object } => { /* forget it */ }
//!             }
//!         }
//!         // When the user changes an object:
//!         // editor.update(object, content, Preview::svg(svg), Some((w, h)))?;
//!     }
//! }
//! # Ok::<(), ibis_embed_protocol::Error>(())
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};

use crate::{
    Error, HandlerArgs, HandlerMessage, HandlerSender, HandlerSession, HostMessage, Instance,
    InstanceRequests, Mode, OleContent, Preview, Result, claim_instance,
};

/// An object open in the editor, as numbered by it (from 0, in the order
/// they came).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(pub usize);

/// What happened to the editor's objects.
#[derive(Debug)]
pub enum EditorEvent {
    /// ibisOffice opened an object here: show it. `title` is the document's
    /// name, `locale` "ja" or "en".
    Opened { object: ObjectId, content: OleContent, title: String, locale: String },
    /// ibisOffice closed the object (or went away): stop showing it.
    Closed { object: ObjectId },
}

/// A new object, or one made from a file.
pub struct Created {
    pub content: OleContent,
    pub preview: Preview,
    /// The size to show it at, in EMU (914 400 an inch); `None` keeps the host's choice.
    pub size_emu: Option<(i64, i64)>,
}

/// The answers to ibisOffice's requests without a window (`--mode
/// headless`). Each has a default that refuses the request.
pub trait Headless {
    /// An empty object (Insert ▸ Object ▸ the editor).
    fn new_object(&mut self) -> std::result::Result<Created, String> {
        Err("this editor does not make new objects".into())
    }
    /// An object made from a file's bytes (`extension` without the dot).
    fn import(&mut self, extension: &str, data: Vec<u8>) -> std::result::Result<Created, String> {
        let _ = data;
        Err(format!("this editor does not import .{extension}"))
    }
    /// The object as a file's bytes.
    fn export(&mut self, content: OleContent, extension: &str) -> std::result::Result<Vec<u8>, String> {
        let _ = content;
        Err(format!("this editor does not export .{extension}"))
    }
    /// The object's picture drawn again, and its size in EMU.
    fn render(&mut self, content: OleContent) -> std::result::Result<(Preview, Option<(i64, i64)>), String> {
        let _ = content;
        Err("this editor does not draw objects without its window".into())
    }
}

/// Answers nothing (an editor with a window only).
pub struct NoHeadless;

impl Headless for NoHeadless {}

/// What [`start`] found.
pub enum Start {
    /// Not started by ibisOffice: the editor runs on its own.
    NotStarted,
    /// Nothing left to do: a headless request was answered, or the object
    /// went to the editor's window already open. Exit.
    Done,
    /// This process is the editor's window: show the objects of [`Editor`].
    Interactive(Editor),
}

/// Reads this process's arguments and does what ibisOffice started it for.
/// `app_id` (reverse-DNS, as `com.example.editor`) makes the editor one
/// instance: every object of every document goes to one window.
pub fn start(app_id: &str, headless: impl Headless) -> Result<Start> {
    start_with(app_id, HandlerArgs::from_env()?, headless)
}

/// [`start`] with the arguments given (for tests, or an editor that parses its own).
pub fn start_with(app_id: &str, args: Option<HandlerArgs>, mut headless: impl Headless) -> Result<Start> {
    let Some(args) = args else { return Ok(Start::NotStarted) };
    match args.mode {
        Mode::Headless => {
            answer(&mut HandlerSession::connect(&args)?, &mut headless)?;
            Ok(Start::Done)
        }
        Mode::Interactive => match claim_instance(app_id, &args)? {
            Instance::HandedOver => Ok(Start::Done),
            Instance::First(requests) => Ok(Start::Interactive(Editor::new(args, requests))),
        },
    }
}

/// One headless request, one answer.
fn answer(session: &mut HandlerSession, headless: &mut impl Headless) -> Result<()> {
    let created = |r: std::result::Result<Created, String>| match r {
        Ok(c) => HandlerMessage::Created { content: c.content, preview: c.preview, size_emu: c.size_emu },
        Err(message) => HandlerMessage::Error { message },
    };
    let reply = match session.recv()? {
        HostMessage::New => created(headless.new_object()),
        HostMessage::Import { extension, data } => created(headless.import(&extension, data)),
        HostMessage::Export { content, extension } => match headless.export(content, &extension) {
            Ok(data) => HandlerMessage::Exported { data },
            Err(message) => HandlerMessage::Error { message },
        },
        HostMessage::Render { content } => match headless.render(content) {
            Ok((preview, size_emu)) => HandlerMessage::Rendered { preview, size_emu },
            Err(message) => HandlerMessage::Error { message },
        },
        other => HandlerMessage::Error { message: format!("not a request without a window: {other:?}") },
    };
    session.send(&reply)
}

/// The editor's window's side: the objects ibisOffice opens in it (this
/// launch's, then those later launches hand over), each on a connection
/// of its own.
pub struct Editor {
    events: mpsc::Receiver<EditorEvent>,
    senders: Arc<Mutex<HashMap<ObjectId, HandlerSender>>>,
}

impl Editor {
    fn new(first: HandlerArgs, requests: InstanceRequests) -> Self {
        let (tx, events) = mpsc::channel();
        let senders: Arc<Mutex<HashMap<ObjectId, HandlerSender>>> = Arc::default();
        connect(ObjectId(0), first, tx.clone(), senders.clone());
        let later = senders.clone();
        std::thread::spawn(move || {
            let mut next = 1;
            while let Some(args) = requests.recv() {
                connect(ObjectId(next), args, tx.clone(), later.clone());
                next += 1;
            }
        });
        Editor { events, senders }
    }

    /// The next event, if one came (for a UI's loop).
    pub fn try_next(&self) -> Option<EditorEvent> {
        self.events.try_recv().ok()
    }

    /// Waits for the next event; `None` once none can come.
    pub fn next(&self) -> Option<EditorEvent> {
        self.events.recv().ok()
    }

    /// A handle for sending from other threads than the one holding the editor.
    pub fn handle(&self) -> EditorHandle {
        EditorHandle { senders: self.senders.clone() }
    }

    /// The object changed: ibisOffice replaces it and shows `preview`
    /// (at `size_emu`, if given). Send it at each change the user should see.
    pub fn update(&self, object: ObjectId, content: OleContent, preview: Preview, size_emu: Option<(i64, i64)>) -> Result<()> {
        self.handle().update(object, content, preview, size_emu)
    }

    /// The object closed here: ibisOffice is told and stops waiting for it.
    pub fn close(&self, object: ObjectId) {
        self.handle().close(object)
    }

    /// Every object closed (the window is closing).
    pub fn close_all(&self) {
        self.handle().close_all()
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.close_all();
    }
}

/// Sends for the editor's objects; cheap to clone, for any thread.
#[derive(Clone)]
pub struct EditorHandle {
    senders: Arc<Mutex<HashMap<ObjectId, HandlerSender>>>,
}

impl EditorHandle {
    /// See [`Editor::update`].
    pub fn update(&self, object: ObjectId, content: OleContent, preview: Preview, size_emu: Option<(i64, i64)>) -> Result<()> {
        if preview.is_empty() {
            return Err(Error::Invalid("an update needs a preview (PNG or SVG)".into()));
        }
        let mut senders = self.senders.lock().map_err(|_| Error::Disconnected)?;
        let sender = senders.get_mut(&object).ok_or(Error::Disconnected)?;
        sender.send(&HandlerMessage::Update { content, preview, size_emu })
    }

    /// See [`Editor::close`].
    pub fn close(&self, object: ObjectId) {
        if let Ok(mut senders) = self.senders.lock()
            && let Some(mut sender) = senders.remove(&object)
        {
            let _ = sender.send(&HandlerMessage::Closed);
        }
    }

    /// See [`Editor::close_all`].
    pub fn close_all(&self) {
        if let Ok(mut senders) = self.senders.lock() {
            for (_, mut sender) in senders.drain() {
                let _ = sender.send(&HandlerMessage::Closed);
            }
        }
    }
}

/// The object of `args` on a thread of its own: its `Open`, then its `Close`, as events.
fn connect(object: ObjectId, args: HandlerArgs, events: mpsc::Sender<EditorEvent>, senders: Arc<Mutex<HashMap<ObjectId, HandlerSender>>>) {
    std::thread::spawn(move || {
        let opened = HandlerSession::connect(&args).and_then(|mut session| match session.recv()? {
            HostMessage::Open { content, title, locale } => Ok((session, content, title, locale)),
            other => Err(Error::Unexpected(format!("{other:?}"))),
        });
        let Ok((session, content, title, locale)) = opened else { return };
        let (sender, mut receiver) = session.split();
        if let Ok(mut senders) = senders.lock() {
            senders.insert(object, sender);
        }
        if events.send(EditorEvent::Opened { object, content, title, locale }).is_err() {
            return;
        }
        loop {
            match receiver.recv() {
                Ok(HostMessage::Close) | Err(_) => break,
                Ok(_) => {}
            }
        }
        if let Ok(mut senders) = senders.lock()
            && let Some(mut sender) = senders.remove(&object)
        {
            let _ = sender.send(&HandlerMessage::Closed);
        }
        let _ = events.send(EditorEvent::Closed { object });
    });
}
