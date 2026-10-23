// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表紙 (DOCX-PAGE-022): the paragraphs of a cover page put at the top of a document, ending with a page break.

use crate::drawing::Rgb;
use crate::flow::{Block, FlowAlign, FlowDocument, FlowParagraph, FlowRun, FlowRunKind, FlowRunStyle, LineHeight, RunFormat};

/// The covers on offer.
pub const COVER_DESIGNS: usize = 3;

/// A cover paragraph: its text, the size (points), bold, colour, alignment, and the space above (points).
fn line(id: u32, text: &str, size: u32, bold: bool, color: Rgb, align: FlowAlign, before: i64, band: Option<Rgb>) -> FlowParagraph {
    let mut p = FlowParagraph { id, ..FlowParagraph::default() };
    p.direct.align = Some(align);
    p.direct.before = Some(before * crate::EMU_PER_POINT);
    p.direct.after = Some(6 * crate::EMU_PER_POINT);
    p.direct.line = Some(LineHeight::Auto(240));
    if let Some(fill) = band {
        p.direct.shading = Some(Some(fill));
    }
    if !text.is_empty() {
        let mut run = FlowRun::new(FlowRunKind::Text(text.to_string()), FlowRunStyle::default());
        run.format = RunFormat { size: Some(size * 100), bold: Some(bold), color: Some(color), ..RunFormat::default() };
        p.runs.push(run);
    }
    p
}

/// The paragraphs of cover `design` (0 to [`COVER_DESIGNS`] - 1) for `doc`, with ids above the document's. `title`, `author`, `date`
/// are the texts of the placeholders (the title is the first heading's text when the document has one).
pub fn cover_blocks(doc: &FlowDocument, design: usize, title: &str, author: &str, date: &str) -> Vec<Block> {
    let first = doc.new_id();
    let mut ids = first..;
    let mut next = || ids.next().unwrap_or(first);
    let dark = Rgb(0x2F528F);
    let white = Rgb(0xFFFFFF);
    let grey = Rgb(0x595959);
    let mut out: Vec<FlowParagraph> = Vec::new();
    match design {
        // 1: centred, plain.
        0 => {
            out.push(line(next(), title, 36, true, Rgb(0x000000), FlowAlign::Center, 150, None));
            out.push(line(next(), "サブタイトル", 18, false, grey, FlowAlign::Center, 12, None));
            out.push(line(next(), author, 14, false, Rgb(0x000000), FlowAlign::Center, 200, None));
            out.push(line(next(), date, 14, false, grey, FlowAlign::Center, 0, None));
        }
        // 2: a band behind the title.
        1 => {
            out.push(line(next(), "", 12, false, white, FlowAlign::Left, 120, None));
            out.push(line(next(), title, 40, true, white, FlowAlign::Left, 12, Some(dark)));
            out.push(line(next(), "サブタイトル", 20, false, white, FlowAlign::Left, 0, Some(dark)));
            out.push(line(next(), author, 14, false, Rgb(0x000000), FlowAlign::Left, 160, None));
            out.push(line(next(), date, 14, false, grey, FlowAlign::Left, 0, None));
        }
        // 3: large, left, in the dark blue.
        _ => {
            out.push(line(next(), title, 48, true, dark, FlowAlign::Left, 180, None));
            out.push(line(next(), "サブタイトル", 22, false, grey, FlowAlign::Left, 6, None));
            out.push(line(next(), author, 14, true, dark, FlowAlign::Left, 220, None));
            out.push(line(next(), date, 14, false, grey, FlowAlign::Left, 0, None));
        }
    }
    // A page break ends the cover.
    let mut end = FlowParagraph { id: next(), ..FlowParagraph::default() };
    end.runs.push(FlowRun::new(FlowRunKind::PageBreak, FlowRunStyle::default()));
    out.push(end);
    out.into_iter()
        .map(|mut p| {
            p.resolve(&doc.styles);
            p.normalize();
            Block::Paragraph(p)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cover_ends_with_a_page_break_and_has_its_own_ids() {
        let doc = FlowDocument { body: vec![Block::Paragraph(FlowParagraph { id: 7, ..FlowParagraph::default() })], ..FlowDocument::default() };
        for design in 0..COVER_DESIGNS {
            let blocks = cover_blocks(&doc, design, "題名", "作成者", "2026-10-14");
            let ids: std::collections::HashSet<u32> = blocks.iter().map(|b| if let Block::Paragraph(p) = b { p.id } else { 0 }).collect();
            assert_eq!(ids.len(), blocks.len(), "distinct ids");
            assert!(ids.iter().all(|&i| i > 7));
            let Some(Block::Paragraph(last)) = blocks.last() else { panic!() };
            assert!(last.runs.iter().any(|r| matches!(r.kind, FlowRunKind::PageBreak)));
            let text: String = blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect();
            assert!(text.contains("題名") && text.contains("作成者"));
        }
    }
}
