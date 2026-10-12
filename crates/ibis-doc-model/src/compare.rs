// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 校閲 > 比較: the changes between two versions of a document, as tracked changes (what Word's 「比較」 makes).
//!
//! The paragraphs of the two versions are lined up by their text; a paragraph only in the original is shown deleted and one
//! only in the revision inserted; two that stand at the same place and look alike are compared by words (a run of letters
//! or digits, or one other character), the words only in the original deleted and those only in the revision inserted.
//! Tables stand as they are in the revision (their cells are not compared); a table only in the original is left out.

use crate::flow::{Block, FlowParagraph, FlowRun, Revision, RevisionKind};

/// A version's paragraph or table, by what it says (for lining the two versions up).
fn key(b: &Block) -> String {
    match b {
        Block::Paragraph(p) => p.text(),
        Block::Table(t) => {
            let mut s = String::from("\u{0}");
            for row in &t.rows {
                for cell in &row.cells {
                    for b in &cell.blocks {
                        if let Block::Paragraph(p) = b {
                            s.push_str(&p.text());
                            s.push('\u{1}');
                        }
                    }
                }
            }
            s
        }
    }
}

/// The longest common subsequence of `a` and `b` as pairs of indices; empty when the table would be too large.
fn lcs<T: PartialEq>(a: &[T], b: &[T]) -> Option<Vec<(usize, usize)>> {
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > 16_000_000 {
        return None;
    }
    let mut table = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[at(i, j)] = if a[i] == b[j] { table[at(i + 1, j + 1)] + 1 } else { table[at(i + 1, j)].max(table[at(i, j + 1)]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if table[at(i + 1, j)] >= table[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    Some(out)
}

/// The words of `text`: a run of ASCII letters or digits is one, any other character is one; as `(start, end)` in characters.
fn words(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        if chars[i].is_ascii_alphanumeric() {
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                i += 1;
            }
        } else {
            i += 1;
        }
        out.push((start, i));
    }
    out
}

/// How much two texts share, 0 to 1 (Dice on their characters).
fn similarity(a: &str, b: &str) -> f32 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut count = std::collections::HashMap::new();
    for c in &a {
        *count.entry(*c).or_insert(0i32) += 1;
    }
    let mut shared = 0;
    for c in &b {
        if let Some(n) = count.get_mut(c)
            && *n > 0
        {
            *n -= 1;
            shared += 1;
        }
    }
    2.0 * shared as f32 / (a.len() + b.len()) as f32
}

/// The runs of `p` from character `start` to `end`.
fn slice(p: &FlowParagraph, start: usize, end: usize) -> Vec<FlowRun> {
    let mut q = p.clone();
    q.split_off_runs(end);
    q.split_off_runs(start)
}

fn marked(mut runs: Vec<FlowRun>, kind: RevisionKind, author: &str, date: &str) -> Vec<FlowRun> {
    for r in &mut runs {
        r.revision = Some(Revision { kind: kind.clone(), author: author.to_string(), date: date.to_string() });
    }
    runs
}

/// `revised` with the words it changed from `original` marked as tracked changes.
fn compare_paragraphs(original: &FlowParagraph, revised: &FlowParagraph, author: &str, date: &str) -> FlowParagraph {
    let (ot, rt) = (original.text(), revised.text());
    let (ow, rw) = (words(&ot), words(&rt));
    let (oc, rc): (Vec<char>, Vec<char>) = (ot.chars().collect(), rt.chars().collect());
    let word = |c: &[char], w: (usize, usize)| c[w.0..w.1].iter().collect::<String>();
    let (ok, rk): (Vec<String>, Vec<String>) = (ow.iter().map(|w| word(&oc, *w)).collect(), rw.iter().map(|w| word(&rc, *w)).collect());
    let mut out = revised.clone();
    out.runs.clear();
    let Some(pairs) = lcs(&ok, &rk) else {
        out.runs.extend(marked(slice(original, 0, oc.len()), RevisionKind::Delete, author, date));
        out.runs.extend(marked(slice(revised, 0, rc.len()), RevisionKind::Insert, author, date));
        return out;
    };
    let (mut oi, mut ri) = (0, 0);
    let mut steps: Vec<(usize, usize)> = pairs;
    steps.push((ow.len(), rw.len()));
    for (po, pr) in steps {
        // What lies between the matches: the original's words deleted, the revision's inserted.
        if oi < po {
            out.runs.extend(marked(slice(original, ow[oi].0, ow[po - 1].1), RevisionKind::Delete, author, date));
        }
        if ri < pr {
            out.runs.extend(marked(slice(revised, rw[ri].0, rw[pr - 1].1), RevisionKind::Insert, author, date));
        }
        if po < ow.len() && pr < rw.len() {
            out.runs.extend(slice(revised, rw[pr].0, rw[pr].1));
        }
        oi = po + 1;
        ri = pr + 1;
    }
    // Neighbouring pieces of the same text and formatting become one run again.
    out
}

/// The body of `revised` with the changes from `original` as tracked changes by `author` at `date`.
pub fn compare_blocks(original: &[Block], revised: &[Block], author: &str, date: &str) -> Vec<Block> {
    let (ok, rk): (Vec<String>, Vec<String>) = (original.iter().map(key).collect(), revised.iter().map(key).collect());
    let mut next_id = original.iter().chain(revised).map(|b| match b { Block::Paragraph(p) => p.id, Block::Table(t) => t.id }).max().unwrap_or(0) + 1;
    let mut anchors = lcs(&ok, &rk).unwrap_or_default();
    anchors.push((original.len(), revised.len()));
    let mut out = Vec::new();
    let (mut oi, mut ri) = (0, 0);
    for (po, pr) in anchors {
        // The gap: original[oi..po] only in the original, revised[ri..pr] only in the revision.
        let deleted: Vec<&Block> = original[oi..po].iter().collect();
        let mut at = 0;
        // Paragraphs only in the revision wait until the deleted ones before them are put, as Word shows a deletion first.
        let mut waiting: Vec<Block> = Vec::new();
        for b in &revised[ri..pr] {
            let Block::Paragraph(r) = b else {
                waiting.push(b.clone());
                continue;
            };
            // The first remaining original paragraph that looks like this one: it is this one, changed.
            let found = (at..deleted.len()).find(|&j| matches!(deleted[j], Block::Paragraph(o) if similarity(&o.text(), &r.text()) >= 0.5 && !o.text().is_empty()));
            match found {
                Some(j) => {
                    for d in &deleted[at..j] {
                        if let Block::Paragraph(d) = d {
                            let mut d = d.clone();
                            d.id = next_id;
                            next_id += 1;
                            let len = d.text().chars().count();
                            d.runs = marked(slice(&d, 0, len), RevisionKind::Delete, author, date);
                            out.push(Block::Paragraph(d));
                        }
                    }
                    out.append(&mut waiting);
                    if let Block::Paragraph(o) = deleted[j] {
                        out.push(Block::Paragraph(compare_paragraphs(o, r, author, date)));
                    }
                    at = j + 1;
                }
                None => {
                    let mut p = r.clone();
                    let len = p.text().chars().count();
                    p.runs = marked(slice(r, 0, len), RevisionKind::Insert, author, date);
                    waiting.push(Block::Paragraph(p));
                }
            }
        }
        for d in &deleted[at..] {
            if let Block::Paragraph(d) = d {
                let mut d = d.clone();
                d.id = next_id;
                next_id += 1;
                let len = d.text().chars().count();
                d.runs = marked(slice(&d, 0, len), RevisionKind::Delete, author, date);
                out.push(Block::Paragraph(d));
            }
        }
        out.append(&mut waiting);
        if pr < revised.len() {
            out.push(revised[pr].clone());
        }
        oi = po + 1;
        ri = pr + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{FlowRunKind, FlowRunStyle};

    fn paragraph(id: u32, text: &str) -> Block {
        let mut p = FlowParagraph { id, ..FlowParagraph::default() };
        if !text.is_empty() {
            p.runs.push(FlowRun::new(FlowRunKind::Text(text.to_string()), FlowRunStyle::default()));
        }
        Block::Paragraph(p)
    }

    /// The paragraph's text with deletions in 【】 and insertions in 《》.
    fn show(b: &Block) -> String {
        let Block::Paragraph(p) = b else { return "[table]".into() };
        p.runs
            .iter()
            .map(|r| {
                let FlowRunKind::Text(t) = &r.kind else { return String::new() };
                match r.revision.as_ref().map(|v| &v.kind) {
                    Some(RevisionKind::Delete) => format!("【{t}】"),
                    Some(RevisionKind::Insert) => format!("《{t}》"),
                    _ => t.clone(),
                }
            })
            .collect()
    }

    #[test]
    fn words_changed_inside_a_paragraph() {
        let o = [paragraph(1, "第1条 甲は乙に100万円を支払う。"), paragraph(2, "第2条 期間は1年とする。")];
        let r = [paragraph(1, "第1条 甲は乙に200万円を支払う。"), paragraph(2, "第2条 期間は1年とする。")];
        let out = compare_blocks(&o, &r, "me", "2026-10-11T00:00:00Z");
        let shown: Vec<String> = out.iter().map(show).collect();
        assert_eq!(shown, ["第1条 甲は乙に【100】《200》万円を支払う。", "第2条 期間は1年とする。"]);
    }

    #[test]
    fn paragraphs_added_and_removed() {
        let o = [paragraph(1, "前文"), paragraph(2, "削る段落です"), paragraph(3, "末尾")];
        let r = [paragraph(1, "前文"), paragraph(2, "足した段落です、まったく別の文"), paragraph(3, "末尾")];
        let out = compare_blocks(&o, &r, "me", "d");
        let shown: Vec<String> = out.iter().map(show).collect();
        assert_eq!(shown, ["前文", "【削る段落です】", "《足した段落です、まったく別の文》", "末尾"]);
        let ids: std::collections::HashSet<u32> = out.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).collect();
        assert_eq!(ids.len(), 4, "ids are distinct");
    }

    #[test]
    fn same_text_has_no_changes() {
        let o = [paragraph(1, "同じ"), paragraph(2, "")];
        let out = compare_blocks(&o, &o.clone(), "me", "d");
        assert_eq!(out.iter().map(show).collect::<Vec<_>>(), ["同じ", ""]);
    }
}
