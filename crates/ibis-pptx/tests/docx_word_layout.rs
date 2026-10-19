// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What a Word document is read as for laying it out as Word does: pictures
//! in text boxes, picture watermarks, the line grid, `w:hint`, and tracked
//! changes inside each other. Each document is made here from the blank one.

use ibis_doc_model::flow::{Block, FlowDocument, FlowParagraph, FlowRunKind};
use ibis_doc_model::{DocGridType, LineSpacing, RevisionKind, Shape, ShapeKind};
use ibis_ooxml::Package;

const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

/// A 1×1 PNG.
fn png() -> Vec<u8> {
    b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89\0\0\0\x0bIDATx\x9cc`\0\x02\0\0\x05\0\x01z^\xab?\0\0\0\0IEND\xaeB`\x82".to_vec()
}

/// The blank document with a picture part, the body `body(rId)` before its
/// section, and its section's `w:docGrid` replaced by `grid` when given.
fn document(body: impl Fn(&str) -> String, grid: Option<&str>) -> FlowDocument {
    let mut package = Package::open(ibis_pptx::blank_file(ibis_pptx::Kind::Document).unwrap()).unwrap();
    package.set_bytes("word/media/image1.png", png());
    let rid = package.add_relationship("word/document.xml", IMAGE, "media/image1.png", false).unwrap();
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.rfind("<w:sectPr").unwrap();
    xml.insert_str(at, &body(&rid));
    if let Some(grid) = grid {
        let start = xml.find("<w:docGrid").unwrap();
        let end = start + xml[start..].find("/>").unwrap() + 2;
        xml.replace_range(start..end, grid);
    }
    package.set_bytes("word/document.xml", xml.into_bytes());
    let data = package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    let flow = ibis_pptx::read(&data).unwrap().flow.unwrap();
    (*flow).clone()
}

fn paragraphs(flow: &FlowDocument) -> impl Iterator<Item = &FlowParagraph> {
    flow.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None })
}

fn shapes(flow: &FlowDocument) -> Vec<&Shape> {
    paragraphs(flow).flat_map(|p| &p.runs).filter_map(|r| if let FlowRunKind::Shape(s) = &r.kind { Some(&**s) } else { None }).collect()
}

/// An inline 2 000 000 × 1 000 000 EMU picture of `rid`.
fn inline_picture(rid: &str) -> String {
    format!(
        r#"<w:drawing><wp:inline xmlns:wp="{WP}" distT="0" distB="0" distL="0" distR="0"><wp:extent cx="2000000" cy="1000000"/><wp:docPr id="2" name="Picture 2"/><a:graphic xmlns:a="{A}"><a:graphicData uri="{PIC}"><pic:pic xmlns:pic="{PIC}"><pic:nvPicPr><pic:cNvPr id="2" name="p"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"#
    )
}

/// A floating 4 000 000 × 3 000 000 EMU text box whose paragraphs are `content`.
fn text_box(content: &str) -> String {
    format!(
        r#"<w:p><w:r><w:drawing><wp:anchor xmlns:wp="{WP}" distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="10" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="4000000" cy="3000000"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="Text Box 1"/><a:graphic xmlns:a="{A}"><a:graphicData uri="{WPS}"><wps:wsp xmlns:wps="{WPS}"><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="4000000" cy="3000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent>{content}</w:txbxContent></wps:txbx><wps:bodyPr lIns="91440" tIns="45720" rIns="91440" bIns="45720"/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#
    )
}

#[test]
fn a_picture_in_a_text_box_is_over_the_box_and_its_caption_kept() {
    let flow = document(|rid| text_box(&format!(r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r>{}</w:r></w:p><w:p><w:r><w:t>Figure 1. Caption</w:t></w:r></w:p>"#, inline_picture(rid))), None);
    let shapes = shapes(&flow);
    // The box (not taken for its picture) and the picture over it.
    let ShapeKind::Auto(boxed) = &shapes[0].kind else { panic!("the box is a shape with text: {:?}", shapes[0].kind) };
    let text = boxed.text.as_ref().expect("its text");
    assert!(text.paragraphs.iter().any(|p| p.runs.iter().any(|r| r.text.contains("Caption"))), "the caption stays");
    // The picture's paragraph keeps a line of its height (1 000 000 EMU).
    assert_eq!(text.paragraphs[0].line_spacing, Some(LineSpacing::Points(1_000_000 / 127)));
    let picture = shapes.get(1).expect("the picture over the box");
    assert!(matches!(picture.kind, ShapeKind::Picture(_)));
    let anchor = picture.floating.as_ref().unwrap();
    // Centred in the box's text room (left inset 91 440), at its top inset.
    assert_eq!((anchor.offset_x, anchor.offset_y), (91_440 + (4_000_000 - 2 * 91_440 - 2_000_000) / 2, 45_720));
    assert_eq!(anchor.wrap, "none");
    assert!(anchor.relative_height > shapes[0].floating.as_ref().unwrap().relative_height, "over the box");
    // Placed with the box wherever it lands, in the box's first paragraph.
    let over = anchor.over.expect("over the box");
    assert_eq!((over.shape, over.dx, over.dy, over.paragraph, over.down), (shapes[0].id.0, anchor.offset_x, 45_720, 0, 0));
}

#[test]
fn a_deleted_picture_in_a_text_box_is_left_out() {
    let flow = document(
        |rid| text_box(&format!(r#"<w:p><w:del w:id="1" w:author="A" w:date="2020-01-01T00:00:00Z"><w:r>{}</w:r></w:del></w:p><w:p><w:r><w:t>Caption</w:t></w:r></w:p>"#, inline_picture(rid))),
        None,
    );
    let shapes = shapes(&flow);
    assert_eq!(shapes.len(), 1, "only the box");
    let ShapeKind::Auto(boxed) = &shapes[0].kind else { panic!() };
    assert_eq!(boxed.text.as_ref().unwrap().paragraphs[0].line_spacing, None, "no room kept for it");
}

#[test]
fn a_grid_without_a_type_is_no_grid() {
    let flow = document(|_| String::new(), Some(r#"<w:docGrid w:linePitch="360"/>"#));
    assert_eq!(flow.page.doc_grid_type, DocGridType::Default);
    assert_eq!(flow.page.line_pitch, None);
    let flow = document(|_| String::new(), Some(r#"<w:docGrid w:type="lines" w:linePitch="360"/>"#));
    assert_eq!(flow.page.doc_grid_type, DocGridType::Lines);
    assert_eq!(flow.page.line_pitch, Some(360 * 635));
}

#[test]
fn shared_punctuation_takes_the_east_asian_typeface_only_with_the_hint() {
    let run = |hint: &str| format!(r#"<w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsia="MS Mincho" {hint}/></w:rPr><w:t>•–“</w:t></w:r>"#);
    let flow = document(|_| format!("<w:p>{}{}</w:p>", run(""), run(r#"w:hint="eastAsia""#)), None);
    let p = paragraphs(&flow).find(|p| p.text().contains('•')).unwrap();
    let (plain, hinted) = (&p.runs[0].style, &p.runs[1].style);
    for c in ['•', '–', '“'] {
        assert_eq!(plain.font_for(c), Some("Arial"), "{c} without the hint");
        assert_eq!(hinted.font_for(c), Some("MS Mincho"), "{c} with the hint");
    }
    // CJK takes the East Asian typeface either way, ASCII the Latin one.
    assert_eq!(plain.font_for('あ'), Some("MS Mincho"));
    assert_eq!(hinted.font_for('a'), Some("Arial"));
}

#[test]
fn latin_1_symbols_take_the_east_asian_typeface_with_the_hint_accented_letters_only_in_chinese() {
    let run = |hint: &str, lang: &str| format!(r#"<w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsia="MS Mincho" {hint}/>{lang}</w:rPr><w:t>§°±×÷éā…</w:t></w:r>"#);
    let hint = r#"w:hint="eastAsia""#;
    let zh = r#"<w:lang w:eastAsia="zh-CN"/>"#;
    let ja = r#"<w:lang w:val="ja-JP" w:eastAsia="ja-JP"/>"#;
    let flow = document(|_| format!("<w:p>{}{}{}{}</w:p>", run("", ja), run(hint, ja), run(hint, zh), run("", zh)), None);
    let p = paragraphs(&flow).find(|p| p.text().contains('§')).unwrap();
    let fonts = |k: usize| "§°±×÷éā…".chars().map(|c| p.runs[k].style.font_for(c).unwrap_or("")).collect::<Vec<_>>();
    let (m, a) = ("MS Mincho", "Arial");
    // As Word's PDFs (the language of the text, ja-JP, changes nothing).
    assert_eq!(fonts(0), [a, a, a, a, a, a, a, a], "no hint");
    assert_eq!(fonts(1), [m, m, m, m, m, a, a, m], "the hint");
    assert_eq!(fonts(2), [m, m, m, m, m, m, m, m], "the hint, Chinese");
    assert_eq!(fonts(3), [a, a, a, a, a, a, a, a], "Chinese, no hint");
}

#[test]
fn a_vml_picture_watermark_is_a_turned_washed_out_picture() {
    let flow = document(
        |rid| {
            format!(
                r##"<w:p><w:r><w:pict><v:shape xmlns:v="urn:schemas-microsoft-com:vml" id="WordPictureWatermark1" type="#_x0000_t75" style="position:absolute;margin-left:0;margin-top:0;width:200pt;height:100pt;rotation:315;z-index:-1;mso-position-horizontal:center;mso-position-horizontal-relative:margin;mso-position-vertical:center;mso-position-vertical-relative:margin"><v:imagedata r:id="{rid}" gain="19661f" blacklevel="22938f"/></v:shape></w:pict></w:r></w:p>"##
            )
        },
        None,
    );
    let shape = shapes(&flow).into_iter().next().expect("the watermark");
    let ShapeKind::Picture(picture) = &shape.kind else { panic!("a picture: {:?}", shape.kind) };
    assert_eq!(shape.rotation, 315 * 60_000);
    assert_eq!((shape.frame.width, shape.frame.height), (200 * 12_700, 100 * 12_700));
    let anchor = shape.floating.as_ref().unwrap();
    assert_eq!((anchor.align_h.as_deref(), anchor.align_v.as_deref()), (Some("center"), Some("center")));
    assert_eq!((anchor.relative_h.as_str(), anchor.relative_v.as_str()), ("margin", "margin"));
    assert!(anchor.behind_doc, "a negative z-index is behind the text");
    // gain 0.3, blacklevel 0.35: each channel 0.5 + 0.3 (v - 0.5) + 0.35 × 1.3
    // (Word's PDFs).
    let (bright, contrast) = picture.luminance.expect("washed out");
    assert!((-70_010..=-69_990).contains(&contrast), "{contrast}");
    assert!((45_490..=45_510).contains(&bright), "{bright}");
}

#[test]
fn insertions_inside_deletions_and_moves_away_are_deleted() {
    let flow = document(
        |_| {
            r#"<w:p><w:del w:id="1" w:author="A" w:date="2020-01-01T00:00:00Z"><w:ins w:id="2" w:author="B" w:date="2020-01-01T00:00:00Z"><w:r><w:t>gone</w:t></w:r></w:ins></w:del><w:moveFrom w:id="3" w:author="A" w:date="2020-01-01T00:00:00Z"><w:ins w:id="4" w:author="B" w:date="2020-01-01T00:00:00Z"><w:r><w:t>moved</w:t></w:r></w:ins></w:moveFrom><w:ins w:id="5" w:author="B" w:date="2020-01-01T00:00:00Z"><w:r><w:t>kept</w:t></w:r></w:ins></w:p>"#.to_string()
        },
        None,
    );
    let p = paragraphs(&flow).find(|p| p.text().contains("kept")).unwrap();
    let kinds: Vec<RevisionKind> = p.runs.iter().filter_map(|r| r.revision.as_ref().map(|v| v.kind.clone())).collect();
    assert_eq!(kinds, vec![RevisionKind::Delete, RevisionKind::MoveFrom, RevisionKind::Insert]);
}

#[test]
fn a_deleted_paragraph_mark_is_read() {
    let flow = document(
        |_| r#"<w:p><w:pPr><w:rPr><w:del w:id="9" w:author="A" w:date="2020-01-01T00:00:00Z"/></w:rPr></w:pPr><w:r><w:t>joined</w:t></w:r></w:p><w:p><w:r><w:t>next</w:t></w:r></w:p>"#.to_string(),
        None,
    );
    let joined = paragraphs(&flow).find(|p| p.text() == "joined").unwrap();
    assert_eq!(joined.mark_deleted.as_deref(), Some("A"));
    let next = paragraphs(&flow).find(|p| p.text() == "next").unwrap();
    assert_eq!(next.mark_deleted, None);
}
