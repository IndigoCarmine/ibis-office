// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スライド マスター表示: a master or a layout made a slide of a file of its
//! own, edited, and its shapes put back.

use ibis_doc_model::{Command, Document, EmuRect};
use ibis_pptx::{master_page, SaveOptions};

#[path = "../../../test/fixtures.rs"]
mod fixtures;

/// The file `name` under `test/`, or `None` (reported as skipped) when the
/// private test data is not there.
fn file(name: &str) -> Option<Vec<u8>> {
    fixtures::read(&format!("test/{name}"))
}

#[test]
fn the_pages_are_the_master_and_its_layouts() {
    let Some(data) = file("editing/pptx-plan.pptx") else { return };
    let pages = master_page::pages(&data).unwrap();
    assert!(pages[0].master, "{pages:?}");
    assert!(pages.len() > 1 && pages[1..].iter().all(|p| !p.master && p.of == pages[0].part), "{pages:?}");
    assert!(pages[1..].iter().any(|p| !p.name.is_empty()));
}

#[test]
fn a_layout_is_a_slide_and_comes_back() {
    let Some(data) = file("editing/pptx-plan.pptx") else { return };
    let original = ibis_pptx::open(data.clone()).unwrap();
    let pages = master_page::pages(&data).unwrap();
    for page in pages.iter() {
        let temp = master_page::build(&data, &page.part).unwrap_or_else(|e| panic!("{}: {e}", page.part));
        let opened = ibis_pptx::open(temp.clone()).unwrap_or_else(|e| panic!("{}: {e}", page.part));
        assert_eq!(opened.presentation.slides.len(), 1, "{}", page.part);
        // Nothing edited: back as it was (the same meaning).
        let edited = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
        let back = master_page::apply(&data, &edited, &page.part).unwrap_or_else(|e| panic!("{}: {e}", page.part));
        let again = ibis_pptx::open(back.clone()).unwrap_or_else(|e| panic!("{}: {e}", page.part));
        assert_eq!(again.presentation.layouts.len(), original.presentation.layouts.len(), "{}", page.part);
        assert_eq!(again.presentation.slides.len(), original.presentation.slides.len());
        for (a, b) in original.presentation.layouts.iter().zip(&again.presentation.layouts) {
            assert_eq!(a.backdrop.len(), b.backdrop.len(), "{}: {}", page.part, a.name);
            assert_eq!(a.placeholders.len(), b.placeholders.len(), "{}: {}", page.part, a.name);
        }
    }
}

#[test]
fn a_shape_moved_on_the_page_moves_in_the_layout() {
    let Some(data) = file("editing/pptx-plan.pptx") else { return };
    let pages = master_page::pages(&data).unwrap();
    let layout = pages.iter().skip(1).find(|p| p.name.contains("タイトル")).unwrap_or(&pages[1]).clone();
    let before = ibis_pptx::open(data.clone()).unwrap();
    let temp = master_page::build(&data, &layout.part).unwrap();
    let opened = ibis_pptx::open(temp).unwrap();
    let slide = opened.presentation.slides[0].id;
    let first = opened.presentation.slides[0].shapes[0].clone();
    assert!(!opened.presentation.slides[0].shapes.is_empty(), "the layout's shapes are the slide's");
    let mut document = Document::new(opened.presentation.clone());
    let moved = EmuRect::new(first.frame.x + 123_456, first.frame.y + 65_432, first.frame.width, first.frame.height);
    document.execute(vec![Command::SetFrame { slide, shape: first.id, frame: moved }]).unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let back = master_page::apply(&data, &edited, &layout.part).unwrap();
    let after = ibis_pptx::open(back).unwrap();
    // The layout's placeholder is where it was put (the same layout, by its name).
    let find = |p: &ibis_doc_model::Presentation| p.layouts.iter().find(|l| l.name == layout.name).unwrap().placeholders.iter().filter_map(|ph| ph.placement.map(|pl| pl.0)).collect::<Vec<_>>();
    let (a, b) = (find(&before.presentation), find(&after.presentation));
    assert_eq!(a.len(), b.len());
    assert!(a != b, "a placeholder of the layout moved");
    assert!(b.contains(&moved) || b.iter().any(|r| r.x == moved.x && r.y == moved.y), "{moved:?} in {b:?}");
}

#[test]
fn every_page_of_a_real_deck_comes_back() {
    let Some(data) = file("benchmark/earnings-presentation.pptx") else { return };
    let original = ibis_pptx::open(data.clone()).unwrap();
    let pages = master_page::pages(&data).unwrap();
    assert!(pages.len() > 2);
    for page in &pages {
        let temp = master_page::build(&data, &page.part).unwrap_or_else(|e| panic!("{}: {e}", page.part));
        let opened = ibis_pptx::open(temp).unwrap_or_else(|e| panic!("{}: {e}", page.part));
        let edited = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
        let back = master_page::apply(&data, &edited, &page.part).unwrap_or_else(|e| panic!("{}: {e}", page.part));
        let again = ibis_pptx::open(back).unwrap_or_else(|e| panic!("{}: {e}", page.part));
        assert_eq!(again.presentation.slides.len(), original.presentation.slides.len(), "{}", page.part);
        for (a, b) in original.presentation.layouts.iter().zip(&again.presentation.layouts) {
            assert_eq!((a.backdrop.len(), a.placeholders.len()), (b.backdrop.len(), b.placeholders.len()), "{}: {}", page.part, a.name);
        }
    }
}

#[test]
fn a_picture_put_on_a_layout_comes_with_its_file() {
    use ibis_doc_model::{MediaFormat, Picture, Shape, ShapeKind};
    let Some(data) = file("editing/pptx-plan.pptx") else { return };
    let pages = master_page::pages(&data).unwrap();
    let layout = pages[1].clone();
    let temp = master_page::build(&data, &layout.part).unwrap();
    let opened = ibis_pptx::open(temp).unwrap();
    let slide = opened.presentation.slides[0].id;
    let mut document = Document::new(opened.presentation.clone());
    let (poster, _, _) = ibis_pptx::clip_poster(false, 16.0 / 9.0);
    let media = document.ids().add_media(MediaFormat::Png, poster.clone());
    let id = document.ids().new_shape_id();
    let picture = Picture { media, crop: Default::default(), stroke: None, corner: 0, duotone: None, clear: None, luminance: None, clip: None, shadow: None, reflection: None, soft_edge: None, alt_text: Default::default(), decorative: false, name: "ロゴ".into() };
    let index = opened.presentation.slides[0].shapes.len();
    document.execute(vec![Command::InsertShape { slide, index, shape: Shape::new(id, EmuRect::new(100_000, 100_000, 800_000, 450_000), ShapeKind::Picture(picture)) }]).unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let back = master_page::apply(&data, &edited, &layout.part).unwrap();
    let after = ibis_pptx::open(back.clone()).unwrap();
    let layout_after = after.presentation.layouts.iter().find(|l| l.name == layout.name).unwrap();
    let logo = layout_after.backdrop.iter().find_map(|s| match &s.kind {
        ShapeKind::Picture(p) => Some(p.clone()),
        _ => None,
    });
    let logo = logo.expect("the picture is on the layout");
    assert_eq!(after.presentation.media(logo.media).unwrap().data.to_vec(), poster, "the picture's file came with it");
    // And on the slides that follow the layout (their backdrop).
    assert!(after.presentation.slides.iter().any(|s| s.layout == Some(layout_after.id) && s.backdrop.iter().any(|sh| matches!(&sh.kind, ShapeKind::Picture(_)))), "shown on a slide of that layout");
}

/// What the edited page does not own is left as it was, byte for byte: the other
/// parts of the file (the theme, the master or the other layouts, the slides, the
/// presentation) come back identical.
#[test]
fn the_other_parts_are_left_as_they_were() {
    for name in ["editing/pptx-plan.pptx", "benchmark/earnings-presentation.pptx"] {
        let Some(data) = file(name) else { continue };
        let before = ibis_ooxml::Package::open(data.clone()).unwrap();
        for page in master_page::pages(&data).unwrap() {
            let temp = master_page::build(&data, &page.part).unwrap();
            let opened = ibis_pptx::open(temp).unwrap();
            let edited = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
            let back = master_page::apply(&data, &edited, &page.part).unwrap();
            let after = ibis_ooxml::Package::open(back).unwrap();
            let own = page.part.clone();
            let own_rels = format!("{}/_rels/{}.rels", own.rsplit_once('/').unwrap().0, own.rsplit_once('/').unwrap().1);
            for part in before.names() {
                if part == own || part == own_rels {
                    continue;
                }
                assert_eq!(before.read(part).unwrap(), after.read(part).unwrap(), "{name}: {part} changed (page {})", page.part);
            }
            assert!(before.names().all(|n| after.contains(n)), "{name}: a part is gone");
        }
    }
}

#[test]
fn the_text_styles_of_a_master_are_read_and_changed() {
    use master_page::{LevelStyle, StyleKind};
    let Some(data) = file("editing/pptx-plan.pptx") else { return };
    let master = master_page::pages(&data).unwrap()[0].part.clone();
    let styles = master_page::text_styles(&data, &master).unwrap();
    let title = styles.iter().find(|s| s.0 == StyleKind::Title && s.1 == 1).unwrap();
    assert_eq!(title.2.size, Some(4400));
    let body2 = styles.iter().find(|s| s.0 == StyleKind::Body && s.1 == 2).unwrap();
    assert_eq!(body2.2.size, Some(2400));
    // The title bigger, bold, red and in another face; the body's first level smaller.
    let changed = master_page::set_text_style(&data, &master, StyleKind::Title, 1, &LevelStyle { size: Some(3600), bold: Some(true), color: Some(0xC00000), font: Some("Meiryo UI".into()) }).unwrap();
    let changed = master_page::set_text_style(&changed, &master, StyleKind::Body, 1, &LevelStyle { size: Some(2000), ..Default::default() }).unwrap();
    let styles = master_page::text_styles(&changed, &master).unwrap();
    let title = styles.iter().find(|s| s.0 == StyleKind::Title && s.1 == 1).unwrap();
    assert_eq!(title.2, LevelStyle { size: Some(3600), bold: Some(true), color: Some(0xC00000), font: Some("Meiryo UI".into()) });
    assert_eq!(styles.iter().find(|s| s.0 == StyleKind::Body && s.1 == 1).unwrap().2.size, Some(2000));
    assert_eq!(styles.iter().find(|s| s.0 == StyleKind::Body && s.1 == 2).unwrap().2.size, Some(2400), "the other levels are as they were");
    // Every other part is as it was.
    let before = ibis_ooxml::Package::open(data.clone()).unwrap();
    let after = ibis_ooxml::Package::open(changed.clone()).unwrap();
    for part in before.names() {
        if part != master {
            assert_eq!(before.read(part).unwrap(), after.read(part).unwrap(), "{part}");
        }
    }
    // The slides show it: the title of a slide that does not say otherwise.
    let opened = ibis_pptx::open(changed).unwrap();
    let size = |p: &ibis_doc_model::Presentation| p.slides[1].shapes[0].text_body().unwrap().paragraphs[0].runs[0].style.font_size;
    let original = ibis_pptx::open(data).unwrap();
    assert_ne!(size(&opened.presentation), size(&original.presentation), "the title's size changed on the slide");
}

#[test]
fn a_notes_page_is_a_slide_and_comes_back_with_its_formatting() {
    use ibis_doc_model::{Command, Document};
    let Some(data) = file("editing/pptx-plan.pptx") else { return };
    let original = ibis_pptx::open(data.clone()).unwrap();
    let index = original.presentation.slides.iter().position(|s| !s.notes.is_empty()).unwrap();
    let temp = ibis_pptx::notes_page::build(&data, index).unwrap();
    let opened = ibis_pptx::open(temp).unwrap();
    assert_eq!(opened.presentation.slides.len(), 1);
    assert_eq!((opened.presentation.slide_size.width, opened.presentation.slide_size.height), (6_858_000, 9_144_000));
    let slide = &opened.presentation.slides[0];
    let body = slide.shapes.iter().find(|s| s.text_body().is_some_and(|b| b.text().contains("発表メモ"))).expect("the notes' text is a shape of the page");
    assert!(body.text_body().unwrap().paragraphs.iter().all(|p| p.bullet.is_none()), "notes have no bullets of the slide master's body");
    let size = body.text_body().unwrap().paragraphs[0].runs[0].style.font_size;
    assert_eq!(size.map(|s| s / 100).unwrap_or(12), 12, "the notes master's size: {size:?}");
    // Part of the text made bold and red, as on any text.
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::SetCharacterFormat { slide: slide.id, shape: body.id, start: 0, end: 3, format: ibis_doc_model::CharacterFormat { bold: Some(true), color: Some(ibis_doc_model::Rgb(0xC00000)), ..Default::default() } }]).unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let back = ibis_pptx::notes_page::apply(&data, &edited, index).unwrap();
    let after = ibis_pptx::open(back.clone()).unwrap();
    assert_eq!(after.presentation.slides[index].notes, original.presentation.slides[index].notes, "the text is the same");
    let package = ibis_ooxml::Package::open(back).unwrap();
    let xml = String::from_utf8(package.read("ppt/notesSlides/notesSlide1.xml").unwrap()).unwrap();
    assert!(xml.contains("b=\"1\"") && xml.contains("C00000"), "{xml}");
    // The rest of the file is as it was.
    let before = ibis_ooxml::Package::open(data).unwrap();
    for part in before.names() {
        if part != "ppt/notesSlides/notesSlide1.xml" {
            assert_eq!(before.read(part).unwrap(), package.read(part).unwrap(), "{part}");
        }
    }
}

#[test]
fn a_slide_without_notes_gets_a_notes_page_from_the_edited_one() {
    use ibis_doc_model::{Command, Document};
    let Some(data) = file("editing/pptx-plan.pptx") else { return };
    let original = ibis_pptx::open(data.clone()).unwrap();
    let index = original.presentation.slides.iter().position(|s| s.notes.is_empty()).unwrap();
    let temp = ibis_pptx::notes_page::build(&data, index).unwrap();
    let opened = ibis_pptx::open(temp).unwrap();
    let slide = opened.presentation.slides[0].clone();
    let body = slide.shapes.iter().find(|s| s.text_body().is_some()).expect("a body to type in");
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::SetText { slide: slide.id, shape: body.id, text: "話すこと\n二つ目".into() }]).unwrap();
    document.execute(vec![Command::SetCharacterFormat { slide: slide.id, shape: body.id, start: 0, end: 4, format: ibis_doc_model::CharacterFormat { italic: Some(true), ..Default::default() } }]).unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let back = ibis_pptx::notes_page::apply(&data, &edited, index).unwrap();
    let after = ibis_pptx::open(back.clone()).unwrap();
    assert_eq!(after.presentation.slides[index].notes, "話すこと\n二つ目");
    let package = ibis_ooxml::Package::open(back).unwrap();
    assert!(package.names().any(|n| n.starts_with("ppt/notesSlides/notesSlide") && String::from_utf8_lossy(&package.read(n).unwrap()).contains("i=\"1\"")), "italic in a new notes page");
}
