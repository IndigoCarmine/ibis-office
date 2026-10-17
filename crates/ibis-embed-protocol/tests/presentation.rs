// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! End to end, as ibisOffice does it: an embedded object of a presentation
//! given to the `ibis-embed-mock` editor over a local socket, its saved
//! content and preview put back as a new object, the file saved and read.
#![cfg(feature = "host")]

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use ibis_doc_model::{Command as Edit, Document, EmuRect, MediaFormat, Picture, Shape, ShapeKind};
use ibis_embed_protocol::{HandlerMessage, HostMessage, HostSession, Mode, host};
use ibis_ooxml::cfb::Item;
use ibis_ooxml::ole::{OleStorage, clsid_from_str};

const PROG_ID: &str = "ChemDraw.Document.6.0";

fn mock_exe() -> PathBuf {
    let profile_dir = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_owned();
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.args(["build", "--quiet", "-p", "ibis-embed-mock"]).current_dir(env!("CARGO_MANIFEST_DIR"));
    if profile_dir.ends_with("release") {
        cmd.arg("--release");
    }
    assert!(cmd.status().expect("run cargo").success(), "building ibis-embed-mock failed");
    profile_dir.join(format!("ibis-embed-mock{}", std::env::consts::EXE_SUFFIX))
}

#[test]
fn an_object_edited_in_the_mock_is_saved() {
    // A presentation with an object (as ChemDraw's: a class, CONTENTS, and a stream of the container's).
    let opened = ibis_pptx::open(ibis_pptx::new_presentation().unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let slide = document.presentation().slides[0].id;
    let mut storage = OleStorage::new(clsid_from_str("{41BA6D21-A02E-11CE-8FD9-0020AFD1F20C}").unwrap(), PROG_ID, "ChemDraw Document", vec![Item::stream("CONTENTS", b"mock".to_vec())]);
    storage.items.push(Item::stream("\u{3}ObjInfo", vec![0, 0, 3, 0, 4, 0]));
    let media = document.ids().add_media(MediaFormat::Png, host::preview_png(&ibis_embed_protocol::Preview::svg(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#), None).unwrap());
    let ole = document.ids().add_ole(PROG_ID, storage.to_bytes(), false);
    let shape = document.ids().new_shape_id();
    let mut picture = Picture::new(media);
    picture.ole = Some(ole);
    document
        .execute(vec![Edit::InsertShape { slide, index: 0, shape: Shape { id: shape, frame: EmuRect::new(0, 0, 1_828_800, 914_400), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Picture(picture), floating: None } }])
        .unwrap();
    let file = opened.source.save(document.presentation(), Default::default()).unwrap();

    // Read back, and given to the editor.
    let opened = ibis_pptx::open(file).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let shape = document.presentation().slides[0].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.ole.is_some())).unwrap().clone();
    let ShapeKind::Picture(mut picture) = shape.kind.clone() else { unreachable!() };
    let object = document.presentation().ole(picture.ole.unwrap()).unwrap().clone();
    let content = host::content_of(&object.data).unwrap();
    assert_eq!(content.prog_id, PROG_ID);
    let mut session = HostSession::spawn(mock_exe(), &[] as &[&str], Mode::Interactive, Duration::from_secs(20)).unwrap();
    session.send(&HostMessage::Open { content, title: "test".into(), locale: "ja".into() }).unwrap();
    let HandlerMessage::Update { content, preview, size_emu } = session.recv().unwrap() else { panic!("no update") };
    session.send(&HostMessage::Close).unwrap();

    // As the app applies an update.
    let data = host::object_of(&content, Some(&object.data)).unwrap();
    let png = host::preview_png(&preview, size_emu).unwrap();
    picture.media = document.ids().add_media(MediaFormat::Png, png);
    picture.ole = Some(document.ids().add_ole(content.prog_id.clone(), data, false));
    document.execute(vec![Edit::SetKind { slide: shape_slide(&document), shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) }]).unwrap();
    let saved = opened.source.save(document.presentation(), Default::default()).unwrap();

    let read = ibis_pptx::read(&saved).unwrap();
    let pic = read.slides[0].shapes.iter().find_map(|s| match &s.kind {
        ShapeKind::Picture(p) if p.ole.is_some() => Some(p),
        _ => None,
    }).unwrap();
    let after = OleStorage::parse(&read.ole(pic.ole.unwrap()).unwrap().data).unwrap();
    assert_eq!(after.prog_id.as_deref(), Some(PROG_ID));
    assert_eq!(after.clsid, clsid_from_str("{41BA6D21-A02E-11CE-8FD9-0020AFD1F20C}").unwrap());
    assert_eq!(after.items.iter().find(|i| i.path == "CONTENTS").and_then(|i| i.data.as_deref()), Some(&b"mock+edited"[..]));
    assert!(after.items.iter().any(|i| i.path == "\u{3}ObjInfo"), "the container's stream stays");
    assert!(read.media(pic.media).unwrap().data.starts_with(b"\x89PNG"));
}

fn shape_slide(document: &Document) -> ibis_doc_model::SlideId {
    document.presentation().slides[0].id
}
