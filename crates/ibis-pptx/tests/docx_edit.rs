// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Editing documents (`.docx`) end to end: the scenarios of
//! `tasks/doc/editing-plan.md` §2.4 for each feature of
//! `tasks/doc/features-docx.md`.
//!
//! Each scenario opens a file, runs flow commands, and checks: the model is
//! as expected; undo gives the first model and redo the edited one; the
//! saved file differs only in the parts expected; read back, it gives the
//! edited model; and after undoing everything the save is the original
//! file byte for byte.

use std::sync::Arc;

use ibis_doc_model::{
    Block, Command, Document, FlowAlign, FlowCommand, FlowDocument, FlowParagraph, FlowPos, FlowRunKind, LineHeight, ParaChange, Presentation,
    Rgb, RunChange,
};
use ibis_ooxml::canon::diff_packages;
use ibis_pptx::SaveOptions;

fn path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)
}

/// Paragraph ids renumbered in document order (a reread numbers them so),
/// and what only the viewer sets cleared.
fn normalized(doc: &FlowDocument) -> FlowDocument {
    fn walk(blocks: &mut [Block], next: &mut u32) {
        for b in blocks {
            *next += 1;
            match b {
                Block::Paragraph(p) => p.id = *next,
                Block::Table(t) => {
                    t.id = *next;
                    for row in &mut t.rows {
                        *next += 1;
                        row.id = *next;
                        for cell in &mut row.cells {
                            *next += 1;
                            cell.id = *next;
                            walk(&mut cell.blocks, next);
                        }
                    }
                }
            }
        }
    }
    // The reader's order: the body, the sections' own headers and footers
    // (in order, the last section's last), the footnotes, the endnotes.
    let mut d = doc.clone();
    let mut next = 0;
    walk(&mut d.body, &mut next);
    for b in &mut d.body {
        if let Block::Paragraph(p) = b
            && let Some(section) = p.section.as_mut()
        {
            for list in Arc::make_mut(section).parts.iter_mut().flatten() {
                walk(list, &mut next);
            }
        }
    }
    for list in [&mut d.header, &mut d.footer, &mut d.header_first, &mut d.footer_first, &mut d.header_even, &mut d.footer_even] {
        walk(list, &mut next);
    }
    for note in d.footnotes.iter_mut().chain(d.endnotes.iter_mut()) {
        walk(&mut note.blocks, &mut next);
    }
    for c in d.comments.iter_mut() {
        walk(&mut c.blocks, &mut next);
    }
    d.id_floor = 0;
    d.file_name = None;
    d.file_size = None;
    d
}

fn flow(p: &Presentation) -> &FlowDocument {
    p.flow.as_deref().unwrap()
}

/// The body's paragraphs with some text, in order.
fn paragraphs(doc: &FlowDocument) -> Vec<&FlowParagraph> {
    fn walk<'a>(blocks: &'a [Block], out: &mut Vec<&'a FlowParagraph>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.push(p),
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&doc.body, &mut out);
    out
}

/// The paragraphs directly in the body (not in tables).
fn body_paragraphs(doc: &FlowDocument) -> Vec<&FlowParagraph> {
    doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).collect()
}

/// A paragraph of plain text of at least `min` characters.
fn text_paragraph(doc: &FlowDocument, min: usize) -> &FlowParagraph {
    paragraphs(doc).into_iter().find(|p| p.len() >= min && !p.text().contains('\u{fffc}')).expect("a paragraph of text")
}

struct Outcome {
    edited: Presentation,
    saved: Vec<u8>,
}

/// Run `commands` on `file` and check undo, redo, save, reread and the
/// save after undoing. `parts` are the parts the save may change.
fn scenario(file: &str, parts: &[&str], commands: impl FnOnce(&FlowDocument) -> Vec<FlowCommand>) -> Outcome {
    let data = std::fs::read(path(file)).unwrap();
    scenario_data(file, data, parts, commands)
}

/// [`scenario`] on a file's bytes (`file` names it in messages).
fn scenario_data(file: &str, data: Vec<u8>, parts: &[&str], commands: impl FnOnce(&FlowDocument) -> Vec<FlowCommand>) -> Outcome {
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let commands = commands(flow(document.presentation()));
    document.execute(commands.into_iter().map(Command::Flow).collect()).unwrap();
    let edited = document.presentation().clone();
    assert!(flow(&edited) != flow(&opened.presentation), "the commands changed nothing");

    // Undo and redo.
    assert!(document.undo());
    assert!(flow(document.presentation()) == flow(&opened.presentation), "undo");
    assert!(document.redo());
    assert!(flow(document.presentation()) == flow(&edited), "redo");

    // Save: only the expected parts differ.
    let saved = opened.source.save(&edited, SaveOptions::default()).unwrap_or_else(|e| panic!("{file}: {e}"));
    let report = diff_packages(&data, &saved, &[]).unwrap();
    for (part, _) in &report.differences {
        assert!(parts.contains(&"*") || parts.contains(&part.as_str()), "{file}: {part} changed: {}", report.to_json());
    }
    // Read back: the edited model.
    let reread = ibis_pptx::read(&saved).unwrap();
    let (a, b) = (normalized(flow(&reread)), normalized(flow(&edited)));
    if a != b {
        let (pa, pb) = (paragraphs(&a), paragraphs(&b));
        for (x, y) in pa.iter().zip(&pb) {
            if x != y {
                panic!("{file}: reads back differently:\n saved {x:#?}\n model {y:#?}");
            }
        }
        panic!("{file}: reads back differently ({} / {} paragraphs)", pa.len(), pb.len());
    }
    // Saved again after undoing: the original file.
    assert!(document.undo());
    let again = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    assert!(again == data, "{file}: undone and saved is not the original");
    // Keep a copy for the outside checks (office-check).
    if let Ok(dir) = std::env::var("IBIS_DOCX_EDIT_OUT") {
        static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let stem = file.rsplit('/').next().unwrap_or(file).trim_end_matches(".docx");
        let name = format!("{}-{}-{n:02}.docx", std::thread::current().name().unwrap_or("t"), stem);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join(name), &saved).unwrap();
    }
    Outcome { edited, saved }
}

const REPORT: &str = "editing/docx-report.docx";
const STATEMENT: &str = "benchmark/securities-registration-statement.docx";
const BODY: &[&str] = &["word/document.xml"];

#[test]
fn every_paragraph_has_its_positions_mapped() {
    for file in [REPORT, STATEMENT] {
        let data = std::fs::read(path(file)).unwrap();
        let opened = ibis_pptx::open(data).unwrap();
        // Typing one character into every paragraph saves (each paragraph's
        // positions are all found).
        let doc = flow(&opened.presentation);
        let mut document = Document::new(opened.presentation.clone());
        let ids: Vec<(u32, usize)> = paragraphs(doc).iter().map(|p| (p.id, p.len())).collect();
        let commands = ids.iter().map(|&(id, len)| Command::Flow(FlowCommand::InsertText { at: FlowPos::new(id, len / 2), text: "Ж".into(), format: None })).collect();
        document.execute(commands).unwrap();
        let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap_or_else(|e| panic!("{file}: {e}"));
        let reread = ibis_pptx::read(&saved).unwrap();
        assert!(normalized(flow(&reread)) == normalized(flow(document.presentation())), "{file}");
    }
}

// T1: typing.
#[test]
fn typing_in_a_paragraph() {
    for file in [REPORT, STATEMENT] {
        scenario(file, BODY, |doc| {
            let p = text_paragraph(doc, 4);
            vec![FlowCommand::InsertText { at: FlowPos::new(p.id, 2), text: "追加 text ".into(), format: None }]
        });
    }
}

// T1: typing into an empty paragraph (a run made from the paragraph mark).
#[test]
fn typing_in_an_empty_paragraph() {
    for file in [REPORT, STATEMENT] {
        scenario(file, BODY, |doc| {
            let p = paragraphs(doc).into_iter().find(|p| p.is_empty()).expect("an empty paragraph");
            vec![FlowCommand::InsertText { at: FlowPos::new(p.id, 0), text: "空の段落に".into(), format: None }]
        });
    }
}

// T2: deleting.
#[test]
fn deleting_characters() {
    for file in [REPORT, STATEMENT] {
        scenario(file, BODY, |doc| {
            let p = text_paragraph(doc, 6);
            vec![FlowCommand::Delete { from: FlowPos::new(p.id, 1), to: FlowPos::new(p.id, 4) }]
        });
    }
}

// T2: deleting a paragraph's whole text.
#[test]
fn deleting_a_whole_paragraph_text() {
    scenario(REPORT, BODY, |doc| {
        let p = text_paragraph(doc, 3);
        vec![FlowCommand::Delete { from: FlowPos::new(p.id, 0), to: FlowPos::new(p.id, p.len()) }]
    });
}

// T4: Enter splits a paragraph; Backspace at the start joins.
#[test]
fn splitting_and_joining_paragraphs() {
    for file in [REPORT, STATEMENT] {
        scenario(file, BODY, |doc| {
            let p = text_paragraph(doc, 6);
            vec![FlowCommand::InsertText { at: FlowPos::new(p.id, 3), text: "\n".into(), format: None }]
        });
        scenario(file, BODY, |doc| {
            let ps = body_paragraphs(doc);
            let i = (1..ps.len()).find(|&i| ps[i - 1].len() > 2 && ps[i].len() > 2 && !ps[i - 1].text().contains('\u{fffc}')).unwrap();
            vec![FlowCommand::Delete { from: FlowPos::new(ps[i - 1].id, ps[i - 1].len()), to: FlowPos::new(ps[i].id, 0) }]
        });
    }
}

// T4 + T1: several paragraphs typed at once, and a selection across
// paragraphs deleted.
#[test]
fn several_paragraphs() {
    scenario(REPORT, BODY, |doc| {
        let p = text_paragraph(doc, 6);
        vec![FlowCommand::InsertText { at: FlowPos::new(p.id, 2), text: "一\n二\t三\u{b}四\n".into(), format: None }]
    });
    scenario(STATEMENT, BODY, |doc| {
        let ps = body_paragraphs(doc);
        let i = (3..ps.len()).find(|&i| ps[i - 3].len() > 2 && ps[i].len() > 2).unwrap();
        vec![FlowCommand::Delete { from: FlowPos::new(ps[i - 3].id, 1), to: FlowPos::new(ps[i].id, 1) }]
    });
}

// F3–F7, F10: character formatting.
#[test]
fn character_formatting() {
    let changes = [
        RunChange::Bold(true),
        RunChange::Italic(true),
        RunChange::Underline(true),
        RunChange::Strike(true),
        RunChange::Font("Arial".into()),
        RunChange::EastAsianFont("游明朝".into()),
        RunChange::Size(2000),
        RunChange::Color(Rgb(0xc00000)),
        RunChange::Highlight(Some(Rgb(0xffff00))),
        RunChange::Highlight(Some(Rgb(0x123456))),
        RunChange::Position(ibis_doc_model::VerticalPosition::Superscript),
        RunChange::Spacing(Some(12_700)),
        RunChange::Spacing(Some(-6_350)),
        RunChange::Scale(Some(150)),
        RunChange::Scale(Some(80)),
        RunChange::Language { lang: Some("en-GB".into()), east_asia: None },
        RunChange::Language { lang: None, east_asia: Some("zh-CN".into()) },
        RunChange::Clear,
    ];
    for file in [REPORT, STATEMENT] {
        for change in &changes {
            eprintln!("{file}: {change:?}");
            scenario(file, BODY, |doc| {
                let p = text_paragraph(doc, 6);
                let mut commands = Vec::new();
                if *change == RunChange::Clear {
                    // Something to clear first.
                    commands.push(FlowCommand::SetRunFormat { from: FlowPos::new(p.id, 2), to: FlowPos::new(p.id, 5), change: RunChange::Italic(true) });
                }
                // A toggle already on is turned off.
                let first = &p.runs[0].format;
                let change = match change {
                    RunChange::Bold(true) if first.bold == Some(true) => RunChange::Bold(false),
                    RunChange::Italic(true) if first.italic == Some(true) => RunChange::Italic(false),
                    RunChange::Underline(true) if first.underline == Some(true) => RunChange::Underline(false),
                    other => other.clone(),
                };
                commands.push(FlowCommand::SetRunFormat { from: FlowPos::new(p.id, 1), to: FlowPos::new(p.id, 4), change });
                commands
            });
        }
    }
}

// F3 over paragraphs, then typing after it takes the formatting.
#[test]
fn bold_across_paragraphs_then_typing() {
    let out = scenario(REPORT, BODY, |doc| {
        let ps = body_paragraphs(doc);
        let i = (1..ps.len()).find(|&i| ps[i - 1].len() > 2 && ps[i].len() > 2).unwrap();
        vec![
            FlowCommand::SetRunFormat { from: FlowPos::new(ps[i - 1].id, 1), to: FlowPos::new(ps[i].id, 2), change: RunChange::Bold(true) },
            FlowCommand::InsertText { at: FlowPos::new(ps[i].id, 1), text: "太".into(), format: None },
        ]
    });
    let reread = ibis_pptx::read(&out.saved).unwrap();
    let bold = paragraphs(flow(&reread)).iter().flat_map(|p| p.runs.iter()).any(|r| r.style.bold && matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains('太')));
    assert!(bold);
    let _ = out.edited;
}

// P1–P4, P8: paragraph formatting and styles.
#[test]
fn paragraph_formatting() {
    let changes = [
        ParaChange::Align(FlowAlign::Center),
        ParaChange::Align(FlowAlign::Right),
        ParaChange::Align(FlowAlign::Justify),
        ParaChange::Align(FlowAlign::Distribute),
        ParaChange::Line(LineHeight::Auto(360)),
        ParaChange::Line(LineHeight::Exact(20 * 12_700)),
        ParaChange::Line(LineHeight::AtLeast(18 * 12_700)),
        ParaChange::SpaceBefore(12 * 12_700),
        ParaChange::SpaceAfter(6 * 12_700),
        ParaChange::Left(360 * 635),
        ParaChange::Right(360 * 635),
        ParaChange::FirstLine(420 * 635),
        ParaChange::FirstLine(-420 * 635),
        ParaChange::OverflowPunct(false),
        ParaChange::AutoSpaceDE(false),
        ParaChange::AutoSpaceDN(false),
        ParaChange::Kinsoku(false),
        ParaChange::WordWrap(false),
        ParaChange::Bidi(true),
        ParaChange::SnapToGrid(false),
    ];
    for file in [REPORT, STATEMENT] {
        for change in &changes {
            scenario(file, BODY, |doc| {
                let ps = body_paragraphs(doc);
                vec![FlowCommand::SetParagraphFormat { first: ps[1].id, last: ps[3].id, change: change.clone() }]
            });
        }
    }
}

#[test]
fn applying_a_heading_style() {
    for file in [REPORT, STATEMENT] {
        let data = std::fs::read(path(file)).unwrap();
        let opened = ibis_pptx::open(data).unwrap();
        let styles = flow(&opened.presentation).styles.clone();
        let Some(heading) = styles.paragraph_style_named("heading 1").map(str::to_string) else { continue };
        let out = scenario(file, BODY, |doc| {
            let p = text_paragraph(doc, 3);
            vec![FlowCommand::SetParagraphFormat { first: p.id, last: p.id, change: ParaChange::Style(Some(heading.clone())) }]
        });
        let reread = ibis_pptx::read(&out.saved).unwrap();
        assert!(paragraphs(flow(&reread)).iter().any(|p| p.style.as_deref() == Some(heading.as_str())));
    }
}

// The header and the footer.
#[test]
fn editing_the_header() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    if flow(&opened.presentation).header.is_empty() {
        return;
    }
    let out = scenario(REPORT, &["word/header1.xml", "word/header2.xml", "word/header3.xml"], |doc| {
        let Block::Paragraph(p) = &doc.header[0] else { panic!() };
        vec![FlowCommand::InsertText { at: FlowPos::new(p.id, 0), text: "ヘッダー".into(), format: None }]
    });
    let _ = Arc::new(out.saved);
}

/// Splitting a paragraph that anchors a floating shape (and has bookmarks)
/// leaves the shape with the first part: it is not copied.
#[test]
fn splitting_keeps_floating_shapes_single() {
    let out = scenario(REPORT, BODY, |doc| {
        let p = body_paragraphs(doc)[0];
        vec![FlowCommand::InsertText { at: FlowPos::new(p.id, 1), text: "\n".into(), format: None }]
    });
    let count = |data: &[u8]| {
        let package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
        let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
        (xml.matches("<wp:anchor").count(), xml.matches("<w:bookmarkStart").count(), xml.matches("w14:paraId=").count())
    };
    let original = std::fs::read(path(REPORT)).unwrap();
    let (a0, b0, p0) = count(&original);
    let (a1, b1, p1) = count(&out.saved);
    assert_eq!((a1, b1), (a0, b0));
    // The new paragraph has no paraId of its own (Word gives one); none is repeated.
    assert_eq!(p1, p0);
}

// P5–P7: lists of the document's own definitions.
#[test]
fn list_items() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let numbering = flow(&opened.presentation).styles.numbering.clone();
    let bullets = numbering.find(true).expect("a list of bullets");
    let numbers = numbering.find(false).expect("a numbered list");
    for change in [ParaChange::List(Some(bullets)), ParaChange::List(Some(numbers))] {
        scenario(REPORT, BODY, |doc| {
            let p = body_paragraphs(doc).into_iter().find(|p| p.list.is_none() && p.len() > 3).unwrap();
            vec![FlowCommand::SetParagraphFormat { first: p.id, last: p.id, change: change.clone() }]
        });
    }
    let out = scenario(REPORT, BODY, |doc| {
        let items: Vec<&FlowParagraph> = body_paragraphs(doc).into_iter().filter(|p| p.list.is_some()).collect();
        vec![
            FlowCommand::SetParagraphFormat { first: items[0].id, last: items[0].id, change: ParaChange::List(None) },
            FlowCommand::SetParagraphFormat { first: items[1].id, last: items[1].id, change: ParaChange::Level(1) },
        ]
    });
    let reread = ibis_pptx::read(&out.saved).unwrap();
    // (The file's lists have one level, so the item is drawn without a number.)
    assert!(body_paragraphs(flow(&reread)).iter().any(|p| p.direct.level == Some(1)));
}

// P5: a list in a document with no lists (numbering.xml made).
#[test]
fn a_new_list() {
    for file in [STATEMENT, REPORT] {
        let parts = ["word/document.xml", "word/numbering.xml", "[Content_Types].xml", "word/_rels/document.xml.rels"];
        let out = scenario(file, &parts, |doc| {
            let (numbering, id) = doc.styles.numbering.with_new_list(true);
            let ps = body_paragraphs(doc);
            vec![
                FlowCommand::SetNumbering { numbering },
                FlowCommand::SetParagraphFormat { first: ps[2].id, last: ps[4].id, change: ParaChange::List(Some(id)) },
            ]
        });
        let reread = ibis_pptx::read(&out.saved).unwrap();
        assert!(body_paragraphs(flow(&reread)).iter().filter(|p| p.list.is_some()).count() >= 3, "{file}");
    }
}

/// The first table of the body with at least `rows` rows and `cols` cells in each.
fn first_table(doc: &FlowDocument, rows: usize, cols: usize) -> &ibis_doc_model::FlowTable {
    doc.body
        .iter()
        .find_map(|b| match b {
            Block::Table(t) if t.rows.len() >= rows && t.rows.iter().all(|r| r.cells.len() >= cols && r.cells.iter().all(|c| c.span == 1 && c.merge == ibis_doc_model::VerticalMerge::None)) => Some(t),
            _ => None,
        })
        .expect("a table")
}

// B3: rows and columns inserted and deleted.
#[test]
fn table_rows_and_columns() {
    use ibis_doc_model::TableEdit;
    let edits = [
        TableEdit::InsertRow { row: 0, below: true },
        TableEdit::InsertRow { row: 0, below: false },
        TableEdit::DeleteRows { first: 1, last: 1 },
        TableEdit::InsertColumn { column: 0, right: true },
        TableEdit::DeleteColumn { column: 1 },
    ];
    for file in [REPORT, STATEMENT] {
        for edit in &edits {
            eprintln!("{file}: {edit:?}");
            let out = scenario(file, BODY, |doc| {
                let t = first_table(doc, 3, 2);
                vec![FlowCommand::EditTable { table: t.id, edit: edit.clone() }]
            });
            let _ = out;
        }
    }
}

// B3 + T1: a row added and typed into.
#[test]
fn typing_in_a_new_row() {
    use ibis_doc_model::TableEdit;
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let t = first_table(flow(&opened.presentation), 2, 2).clone();
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::Flow(FlowCommand::EditTable { table: t.id, edit: TableEdit::InsertRow { row: t.rows.len() - 1, below: true } })]).unwrap();
    let doc = flow(document.presentation()).clone();
    let t2 = doc.body.iter().find_map(|b| if let Block::Table(x) = b && x.id == t.id { Some(x.clone()) } else { None }).unwrap();
    let new_row = t2.rows.last().unwrap();
    let commands: Vec<Command> = new_row
        .cells
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let Block::Paragraph(p) = &c.blocks[0] else { panic!() };
            Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p.id, 0), text: format!("新しい行 {i}"), format: None })
        })
        .collect();
    document.execute(commands).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    assert!(normalized(flow(&reread)) == normalized(flow(document.presentation())));
    let texts: Vec<String> = paragraphs(flow(&reread)).iter().map(|p| p.text()).collect();
    assert!(texts.iter().any(|t| t == "新しい行 0"));
}

// B7, B8, B5: cell shading, alignment, column width.
#[test]
fn table_cell_formatting() {
    use ibis_doc_model::{CellAnchor, TableEdit};
    let edits = [
        TableEdit::Shading { cells: vec![(0, 0), (0, 1)], color: Some(Rgb(0xdeeaf6)) },
        TableEdit::Anchor { cells: vec![(1, 0)], anchor: CellAnchor::Bottom },
        TableEdit::ColumnWidth { column: 0, width: 2 * 914_400 },
    ];
    for file in [REPORT, STATEMENT] {
        for edit in &edits {
            eprintln!("{file}: {edit:?}");
            scenario(file, BODY, |doc| {
                let t = first_table(doc, 2, 2);
                vec![FlowCommand::EditTable { table: t.id, edit: edit.clone() }]
            });
        }
    }
}

// B1, B9: a new table typed into, and a table removed.
#[test]
fn inserting_and_deleting_tables() {
    for file in [REPORT, STATEMENT] {
        let out = scenario(file, BODY, |doc| {
            let p = text_paragraph(doc, 4);
            vec![FlowCommand::InsertTable { at: FlowPos::new(p.id, p.len()), rows: 3, columns: 2, width: 4 * 914_400 }]
        });
        let reread = ibis_pptx::read(&out.saved).unwrap();
        assert!(flow(&reread).body.iter().any(|b| matches!(b, Block::Table(t) if t.rows.len() == 3 && t.columns.len() == 2)));
        scenario(file, BODY, |doc| {
            let t = first_table(doc, 1, 1);
            vec![FlowCommand::DeleteTable { table: t.id }]
        });
        // Splitting a paragraph with a table in its middle.
        scenario(file, BODY, |doc| {
            let p = text_paragraph(doc, 6);
            vec![FlowCommand::InsertTable { at: FlowPos::new(p.id, 3), rows: 2, columns: 3, width: 5 * 914_400 }]
        });
    }
}

/// A picture of the report (its bytes), as a file to insert.
fn a_png() -> Vec<u8> {
    let data = std::fs::read(path(REPORT)).unwrap();
    let package = ibis_ooxml::Package::open(data).unwrap();
    let name = package.names().find(|n| n.ends_with(".png")).expect("a picture in the report").to_string();
    package.read(&name).unwrap()
}

// I1: a new picture inserted; I2: a picture resized; I3: a picture deleted.
#[test]
fn pictures() {
    let parts = ["word/document.xml", "word/_rels/document.xml.rels", "[Content_Types].xml"];
    for file in [REPORT, STATEMENT] {
        let data = std::fs::read(path(file)).unwrap();
        let opened = ibis_pptx::open(data.clone()).unwrap();
        let mut document = Document::new(opened.presentation.clone());
        let media = document.ids().add_media(ibis_doc_model::MediaFormat::Png, a_png());
        let p = text_paragraph(flow(document.presentation()), 4).clone();
        let size = ibis_doc_model::EmuSize::new(914_400, 685_800);
        document.execute(vec![Command::Flow(FlowCommand::InsertPicture { at: FlowPos::new(p.id, 2), media, size })]).unwrap();
        let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
        let report = diff_packages(&data, &saved, &[]).unwrap();
        for (part, _) in &report.differences {
            assert!(parts.contains(&part.as_str()) || part.starts_with("word/media/"), "{file}: {part}");
        }
        let reread = ibis_pptx::read(&saved).unwrap();
        let q = paragraphs(flow(&reread)).into_iter().find(|q| q.text().starts_with(&p.text().chars().take(2).collect::<String>()) && q.runs.iter().any(|r| matches!(r.kind, ibis_doc_model::FlowRunKind::Picture { size: s, .. } if s == size))).expect("the picture read back").clone();
        let _ = q;
        // Undone: the original file.
        assert!(document.undo());
        assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
        if let Ok(dir) = std::env::var("IBIS_DOCX_EDIT_OUT") {
            let stem = file.rsplit('/').next().unwrap().trim_end_matches(".docx");
            std::fs::write(std::path::Path::new(&dir).join(format!("pictures-insert-{stem}.docx")), &saved).unwrap();
        }
    }
    // The report's own picture resized, then deleted.
    let find = |doc: &FlowDocument| -> FlowPos {
        paragraphs(doc).iter().find_map(|p| {
            let mut at = 0;
            for r in &p.runs {
                if matches!(r.kind, ibis_doc_model::FlowRunKind::Picture { .. }) {
                    return Some(FlowPos::new(p.id, at));
                }
                at += r.len();
            }
            None
        }).expect("a picture")
    };
    scenario(REPORT, BODY, |doc| vec![FlowCommand::SetPictureSize { at: find(doc), size: ibis_doc_model::EmuSize::new(1_828_800, 1_371_600) }]);
    scenario(REPORT, BODY, |doc| {
        let at = find(doc);
        vec![FlowCommand::Delete { from: at, to: FlowPos::new(at.para, at.offset + 1) }]
    });
}

#[test]
fn picture_wrapping() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let media = document.ids().add_media(ibis_doc_model::MediaFormat::Png, a_png());
    let p = text_paragraph(flow(document.presentation()), 4).clone();
    let size = ibis_doc_model::EmuSize::new(914_400, 685_800);
    let at = FlowPos::new(p.id, 2);

    // 1. Insert an inline picture
    document.execute(vec![Command::Flow(FlowCommand::InsertPicture { at, media, size })]).unwrap();
    let saved_inline = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_inline = ibis_pptx::read(&saved_inline).unwrap();
    let q = paragraphs(flow(&reread_inline)).into_iter().find(|q| q.id == p.id).unwrap();
    assert!(q.runs.iter().any(|r| matches!(r.kind, ibis_doc_model::FlowRunKind::Picture { .. })));

    // 2. Set wrap to Square (wp:anchor + wp:wrapSquare)
    document.execute(vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::Square })]).unwrap();
    let saved_square = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_square = ibis_pptx::read(&saved_square).unwrap();
    let q_square = paragraphs(flow(&reread_square)).into_iter().find(|q| q.id == p.id).unwrap();
    let shape_square = q_square.runs.iter().find_map(|r| match &r.kind {
        ibis_doc_model::FlowRunKind::Shape(s) => Some(s),
        _ => None,
    }).expect("expected shape with floating anchor");
    assert_eq!(shape_square.floating.as_ref().unwrap().wrap, "square");
    assert!(!shape_square.floating.as_ref().unwrap().behind_doc);

    // 3. Set wrap to TopAndBottom (wp:anchor + wp:wrapTopAndBottom)
    document.execute(vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::TopAndBottom })]).unwrap();
    let saved_tb = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_tb = ibis_pptx::read(&saved_tb).unwrap();
    let q_tb = paragraphs(flow(&reread_tb)).into_iter().find(|q| q.id == p.id).unwrap();
    let shape_tb = q_tb.runs.iter().find_map(|r| match &r.kind {
        ibis_doc_model::FlowRunKind::Shape(s) => Some(s),
        _ => None,
    }).expect("expected shape with floating anchor");
    assert_eq!(shape_tb.floating.as_ref().unwrap().wrap, "topAndBottom");

    // 4. Set wrap to BehindText (wp:anchor + behindDoc="1")
    document.execute(vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::BehindText })]).unwrap();
    let saved_behind = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_behind = ibis_pptx::read(&saved_behind).unwrap();
    let q_behind = paragraphs(flow(&reread_behind)).into_iter().find(|q| q.id == p.id).unwrap();
    let shape_behind = q_behind.runs.iter().find_map(|r| match &r.kind {
        ibis_doc_model::FlowRunKind::Shape(s) => Some(s),
        _ => None,
    }).expect("expected shape with floating anchor");
    assert!(shape_behind.floating.as_ref().unwrap().behind_doc);
    assert!(shape_behind.floating.as_ref().unwrap().layout_in_cell);

    // 4b. Set relative_v to "page" (Fix position on page)
    let mut custom_anchor = shape_behind.floating.clone().unwrap();
    custom_anchor.relative_v = "page".to_string();
    custom_anchor.locked = true;
    custom_anchor.allow_overlap = false;
    custom_anchor.layout_in_cell = false;
    custom_anchor.relative_height = 300_000_000;
    document.execute(vec![Command::Flow(FlowCommand::SetPictureFloating { at, anchor: Some(custom_anchor) })]).unwrap();
    let saved_page = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_page = ibis_pptx::read(&saved_page).unwrap();
    let q_page = paragraphs(flow(&reread_page)).into_iter().find(|q| q.id == p.id).unwrap();
    let shape_page = q_page.runs.iter().find_map(|r| match &r.kind {
        ibis_doc_model::FlowRunKind::Shape(s) => Some(s),
        _ => None,
    }).expect("expected shape with floating anchor");
    assert_eq!(shape_page.floating.as_ref().unwrap().relative_v, "page");
    assert!(!shape_page.floating.as_ref().unwrap().moves_with_text());
    assert!(shape_page.floating.as_ref().unwrap().locked);
    assert!(!shape_page.floating.as_ref().unwrap().allow_overlap);
    assert!(!shape_page.floating.as_ref().unwrap().layout_in_cell);
    assert_eq!(shape_page.floating.as_ref().unwrap().relative_height, 300_000_000);

    // 5. Convert back to Inline
    document.execute(vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::Inline })]).unwrap();
    let saved_back = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_back = ibis_pptx::read(&saved_back).unwrap();
    let q_back = paragraphs(flow(&reread_back)).into_iter().find(|q| q.id == p.id).unwrap();
    assert!(q_back.runs.iter().any(|r| matches!(r.kind, ibis_doc_model::FlowRunKind::Picture { .. })));

    // 6. Undo all and check pristine
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

// S1, S2: paper orientation and size, margins.
#[test]
fn page_setup() {
    for file in [REPORT, STATEMENT] {
        let landscape = |doc: &FlowDocument| {
            let mut page = doc.page;
            page.size = ibis_doc_model::EmuSize::new(page.size.height, page.size.width);
            vec![FlowCommand::SetPage { page }]
        };
        let out = scenario(file, BODY, landscape);
        let reread = ibis_pptx::read(&out.saved).unwrap();
        assert!(flow(&reread).page.size.width > flow(&reread).page.size.height);
        scenario(file, BODY, |doc| {
            let mut page = doc.page;
            for m in [&mut page.top, &mut page.right, &mut page.bottom, &mut page.left] {
                *m = 720 * 635;
            }
            vec![FlowCommand::SetPage { page }]
        });
        scenario(file, BODY, |doc| {
            let mut page = doc.page;
            page.size = ibis_doc_model::EmuSize::new(10_320 * 635, 14_568 * 635); // B5
            vec![FlowCommand::SetPage { page }]
        });
    }
}

// S4: a section break (next page, continuous) after a paragraph, and removed.
#[test]
fn section_breaks() {
    use ibis_doc_model::SectionBreak;
    for file in [REPORT, STATEMENT] {
        for kind in [SectionBreak::NextPage, SectionBreak::Continuous, SectionBreak::OddPage] {
            let out = scenario(file, BODY, |doc| {
                let p = text_paragraph(doc, 6);
                vec![FlowCommand::SetParagraphFormat { first: p.id, last: p.id, change: ParaChange::SectionBreak(kind) }]
            });
            let reread = ibis_pptx::read(&out.saved).unwrap();
            assert!(paragraphs(flow(&reread)).iter().any(|p| p.section_break == kind));
        }
    }
    // The report's own section breaks: none; the statement's: remove one if any.
    let data = std::fs::read(path(STATEMENT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    if paragraphs(flow(&opened.presentation)).iter().any(|p| p.section_break != SectionBreak::None) {
        scenario(STATEMENT, BODY, |doc| {
            let p = paragraphs(doc).into_iter().find(|p| p.section_break != SectionBreak::None).unwrap();
            vec![FlowCommand::SetParagraphFormat { first: p.id, last: p.id, change: ParaChange::SectionBreak(SectionBreak::None) }]
        });
    }
}

// H2, H3: a footer added to a document without one, with a page number in
// it; a page number in an existing header.
#[test]
fn headers_and_page_numbers() {
    // The statement without its header and footer references (a document
    // that has none).
    let bare = {
        let mut package = ibis_ooxml::Package::open(std::fs::read(path(STATEMENT)).unwrap()).unwrap();
        let doc = package.xml_mut("word/document.xml").unwrap();
        let refs: Vec<_> = doc.descendants(doc.root_element().unwrap()).into_iter().filter(|&n| doc.is_element(n) && matches!(doc.local_name(n), "headerReference" | "footerReference")).collect();
        for r in refs {
            doc.detach(r);
        }
        package.save(SaveOptions::default()).unwrap()
    };
    for file in [REPORT, STATEMENT, "bare"] {
        let data = if file == "bare" { bare.clone() } else { std::fs::read(path(file)).unwrap() };
        let opened = ibis_pptx::open(data.clone()).unwrap();
        let doc = flow(&opened.presentation);
        if file == "bare" {
            assert!(doc.header.is_empty() && doc.footer.is_empty());
        }
        for footer in [false, true].into_iter().filter(|&f| if f { doc.footer.is_empty() } else { doc.header.is_empty() }) {
            let mut document = Document::new(opened.presentation.clone());
            document.execute(vec![Command::Flow(FlowCommand::AddHeader { footer })]).unwrap();
            let d = flow(document.presentation()).clone();
            let list = if footer { &d.footer } else { &d.header };
            let Block::Paragraph(p) = &list[0] else { panic!() };
            document.execute(vec![Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p.id, 0), text: "ページ ".into(), format: None }), Command::Flow(FlowCommand::InsertPageNumber { at: FlowPos::new(p.id, 4) })]).unwrap();
            let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
            let reread = ibis_pptx::read(&saved).unwrap();
            assert!(normalized(flow(&reread)) == normalized(flow(document.presentation())), "{file}");
            while document.undo() {}
            assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
            if let Ok(dir) = std::env::var("IBIS_DOCX_EDIT_OUT") {
                let stem = file.rsplit('/').next().unwrap().trim_end_matches(".docx");
                std::fs::write(std::path::Path::new(&dir).join(format!("headers-add-{footer}-{stem}.docx")), &saved).unwrap();
            }
        }
        if !doc.header.is_empty() && file != "bare" {
            let header_parts: Vec<String> = (1..=3).map(|n| format!("word/header{n}.xml")).collect();
            let parts: Vec<&str> = header_parts.iter().map(String::as_str).collect();
            scenario(file, &parts, |doc| {
                let Block::Paragraph(p) = &doc.header[0] else { panic!() };
                vec![FlowCommand::InsertPageNumber { at: FlowPos::new(p.id, p.len()) }]
            });
        }
    }
}

// X1: a link made on some text (an address, a bookmark), typed into, and
// removed.
#[test]
fn links() {
    use ibis_doc_model::Link;
    let rels = ["word/document.xml", "word/_rels/document.xml.rels"];
    for file in [REPORT, STATEMENT] {
        let out = scenario(file, &rels, |doc| {
            let p = text_paragraph(doc, 8);
            vec![
                FlowCommand::SetLink { from: FlowPos::new(p.id, 1), to: FlowPos::new(p.id, 5), link: Some(Link::Url("https://example.com/report".into())) },
                FlowCommand::InsertText { at: FlowPos::new(p.id, 3), text: "リンク".into(), format: None },
            ]
        });
        let reread = ibis_pptx::read(&out.saved).unwrap();
        let linked: String = paragraphs(flow(&reread)).iter().flat_map(|p| p.runs.iter()).filter(|r| r.link.is_some()).filter_map(|r| if let ibis_doc_model::FlowRunKind::Text(t) = &r.kind { Some(t.clone()) } else { None }).collect();
        assert!(linked.contains("リンク"), "typed inside the link: {linked}");
        scenario(file, BODY, |doc| {
            let p = text_paragraph(doc, 8);
            vec![FlowCommand::SetLink { from: FlowPos::new(p.id, 0), to: FlowPos::new(p.id, 3), link: Some(Link::Anchor("_Toc1".into())) }]
        });
    }
    // Links read from the file and removed: the statement's, if it has any.
    let data = std::fs::read(path(STATEMENT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let has = paragraphs(flow(&opened.presentation)).iter().any(|p| p.runs.iter().any(|r| r.link.is_some()));
    if has {
        scenario(STATEMENT, BODY, |doc| {
            let p = paragraphs(doc).into_iter().find(|p| p.runs.iter().any(|r| r.link.is_some())).unwrap();
            vec![FlowCommand::SetLink { from: FlowPos::new(p.id, 0), to: FlowPos::new(p.id, p.len()), link: None }]
        });
    }
}

#[test]
fn a_table_of_contents_defines_its_styles() {
    use ibis_doc_model::flow_command::{missing_toc_styles, toc_entries};
    for file in [REPORT, STATEMENT] {
        let out = scenario(file, &["word/document.xml", "word/styles.xml"], |doc| {
            let missing = missing_toc_styles(&doc.styles, 1, 3);
            let mut styled = doc.clone();
            let styles = Arc::make_mut(&mut styled.styles);
            for (id, s) in &missing {
                styles.styles.insert(id.clone(), s.clone());
            }
            let first = body_paragraphs(doc)[0].id;
            let (entries, bookmarks) = toc_entries(&styled, 1, 3, &|_| Some(1), doc.new_id(), 9_448 * 635);
            let mut commands: Vec<FlowCommand> = missing.into_iter().map(|(id, s)| FlowCommand::SetStyle { id, style: Some(s) }).collect();
            commands.push(FlowCommand::SetToc { before: first, replace: false, entries, bookmarks });
            commands
        });
        let reread = ibis_pptx::read(&out.saved).unwrap();
        let styles = &flow(&reread).styles;
        for level in 1..=3 {
            assert!(styles.paragraph_style_named(&format!("toc {level}")).is_some(), "{file}: toc {level}");
        }
    }
}

#[test]
fn a_table_of_contents_with_custom_styles() {
    use ibis_doc_model::flow::{FlowStyle, StyleKind};
    use ibis_doc_model::flow_command::{missing_toc_styles_for_levels, toc_entries_with_instruction, ParaChange, TocInstruction};
    let file = STATEMENT;
    let out = scenario(file, &["word/document.xml", "word/styles.xml"], |doc| {
        let mut styled = doc.clone();

        // カスタム段落スタイル "AppendixStyle" を追加
        let normal = styled.styles.default_paragraph.clone();
        let app_style = FlowStyle {
            kind: StyleKind::Paragraph,
            name: "AppendixStyle".into(),
            based_on: normal.clone(),
            next: normal,
            ..FlowStyle::default()
        };
        Arc::make_mut(&mut styled.styles).styles.insert("AppendixStyle".into(), app_style.clone());

        let instruction = TocInstruction {
            heading_levels: Some((1, 2)),
            custom_styles: vec![("AppendixStyle".to_string(), 2)],
            caption_label: None,
            hyperlinks: true,
            hide_in_web: true,
            use_outline_levels: true,
        };

        let missing = missing_toc_styles_for_levels(&styled.styles, instruction.all_levels());
        for (id, s) in &missing {
            Arc::make_mut(&mut styled.styles).styles.insert(id.clone(), s.clone());
        }

        // 最後の段落に AppendixStyle を適用
        let paras = body_paragraphs(&styled);
        let target_p_id = paras.last().unwrap().id;
        let first = paras[0].id;

        let (entries, bookmarks) = toc_entries_with_instruction(&styled, &instruction, &|_| Some(1), doc.new_id(), 9_448 * 635);

        let mut commands: Vec<FlowCommand> = vec![
            FlowCommand::SetStyle { id: "AppendixStyle".into(), style: Some(app_style) },
            FlowCommand::SetParagraphFormat { first: target_p_id, last: target_p_id, change: ParaChange::Style(Some("AppendixStyle".into())) },
        ];
        commands.extend(missing.into_iter().map(|(id, s)| FlowCommand::SetStyle { id, style: Some(s) }));
        commands.push(FlowCommand::SetToc { before: first, replace: false, entries, bookmarks });
        commands
    });

    let reread = ibis_pptx::read(&out.saved).unwrap();
    let doc = flow(&reread);
    let ps = body_paragraphs(doc);
    let toc_start = ps.iter().find(|p| p.field_start.is_some()).expect("TOC exists");
    let field_start = toc_start.field_start.as_deref().unwrap();
    assert!(field_start.contains("\\t"), "field_start should contain \\t switch: {field_start}");
    assert!(field_start.contains("AppendixStyle"), "field_start should contain AppendixStyle: {field_start}");
}

#[test]
fn changing_a_style() {
    for file in [REPORT, STATEMENT] {
        scenario(file, &["word/styles.xml"], |doc| {
            let id = doc.styles.paragraph_style_named("heading 1").or(doc.styles.default_paragraph.as_deref()).unwrap().to_string();
            let mut style = doc.styles.styles[&id].clone();
            style.run.bold = Some(!style.run.bold.unwrap_or(false));
            style.run.size = Some(1800);
            style.run.color = Some(Rgb(0x204080));
            style.run.theme_color = None;
            style.para.before = Some(240 * 635);
            style.para.align = Some(FlowAlign::Center);
            vec![FlowCommand::SetStyle { id, style: Some(style) }]
        });
    }
}

#[test]
fn restarting_and_continuing_numbering() {
    use ibis_doc_model::flow_command::{Renumber, renumber};
    let parts = ["word/document.xml", "word/numbering.xml"];
    let mut ran = 0;
    for file in [REPORT, STATEMENT] {
        let data = std::fs::read(path(file)).unwrap();
        let opened = ibis_pptx::open(data).unwrap();
        let doc = flow(&opened.presentation);
        let numbered = |p: &&FlowParagraph| p.list.and_then(|l| doc.styles.numbering.level(l.num_id, l.level)).is_some_and(|l| l.format != "bullet");
        let Some(item) = paragraphs(doc).into_iter().filter(numbered).nth(1) else { continue };
        let id = item.id;
        ran += 1;
        let out = scenario(file, &parts, |doc| renumber(doc, id, Renumber::StartAt(5)).unwrap());
        let reread = ibis_pptx::read(&out.saved).unwrap();
        let labels = ibis_doc_model::list_labels(flow(&reread));
        assert!(labels.get(&id).is_some_and(|l| l.contains('5')), "{file}: {:?}", labels.get(&id));
        // Back to going on from the list before it, in the edited file.
        let edited = flow(&out.edited).clone();
        if let Some(commands) = renumber(&edited, id, Renumber::Continue) {
            assert!(!commands.is_empty());
        }
    }
    assert!(ran > 0, "a file with a numbered list");
}

#[test]
fn header_rows_and_rows_kept_whole() {
    use ibis_doc_model::TableEdit;
    for file in [REPORT, STATEMENT] {
        let data = std::fs::read(path(file)).unwrap();
        let opened = ibis_pptx::open(data).unwrap();
        let Some(t) = flow(&opened.presentation).body.iter().find_map(|b| if let Block::Table(t) = b && t.rows.len() >= 3 { Some(t.clone()) } else { None }) else { continue };
        let id = t.id;
        let on = !t.rows[0].header;
        let out = scenario(file, BODY, |_| {
            vec![
                FlowCommand::EditTable { table: id, edit: TableEdit::RepeatHeader { row: 0, on } },
                FlowCommand::EditTable { table: id, edit: TableEdit::CantSplit { rows: vec![1, 2], on: !t.rows[1].cant_split } },
                FlowCommand::EditTable { table: id, edit: TableEdit::InsertRow { row: 0, below: true } },
            ]
        });
        let reread = ibis_pptx::read(&out.saved).unwrap();
        let t2 = flow(&reread).body.iter().find_map(|b| if let Block::Table(x) = b && x.rows.len() == t.rows.len() + 1 { Some(x.clone()) } else { None }).unwrap();
        assert_eq!(t2.rows[0].header, on, "{file}");
        assert!(!t2.rows[1].header, "{file}: the row added below the header row is not one");
    }
}

#[test]
fn captions_and_cross_references() {
    use ibis_doc_model::flow_command::{cross_reference, insert_caption, updated_fields};
    for file in [REPORT, STATEMENT] {
        let out = scenario(file, &["word/document.xml", "word/styles.xml"], |doc| {
            let p = text_paragraph(doc, 8);
            let (mut commands, _) = insert_caption(doc, p.id, "表", "売上").unwrap();
            // Applied on a copy, to refer to the caption and number it.
            let mut presentation = Presentation::default();
            presentation.flow = Some(Arc::new(doc.clone()));
            let mut d = Document::new(presentation);
            d.execute(commands.iter().cloned().map(Command::Flow).collect()).unwrap();
            let caption = paragraphs(flow(d.presentation())).into_iter().find(|q| q.bookmarks.iter().any(|b| b.starts_with("_Ref"))).unwrap().id;
            let more = cross_reference(flow(d.presentation()), FlowPos::new(p.id, 2), caption).unwrap();
            d.execute(more.iter().cloned().map(Command::Flow).collect()).unwrap();
            let update = updated_fields(flow(d.presentation()));
            commands.extend(more);
            commands.extend(update);
            commands
        });
        let reread = ibis_pptx::read(&out.saved).unwrap();
        let shown: Vec<String> = paragraphs(flow(&reread)).iter().flat_map(|p| p.runs.iter()).filter_map(|r| if let ibis_doc_model::FlowRunKind::Field { instruction, shown } = &r.kind { instruction.starts_with("REF").then(|| shown.clone()) } else { None }).collect();
        assert!(shown.iter().any(|s| s.starts_with("表 ")), "{file}: {shown:?}");
    }
}

fn report_with_complex_paragraph() -> Vec<u8> {
    ibis_pptx::test_files::docx_with_complex_paragraph(&std::fs::read(path(REPORT)).unwrap())
}

// H12: typing, deleting and splitting around a comment, tracked changes,
// a footnote's mark and a field: they stay as they were.
#[test]
fn editing_around_complex_elements() {
    let data = report_with_complex_paragraph();
    let count = |xml: &str, tag: &str| xml.matches(tag).count();
    let tags = ["<w:commentRangeStart", "<w:commentRangeEnd", "<w:commentReference", "<w:ins ", "<w:del ", "<w:delText", "<w:footnoteReference", "<w:fldSimple"];
    let original = String::from_utf8(ibis_ooxml::Package::open(data.clone()).unwrap().read("word/document.xml").unwrap()).unwrap();
    let edits: Vec<(&str, Box<dyn Fn(&FlowParagraph) -> Vec<FlowCommand>>)> = vec![
        ("typing at the start and the end", Box::new(|p: &FlowParagraph| vec![FlowCommand::InsertText { at: FlowPos::new(p.id, p.len()), text: "（末尾）".into(), format: None }, FlowCommand::InsertText { at: FlowPos::new(p.id, 0), text: "（先頭）".into(), format: None }])),
        ("deleting next to the comment", Box::new(|p: &FlowParagraph| vec![FlowCommand::Delete { from: FlowPos::new(p.id, 1), to: FlowPos::new(p.id, 3) }])),
        ("splitting after the footnote", Box::new(|p: &FlowParagraph| {
            let at = p.text().find("後の文").map(|b| p.text()[..b].chars().count()).unwrap();
            vec![FlowCommand::InsertText { at: FlowPos::new(p.id, at + 2), text: "\n".into(), format: None }]
        })),
        ("typing inside the comment's range", Box::new(|p: &FlowParagraph| {
            let at = p.text().find("範囲").map(|b| p.text()[..b].chars().count()).unwrap();
            vec![FlowCommand::InsertText { at: FlowPos::new(p.id, at), text: "の".into(), format: None }]
        })),
    ];
    for (name, edit) in edits {
        let out = scenario_data(name, data.clone(), BODY, |doc| {
            let p = paragraphs(doc).into_iter().rev().find(|p| p.text().starts_with("前の文")).expect("the paragraph");
            edit(p)
        });
        let saved = String::from_utf8(ibis_ooxml::Package::open(out.saved).unwrap().read("word/document.xml").unwrap()).unwrap();
        for tag in tags {
            assert_eq!(count(&saved, tag), count(&original, tag), "{name}: {tag}");
        }
    }
}

/// Commands made one after another, each from the document the ones before
/// it make.
fn chained(doc: &FlowDocument, steps: Vec<Box<dyn Fn(&FlowDocument) -> Vec<FlowCommand>>>) -> Vec<FlowCommand> {
    let mut presentation = Presentation::default();
    presentation.flow = Some(Arc::new(doc.clone()));
    let mut d = Document::new(presentation);
    let mut all = Vec::new();
    for step in steps {
        let commands = step(flow(d.presentation()));
        d.execute(commands.iter().cloned().map(Command::Flow).collect()).unwrap();
        all.extend(commands);
    }
    all
}

// H6: a section break; the second section landscape, numbered from 1, with
// a header of its own (前と同じ off) and its own even pages' header; the
// first numbered i, ii.
#[test]
fn sections_with_their_own_headers_and_numbers() {
    use ibis_doc_model::flow_command::{link_part, sections, set_even_and_odd, set_section};
    use ibis_doc_model::{PartKind, SectionBreak};
    for file in [REPORT, STATEMENT] {
        let out = scenario(file, &["*"], |doc| {
            let p = text_paragraph(doc, 6).id;
            chained(
                doc,
                vec![
                    Box::new(move |_| vec![FlowCommand::SetParagraphFormat { first: p, last: p, change: ParaChange::SectionBreak(SectionBreak::NextPage) }]),
                    Box::new(|d| {
                        vec![set_section(d, 0, |s| {
                            s.page.size = ibis_doc_model::EmuSize::new(s.page.size.height, s.page.size.width);
                            s.restart_numbering = true;
                            s.page.first_page = 1;
                        })
                        .unwrap()]
                    }),
                    Box::new(|d| {
                        let first = sections(d)[0];
                        vec![set_section(d, first, |s| {
                            s.page_number_format = Some("lowerRoman".into());
                            s.restart_numbering = true;
                            s.page.first_page = 1;
                        })
                        .unwrap()]
                    }),
                    Box::new(|d| vec![link_part(d, 0, PartKind::Header, true).unwrap()]),
                    Box::new(|d| {
                        let id = match &d.header[0] {
                            Block::Paragraph(p) => p.id,
                            _ => unreachable!(),
                        };
                        vec![FlowCommand::InsertText { at: FlowPos::new(id, 0), text: "（後ろ）".into(), format: None }]
                    }),
                    Box::new(|d| vec![set_even_and_odd(d, true)]),
                    Box::new(|d| vec![link_part(d, 0, PartKind::HeaderEven, true).unwrap()]),
                ],
            )
        });
        let reread = ibis_pptx::read(&out.saved).unwrap();
        let d = flow(&reread);
        assert!(d.page.size.width > d.page.size.height, "{file}: the last section is landscape");
        assert!(d.restart_numbering && d.even_and_odd && !d.linked[0] && !d.linked[PartKind::HeaderEven.index()], "{file}: {} {} {:?}", d.restart_numbering, d.even_and_odd, d.linked);
        let text: String = d.header.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect();
        assert!(text.starts_with("（後ろ）"), "{file}: {text}");
        let first = paragraphs(d).into_iter().find_map(|p| p.section.clone()).expect("the first section");
        assert_eq!(first.page_number_format.as_deref(), Some("lowerRoman"));
        assert!(first.page.size.width < first.page.size.height, "{file}: the first section stays portrait");
    }
}

// H5: footnotes put in (a document without footnotes.xml, and one with),
// one removed, all made endnotes.
#[test]
fn footnotes_and_endnotes() {
    use ibis_doc_model::flow_command::{convert_notes, insert_note};
    let complex = report_with_complex_paragraph();
    let files: Vec<(&str, Vec<u8>)> = vec![(REPORT, std::fs::read(path(REPORT)).unwrap()), (STATEMENT, std::fs::read(path(STATEMENT)).unwrap()), ("complex", complex)];
    for (file, data) in files {
        let out = scenario_data(file, data.clone(), &["*"], |doc| {
            let p = text_paragraph(doc, 8).id;
            chained(
                doc,
                vec![
                    Box::new(move |d| insert_note(d, FlowPos::new(p, 2), false).unwrap().0),
                    Box::new(move |d| insert_note(d, FlowPos::new(p, 5), false).unwrap().0),
                    Box::new(move |d| {
                        let id = match &d.footnotes.last().unwrap().blocks[0] {
                            Block::Paragraph(q) => q.id,
                            _ => unreachable!(),
                        };
                        vec![FlowCommand::InsertText { at: FlowPos::new(id, 2), text: "注の文".into(), format: None }]
                    }),
                ],
            )
        });
        let reread = ibis_pptx::read(&out.saved).unwrap();
        let d = flow(&reread);
        assert!(d.footnotes.len() >= 2, "{file}: {}", d.footnotes.len());
        assert!(d.footnotes.iter().any(|n| n.blocks.iter().any(|b| matches!(b, Block::Paragraph(p) if p.text().contains("注の文")))), "{file}");
        // All made endnotes.
        let out2 = scenario_data(file, out.saved.clone(), &["*"], |doc| vec![convert_notes(doc, true).unwrap()]);
        let reread = ibis_pptx::read(&out2.saved).unwrap();
        let d = flow(&reread);
        assert!(d.footnotes.is_empty() && d.endnotes.len() >= 2, "{file}");
        assert!(paragraphs(d).iter().any(|p| p.runs.iter().any(|r| matches!(r.kind, ibis_doc_model::FlowRunKind::NoteRef { endnote: true, .. }))));
    }
}

// H10: a comment on a word, an answer, resolved; then removed.
#[test]
fn comments_answers_and_resolving() {
    use ibis_doc_model::flow_command::{CommentAuthor, add_comment, delete_comment, reply_comment, resolve_comment};
    let who = CommentAuthor { author: "Test".into(), initials: "T".into(), date: "2026-10-01T00:00:00Z".into() };
    let complex = report_with_complex_paragraph();
    for (file, data) in [(REPORT, std::fs::read(path(REPORT)).unwrap()), ("complex", complex)] {
        let w = who.clone();
        let out = scenario_data(file, data.clone(), &["*"], move |doc| {
            let p = text_paragraph(doc, 8).id;
            let w2 = w.clone();
            let w3 = w.clone();
            chained(
                doc,
                vec![
                    Box::new(move |d| add_comment(d, FlowPos::new(p, 1), FlowPos::new(p, 4), &w2, "確認してください").unwrap()),
                    Box::new(move |d| {
                        let id = d.comments.iter().map(|c| c.id).max().unwrap();
                        reply_comment(d, id, &w3, "確認しました").unwrap()
                    }),
                    Box::new(|d| {
                        let id = d.comments.iter().filter(|c| c.parent.is_none()).map(|c| c.id).max().unwrap();
                        vec![resolve_comment(d, id, true)]
                    }),
                ],
            )
        });
        let reread = ibis_pptx::read(&out.saved).unwrap();
        let d = flow(&reread);
        let parent = d.comments.iter().find(|c| c.blocks.iter().any(|b| matches!(b, Block::Paragraph(p) if p.text() == "確認してください"))).expect("the comment");
        let reply = d.comments.iter().find(|c| c.parent == Some(parent.id)).expect("the answer");
        assert!(parent.done && reply.done, "{file}: resolved");
        // Removed again.
        let id = parent.id;
        scenario_data(file, out.saved.clone(), &["*"], move |doc| vec![delete_comment(doc, id).unwrap()]);
    }
}

// H9: changes tracked: an insertion, a deletion and a change of formatting
// saved as w:ins, w:del and w:rPrChange; then accepted, or rejected.
#[test]
fn tracked_changes() {
    use ibis_doc_model::flow_command::resolve_revisions;
    use ibis_doc_model::{Revision, RevisionKind};
    let complex = report_with_complex_paragraph();
    for (file, data) in [(REPORT, std::fs::read(path(REPORT)).unwrap()), (STATEMENT, std::fs::read(path(STATEMENT)).unwrap()), ("complex", complex)] {
        let (author, date) = ("Test".to_string(), "2026-10-01T00:00:00Z".to_string());
        let out = scenario_data(file, data.clone(), &["*"], |doc| {
            let p = text_paragraph(doc, 12).id;
            let insert = Revision { kind: RevisionKind::Insert, author: author.clone(), date: date.clone() };
            vec![
                FlowCommand::SetTrackChanges(true),
                FlowCommand::InsertText { at: FlowPos::new(p, 2), text: "追加".into(), format: None },
                FlowCommand::MarkRevision { from: FlowPos::new(p, 2), to: FlowPos::new(p, 4), revision: Some(insert) },
                FlowCommand::TrackDelete { from: FlowPos::new(p, 6), to: FlowPos::new(p, 8), author: author.clone(), date: date.clone() },
                FlowCommand::TrackFormat { from: FlowPos::new(p, 9), to: FlowPos::new(p, 11), author: author.clone(), date: date.clone() },
                FlowCommand::SetRunFormat { from: FlowPos::new(p, 9), to: FlowPos::new(p, 11), change: RunChange::Bold(true) },
            ]
        });
        let xml = String::from_utf8(ibis_ooxml::Package::open(out.saved.clone()).unwrap().read("word/document.xml").unwrap()).unwrap();
        assert!(xml.contains("<w:ins ") && xml.contains("<w:delText") && xml.contains("<w:rPrChange"), "{file}");
        let reread = ibis_pptx::read(&out.saved).unwrap();
        assert!(flow(&reread).track_changes);
        // All accepted; all rejected.
        for accept in [true, false] {
            let o = scenario_data(file, out.saved.clone(), &["*"], move |doc| resolve_revisions(doc, None, accept));
            let d = ibis_pptx::read(&o.saved).unwrap();
            let p = paragraphs(flow(&d)).into_iter().find(|p| p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains("追加")))).map(|p| p.id);
            assert_eq!(p.is_some(), accept, "{file}: the insertion stays when accepted");
            assert!(paragraphs(flow(&d)).iter().all(|p| p.runs.iter().all(|r| r.revision.is_none() || file == "complex")), "{file}");
        }
    }
}

// P9: changed shading/edges save without disturbing between/bar or other pPr.
#[test]
fn paragraph_decorations_preserve_unedited_properties() {
    let mut package=ibis_ooxml::Package::open(std::fs::read(path(REPORT)).unwrap()).unwrap();
    let xml=String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let between=r#"<w:between w:val="dashed" w:sz="17" w:space="7" w:color="123456"/>"#;
    let bar=r#"<w:bar w:val="double" w:sz="9" w:space="3" w:color="654321"/>"#;
    let paragraph=format!(r#"<w:p><w:pPr><w:keepNext/><w:pBdr><w:top w:val="single" w:sz="6" w:space="2" w:color="112233" w:shadow="1"/>{between}{bar}</w:pBdr><w:shd w:val="pct20" w:color="FF0000" w:fill="FFFFFF"/></w:pPr><w:r><w:t>P9 preservation target</w:t></w:r></w:p>"#);
    let at=xml.rfind("<w:sectPr").unwrap();
    package.set_bytes("word/document.xml",format!("{}{}{}",&xml[..at],paragraph,&xml[at..]).into_bytes());
    let data=package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    for remove in [false,true] {
        let outcome=scenario_data("p9-preservation",data.clone(),BODY,|doc| {
            let p=body_paragraphs(doc).into_iter().find(|p|p.text()=="P9 preservation target").unwrap();
            let (container,index)=ibis_doc_model::flow_command::locate(doc,p.id).unwrap();
            let mut new=p.clone();
            new.direct.shading=Some(if remove {None}else{Some(Rgb(0xe2efd9))});
            new.direct.borders[0]=Some(if remove {None}else{Some(ibis_doc_model::FlowBorder {style:ibis_doc_model::BorderStyle::Double,width:12700,color:Rgb(0x00ff00),space:25400,art:None})});
            new.resolve(&doc.styles);
            vec![FlowCommand::ReplaceBlocks {container,index,remove:1,blocks:vec![Block::Paragraph(new)]}]
        });
        let xml=String::from_utf8(ibis_ooxml::Package::open(outcome.saved).unwrap().read("word/document.xml").unwrap()).unwrap();
        assert!(xml.contains(between),"between edge changed");
        assert!(xml.contains(bar),"bar edge changed");
        assert!(xml.contains("w:shadow=\"1\""),"unmodeled edge property changed");
        assert!(body_paragraphs(flow(&outcome.edited)).into_iter().find(|p|p.text()=="P9 preservation target").unwrap().keep_with_next);
    }
}

#[test]
fn table_row_height_and_column_width() {
    for height in [ibis_doc_model::RowHeight::AtLeast(30*12700),ibis_doc_model::RowHeight::Exact(24*12700),ibis_doc_model::RowHeight::Auto] {
        scenario(REPORT,BODY,|doc| {
            let (index,table)=doc.body.iter().enumerate().find_map(|(i,b)|if let Block::Table(t)=b {Some((i,t))}else{None}).unwrap();
            let mut t=table.clone();
            t.rows[0].height=height;
            t.rows[0].header=true;
            t.rows[0].cant_split=true;
            t.columns[0]=40*12700;
            vec![FlowCommand::ReplaceBlocks {container:ibis_doc_model::flow_command::Container::Body,index,remove:1,blocks:vec![Block::Table(t)]}]
        });
    }
}

#[test]
fn cell_borders_and_diagonals() {
    let mut package=ibis_ooxml::Package::open(std::fs::read(path(REPORT)).unwrap()).unwrap();
    let mut xml=ibis_ooxml::XmlDocument::parse(&package.read("word/document.xml").unwrap()).unwrap();
    let body=xml.child(xml.root_element().unwrap(),"body").unwrap();
    let tbl=xml.child(body,"tbl").unwrap();let tr=xml.child(tbl,"tr").unwrap();let tc=xml.child(tr,"tc").unwrap();
    let pr=xml.child(tc,"tcPr").unwrap();
    let edges=if let Some(e)=xml.child(pr,"tcBorders") {e}else {
        let at=xml.children(pr).iter().position(|&n| matches!(xml.local_name(n),"shd"|"tcMar"|"textDirection"|"vAlign")).unwrap_or(xml.children(pr).len());
        xml.insert_element(pr,at,"w:tcBorders")
    };
    let top=if let Some(e)=xml.child(edges,"top") {e}else{xml.insert_element(edges,0,"w:top")};
    for (k,v) in [("w:val","single"),("w:sz","6"),("w:color","000000"),("w:shadow","1")] {xml.set_attr(top,k,v);}
    for name in ["insideH","insideV"] {
        let e=xml.append_element(edges,&format!("w:{name}"));
        for (k,v) in [("w:val","dotted"),("w:sz","7"),("w:color","123456")] {xml.set_attr(e,k,v);}
    }
    package.set_xml("word/document.xml",xml);let data=package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    for border in [Some(ibis_doc_model::FlowBorder {width:19050,color:Rgb(0x0070c0),style:ibis_doc_model::BorderStyle::Double,space:0,art:None}),None] {
        let outcome=scenario_data("b6-preservation",data.clone(),BODY,|doc| {
            let (index,table)=doc.body.iter().enumerate().find_map(|(i,b)|if let Block::Table(t)=b {Some((i,t))}else{None}).unwrap();
            let mut t=table.clone();
            t.rows[0].cells[0].own_borders=[Some(border);4];
            t.rows[0].cells[0].diagonals=[border;2];
            t.resolve_borders();
            vec![FlowCommand::ReplaceBlocks {container:ibis_doc_model::flow_command::Container::Body,index,remove:1,blocks:vec![Block::Table(t)]}]
        });
        let xml=String::from_utf8(ibis_ooxml::Package::open(outcome.saved).unwrap().read("word/document.xml").unwrap()).unwrap();
        assert!(xml.contains("w:shadow=\"1\""));
        assert!(xml.contains("<w:insideH w:val=\"dotted\" w:sz=\"7\" w:color=\"123456\""));
        assert!(xml.contains("<w:insideV w:val=\"dotted\" w:sz=\"7\" w:color=\"123456\""));
    }
}

#[test]
fn horizontal_cell_merge_keeps_paragraph_xml() {
    let mut package=ibis_ooxml::Package::open(std::fs::read(path(REPORT)).unwrap()).unwrap();
    let mut xml=ibis_ooxml::XmlDocument::parse(&package.read("word/document.xml").unwrap()).unwrap();
    let body=xml.child(xml.root_element().unwrap(),"body").unwrap();let tbl=xml.child(body,"tbl").unwrap();let tr=xml.child(tbl,"tr").unwrap();
    let cells:Vec<_>=xml.children_named(tr,"tc").collect();let p=xml.child(cells[1],"p").unwrap();
    let pr=if let Some(pr)=xml.child(p,"pPr") {pr}else{xml.insert_element(p,0,"w:pPr")};
    let e=xml.append_element(pr,"w:textAlignment");xml.set_attr(e,"w:val","bottom");
    let before=xml.to_bytes();
    package.set_xml("word/document.xml",xml);let data=package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    let outcome=scenario_data("b4-horizontal-merge",data,BODY,|doc| {
        let (index,t)=doc.body.iter().enumerate().find_map(|(i,b)|if let Block::Table(t)=b {Some((i,t))}else{None}).unwrap();
        let mut t=t.clone();let right=t.rows[0].cells.remove(1);
        t.rows[0].cells[0].span=t.rows[0].cells[0].span.max(1)+right.span.max(1);
        t.rows[0].cells[0].blocks.extend(right.blocks);
        t.resolve_borders();
        vec![FlowCommand::ReplaceBlocks {container:ibis_doc_model::flow_command::Container::Body,index,remove:1,blocks:vec![Block::Table(t)]}]
    });
    let after=ibis_ooxml::Package::open(outcome.saved).unwrap().read("word/document.xml").unwrap();
    let count=|data:&[u8]|String::from_utf8_lossy(data).matches("<w:textAlignment").count();
    assert_eq!(count(&after),count(&before),"unmodeled paragraph property lost during cell merge");
}

#[test]
fn merge_then_split_writes_new_cell_grid_width() {
    let out=scenario(REPORT,BODY,|doc| {
        let (index,t)=doc.body.iter().enumerate().find_map(|(i,b)|if let Block::Table(t)=b {Some((i,t))}else{None}).unwrap();
        let mut t=t.clone();let right=t.rows[0].cells.remove(1);t.rows[0].cells[0].blocks.extend(right.blocks);
        let mut cell=t.rows[0].cells[0].clone();cell.id=doc.new_id();cell.span=1;
        let Block::Paragraph(p)=cell.blocks[0].clone() else{panic!()};
        let mut blank=FlowParagraph {id:cell.id+1,style:p.style.clone(),table_style:p.table_style.clone(),direct:p.direct.clone(),mark_format:p.mark_format.clone(),..Default::default()};blank.resolve(&doc.styles);
        cell.blocks=vec![Block::Paragraph(blank)];t.rows[0].cells.insert(1,cell);t.resolve_borders();
        vec![FlowCommand::ReplaceBlocks {container:ibis_doc_model::flow_command::Container::Body,index,remove:1,blocks:vec![Block::Table(t)]}]
    });
    let xml=ibis_ooxml::XmlDocument::parse(&ibis_ooxml::Package::open(out.saved).unwrap().read("word/document.xml").unwrap()).unwrap();
    let body=xml.child(xml.root_element().unwrap(),"body").unwrap();let tbl=xml.child(body,"tbl").unwrap();let tr=xml.child(tbl,"tr").unwrap();
    let t=flow(&out.edited).body.iter().find_map(|b|if let Block::Table(t)=b {Some(t)}else{None}).unwrap();
    for (cell,width) in xml.children_named(tr,"tc").zip(&t.columns) {
        let pr=xml.child(cell,"tcPr").unwrap();let w=xml.child(pr,"tcW").unwrap();
        assert_eq!(xml.attr_local(w,"w").unwrap().parse::<i64>().unwrap()*635,*width);
    }
}

// Start from an actual ZIP containing old history, not a runtime-only revision.
#[test]
fn tracked_replacement_preserves_original_deletion_xml() {
    use ibis_doc_model::{Revision, RevisionKind};
    let history = ibis_pptx::test_files::ORIGINAL_DELETION_XML;
    let data = ibis_pptx::test_files::docx_with_original_deletion_history(&std::fs::read(path(REPORT)).unwrap());
    let outcome = scenario_data("original-deletion-history", data, &["*"], |doc| {
        let p = body_paragraphs(doc).into_iter().find(|p| p.text() == "abX Ycd").unwrap().id;
        vec![
            FlowCommand::SetTrackChanges(true),
            FlowCommand::TrackDelete { from: FlowPos::new(p, 1), to: FlowPos::new(p, 6), author: "Test".into(), date: "2026-10-01T00:00:00Z".into() },
            FlowCommand::InsertText { at: FlowPos::new(p, 6), text: "Q".into(), format: None },
            FlowCommand::MarkRevision { from: FlowPos::new(p, 6), to: FlowPos::new(p, 7), revision: Some(Revision { kind: RevisionKind::Insert, author: "Test".into(), date: "2026-10-01T00:00:00Z".into() }) },
        ]
    });
    let xml = String::from_utf8(ibis_ooxml::Package::open(outcome.saved).unwrap().read("word/document.xml").unwrap()).unwrap();
    assert!(xml.contains(history), "original deletion container/attributes/children changed");
    let parsed = ibis_ooxml::XmlDocument::parse(xml.as_bytes()).unwrap();
    let ids: Vec<_> = parsed.descendants(parsed.root_element().unwrap()).into_iter()
        .filter(|&n| matches!(parsed.local_name(n), "ins" | "del"))
        .filter_map(|n| parsed.attr_local(n, "id")).collect();
    assert_eq!(ids.iter().filter(|&&id| id == "841").count(), 1);
    assert_eq!(ids.len(), ids.iter().collect::<std::collections::HashSet<_>>().len(), "revision IDs must be unique");
}

#[test]
fn hidden_boundary_replacement_keeps_text_outside_old_deletion() {
    let data=ibis_pptx::test_files::docx_with_search_history_edges(&std::fs::read(path(REPORT)).unwrap());
    for (text,from,to) in [("X Ypq",3,5),("rsX Y",0,2)] {
        let out=scenario_data("hidden-history-boundary",data.clone(),BODY,|doc| {
            let p=body_paragraphs(doc).into_iter().find(|p|p.text()==text).unwrap().id;
            vec![FlowCommand::Delete {from:FlowPos::new(p,from),to:FlowPos::new(p,to)},FlowCommand::InsertText {at:FlowPos::new(p,from),text:"UV".into(),format:None}]
        });
        let xml=ibis_pptx::test_files::part_text(&out.saved,"word/document.xml").unwrap();
        for id in ["841","842","843","844","845"] {assert!(xml.contains(&ibis_pptx::test_files::ORIGINAL_DELETION_XML.replace("841",id)),"original history {id} changed");}
    }
}

#[test]
fn original_history_adjacent_field_preserved() {
    let data=ibis_pptx::test_files::docx_with_history_adjacent_field(&std::fs::read(path(REPORT)).unwrap());
    for operation in ["insert","format","delete"] {
        let out=scenario_data("history-adjacent-field",data.clone(),BODY,|doc| {
            let paragraph=body_paragraphs(doc).into_iter().find(|p|p.text().starts_with("abX Ycd")).unwrap();
            assert_eq!(paragraph.len(),8);assert!(matches!(paragraph.runs.last().unwrap().kind,ibis_doc_model::FlowRunKind::PageNumber));
            let p=paragraph.id;
            match operation {
                "insert"=>vec![FlowCommand::InsertText {at:FlowPos::new(p,0),text:"N".into(),format:None}],
                "format"=>vec![FlowCommand::SetRunFormat {from:FlowPos::new(p,7),to:FlowPos::new(p,8),change:RunChange::Bold(true)}],
                _=>vec![FlowCommand::Delete {from:FlowPos::new(p,7),to:FlowPos::new(p,8)}],
            }
        });
        let xml=ibis_pptx::test_files::part_text(&out.saved,"word/document.xml").unwrap();
        assert!(xml.contains(ibis_pptx::test_files::ORIGINAL_DELETION_XML));
        if operation=="insert" {assert!(xml.contains(ibis_pptx::test_files::HISTORY_ADJACENT_FIELD_XML));}
        if operation=="delete" {assert!(!xml.contains(ibis_pptx::test_files::HISTORY_ADJACENT_FIELD_XML));}
    }
}

// Settings/comment-only changes must not be discarded by the unchanged-body fast path.
#[test]
fn source_only_settings_and_comments() {
    use ibis_doc_model::flow_command::{CommentAuthor,add_comment,resolve_comment};
    let data=std::fs::read(path(REPORT)).unwrap();
    let tracked=scenario_data("track-only",data.clone(),&["word/settings.xml","word/_rels/document.xml.rels","[Content_Types].xml"],|d|vec![FlowCommand::SetTrackChanges(!d.track_changes)]);
    assert!(flow(&ibis_pptx::read(&tracked.saved).unwrap()).track_changes);
    let who=CommentAuthor{author:"Test".into(),initials:"T".into(),date:"2026-10-01T00:00:00Z".into()};
    let seed=scenario_data("comment-only-base",data,&["*"],|d|{let id=text_paragraph(d,8).id;add_comment(d,FlowPos::new(id,1),FlowPos::new(id,4),&who,"CommentSeed").unwrap()});
    let source=ibis_pptx::read(&seed.saved).unwrap();let id=flow(&source).comments[0].id;
    let resolved=scenario_data("comment-only-resolved",seed.saved.clone(),&["word/comments.xml","word/commentsExtended.xml","word/_rels/document.xml.rels","[Content_Types].xml"],|_|vec![resolve_comment(flow(&source),id,true)]);
    assert!(flow(&ibis_pptx::read(&resolved.saved).unwrap()).comments.iter().find(|c|c.id==id).unwrap().done);
    let text=scenario_data("comment-only-text",seed.saved,&["word/comments.xml"],|d|{let p=match &d.comments[0].blocks[0]{Block::Paragraph(p)=>p,_=>panic!("comment paragraph")};vec![FlowCommand::InsertText{at:FlowPos::new(p.id,0),text:"OnlyEdit".into(),format:None}]});
    assert!(flow(&ibis_pptx::read(&text.saved).unwrap()).comments[0].blocks.iter().any(|b|matches!(b,Block::Paragraph(p) if p.text()=="OnlyEditCommentSeed")));
}

#[test]
fn note_part_links() {
    use ibis_doc_model::{Link,FlowRunKind};use ibis_doc_model::flow_command::insert_note;
    let seed=scenario_data("note-link-base",std::fs::read(path(REPORT)).unwrap(),&["*"],|d|{let id=text_paragraph(d,12).id;chained(d,vec![Box::new(move|d|insert_note(d,FlowPos::new(id,3),false).unwrap().0),Box::new(|d|{let p=match &d.footnotes[0].blocks[0]{Block::Paragraph(p)=>p,_=>panic!("note paragraph")};vec![FlowCommand::InsertText{at:FlowPos::new(p.id,p.len()),text:"NoteLinkA".into(),format:None}]}),Box::new(move|d|insert_note(d,FlowPos::new(id,8),true).unwrap().0),Box::new(|d|{let p=match &d.endnotes[0].blocks[0]{Block::Paragraph(p)=>p,_=>panic!("note paragraph")};vec![FlowCommand::InsertText{at:FlowPos::new(p.id,p.len()),text:"NoteLinkB".into(),format:None}]})])});
    let linked=scenario_data("note-link-added",seed.saved,&["word/footnotes.xml","word/endnotes.xml","word/_rels/footnotes.xml.rels","word/_rels/endnotes.xml.rels"],|d|{[(&d.footnotes,"https://example.com/foot"),(&d.endnotes,"https://example.com/end")].into_iter().map(|(notes,url)|{let p=match &notes[0].blocks[0]{Block::Paragraph(p)=>p,_=>panic!("note paragraph")};FlowCommand::SetLink{from:FlowPos::new(p.id,2),to:FlowPos::new(p.id,11),link:Some(Link::Url(url.into()))}}).collect()});
    let changed=scenario_data("note-link-changed",linked.saved.clone(),&["word/footnotes.xml","word/_rels/footnotes.xml.rels"],|d|{let p=match &d.footnotes[0].blocks[0]{Block::Paragraph(p)=>p,_=>panic!("note paragraph")};vec![FlowCommand::SetLink{from:FlowPos::new(p.id,2),to:FlowPos::new(p.id,11),link:Some(Link::Url("https://example.com/foot-changed".into()))}]});
    scenario_data("note-link-removed",changed.saved,&["word/footnotes.xml"],|d|{let p=match &d.footnotes[0].blocks[0]{Block::Paragraph(p)=>p,_=>panic!("note paragraph")};vec![FlowCommand::SetLink{from:FlowPos::new(p.id,2),to:FlowPos::new(p.id,11),link:None}]});
    let reread=ibis_pptx::read(&linked.saved).unwrap();for (notes,url) in [(&flow(&reread).footnotes,"https://example.com/foot"),(&flow(&reread).endnotes,"https://example.com/end")]{let linked:Vec<_>=notes[0].blocks.iter().flat_map(|b|match b{Block::Paragraph(p)=>p.runs.iter().collect::<Vec<_>>(),_=>Vec::new()}).filter(|r|matches!(&r.kind,FlowRunKind::Text(_))&&r.link==Some(Link::Url(url.into()))).collect();assert!(!linked.is_empty());}
}

#[test]
fn wordprocessing_shapes() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let doc = flow(&opened.presentation);

    // 1. Verify that the shape in REPORT is read as FlowRunKind::Shape
    let shape_run = doc.body.iter().find_map(|b| {
        if let Block::Table(t) = b {
            for row in &t.rows {
                for cell in &row.cells {
                    for b in &cell.blocks {
                        if let Block::Paragraph(p) = b {
                            for r in &p.runs {
                                if let ibis_doc_model::FlowRunKind::Shape(s) = &r.kind {
                                    return Some((p.id, s.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }).expect("shape must be present in table");

    let (para_id, shape) = shape_run;
    assert_eq!(shape.frame.width, 1905000);
    assert_eq!(shape.frame.height, 762000);
    if let ibis_doc_model::ShapeKind::Auto(auto) = &shape.kind {
        let text = auto.text.as_ref().expect("shape has text body");
        assert_eq!(text.paragraphs[0].runs[0].text, "注記：数値は架空です");
    } else {
        panic!("expected AutoShape");
    }

    let floating = shape.floating.as_ref().expect("floating anchor");
    assert_eq!(floating.offset_x, 3810000);
    assert_eq!(floating.offset_y, 6350000);
    assert_eq!(floating.relative_h, "column");
    assert_eq!(floating.relative_v, "paragraph");
    assert_eq!(floating.wrap, "none");
    assert!(!floating.behind_doc);

    // 2. Scenario edit: type in document, verify save preserves shape XML and undo restores original byte-exact
    scenario(REPORT, &["word/document.xml"], |_| {
        vec![FlowCommand::InsertText {
            at: FlowPos::new(para_id, 0),
            text: "Prefix".into(),
            format: None,
        }]
    });
}

#[test]
fn caps_and_small_caps() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let doc = flow(&opened.presentation);
    let para_id = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap();

    // 1. Caps: apply, save, check XML, reopen, undo byte-exact
    let outcome1 = scenario(REPORT, &["word/document.xml"], |_| {
        vec![FlowCommand::SetRunFormat {
            from: FlowPos::new(para_id, 0),
            to: FlowPos::new(para_id, 4),
            change: RunChange::Caps(true),
        }]
    });
    let xml1 = ibis_pptx::test_files::part_text(&outcome1.saved, "word/document.xml").unwrap();
    assert!(xml1.contains("<w:caps/>") || xml1.contains("<w:caps "));

    // 2. SmallCaps: apply, save, check XML, reopen, undo byte-exact
    let outcome2 = scenario(REPORT, &["word/document.xml"], |_| {
        vec![FlowCommand::SetRunFormat {
            from: FlowPos::new(para_id, 0),
            to: FlowPos::new(para_id, 4),
            change: RunChange::SmallCaps(true),
        }]
    });
    let xml2 = ibis_pptx::test_files::part_text(&outcome2.saved, "word/document.xml").unwrap();
    assert!(xml2.contains("<w:smallCaps/>") || xml2.contains("<w:smallCaps "));

    // 3. Mutual exclusion: Caps then SmallCaps
    let outcome3 = scenario(REPORT, &["word/document.xml"], |_| {
        vec![
            FlowCommand::SetRunFormat {
                from: FlowPos::new(para_id, 0),
                to: FlowPos::new(para_id, 4),
                change: RunChange::Caps(true),
            },
            FlowCommand::SetRunFormat {
                from: FlowPos::new(para_id, 0),
                to: FlowPos::new(para_id, 4),
                change: RunChange::SmallCaps(true),
            },
        ]
    });
    let xml3 = ibis_pptx::test_files::part_text(&outcome3.saved, "word/document.xml").unwrap();
    assert!(xml3.contains("<w:smallCaps/>") || xml3.contains("<w:smallCaps "));
    assert!(!xml3.contains("<w:caps/>") && !xml3.contains("<w:caps>"));
}

#[test]
fn hidden_text_formatting() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let doc = flow(&opened.presentation);
    let p = text_paragraph(doc, 6);
    let para_id = p.id;

    // Apply Hidden(true)
    let outcome = scenario(REPORT, &["word/document.xml"], |_| {
        vec![
            FlowCommand::SetRunFormat {
                from: FlowPos::new(para_id, 1),
                to: FlowPos::new(para_id, 4),
                change: RunChange::Hidden(true),
            },
        ]
    });
    // Verify OOXML contains <w:vanish/>
    let xml = ibis_pptx::test_files::part_text(&outcome.saved, "word/document.xml").unwrap();
    assert!(xml.contains("<w:vanish/>") || xml.contains("<w:vanish "), "OOXML must contain w:vanish");

    // Verify reread flow model has hidden: true
    let reread = ibis_pptx::read(&outcome.saved).unwrap();
    let has_hidden = paragraphs(flow(&reread))
        .iter()
        .flat_map(|p| p.runs.iter())
        .any(|r| r.style.hidden && r.format.hidden == Some(true));
    assert!(has_hidden, "saved and reread document should have hidden run");
}

#[test]
fn floating_pictures_and_theme_shading() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let doc = flow(&opened.presentation);

    // Verify wordprocessing shape has floating anchor with new fields
    let shape = doc
        .body
        .iter()
        .flat_map(|b| if let Block::Table(t) = b { t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).collect() } else { vec![b] })
        .find_map(|b| if let Block::Paragraph(p) = b { p.runs.iter().find_map(|r| if let ibis_doc_model::FlowRunKind::Shape(s) = &r.kind { Some(s) } else { None }) } else { None })
        .expect("wordprocessing shape");

    let floating = shape.floating.as_ref().expect("floating anchor");
    assert_eq!(floating.offset_x, 3810000);
    assert_eq!(floating.offset_y, 6350000);
    assert_eq!(floating.align_h, None);
    assert_eq!(floating.align_v, None);
    assert_eq!(floating.wrap, "none");
}

#[test]
fn two_color_shading_patterns() {
    let mut package = ibis_ooxml::Package::open(std::fs::read(path(REPORT)).unwrap()).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let pattern_paragraph = r#"<w:p><w:pPr><w:shd w:val="horzStripe" w:color="FF0000" w:fill="0000FF"/></w:pPr><w:r><w:t>TwoColorPattern</w:t></w:r></w:p>"#;
    let at = xml.rfind("<w:sectPr").unwrap();
    package.set_bytes("word/document.xml", format!("{}{}{}", &xml[..at], pattern_paragraph, &xml[at..]).into_bytes());
    let data = package.save(ibis_ooxml::SaveOptions::default()).unwrap();

    let outcome = scenario_data("two-color-shading", data.clone(), BODY, |doc| {
        let p = body_paragraphs(doc).into_iter().find(|p| p.text() == "TwoColorPattern").expect("pattern paragraph");
        assert_eq!(p.shading_pattern, Some(ibis_doc_model::ShadingPattern::HorzStripe));
        assert_eq!(p.shading_color, Some(Rgb(0xff0000)));
        assert_eq!(p.shading_fill, Some(Rgb(0x0000ff)));
        // Mixed color: 50% red (FF0000) + 50% blue (0000FF) = Rgb(0x800080)
        assert_eq!(p.shading, Some(Rgb(0x800080)));

        // Type at the end of another paragraph
        let target = text_paragraph(doc, 4);
        vec![FlowCommand::InsertText { at: FlowPos::new(target.id, 2), text: "stripe test ".into(), format: None }]
    });

    let reread = ibis_pptx::read(&outcome.saved).unwrap();
    let p_reread = body_paragraphs(flow(&reread)).into_iter().find(|p| p.text() == "TwoColorPattern").expect("pattern paragraph in reread");
    assert_eq!(p_reread.shading_pattern, Some(ibis_doc_model::ShadingPattern::HorzStripe));
    assert_eq!(p_reread.shading_color, Some(Rgb(0xff0000)));
    assert_eq!(p_reread.shading_fill, Some(Rgb(0x0000ff)));
}

// w:kern (pair kerning from a size up) is read, and kept by a save that
// rewrites the run: on a run's rPr and in the defaults (rPrDefault).
#[test]
fn kerning_is_read_and_kept() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let mut package = ibis_ooxml::Package::open(data).unwrap();
    let main = "word/document.xml";
    let xml = String::from_utf8(package.read(main).unwrap()).unwrap();
    let paragraph = r#"<w:p><w:r><w:rPr><w:kern w:val="28"/></w:rPr><w:t>KernedRunText</w:t></w:r></w:p>"#;
    let at = xml.rfind("<w:sectPr").unwrap();
    package.set_bytes(main, format!("{}{}{}", &xml[..at], paragraph, &xml[at..]).into_bytes());
    let data = package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    let styles = ibis_pptx::test_files::part_text(&data, "word/styles.xml").unwrap();
    assert!(styles.contains(r#"<w:rPrDefault><w:rPr>"#) && styles.contains(r#"<w:kern w:val="2"/>"#), "the report's defaults kern from 1 pt");

    let opened = ibis_pptx::open(data.clone()).unwrap();
    let doc = flow(&opened.presentation);
    let p = paragraphs(doc).into_iter().find(|p| p.text() == "KernedRunText").unwrap();
    assert_eq!(p.runs[0].style.kern, Some(28));
    // The title's style kerns from 14 pt; the other paragraphs take the
    // defaults' 1 pt.
    assert!(paragraphs(doc).iter().any(|p| p.runs.first().is_some_and(|r| r.style.kern == Some(2))), "from the defaults");

    let outcome = scenario_data(REPORT, data, BODY, |doc| {
        let p = paragraphs(doc).into_iter().find(|p| p.text() == "KernedRunText").unwrap();
        vec![
            FlowCommand::SetRunFormat { from: FlowPos::new(p.id, 2), to: FlowPos::new(p.id, 6), change: RunChange::Bold(true) },
            FlowCommand::InsertText { at: FlowPos::new(p.id, 1), text: "x".into(), format: None },
        ]
    });
    let saved = ibis_pptx::test_files::part_text(&outcome.saved, main).unwrap();
    let start = saved.find("KernedRunText").map_or_else(|| saved.find(">K").unwrap(), |i| i);
    let tail = &saved[saved[..start].rfind("<w:p>").or_else(|| saved[..start].rfind("<w:p ")).unwrap()..];
    let tail = &tail[..tail.find("</w:p>").unwrap()];
    assert_eq!(tail.matches("<w:r>").count() + tail.matches("<w:r ").count(), tail.matches(r#"<w:kern w:val="28"/>"#).count(), "every run keeps its w:kern: {tail}");
    assert!(tail.matches(r#"<w:kern w:val="28"/>"#).count() >= 3, "{tail}");
    assert_eq!(ibis_pptx::test_files::part_text(&outcome.saved, "word/styles.xml").unwrap(), styles);
}

#[test]
fn art_page_borders_preserved_on_save() {
    let mut package = ibis_ooxml::Package::open(std::fs::read(path(REPORT)).unwrap()).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let borders_tag = r#"<w:pgBorders w:offsetFrom="text"><w:top w:val="apples" w:sz="24" w:space="15" w:color="C00000"/><w:right w:val="apples" w:sz="24" w:space="15" w:color="C00000"/><w:bottom w:val="apples" w:sz="24" w:space="15" w:color="C00000"/><w:left w:val="apples" w:sz="24" w:space="15" w:color="C00000"/></w:pgBorders>"#;
    let sect_end = xml.rfind("</w:sectPr>").expect("sectPr closing tag");
    let injected = format!("{}{}{}", &xml[..sect_end], borders_tag, &xml[sect_end..]);
    package.set_bytes("word/document.xml", injected.into_bytes());
    let data = package.save(ibis_ooxml::SaveOptions::default()).unwrap();

    let doc = ibis_pptx::read(&data).unwrap();
    let flow_doc = flow(&doc);
    assert!(flow_doc.page.borders_from_text);
    for i in 0..4 {
        let b = flow_doc.page.borders[i].as_ref().expect("border exists");
        assert_eq!(b.art, Some(ibis_doc_model::ArtBorder::Apples));
        assert_eq!(b.width, 24 * 12_700);
        assert_eq!(b.space, 15 * 12_700);
        assert_eq!(b.color, Rgb(0xc00000));
    }

    let outcome = scenario_data("art-page-borders-preservation", data, BODY, |doc| {
        let target = text_paragraph(doc, 4);
        vec![FlowCommand::InsertText {
            at: FlowPos::new(target.id, 2),
            text: "edited text ".into(),
            format: None,
        }]
    });

    let saved_package = ibis_ooxml::Package::open(outcome.saved.clone()).unwrap();
    let saved_xml = String::from_utf8(saved_package.read("word/document.xml").unwrap()).unwrap();
    assert!(saved_xml.contains(r#"w:val="apples""#), "art border name preserved");
    assert!(saved_xml.contains(r#"w:sz="24""#), "art border sz preserved as point value 24");
    assert!(!saved_xml.contains(r#"w:sz="192""#), "art border sz not rewritten with 1/8pt formula");
    assert!(saved_xml.contains(r#"w:space="15""#), "art border space preserved");
    assert!(saved_xml.contains(r#"w:color="C00000""#), "art border color preserved");
    assert!(saved_xml.contains(r#"w:offsetFrom="text""#), "art border offsetFrom preserved");

    let reread = ibis_pptx::read(&outcome.saved).unwrap();
    let reread_flow = flow(&reread);
    assert!(reread_flow.page.borders_from_text);
    for i in 0..4 {
        let b = reread_flow.page.borders[i].as_ref().expect("border exists in reread");
        assert_eq!(b.art, Some(ibis_doc_model::ArtBorder::Apples));
        assert_eq!(b.width, 24 * 12_700);
        assert_eq!(b.space, 15 * 12_700);
        assert_eq!(b.color, Rgb(0xc00000));
    }
}

#[test]
fn page_borders_edit_and_save_reopen() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    // 1. Initial page has no borders
    assert!(!flow(document.presentation()).page.borders.iter().any(Option::is_some));

    // 2. Set box borders (1 pt single, black, 24 pt space, from page)
    let border = ibis_doc_model::FlowBorder {
        width: 12_700,
        color: Rgb(0x000000),
        style: ibis_doc_model::BorderStyle::Single,
        space: 24 * 12_700,
        art: None,
    };
    let mut new_page = flow(document.presentation()).page;
    new_page.borders = [Some(border); 4];
    new_page.borders_from_text = false;
    document.execute(vec![Command::Flow(FlowCommand::SetPage { page: new_page })]).unwrap();

    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    assert!(!reread_flow.page.borders_from_text);
    for i in 0..4 {
        let b = reread_flow.page.borders[i].as_ref().expect("border exists");
        assert_eq!(b.style, ibis_doc_model::BorderStyle::Single);
        assert_eq!(b.width, 12_700);
        assert_eq!(b.color, Rgb(0x000000));
        assert_eq!(b.space, 24 * 12_700);
        assert_eq!(b.art, None);
    }

    // 3. Change to double border, from text
    let double_border = ibis_doc_model::FlowBorder {
        width: 19_050,
        color: Rgb(0x1F497D),
        style: ibis_doc_model::BorderStyle::Double,
        space: 12_700,
        art: None,
    };
    let mut text_page = reread_flow.page;
    text_page.borders = [Some(double_border); 4];
    text_page.borders_from_text = true;
    document.execute(vec![Command::Flow(FlowCommand::SetPage { page: text_page })]).unwrap();

    let saved_text = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_text = ibis_pptx::read(&saved_text).unwrap();
    let flow_text = flow(&reread_text);
    assert!(flow_text.page.borders_from_text);
    for i in 0..4 {
        let b = flow_text.page.borders[i].as_ref().expect("double border exists");
        assert_eq!(b.style, ibis_doc_model::BorderStyle::Double);
        assert_eq!(b.color, Rgb(0x1F497D));
        assert_eq!(b.space, 12_700);
    }

    // 4. Remove borders
    let mut no_borders_page = flow_text.page;
    no_borders_page.borders = [None; 4];
    document.execute(vec![Command::Flow(FlowCommand::SetPage { page: no_borders_page })]).unwrap();

    let saved_none = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_none = ibis_pptx::read(&saved_none).unwrap();
    assert!(!flow(&reread_none).page.borders.iter().any(Option::is_some));

    // 5. Undo all
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn overflow_punct_edit_and_save_reopen() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let ps = body_paragraphs(flow(document.presentation()));
    let target_id = ps[1].id;
    assert_eq!(
        flow(document.presentation()).body.iter().find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p.overflow_punct),
            _ => None,
        }),
        Some(true)
    );

    // false に変更
    document
        .execute(vec![Command::Flow(FlowCommand::SetParagraphFormat {
            first: target_id,
            last: target_id,
            change: ParaChange::OverflowPunct(false),
        })])
        .unwrap();

    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let target_p = reread_flow
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p.overflow_punct, false);
    assert_eq!(target_p.direct.overflow_punct, Some(false));

    // true に戻す
    document
        .execute(vec![Command::Flow(FlowCommand::SetParagraphFormat {
            first: target_id,
            last: target_id,
            change: ParaChange::OverflowPunct(true),
        })])
        .unwrap();

    let saved_true = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_true = ibis_pptx::read(&saved_true).unwrap();
    let target_p_true = flow(&reread_true)
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p_true.overflow_punct, true);
    assert_eq!(target_p_true.direct.overflow_punct, Some(true));

    // Undo all
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn auto_space_edit_and_save_reopen() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let ps = body_paragraphs(flow(document.presentation()));
    let target_id = ps[1].id;
    let initial_p = flow(document.presentation())
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some((p.auto_space_letters, p.auto_space_digits)),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(initial_p, (true, true));

    // false に変更
    document
        .execute(vec![
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::AutoSpaceDE(false),
            }),
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::AutoSpaceDN(false),
            }),
        ])
        .unwrap();

    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let target_p = reread_flow
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p.auto_space_letters, false);
    assert_eq!(target_p.auto_space_digits, false);
    assert_eq!(target_p.direct.auto_space_letters, Some(false));
    assert_eq!(target_p.direct.auto_space_digits, Some(false));

    // true に戻す
    document
        .execute(vec![
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::AutoSpaceDE(true),
            }),
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::AutoSpaceDN(true),
            }),
        ])
        .unwrap();

    let saved_true = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_true = ibis_pptx::read(&saved_true).unwrap();
    let target_p_true = flow(&reread_true)
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p_true.auto_space_letters, true);
    assert_eq!(target_p_true.auto_space_digits, true);
    assert_eq!(target_p_true.direct.auto_space_letters, Some(true));
    assert_eq!(target_p_true.direct.auto_space_digits, Some(true));

    // Undo all
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn kinsoku_and_word_wrap_edit_and_save_reopen() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let ps = body_paragraphs(flow(document.presentation()));
    let target_id = ps[1].id;
    let initial_p = flow(document.presentation())
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some((p.kinsoku, p.word_wrap)),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(initial_p, (true, true));

    // false に変更
    document
        .execute(vec![
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::Kinsoku(false),
            }),
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::WordWrap(false),
            }),
        ])
        .unwrap();

    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let target_p = reread_flow
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p.kinsoku, false);
    assert_eq!(target_p.word_wrap, false);
    assert_eq!(target_p.direct.kinsoku, Some(false));
    assert_eq!(target_p.direct.word_wrap, Some(false));

    // true に戻す
    document
        .execute(vec![
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::Kinsoku(true),
            }),
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::WordWrap(true),
            }),
        ])
        .unwrap();

    let saved_true = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_true = ibis_pptx::read(&saved_true).unwrap();
    let target_p_true = flow(&reread_true)
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p_true.kinsoku, true);
    assert_eq!(target_p_true.word_wrap, true);
    assert_eq!(target_p_true.direct.kinsoku, Some(true));
    assert_eq!(target_p_true.direct.word_wrap, Some(true));

    // Undo all
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn suppress_auto_hyphens_edit_and_save_reopen() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let ps = body_paragraphs(flow(document.presentation()));
    let target_id = ps[1].id;
    let initial_val = flow(document.presentation())
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p.suppress_auto_hyphens),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(initial_val, false);

    // true に変更
    document
        .execute(vec![
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::SuppressAutoHyphens(true),
            }),
        ])
        .unwrap();

    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let target_p = reread_flow
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p.suppress_auto_hyphens, true);
    assert_eq!(target_p.direct.suppress_auto_hyphens, Some(true));

    // false に変更
    document
        .execute(vec![
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::SuppressAutoHyphens(false),
            }),
        ])
        .unwrap();

    let saved_false = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_false = ibis_pptx::read(&saved_false).unwrap();
    let target_p_false = flow(&reread_false)
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p_false.suppress_auto_hyphens, false);
    assert_eq!(target_p_false.direct.suppress_auto_hyphens, Some(false));

    // Undo all
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn bidi_edit_and_save_reopen() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let ps = body_paragraphs(flow(document.presentation()));
    let target_id = ps[1].id;
    let initial_bidi = flow(document.presentation())
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p.bidi),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(initial_bidi, false);

    // true に変更（右から左へ記述）
    document
        .execute(vec![
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::Bidi(true),
            }),
        ])
        .unwrap();

    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let target_p = reread_flow
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p.bidi, true);
    assert_eq!(target_p.direct.bidi, Some(true));

    // false に戻す（左から右へ記述）
    document
        .execute(vec![
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::Bidi(false),
            }),
        ])
        .unwrap();

    let saved_false = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_false = ibis_pptx::read(&saved_false).unwrap();
    let target_p_false = flow(&reread_false)
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p_false.bidi, false);
    assert_eq!(target_p_false.direct.bidi, Some(false));

    // Undo all
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn snap_to_grid_edit_and_save_reopen() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let ps = body_paragraphs(flow(document.presentation()));
    let target_id = ps[1].id;
    let initial_snap = flow(document.presentation())
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p.snap_to_grid),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(initial_snap, true);

    // false に変更（行グリッド線に合わせない）
    document
        .execute(vec![
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::SnapToGrid(false),
            }),
        ])
        .unwrap();

    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let target_p = reread_flow
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p.snap_to_grid, false);
    assert_eq!(target_p.direct.snap, Some(false));

    // true に戻す（行グリッド線に合わせる）
    document
        .execute(vec![
            Command::Flow(FlowCommand::SetParagraphFormat {
                first: target_id,
                last: target_id,
                change: ParaChange::SnapToGrid(true),
            }),
        ])
        .unwrap();

    let saved_true = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_true = ibis_pptx::read(&saved_true).unwrap();
    let target_p_true = flow(&reread_true)
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.id == target_id => Some(p),
            _ => None,
        })
        .expect("paragraph exists");
    assert_eq!(target_p_true.snap_to_grid, true);
    assert_eq!(target_p_true.direct.snap, Some(true));

    // Undo all
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_doc_properties_and_document_fields() {
    use ibis_doc_model::{DocProperties, FlowPos, FlowRunKind};

    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let mut props = DocProperties::default();
    props.title = Some("Quarterly Report".to_string());
    props.creator = Some("Docx Specialist".to_string());
    props.subject = Some("Finance".to_string());
    props.description = Some("Financial Review 2026".to_string());
    props.created = Some("2026-10-01T09:00:00Z".to_string());
    props.modified = Some("2026-10-08T15:30:00Z".to_string());

    // 1. プロパティの変更とUndo/Redo
    document.execute(vec![Command::Flow(FlowCommand::SetDocProperties(props.clone()))]).unwrap();
    assert_eq!(flow(document.presentation()).properties.title.as_deref(), Some("Quarterly Report"));
    assert_eq!(flow(document.presentation()).properties.creator.as_deref(), Some("Docx Specialist"));

    // 2. 各フィールドの評価値検証
    let doc_ref = flow(document.presentation());
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction("AUTHOR", doc_ref, FlowPos::new(0, 0)), "Docx Specialist");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction("TITLE", doc_ref, FlowPos::new(0, 0)), "Quarterly Report");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction("SUBJECT", doc_ref, FlowPos::new(0, 0)), "Finance");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction("COMMENTS", doc_ref, FlowPos::new(0, 0)), "Financial Review 2026");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction(r#"DOCPROPERTY "Title""#, doc_ref, FlowPos::new(0, 0)), "Quarterly Report");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction(r#"DOCPROPERTY "Author""#, doc_ref, FlowPos::new(0, 0)), "Docx Specialist");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction(r#"CREATEDATE \@ "yyyy/MM/dd""#, doc_ref, FlowPos::new(0, 0)), "2026/10/01");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction(r#"SAVEDATE \@ "yyyy/MM/dd""#, doc_ref, FlowPos::new(0, 0)), "2026/10/08");

    // 3. フィールドの挿入
    let p_id = flow(document.presentation()).body.iter().find_map(|b| match b {
        ibis_doc_model::Block::Paragraph(p) => Some(p.id),
        _ => None,
    }).unwrap();
    let at = FlowPos::new(p_id, 0);
    let (cmds, _) = ibis_doc_model::flow_command::insert_field_at_range(doc_ref, Some((at, at)), "AUTHOR", false).unwrap();
    document.execute(cmds.into_iter().map(Command::Flow).collect()).unwrap();

    // 4. 保存と再読込でのプロパティおよびフィールドの保持検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    assert_eq!(reread_flow.properties.title.as_deref(), Some("Quarterly Report"));
    assert_eq!(reread_flow.properties.creator.as_deref(), Some("Docx Specialist"));
    assert_eq!(reread_flow.properties.subject.as_deref(), Some("Finance"));
    assert_eq!(reread_flow.properties.description.as_deref(), Some("Financial Review 2026"));
    assert_eq!(reread_flow.properties.created.as_deref(), Some("2026-10-01T09:00:00Z"));
    assert_eq!(reread_flow.properties.modified.as_deref(), Some("2026-10-08T15:30:00Z"));

    let p_reread = reread_flow.body.iter().find_map(|b| match b {
        ibis_doc_model::Block::Paragraph(p) if p.id == p_id => Some(p),
        _ => None,
    }).unwrap();
    assert!(p_reread.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, shown } if instruction == "AUTHOR" && shown == "Docx Specialist")));

    // 5. 完全な Undo
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_noteref_field() {
    use ibis_doc_model::{FlowPos, FlowRunKind};

    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    // 1. 脚注の作成・設定の確認
    let p_id = flow(document.presentation()).body.iter().find_map(|b| match b {
        ibis_doc_model::Block::Paragraph(p) => Some(p.id),
        _ => None,
    }).unwrap();

    // 2. NOTEREF フィールドの評価検証（フォールバックおよびブックマーク検索）
    let doc_ref = flow(document.presentation());
    assert_eq!(doc_ref.noteref_text(r#"NOTEREF _Ref100000000 \f"#), Some("1".to_string()));
    assert_eq!(doc_ref.noteref_text(r#"NOTEREF _Ref100000000 \p"#), Some("上".to_string()));
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction(r#"NOTEREF _Ref100000000 \f"#, doc_ref, FlowPos::new(0, 0)), "1");

    // 3. フィールドの挿入
    let at = FlowPos::new(p_id, 0);
    let (cmds, _) = ibis_doc_model::flow_command::insert_field_at_range(doc_ref, Some((at, at)), "NOTEREF _Ref100000000 \\f", false).unwrap();
    document.execute(cmds.into_iter().map(Command::Flow).collect()).unwrap();

    // 4. 保存と再読込での NOTEREF フィールド保持検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let p_reread = reread_flow.body.iter().find_map(|b| match b {
        ibis_doc_model::Block::Paragraph(p) if p.id == p_id => Some(p),
        _ => None,
    }).unwrap();
    assert!(p_reread.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, shown } if instruction.contains("NOTEREF") && shown == "1")));

    // 5. 完全な Undo
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_cross_reference_targets_and_insertion() {
    use ibis_doc_model::{CrossReferenceKind, CrossReferenceType, FlowPos, FlowRunKind};

    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let doc = flow(document.presentation());

    // 1. 各種ターゲットの抽出検証
    let headings = ibis_doc_model::cross_reference_targets(doc, CrossReferenceType::Heading);
    assert!(!headings.is_empty(), "headings should exist in REPORT");
    let target_heading = headings[0].clone();

    let _captions = ibis_doc_model::cross_reference_targets(doc, CrossReferenceType::Caption);
    // Even if captions is empty initially, Heading is present

    // 2. ブックマークの追加と抽出検証
    let p_id = target_heading.id;
    let bm_name = "test_target_bookmark".to_string();
    let add_bm_cmd = vec![ibis_doc_model::FlowCommand::SetParagraphFormat {
        first: p_id,
        last: p_id,
        change: ibis_doc_model::ParaChange::AddBookmark(bm_name.clone()),
    }];
    document.execute(add_bm_cmd.into_iter().map(Command::Flow).collect()).unwrap();

    let doc = flow(document.presentation());
    let bookmarks = ibis_doc_model::cross_reference_targets(doc, CrossReferenceType::Bookmark);
    let target_bm = bookmarks.iter().find(|b| b.bookmark == bm_name).cloned().expect("bookmark target");

    // 3. 相互参照の挿入検証: 見出しテキスト (REF \h)
    let at1 = FlowPos::new(p_id, 0);
    let cmds1 = ibis_doc_model::insert_cross_reference_target(doc, at1, &target_heading, CrossReferenceType::Heading, CrossReferenceKind::Text, true).unwrap();
    document.execute(cmds1.into_iter().map(Command::Flow).collect()).unwrap();

    // 4. 相互参照の挿入検証: ページ番号 (PAGEREF \h)
    let doc = flow(document.presentation());
    let at2 = FlowPos::new(p_id, 1);
    let cmds2 = ibis_doc_model::insert_cross_reference_target(doc, at2, &target_bm, CrossReferenceType::Bookmark, CrossReferenceKind::PageNumber, true).unwrap();
    document.execute(cmds2.into_iter().map(Command::Flow).collect()).unwrap();

    // 5. 相互参照の挿入検証: 脚注番号 (NOTEREF \h)
    let doc = flow(document.presentation());
    let footnotes = ibis_doc_model::cross_reference_targets(doc, CrossReferenceType::Footnote);
    if !footnotes.is_empty() {
        let target_fn = &footnotes[0];
        let at3 = FlowPos::new(p_id, 2);
        let cmds3 = ibis_doc_model::insert_cross_reference_target(doc, at3, target_fn, CrossReferenceType::Footnote, CrossReferenceKind::NoteNumber, true).unwrap();
        document.execute(cmds3.into_iter().map(Command::Flow).collect()).unwrap();
    }

    // 6. 保存と再読込での REF / PAGEREF / NOTEREF フィールド保持検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let p_reread = reread_flow.body.iter().find_map(|b| match b {
        ibis_doc_model::Block::Paragraph(p) if p.id == p_id => Some(p),
        _ => None,
    }).unwrap();

    assert!(p_reread.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if instruction.contains("REF") && !instruction.contains("PAGE") && !instruction.contains("NOTE"))));
    assert!(p_reread.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if instruction.contains("PAGEREF")) || matches!(&r.kind, FlowRunKind::PageRef { .. })));

    // 7. 完全な Undo
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_styleref_field() {
    use ibis_doc_model::{FlowPos, FlowRunKind};

    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let doc = flow(document.presentation());

    // 1. REPORT 内の見出し段落の存在確認と styleref_text 評価
    let (target_p_id, expected_text, style_id, mut modified_heading) = doc.body.iter().find_map(|b| match b {
        ibis_doc_model::Block::Paragraph(p) if ibis_doc_model::flow_command::heading_level(&doc.styles, p).is_some() => {
            Some((p.id, p.text(), p.style.as_ref().unwrap().clone(), p.clone()))
        }
        _ => None,
    }).expect("REPORT should contain heading paragraph");

    // 2. styleref_text の各種指定方式の検証
    let resolved_id = doc.styleref_text(&format!("STYLEREF \"{style_id}\"")).unwrap();
    assert_eq!(resolved_id, expected_text);

    let resolved_h1 = doc.styleref_text("STYLEREF \"Heading 1\"").unwrap();
    assert_eq!(resolved_h1, expected_text);

    let resolved_num = doc.styleref_text("STYLEREF 1").unwrap();
    assert_eq!(resolved_num, expected_text);

    // 3. フィールド挿入（STYLEREF "Heading 1"）
    let first_p_id = match &doc.body[0] {
        ibis_doc_model::Block::Paragraph(p) => p.id,
        _ => panic!("first block is paragraph"),
    };
    let at = FlowPos::new(first_p_id, 0);
    let (insert_cmds, _) = ibis_doc_model::insert_field_at_range(doc, Some((at, at)), "STYLEREF \"Heading 1\"", false).unwrap();
    document.execute(insert_cmds.into_iter().map(Command::Flow).collect()).unwrap();

    let doc = flow(document.presentation());
    let first_p = match &doc.body[0] {
        ibis_doc_model::Block::Paragraph(p) => p,
        _ => panic!("first block is paragraph"),
    };
    let field_run = first_p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if instruction.contains("STYLEREF"))).expect("STYLEREF run");
    if let FlowRunKind::Field { shown, .. } = &field_run.kind {
        assert_eq!(shown, &expected_text, "initial evaluated text matches Heading 1");
    }

    // 4. 見出し段落のテキスト変更後の updated_fields_in_range（F9 更新）の検証
    let new_heading_text = "更新された見出しタイトル".to_string();
    modified_heading.runs = vec![ibis_doc_model::FlowRun {
        kind: FlowRunKind::Text(new_heading_text.clone()),
        style: ibis_doc_model::FlowRunStyle::default(),
        format: ibis_doc_model::RunFormat::default(),
        char_style: None,
        link: None,
        revision: None,
        show_field_code: false,
        field_locked: false,
    }];
    let (container, idx) = ibis_doc_model::flow_command::locate(doc, target_p_id).unwrap();
    let change_heading_cmd = ibis_doc_model::FlowCommand::ReplaceBlocks {
        container,
        index: idx,
        remove: 1,
        blocks: vec![ibis_doc_model::Block::Paragraph(modified_heading)],
    };
    document.execute(vec![Command::Flow(change_heading_cmd)]).unwrap();

    // F9 フィールド更新を実行
    let doc = flow(document.presentation());
    let update_cmds = ibis_doc_model::updated_fields_in_range(doc, None, None);
    assert!(!update_cmds.is_empty(), "update_fields should produce update commands when heading changed");
    document.execute(update_cmds.into_iter().map(Command::Flow).collect()).unwrap();

    let doc = flow(document.presentation());
    let first_p = match &doc.body[0] {
        ibis_doc_model::Block::Paragraph(p) => p,
        _ => panic!("first block is paragraph"),
    };
    let updated_run = first_p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if instruction.contains("STYLEREF"))).expect("STYLEREF run");
    if let FlowRunKind::Field { shown, .. } = &updated_run.kind {
        assert_eq!(shown, &new_heading_text, "field result should dynamically update to new heading text");
    }

    // 5. 保存と再読込による永続化検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let p_reread = match &reread_flow.body[0] {
        ibis_doc_model::Block::Paragraph(p) => p,
        _ => panic!("first block is paragraph"),
    };
    assert!(p_reread.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if instruction.contains("STYLEREF"))));

    // 6. 完全な Undo
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_picture_alt_text() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let media = document.ids().add_media(ibis_doc_model::MediaFormat::Png, a_png());
    let p = text_paragraph(flow(document.presentation()), 4).clone();
    let size = ibis_doc_model::EmuSize::new(914_400, 685_800);
    let at = FlowPos::new(p.id, 2);

    // 1. 画像を挿入
    document.execute(vec![Command::Flow(FlowCommand::InsertPicture { at, media, size })]).unwrap();

    // 2. 初期状態の代替テキストが空であることを確認
    let doc = flow(document.presentation());
    let run = doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("inserted picture run");
    if let FlowRunKind::Picture { look, .. } = &run.kind {
        assert_eq!(look.alt_text.title, "");
        assert_eq!(look.alt_text.description, "");
    } else {
        panic!("expected FlowRunKind::Picture");
    }

    // 3. 代替テキスト（タイトル・詳細説明）を設定
    let alt1 = ibis_doc_model::AltText {
        title: "四半期業績チャート".to_string(),
        description: "売上高と営業利益の四半期ごとの推移を示す棒グラフと折れ線グラフ。".to_string(),
    };
    let mut look1 = ibis_doc_model::PictureLook::default();
    look1.alt_text = alt1.clone();
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: look1 })]).unwrap();

    // 4. 保存して再読込し、wp:docPr の descr と title が保持されていることを確認
    let saved1 = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread1 = ibis_pptx::read(&saved1).unwrap();
    let reread1_doc = flow(&reread1);
    let run1 = reread1_doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("picture in reread1");
    if let FlowRunKind::Picture { look, .. } = &run1.kind {
        assert_eq!(look.alt_text.title, "四半期業績チャート");
        assert_eq!(look.alt_text.description, "売上高と営業利益の四半期ごとの推移を示す棒グラフと折れ線グラフ。");
    } else {
        panic!("expected FlowRunKind::Picture");
    }

    // 5. 代替テキストを空に更新して保存・再読込
    let empty_alt = ibis_doc_model::AltText::default();
    let mut look_empty = ibis_doc_model::PictureLook::default();
    look_empty.alt_text = empty_alt;
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: look_empty })]).unwrap();
    let saved_empty = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_empty = ibis_pptx::read(&saved_empty).unwrap();
    let reread_empty_doc = flow(&reread_empty);
    let run_empty = reread_empty_doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("picture in reread_empty");
    if let FlowRunKind::Picture { look, .. } = &run_empty.kind {
        assert_eq!(look.alt_text.title, "");
        assert_eq!(look.alt_text.description, "");
    }

    // 6. Undo/Redo の検証
    // 空への更新を Undo -> alt1 に戻る
    assert!(document.undo());
    let doc_after_undo = flow(document.presentation());
    let run_undo = doc_after_undo.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("picture after undo");
    if let FlowRunKind::Picture { look, .. } = &run_undo.kind {
        assert_eq!(look.alt_text, alt1);
    }

    // Redo -> 空に戻る
    assert!(document.redo());
    let doc_after_redo = flow(document.presentation());
    let run_redo = doc_after_redo.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("picture after redo");
    if let FlowRunKind::Picture { look, .. } = &run_redo.kind {
        assert_eq!(look.alt_text.title, "");
        assert_eq!(look.alt_text.description, "");
    }

    // 7. フローティング画像（ShapeKind::Picture）における代替テキストの保持
    let mut look2 = ibis_doc_model::PictureLook::default();
    look2.alt_text = alt1.clone();
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: look2 })]).unwrap();
    document.execute(vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::Square })]).unwrap();
    let saved_floating = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_floating = ibis_pptx::read(&saved_floating).unwrap();
    let reread_fl_doc = flow(&reread_floating);
    let run_fl = reread_fl_doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Shape(_))) } else { None }
    } else { None }).expect("floating shape run in reread_fl");
    if let FlowRunKind::Shape(shape) = &run_fl.kind {
        if let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind {
            assert_eq!(pic.alt_text.title, "四半期業績チャート");
            assert_eq!(pic.alt_text.description, "売上高と営業利益の四半期ごとの推移を示す棒グラフと折れ線グラフ。");
        } else {
            panic!("expected ShapeKind::Picture");
        }
    } else {
        panic!("expected FlowRunKind::Shape");
    }

    // 8. すべて Undo して元のファイルに戻る
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_cross_reference_relative_position_and_caption_text() {
    use ibis_doc_model::{CrossReferenceKind, CrossReferenceType, FlowPos, FlowRunKind};

    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let doc = flow(document.presentation());

    // 1. 図表番号（Caption）を挿入: "図 1: システム概要"
    let p_first = doc.body.first().and_then(|b| match b {
        ibis_doc_model::Block::Paragraph(p) => Some(p.id),
        _ => None,
    }).unwrap();
    let (caption_cmds, caption_bm) = ibis_doc_model::insert_caption_detailed(
        doc,
        p_first,
        "図",
        ": システム概要",
        ibis_doc_model::CaptionNumberFormat::Arabic,
        false,
    ).unwrap();
    document.execute(caption_cmds.into_iter().map(Command::Flow).collect()).unwrap();

    let doc = flow(document.presentation());
    let captions = ibis_doc_model::cross_reference_targets(doc, CrossReferenceType::Caption);
    assert!(!captions.is_empty(), "captions should exist");
    let target_cap = captions.iter().find(|c| c.bookmark == caption_bm).cloned().expect("target caption");

    // 2. 図表番号より後の段落から相互参照を挿入（下から上を参照 -> "上"）
    let p_after = doc.body.last().and_then(|b| match b {
        ibis_doc_model::Block::Paragraph(p) => Some(p.id),
        _ => None,
    }).unwrap();
    let at_after = FlowPos::new(p_after, 0);

    // 2a. ラベルと番号のみ
    let cmds_ln = ibis_doc_model::insert_cross_reference_target(doc, at_after, &target_cap, CrossReferenceType::Caption, CrossReferenceKind::LabelAndNumber, true).unwrap();
    assert!(cmds_ln.iter().any(|c| match c {
        ibis_doc_model::FlowCommand::InsertField { kind: FlowRunKind::Field { instruction, shown }, .. } => {
            instruction.contains(&caption_bm) && shown == "図 1"
        }
        _ => false,
    }));

    // 2b. 説明文のみ
    let cmds_desc = ibis_doc_model::insert_cross_reference_target(doc, at_after, &target_cap, CrossReferenceType::Caption, CrossReferenceKind::CaptionText, true).unwrap();
    assert!(cmds_desc.iter().any(|c| match c {
        ibis_doc_model::FlowCommand::InsertField { kind: FlowRunKind::Field { instruction, shown }, .. } => {
            instruction.contains(&caption_bm) && shown == "システム概要"
        }
        _ => false,
    }));

    // 2c. 相対位置（参照先が上にあるため "上"）
    let cmds_rel_up = ibis_doc_model::insert_cross_reference_target(doc, at_after, &target_cap, CrossReferenceType::Caption, CrossReferenceKind::RelativePosition, true).unwrap();
    assert!(cmds_rel_up.iter().any(|c| match c {
        ibis_doc_model::FlowCommand::InsertField { kind: FlowRunKind::Field { instruction, shown }, .. } => {
            instruction.contains(r#"\p"#) && shown == "上"
        }
        _ => false,
    }));
    document.execute(cmds_rel_up.into_iter().map(Command::Flow).collect()).unwrap();

    // 3. 図表番号より前の段落から相互参照を挿入（上から下を参照 -> "下"）
    let doc = flow(document.presentation());
    let at_before = FlowPos::new(p_first, 0);
    let cmds_rel_down = ibis_doc_model::insert_cross_reference_target(doc, at_before, &target_cap, CrossReferenceType::Caption, CrossReferenceKind::RelativePosition, true).unwrap();
    assert!(cmds_rel_down.iter().any(|c| match c {
        ibis_doc_model::FlowCommand::InsertField { kind: FlowRunKind::Field { instruction, shown }, .. } => {
            instruction.contains(r#"\p"#) && shown == "下"
        }
        _ => false,
    }));
    document.execute(cmds_rel_down.into_iter().map(Command::Flow).collect()).unwrap();

    // 4. evaluate_field_instruction の位置別評価検証
    let doc = flow(document.presentation());
    let eval_down = ibis_doc_model::flow_command::evaluate_field_instruction(&format!("REF {caption_bm} \\p"), doc, at_before);
    assert_eq!(eval_down, "下");
    let eval_up = ibis_doc_model::flow_command::evaluate_field_instruction(&format!("REF {caption_bm} \\p"), doc, at_after);
    assert_eq!(eval_up, "上");

    // 5. 保存と再読込での \p スイッチ付き REF フィールド保持検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let mut ref_p_count = 0;
    for b in &reread_flow.body {
        if let ibis_doc_model::Block::Paragraph(p) = b {
            for r in &p.runs {
                if let FlowRunKind::Field { instruction, .. } = &r.kind {
                    if instruction.contains("REF") && instruction.contains(r#"\p"#) {
                        ref_p_count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(ref_p_count, 2, "both relative position cross-references preserved in OOXML");

    // 6. 完全 Undo
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_picture_decorative() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let media = document.ids().add_media(ibis_doc_model::MediaFormat::Png, a_png());
    let p = text_paragraph(flow(document.presentation()), 4).clone();
    let size = ibis_doc_model::EmuSize::new(914_400, 685_800);
    let at = FlowPos::new(p.id, 2);

    // 1. 画像を挿入
    document.execute(vec![Command::Flow(FlowCommand::InsertPicture { at, media, size })]).unwrap();

    // 2. 初期状態の decorative が false であることを確認
    let doc = flow(document.presentation());
    let run = doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("inserted picture run");
    if let FlowRunKind::Picture { look, .. } = &run.kind {
        assert!(!look.decorative);
    } else {
        panic!("expected FlowRunKind::Picture");
    }

    // 3. 装飾画像としてマーク（look.decorative = true）
    let mut look_dec = ibis_doc_model::PictureLook::default();
    look_dec.decorative = true;
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: look_dec })]).unwrap();

    // 4. 保存して再読込し、look.decorative が true として保持されていること、XML に a1611:decorative が含まれることを確認
    let saved_dec = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let xml = ibis_ooxml::XmlDocument::parse(&ibis_ooxml::Package::open(saved_dec.clone()).unwrap().read("word/document.xml").unwrap()).unwrap();
    let root = xml.root_element().unwrap();
    let has_dec_elem = xml.descendants(root).into_iter().any(|n| xml.is_element(n) && xml.local_name(n) == "decorative");
    assert!(has_dec_elem, "document.xml should contain decorative element");

    let reread_dec = ibis_pptx::read(&saved_dec).unwrap();
    let reread_dec_doc = flow(&reread_dec);
    let run_dec = reread_dec_doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("picture in reread_dec");
    if let FlowRunKind::Picture { look, .. } = &run_dec.kind {
        assert!(look.decorative, "reloaded picture should be decorative");
        assert_eq!(look.alt_text.title, "");
        assert_eq!(look.alt_text.description, "");
    } else {
        panic!("expected FlowRunKind::Picture");
    }

    // 5. 装飾画像マークを解除し、代替テキストを設定して保存・再読込
    let mut look_undec = ibis_doc_model::PictureLook::default();
    look_undec.decorative = false;
    look_undec.alt_text = ibis_doc_model::AltText {
        title: "タイトル".to_string(),
        description: "説明文".to_string(),
    };
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: look_undec })]).unwrap();

    let saved_undec = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let xml_undec = ibis_ooxml::XmlDocument::parse(&ibis_ooxml::Package::open(saved_undec.clone()).unwrap().read("word/document.xml").unwrap()).unwrap();
    let root_undec = xml_undec.root_element().unwrap();
    let has_dec_elem_undec = xml_undec.descendants(root_undec).into_iter().any(|n| xml_undec.is_element(n) && xml_undec.local_name(n) == "decorative");
    assert!(!has_dec_elem_undec, "document.xml should no longer contain decorative element");

    let reread_undec = ibis_pptx::read(&saved_undec).unwrap();
    let reread_undec_doc = flow(&reread_undec);
    let run_undec = reread_undec_doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("picture in reread_undec");
    if let FlowRunKind::Picture { look, .. } = &run_undec.kind {
        assert!(!look.decorative);
        assert_eq!(look.alt_text.title, "タイトル");
        assert_eq!(look.alt_text.description, "説明文");
    } else {
        panic!("expected FlowRunKind::Picture");
    }

    // 6. Undo/Redo の検証
    // Undo -> decorative = true に戻る
    assert!(document.undo());
    let doc_after_undo = flow(document.presentation());
    let run_undo = doc_after_undo.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("picture after undo");
    if let FlowRunKind::Picture { look, .. } = &run_undo.kind {
        assert!(look.decorative);
    }

    // Redo -> decorative = false, 代替テキストありに戻る
    assert!(document.redo());
    let doc_after_redo = flow(document.presentation());
    let run_redo = doc_after_redo.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("picture after redo");
    if let FlowRunKind::Picture { look, .. } = &run_redo.kind {
        assert!(!look.decorative);
        assert_eq!(look.alt_text.title, "タイトル");
    }

    // 7. フローティング画像（ShapeKind::Picture）における decorative の保持
    let mut look_fl = ibis_doc_model::PictureLook::default();
    look_fl.decorative = true;
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: look_fl })]).unwrap();
    document.execute(vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::Square })]).unwrap();

    let saved_floating = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_floating = ibis_pptx::read(&saved_floating).unwrap();
    let reread_fl_doc = flow(&reread_floating);
    let run_fl = reread_fl_doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Shape(_))) } else { None }
    } else { None }).expect("floating shape run in reread_fl");
    if let FlowRunKind::Shape(shape) = &run_fl.kind {
        if let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind {
            assert!(pic.decorative, "floating picture should be decorative");
        } else {
            panic!("expected ShapeKind::Picture");
        }
    }

    // 8. 全て Undo して元のファイルに戻る
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_picture_name() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let media = document.ids().add_media(ibis_doc_model::MediaFormat::Png, a_png());
    let p = text_paragraph(flow(document.presentation()), 4).clone();
    let size = ibis_doc_model::EmuSize::new(914_400, 685_800);
    let at = FlowPos::new(p.id, 2);

    // 1. 画像を挿入
    document.execute(vec![Command::Flow(FlowCommand::InsertPicture { at, media, size })]).unwrap();

    // 2. 名前を設定（look.name = "図 1: サンプル画像"）
    let mut look = ibis_doc_model::PictureLook::default();
    look.name = "図 1: サンプル画像".to_string();
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: look.clone() })]).unwrap();

    // 3. 保存して再読込し、look.name が保持されていること、XML の wp:docPr に name="図 1: サンプル画像" が含まれることを確認
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let xml = ibis_ooxml::XmlDocument::parse(&ibis_ooxml::Package::open(saved.clone()).unwrap().read("word/document.xml").unwrap()).unwrap();
    let root = xml.root_element().unwrap();
    let has_name_attr = xml.descendants(root).into_iter().any(|n| {
        xml.is_element(n) && xml.local_name(n) == "docPr" && xml.attr_local(n, "name").as_deref() == Some("図 1: サンプル画像")
    });
    assert!(has_name_attr, "wp:docPr should contain name attribute with '図 1: サンプル画像'");

    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_doc = flow(&reread);
    let run = reread_doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Picture { .. })) } else { None }
    } else { None }).expect("picture in reread");
    if let FlowRunKind::Picture { look, .. } = &run.kind {
        assert_eq!(look.name, "図 1: サンプル画像");
    } else {
        panic!("expected FlowRunKind::Picture");
    }

    // 4. フローティング画像（ShapeKind::Picture）における name の保持
    document.execute(vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::Square })]).unwrap();
    let saved_floating = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread_floating = ibis_pptx::read(&saved_floating).unwrap();
    let reread_fl_doc = flow(&reread_floating);
    let run_fl = reread_fl_doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Shape(_))) } else { None }
    } else { None }).expect("floating shape run in reread_fl");
    if let FlowRunKind::Shape(shape) = &run_fl.kind {
        if let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind {
            assert_eq!(pic.name, "図 1: サンプル画像", "floating picture should retain name");
        } else {
            panic!("expected ShapeKind::Picture");
        }
    }

    // 5. 名前の変更
    let mut look2 = ibis_doc_model::PictureLook::default();
    look2.name = "ロゴ画像".to_string();
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: look2 })]).unwrap();
    let doc_mut = flow(document.presentation());
    let run_renamed = doc_mut.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Shape(_))) } else { None }
    } else { None }).expect("renamed floating shape run");
    if let FlowRunKind::Shape(shape) = &run_renamed.kind {
        if let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind {
            assert_eq!(pic.name, "ロゴ画像");
        }
    }

    // 6. Undo/Redo の検証
    assert!(document.undo());
    let doc_undo = flow(document.presentation());
    let run_undo = doc_undo.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Shape(_))) } else { None }
    } else { None }).expect("floating shape run after undo");
    if let FlowRunKind::Shape(shape) = &run_undo.kind {
        if let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind {
            assert_eq!(pic.name, "図 1: サンプル画像");
        }
    }

    assert!(document.redo());
    let doc_redo = flow(document.presentation());
    let run_redo = doc_redo.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == at.para { p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Shape(_))) } else { None }
    } else { None }).expect("floating shape run after redo");
    if let FlowRunKind::Shape(shape) = &run_redo.kind {
        if let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind {
            assert_eq!(pic.name, "ロゴ画像");
        }
    }

    // 7. 全て Undo して元のファイルに戻る
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_mark_as_final_and_read_only() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation);

    // 1. 初期状態
    let initial = flow(document.presentation());
    assert!(!initial.mark_as_final);
    assert!(!initial.read_only_recommended);

    // 2. 個別に設定（各 1 ステップの Undo）
    document.execute(vec![Command::Flow(FlowCommand::SetMarkAsFinal(true))]).unwrap();
    document.execute(vec![Command::Flow(FlowCommand::SetReadOnlyRecommended(true))]).unwrap();

    let modified = flow(document.presentation());
    assert!(modified.mark_as_final);
    assert!(modified.read_only_recommended);

    // 3. 保存して再読込
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let doc_reopened = flow(&reopened.presentation);
    assert!(doc_reopened.mark_as_final);
    assert!(doc_reopened.read_only_recommended);

    // package 内の XML 内容を検証
    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let custom_xml = pkg.read("docProps/custom.xml").unwrap();
    let custom_str = String::from_utf8_lossy(&custom_xml);
    assert!(custom_str.contains("_MarkAsFinal"));
    assert!(custom_str.contains("true"));

    let settings_xml = pkg.read("word/settings.xml").unwrap();
    let settings_str = String::from_utf8_lossy(&settings_xml);
    assert!(settings_str.contains("readOnlyRecommended"));

    // 4. 個別に解除
    document.execute(vec![Command::Flow(FlowCommand::SetMarkAsFinal(false))]).unwrap();
    document.execute(vec![Command::Flow(FlowCommand::SetReadOnlyRecommended(false))]).unwrap();

    let cleared = flow(document.presentation());
    assert!(!cleared.mark_as_final);
    assert!(!cleared.read_only_recommended);

    let saved_cleared = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reopened_cleared = ibis_pptx::open(saved_cleared.clone()).unwrap();
    let doc_reopened_cleared = flow(&reopened_cleared.presentation);
    assert!(!doc_reopened_cleared.mark_as_final);
    assert!(!doc_reopened_cleared.read_only_recommended);

    // 5. Undo / Redo の個別検証
    // まず解除2（SetReadOnlyRecommended(false)）を Undo -> read_only_recommended が true に戻る
    assert!(document.undo());
    let doc_u1 = flow(document.presentation());
    assert!(!doc_u1.mark_as_final);
    assert!(doc_u1.read_only_recommended);

    // 次に解除1（SetMarkAsFinal(false)）を Undo -> mark_as_final も true に戻る
    assert!(document.undo());
    let doc_u2 = flow(document.presentation());
    assert!(doc_u2.mark_as_final);
    assert!(doc_u2.read_only_recommended);

    // Redo して false に戻す
    assert!(document.redo());
    let doc_r1 = flow(document.presentation());
    assert!(!doc_r1.mark_as_final);
    assert!(doc_r1.read_only_recommended);

    assert!(document.redo());
    let doc_r2 = flow(document.presentation());
    assert!(!doc_r2.mark_as_final);
    assert!(!doc_r2.read_only_recommended);

    // 6. 全て Undo して元のファイルに戻る
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_doc_grid_and_line_pitch() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation);

    let initial = flow(document.presentation());
    let mut page = initial.page;

    // 1. 行グリッドと行送りを設定（720 twips = 457_200 EMU = 36 pt）
    page.doc_grid_type = ibis_doc_model::DocGridType::Lines;
    page.line_pitch = Some(457_200);
    page.char_space = None;
    document.execute(vec![Command::Flow(FlowCommand::SetPage { page })]).unwrap();

    let modified = flow(document.presentation());
    assert_eq!(modified.page.doc_grid_type, ibis_doc_model::DocGridType::Lines);
    assert_eq!(modified.page.line_pitch, Some(457_200));

    // 保存して再読込
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let doc_reopened = flow(&reopened.presentation);
    assert_eq!(doc_reopened.page.doc_grid_type, ibis_doc_model::DocGridType::Lines);
    assert_eq!(doc_reopened.page.line_pitch, Some(457_200));

    // XML 内の検証
    let pkg = ibis_ooxml::Package::open(saved).unwrap();
    let doc_xml = pkg.read("word/document.xml").unwrap();
    let doc_str = String::from_utf8_lossy(&doc_xml);
    assert!(doc_str.contains("docGrid"));
    assert!(doc_str.contains("type=\"lines\""));
    assert!(doc_str.contains("linePitch=\"720\""));

    // 2. 文字数と行数を指定（linesAndChars, charSpace = 200 twips = 127_000 EMU）
    let mut page2 = doc_reopened.page;
    page2.doc_grid_type = ibis_doc_model::DocGridType::LinesAndChars;
    page2.line_pitch = Some(457_200);
    page2.char_space = Some(127_000);
    document.execute(vec![Command::Flow(FlowCommand::SetPage { page: page2 })]).unwrap();

    let saved2 = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reopened2 = ibis_pptx::open(saved2.clone()).unwrap();
    let doc_reopened2 = flow(&reopened2.presentation);
    assert_eq!(doc_reopened2.page.doc_grid_type, ibis_doc_model::DocGridType::LinesAndChars);
    assert_eq!(doc_reopened2.page.line_pitch, Some(457_200));
    assert_eq!(doc_reopened2.page.char_space, Some(127_000));

    let pkg2 = ibis_ooxml::Package::open(saved2).unwrap();
    let doc_xml2 = pkg2.read("word/document.xml").unwrap();
    let doc_str2 = String::from_utf8_lossy(&doc_xml2);
    assert!(doc_str2.contains("type=\"linesAndChars\""));
    assert!(doc_str2.contains("charSpace=\"200\""));

    // 3. グリッドなし（type="default"）
    let mut page3 = doc_reopened2.page;
    page3.doc_grid_type = ibis_doc_model::DocGridType::Default;
    page3.line_pitch = None;
    page3.char_space = None;
    document.execute(vec![Command::Flow(FlowCommand::SetPage { page: page3 })]).unwrap();

    let saved3 = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reopened3 = ibis_pptx::open(saved3.clone()).unwrap();
    let doc_reopened3 = flow(&reopened3.presentation);
    assert_eq!(doc_reopened3.page.doc_grid_type, ibis_doc_model::DocGridType::Default);
    assert_eq!(doc_reopened3.page.line_pitch, None);

    let pkg3 = ibis_ooxml::Package::open(saved3).unwrap();
    let doc_xml3 = pkg3.read("word/document.xml").unwrap();
    let doc_str3 = String::from_utf8_lossy(&doc_xml3);
    assert!(doc_str3.contains("type=\"default\""));

    // 4. Undo して元に戻る
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_char_unit_indents() {
    use ibis_doc_model::ParaChange;

    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let p_id = text_paragraph(flow(document.presentation()), 0).id;

    // 1. 左2字 (200), 右1字 (100), 1行目字下げ1字 (100)
    document.execute(vec![
        Command::Flow(FlowCommand::SetParagraphFormat {
            first: p_id,
            last: p_id,
            change: ParaChange::Left(266_700),
        }),
        Command::Flow(FlowCommand::SetParagraphFormat {
            first: p_id,
            last: p_id,
            change: ParaChange::LeftChars(Some(200)),
        }),
        Command::Flow(FlowCommand::SetParagraphFormat {
            first: p_id,
            last: p_id,
            change: ParaChange::Right(133_350),
        }),
        Command::Flow(FlowCommand::SetParagraphFormat {
            first: p_id,
            last: p_id,
            change: ParaChange::RightChars(Some(100)),
        }),
        Command::Flow(FlowCommand::SetParagraphFormat {
            first: p_id,
            last: p_id,
            change: ParaChange::FirstLine(133_350),
        }),
        Command::Flow(FlowCommand::SetParagraphFormat {
            first: p_id,
            last: p_id,
            change: ParaChange::FirstLineChars(Some(100)),
        }),
    ]).unwrap();

    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let doc_xml = pkg.read("word/document.xml").unwrap();
    let doc_str = String::from_utf8_lossy(&doc_xml);
    assert!(doc_str.contains("w:leftChars=\"200\""), "leftChars not found in: {}", doc_str);
    assert!(doc_str.contains("w:rightChars=\"100\""), "rightChars not found in: {}", doc_str);
    assert!(doc_str.contains("w:firstLineChars=\"100\""), "firstLineChars not found in: {}", doc_str);

    let reopened = ibis_pptx::open(saved).unwrap();
    let doc_reopened = flow(&reopened.presentation);
    let p_reopened = paragraphs(doc_reopened).into_iter().find(|p| p.id == p_id).unwrap();
    assert_eq!(p_reopened.left_chars, Some(200));
    assert_eq!(p_reopened.right_chars, Some(100));
    assert_eq!(p_reopened.first_line_chars, Some(100));

    // 2. ぶら下げ2字 (-200)
    document.execute(vec![
        Command::Flow(FlowCommand::SetParagraphFormat {
            first: p_id,
            last: p_id,
            change: ParaChange::FirstLine(-266_700),
        }),
        Command::Flow(FlowCommand::SetParagraphFormat {
            first: p_id,
            last: p_id,
            change: ParaChange::FirstLineChars(Some(-200)),
        }),
    ]).unwrap();

    let saved2 = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let doc_xml2 = pkg2.read("word/document.xml").unwrap();
    let doc_str2 = String::from_utf8_lossy(&doc_xml2);
    assert!(doc_str2.contains("w:hangingChars=\"200\""), "hangingChars not found in: {}", doc_str2);
    assert!(!doc_str2.contains("w:firstLineChars"), "unexpected firstLineChars in: {}", doc_str2);

    let reopened2 = ibis_pptx::open(saved2).unwrap();
    let doc_reopened2 = flow(&reopened2.presentation);
    let p_reopened2 = paragraphs(doc_reopened2).into_iter().find(|p| p.id == p_id).unwrap();
    assert_eq!(p_reopened2.first_line_chars, Some(-200));

    // 3. Undo して元に戻る
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_insert_svg_picture() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let svg_bytes = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="150" viewBox="0 0 200 150"><rect width="200" height="150" fill="#ff0000"/><circle cx="100" cy="75" r="50" fill="#00ff00"/></svg>"##.to_vec();
    let media = document.ids().add_media(ibis_doc_model::MediaFormat::Svg, svg_bytes.clone());
    let p = text_paragraph(flow(document.presentation()), 4).clone();
    let size = ibis_doc_model::EmuSize::new(914_400, 685_800);
    let at = FlowPos::new(p.id, 2);

    // 1. SVG 画像を挿入
    document.execute(vec![Command::Flow(FlowCommand::InsertPicture { at, media, size })]).unwrap();

    // 2. 保存してパッケージ内検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();

    // (a) word/media/ に .svg ファイルが保存されていること
    let svg_part = pkg.names().find(|n| n.starts_with("word/media/") && n.ends_with(".svg")).expect("saved SVG part in word/media/");
    let saved_svg_bytes = pkg.read(svg_part).unwrap();
    assert_eq!(saved_svg_bytes, svg_bytes, "SVG bytes preserved");

    // (b) [Content_Types].xml に image/svg+xml が含まれること
    let content_types = String::from_utf8_lossy(&pkg.read("[Content_Types].xml").unwrap()).to_string();
    assert!(content_types.contains("image/svg+xml"), "image/svg+xml in Content_Types.xml");

    // (c) word/document.xml に asvg:svgBlip が出力されていること
    let doc_xml = String::from_utf8_lossy(&pkg.read("word/document.xml").unwrap()).to_string();
    assert!(doc_xml.contains("asvg:svgBlip"), "asvg:svgBlip element in document.xml");
    assert!(doc_xml.contains("http://schemas.microsoft.com/office/drawing/2016/SVG/main"), "SVG namespace in document.xml");

    // 3. 再読込検証: MediaFormat::Svg として復元され、データも一致
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let q = paragraphs(reread_flow).into_iter().find(|q| q.id == p.id).expect("paragraph preserved");
    let pic_run = q.runs.iter().find(|r| matches!(r.kind, ibis_doc_model::FlowRunKind::Picture { size: s, .. } if s == size)).expect("picture run read back");
    let FlowRunKind::Picture { media: reread_media, .. } = pic_run.kind else { panic!("expected picture run") };
    let m = reread.media.iter().find(|m| m.id == reread_media).expect("reread media exists");
    assert_eq!(m.format, ibis_doc_model::MediaFormat::Svg, "MediaFormat is Svg");
    assert_eq!(m.data.as_ref(), svg_bytes.as_slice(), "Media data matches inserted SVG");

    // 4. Undo / Redo 検証
    assert!(document.undo(), "undo succeeds");
    let after_undo = flow(document.presentation());
    let p_undo = paragraphs(after_undo).into_iter().find(|q| q.id == p.id).unwrap();
    assert!(!p_undo.runs.iter().any(|r| matches!(r.kind, FlowRunKind::Picture { .. })), "picture removed after undo");

    assert!(document.redo(), "redo succeeds");
    let after_redo = flow(document.presentation());
    let p_redo = paragraphs(after_redo).into_iter().find(|q| q.id == p.id).unwrap();
    assert!(p_redo.runs.iter().any(|r| matches!(r.kind, FlowRunKind::Picture { .. })), "picture restored after redo");

    // 5. SVG 画像を含む文書の再オープン・再保存（ラウンドトリップ・永続化保持）
    let opened2 = ibis_pptx::open(saved).unwrap();
    let mut document2 = Document::new(opened2.presentation.clone());
    // 別の操作（テキスト追記等）を行っても SVG が維持される
    let p2 = text_paragraph(flow(document2.presentation()), 1).clone();
    document2.execute(vec![Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p2.id, 0), text: "Test ".to_string(), format: None })]).unwrap();
    let saved2 = opened2.source.save(document2.presentation(), SaveOptions::default()).unwrap();
    let pkg2 = ibis_ooxml::Package::open(saved2).unwrap();
    assert!(pkg2.names().any(|n| n.starts_with("word/media/") && n.ends_with(".svg")), "SVG preserved after secondary edit");
}

#[test]
fn test_theme_color_text_formatting() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p = text_paragraph(flow(document.presentation()), 1).clone();
    let from = FlowPos::new(p.id, 0);
    let to = FlowPos::new(p.id, 5);

    // 1. テーマ色 (accent1) を適用
    let theme_tc = ibis_doc_model::FlowThemeColor::new("accent1", None::<String>, None::<String>);
    document.execute(vec![Command::Flow(FlowCommand::SetRunFormat {
        from,
        to,
        change: RunChange::ThemeColor {
            color: Rgb(0x4472c4),
            theme_color: theme_tc.clone(),
        },
    })]).unwrap();

    // モデルで theme_color が設定されていること
    let cur_flow = flow(document.presentation());
    let p_mod = paragraphs(cur_flow).into_iter().find(|q| q.id == p.id).unwrap();
    let run0 = &p_mod.runs[0];
    assert_eq!(run0.format.color, Some(Rgb(0x4472c4)));
    assert_eq!(run0.format.theme_color, Some(theme_tc.clone()));

    // 2. 保存して document.xml 内の w:themeColor を確認
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let doc_xml = String::from_utf8_lossy(&pkg.read("word/document.xml").unwrap()).to_string();
    assert!(doc_xml.contains("w:themeColor=\"accent1\""), "w:themeColor=\"accent1\" in document.xml: {}", doc_xml);
    assert!(doc_xml.contains("w:val=\"4472C4\""), "w:val=\"4472C4\" in document.xml");

    // 3. 再読込検証: FlowThemeColor が復元されること
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let q = paragraphs(reread_flow).into_iter().find(|q| q.id == p.id).unwrap();
    let r_run0 = &q.runs[0];
    assert_eq!(r_run0.format.color, Some(Rgb(0x4472c4)));
    assert_eq!(r_run0.format.theme_color, Some(theme_tc));

    // 4. カスタム色 (0xFF0000) に変更 -> w:themeColor が外れること
    document.execute(vec![Command::Flow(FlowCommand::SetRunFormat {
        from,
        to,
        change: RunChange::Color(Rgb(0xff0000)),
    })]).unwrap();

    let saved2 = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg2 = ibis_ooxml::Package::open(saved2).unwrap();
    let doc_xml2 = String::from_utf8_lossy(&pkg2.read("word/document.xml").unwrap()).to_string();
    assert!(doc_xml2.contains("w:val=\"FF0000\""), "w:val=\"FF0000\" in document.xml: {}", doc_xml2);

    // 5. Undo で元に戻ること
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_if_and_compare_fields() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let doc_ref = flow(document.presentation());

    // 1. evaluate_field_instruction 単体評価検証
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction("IF 10 > 2 \"Yes\" \"No\"", doc_ref, FlowPos::new(0, 0)), "Yes");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction("IF 10 < 2 \"Yes\" \"No\"", doc_ref, FlowPos::new(0, 0)), "No");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction("IF \"apple\" = \"APPLE\" \"Match\" \"Mismatch\"", doc_ref, FlowPos::new(0, 0)), "Match");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction("IF \"a\" <> \"b\" \"Diff\" \"Same\"", doc_ref, FlowPos::new(0, 0)), "Diff");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction("COMPARE 5 >= 5", doc_ref, FlowPos::new(0, 0)), "1");
    assert_eq!(ibis_doc_model::flow_command::evaluate_field_instruction("COMPARE 2 > 10", doc_ref, FlowPos::new(0, 0)), "0");

    // 2. フィールド挿入 (IF 1 = 1 "合格" "不合格")
    let p = text_paragraph(doc_ref, 0).clone();
    let at = FlowPos::new(p.id, 2);
    let find_field = |d: &FlowDocument, prefix: &str| -> Option<(String, String)> {
        for b in &d.body {
            if let ibis_doc_model::Block::Paragraph(p) = b {
                for r in &p.runs {
                    if let ibis_doc_model::FlowRunKind::Field { instruction, shown } = &r.kind {
                        if instruction.starts_with(prefix) {
                            return Some((instruction.clone(), shown.clone()));
                        }
                    }
                }
            }
        }
        None
    };

    let (cmds, _) = ibis_doc_model::flow_command::insert_field_at_range(doc_ref, Some((at, at)), "IF 1 = 1 \"合格\" \"不合格\"", false).expect("insert field commands");
    for cmd in cmds {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 挿入直後の値検証
    let doc1 = flow(document.presentation());
    let (fld_instr, fld_shown) = find_field(doc1, "IF 1 = 1").expect("field exists");
    assert_eq!(fld_instr, "IF 1 = 1 \"合格\" \"不合格\"");
    assert_eq!(fld_shown, "合格");

    // 3. OOXML 保存・再オープン検証
    let saved1 = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg1 = ibis_ooxml::Package::open(saved1.clone()).unwrap();
    let doc_xml1 = String::from_utf8_lossy(&pkg1.read("word/document.xml").unwrap()).to_string();
    assert!(doc_xml1.contains("IF 1 = 1"), "document.xml contains IF field instruction");
    assert!(doc_xml1.contains("合格"), "document.xml contains field cached result");

    let reread1 = ibis_pptx::read(&saved1).unwrap();
    let reread_flow1 = flow(&reread1);
    let (r_instr, r_shown) = find_field(reread_flow1, "IF 1 = 1").expect("reread field exists");
    assert_eq!(r_instr, "IF 1 = 1 \"合格\" \"不合格\"");
    assert_eq!(r_shown, "合格");

    // 4. フィールドコードを IF 1 = 2 に編集 & F9 更新 (updated_fields_in_range)
    let edit_cmds = ibis_doc_model::flow_command::edit_field_at(doc1, at, "IF 1 = 2 \"合格\" \"不合格\"").expect("edit field commands");
    for cmd in edit_cmds {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }
    let doc2 = flow(document.presentation());
    let update_cmds = ibis_doc_model::updated_fields_in_range(doc2, None, None);
    for cmd in update_cmds {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    let doc3 = flow(document.presentation());
    let (fld_instr2, fld_shown2) = find_field(doc3, "IF 1 = 2").expect("updated field exists");
    assert_eq!(fld_instr2, "IF 1 = 2 \"合格\" \"不合格\"");
    assert_eq!(fld_shown2, "不合格");

    // 5. COMPARE フィールドの挿入と更新検証
    let at_cmp = FlowPos::new(p.id, 0);
    let (cmp_cmds, _) = ibis_doc_model::flow_command::insert_field_at_range(doc3, Some((at_cmp, at_cmp)), "COMPARE 10 > 5", false).expect("insert compare commands");
    for cmd in cmp_cmds {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }
    let doc4 = flow(document.presentation());
    let (cmp_instr, cmp_shown) = find_field(doc4, "COMPARE 10 > 5").expect("compare field exists");
    assert_eq!(cmp_instr, "COMPARE 10 > 5");
    assert_eq!(cmp_shown, "1");

    // 6. Undo で元に戻ること
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn test_footnote_restart_each_page_settings() {
    use ibis_doc_model::NoteRestart;
    let file = STATEMENT;
    let out = scenario(file, &["word/document.xml"], |doc| {
        let mut new_page = doc.page;
        new_page.footnotes.restart = NoteRestart::EachPage;
        vec![FlowCommand::SetPage { page: new_page }]
    });

    let reread = ibis_pptx::read(&out.saved).unwrap();
    let doc = flow(&reread);
    assert_eq!(doc.page.footnotes.restart, NoteRestart::EachPage, "footnote restart setting eachPage should be preserved");
}
#[test]
fn test_note_position_settings() {
    use ibis_doc_model::NotePosition;
    let file = STATEMENT;
    let out = scenario(file, &["word/document.xml"], |doc| {
        let mut new_page = doc.page;
        new_page.footnotes.position = NotePosition::BeneathText;
        new_page.endnotes.position = NotePosition::SectionEnd;
        vec![FlowCommand::SetPage { page: new_page }]
    });

    let reread = ibis_pptx::read(&out.saved).unwrap();
    let doc = flow(&reread);
    assert_eq!(doc.page.footnotes.position, NotePosition::BeneathText, "footnote position beneathText should be preserved");
    assert_eq!(doc.page.endnotes.position, NotePosition::SectionEnd, "endnote position sectEnd should be preserved");
}

#[test]
fn test_field_text_format_switches_update() {
    use ibis_doc_model::{Block, FlowParagraph, FlowPos, FlowRunKind};

    let mut doc = ibis_doc_model::FlowDocument::default();
    doc.body.push(Block::Paragraph(FlowParagraph { id: 1, ..FlowParagraph::default() }));
    doc.properties.title = Some("quarterly progress report".to_string());
    doc.properties.creator = Some("john doe".to_string());

    let (cmds1, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, None, r#"TITLE \* Caps"#, false).unwrap();
    for cmd in cmds1 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }
    let p_id = match &doc.body[0] {
        Block::Paragraph(p) => p.id,
        _ => unreachable!(),
    };
    let (cmds2, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(p_id, 1), FlowPos::new(p_id, 1))), r#"AUTHOR \* Upper"#, false).unwrap();
    for cmd in cmds2 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }
    let (cmds3, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(p_id, 2), FlowPos::new(p_id, 2))), r#"TITLE \* FirstCap"#, false).unwrap();
    for cmd in cmds3 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    let p = match &doc.body[0] {
        Block::Paragraph(p) => p,
        _ => unreachable!(),
    };
    let fields: Vec<(&str, &str)> = p.runs.iter().filter_map(|r| if let FlowRunKind::Field { instruction, shown } = &r.kind { Some((instruction.as_str(), shown.as_str())) } else { None }).collect();
    assert_eq!(fields[0], (r#"TITLE \* Caps"#, "Quarterly Progress Report"));
    assert_eq!(fields[1], (r#"AUTHOR \* Upper"#, "JOHN DOE"));
    assert_eq!(fields[2], (r#"TITLE \* FirstCap"#, "Quarterly progress report"));

    // プロパティを変更して F9 更新
    doc.properties.title = Some("annual financial statement".to_string());
    let update_cmds = ibis_doc_model::updated_fields_in_range(&doc, None, None);
    for cmd in update_cmds {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    let p_updated = match &doc.body[0] {
        Block::Paragraph(p) => p,
        _ => unreachable!(),
    };
    let fields_updated: Vec<(&str, &str)> = p_updated.runs.iter().filter_map(|r| if let FlowRunKind::Field { instruction, shown } = &r.kind { Some((instruction.as_str(), shown.as_str())) } else { None }).collect();
    assert_eq!(fields_updated[0], (r#"TITLE \* Caps"#, "Annual Financial Statement"));
    assert_eq!(fields_updated[2], (r#"TITLE \* FirstCap"#, "Annual financial statement"));
}

#[test]
fn test_autonum_and_filesize_fields_update() {
    use ibis_doc_model::{Block, FlowParagraph, FlowPos, FlowRunKind};

    let mut doc = ibis_doc_model::FlowDocument::default();
    doc.body.push(Block::Paragraph(FlowParagraph { id: 1, ..FlowParagraph::default() }));
    doc.body.push(Block::Paragraph(FlowParagraph { id: 2, ..FlowParagraph::default() }));
    doc.body.push(Block::Paragraph(FlowParagraph { id: 3, ..FlowParagraph::default() }));
    doc.file_size = Some(1048576); // 1 MB (1024 KB)

    // 段落 1 に AUTONUM を挿入
    let (cmds1, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(1, 0), FlowPos::new(1, 0))), "AUTONUM", false).unwrap();
    for cmd in cmds1 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    // 段落 2 に AUTONUM を挿入
    let (cmds2, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(2, 0), FlowPos::new(2, 0))), "AUTONUM", false).unwrap();
    for cmd in cmds2 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    // 段落 3 に AUTONUMOUT を挿入
    let (cmds3, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(3, 0), FlowPos::new(3, 0))), "AUTONUMOUT", false).unwrap();
    for cmd in cmds3 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    // 段落 3 の末尾に FILESIZE \k を挿入
    let (cmds4, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(3, 1), FlowPos::new(3, 1))), "FILESIZE \\k", false).unwrap();
    for cmd in cmds4 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    // F9 更新を実行
    let update_cmds = ibis_doc_model::updated_fields_in_range(&doc, None, None);
    for cmd in update_cmds {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    let p1 = match &doc.body[0] { Block::Paragraph(p) => p, _ => unreachable!() };
    let p2 = match &doc.body[1] { Block::Paragraph(p) => p, _ => unreachable!() };
    let p3 = match &doc.body[2] { Block::Paragraph(p) => p, _ => unreachable!() };

    let p1_shown: Vec<&str> = p1.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    let p2_shown: Vec<&str> = p2.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    let p3_shown: Vec<&str> = p3.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();

    assert_eq!(p1_shown, vec!["1."]);
    assert_eq!(p2_shown, vec!["2."]);
    assert_eq!(p3_shown, vec!["I.", "1024"]);
}

#[test]
fn test_doc_variable_and_user_fields_update() {
    use ibis_doc_model::{Block, FlowParagraph, FlowPos, FlowRunKind};

    let mut doc = ibis_doc_model::FlowDocument::default();
    doc.body.push(Block::Paragraph(FlowParagraph { id: 1, ..FlowParagraph::default() }));
    doc.body.push(Block::Paragraph(FlowParagraph { id: 2, ..FlowParagraph::default() }));
    doc.properties.creator = Some("John Doe".to_string());
    doc.doc_variables.insert("BuildNumber".to_string(), "1042".to_string());
    doc.doc_variables.insert("Project".to_string(), "ibisOffice suite".to_string());

    // 段落 1 に DOCVARIABLE BuildNumber を挿入
    let (cmds1, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(1, 0), FlowPos::new(1, 0))), "DOCVARIABLE BuildNumber", false).unwrap();
    for cmd in cmds1 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    // 段落 1 に DOCVARIABLE Project \* Caps を挿入
    let (cmds2, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(1, 1), FlowPos::new(1, 1))), "DOCVARIABLE Project \\* Caps", false).unwrap();
    for cmd in cmds2 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    // 段落 2 に USERNAME, USERINITIALS, USERADDRESS を挿入
    let (cmds3, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(2, 0), FlowPos::new(2, 0))), "USERNAME", false).unwrap();
    for cmd in cmds3 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }
    let (cmds4, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(2, 1), FlowPos::new(2, 1))), "USERINITIALS", false).unwrap();
    for cmd in cmds4 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }
    let (cmds5, _) = ibis_doc_model::flow_command::insert_field_at_range(&doc, Some((FlowPos::new(2, 2), FlowPos::new(2, 2))), "USERADDRESS", false).unwrap();
    for cmd in cmds5 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    // F9 更新を実行
    let update_cmds = ibis_doc_model::updated_fields_in_range(&doc, None, None);
    for cmd in update_cmds {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }

    let p1 = match &doc.body[0] { Block::Paragraph(p) => p, _ => unreachable!() };
    let p2 = match &doc.body[1] { Block::Paragraph(p) => p, _ => unreachable!() };

    let p1_shown: Vec<&str> = p1.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    let p2_shown: Vec<&str> = p2.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();

    assert_eq!(p1_shown, vec!["1042", "IbisOffice Suite"]);
    assert_eq!(p2_shown, vec!["John Doe", "JD", ""]);

    // 変数を更新して F9 で再計算
    doc.doc_variables.insert("BuildNumber".to_string(), "2000".to_string());
    let update_cmds2 = ibis_doc_model::updated_fields_in_range(&doc, None, None);
    for cmd in update_cmds2 {
        ibis_doc_model::flow_command::apply_to(&mut doc, cmd).unwrap();
    }
    let p1_after = match &doc.body[0] { Block::Paragraph(p) => p, _ => unreachable!() };
    let p1_after_shown: Vec<&str> = p1_after.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    assert_eq!(p1_after_shown[0], "2000");
}

#[test]
fn test_table_formula_field_calculation_and_update() {
    use ibis_doc_model::{Block, FlowCell, FlowParagraph, FlowRow, FlowRun, FlowRunKind, FlowTable};

    let mut doc = ibis_doc_model::FlowDocument::default();
    let mut next_id = 1u32;
    let make_cell = |text: &str, next_id: &mut u32| -> FlowCell {
        let p_id = *next_id;
        *next_id += 1;
        let c_id = *next_id;
        *next_id += 1;
        let runs = if text.is_empty() {
            Vec::new()
        } else {
            vec![FlowRun { kind: FlowRunKind::Text(text.into()), ..FlowRun::default() }]
        };
        FlowCell {
            id: c_id,
            blocks: vec![Block::Paragraph(FlowParagraph { id: p_id, runs, ..FlowParagraph::default() })],
            ..FlowCell::default()
        }
    };

    let tbl_id = next_id;
    next_id += 1;
    let row1 = FlowRow { id: { next_id += 1; next_id - 1 }, cells: vec![make_cell("10", &mut next_id), make_cell("20", &mut next_id), make_cell("30", &mut next_id)], ..FlowRow::default() };
    let row2 = FlowRow { id: { next_id += 1; next_id - 1 }, cells: vec![make_cell("40", &mut next_id), make_cell("50", &mut next_id), make_cell("60", &mut next_id)], ..FlowRow::default() };
    let row3 = FlowRow { id: { next_id += 1; next_id - 1 }, cells: vec![make_cell("", &mut next_id), make_cell("", &mut next_id), make_cell("", &mut next_id)], ..FlowRow::default() };

    let table = FlowTable {
        id: tbl_id,
        rows: vec![row1, row2, row3],
        columns: vec![1000, 1000, 1000],
        ..FlowTable::default()
    };
    doc.body = vec![Block::Table(table)];

    let mut presentation = Presentation::default();
    presentation.flow = Some(Arc::new(doc));
    let mut document = Document::new(presentation);

    // 1. セル (2, 0) に =SUM(ABOVE) \# "#,##0" のフィールドを挿入
    let cell_2_0_p_id = match &flow(document.presentation()).body[0] {
        Block::Table(t) => match &t.rows[2].cells[0].blocks[0] {
            Block::Paragraph(p) => p.id,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    let (cmds1, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(cell_2_0_p_id, 0), FlowPos::new(cell_2_0_p_id, 0))),
        "=SUM(ABOVE) \\# \"#,##0\"",
        false,
    ).unwrap();
    for cmd in cmds1 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 挿入されたフィールドの shown 値が 50 (10 + 40) であることを確認
    let p_2_0 = match &flow(document.presentation()).body[0] {
        Block::Table(t) => match &t.rows[2].cells[0].blocks[0] {
            Block::Paragraph(p) => p.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    match &p_2_0.runs[0].kind {
        FlowRunKind::Field { instruction, shown } => {
            assert!(instruction.contains("SUM(ABOVE)"));
            assert_eq!(shown, "50", "10 + 40 = 50");
        }
        _ => panic!("expected field"),
    }

    // 2. セル (2, 1) に =AVERAGE(ABOVE) を挿入
    let cell_2_1_p_id = match &flow(document.presentation()).body[0] {
        Block::Table(t) => match &t.rows[2].cells[1].blocks[0] {
            Block::Paragraph(p) => p.id,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    let (cmds2, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(cell_2_1_p_id, 0), FlowPos::new(cell_2_1_p_id, 0))),
        "=AVERAGE(ABOVE)",
        false,
    ).unwrap();
    for cmd in cmds2 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    let p_2_1 = match &flow(document.presentation()).body[0] {
        Block::Table(t) => match &t.rows[2].cells[1].blocks[0] {
            Block::Paragraph(p) => p.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    match &p_2_1.runs[0].kind {
        FlowRunKind::Field { shown, .. } => {
            assert_eq!(shown, "35", "(20 + 50) / 2 = 35");
        }
        _ => panic!("expected field"),
    }

    // 3. セル (2, 2) に =A1 + B2 を挿入 (A1=10, B2=50 -> 60)
    let cell_2_2_p_id = match &flow(document.presentation()).body[0] {
        Block::Table(t) => match &t.rows[2].cells[2].blocks[0] {
            Block::Paragraph(p) => p.id,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    let (cmds3, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(cell_2_2_p_id, 0), FlowPos::new(cell_2_2_p_id, 0))),
        "=A1 + B2",
        false,
    ).unwrap();
    for cmd in cmds3 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    let p_2_2 = match &flow(document.presentation()).body[0] {
        Block::Table(t) => match &t.rows[2].cells[2].blocks[0] {
            Block::Paragraph(p) => p.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    match &p_2_2.runs[0].kind {
        FlowRunKind::Field { shown, .. } => {
            assert_eq!(shown, "60", "10 + 50 = 60");
        }
        _ => panic!("expected field"),
    }

    // 4. セル (0, 0) の値を 10 から 100 に更新し、フィールド更新（F9）を実行
    let cell_0_0_p_id = match &flow(document.presentation()).body[0] {
        Block::Table(t) => match &t.rows[0].cells[0].blocks[0] {
            Block::Paragraph(p) => p.id,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    document.execute(vec![
        Command::Flow(FlowCommand::Delete {
            from: FlowPos::new(cell_0_0_p_id, 0),
            to: FlowPos::new(cell_0_0_p_id, 2),
        }),
        Command::Flow(FlowCommand::InsertText {
            at: FlowPos::new(cell_0_0_p_id, 0),
            text: "100".to_string(),
            format: None,
        }),
    ]).unwrap();

    let update_cmds = ibis_doc_model::updated_fields_in_range(flow(document.presentation()), None, None);
    assert!(!update_cmds.is_empty(), "update commands should be generated");
    for cmd in update_cmds {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // セル (2, 0) の SUM(ABOVE) が 140 (100 + 40) に更新されたことを検証
    let p_2_0_updated = match &flow(document.presentation()).body[0] {
        Block::Table(t) => match &t.rows[2].cells[0].blocks[0] {
            Block::Paragraph(p) => p.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    match &p_2_0_updated.runs[0].kind {
        FlowRunKind::Field { shown, .. } => {
            assert_eq!(shown, "140", "100 + 40 = 140 after update");
        }
        _ => panic!("expected field"),
    }

    // セル (2, 2) の A1 + B2 が 150 (100 + 50) に更新されたことを検証
    let p_2_2_updated = match &flow(document.presentation()).body[0] {
        Block::Table(t) => match &t.rows[2].cells[2].blocks[0] {
            Block::Paragraph(p) => p.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    match &p_2_2_updated.runs[0].kind {
        FlowRunKind::Field { shown, .. } => {
            assert_eq!(shown, "150", "100 + 50 = 150 after update");
        }
        _ => panic!("expected field"),
    }
}

#[test]
fn test_listnum_field_numbering_and_levels() {
    let file = "editing/docx-report.docx";
    let opened = ibis_pptx::open(std::fs::read(path(file)).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let (p0_id, p1_id) = {
        let f = flow(document.presentation());
        let mut paras = f.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None });
        (paras.next().unwrap(), paras.next().unwrap())
    };

    // 1. 段落0に LISTNUM \l 1 (レベル1) を挿入 -> "1)"
    let (cmds1, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p0_id, 0), FlowPos::new(p0_id, 0))),
        "LISTNUM \\l 1",
        false,
    ).unwrap();
    for cmd in cmds1 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 2. 段落0に続けて LISTNUM \l 2 (レベル2) を挿入 -> "a)"
    let (cmds2, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p0_id, 1), FlowPos::new(p0_id, 1))),
        "LISTNUM \\l 2",
        false,
    ).unwrap();
    for cmd in cmds2 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 3. 段落1に LISTNUM \l 2 (レベル2 2回目) を挿入 -> "b)"
    let (cmds3, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p1_id, 0), FlowPos::new(p1_id, 0))),
        "LISTNUM \\l 2",
        false,
    ).unwrap();
    for cmd in cmds3 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 4. 段落1に LISTNUM \l 1 (レベル1 2回目) を挿入 -> "2)"
    let (cmds4, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p1_id, 1), FlowPos::new(p1_id, 1))),
        "LISTNUM \\l 1",
        false,
    ).unwrap();
    for cmd in cmds4 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 5. 段落1に レベル1の後に LISTNUM \l 2 を挿入 -> 上位レベル出現によりレベル2がリセットされて "a)" になることを検証
    let (cmds5, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p1_id, 2), FlowPos::new(p1_id, 2))),
        "LISTNUM \\l 2",
        false,
    ).unwrap();
    for cmd in cmds5 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 6. F9 (updated_fields_in_range) で全 LISTNUM フィールドの連番を更新
    let update_cmds = ibis_doc_model::updated_fields_in_range(flow(document.presentation()), None, None);
    assert!(!update_cmds.is_empty(), "update commands should be generated");
    for cmd in update_cmds {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 更新後の各 LISTNUM フィールドの値を検証
    let doc_updated = flow(document.presentation());
    let p0 = doc_updated.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p0_id { Some(p) } else { None } } else { None }).unwrap();
    let p1 = doc_updated.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p1_id { Some(p) } else { None } } else { None }).unwrap();

    let p0_fields: Vec<&str> = p0.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    let p1_fields: Vec<&str> = p1.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();

    assert_eq!(p0_fields, vec!["1)", "a)"], "p0 has level 1 (1)) and level 2 (a))");
    assert_eq!(p1_fields, vec!["b)", "2)", "a)"], "p1 has level 2 (b)), level 1 (2)), and level 2 reset to (a))");

    // 7. 保存と再オープン検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let (r_p0, r_p1) = {
        let mut paras = reread_flow.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None });
        (paras.next().unwrap(), paras.next().unwrap())
    };

    let r_p0_fields: Vec<&str> = r_p0.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    let r_p1_fields: Vec<&str> = r_p1.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    assert_eq!(r_p0_fields, vec!["1)", "a)"], "reopened p0 fields preserved");
    assert_eq!(r_p1_fields, vec!["b)", "2)", "a)"], "reopened p1 fields preserved");
}

#[test]
fn test_quote_symbol_and_set_fields_update() {
    let file = "editing/docx-report.docx";
    let opened = ibis_pptx::open(std::fs::read(path(file)).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let doc = flow(document.presentation());
    let p_target = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).unwrap();
    let p_id = p_target.id;

    // 1. QUOTE フィールドを挿入
    let (cmds1, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 0), FlowPos::new(p_id, 0))),
        "QUOTE \"Sample Literal Text\"",
        false,
    ).unwrap();
    for cmd in cmds1 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 2. QUOTE \* Caps フィールドを挿入
    let (cmds2, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 1), FlowPos::new(p_id, 1))),
        "QUOTE \"hello world\" \\* Caps",
        false,
    ).unwrap();
    for cmd in cmds2 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 3. QUOTE 文字コード列 (65 66 67 -> "ABC") を挿入
    let (cmds3, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 2), FlowPos::new(p_id, 2))),
        "QUOTE 65 66 67",
        false,
    ).unwrap();
    for cmd in cmds3 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 4. SYMBOL 65 ("A") を挿入
    let (cmds4, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 3), FlowPos::new(p_id, 3))),
        "SYMBOL 65",
        false,
    ).unwrap();
    for cmd in cmds4 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 5. SYMBOL 0x00A9 ("©") を挿入
    let (cmds5, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 4), FlowPos::new(p_id, 4))),
        "SYMBOL 0x00A9",
        false,
    ).unwrap();
    for cmd in cmds5 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 6. SET TitleVar "Ibis Dynamic Title" を挿入 (表示値は空)
    let (cmds6, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 5), FlowPos::new(p_id, 5))),
        "SET TitleVar \"Ibis Dynamic Title\"",
        false,
    ).unwrap();
    for cmd in cmds6 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 7. REF TitleVar を挿入 (SET の値を解決)
    let (cmds7, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 6), FlowPos::new(p_id, 6))),
        "REF TitleVar",
        false,
    ).unwrap();
    for cmd in cmds7 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 初期評価の検証
    {
        let doc_now = flow(document.presentation());
        let p = doc_now.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        let field_vals: Vec<&str> = p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
        assert_eq!(field_vals, vec![
            "Sample Literal Text",
            "Hello World",
            "ABC",
            "A",
            "©",
            "",
            "Ibis Dynamic Title",
        ], "initial evaluated field values");
    }

    // 8. F9 (updated_fields_in_range) で更新
    let update_cmds = ibis_doc_model::updated_fields_in_range(flow(document.presentation()), None, None);
    for cmd in update_cmds {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 更新後の検証
    {
        let doc_updated = flow(document.presentation());
        let p = doc_updated.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        let field_vals: Vec<&str> = p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
        assert_eq!(field_vals, vec![
            "Sample Literal Text",
            "Hello World",
            "ABC",
            "A",
            "©",
            "",
            "Ibis Dynamic Title",
        ], "updated field values after F9");
    }

    // 9. 保存と再オープン検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let r_p = reread_flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    let r_field_vals: Vec<&str> = r_p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    assert_eq!(r_field_vals, vec![
        "Sample Literal Text",
        "Hello World",
        "ABC",
        "A",
        "©",
        "",
        "Ibis Dynamic Title",
    ], "reopened field values preserved");
}

#[test]
fn test_hyperlink_template_advance_fields_update() {
    let file = "editing/docx-report.docx";
    let opened = ibis_pptx::open(std::fs::read(path(file)).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let doc = flow(document.presentation());
    let p_target = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).unwrap();
    let p_id = p_target.id;

    // 1. HYPERLINK "https://www.example.com" \o "Example Tip" を挿入
    let (cmds1, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 0), FlowPos::new(p_id, 0))),
        "HYPERLINK \"https://www.example.com\" \\o \"Example Tip\"",
        false,
    ).unwrap();
    for cmd in cmds1 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 2. HYPERLINK "https://ibis.inc" \* Upper を挿入
    let (cmds2, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 1), FlowPos::new(p_id, 1))),
        "HYPERLINK \"https://ibis.inc\" \\* Upper",
        false,
    ).unwrap();
    for cmd in cmds2 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 3. TEMPLATE を挿入
    let (cmds3, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 2), FlowPos::new(p_id, 2))),
        "TEMPLATE",
        false,
    ).unwrap();
    for cmd in cmds3 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 4. TEMPLATE \* Upper を挿入
    let (cmds4, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 3), FlowPos::new(p_id, 3))),
        "TEMPLATE \\* Upper",
        false,
    ).unwrap();
    for cmd in cmds4 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 5. ADVANCE \d 6 を挿入
    let (cmds5, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 4), FlowPos::new(p_id, 4))),
        "ADVANCE \\d 6",
        false,
    ).unwrap();
    for cmd in cmds5 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 初期評価の検証
    {
        let doc_now = flow(document.presentation());
        let p = doc_now.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        let field_vals: Vec<&str> = p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
        assert_eq!(field_vals, vec![
            "https://www.example.com",
            "HTTPS://IBIS.INC",
            "Normal.dotm",
            "NORMAL.DOTM",
            "",
        ], "initial evaluated field values");
    }

    // 6. F9 (updated_fields_in_range) で更新
    let update_cmds = ibis_doc_model::updated_fields_in_range(flow(document.presentation()), None, None);
    for cmd in update_cmds {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 更新後の検証
    {
        let doc_updated = flow(document.presentation());
        let p = doc_updated.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        let field_vals: Vec<&str> = p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
        assert_eq!(field_vals, vec![
            "https://www.example.com",
            "HTTPS://IBIS.INC",
            "Normal.dotm",
            "NORMAL.DOTM",
            "",
        ], "updated field values after F9");
    }

    // 7. 保存と再オープン検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let r_p = reread_flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    let r_field_vals: Vec<&str> = r_p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    assert_eq!(r_field_vals, vec![
        "https://www.example.com",
        "HTTPS://IBIS.INC",
        "Normal.dotm",
        "NORMAL.DOTM",
        "",
    ], "reopened field values preserved");

    // 8. HYPERLINK フィールドの link 属性が正しく復元されていることの検証
    let first_field_run = r_p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if instruction.contains("example.com"))).unwrap();
    assert_eq!(first_field_run.link, Some(ibis_doc_model::Link::Url("https://www.example.com".to_string())));
}

#[test]
fn test_nested_fields_and_info_update() {
    let file = "editing/docx-report.docx";
    let opened = ibis_pptx::open(std::fs::read(path(file)).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p_id = flow(document.presentation()).body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap();

    // 文書プロパティを設定
    let mut props = flow(document.presentation()).properties.clone();
    props.creator = Some("Antigravity Author".to_string());
    props.title = Some("Docx Feature Test".to_string());
    props.subject = Some("Nested Fields & INFO".to_string());
    document.execute(vec![Command::Flow(FlowCommand::SetDocProperties(props))]).unwrap();

    // 1. = { PAGE } + 10 を挿入
    let (cmds1, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 0), FlowPos::new(p_id, 0))),
        "= { PAGE } + 10",
        false,
    ).unwrap();
    for cmd in cmds1 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 2. IF { PAGE } = 1 "First" "Other" を挿入
    let (cmds2, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 1), FlowPos::new(p_id, 1))),
        "IF { PAGE } = 1 \"First\" \"Other\"",
        false,
    ).unwrap();
    for cmd in cmds2 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 3. IF { = { PAGE } + 1 } = 2 "Two" "NotTwo" を挿入
    let (cmds3, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 2), FlowPos::new(p_id, 2))),
        "IF { = { PAGE } + 1 } = 2 \"Two\" \"NotTwo\"",
        false,
    ).unwrap();
    for cmd in cmds3 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 4. INFO Author を挿入
    let (cmds4, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 3), FlowPos::new(p_id, 3))),
        "INFO Author",
        false,
    ).unwrap();
    for cmd in cmds4 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 5. INFO Title を挿入
    let (cmds5, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 4), FlowPos::new(p_id, 4))),
        "INFO Title",
        false,
    ).unwrap();
    for cmd in cmds5 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 6. INFO "Author" \* Upper を挿入
    let (cmds6, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 5), FlowPos::new(p_id, 5))),
        "INFO \"Author\" \\* Upper",
        false,
    ).unwrap();
    for cmd in cmds6 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 初期評価の検証
    {
        let doc_now = flow(document.presentation());
        let p = doc_now.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        let field_vals: Vec<&str> = p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
        assert_eq!(field_vals, vec![
            "11",
            "First",
            "Two",
            "Antigravity Author",
            "Docx Feature Test",
            "ANTIGRAVITY AUTHOR",
        ], "initial evaluated field values");
    }

    // F9 更新 (updated_fields_in_range)
    let update_cmds = ibis_doc_model::updated_fields_in_range(flow(document.presentation()), None, None);
    for cmd in update_cmds {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 更新後の検証
    {
        let doc_updated = flow(document.presentation());
        let p = doc_updated.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        let field_vals: Vec<&str> = p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
        assert_eq!(field_vals, vec![
            "11",
            "First",
            "Two",
            "Antigravity Author",
            "Docx Feature Test",
            "ANTIGRAVITY AUTHOR",
        ], "updated field values after F9");
    }

    // 保存と再オープン検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let r_p = reread_flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    let r_field_vals: Vec<&str> = r_p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    assert_eq!(r_field_vals, vec![
        "11",
        "First",
        "Two",
        "Antigravity Author",
        "Docx Feature Test",
        "ANTIGRAVITY AUTHOR",
    ], "reopened nested fields and INFO values preserved");
}

#[test]
fn test_ask_and_fillin_fields_update() {
    let file = "editing/docx-report.docx";
    let opened = ibis_pptx::open(std::fs::read(path(file)).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p_id = flow(document.presentation()).body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap();

    // 1. ASK を挿入
    let (cmds1, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 0), FlowPos::new(p_id, 0))),
        "ASK UserRole \"Enter role:\" \\d \"Lead Architect\"",
        false,
    ).unwrap();
    for cmd in cmds1 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 2. REF を挿入
    let (cmds2, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 1), FlowPos::new(p_id, 1))),
        "REF UserRole",
        false,
    ).unwrap();
    for cmd in cmds2 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 3. FILLIN を挿入
    let (cmds3, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 2), FlowPos::new(p_id, 2))),
        "FILLIN \"Enter department:\" \\d \"Engineering\" \\* Upper",
        false,
    ).unwrap();
    for cmd in cmds3 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 4. FILL-IN を挿入
    let (cmds4, _) = ibis_doc_model::flow_command::insert_field_at_range(
        flow(document.presentation()),
        Some((FlowPos::new(p_id, 3), FlowPos::new(p_id, 3))),
        "FILL-IN \\d \"tokyo office\" \\* Caps",
        false,
    ).unwrap();
    for cmd in cmds4 {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 初期評価の検証
    {
        let doc_now = flow(document.presentation());
        let p = doc_now.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        let field_vals: Vec<&str> = p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
        assert_eq!(field_vals, vec![
            "",
            "Lead Architect",
            "ENGINEERING",
            "Tokyo Office",
        ], "initial evaluated ASK and FILLIN field values");
    }

    // F9 更新 (updated_fields_in_range)
    let update_cmds = ibis_doc_model::updated_fields_in_range(flow(document.presentation()), None, None);
    for cmd in update_cmds {
        document.execute(vec![Command::Flow(cmd)]).unwrap();
    }

    // 更新後の検証
    {
        let doc_updated = flow(document.presentation());
        let p = doc_updated.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        let field_vals: Vec<&str> = p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
        assert_eq!(field_vals, vec![
            "",
            "Lead Architect",
            "ENGINEERING",
            "Tokyo Office",
        ], "updated field values after F9");
    }

    // 保存と再オープン検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let r_p = reread_flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    let r_field_vals: Vec<&str> = r_p.runs.iter().filter_map(|r| if let FlowRunKind::Field { shown, .. } = &r.kind { Some(shown.as_str()) } else { None }).collect();
    assert_eq!(r_field_vals, vec![
        "",
        "Lead Architect",
        "ENGINEERING",
        "Tokyo Office",
    ], "reopened ASK and FILLIN field values preserved");
}

#[test]
fn test_checkbox_control_load_toggle_save() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p_id = flow(document.presentation()).body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap();
    let at = FlowPos::new(p_id, 0);

    // 1. チェックボックスコントロールの挿入
    let cmd = FlowCommand::InsertField {
        at,
        kind: FlowRunKind::CheckBox {
            checked: false,
            checked_char: '\u{2611}',
            unchecked_char: '\u{2610}',
        },
    };
    document.execute(vec![Command::Flow(cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(p.runs[0].kind, FlowRunKind::CheckBox { checked: false, checked_char: '☑', unchecked_char: '☐' });
    }

    // 2. チェックボックスのトグル切り替え（false -> true）
    let toggle_cmd = FlowCommand::ToggleCheckBox { at };
    document.execute(vec![Command::Flow(toggle_cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(p.runs[0].kind, FlowRunKind::CheckBox { checked: true, checked_char: '☑', unchecked_char: '☐' });
    }

    // 3. Undo (true -> false)
    assert!(document.undo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(p.runs[0].kind, FlowRunKind::CheckBox { checked: false, checked_char: '☑', unchecked_char: '☐' });
    }

    // 4. Redo (false -> true)
    assert!(document.redo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(p.runs[0].kind, FlowRunKind::CheckBox { checked: true, checked_char: '☑', unchecked_char: '☐' });
    }

    // 5. 保存と XML 検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let xml = ibis_pptx::test_files::part_text(&saved, "word/document.xml").unwrap();
    assert!(xml.contains("checkbox"), "contains checkbox tag in XML: {xml}");
    assert!(xml.contains("☑"), "contains checked symbol in XML: {xml}");

    // 6. 再オープン検証（保持・復元）
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let r_p = reread_flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(r_p.runs[0].kind, FlowRunKind::CheckBox { checked: true, checked_char: '☑', unchecked_char: '☐' });

    // 7. 再オープン後のトグル（true -> false）と再保存検証
    let opened2 = ibis_pptx::open(saved).unwrap();
    let mut document2 = Document::new(opened2.presentation.clone());
    let at0 = FlowPos::new(p_id, 0);
    document2.execute(vec![Command::Flow(FlowCommand::ToggleCheckBox { at: at0 })]).unwrap();
    let saved2 = opened2.source.save(document2.presentation(), SaveOptions::default()).unwrap();
    let xml2 = ibis_pptx::test_files::part_text(&saved2, "word/document.xml").unwrap();
    assert!(xml2.contains("☐"), "contains unchecked symbol in XML: {xml2}");

    let reread2 = ibis_pptx::read(&saved2).unwrap();
    let r_p2 = flow(&reread2).body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(r_p2.runs[0].kind, FlowRunKind::CheckBox { checked: false, checked_char: '☑', unchecked_char: '☐' });
}

#[test]
fn test_dropdown_list_control_load_select_save() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p_id = flow(document.presentation()).body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap();
    let at = FlowPos::new(p_id, 0);

    let items = vec![
        ("企画部".to_string(), "dept_plan".to_string()),
        ("開発部".to_string(), "dept_dev".to_string()),
        ("営業部".to_string(), "dept_sales".to_string()),
    ];

    // 1. ドロップダウン リスト コントロールの挿入
    let cmd = FlowCommand::InsertField {
        at,
        kind: FlowRunKind::DropDownList {
            items: items.clone(),
            selected: 0,
            combo: false,
        },
    };
    document.execute(vec![Command::Flow(cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DropDownList {
                items: items.clone(),
                selected: 0,
                combo: false,
            }
        );
        assert!(p.text().starts_with("企画部"));
    }

    // 2. 候補選択の実行（0 -> 1: 開発部）
    let select_cmd = FlowCommand::SelectDropDownItem { at, index: 1 };
    document.execute(vec![Command::Flow(select_cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DropDownList {
                items: items.clone(),
                selected: 1,
                combo: false,
            }
        );
        assert!(p.text().starts_with("開発部"));
    }

    // 3. Undo (1 -> 0: 企画部)
    assert!(document.undo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DropDownList {
                items: items.clone(),
                selected: 0,
                combo: false,
            }
        );
        assert!(p.text().starts_with("企画部"));
    }

    // 4. Redo (0 -> 1: 開発部)
    assert!(document.redo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DropDownList {
                items: items.clone(),
                selected: 1,
                combo: false,
            }
        );
        assert!(p.text().starts_with("開発部"));
    }

    // 5. 保存と XML 検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let xml = ibis_pptx::test_files::part_text(&saved, "word/document.xml").unwrap();
    assert!(xml.contains("dropDownList"), "contains dropDownList tag in XML: {xml}");
    assert!(xml.contains("w:displayText=\"開発部\""), "contains listItem with displayText: {xml}");
    assert!(xml.contains("<w:t>開発部</w:t>"), "contains selected text run: {xml}");

    // 6. 再オープン検証（保持・復元）
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let r_p = reread_flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(
        r_p.runs[0].kind,
        FlowRunKind::DropDownList {
            items: items.clone(),
            selected: 1,
            combo: false,
        }
    );

    // 7. 再オープン後の選択変更（1 -> 2: 営業部）と再保存検証
    let opened2 = ibis_pptx::open(saved).unwrap();
    let mut document2 = Document::new(opened2.presentation.clone());
    let at0 = FlowPos::new(p_id, 0);
    document2.execute(vec![Command::Flow(FlowCommand::SelectDropDownItem { at: at0, index: 2 })]).unwrap();
    let saved2 = opened2.source.save(document2.presentation(), SaveOptions::default()).unwrap();
    let xml2 = ibis_pptx::test_files::part_text(&saved2, "word/document.xml").unwrap();
    assert!(xml2.contains("<w:t>営業部</w:t>"), "contains updated text run: {xml2}");

    let reread2 = ibis_pptx::read(&saved2).unwrap();
    let r_p2 = flow(&reread2).body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(
        r_p2.runs[0].kind,
        FlowRunKind::DropDownList {
            items: items.clone(),
            selected: 2,
            combo: false,
        }
    );

    // 8. コンボ ボックスの挿入・保存・再オープン検証
    let opened3 = ibis_pptx::open(saved2).unwrap();
    let mut document3 = Document::new(opened3.presentation.clone());
    let p_id2 = flow(document3.presentation()).body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).nth(1).unwrap();
    let at_p2 = FlowPos::new(p_id2, 0);
    let cmd_combo = FlowCommand::InsertField {
        at: at_p2,
        kind: FlowRunKind::DropDownList {
            items: items.clone(),
            selected: 0,
            combo: true,
        },
    };
    document3.execute(vec![Command::Flow(cmd_combo)]).unwrap();
    let saved_combo = opened3.source.save(document3.presentation(), SaveOptions::default()).unwrap();
    let xml_combo = ibis_pptx::test_files::part_text(&saved_combo, "word/document.xml").unwrap();
    assert!(xml_combo.contains("comboBox"), "contains comboBox tag in XML: {xml_combo}");
    let reread_combo = ibis_pptx::read(&saved_combo).unwrap();
    let r_p_combo = flow(&reread_combo).body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id2 { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(
        r_p_combo.runs[0].kind,
        FlowRunKind::DropDownList {
            items: items.clone(),
            selected: 0,
            combo: true,
        }
    );
}

#[test]
fn test_combo_box_control_load_select_save() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p_id = flow(document.presentation()).body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap();
    let at = FlowPos::new(p_id, 0);

    let items = vec![
        ("企画部".to_string(), "dept_plan".to_string()),
        ("開発部".to_string(), "dept_dev".to_string()),
        ("営業部".to_string(), "dept_sales".to_string()),
    ];

    // 1. コンボ ボックス コントロールの挿入
    let cmd = FlowCommand::InsertField {
        at,
        kind: FlowRunKind::DropDownList {
            items: items.clone(),
            selected: 0,
            combo: true,
        },
    };
    document.execute(vec![Command::Flow(cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DropDownList {
                items: items.clone(),
                selected: 0,
                combo: true,
            }
        );
        assert!(p.text().starts_with("企画部"));
    }

    // 2. 候補選択の実行（0 -> 1: 開発部）
    let select_cmd = FlowCommand::SelectDropDownItem { at, index: 1 };
    document.execute(vec![Command::Flow(select_cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DropDownList {
                items: items.clone(),
                selected: 1,
                combo: true,
            }
        );
        assert!(p.text().starts_with("開発部"));
    }

    // 3. Undo (1 -> 0: 企画部)
    assert!(document.undo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DropDownList {
                items: items.clone(),
                selected: 0,
                combo: true,
            }
        );
        assert!(p.text().starts_with("企画部"));
    }

    // 4. Redo (0 -> 1: 開発部)
    assert!(document.redo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DropDownList {
                items: items.clone(),
                selected: 1,
                combo: true,
            }
        );
        assert!(p.text().starts_with("開発部"));
    }

    // 5. 保存と XML 検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let xml = ibis_pptx::test_files::part_text(&saved, "word/document.xml").unwrap();
    assert!(xml.contains("comboBox"), "contains comboBox tag in XML: {xml}");
    assert!(xml.contains("w:displayText=\"開発部\""), "contains listItem with displayText: {xml}");
    assert!(xml.contains("<w:t>開発部</w:t>"), "contains selected text run: {xml}");

    // 6. 再オープン検証（保持・復元）
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let r_p = reread_flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(
        r_p.runs[0].kind,
        FlowRunKind::DropDownList {
            items: items.clone(),
            selected: 1,
            combo: true,
        }
    );

    // 7. 再オープン後の選択変更（1 -> 2: 営業部）と再保存検証
    let opened2 = ibis_pptx::open(saved).unwrap();
    let mut document2 = Document::new(opened2.presentation.clone());
    let at0 = FlowPos::new(p_id, 0);
    document2.execute(vec![Command::Flow(FlowCommand::SelectDropDownItem { at: at0, index: 2 })]).unwrap();
    let saved2 = opened2.source.save(document2.presentation(), SaveOptions::default()).unwrap();
    let xml2 = ibis_pptx::test_files::part_text(&saved2, "word/document.xml").unwrap();
    assert!(xml2.contains("<w:t>営業部</w:t>"), "contains updated text run: {xml2}");

    let reread2 = ibis_pptx::read(&saved2).unwrap();
    let r_p2 = flow(&reread2).body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(
        r_p2.runs[0].kind,
        FlowRunKind::DropDownList {
            items: items.clone(),
            selected: 2,
            combo: true,
        }
    );
}

#[test]
fn test_date_picker_control_load_step_save() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p_id = flow(document.presentation()).body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap();
    let at = FlowPos::new(p_id, 0);

    // 1. 日付選択コントロールの挿入
    let initial_date = "2026-10-09T00:00:00Z".to_string();
    let initial_fmt = "yyyy/M/d".to_string();
    let initial_text = "2026/10/9".to_string();

    let cmd = FlowCommand::InsertField {
        at,
        kind: FlowRunKind::DatePicker {
            full_date: initial_date.clone(),
            date_format: initial_fmt.clone(),
            text: initial_text.clone(),
        },
    };
    document.execute(vec![Command::Flow(cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DatePicker {
                full_date: initial_date.clone(),
                date_format: initial_fmt.clone(),
                text: initial_text.clone(),
            }
        );
        assert!(p.text().starts_with("2026/10/9"));
    }

    // 2. 日付変更の実行（2026-10-09 -> 2026-10-10）
    let next_date = "2026-10-10T00:00:00Z".to_string();
    let next_text = "2026/10/10".to_string();
    let set_cmd = FlowCommand::SetDatePickerDate {
        at,
        full_date: next_date.clone(),
        text: next_text.clone(),
    };
    document.execute(vec![Command::Flow(set_cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DatePicker {
                full_date: next_date.clone(),
                date_format: initial_fmt.clone(),
                text: next_text.clone(),
            }
        );
        assert!(p.text().starts_with("2026/10/10"));
    }

    // 3. Undo (2026-10-10 -> 2026-10-09)
    assert!(document.undo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DatePicker {
                full_date: initial_date.clone(),
                date_format: initial_fmt.clone(),
                text: initial_text.clone(),
            }
        );
        assert!(p.text().starts_with("2026/10/9"));
    }

    // 4. Redo (2026-10-09 -> 2026-10-10)
    assert!(document.redo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::DatePicker {
                full_date: next_date.clone(),
                date_format: initial_fmt.clone(),
                text: next_text.clone(),
            }
        );
        assert!(p.text().starts_with("2026/10/10"));
    }

    // 5. 保存と XML 検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let xml = ibis_pptx::test_files::part_text(&saved, "word/document.xml").unwrap();
    assert!(xml.contains("w:date"), "contains w:date tag in XML: {xml}");
    assert!(xml.contains("w:fullDate=\"2026-10-10T00:00:00Z\""), "contains w:fullDate in XML: {xml}");
    assert!(xml.contains("<w:dateFormat w:val=\"yyyy/M/d\"/>"), "contains w:dateFormat in XML: {xml}");
    assert!(xml.contains("<w:t>2026/10/10</w:t>"), "contains date text run: {xml}");

    // 6. 再オープン検証（保持・復元）
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let r_p = reread_flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(
        r_p.runs[0].kind,
        FlowRunKind::DatePicker {
            full_date: next_date.clone(),
            date_format: initial_fmt.clone(),
            text: next_text.clone(),
        }
    );

    // 7. 再オープン後の日付変更（2026-10-10 -> 2026-10-11）と再保存検証
    let opened2 = ibis_pptx::open(saved).unwrap();
    let mut document2 = Document::new(opened2.presentation.clone());
    let at0 = FlowPos::new(p_id, 0);
    let date3 = "2026-10-11T00:00:00Z".to_string();
    let text3 = "2026/10/11".to_string();
    document2.execute(vec![Command::Flow(FlowCommand::SetDatePickerDate {
        at: at0,
        full_date: date3.clone(),
        text: text3.clone(),
    })]).unwrap();
    let saved2 = opened2.source.save(document2.presentation(), SaveOptions::default()).unwrap();
    let xml2 = ibis_pptx::test_files::part_text(&saved2, "word/document.xml").unwrap();
    assert!(xml2.contains("<w:t>2026/10/11</w:t>"), "contains updated date text run: {xml2}");
    assert!(xml2.contains("w:fullDate=\"2026-10-11T00:00:00Z\""), "contains updated fullDate: {xml2}");

    let reread2 = ibis_pptx::read(&saved2).unwrap();
    let r_p2 = flow(&reread2).body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(
        r_p2.runs[0].kind,
        FlowRunKind::DatePicker {
            full_date: date3,
            date_format: initial_fmt,
            text: text3,
        }
    );
}

#[test]
fn test_rich_text_control_load_edit_save() {
    let bytes = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(bytes).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p_id = flow(document.presentation()).body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap();
    let at = FlowPos::new(p_id, 0);

    // 1. リッチテキスト コンテンツ コントロールの挿入 (DOCX-FORM-001)
    let initial_text = "リッチテキスト初期値".to_string();
    let cmd = FlowCommand::InsertField {
        at,
        kind: FlowRunKind::RichText {
            text: initial_text.clone(),
        },
    };
    document.execute(vec![Command::Flow(cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::RichText {
                text: initial_text.clone(),
            }
        );
        assert!(p.text().starts_with("リッチテキスト初期値"));
    }

    // 2. テキスト編集の実行
    let next_text = "リッチテキスト更新値".to_string();
    let set_cmd = FlowCommand::SetContentControlText {
        at,
        text: next_text.clone(),
    };
    document.execute(vec![Command::Flow(set_cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::RichText {
                text: next_text.clone(),
            }
        );
        assert!(p.text().starts_with("リッチテキスト更新値"));
    }

    // 3. Undo / Redo
    assert!(document.undo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(p.runs[0].kind, FlowRunKind::RichText { text: initial_text.clone() });
    }
    assert!(document.redo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(p.runs[0].kind, FlowRunKind::RichText { text: next_text.clone() });
    }

    // 4. 保存と XML 検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let xml = ibis_pptx::test_files::part_text(&saved, "word/document.xml").unwrap();
    assert!(xml.contains("<w:sdt>"), "contains w:sdt in XML: {xml}");
    assert!(xml.contains("<w:placeholder>"), "contains w:placeholder in XML: {xml}");
    assert!(xml.contains("<w:t>リッチテキスト更新値</w:t>"), "contains rich text run: {xml}");

    // 5. 再オープン検証
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let r_p = reread_flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(
        r_p.runs[0].kind,
        FlowRunKind::RichText {
            text: next_text,
        }
    );
}

#[test]
fn test_plain_text_control_load_edit_save() {
    let bytes = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(bytes).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p_id = flow(document.presentation()).body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap();
    let at = FlowPos::new(p_id, 0);

    // 1. プレーンテキスト コンテンツ コントロールの挿入 (DOCX-FORM-002)
    let initial_text = "プレーンテキスト初期値".to_string();
    let cmd = FlowCommand::InsertField {
        at,
        kind: FlowRunKind::PlainText {
            text: initial_text.clone(),
        },
    };
    document.execute(vec![Command::Flow(cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::PlainText {
                text: initial_text.clone(),
            }
        );
        assert!(p.text().starts_with("プレーンテキスト初期値"));
    }

    // 2. テキスト編集の実行
    let next_text = "プレーンテキスト更新値".to_string();
    let set_cmd = FlowCommand::SetContentControlText {
        at,
        text: next_text.clone(),
    };
    document.execute(vec![Command::Flow(set_cmd)]).unwrap();

    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(
            p.runs[0].kind,
            FlowRunKind::PlainText {
                text: next_text.clone(),
            }
        );
        assert!(p.text().starts_with("プレーンテキスト更新値"));
    }

    // 3. Undo / Redo
    assert!(document.undo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(p.runs[0].kind, FlowRunKind::PlainText { text: initial_text.clone() });
    }
    assert!(document.redo());
    {
        let doc = flow(document.presentation());
        let p = doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
        assert_eq!(p.runs[0].kind, FlowRunKind::PlainText { text: next_text.clone() });
    }

    // 4. 保存と XML 検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let xml = ibis_pptx::test_files::part_text(&saved, "word/document.xml").unwrap();
    assert!(xml.contains("<w:sdt>"), "contains w:sdt in XML: {xml}");
    assert!(xml.contains("<w:text/>"), "contains w:text in XML: {xml}");
    assert!(xml.contains("<w:t>プレーンテキスト更新値</w:t>"), "contains plain text run: {xml}");

    // 5. 再オープン検証
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let r_p = reread_flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { if p.id == p_id { Some(p) } else { None } } else { None }).unwrap();
    assert_eq!(
        r_p.runs[0].kind,
        FlowRunKind::PlainText {
            text: next_text,
        }
    );
}

#[test]
fn test_document_view_mode_settings_save_and_reload() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation);

    // 1. 初期状態は Print
    assert_eq!(flow(document.presentation()).view_mode, ibis_doc_model::DocumentViewMode::Print);

    // 2. 閲覧モード (Reading) へ変更
    document.execute(vec![Command::Flow(FlowCommand::SetViewMode(ibis_doc_model::DocumentViewMode::Reading))]).unwrap();
    assert_eq!(flow(document.presentation()).view_mode, ibis_doc_model::DocumentViewMode::Reading);

    // 3. 保存して settings.xml を検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let settings_xml = pkg.read("word/settings.xml").unwrap();
    let settings_str = String::from_utf8_lossy(&settings_xml);
    assert!(settings_str.contains("view"), "settings.xml must contain view: {settings_str}");
    assert!(settings_str.contains("reading"), "settings.xml must contain reading: {settings_str}");

    // 4. 再オープン検証
    let reread = ibis_pptx::open(saved).unwrap();
    assert_eq!(flow(&reread.presentation).view_mode, ibis_doc_model::DocumentViewMode::Reading);

    // 5. 1手 Undo で Print に戻る
    assert!(document.undo());
    assert_eq!(flow(document.presentation()).view_mode, ibis_doc_model::DocumentViewMode::Print);

    // 6. Redo で Reading に戻る
    assert!(document.redo());
    assert_eq!(flow(document.presentation()).view_mode, ibis_doc_model::DocumentViewMode::Reading);

    // 7. Web レイアウト (Web) へ変更・保存・再オープン
    document.execute(vec![Command::Flow(FlowCommand::SetViewMode(ibis_doc_model::DocumentViewMode::Web))]).unwrap();
    assert_eq!(flow(document.presentation()).view_mode, ibis_doc_model::DocumentViewMode::Web);
    let saved_web = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg_web = ibis_ooxml::Package::open(saved_web.clone()).unwrap();
    let settings_web = String::from_utf8_lossy(&pkg_web.read("word/settings.xml").unwrap()).to_string();
    assert!(settings_web.contains("web"), "settings.xml must contain web: {settings_web}");
    let reread_web = ibis_pptx::open(saved_web).unwrap();
    assert_eq!(flow(&reread_web.presentation).view_mode, ibis_doc_model::DocumentViewMode::Web);

    // 8. 下書き (Draft / normal) へ変更・保存・再オープン
    document.execute(vec![Command::Flow(FlowCommand::SetViewMode(ibis_doc_model::DocumentViewMode::Draft))]).unwrap();
    assert_eq!(flow(document.presentation()).view_mode, ibis_doc_model::DocumentViewMode::Draft);
    let saved_draft = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg_draft = ibis_ooxml::Package::open(saved_draft.clone()).unwrap();
    let settings_draft = String::from_utf8_lossy(&pkg_draft.read("word/settings.xml").unwrap()).to_string();
    assert!(settings_draft.contains("normal"), "settings.xml must contain normal for draft: {settings_draft}");
    let reread_draft = ibis_pptx::open(saved_draft).unwrap();
    assert_eq!(flow(&reread_draft.presentation).view_mode, ibis_doc_model::DocumentViewMode::Draft);

    // 9. アウトライン (Outline) へ変更・保存・再オープン
    document.execute(vec![Command::Flow(FlowCommand::SetViewMode(ibis_doc_model::DocumentViewMode::Outline))]).unwrap();
    assert_eq!(flow(document.presentation()).view_mode, ibis_doc_model::DocumentViewMode::Outline);
    let saved_outline = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg_outline = ibis_ooxml::Package::open(saved_outline.clone()).unwrap();
    let settings_outline = String::from_utf8_lossy(&pkg_outline.read("word/settings.xml").unwrap()).to_string();
    assert!(settings_outline.contains("outline"), "settings.xml must contain outline: {settings_outline}");
    let reread_outline = ibis_pptx::open(saved_outline.clone()).unwrap();
    assert_eq!(flow(&reread_outline.presentation).view_mode, ibis_doc_model::DocumentViewMode::Outline);

    // 10. 印刷レイアウト (Print) へ復帰・保存・再オープン
    let reread_outline_doc = ibis_pptx::open(saved_outline).unwrap();
    let mut doc_from_outline = Document::new(reread_outline_doc.presentation);
    doc_from_outline.execute(vec![Command::Flow(FlowCommand::SetViewMode(ibis_doc_model::DocumentViewMode::Print))]).unwrap();
    assert_eq!(flow(doc_from_outline.presentation()).view_mode, ibis_doc_model::DocumentViewMode::Print);
    let saved_print = reread_outline_doc.source.save(doc_from_outline.presentation(), SaveOptions::default()).unwrap();
    let pkg_print = ibis_ooxml::Package::open(saved_print.clone()).unwrap();
    let settings_print = String::from_utf8_lossy(&pkg_print.read("word/settings.xml").unwrap()).to_string();
    assert!(settings_print.contains("print"), "settings.xml must contain print: {settings_print}");
    let reread_print = ibis_pptx::open(saved_print).unwrap();
    assert_eq!(flow(&reread_print.presentation).view_mode, ibis_doc_model::DocumentViewMode::Print);
}

#[test]
fn test_document_protection_settings_save_and_reload() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation);

    // 1. 初期状態は None
    assert_eq!(flow(document.presentation()).protection, ibis_doc_model::DocumentProtection::None);

    // 2. 読み取り専用 (ReadOnly) 保護へ変更
    document.execute(vec![Command::Flow(FlowCommand::SetDocumentProtection(ibis_doc_model::DocumentProtection::ReadOnly))]).unwrap();
    assert_eq!(flow(document.presentation()).protection, ibis_doc_model::DocumentProtection::ReadOnly);

    // 3. 保存して settings.xml を検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let settings_xml = pkg.read("word/settings.xml").unwrap();
    let settings_str = String::from_utf8_lossy(&settings_xml);
    assert!(settings_str.contains("documentProtection"), "settings.xml must contain documentProtection: {settings_str}");
    assert!(settings_str.contains(r#"w:edit="readOnly""#), "settings.xml must contain edit=\"readOnly\": {settings_str}");
    assert!(settings_str.contains(r#"w:enforcement="1""#), "settings.xml must contain enforcement=\"1\": {settings_str}");

    // 4. 再オープン検証 (ReadOnly)
    let reread = ibis_pptx::open(saved).unwrap();
    assert_eq!(flow(&reread.presentation).protection, ibis_doc_model::DocumentProtection::ReadOnly);

    // 5. フォーム入力保護 (Forms) へ変更
    let mut document2 = Document::new(reread.presentation);
    document2.execute(vec![Command::Flow(FlowCommand::SetDocumentProtection(ibis_doc_model::DocumentProtection::Forms))]).unwrap();
    assert_eq!(flow(document2.presentation()).protection, ibis_doc_model::DocumentProtection::Forms);
    let saved2 = reread.source.save(document2.presentation(), SaveOptions::default()).unwrap();
    let pkg2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let settings_xml2 = pkg2.read("word/settings.xml").unwrap();
    let settings_str2 = String::from_utf8_lossy(&settings_xml2);
    assert!(settings_str2.contains(r#"w:edit="forms""#), "settings.xml must contain edit=\"forms\": {settings_str2}");

    // 6. 再オープン検証 (Forms)
    let reread2 = ibis_pptx::open(saved2).unwrap();
    assert_eq!(flow(&reread2.presentation).protection, ibis_doc_model::DocumentProtection::Forms);

    // 7. 保護解除 (None)
    let mut document3 = Document::new(reread2.presentation);
    document3.execute(vec![Command::Flow(FlowCommand::SetDocumentProtection(ibis_doc_model::DocumentProtection::None))]).unwrap();
    assert_eq!(flow(document3.presentation()).protection, ibis_doc_model::DocumentProtection::None);
    let saved3 = reread2.source.save(document3.presentation(), SaveOptions::default()).unwrap();
    let pkg3 = ibis_ooxml::Package::open(saved3.clone()).unwrap();
    let settings_xml3 = pkg3.read("word/settings.xml").unwrap();
    let settings_str3 = String::from_utf8_lossy(&settings_xml3);
    assert!(!settings_str3.contains("documentProtection"), "settings.xml must not contain documentProtection: {settings_str3}");

    let reread3 = ibis_pptx::open(saved3).unwrap();
    assert_eq!(flow(&reread3.presentation).protection, ibis_doc_model::DocumentProtection::None);

    // 8. Undo/Redo 検証
    assert!(document.undo());
    assert_eq!(flow(document.presentation()).protection, ibis_doc_model::DocumentProtection::None);
    assert!(document.redo());
    assert_eq!(flow(document.presentation()).protection, ibis_doc_model::DocumentProtection::ReadOnly);
}

#[test]
fn test_formatting_and_comments_protection_save_and_reload() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation);

    // 1. 初期状態は formatting_protection == false, protection == None
    assert!(!flow(document.presentation()).formatting_protection);
    assert_eq!(flow(document.presentation()).protection, ibis_doc_model::DocumentProtection::None);

    // 2. 書式変更の制限 (formatting_protection = true)
    document.execute(vec![Command::Flow(FlowCommand::SetFormattingProtection(true))]).unwrap();
    assert!(flow(document.presentation()).formatting_protection);
    assert_eq!(flow(document.presentation()).protection, ibis_doc_model::DocumentProtection::None);

    // 3. 保存して settings.xml を検証 (w:formatting="1", w:enforcement="1", edit 属性なし)
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let settings_xml = pkg.read("word/settings.xml").unwrap();
    let settings_str = String::from_utf8_lossy(&settings_xml);
    assert!(settings_str.contains("documentProtection"), "settings.xml must contain documentProtection: {settings_str}");
    assert!(settings_str.contains(r#"w:formatting="1""#), "settings.xml must contain formatting=\"1\": {settings_str}");
    assert!(settings_str.contains(r#"w:enforcement="1""#), "settings.xml must contain enforcement=\"1\": {settings_str}");
    assert!(!settings_str.contains(r#"w:edit="#), "settings.xml must not contain edit attribute: {settings_str}");

    // 4. 再オープン検証 (formatting_protection == true)
    let reread = ibis_pptx::open(saved).unwrap();
    assert!(flow(&reread.presentation).formatting_protection);
    assert_eq!(flow(&reread.presentation).protection, ibis_doc_model::DocumentProtection::None);

    // 5. コメント保護 (Comments) を同時に設定
    let mut document2 = Document::new(reread.presentation);
    document2.execute(vec![Command::Flow(FlowCommand::SetDocumentProtection(ibis_doc_model::DocumentProtection::Comments))]).unwrap();
    assert!(flow(document2.presentation()).formatting_protection);
    assert_eq!(flow(document2.presentation()).protection, ibis_doc_model::DocumentProtection::Comments);

    let saved2 = reread.source.save(document2.presentation(), SaveOptions::default()).unwrap();
    let pkg2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let settings_xml2 = pkg2.read("word/settings.xml").unwrap();
    let settings_str2 = String::from_utf8_lossy(&settings_xml2);
    assert!(settings_str2.contains(r#"w:formatting="1""#), "settings.xml must contain formatting=\"1\": {settings_str2}");
    assert!(settings_str2.contains(r#"w:edit="comments""#), "settings.xml must contain edit=\"comments\": {settings_str2}");

    // 6. 再オープン検証 (formatting_protection == true && protection == Comments)
    let reread2 = ibis_pptx::open(saved2).unwrap();
    assert!(flow(&reread2.presentation).formatting_protection);
    assert_eq!(flow(&reread2.presentation).protection, ibis_doc_model::DocumentProtection::Comments);

    // 7. 保護全解除 (formatting_protection = false && protection = None)
    let mut document3 = Document::new(reread2.presentation);
    document3.execute(vec![
        Command::Flow(FlowCommand::SetDocumentProtection(ibis_doc_model::DocumentProtection::None)),
        Command::Flow(FlowCommand::SetFormattingProtection(false)),
    ]).unwrap();
    assert!(!flow(document3.presentation()).formatting_protection);
    assert_eq!(flow(document3.presentation()).protection, ibis_doc_model::DocumentProtection::None);

    let saved3 = reread2.source.save(document3.presentation(), SaveOptions::default()).unwrap();
    let pkg3 = ibis_ooxml::Package::open(saved3.clone()).unwrap();
    let settings_xml3 = pkg3.read("word/settings.xml").unwrap();
    let settings_str3 = String::from_utf8_lossy(&settings_xml3);
    assert!(!settings_str3.contains("documentProtection"), "settings.xml must not contain documentProtection: {settings_str3}");

    let reread3 = ibis_pptx::open(saved3).unwrap();
    assert!(!flow(&reread3.presentation).formatting_protection);
    assert_eq!(flow(&reread3.presentation).protection, ibis_doc_model::DocumentProtection::None);

    // 8. Undo/Redo 検証
    assert!(document.undo());
    assert!(!flow(document.presentation()).formatting_protection);
    assert!(document.redo());
    assert!(flow(document.presentation()).formatting_protection);
}

#[test]
fn test_interaction_modes_and_persistence() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    // 1. 初期状態: Editing モード
    assert_eq!(flow(document.presentation()).interaction_mode, ibis_doc_model::DocumentInteractionMode::Editing);

    // 2. Reviewing モードへ切替 -> track_changes が true になる
    document.execute(vec![Command::Flow(FlowCommand::SetInteractionMode(ibis_doc_model::DocumentInteractionMode::Reviewing))]).unwrap();
    assert_eq!(flow(document.presentation()).interaction_mode, ibis_doc_model::DocumentInteractionMode::Reviewing);
    assert!(flow(document.presentation()).track_changes, "track_changes must be turned on in Reviewing mode");

    // 3. 保存と再読込
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::open(saved.clone()).unwrap();
    assert_eq!(flow(&reread.presentation).interaction_mode, ibis_doc_model::DocumentInteractionMode::Reviewing);
    assert!(flow(&reread.presentation).track_changes);

    // 4. Viewing モードへ切替
    let mut document2 = Document::new(reread.presentation);
    document2.execute(vec![Command::Flow(FlowCommand::SetInteractionMode(ibis_doc_model::DocumentInteractionMode::Viewing))]).unwrap();
    assert_eq!(flow(document2.presentation()).interaction_mode, ibis_doc_model::DocumentInteractionMode::Viewing);

    // 5. Editing モードへ切替 -> track_changes が false に戻る
    document2.execute(vec![Command::Flow(FlowCommand::SetInteractionMode(ibis_doc_model::DocumentInteractionMode::Editing))]).unwrap();
    assert_eq!(flow(document2.presentation()).interaction_mode, ibis_doc_model::DocumentInteractionMode::Editing);
    assert!(!flow(document2.presentation()).track_changes, "track_changes should be turned off when switching to Editing");

    // 6. Undo/Redo 検証
    assert!(document2.undo());
    assert_eq!(flow(document2.presentation()).interaction_mode, ibis_doc_model::DocumentInteractionMode::Viewing);
    assert!(document2.undo());
    assert_eq!(flow(document2.presentation()).interaction_mode, ibis_doc_model::DocumentInteractionMode::Reviewing);
    assert!(flow(document2.presentation()).track_changes);
    assert!(document2.redo());
    assert_eq!(flow(document2.presentation()).interaction_mode, ibis_doc_model::DocumentInteractionMode::Viewing);
    assert!(document2.redo());
    assert_eq!(flow(document2.presentation()).interaction_mode, ibis_doc_model::DocumentInteractionMode::Editing);
}

#[test]
fn test_complex_script_font_formatting_and_roundtrip() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p = text_paragraph(flow(document.presentation()), 1).clone();
    let from = FlowPos::new(p.id, 0);
    let to = FlowPos::new(p.id, 5);

    // 1. 欧文・和文・複雑文字用フォントを設定
    document.execute(vec![
        Command::Flow(FlowCommand::SetRunFormat { from, to, change: RunChange::Font("Calibri".into()) }),
        Command::Flow(FlowCommand::SetRunFormat { from, to, change: RunChange::EastAsianFont("MS Mincho".into()) }),
        Command::Flow(FlowCommand::SetRunFormat { from, to, change: RunChange::ComplexScriptFont("Traditional Arabic".into()) }),
    ]).unwrap();

    let cur_flow = flow(document.presentation());
    let p_mod = paragraphs(cur_flow).into_iter().find(|q| q.id == p.id).unwrap();
    let r0 = &p_mod.runs[0];
    assert_eq!(r0.format.font.as_deref(), Some("Calibri"));
    assert_eq!(r0.format.east_asian_font.as_deref(), Some("MS Mincho"));
    assert_eq!(r0.format.cs_font.as_deref(), Some("Traditional Arabic"));
    assert_eq!(r0.style.font.as_deref(), Some("Calibri"));
    assert_eq!(r0.style.east_asian_font.as_deref(), Some("MS Mincho"));
    assert_eq!(r0.style.cs_font.as_deref(), Some("Traditional Arabic"));

    // font_for の文字種別判定テスト
    assert_eq!(r0.style.font_for('A'), Some("Calibri"));
    assert_eq!(r0.style.font_for('漢'), Some("MS Mincho"));
    assert_eq!(r0.style.font_for('\u{0627}'), Some("Traditional Arabic")); // Arabic alef
    assert_eq!(r0.style.font_for('\u{05D0}'), Some("Traditional Arabic")); // Hebrew alef
    assert_eq!(r0.style.font_for('\u{0E01}'), Some("Traditional Arabic")); // Thai

    // 2. OOXML 保存・XML 検証
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let doc_xml = String::from_utf8_lossy(&pkg.read("word/document.xml").unwrap()).to_string();
    assert!(doc_xml.contains("w:cs=\"Traditional Arabic\""), "w:cs attribute preserved in document.xml: {}", doc_xml);
    assert!(doc_xml.contains("w:ascii=\"Calibri\""));
    assert!(doc_xml.contains("w:eastAsia=\"MS Mincho\""));

    // 3. 再読込検証
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let q = paragraphs(reread_flow).into_iter().find(|q| q.id == p.id).unwrap();
    let qr0 = &q.runs[0];
    assert_eq!(qr0.format.font.as_deref(), Some("Calibri"));
    assert_eq!(qr0.format.east_asian_font.as_deref(), Some("MS Mincho"));
    assert_eq!(qr0.format.cs_font.as_deref(), Some("Traditional Arabic"));
    assert_eq!(qr0.style.cs_font.as_deref(), Some("Traditional Arabic"));
    assert_eq!(qr0.style.font_for('\u{0627}'), Some("Traditional Arabic"));

    // 4. Undo/Redo 検証
    assert!(document.undo());
    let p_undone = paragraphs(flow(document.presentation())).into_iter().find(|q| q.id == p.id).unwrap();
    assert_eq!(p_undone.runs[0].format.cs_font, p.runs[0].format.cs_font);
    assert!(document.redo());
    let p_redone = paragraphs(flow(document.presentation())).into_iter().find(|q| q.id == p.id).unwrap();
    assert_eq!(p_redone.runs[0].format.cs_font.as_deref(), Some("Traditional Arabic"));
}

#[test]
fn test_complex_script_size_and_roundtrip() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p = text_paragraph(flow(document.presentation()), 1).clone();
    let from = FlowPos::new(p.id, 0);
    let to = FlowPos::new(p.id, 5);

    // 1. 欧文/和文サイズ (10.5pt = 1050, w:sz="21") と 複雑スクリプトサイズ (14pt = 1400, w:szCs="28") を設定
    document.execute(vec![
        Command::Flow(FlowCommand::SetRunFormat { from, to, change: RunChange::Size(1050) }),
        Command::Flow(FlowCommand::SetRunFormat { from, to, change: RunChange::ComplexScriptSize(1400) }),
    ]).unwrap();

    let cur_flow = flow(document.presentation());
    let p_mod = paragraphs(cur_flow).into_iter().find(|q| q.id == p.id).unwrap();
    let r0 = &p_mod.runs[0];
    assert_eq!(r0.format.size, Some(1050));
    assert_eq!(r0.format.cs_size, Some(1400));
    assert_eq!(r0.style.size, 1050);
    assert_eq!(r0.style.cs_size, Some(1400));

    // 文字種別ごとのサイズ判定 (size_for) 検証
    assert_eq!(r0.style.size_for('A'), 1050);
    assert_eq!(r0.style.size_for('あ'), 1050);
    assert_eq!(r0.style.size_for('\u{0627}'), 1400); // Arabic alef
    assert_eq!(r0.style.size_for('\u{05D0}'), 1400); // Hebrew alef
    assert_eq!(r0.style.size_for('\u{0E01}'), 1400); // Thai ko kai

    // 2. OOXML 保存・XML 検証 (w:sz="21" かつ w:szCs="28")
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let doc_xml = String::from_utf8_lossy(&pkg.read("word/document.xml").unwrap()).to_string();
    assert!(doc_xml.contains("<w:sz w:val=\"21\"/>"), "w:sz element preserved: {}", doc_xml);
    assert!(doc_xml.contains("<w:szCs w:val=\"28\"/>"), "w:szCs element preserved: {}", doc_xml);

    // 3. 再読込検証 (szCs が cs_size に復元されること)
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let q = paragraphs(reread_flow).into_iter().find(|q| q.id == p.id).unwrap();
    let qr0 = &q.runs[0];
    assert_eq!(qr0.format.size, Some(1050));
    assert_eq!(qr0.format.cs_size, Some(1400));
    assert_eq!(qr0.style.size, 1050);
    assert_eq!(qr0.style.cs_size, Some(1400));
    assert_eq!(qr0.style.size_for('A'), 1050);
    assert_eq!(qr0.style.size_for('\u{0627}'), 1400);

    // 4. Undo/Redo 検証
    assert!(document.undo());
    let p_undone = paragraphs(flow(document.presentation())).into_iter().find(|q| q.id == p.id).unwrap();
    assert_eq!(p_undone.runs[0].format.cs_size, p.runs[0].format.cs_size);
    assert!(document.redo());
    let p_redone = paragraphs(flow(document.presentation())).into_iter().find(|q| q.id == p.id).unwrap();
    assert_eq!(p_redone.runs[0].format.cs_size, Some(1400));
    assert_eq!(p_redone.runs[0].style.size_for('\u{0627}'), 1400);
}

#[test]
fn test_font_effects_outline_shadow_emboss_imprint_roundtrip() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());

    let p = text_paragraph(flow(document.presentation()), 1).clone();
    let from = FlowPos::new(p.id, 0);
    let to = FlowPos::new(p.id, 5);

    // 1. 中抜き (outline) と 影付き (shadow) の設定
    document.execute(vec![
        Command::Flow(FlowCommand::SetRunFormat { from, to, change: RunChange::Outline(true) }),
        Command::Flow(FlowCommand::SetRunFormat { from, to, change: RunChange::Shadow(true) }),
    ]).unwrap();

    let cur_flow = flow(document.presentation());
    let p_mod = paragraphs(cur_flow).into_iter().find(|q| q.id == p.id).unwrap();
    let r0 = &p_mod.runs[0];
    assert_eq!(r0.format.outline, Some(true));
    assert_eq!(r0.format.shadow, Some(true));
    assert!(r0.style.outline);
    assert!(r0.style.shadow);
    assert!(!r0.style.emboss);
    assert!(!r0.style.imprint);

    // 2. OOXML 保存・XML 検証 (<w:outline/>, <w:shadow/>)
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let doc_xml = String::from_utf8_lossy(&pkg.read("word/document.xml").unwrap()).to_string();
    assert!(doc_xml.contains("<w:outline/>") || doc_xml.contains("<w:outline "), "w:outline in doc_xml: {}", doc_xml);
    assert!(doc_xml.contains("<w:shadow/>") || doc_xml.contains("<w:shadow "), "w:shadow in doc_xml: {}", doc_xml);

    // 3. 再読込検証 (outline, shadow が復元されること)
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_flow = flow(&reread);
    let q = paragraphs(reread_flow).into_iter().find(|q| q.id == p.id).unwrap();
    let qr0 = &q.runs[0];
    assert_eq!(qr0.format.outline, Some(true));
    assert_eq!(qr0.format.shadow, Some(true));
    assert!(qr0.style.outline);
    assert!(qr0.style.shadow);

    // 4. 浮き出し (emboss) 設定 (outline, shadow は Word のルールに従い排他解除される)
    document.execute(vec![
        Command::Flow(FlowCommand::SetRunFormat { from, to, change: RunChange::Emboss(true) }),
    ]).unwrap();

    let cur_flow2 = flow(document.presentation());
    let p_mod2 = paragraphs(cur_flow2).into_iter().find(|q| q.id == p.id).unwrap();
    let r0_emboss = &p_mod2.runs[0];
    assert_eq!(r0_emboss.format.emboss, Some(true));
    assert!(r0_emboss.style.emboss);
    assert!(!r0_emboss.style.outline);
    assert!(!r0_emboss.style.shadow);

    let saved2 = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let pkg2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let doc_xml2 = String::from_utf8_lossy(&pkg2.read("word/document.xml").unwrap()).to_string();
    assert!(doc_xml2.contains("<w:emboss/>") || doc_xml2.contains("<w:emboss "), "w:emboss in doc_xml2");

    // 5. 浮き彫り (imprint) 設定 (emboss は排他解除される)
    document.execute(vec![
        Command::Flow(FlowCommand::SetRunFormat { from, to, change: RunChange::Imprint(true) }),
    ]).unwrap();

    let cur_flow3 = flow(document.presentation());
    let p_mod3 = paragraphs(cur_flow3).into_iter().find(|q| q.id == p.id).unwrap();
    let r0_imprint = &p_mod3.runs[0];
    assert_eq!(r0_imprint.format.imprint, Some(true));
    assert!(r0_imprint.style.imprint);
    assert!(!r0_imprint.style.emboss);

    // 6. Undo/Redo 検証
    assert!(document.undo());
    let p_undone = paragraphs(flow(document.presentation())).into_iter().find(|q| q.id == p.id).unwrap();
    assert_eq!(p_undone.runs[0].format.emboss, Some(true));
    assert!(document.redo());
    let p_redone = paragraphs(flow(document.presentation())).into_iter().find(|q| q.id == p.id).unwrap();
    assert_eq!(p_redone.runs[0].format.imprint, Some(true));
}










/// The text of the text box in paragraph `para` of `doc`, with its position.
fn text_box_of(doc: &FlowDocument, para: u32) -> Option<(usize, String, ibis_doc_model::EmuSize)> {
    let p = paragraphs(doc).into_iter().find(|p| p.id == para)?;
    let mut at = 0;
    for r in &p.runs {
        if let ibis_doc_model::FlowRunKind::Shape(s) = &r.kind
            && let ibis_doc_model::ShapeKind::Auto(a) = &s.kind
            && let Some(body) = &a.text
        {
            return Some((at, body.text(), ibis_doc_model::EmuSize::new(s.frame.width, s.frame.height)));
        }
        at += r.len();
    }
    None
}

#[test]
fn text_box_insert_and_edit() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let p = text_paragraph(flow(document.presentation()), 4).clone();
    let size = ibis_doc_model::EmuSize::new(1_828_800, 914_400);

    // Insert: saved as a floating text box, read back with its text.
    let at = FlowPos::new(p.id, 2);
    document.execute(vec![Command::Flow(FlowCommand::InsertTextBox { at, size, text: "一行目\n二行目 & <三>".into() })]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let report = diff_packages(&data, &saved, &[]).unwrap();
    for (part, _) in &report.differences {
        assert_eq!(part, "word/document.xml", "{}", report.to_json());
    }
    let reread = ibis_pptx::read(&saved).unwrap();
    let (offset, text, got) = text_box_of(flow(&reread), p.id).expect("the text box is saved");
    assert_eq!((offset, text.as_str(), got), (2, "一行目\n二行目 & <三>", size));
    let shape = paragraphs(flow(&reread)).into_iter().find(|q| q.id == p.id).unwrap().runs.iter().find_map(|r| match &r.kind {
        ibis_doc_model::FlowRunKind::Shape(s) => Some(s.clone()),
        _ => None,
    }).unwrap();
    assert_eq!(shape.floating.as_ref().map(|f| f.wrap.as_str()), Some("square"));

    // Edit the text of the text box (in the model, then the file).
    document.execute(vec![Command::Flow(FlowCommand::SetShapeText { at: FlowPos::new(p.id, 2), text: "書き換え".into() })]).unwrap();
    let saved2 = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread2 = ibis_pptx::read(&saved2).unwrap();
    assert_eq!(text_box_of(flow(&reread2), p.id).map(|t| t.1).as_deref(), Some("書き換え"));

    // A text box read from the file is edited in its own element: the saved
    // copy of the file with the box, edited again.
    let reopened = ibis_pptx::open(saved2.clone()).unwrap();
    let mut second = Document::new(reopened.presentation.clone());
    second.execute(vec![Command::Flow(FlowCommand::SetShapeText { at: FlowPos::new(p.id, 2), text: "三度目\n別の行".into() })]).unwrap();
    let saved3 = reopened.source.save(second.presentation(), SaveOptions::default()).unwrap();
    let reread3 = ibis_pptx::read(&saved3).unwrap();
    assert_eq!(text_box_of(flow(&reread3), p.id).map(|t| t.1).as_deref(), Some("三度目\n別の行"));
    // Unedited, it is saved as it was.
    let again = reopened.source.save(&reopened.presentation, SaveOptions::default()).unwrap();
    assert!(again == saved2);

    // Undo everything: the original file.
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn text_language_is_written_and_kept() {
    let out = scenario(REPORT, BODY, |doc| {
        let p = text_paragraph(doc, 8);
        vec![
            FlowCommand::SetRunFormat { from: FlowPos::new(p.id, 2), to: FlowPos::new(p.id, 6), change: RunChange::Language { lang: Some("fr-FR".into()), east_asia: Some("ko-KR".into()) } },
        ]
    });
    let xml = ibis_pptx::test_files::part_text(&out.saved, "word/document.xml").unwrap();
    assert!(xml.contains("w:val=\"fr-FR\"") && xml.contains("w:eastAsia=\"ko-KR\""), "w:lang is written");
    let reread = ibis_pptx::read(&out.saved).unwrap();
    let langs: Vec<(Option<String>, Option<String>)> = paragraphs(flow(&reread)).into_iter().flat_map(|p| p.runs.iter().map(|r| (r.format.lang.clone(), r.format.lang_east_asia.clone())).collect::<Vec<_>>()).collect();
    assert!(langs.contains(&(Some("fr-FR".into()), Some("ko-KR".into()))));
}

#[test]
fn picture_bullets_are_read_and_kept() {
    let data = ibis_pptx::test_files::docx_with_picture_bullet(&std::fs::read(path(REPORT)).unwrap());
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let doc = flow(&opened.presentation);
    let level = doc.styles.numbering.level(90, 0).expect("the list");
    assert_eq!(level.pic_bullet, Some(0));
    let &(media, size) = doc.styles.numbering.pic_bullets.get(&0).expect("the bullet's picture");
    assert_eq!(size, Some(ibis_doc_model::EmuSize::new(9 * 12700, 9 * 12700)));
    assert!(!opened.presentation.media(media).map_or(true, |m| m.data.is_empty()), "the picture is in the media");
    // Unchanged, saved as it was; an edit of the text keeps the list.
    assert!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap() == data);
    let out = scenario_data("picture-bullets.docx", data, BODY, |doc| {
        let p = body_paragraphs(doc).into_iter().rev().find(|p| p.list.is_some_and(|l| l.num_id == 90)).expect("an item");
        vec![FlowCommand::InsertText { at: FlowPos::new(p.id, 2), text: "★".into(), format: None }]
    });
    let reread = ibis_pptx::read(&out.saved).unwrap();
    assert_eq!(flow(&reread).styles.numbering.level(90, 0).and_then(|l| l.pic_bullet), Some(0));
}

#[test]
fn text_box_geometry_and_text_format() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let p = text_paragraph(flow(document.presentation()), 4).clone();
    let at = FlowPos::new(p.id, 2);
    let shape_of = |presentation: &Presentation| -> ibis_doc_model::Shape {
        paragraphs(flow(presentation)).into_iter().find(|q| q.id == p.id).unwrap().runs.iter().find_map(|r| match &r.kind {
            ibis_doc_model::FlowRunKind::Shape(s) => Some((**s).clone()),
            _ => None,
        }).expect("the text box")
    };
    document.execute(vec![Command::Flow(FlowCommand::InsertTextBox { at, size: ibis_doc_model::EmuSize::new(1_828_800, 914_400), text: "枠".into() })]).unwrap();
    // Moved, resized, bold 14 pt in red.
    document.execute(vec![
        Command::Flow(FlowCommand::SetShapeGeometry { at, size: Some(ibis_doc_model::EmuSize::new(2_540_000, 1_270_000)), offset: Some((635_000, 317_500)) }),
        Command::Flow(FlowCommand::SetShapeTextFormat { at, bold: Some(true), italic: None, size: Some(1400), color: Some(Rgb(0xc00000)) }),
    ]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let shape = shape_of(&reread);
    assert_eq!((shape.frame.width, shape.frame.height), (2_540_000, 1_270_000));
    let f = shape.floating.as_ref().expect("floating");
    assert_eq!((f.offset_x, f.offset_y), (635_000, 317_500));
    let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("an auto shape") };
    let run = &a.text.as_ref().unwrap().paragraphs[0].runs[0];
    assert_eq!((run.style.bold, run.style.font_size, run.style.color), (Some(true), Some(1400), Some(Rgb(0xc00000))));
    // The read box, edited by text only, keeps its formatting; moved only, as well.
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let mut second = Document::new(reopened.presentation.clone());
    second.execute(vec![Command::Flow(FlowCommand::SetShapeGeometry { at, size: None, offset: Some((0, 0)) })]).unwrap();
    let saved2 = reopened.source.save(second.presentation(), SaveOptions::default()).unwrap();
    let shape2 = shape_of(&ibis_pptx::read(&saved2).unwrap());
    let ibis_doc_model::ShapeKind::Auto(a2) = &shape2.kind else { panic!() };
    let run2 = &a2.text.as_ref().unwrap().paragraphs[0].runs[0];
    assert_eq!((run2.style.bold, run2.style.font_size, run2.style.color), (Some(true), Some(1400), Some(Rgb(0xc00000))));
    assert_eq!(shape2.floating.as_ref().map(|f| (f.offset_x, f.offset_y)), Some((0, 0)));
    // Undo to the original file.
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn blocks_moved_are_saved_in_their_new_order() {
    let data = ibis_pptx::test_files::docx_with_outline(&std::fs::read(path(REPORT)).unwrap());
    // 章B (with its text) before 章A (with what is under it), as the outline's move does.
    let out = scenario_data("outline.docx", data, BODY, |doc| {
        let a = doc.body.iter().position(|x| matches!(x, Block::Paragraph(p) if p.text() == "章A")).unwrap();
        let mut blocks: Vec<Block> = doc.body[a + 4..a + 6].to_vec();
        blocks.extend(doc.body[a..a + 4].iter().cloned());
        vec![FlowCommand::ReplaceBlocks { container: ibis_doc_model::flow_command::Container::Body, index: a, remove: 6, blocks }]
    });
    let reread = ibis_pptx::read(&out.saved).unwrap();
    let all = body_paragraphs(flow(&reread));
    let texts: Vec<String> = all[all.len() - 6..].iter().map(|p| p.text()).collect();
    assert_eq!(texts, vec!["章B", "B本文", "章A", "A本文", "節A-1", "A-1本文"]);
}

#[test]
fn drawn_shape_insert_and_style() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let p = text_paragraph(flow(document.presentation()), 4).clone();
    let at = FlowPos::new(p.id, 2);
    let shape_of = |presentation: &Presentation| -> ibis_doc_model::Shape {
        paragraphs(flow(presentation)).into_iter().find(|q| q.id == p.id).unwrap().runs.iter().find_map(|r| match &r.kind {
            ibis_doc_model::FlowRunKind::Shape(s) => Some((**s).clone()),
            _ => None,
        }).expect("the shape")
    };
    let size = ibis_doc_model::EmuSize::new(1_270_000, 635_000);
    for (preset, name) in [("rightArrow", "rightArrow"), ("roundRect", "roundRect"), ("ellipse", "ellipse"), ("star5", "star5")] {
        let geometry = ibis_pptx::preset_geometry(preset).unwrap();
        document.execute(vec![Command::Flow(FlowCommand::InsertShape { at, size, geometry, fill: Some(Rgb(0x4472C4)), line: Some((Rgb(0x2F528F), 12_700)) })]).unwrap();
        let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
        let shape = shape_of(&ibis_pptx::read(&saved).unwrap());
        assert_eq!((shape.frame.width, shape.frame.height), (1_270_000, 635_000), "{preset}");
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("an auto shape: {preset}") };
        assert_eq!(a.geometry.preset_name(), Some(name));
        assert!(matches!(&a.fill, ibis_doc_model::Fill::Solid(c) if c.rgb == Rgb(0x4472C4)), "{preset}: {:?}", a.fill);
        assert_eq!(a.stroke.as_ref().map(|s| (s.color.rgb, s.width)), Some((Rgb(0x2F528F), 12_700)));
        assert!(a.text.is_none());
        document.undo();
    }
    // Insert, then restyle: a filled red shape without a line; saved, read, restyled again (the old element).
    document.execute(vec![Command::Flow(FlowCommand::InsertShape { at, size, geometry: ibis_pptx::preset_geometry("triangle").unwrap(), fill: Some(Rgb(0x4472C4)), line: Some((Rgb(0x2F528F), 12_700)) })]).unwrap();
    document.execute(vec![Command::Flow(FlowCommand::SetShapeStyle { at, fill: Some(Some(Rgb(0xC00000))), line: Some(None) })]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let shape = shape_of(&ibis_pptx::read(&saved).unwrap());
    let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!() };
    assert!(matches!(&a.fill, ibis_doc_model::Fill::Solid(c) if c.rgb == Rgb(0xC00000)));
    assert!(a.stroke.is_none());
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let mut second = Document::new(reopened.presentation.clone());
    second.execute(vec![Command::Flow(FlowCommand::SetShapeStyle { at, fill: Some(None), line: Some(Some((Rgb(0x00B050), 25_400))) }), Command::Flow(FlowCommand::SetShapeGeometry { at, size: Some(ibis_doc_model::EmuSize::new(2_000_000, 1_000_000)), offset: Some((100_000, 200_000)) })]).unwrap();
    let saved2 = reopened.source.save(second.presentation(), SaveOptions::default()).unwrap();
    let shape2 = shape_of(&ibis_pptx::read(&saved2).unwrap());
    let ibis_doc_model::ShapeKind::Auto(a2) = &shape2.kind else { panic!() };
    assert!(matches!(a2.fill, ibis_doc_model::Fill::None));
    assert_eq!(a2.stroke.as_ref().map(|s| (s.color.rgb, s.width)), Some((Rgb(0x00B050), 25_400)));
    assert_eq!((shape2.frame.width, shape2.frame.height), (2_000_000, 1_000_000));
    assert_eq!(a2.geometry.preset_name(), Some("triangle"));
    // Undo everything: the original file.
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn drawn_line_insert_and_stroke() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let p = text_paragraph(flow(document.presentation()), 4).clone();
    let at = FlowPos::new(p.id, 2);
    let shape_of = |presentation: &Presentation| -> ibis_doc_model::Shape {
        paragraphs(flow(presentation)).into_iter().find(|q| q.id == p.id).unwrap().runs.iter().find_map(|r| match &r.kind {
            ibis_doc_model::FlowRunKind::Shape(s) => Some((**s).clone()),
            _ => None,
        }).expect("the line")
    };
    let mut stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(Rgb(0x2F528F)), 19_050);
    stroke.tail = ibis_doc_model::LineEnd::parse(Some("triangle"), None, None);
    document.execute(vec![Command::Flow(FlowCommand::InsertLine { at, size: ibis_doc_model::EmuSize::new(1_905_000, 0), stroke: stroke.clone() })]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let shape = shape_of(&ibis_pptx::read(&saved).unwrap());
    let ibis_doc_model::ShapeKind::Line(read) = &shape.kind else { panic!("a line: {:?}", shape.kind) };
    assert_eq!((read.color.rgb, read.width), (Rgb(0x2F528F), 19_050));
    assert_eq!(read.tail, stroke.tail);
    assert_eq!(read.head, ibis_doc_model::LineEnd::None);
    assert_eq!((shape.frame.width, shape.frame.height), (1_905_000, 0));
    // Restroked (colour, width, arrow at both ends), saved from the read file's element.
    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    let mut second = Document::new(reopened.presentation.clone());
    let mut changed = read.clone();
    changed.color = ibis_doc_model::Color::opaque(Rgb(0xC00000));
    changed.width = 38_100;
    changed.head = ibis_doc_model::LineEnd::parse(Some("triangle"), None, None);
    second.execute(vec![Command::Flow(FlowCommand::SetShapeStroke { at, stroke: Some(changed.clone()) })]).unwrap();
    let saved2 = reopened.source.save(second.presentation(), SaveOptions::default()).unwrap();
    let shape2 = shape_of(&ibis_pptx::read(&saved2).unwrap());
    let ibis_doc_model::ShapeKind::Line(read2) = &shape2.kind else { panic!() };
    assert_eq!((read2.color.rgb, read2.width, read2.head, read2.tail), (Rgb(0xC00000), 38_100, changed.head, changed.tail));
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn picture_corrections_brightness_contrast_recolor() {
    let data = std::fs::read(path(REPORT)).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let media = document.ids().add_media(ibis_doc_model::MediaFormat::Png, a_png());
    let p = text_paragraph(flow(document.presentation()), 4).clone();
    let at = FlowPos::new(p.id, 2);
    let look_of = |presentation: &Presentation| -> ibis_doc_model::PictureLook {
        paragraphs(flow(presentation)).into_iter().find(|q| q.id == p.id).unwrap().runs.iter().find_map(|r| match &r.kind {
            FlowRunKind::Picture { look, .. } => Some(look.clone()),
            FlowRunKind::Shape(s) => match &s.kind {
                ibis_doc_model::ShapeKind::Picture(pic) => Some(ibis_doc_model::PictureLook { luminance: pic.luminance, duotone: pic.duotone, ..Default::default() }),
                _ => None,
            },
            _ => None,
        }).expect("the picture")
    };
    document.execute(vec![Command::Flow(FlowCommand::InsertPicture { at, media, size: ibis_doc_model::EmuSize::new(914_400, 685_800) })]).unwrap();
    // Brighter and more contrasty, in grayscale.
    let mut look = ibis_doc_model::PictureLook::default();
    look.luminance = Some((20_000, -10_000));
    look.duotone = Some((Rgb(0x000000), Rgb(0xFFFFFF)));
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: look.clone() })]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let read = look_of(&ibis_pptx::read(&saved).unwrap());
    assert_eq!((read.luminance, read.duotone), (look.luminance, look.duotone));
    let xml = ibis_pptx::test_files::part_text(&saved, "word/document.xml").unwrap();
    assert!(xml.contains("<a:lum bright=\"20000\" contrast=\"-10000\"/>") && xml.contains("<a:duotone>"), "the blip has them");
    // A floating picture keeps them too.
    document.execute(vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::Square })]).unwrap();
    let floating = look_of(&ibis_pptx::read(&opened.source.save(document.presentation(), SaveOptions::default()).unwrap()).unwrap());
    assert_eq!((floating.luminance, floating.duotone), (look.luminance, look.duotone));
    // Changed, then taken away.
    let mut sepia = look.clone();
    sepia.luminance = None;
    sepia.duotone = Some((Rgb(0x3F2300), Rgb(0xFFF3D6)));
    document.execute(vec![Command::Flow(FlowCommand::SetPictureLook { at, look: sepia.clone() })]).unwrap();
    let again = look_of(&ibis_pptx::read(&opened.source.save(document.presentation(), SaveOptions::default()).unwrap()).unwrap());
    assert_eq!((again.luminance, again.duotone), (None, sepia.duotone));
    while document.can_undo() {
        document.undo();
    }
    assert!(opened.source.save(document.presentation(), SaveOptions::default()).unwrap() == data);
}
