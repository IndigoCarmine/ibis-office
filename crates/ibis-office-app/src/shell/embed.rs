// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 埋め込みオブジェクトの編集: an embedded OLE object opened in the external
//! editor registered for its ProgID (a manifest in `embed-handlers/` beside
//! the settings), over a local socket (`ibis-embed-protocol`), with no COM.
//! Each time the editor saves, the object and its preview are replaced as
//! one undoable edit. The presentation's, the workbook's and the
//! document's pictures share this.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use gpui_kit::*;
use ibis_doc_model::{Block, Command, EmuRect, FlowRunKind, MediaFormat, MediaId, OleId, Picture, ShapeId, ShapeKind, SlideId};
use ibis_embed_protocol::{self as protocol, HandlerManifest, HandlerMessage, HostMessage, HostSender, HostSession, Mode, OleContent, Preview};

use crate::shell::editor::SlideEditor;
use crate::shell::i18n;

/// The folder of the external editors' manifests, beside the settings.
const HANDLERS_DIR: &str = "embed-handlers";
/// How long an editor has to start and connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the editor's messages are looked at while one is open.
const POLL: Duration = Duration::from_millis(120);

/// The external editors registered, as last read.
static HANDLERS: Mutex<Option<Vec<HandlerManifest>>> = Mutex::new(None);

/// The external editors registered (bad manifests logged and left out),
/// read again from their folder.
pub(crate) fn reload_handlers() -> Vec<HandlerManifest> {
    let read: Vec<HandlerManifest> = handlers_dir()
        .map(|dir| protocol::load_manifest_dir(&dir))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(path, manifest)| manifest.map_err(|e| log::warn!("{}: {e}", path.display())).ok())
        .collect();
    if let Ok(mut cached) = HANDLERS.lock() {
        *cached = Some(read.clone());
    }
    read
}

/// The external editors registered, as last read (menus are built often).
pub(crate) fn handlers() -> Vec<HandlerManifest> {
    let cached = HANDLERS.lock().ok().and_then(|h| h.clone());
    cached.unwrap_or_else(reload_handlers)
}

/// 挿入 > オブジェクト: a new object of each editor that makes them, and one from a file.
pub(crate) fn insert_menu() -> crate::shell::chrome::MenuEntry {
    use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
    let mut items: Vec<MenuEntry> = handlers()
        .into_iter()
        .filter(|h| h.new_object)
        .map(|h| {
            let id = h.id.clone();
            MenuItem::new("embed.new", h.name.get(locale()).to_string()).icon("draw").on(move |e, w, cx| e.insert_embedded(&id, None, w, cx)).into()
        })
        .collect();
    if !items.is_empty() {
        items.push(MenuEntry::Separator);
    }
    items.push(MenuItem::tr("embed.from-file").icon("upload_file").on(|e, w, cx| e.pick_embedded_file(w, cx)).into());
    Submenu::tr("embed.insert", "shapes", items).into()
}

/// One request to an editor without its window (`--mode headless`), and its answer.
fn headless(handler: &HandlerManifest, request: HostMessage) -> Result<HandlerMessage, String> {
    let mut session = HostSession::spawn(&handler.command, &handler.args, Mode::Headless, CONNECT_TIMEOUT).map_err(|e| e.to_string())?;
    match session.request(&request).map_err(|e| e.to_string())? {
        HandlerMessage::Error { message } => Err(message),
        answer => Ok(answer),
    }
}

/// The folder the manifests go in (shown in the settings).
pub(crate) fn handlers_dir() -> Option<std::path::PathBuf> {
    Some(crate::shell::settings::dir()?.join(HANDLERS_DIR))
}

/// What an editor is editing: a shape (of a slide or a sheet), or a
/// document's inline picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Shape { slide: SlideId, shape: ShapeId },
    Inline(ibis_doc_model::FlowPos),
}

/// What the editor's thread hands the UI.
enum Event {
    Update { content: OleContent, preview: Preview, size_emu: Option<(i64, i64)> },
    Error(String),
    Closed,
}

/// An open editor.
pub(crate) struct Session {
    target: Target,
    /// The status shown while the editor starts, cleared when it is heard from.
    opening: Option<SharedString>,
    events: mpsc::Receiver<Event>,
    sender: Arc<Mutex<Option<HostSender>>>,
    /// The size the editor draws the object at (before this edit), when
    /// known: an edit keeps the object's scale on the page.
    natural: Option<(i64, i64)>,
}

impl Session {
    fn send(&self, message: &HostMessage) {
        if let Some(sender) = self.sender.lock().ok().as_mut().and_then(|s| s.as_mut()) {
            let _ = sender.send(message);
        }
    }
}

impl Drop for Session {
    /// The document is closed or another opened: the editor is told to close.
    fn drop(&mut self) {
        self.send(&HostMessage::Close);
    }
}

/// The editors open for the document.
#[derive(Default)]
pub(crate) struct Sessions {
    open: Vec<Session>,
    polling: bool,
    /// The size each object's editor draws it at, as far as asked (see
    /// `render_embedded_previews`).
    natural: HashMap<OleId, (i64, i64)>,
}

impl Sessions {
    /// Every editor closed (a new document replaces the old).
    pub(crate) fn close_all(&mut self) {
        self.open.clear();
        self.natural.clear();
    }
}

/// `frame`'s size for an object drawn at `new` that was drawn at `old`:
/// scaled as it was on the page (PowerPoint keeps a resized object's scale).
fn scaled_size(frame: (i64, i64), old: Option<(i64, i64)>, new: (i64, i64)) -> (i64, i64) {
    match old {
        Some((ow, oh)) if ow > 0 && oh > 0 => {
            let scale = |f: i64, n: i64, o: i64| ((f as i128 * n as i128) / o as i128) as i64;
            (scale(frame.0, new.0, ow).max(1), scale(frame.1, new.1, oh).max(1))
        }
        _ => new,
    }
}

/// The editor for `handler` started on `content`: its messages come to `events`.
fn launch(handler: &HandlerManifest, target: Target, content: OleContent, title: String) -> Session {
    let (tx, events) = mpsc::channel();
    let sender = Arc::new(Mutex::new(None));
    let slot = sender.clone();
    let command = handler.command.clone();
    let args = handler.args.clone();
    let opening = Some(i18n::format("embed.opening", &[("name", &handler.name.get(locale()))]).into());
    let locale = locale().to_string();
    std::thread::spawn(move || {
        let fail = |e: protocol::Error| {
            let _ = tx.send(Event::Error(e.to_string()));
            let _ = tx.send(Event::Closed);
        };
        let mut session = match HostSession::spawn(&command, &args, Mode::Interactive, CONNECT_TIMEOUT) {
            Ok(s) => s,
            Err(e) => return fail(e),
        };
        if let Err(e) = session.send(&HostMessage::Open { content, title, locale }) {
            return fail(e);
        }
        let (host_sender, mut receiver, _process) = session.split();
        if let Ok(mut slot) = slot.lock() {
            *slot = Some(host_sender);
        }
        loop {
            let event = match receiver.recv() {
                Ok(HandlerMessage::Update { content, preview, size_emu }) => Event::Update { content, preview, size_emu },
                Ok(HandlerMessage::Error { message }) => Event::Error(message),
                Ok(HandlerMessage::Closed) | Err(_) => break,
                Ok(other) => {
                    log::warn!("external editor: unexpected {other:?}");
                    continue;
                }
            };
            if tx.send(event).is_err() {
                break;
            }
        }
        let _ = tx.send(Event::Closed);
    });
    Session { target, opening, events, sender, natural: None }
}

/// The picture `handler` draws of `content` (headless `Render`): PNG and the size it is drawn at.
fn render(handler: &HandlerManifest, content: OleContent) -> Result<(Vec<u8>, Option<(i64, i64)>), String> {
    match headless(handler, HostMessage::Render { content })? {
        HandlerMessage::Rendered { preview, size_emu } => Ok((protocol::host::preview_png(&preview, size_emu).map_err(|e| e.to_string())?, size_emu)),
        other => Err(format!("{other:?}")),
    }
}

/// Every picture in `blocks` showing an embedded object.
fn flow_objects(blocks: &[Block], out: &mut Vec<(MediaId, OleId)>) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => out.extend(p.runs.iter().filter_map(|r| match &r.kind {
                FlowRunKind::Picture { media, look, .. } => Some((*media, look.ole?)),
                _ => None,
            })),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| flow_objects(&c.blocks, out)),
        }
    }
}

impl SlideEditor {
    /// The embedded object a target shows, and the picture showing it.
    fn embedded_at(&self, target: Target) -> Option<(Picture, OleId)> {
        let picture = match target {
            Target::Shape { slide, shape } => match &self.presentation().slide(slide)?.shape(shape)?.kind {
                ShapeKind::Picture(p) => p.clone(),
                _ => return None,
            },
            Target::Inline(at) => self.inline_picture_at(at)?,
        };
        let ole = picture.ole?;
        Some((picture, ole))
    }

    /// Whether `target` is an embedded object (a double click opens its editor).
    pub(crate) fn is_embedded(&self, target: Target) -> bool {
        self.embedded_at(target).is_some()
    }

    /// Open the embedded object at `target` in its external editor; one
    /// already open for it is brought to mind instead.
    pub(crate) fn edit_embedded(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, ole)) = self.embedded_at(target) else { return };
        if let Some(open) = self.embed_sessions.open.iter().find(|s| s.target == target) {
            open.send(&HostMessage::Ping);
            return;
        }
        let Some(object) = self.presentation().ole(ole).cloned() else { return };
        let handlers = reload_handlers();
        let Some(handler) = protocol::find_handler(&handlers, &object.prog_id) else {
            let detail = i18n::format("embed.no-editor.detail", &[("prog_id", &object.prog_id), ("dir", &handlers_dir().map(|d| d.display().to_string()).unwrap_or_default())]);
            self.tell(&i18n::text("embed.no-editor"), &detail, window, cx);
            return;
        };
        let content = match protocol::host::content_of(&object.data) {
            Ok(c) => c,
            Err(e) => return self.tell(&i18n::text("embed.unreadable"), &e.to_string(), window, cx),
        };
        let title = self.file_name.as_ref().map(|n| n.to_string()).unwrap_or_default();
        let mut session = launch(handler, target, content.clone(), title);
        session.natural = self.embed_sessions.natural.get(&ole).copied();
        if session.natural.is_none() {
            // The size the editor draws it at now, so the edit keeps its scale.
            let handler = handler.clone();
            let job = cx.background_executor().spawn(async move { render(&handler, content) });
            cx.spawn(async move |this, cx| {
                let Ok((_, Some(size))) = job.await else { return };
                this.update(cx, |this, _| {
                    this.embed_sessions.natural.insert(ole, size);
                    if let Some(open) = this.embed_sessions.open.iter_mut().find(|s| s.target == target && s.natural.is_none()) {
                        open.natural = Some(size);
                    }
                })
                .ok();
            })
            .detach();
        }
        self.status = session.opening.clone();
        self.embed_sessions.open.push(session);
        cx.notify();
        if !self.embed_sessions.polling {
            self.embed_sessions.polling = true;
            cx.spawn_in(window, async move |this, cx| loop {
                cx.background_executor().timer(POLL).await;
                let more = this.update_in(cx, |this, window, cx| this.poll_embedded(window, cx)).unwrap_or(false);
                if !more {
                    break;
                }
            })
            .detach();
        }
    }

    /// One tick: the editors' saves applied, closed ones forgotten. Whether any is left.
    fn poll_embedded(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let mut updates = Vec::new();
        let mut errors = Vec::new();
        let mut heard = Vec::new();
        self.embed_sessions.open.retain_mut(|session| {
            loop {
                let event = session.events.try_recv();
                if !matches!(event, Err(mpsc::TryRecvError::Empty)) {
                    heard.extend(session.opening.take());
                }
                match event {
                    Ok(Event::Update { content, preview, size_emu }) => {
                        updates.push((session.target, content, preview, size_emu, session.natural));
                        // The next edit is scaled from this one.
                        session.natural = size_emu.or(session.natural);
                    }
                    Ok(Event::Error(message)) => errors.push(message),
                    Ok(Event::Closed) | Err(mpsc::TryRecvError::Disconnected) => return false,
                    Err(mpsc::TryRecvError::Empty) => return true,
                }
            }
        });
        if self.status.as_ref().is_some_and(|s| heard.contains(s)) {
            self.status = None;
            cx.notify();
        }
        for (target, content, preview, size_emu, natural) in updates {
            if let Err(message) = self.apply_embedded(target, &content, &preview, size_emu, natural, cx) {
                errors.push(message);
            }
        }
        if let Some(message) = errors.into_iter().next() {
            self.tell(&i18n::text("embed.failed"), &message, window, cx);
        }
        let more = !self.embed_sessions.open.is_empty();
        self.embed_sessions.polling = more;
        more
    }

    /// The editor saved: a new object and preview for `target`, as one undoable edit.
    /// `natural` is the size the editor drew it at before, which the new size
    /// is scaled from.
    fn apply_embedded(&mut self, target: Target, content: &OleContent, preview: &Preview, size_emu: Option<(i64, i64)>, natural: Option<(i64, i64)>, cx: &mut Context<Self>) -> Result<(), String> {
        // Gone, or no longer an object (deleted, undone past): nothing to update.
        let Some((mut picture, ole)) = self.embedded_at(target) else { return Ok(()) };
        let old = self.presentation().ole(ole).cloned().ok_or_else(|| "the object is gone".to_string())?;
        let data = protocol::host::object_of(content, Some(&old.data)).map_err(|e| e.to_string())?;
        let png = protocol::host::preview_png(preview, size_emu).map_err(|e| e.to_string())?;
        let media = self.document.ids().add_media(MediaFormat::Png, png.clone());
        self.resources.add_picture(media, MediaFormat::Png, &png);
        picture.media = media;
        let ole = self.document.ids().add_ole(content.prog_id.clone(), data, old.as_icon);
        self.document.ids().set_ole_natural(ole, size_emu);
        picture.ole = Some(ole);
        match target {
            Target::Shape { slide, shape } => {
                let mut commands = vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Picture(picture)) }];
                // The size the editor asks for, at the object's scale, from the same top left corner.
                if let Some(new) = size_emu.filter(|&(w, h)| w > 0 && h > 0)
                    && let Some(frame) = self.presentation().slide(slide).and_then(|s| s.shape(shape)).map(|s| s.frame)
                {
                    let (w, h) = scaled_size((frame.width, frame.height), natural, new);
                    if (frame.width, frame.height) != (w, h) {
                        commands.push(Command::SetFrame { slide, shape, frame: EmuRect::new(frame.x, frame.y, w, h) });
                    }
                }
                self.run(commands, cx);
            }
            Target::Inline(at) => {
                let size = size_emu.filter(|&(w, h)| w > 0 && h > 0).map(|new| {
                    let (w, h) = match self.inline_size_at(at) {
                        Some(now) => scaled_size((now.width, now.height), natural, new),
                        None => new,
                    };
                    ibis_doc_model::EmuSize::new(w, h)
                });
                self.replace_inline_object(at, media, picture.ole.expect("an object"), size, cx);
            }
        }
        Ok(())
    }

    /// Pictures of embedded objects that cannot be drawn here (a vector EMF
    /// Office keeps for them) are drawn by their editors instead (headless `Render`), the current slide's first. Only
    /// the screen changes: the document keeps its own pictures.
    pub(crate) fn render_embedded_previews(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let presentation = self.presentation();
        let mut objects: Vec<(MediaId, OleId)> = Vec::new();
        let current = self.current_slide;
        let slides = presentation.slides.iter().filter(|s| s.id == current).chain(presentation.slides.iter().filter(|s| s.id != current));
        for slide in slides {
            objects.extend(slide.shapes.iter().filter_map(|s| match &s.kind {
                ShapeKind::Picture(p) => Some((p.media, p.ole?)),
                _ => None,
            }));
        }
        if let Some(flow) = &presentation.flow {
            flow_objects(&flow.body, &mut objects);
        }
        let mut seen = std::collections::HashSet::new();
        objects.retain(|o| seen.insert(o.1));
        let handlers = handlers();
        let jobs: Vec<(MediaId, OleId, HandlerManifest, OleContent)> = objects
            .into_iter()
            .filter(|(media, _)| {
                presentation.media(*media).is_some_and(|m| matches!(m.format, MediaFormat::Emf | MediaFormat::Wmf) && ibis_emf::extract_image(&m.data).is_none())
            })
            .filter_map(|(media, ole)| {
                let object = presentation.ole(ole)?;
                let handler = protocol::find_handler(&handlers, &object.prog_id)?.clone();
                let content = protocol::host::content_of(&object.data).ok()?;
                Some((media, ole, handler, content))
            })
            .collect();
        if jobs.is_empty() {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            // One at a time: each is a process of its own.
            let mut refused = Vec::new();
            for (media, ole, handler, content) in jobs {
                if refused.contains(&handler.id) {
                    continue;
                }
                let id = handler.id.clone();
                let answer = cx.background_executor().spawn(async move { render(&handler, content) }).await;
                let done = this.update(cx, |this, cx| match answer {
                    Ok((png, size)) => {
                        this.resources.set_stand_in(media, &png);
                        if let Some(size) = size {
                            this.embed_sessions.natural.insert(ole, size);
                        }
                        cx.notify();
                    }
                    Err(e) => {
                        log::info!("embedded object {}: no picture from {id}: {e}", ole.0);
                        refused.push(id);
                    }
                });
                if done.is_err() {
                    return;
                }
            }
        })
        .detach();
    }

    /// 挿入 > オブジェクト > ファイルから: a file an editor imports (by its extension).
    pub(crate) fn pick_embedded_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(i18n::text("_shell.insert").to_string().into()) });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            let data = std::fs::read(&path);
            this.update_in(cx, |this, window, cx| {
                let handlers = reload_handlers();
                let Some(handler) = handlers.iter().find(|h| h.can_import(&extension)) else {
                    let detail = i18n::format("embed.no-importer.detail", &[("extension", &extension)]);
                    return this.tell(&i18n::text("embed.no-editor"), &detail, window, cx);
                };
                match data {
                    Ok(data) => this.insert_embedded(&handler.id.clone(), Some((extension, data)), window, cx),
                    Err(e) => this.tell(&i18n::text("embed.failed"), &e.to_string(), window, cx),
                }
            })
            .ok();
        })
        .detach();
    }

    /// A new object made by the editor `handler_id` (empty, or from a file
    /// it imports), inserted as a picture of it; a new empty one is opened in
    /// the editor at once.
    pub(crate) fn insert_embedded(&mut self, handler_id: &str, file: Option<(String, Vec<u8>)>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(handler) = reload_handlers().into_iter().find(|h| h.id == handler_id) else { return };
        let open_after = file.is_none();
        let request = match file {
            Some((extension, data)) => HostMessage::Import { extension, data },
            None => HostMessage::New,
        };
        self.status = Some(i18n::format("embed.opening", &[("name", &handler.name.get(locale()))]).into());
        cx.notify();
        let job = cx.background_executor().spawn(async move { headless(&handler, request) });
        cx.spawn_in(window, async move |this, cx| {
            let answer = job.await;
            this.update_in(cx, |this, window, cx| {
                this.status = None;
                let (content, preview, size_emu) = match answer {
                    Ok(HandlerMessage::Created { content, preview, size_emu }) => (content, preview, size_emu),
                    Ok(other) => return this.tell(&i18n::text("embed.failed"), &format!("{other:?}"), window, cx),
                    Err(message) => return this.tell(&i18n::text("embed.failed"), &message, window, cx),
                };
                match this.insert_embedded_object(&content, &preview, size_emu, cx) {
                    Ok(Some(target)) if open_after => this.edit_embedded(target, window, cx),
                    Ok(_) => {}
                    Err(message) => this.tell(&i18n::text("embed.failed"), &message, window, cx),
                }
            })
            .ok();
        })
        .detach();
    }

    /// The object put where a picture would go (the slide's middle, the
    /// sheet's selected cell), the size the editor asks for.
    fn insert_embedded_object(&mut self, content: &OleContent, preview: &Preview, size_emu: Option<(i64, i64)>, cx: &mut Context<Self>) -> Result<Option<Target>, String> {
        let data = protocol::host::object_of(content, None).map_err(|e| e.to_string())?;
        let png = protocol::host::preview_png(preview, size_emu).map_err(|e| e.to_string())?;
        self.insert_embedded_data(&content.prog_id, data, png, size_emu, cx)
    }

    /// Ctrl+V of an object copied in its application (an OLE "Embed
    /// Source" on the clipboard): the object as it was copied, drawn by its
    /// editor here. False when there is no such object, or no editor for it
    /// (the clipboard's picture is pasted then, as before).
    pub(crate) fn paste_embedded(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(clip) = crate::shell::ole_clipboard::read() else { return false };
        let Ok(content) = protocol::host::content_of(&clip.embed_source) else { return false };
        let handlers = reload_handlers();
        let Some(handler) = protocol::find_handler(&handlers, &content.prog_id).cloned() else { return false };
        // Rebuilt, so what the container keeps is clean (no stale cached picture).
        let Ok(data) = protocol::host::object_of(&content, Some(&clip.embed_source)) else { return false };
        let prog_id = content.prog_id.clone();
        let size_hint = clip.size_emu;
        self.status = Some(i18n::format("embed.pasting", &[("name", &handler.name.get(locale()))]).into());
        cx.notify();
        let job = cx.background_executor().spawn(async move { render(&handler, content) });
        cx.spawn(async move |this, cx| {
            let answer = job.await;
            this.update(cx, |this, cx| {
                this.status = None;
                // The copied object, with the picture its editor draws.
                let inserted = answer.and_then(|(png, size_emu)| this.insert_embedded_data(&prog_id, data, png, size_emu.or(size_hint), cx));
                match inserted {
                    Ok(Some(Target::Shape { slide, shape })) => this.remember_natural(slide, shape),
                    Ok(_) => {}
                    Err(message) => {
                        log::warn!("paste of an embedded object: {message}");
                        this.status = Some(i18n::format("embed.paste-failed", &[("message", &message)]).into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        true
    }

    /// An inserted object's frame is the size its editor drew it at.
    fn remember_natural(&mut self, slide: SlideId, shape: ShapeId) {
        let Some(found) = self.presentation().slide(slide).and_then(|s| s.shape(shape)) else { return };
        if let ShapeKind::Picture(Picture { ole: Some(ole), .. }) = &found.kind {
            let size = (found.frame.width, found.frame.height);
            self.embed_sessions.natural.insert(*ole, size);
        }
    }

    /// Ctrl+C of one embedded object: put on the clipboard as OLE
    /// applications put one, so Office and the object's application paste
    /// it as the object. False when it could not be (the caller copies the
    /// picture instead).
    pub(crate) fn copy_embedded(&self, picture: &Picture, size_emu: (i64, i64)) -> bool {
        let Some(object) = picture.ole.and_then(|ole| self.presentation().ole(ole)) else { return false };
        let Ok(content) = protocol::host::content_of(&object.data) else { return false };
        let media = self.presentation().media(picture.media);
        let emf = media.filter(|m| m.format == MediaFormat::Emf).map(|m| &m.data[..]);
        let stand_in = self.resources.images.get(&picture.media).filter(|i| i.format == ImageFormat::Png).map(|i| i.bytes.clone());
        let png = media.filter(|m| m.format == MediaFormat::Png).map(|m| m.data.to_vec()).or(stand_in);
        let copy = crate::shell::ole_clipboard::ObjectCopy {
            embed_source: &object.data,
            clsid: content.clsid.0,
            user_type: &content.user_type,
            size_emu,
            emf,
            png: png.as_deref(),
        };
        match crate::shell::ole_clipboard::write(&copy) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("copy of an embedded object: {e}");
                false
            }
        }
    }

    /// The object `data` (a compound file of ProgID `prog_id`) put where a
    /// picture would go, shown by `png`, the size `size_emu`.
    fn insert_embedded_data(&mut self, prog_id: &str, data: Vec<u8>, png: Vec<u8>, size_emu: Option<(i64, i64)>, cx: &mut Context<Self>) -> Result<Option<Target>, String> {
        let shape = match self.format.kind() {
            ibis_pptx::Kind::Presentation => self.insert_picture("object.png", png, cx),
            ibis_pptx::Kind::Workbook => self.insert_sheet_picture("object.png", png, cx),
            ibis_pptx::Kind::Document => {
                let ole = self.document.ids().add_ole(prog_id.to_string(), data, false);
                self.document.ids().set_ole_natural(ole, size_emu);
                let size = size_emu.filter(|&(w, h)| w > 0 && h > 0).map(|(w, h)| ibis_doc_model::EmuSize::new(w, h));
                return match self.insert_inline_object(png, ole, size, cx) {
                    Some(at) => Ok(Some(Target::Inline(at))),
                    None => Err(i18n::text("embed.not-here").to_string()),
                };
            }
        };
        let Some(shape) = shape else { return Ok(None) };
        let slide = self.current_slide;
        let Some(ShapeKind::Picture(mut picture)) = self.presentation().slide(slide).and_then(|s| s.shape(shape)).map(|s| s.kind.clone()) else { return Ok(None) };
        let ole = self.document.ids().add_ole(prog_id.to_string(), data, false);
        self.document.ids().set_ole_natural(ole, size_emu);
        picture.ole = Some(ole);
        let mut commands = vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Picture(picture)) }];
        if let Some((w, h)) = size_emu.filter(|&(w, h)| w > 0 && h > 0)
            && let Some(frame) = self.presentation().slide(slide).and_then(|s| s.shape(shape)).map(|s| s.frame)
        {
            // Kept where it was put, around its middle.
            commands.push(Command::SetFrame { slide, shape, frame: EmuRect::new(frame.x + (frame.width - w) / 2, frame.y + (frame.height - h) / 2, w, h) });
        }
        // One undo step with the insertion.
        self.document.amend_last(commands).map_err(|e| e.to_string())?;
        if self.format.kind() == ibis_pptx::Kind::Workbook {
            self.sync_workbook_peers(cx);
        }
        cx.notify();
        Ok(Some(Target::Shape { slide, shape }))
    }

    /// A message for the user about an embedded object.
    fn tell(&mut self, message: &str, detail: &str, window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(PromptLevel::Warning, message, Some(detail), &[PromptButton::ok(i18n::text("embed.ok"))], cx);
        cx.background_executor().spawn(async move { let _ = answer.await; }).detach();
    }
}

fn locale() -> &'static str {
    match i18n::language() {
        i18n::Language::English => "en",
        i18n::Language::Japanese => "ja",
    }
}
