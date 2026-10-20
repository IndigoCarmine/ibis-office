// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What a Word document is read as for laying it out as Word does: text
//! boxes' runs, the line grid, and tracked changes inside each other. Each
//! document is made here from the blank one.

use ibis_doc_model::flow::{Block, FlowDocument, FlowParagraph, FlowRunKind};
use ibis_doc_model::{DocGridType, RevisionKind, Shape, ShapeKind};
use ibis_ooxml::Package;

const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
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

/// A floating 4 000 000 × 3 000 000 EMU text box whose paragraphs are `content`.
fn text_box(content: &str) -> String {
    format!(
        r#"<w:p><w:r><w:drawing><wp:anchor xmlns:wp="{WP}" distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="10" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="4000000" cy="3000000"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="Text Box 1"/><a:graphic xmlns:a="{A}"><a:graphicData uri="{WPS}"><wps:wsp xmlns:wps="{WPS}"><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="4000000" cy="3000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent>{content}</w:txbxContent></wps:txbx><wps:bodyPr lIns="91440" tIns="45720" rIns="91440" bIns="45720"/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#
    )
}

#[test]
fn a_text_box_reads_runs_in_hyperlinks_tabs_and_empty_paragraphs() {
    let flow = document(
        |_| {
            text_box(
                r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:tab/></w:r><w:hyperlink w:anchor="x"><w:r><w:t>link</w:t></w:r></w:hyperlink><w:del w:id="1" w:author="A" w:date="2020-01-01T00:00:00Z"><w:r><w:tab/></w:r></w:del></w:p><w:p/><w:p><w:r><w:t>last</w:t></w:r></w:p>"#,
            )
        },
        None,
    );
    let shape = shapes(&flow).into_iter().next().expect("the text box");
    let ShapeKind::Auto(boxed) = &shape.kind else { panic!("a shape with text: {:?}", shape.kind) };
    let text = boxed.text.as_ref().expect("its text");
    let lines: Vec<String> = text.paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect()).collect();
    assert_eq!(lines, vec!["a\tlink".to_string(), String::new(), "last".to_string()], "a deleted tab is left out, the empty paragraph kept");
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
