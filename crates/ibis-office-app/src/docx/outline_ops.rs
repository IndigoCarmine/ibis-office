// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! アウトライン操作 (Word's ナビゲーション ウィンドウの 見出しのレベル上げ・下げ、
//! 見出しを上へ・下へ移動): a heading's level changes with its sub-headings, and a heading
//! moves with everything under it (up to the next heading of its level or above).

use gpui_kit::*;
use ibis_doc_model::{Block, FlowCommand, FlowDocument, FlowPos, ParaChange, flow_command};

use crate::shell::editor::SlideEditor;

/// The heading level of body block `index`.
fn level_at(doc: &FlowDocument, index: usize) -> Option<u32> {
    match doc.body.get(index)? {
        Block::Paragraph(p) => flow_command::heading_level(&doc.styles, p),
        _ => None,
    }
}

/// The heading at or before body block `index`: the one the caret is under.
fn heading_for(doc: &FlowDocument, index: usize) -> Option<usize> {
    (0..=index.min(doc.body.len().saturating_sub(1))).rev().find(|&i| level_at(doc, i).is_some())
}

/// The blocks of the heading at `index` and what is under it: to before the next heading
/// of its level or above.
fn range_of(doc: &FlowDocument, index: usize) -> std::ops::Range<usize> {
    let level = level_at(doc, index).unwrap_or(1);
    let end = (index + 1..doc.body.len()).find(|&i| level_at(doc, i).is_some_and(|l| l <= level)).unwrap_or(doc.body.len());
    index..end
}

/// Whether a block holds a section break (moving it would move the section's end).
fn has_section(block: &Block) -> bool {
    matches!(block, Block::Paragraph(p) if p.section.is_some())
}

/// The document as shown with the headings `collapsed` closed: without the blocks under
/// them. `None` when none of them is a heading with something under it.
pub(crate) fn collapsed_view(doc: &FlowDocument, collapsed: &std::collections::BTreeSet<u32>) -> Option<FlowDocument> {
    let mut hidden = vec![false; doc.body.len()];
    let mut any = false;
    for (i, block) in doc.body.iter().enumerate() {
        if hidden[i] || !matches!(block, Block::Paragraph(p) if collapsed.contains(&p.id)) || level_at(doc, i).is_none() {
            continue;
        }
        for h in range_of(doc, i).skip(1) {
            hidden[h] = true;
            any = true;
        }
    }
    any.then(|| {
        let mut view = doc.clone();
        view.body = doc.body.iter().zip(&hidden).filter(|(_, h)| !**h).map(|(b, _)| b.clone()).collect();
        view
    })
}

impl SlideEditor {
    /// The body index of the heading the caret is under.
    fn heading_index_at_caret(&self) -> Option<usize> {
        let (start, _) = self.ordered_selection()?;
        let doc = self.flow()?;
        let index = doc.body.iter().position(|b| matches!(b, Block::Paragraph(p) if p.id == start.para))?;
        heading_for(doc, index)
    }

    /// 見出しのレベルを上げる (`delta` -1) / 下げる (+1): the heading under the caret and its
    /// sub-headings, one level each. Whether it was done.
    pub(crate) fn change_heading_level(&mut self, delta: i32, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.heading_index_at_caret() else { return false };
        let Some(doc) = self.flow() else { return false };
        let mut commands = Vec::new();
        for i in range_of(doc, index) {
            let (Some(level), Block::Paragraph(p)) = (level_at(doc, i), &doc.body[i]) else { continue };
            let target = level as i32 + delta;
            let id = (1..=9).contains(&target).then(|| doc.styles.paragraph_style_named(&format!("heading {target}"))).flatten();
            let Some(id) = id else {
                self.status = Some(crate::shell::i18n::text("docx.outline-ops.heading-level-cannot-changed-any"));
                cx.notify();
                return false;
            };
            commands.push(FlowCommand::SetParagraphFormat { first: p.id, last: p.id, change: ParaChange::Style(Some(id.to_string())) });
        }
        let keep = self.text_selection();
        let done = !commands.is_empty() && self.run_flow(commands, cx);
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = (a != c).then_some(a);
            state.caret = Some(c);
        }
        done
    }

    /// 見出しを上へ・下へ移動: the heading under the caret with its contents, past the
    /// neighbouring heading of the same level (Word's navigation pane keeps the order of levels
    /// the same). Whether it was done.
    pub(crate) fn move_heading(&mut self, up: bool, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.heading_index_at_caret() else { return false };
        let Some(doc) = self.flow() else { return false };
        let level = level_at(doc, index).unwrap_or(1);
        let this = range_of(doc, index);
        // The neighbour: the previous heading of the same level (before a higher one), or the
        // next range that follows this.
        let other = if up {
            let mut found = None;
            for i in (0..index).rev() {
                match level_at(doc, i) {
                    Some(l) if l == level => {
                        found = Some(i..this.start);
                        break;
                    }
                    Some(l) if l < level => break,
                    _ => {}
                }
            }
            found
        } else {
            (this.end < doc.body.len() && level_at(doc, this.end) == Some(level)).then(|| this.end..range_of(doc, this.end).end)
        };
        let Some(other) = other else {
            self.status = Some(crate::shell::i18n::text("docx.outline-ops.cannot-move-any-further"));
            cx.notify();
            return false;
        };
        let (first, second) = if up { (other.clone(), this.clone()) } else { (this.clone(), other.clone()) };
        if doc.body[first.start..second.end].iter().any(has_section) {
            self.status = Some(crate::shell::i18n::text("docx.outline-ops.cannot-move-across-section-break"));
            cx.notify();
            return false;
        }
        let mut blocks: Vec<Block> = doc.body[second.clone()].to_vec();
        blocks.extend(doc.body[first.clone()].iter().cloned());
        let Some(Block::Paragraph(moved)) = doc.body.get(this.start) else { return false };
        let id = moved.id;
        let command = FlowCommand::ReplaceBlocks { container: flow_command::Container::Body, index: first.start, remove: second.end - first.start, blocks };
        let done = self.run_flow(vec![command], cx);
        if done {
            self.set_caret(FlowPos::new(id, 0), false, cx);
            self.scroll_to_caret(cx);
        }
        done
    }
}

impl SlideEditor {
    /// Whether the heading `id` is shown collapsed.
    pub(crate) fn heading_collapsed(&self, id: u32) -> bool {
        self.resources.collapsed_headings.contains(&id)
    }

    /// Whether heading `id` has anything under it (a sub-heading or text).
    pub(crate) fn heading_has_content(&self, id: u32) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(i) = doc.body.iter().position(|b| matches!(b, Block::Paragraph(p) if p.id == id)) else { return false };
        level_at(doc, i).is_some() && range_of(doc, i).len() > 1
    }

    /// 見出しの折りたたみ・展開: close (`collapse`) or open the heading `id`; the caret under it
    /// goes to it. Not an edit: not in the undo steps, the file untouched.
    pub(crate) fn set_heading_collapsed(&mut self, id: u32, collapse: bool, cx: &mut Context<Self>) {
        if self.resources.collapsed_headings.contains(&id) == collapse {
            return;
        }
        let mut set = (*self.resources.collapsed_headings).clone();
        if collapse {
            if !self.heading_has_content(id) {
                return;
            }
            set.insert(id);
        } else {
            set.remove(&id);
        }
        self.resources.collapsed_headings = std::sync::Arc::new(set);
        if collapse {
            // A caret under the closed heading goes to the heading.
            let hidden_caret = self.flow().zip(self.text_selection()).is_some_and(|(doc, (_, c))| {
                let Some(i) = doc.body.iter().position(|b| matches!(b, Block::Paragraph(p) if p.id == id)) else { return false };
                range_of(doc, i).skip(1).any(|k| matches!(&doc.body[k], Block::Paragraph(p) if p.id == c.para))
            });
            if hidden_caret {
                self.set_caret(FlowPos::new(id, 0), false, cx);
            }
        }
        self.relayout_now(cx);
        cx.notify();
    }

    /// The heading under the caret, collapsed or expanded.
    pub(crate) fn toggle_heading_at_caret(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.heading_index_at_caret() else { return };
        let Some(Block::Paragraph(p)) = self.flow().and_then(|d| d.body.get(index)) else { return };
        let id = p.id;
        let now = self.heading_collapsed(id);
        self.set_heading_collapsed(id, !now, cx);
    }

    /// Every heading closed (`collapse`) or opened.
    pub(crate) fn set_all_headings_collapsed(&mut self, collapse: bool, cx: &mut Context<Self>) {
        let set: std::collections::BTreeSet<u32> = if collapse {
            let Some(doc) = self.flow() else { return };
            (0..doc.body.len()).filter(|&i| level_at(doc, i).is_some() && range_of(doc, i).len() > 1).filter_map(|i| match &doc.body[i] { Block::Paragraph(p) => Some(p.id), _ => None }).collect()
        } else {
            Default::default()
        };
        self.resources.collapsed_headings = std::sync::Arc::new(set);
        if collapse && let Some(id) = self.text_selection().map(|(_, c)| c.para) {
            // A caret in what is hidden goes to the last block shown before it.
            let target = self.flow().and_then(|doc| {
                let view = collapsed_view(doc, &self.resources.collapsed_headings)?;
                let shown: std::collections::HashSet<u32> = view.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).collect();
                if shown.contains(&id) {
                    return None;
                }
                let at = doc.body.iter().position(|b| matches!(b, Block::Paragraph(p) if p.id == id))?;
                doc.body[..at].iter().rev().find_map(|b| match b { Block::Paragraph(p) if shown.contains(&p.id) => Some(p.id), _ => None })
            });
            if let Some(target) = target {
                self.set_caret(FlowPos::new(target, 0), false, cx);
            }
        }
        self.relayout_now(cx);
        cx.notify();
    }
}
