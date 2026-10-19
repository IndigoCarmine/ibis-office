// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 透明度: a picture's whole opacity (`a:alphaModFix`), written, changed and
//! taken away in presentations and documents.

use ibis_doc_model::flow::{Block, FlowRunKind};
use ibis_doc_model::{Command, Document, EmuRect, FlowCommand, FlowPos, MediaFormat, Picture, PictureLook, Presentation, Shape, ShapeKind};
use ibis_ooxml::Package;
use ibis_pptx::SaveOptions;

/// A 1×1 PNG with a clear pixel.
fn png() -> Vec<u8> {
    b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89\0\0\0\x0bIDATx\x9cc`\0\x02\0\0\x05\0\x01z^\xab?\0\0\0\0IEND\xaeB`\x82".to_vec()
}

fn part(file: &[u8], name: &str) -> String {
    let package = Package::open(file.to_vec()).unwrap();
    String::from_utf8(package.read(name).unwrap().to_vec()).unwrap()
}

fn slide_picture(p: &Presentation) -> &Picture {
    p.slides[0].shapes.iter().find_map(|s| match &s.kind {
        ShapeKind::Picture(pic) => Some(pic),
        _ => None,
    }).expect("a picture")
}

#[test]
fn slide_picture_alpha_round_trips() {
    let opened = ibis_pptx::open(ibis_pptx::new_presentation().unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let slide = document.presentation().slides[0].id;
    let media = document.ids().add_media(MediaFormat::Png, png());
    let id = document.ids().new_shape_id();
    let mut picture = Picture::new(media);
    picture.effects.opacity = Some(40_000);
    let index = document.presentation().slides[0].shapes.len();
    let shape = Shape { id, frame: EmuRect::new(0, 0, 914_400, 914_400), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Picture(picture.clone()), floating: None };
    document.execute(vec![Command::InsertShape { slide, index, shape }]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    assert!(part(&saved, "ppt/slides/slide1.xml").contains(r#"<a:alphaModFix amt="40000"/>"#));
    assert_eq!(slide_picture(&ibis_pptx::read(&saved).unwrap()).effects.opacity, Some(40_000));

    // Changed in the saved file, then taken away.
    let opened = ibis_pptx::open(saved).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let slide = document.presentation().slides[0].id;
    let shape = document.presentation().slides[0].shapes.iter().find(|s| matches!(s.kind, ShapeKind::Picture(_))).unwrap().clone();
    let ShapeKind::Picture(mut picture) = shape.kind else { unreachable!() };
    for (alpha, xml) in [(Some(75_000), Some(r#"amt="75000""#)), (None, None)] {
        picture.effects.opacity = alpha;
        document.execute(vec![Command::SetKind { slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture.clone())) }]).unwrap();
        let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
        let slide_xml = part(&saved, "ppt/slides/slide1.xml");
        assert_eq!(slide_xml.matches("alphaModFix").count(), usize::from(xml.is_some()), "{slide_xml}");
        if let Some(xml) = xml {
            assert!(slide_xml.contains(xml), "{slide_xml}");
        }
        assert_eq!(slide_picture(&ibis_pptx::read(&saved).unwrap()).effects.opacity, alpha);
    }
}

fn first_para(p: &Presentation) -> u32 {
    p.flow.as_ref().unwrap().body.iter().find_map(|b| match b { Block::Paragraph(p) => Some(p.id), _ => None }).unwrap()
}

/// The alpha of the document's first picture, inline or floating.
fn document_alpha(p: &Presentation) -> Option<i32> {
    p.flow.as_ref().unwrap().body.iter().find_map(|b| match b {
        Block::Paragraph(p) => p.runs.iter().find_map(|r| match &r.kind {
            FlowRunKind::Picture { look, .. } => Some(look.effects.opacity),
            FlowRunKind::Shape(s) => match &s.kind {
                ShapeKind::Picture(pic) => Some(pic.effects.opacity),
                _ => None,
            },
            _ => None,
        }),
        _ => None,
    }).expect("a picture")
}

#[test]
fn document_picture_alpha_round_trips() {
    let opened = ibis_pptx::open(ibis_pptx::blank_file(ibis_pptx::Kind::Document).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let at = FlowPos::new(first_para(document.presentation()), 0);
    let media = document.ids().add_media(MediaFormat::Png, png());
    document.execute(vec![Command::Flow(FlowCommand::InsertPicture { at, media, size: ibis_doc_model::EmuSize::new(914_400, 914_400) })]).unwrap();
    let save = |document: &Document| opened.source.save(document.presentation(), SaveOptions::default()).unwrap();

    let mut look = PictureLook::default();
    look.effects.opacity = Some(50_000);
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: look.clone() })]).unwrap();
    let saved = save(&document);
    assert!(part(&saved, "word/document.xml").contains(r#"<a:alphaModFix amt="50000"/>"#));
    assert_eq!(document_alpha(&ibis_pptx::read(&saved).unwrap()), Some(50_000));

    // A floating picture keeps it.
    document.execute(vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::Square })]).unwrap();
    assert_eq!(document_alpha(&ibis_pptx::read(&save(&document)).unwrap()), Some(50_000));

    // Taken away.
    document.execute(vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::Inline })]).unwrap();
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: PictureLook::default() })]).unwrap();
    let saved = save(&document);
    assert!(!part(&saved, "word/document.xml").contains("alphaModFix"));
    assert_eq!(document_alpha(&ibis_pptx::read(&saved).unwrap()), None);
}
