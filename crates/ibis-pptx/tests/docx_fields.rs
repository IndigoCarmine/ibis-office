// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Fields the layout computes, read from the file: simple and complex
//! fields with format switches, a formula over a nested field, and the
//! result of a simple field in the paragraph's formatting (Word writes it
//! again so) unless `\* MERGEFORMAT`.

#[path = "../../../test/fixtures.rs"]
mod fixtures;

use ibis_doc_model::{Block, FlowParagraph, FlowRunKind};

/// The last paragraph of the report with `runs` (the XML of a `w:p`'s
/// content) added at the end of its body, or `None` (reported as skipped)
/// when the private test data is not there.
fn read_paragraph(runs: &str) -> Option<FlowParagraph> {
    let mut package = ibis_ooxml::Package::open(fixtures::read("test/editing/docx-report.docx")?).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.rfind("<w:sectPr").unwrap();
    package.set_bytes("word/document.xml", format!("{}<w:p>{runs}</w:p>{}", &xml[..at], &xml[at..]).into_bytes());
    let data = package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    let p = ibis_pptx::read(&data).unwrap();
    Some(p.flow.as_deref().unwrap().body.iter().rev().find_map(|b| if let Block::Paragraph(p) = b { Some(p.clone()) } else { None }).unwrap())
}

fn fields(p: &FlowParagraph) -> Vec<(String, String)> {
    p.runs.iter().filter_map(|r| if let FlowRunKind::Field { instruction, shown } = &r.kind { Some((instruction.clone(), shown.clone())) } else { None }).collect()
}

#[test]
fn simple_fields_with_switches() {
    let Some(p) = read_paragraph(r#"<w:fldSimple w:instr=" PAGE \* ROMAN "><w:r><w:rPr><w:sz w:val="32"/></w:rPr><w:t>1</w:t></w:r></w:fldSimple><w:fldSimple w:instr=" NUMPAGES "><w:r><w:t>99</w:t></w:r></w:fldSimple><w:fldSimple w:instr=" SECTION "><w:r><w:t>1</w:t></w:r></w:fldSimple>"#) else { return };
    assert_eq!(fields(&p), vec![("PAGE \\* ROMAN".to_string(), "1".to_string()), ("NUMPAGES".into(), "99".into()), ("SECTION".into(), "1".into())]);
    assert_eq!(p.runs[0].format.size, None, "the result's size is not kept without MERGEFORMAT");
    // Plain PAGE stays a page number; with MERGEFORMAT its result's size.
    let Some(p) = read_paragraph(r#"<w:fldSimple w:instr=" PAGE \* MERGEFORMAT "><w:r><w:rPr><w:sz w:val="32"/></w:rPr><w:t>1</w:t></w:r></w:fldSimple>"#) else { return };
    assert!(matches!(p.runs[0].kind, FlowRunKind::PageNumber));
    assert!(p.runs[0].format.size.is_some());
}

#[test]
fn a_formula_over_a_nested_field() {
    let r = |inner: &str| format!("<w:r>{inner}</w:r>");
    let runs = [
        r(r#"<w:fldChar w:fldCharType="begin"/>"#),
        r(r#"<w:instrText xml:space="preserve"> = </w:instrText>"#),
        r(r#"<w:fldChar w:fldCharType="begin"/>"#),
        r(r#"<w:instrText xml:space="preserve"> PAGE </w:instrText>"#),
        r(r#"<w:fldChar w:fldCharType="separate"/>"#),
        r("<w:t>1</w:t>"),
        r(r#"<w:fldChar w:fldCharType="end"/>"#),
        r(r#"<w:instrText xml:space="preserve"> + 10 </w:instrText>"#),
        r(r#"<w:fldChar w:fldCharType="separate"/>"#),
        r("<w:t>11</w:t>"),
        r(r#"<w:fldChar w:fldCharType="end"/>"#),
    ]
    .concat();
    let Some(p) = read_paragraph(&runs) else { return };
    assert_eq!(fields(&p), vec![("= { PAGE } + 10".to_string(), "11".to_string())]);
    assert_eq!(p.text().chars().count(), 1, "the nested field's result is not shown: {:?}", p.runs);
}
