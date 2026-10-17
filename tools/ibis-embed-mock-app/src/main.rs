// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A windowed mock editor for ibisOffice's embedded objects, as
//! `ibis-embed-mock` but for trying by hand: the object's `CONTENTS` stream
//! is the text in the window (UTF-8), and each change goes to ibisOffice at
//! once (`Update`) with a picture of the text, which it shows as the
//! object. It opens an object of any class; the class and the other
//! streams are given back as they came. One window holds every object
//! ibisOffice opens in it (a second launch hands its object over), listed
//! on the left.
//!
//! All the talking to ibisOffice is `ibis_embed_protocol::editor`'s: what
//! is here is what any editor has to do itself (its content, its picture,
//! its UI).
//!
//! Started by ibisOffice (`--ibis-embed … --mode interactive|headless`);
//! register it with a manifest in ibisOffice's `embed-handlers/` folder
//! (see `ibis-embed-mock-app.toml` beside this crate).

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::time::Duration;

use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme as _, Root, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_embed_protocol::editor::{self, Created, Editor, EditorEvent, Headless, ObjectId, Start};
use ibis_embed_protocol::{OleContent, Preview};

/// The class of the objects it makes, as `ibis-embed-mock`'s.
const PROG_ID: &str = "IbisOffice.Mock.1";
const USER_TYPE: &str = "Mock Object";
const STREAM: &str = "CONTENTS";

/// `like` (the object opened, with its class and other streams) holding
/// `text`; a new object of the mock's class without one.
fn content(text: &str, like: Option<&OleContent>) -> OleContent {
    let mut content = like.cloned().unwrap_or_default();
    if content.prog_id.is_empty() {
        content.prog_id = PROG_ID.into();
        content.user_type = USER_TYPE.into();
    }
    content.set_stream(STREAM, text.as_bytes().to_vec());
    content
}

fn text_of(content: &OleContent) -> String {
    String::from_utf8_lossy(content.stream(STREAM).unwrap_or_default()).into_owned()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The picture of `text`: its lines in a frame, as wide as the longest. Its size in EMU.
fn preview(text: &str) -> (Preview, (i64, i64)) {
    let lines: Vec<&str> = if text.is_empty() { vec![" "] } else { text.lines().collect() };
    let longest = lines.iter().map(|l| l.chars().map(|c| if c.is_ascii() { 1.0 } else { 1.9 }).sum::<f64>()).fold(1.0, f64::max);
    let (size, line) = (24.0, 32.0);
    let w = (longest * size * 0.62 + 40.0).ceil().max(120.0);
    let h = (lines.len() as f64 * line + 32.0).ceil();
    let mut svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><rect x="1" y="1" width="{}" height="{}" rx="6" fill="#ffffff" stroke="#2b6cb0" stroke-width="2"/>"##, w - 2.0, h - 2.0);
    for (i, l) in lines.iter().enumerate() {
        svg.push_str(&format!(r##"<text x="20" y="{}" font-family="Helvetica, Arial, sans-serif" font-size="{size}" fill="#1a202c">{}</text>"##, 16.0 + line * (i as f64 + 0.75), escape(l)));
    }
    svg.push_str("</svg>");
    // 96 pixels an inch, 914 400 EMU an inch.
    (Preview::svg(svg), ((w * 9525.0) as i64, (h * 9525.0) as i64))
}

/// The id that makes the mock one instance (one window for every object).
const APP_ID: &str = "jp.ibis.embed-mock-app";

fn main() {
    match editor::start(APP_ID, Requests) {
        Ok(Start::NotStarted) => {
            eprintln!("ibis-embed-mock-app is started by ibisOffice (--ibis-embed <socket> --token <token> --mode interactive|headless).");
            std::process::exit(2);
        }
        Ok(Start::Done) => {}
        Ok(Start::Interactive(editor)) => interactive(editor),
        Err(e) => {
            eprintln!("ibis-embed-mock-app: {e}");
            std::process::exit(1);
        }
    }
}

/// The requests without the window.
struct Requests;

impl Headless for Requests {
    fn new_object(&mut self) -> Result<Created, String> {
        Ok(created("mock"))
    }

    fn import(&mut self, _: &str, data: Vec<u8>) -> Result<Created, String> {
        Ok(created(&String::from_utf8_lossy(&data)))
    }

    fn export(&mut self, content: OleContent, _: &str) -> Result<Vec<u8>, String> {
        Ok(content.stream(STREAM).unwrap_or_default().to_vec())
    }

    fn render(&mut self, content: OleContent) -> Result<(Preview, Option<(i64, i64)>), String> {
        let (preview, size) = preview(&text_of(&content));
        Ok((preview, Some(size)))
    }
}

fn created(text: &str) -> Created {
    let (preview, size) = preview(text);
    Created { content: content(text, None), preview, size_emu: Some(size) }
}

/// The window, with every object ibisOffice opens in it, until the last is closed or the window is.
fn interactive(editor: Editor) {
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(560.), px(300.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("Mock editor".into()), ..Default::default() }),
            ..Default::default()
        };
        cx.open_window(options, move |window, cx| {
            let view = cx.new(|cx| MockWindow::new(editor, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("a window");
    });
}

/// An object open in the window.
struct Doc {
    id: ObjectId,
    title: String,
    opened: OleContent,
    field: Entity<TextareaState>,
    sent: usize,
    _subscription: Subscription,
}

struct MockWindow {
    /// The objects' connections (closing the window tells ibisOffice, as it drops).
    editor: Editor,
    docs: Vec<Doc>,
    selected: usize,
}

impl MockWindow {
    fn new(editor: Editor, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The objects' news, taken in on the window's thread.
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                if this.update_in(cx, |this, window, cx| this.take_news(window, cx)).is_err() {
                    return;
                }
            }
        })
        .detach();
        Self { editor, docs: Vec::new(), selected: 0 }
    }

    fn take_news(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(event) = self.editor.try_next() {
            match event {
                EditorEvent::Opened { object, content, title, .. } => {
                    let field = cx.new(|cx| TextareaState::new(window, cx).placeholder("テキスト / Text").default_value(text_of(&content)));
                    let subscription = cx.subscribe_in(&field, window, move |this, _, event: &InputEvent, _, cx| {
                        if let InputEvent::Change = event {
                            this.send(object, cx);
                        }
                    });
                    self.docs.push(Doc { id: object, title, opened: content, field, sent: 0, _subscription: subscription });
                    self.select(self.docs.len() - 1, window, cx);
                    window.activate_window();
                }
                EditorEvent::Closed { object } => {
                    self.docs.retain(|d| d.id != object);
                    self.selected = self.selected.min(self.docs.len().saturating_sub(1));
                    // ibisOffice closed the last one: nothing left to edit.
                    if self.docs.is_empty() {
                        cx.quit();
                    }
                    cx.notify();
                }
            }
        }
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = index;
        if let Some(doc) = self.docs.get(index) {
            doc.field.update(cx, |f, cx| f.focus(window, cx));
        }
        cx.notify();
    }

    /// The text of `object` now, as the object ibisOffice shows.
    fn send(&mut self, object: ObjectId, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.iter_mut().find(|d| d.id == object) else { return };
        let text = doc.field.read(cx).value().to_string();
        let (preview, size) = preview(&text);
        if self.editor.update(object, content(&text, Some(&doc.opened)), preview, Some(size)).is_ok() {
            doc.sent += 1;
        }
        cx.notify();
    }
}

/// How an object is listed: its number and first line.
fn label(doc: &Doc, cx: &App) -> String {
    let text = doc.field.read(cx).value().to_string();
    let first = text.lines().next().unwrap_or("").chars().take(18).collect::<String>();
    format!("#{} {}", doc.id.0 + 1, if first.is_empty() { "(空)".into() } else { first })
}

impl Render for MockWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (accent, border, sidebar, muted, background, foreground) = {
            let t = cx.theme();
            (t.accent, t.border, t.sidebar, t.muted_foreground, t.background, t.foreground)
        };
        let list = v_flex().w(px(170.)).h_full().p_2().gap_1().bg(sidebar).border_r_1().border_color(border).children(self.docs.iter().enumerate().map(|(i, doc)| {
            div()
                .id(("object", doc.id.0))
                .px_2()
                .py_1()
                .rounded_md()
                .when(i == self.selected, |d| d.bg(accent))
                .child(label(doc, cx))
                .on_click(cx.listener(move |this, _, window, cx| this.select(i, window, cx)))
        }));
        let detail = match self.docs.get(self.selected) {
            Some(doc) => {
                let status = if doc.sent == 0 { "入力すると ibisOffice の図が更新されます / Typing updates the picture in ibisOffice".to_string() } else { format!("ibisOffice に送信: {} 回 / Sent {} times", doc.sent, doc.sent) };
                v_flex()
                    .flex_1()
                    .p_4()
                    .gap_2()
                    .child(div().text_lg().child(format!("#{} — {}", doc.id.0 + 1, doc.title)))
                    .child(Textarea::new(&doc.field).h(px(150.)))
                    .child(div().text_sm().text_color(muted).child(status))
            }
            None => v_flex().flex_1().p_4().child(div().text_color(muted).child("オブジェクトを待っています / Waiting for an object")),
        };
        h_flex().size_full().items_start().bg(background).text_color(foreground).child(list).child(detail)
    }
}

#[cfg(test)]
mod tests {
    use super::{PROG_ID, content, text_of};
    use ibis_embed_protocol::{Clsid, OleContent, OleStream};

    #[test]
    fn an_opened_objects_class_and_other_streams_stay() {
        let opened = OleContent {
            clsid: Clsid([7; 16]),
            prog_id: "Other.App.2".into(),
            user_type: "Other".into(),
            streams: vec![OleStream { path: "CONTENTS".into(), bytes: b"old".to_vec() }, OleStream { path: "Sub/Data".into(), bytes: vec![1, 2] }],
        };
        let edited = content("新しい", Some(&opened));
        assert_eq!((edited.clsid, edited.prog_id.as_str(), edited.user_type.as_str()), (Clsid([7; 16]), "Other.App.2", "Other"));
        assert_eq!(edited.stream("Sub/Data"), Some(&[1, 2][..]));
        assert_eq!(text_of(&edited), "新しい");
        assert_eq!(content("x", None).prog_id, PROG_ID);
    }
}
