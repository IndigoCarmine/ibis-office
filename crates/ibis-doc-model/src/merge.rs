// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 差し込み印刷 (mail merge): the `MERGEFIELD` fields of a document filled with the values of a
//! record, and a document made of one copy of the main document for each record.

use std::collections::HashMap;

use crate::flow::{Block, FlowDocument, FlowParagraph, FlowRun, FlowRunKind};
use crate::flow_command::renumbered;

/// The field a `MERGEFIELD` instruction names (`MERGEFIELD 氏名 \* MERGEFORMAT`, or
/// `MERGEFIELD "Last Name"`).
pub fn merge_field_name(instruction: &str) -> Option<String> {
    let rest = instruction.trim().strip_prefix("MERGEFIELD").or_else(|| instruction.trim().strip_prefix("mergefield"))?;
    let rest = rest.trim_start();
    let name = if let Some(quoted) = rest.strip_prefix('"') {
        quoted.split('"').next()?.to_string()
    } else {
        rest.split_whitespace().next()?.to_string()
    };
    (!name.is_empty() && !name.starts_with('\\')).then_some(name)
}

/// The instruction of the `MERGEFIELD` for field `name`.
pub fn merge_field_instruction(name: &str) -> String {
    if name.contains(char::is_whitespace) { format!("MERGEFIELD \"{name}\" ") } else { format!("MERGEFIELD {name} ") }
}

/// What a merge field shows before it is merged (Word's «氏名»).
pub fn merge_field_placeholder(name: &str) -> String {
    format!("«{name}»")
}

/// The fields of `doc`'s main text (their names, in the order they are first used).
pub fn merge_fields_used(doc: &FlowDocument) -> Vec<String> {
    fn walk(blocks: &[Block], out: &mut Vec<String>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        if let FlowRunKind::Field { instruction, .. } = &r.kind
                            && let Some(name) = merge_field_name(instruction)
                            && !out.contains(&name)
                        {
                            out.push(name);
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    walk(&doc.body, &mut out);
    out
}

/// The first word of a field's instruction, upper-cased (`IF`, `MERGEFIELD`).
fn first_word(instruction: &str) -> String {
    instruction.split_whitespace().next().unwrap_or("").to_ascii_uppercase()
}

/// `instruction` with each `{ MERGEFIELD 名 }` in it replaced by the record's value (a value with a space and no quotes
/// around the field gets quotes, so the comparison reads it as one word).
fn expand_merge_fields(instruction: &str, value: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = instruction;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}').map(|c| open + c) else { break };
        let inner = rest[open + 1..close].trim();
        out.push_str(&rest[..open]);
        if let Some(name) = merge_field_name(inner) {
            let text = merged_text(inner, &name, value);
            if !out.ends_with('"') && text.contains(char::is_whitespace) {
                out.push_str(&format!("\"{text}\""));
            } else {
                out.push_str(&text);
            }
        } else {
            out.push_str(&rest[open..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// What a `MERGEFIELD` shows: its value, with the case switches (`\* Upper`…) and the text before (`\b`) and after (`\f`)
/// it when the value is not empty.
fn merged_text(instruction: &str, name: &str, value: &dyn Fn(&str) -> Option<String>) -> String {
    let text = value(name).unwrap_or_default();
    if text.is_empty() {
        return text;
    }
    let words = crate::field_format::field_words(instruction);
    let switch = |flag: &str| words.iter().position(|w| w == flag).and_then(|i| words.get(i + 1)).cloned().unwrap_or_default();
    let text = crate::field_format::apply_text_switches(text, &words);
    format!("{}{text}{}", switch("\\b"), switch("\\f"))
}

/// What `instruction` shows once merged, or `None` for a field that is not a merge's.
fn merged_field_text(instruction: &str, value: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    if let Some(name) = merge_field_name(instruction) {
        return Some(merged_text(instruction, &name, value));
    }
    match first_word(instruction).as_str() {
        "IF" if instruction.contains('{') => Some(crate::field_format::if_field_text(&expand_merge_fields(instruction, value)).unwrap_or_default()),
        "COMPARE" if instruction.contains('{') => Some(crate::field_format::compare_field_text(&expand_merge_fields(instruction, value)).unwrap_or_default()),
        "SKIPIF" => Some(String::new()),
        _ => None,
    }
}

fn fill_paragraph(p: &mut FlowParagraph, value: &dyn Fn(&str) -> Option<String>) {
    for run in &mut p.runs {
        let FlowRunKind::Field { instruction, .. } = &run.kind else { continue };
        let Some(text) = merged_field_text(instruction, value) else { continue };
        run.kind = FlowRunKind::Text(text);
    }
    p.normalize();
}

/// Whether a `SKIPIF` field in `blocks` holds for the record `value` gives (`SKIPIF { MERGEFIELD 氏名 } = ""`): that
/// record is left out of the letters.
fn skipped(blocks: &[Block], value: &dyn Fn(&str) -> Option<String>) -> bool {
    blocks.iter().any(|b| match b {
        Block::Paragraph(p) => p.runs.iter().any(|r| {
            let FlowRunKind::Field { instruction, .. } = &r.kind else { return false };
            let trimmed = instruction.trim_start();
            first_word(trimmed) == "SKIPIF"
                && crate::field_format::compare_field_text(&expand_merge_fields(&format!("COMPARE{}", &trimmed[6..]), value)).as_deref() == Some("1")
        }),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).any(|c| skipped(&c.blocks, value)),
    })
}

fn fill_blocks(blocks: &mut [Block], value: &dyn Fn(&str) -> Option<String>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => fill_paragraph(p, value),
            Block::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        fill_blocks(&mut cell.blocks, value);
                    }
                }
            }
        }
    }
}

/// `doc` with each merge field replaced by its value in `values` (a field it has no value for: empty), its headers and
/// footers too: what 結果のプレビュー shows.
pub fn merged_view(doc: &FlowDocument, values: &HashMap<String, String>) -> FlowDocument {
    let value = |name: &str| values.get(name).or_else(|| values.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v)).cloned();
    let mut out = doc.clone();
    fill_blocks(&mut out.body, &value);
    for list in [&mut out.header, &mut out.footer, &mut out.header_first, &mut out.footer_first, &mut out.header_even, &mut out.footer_even] {
        fill_blocks(list, &value);
    }
    out
}

/// A document with one copy of `doc`'s main text for each record (`records` are the rows of `fields`' values), each
/// on a new page: 個々のドキュメントの編集.
pub fn merged_letters(doc: &FlowDocument, fields: &[String], records: &[Vec<String>]) -> FlowDocument {
    let mut out = doc.clone();
    let mut body: Vec<Block> = Vec::new();
    let mut next = doc.new_id();
    let mut n = 0;
    for record in records {
        let values: HashMap<String, String> = fields.iter().cloned().zip(record.iter().cloned()).collect();
        let lookup = |name: &str| values.get(name).or_else(|| values.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v)).cloned();
        if skipped(&doc.body, &lookup) {
            continue;
        }
        let copy = merged_view(doc, &values);
        let mut blocks = if n == 0 { copy.body } else { renumbered(&copy.body, &mut next) };
        if n > 0 {
            match blocks.first_mut() {
                Some(Block::Paragraph(p)) => p.page_break_before = true,
                _ => {
                    let mut p = FlowParagraph { id: next, ..FlowParagraph::default() };
                    next += 1;
                    p.page_break_before = true;
                    p.runs.push(FlowRun::new(FlowRunKind::Text(String::new()), Default::default()));
                    blocks.insert(0, Block::Paragraph(p));
                }
            }
        }
        body.extend(blocks);
        n += 1;
    }
    if body.is_empty() {
        body = doc.body.clone();
    }
    out.body = body;
    out
}

/// A table of text values read from `text`: comma, tab or semicolon separated (the first line's, whichever it has most), quotes as CSV
/// has them; the first line the field names. Empty lines are left out.
pub fn parse_table(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().next().unwrap_or("");
    let separator = [',', '\t', ';'].into_iter().max_by_key(|c| first.matches(*c).count()).filter(|c| first.contains(*c)).unwrap_or(',');
    let mut rows: Vec<Vec<String>> = Vec::new();
    let (mut row, mut cell, mut quoted, mut chars) = (Vec::new(), String::new(), false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == separator && !quoted => row.push(std::mem::take(&mut cell)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            c => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    let mut rows = rows.into_iter();
    let fields: Vec<String> = rows.next().unwrap_or_default().into_iter().map(|f| f.trim().to_string()).collect();
    let width = fields.len();
    let records = rows.map(|mut r| {
        r.resize(width, String::new());
        r
    }).collect();
    (fields, records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_names() {
        assert_eq!(merge_field_name("MERGEFIELD 氏名 \\* MERGEFORMAT").as_deref(), Some("氏名"));
        assert_eq!(merge_field_name(" MERGEFIELD \"Last Name\" ").as_deref(), Some("Last Name"));
        assert_eq!(merge_field_name("PAGE"), None);
        assert_eq!(merge_field_name(&merge_field_instruction("会社 名")).as_deref(), Some("会社 名"));
    }

    fn field(instruction: &str) -> FlowRun {
        FlowRun::new(FlowRunKind::Field { instruction: instruction.into(), shown: String::new() }, Default::default())
    }

    fn text_of(doc: &FlowDocument) -> Vec<String> {
        doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.runs.iter().filter_map(|r| if let FlowRunKind::Text(t) = &r.kind { Some(t.as_str()) } else { None }).collect()) } else { None }).collect()
    }

    #[test]
    fn if_and_switches_and_skipif() {
        let mut doc = FlowDocument::default();
        let mut p = FlowParagraph { id: 1, ..FlowParagraph::default() };
        p.runs.push(field("IF { MERGEFIELD 年齢 } >= 20 \"成人\" \"未成年\" "));
        p.runs.push(field("MERGEFIELD 氏名 \\b \"様: \" \\* Upper"));
        p.runs.push(field("IF \"{ MERGEFIELD 会社 }\" = \"ibis\" \"自社\" \"他社\""));
        doc.body.push(Block::Paragraph(p));
        let mut skip = FlowParagraph { id: 2, ..FlowParagraph::default() };
        skip.runs.push(field("SKIPIF { MERGEFIELD 年齢 } < 10"));
        let mut with_skip = doc.clone();
        with_skip.body.push(Block::Paragraph(skip));
        let values: HashMap<String, String> = [("年齢", "30"), ("氏名", "yamada"), ("会社", "ibis")].map(|(k, v)| (k.to_string(), v.to_string())).into_iter().collect();
        assert_eq!(text_of(&merged_view(&doc, &values)), ["成人様: YAMADA自社"]);
        let young: HashMap<String, String> = [("年齢", "5"), ("氏名", ""), ("会社", "a b")].map(|(k, v)| (k.to_string(), v.to_string())).into_iter().collect();
        assert_eq!(text_of(&merged_view(&doc, &young)), ["未成年他社"]);
        let fields = ["年齢".to_string(), "氏名".to_string(), "会社".to_string()];
        let records = vec![vec!["30".into(), "a".into(), "x".into()], vec!["5".into(), "b".into(), "y".into()], vec!["12".into(), "c".into(), "z".into()]];
        let letters = merged_letters(&with_skip, &fields, &records);
        assert_eq!(letters.body.len(), 4, "the record under 10 is left out");
    }

    #[test]
    fn tables_from_text() {
        let (fields, records) = parse_table("氏名,住所\n山田,\"東京都, 港区\"\n鈴木,大阪\n\n");
        assert_eq!(fields, ["氏名", "住所"]);
        assert_eq!(records, [["山田", "東京都, 港区"], ["鈴木", "大阪"]]);
        let (fields, records) = parse_table("a\tb\tc\n1\t2\n");
        assert_eq!((fields.len(), records), (3, vec![vec!["1".to_string(), "2".to_string(), String::new()]]));
    }
}
