// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Editing a document's text: the caret and the selection, moving them,
//! and turning keys, typing and toolbar buttons into flow commands
//! (`ibis_doc_model::FlowCommand`).
//!
//! What is here only decides which command to run; the commands (and
//! undo) are in the model, so a new toolbar or menu calls the same
//! methods. The caret's place on the pages comes from the layout's line
//! positions (`flow_layout::Item::Line`).

use std::sync::Arc;

use gpui_kit::*;
use ibis_doc_model::flow_command::{self, Container};
use ibis_doc_model::{
    Block, CaseConversion, Command, DocumentProtection, FlowAlign, FlowCommand, FlowDocument, FlowParagraph, FlowPos, FlowRunKind, InsertFormat, LineHeight, ParaChange, Rgb,
    RunChange, RunFormat,
};

use crate::docx::flow_layout::{Item, pages};
use crate::shell::editor::{MAX_ZOOM, MIN_ZOOM, RulerUnit, SlideEditor, zoom_scale};

/// Key context of a document's workspace while its text is edited.
pub(crate) const TEXT_CONTEXT: &str = "DocumentText";

#[derive(Clone, Debug, PartialEq, Eq)]
enum TocType {
    Headings((u32, u32)),
    Figures(String),
    Custom(flow_command::TocInstruction),
}

actions!(
    docx,
    [
        /// Delete the selection, or the character before the caret.
        DeleteBackward,
        /// Delete the selection, or the character after the caret.
        DeleteForward,
        /// End the paragraph (Enter).
        NewParagraph,
        /// A line break in the paragraph (Shift+Enter).
        InsertLineBreak,
        InsertTab,
        /// Shift+Tab: a list item one level up.
        OutdentList,
        /// Continue on the next page (⌘Enter).
        InsertPageBreak,
        /// Continue in the next column (⌃⇧↩ / Ctrl+Shift+Enter).
        InsertColumnBreak,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        DocumentStart,
        DocumentEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        SelectAll,
        Copy,
        Cut,
        Paste,
        PasteKeepSource,
        PastePlain,
        PasteMatchFormat,
        ToggleBold,
        ToggleItalic,
        ToggleUnderline,
        ToggleStrike,
        ToggleCaps,
        ToggleSmallCaps,
        ToggleSuperscript,
        ToggleSubscript,
        AlignLeft,
        AlignCenter,
        AlignRight,
        AlignJustify,
        AlignDistribute,
        GrowFont,
        ShrinkFont,
        ClearFormatting,
        /// 文字の囲み線.
        ToggleCharacterBorder,
        /// 文字の網かけ.
        ToggleCharacterShading,
        /// 検索と置換 (⌘F, ⇧⌘H).
        OpenFind,
        /// 次を検索 (⌘G, F3).
        FindNext,
        /// 前を検索 (⇧⌘G, ⇧F3).
        FindPrevious,
        /// ジャンプ (⌥⌘G, F5).
        OpenGoTo,
        /// リンク (⌘K).
        OpenLink,
        /// 文字種の変換 (DOCX-JA-020).
        ConvertCaseSentence,
        ConvertCaseLower,
        ConvertCaseUpper,
        ConvertCaseTitle,
        ConvertCaseToggle,
        ConvertCaseHalfWidth,
        ConvertCaseFullWidth,
        ConvertCaseKatakana,
        ConvertCaseHiragana,
        /// 書式のコピー (⌥⌘C / Ctrl+Shift+C).
        CopyFormat,
        /// 書式の貼り付け (⌥⌘V / Ctrl+Shift+V).
        PasteFormat,
        /// 書式ブラシの解除 (Esc).
        CancelFormatBrush,
        /// 編集記号の表示 (⌘8 / Ctrl+Shift+8).
        ToggleFormattingMarks,
        /// フィールドの更新 (F9).
        UpdateFields,
        /// フィールドのリンク解除 (⇧⌘F9 / Ctrl+Shift+F9).
        UnlinkFields,
        /// フィールドコードの表示/非表示 (Shift+F9).
        ToggleFieldCodes,
        /// すべてのフィールドコードの表示/非表示 (Alt+F9).
        ToggleAllFieldCodes,
        /// フィールドのロック (Ctrl+F11 / ⌘F11).
        LockField,
        /// フィールドのロック解除 (Ctrl+Shift+F11 / ⇧⌘F11).
        UnlockField,
        /// フィールドの挿入 (Ctrl+F9 / ⌘F9).
        InsertField,
        /// フィールドの編集.
        EditField,
        /// 上書きモード切替 (Insert / Overtype mode).
        ToggleOvertype,
        /// 隠し文字 (Ctrl+Shift+H / ⌘⇧X).
        ToggleHidden,
        /// ユーザー名の変更 (Word Options > 全般: ユーザー設定 DOCX-OPTIONS-003).
        ChangeUserName,
        /// ウィンドウの分割の切り替え (⌥⌘S / Alt+Ctrl+S DOCX-VIEW-019).
        ToggleSplit,
    ]
);

/// The keys of a document's text (Word's, with ⌘ on a Mac).
pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    let c = Some(TEXT_CONTEXT);
    let m = |k: &str| if cfg!(target_os = "macos") { format!("cmd-{k}") } else { format!("ctrl-{k}") };
    let word = |k: &str| if cfg!(target_os = "macos") { format!("alt-{k}") } else { format!("ctrl-{k}") };
    vec![
        KeyBinding::new("insert", ToggleOvertype, c),
        KeyBinding::new("backspace", DeleteBackward, c),
        KeyBinding::new("shift-backspace", DeleteBackward, c),
        KeyBinding::new("delete", DeleteForward, c),
        KeyBinding::new("enter", NewParagraph, c),
        KeyBinding::new("shift-enter", InsertLineBreak, c),
        KeyBinding::new("tab", InsertTab, c),
        KeyBinding::new("shift-tab", OutdentList, c),
        KeyBinding::new(&m("enter"), InsertPageBreak, c),
        KeyBinding::new("ctrl-shift-enter", InsertColumnBreak, c),
        KeyBinding::new(&m("shift-enter"), InsertColumnBreak, c),
        KeyBinding::new("left", MoveLeft, c),
        KeyBinding::new("right", MoveRight, c),
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new(&word("left"), WordLeft, c),
        KeyBinding::new(&word("right"), WordRight, c),
        KeyBinding::new(&word("shift-left"), SelectWordLeft, c),
        KeyBinding::new(&word("shift-right"), SelectWordRight, c),
        KeyBinding::new("home", LineStart, c),
        KeyBinding::new("end", LineEnd, c),
        KeyBinding::new("shift-home", SelectLineStart, c),
        KeyBinding::new("shift-end", SelectLineEnd, c),
        KeyBinding::new(&m("left"), LineStart, c),
        KeyBinding::new(&m("right"), LineEnd, c),
        KeyBinding::new(&m("shift-left"), SelectLineStart, c),
        KeyBinding::new(&m("shift-right"), SelectLineEnd, c),
        KeyBinding::new(&m("up"), DocumentStart, c),
        KeyBinding::new(&m("down"), DocumentEnd, c),
        KeyBinding::new(&m("home"), DocumentStart, c),
        KeyBinding::new(&m("end"), DocumentEnd, c),
        KeyBinding::new(&m("shift-up"), SelectDocumentStart, c),
        KeyBinding::new(&m("shift-down"), SelectDocumentEnd, c),
        KeyBinding::new(&m("a"), SelectAll, c),
        KeyBinding::new(&m("c"), Copy, c),
        KeyBinding::new(&m("x"), Cut, c),
        KeyBinding::new(&m("v"), Paste, c),
        KeyBinding::new(&m("shift-v"), PastePlain, c),
        KeyBinding::new("alt-cmd-shift-v", PasteMatchFormat, c),
        KeyBinding::new("alt-ctrl-shift-v", PasteMatchFormat, c),
        KeyBinding::new(&m("b"), ToggleBold, c),
        KeyBinding::new(&m("i"), ToggleItalic, c),
        KeyBinding::new(&m("u"), ToggleUnderline, c),
        KeyBinding::new(&m("shift-a"), ToggleCaps, c),
        KeyBinding::new(&m("shift-k"), ToggleSmallCaps, c),
        KeyBinding::new(&m("shift-="), ToggleSuperscript, c),
        KeyBinding::new(&m("="), ToggleSubscript, c),
        KeyBinding::new(&m("l"), AlignLeft, c),
        KeyBinding::new(&m("e"), AlignCenter, c),
        KeyBinding::new(&m("r"), AlignRight, c),
        KeyBinding::new(&m("j"), AlignJustify, c),
        KeyBinding::new(&m("shift-j"), AlignDistribute, c),
        KeyBinding::new("ctrl-shift-j", AlignDistribute, c),
        KeyBinding::new(&m("shift-."), GrowFont, c),
        KeyBinding::new(&m("shift-,"), ShrinkFont, c),
        KeyBinding::new(&m("]"), GrowFont, c),
        KeyBinding::new(&m("["), ShrinkFont, c),
        KeyBinding::new(&m("\\"), ClearFormatting, c),
        KeyBinding::new(&m("space"), ClearFormatting, c),
        KeyBinding::new(&m("f"), OpenFind, c),
        KeyBinding::new(&m("k"), OpenLink, c),
        KeyBinding::new(&m("shift-h"), OpenFind, c),
        KeyBinding::new(&m("g"), FindNext, c),
        KeyBinding::new(&m("shift-g"), FindPrevious, c),
        KeyBinding::new("f3", FindNext, c),
        KeyBinding::new("shift-f3", FindPrevious, c),
        KeyBinding::new("alt-cmd-g", OpenGoTo, c),
        KeyBinding::new("alt-ctrl-g", OpenGoTo, c),
        KeyBinding::new("f5", OpenGoTo, c),
        KeyBinding::new("alt-cmd-c", CopyFormat, c),
        KeyBinding::new("alt-ctrl-c", CopyFormat, c),
        KeyBinding::new("ctrl-shift-c", CopyFormat, c),
        KeyBinding::new("alt-cmd-v", PasteFormat, c),
        KeyBinding::new("alt-ctrl-v", PasteFormat, c),
        KeyBinding::new("ctrl-shift-v", PasteFormat, c),
        KeyBinding::new("escape", CancelFormatBrush, c),
        KeyBinding::new(&m("shift-8"), ToggleFormattingMarks, c),
        KeyBinding::new(&m("8"), ToggleFormattingMarks, c),
        KeyBinding::new("f9", UpdateFields, c),
        KeyBinding::new(&m("f9"), InsertField, c),
        KeyBinding::new("ctrl-f9", InsertField, c),
        KeyBinding::new(&m("shift-f9"), UnlinkFields, c),
        KeyBinding::new("ctrl-shift-f9", UnlinkFields, c),
        KeyBinding::new("shift-f9", ToggleFieldCodes, c),
        KeyBinding::new("alt-f9", ToggleAllFieldCodes, c),
        KeyBinding::new("alt-cmd-f9", ToggleAllFieldCodes, c),
        KeyBinding::new(&m("f11"), LockField, c),
        KeyBinding::new("ctrl-f11", LockField, c),
        KeyBinding::new(&m("shift-f11"), UnlockField, c),
        KeyBinding::new("ctrl-shift-f11", UnlockField, c),
        KeyBinding::new("ctrl-shift-h", ToggleHidden, c),
        KeyBinding::new(&m("shift-x"), ToggleHidden, c),
        KeyBinding::new(if cfg!(target_os = "macos") { "alt-cmd-s" } else { "alt-ctrl-s" }, ToggleSplit, c),
    ]
}

/// A line of text on a page, from the layout (points from the page's
/// top-left corner).
#[derive(Clone, Debug)]
pub(crate) struct LineGeom {
    pub(crate) page: usize,
    pub(crate) para: u32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) h: f32,
    pub(crate) stops: Arc<[(usize, f32)]>,
    pub(crate) vertical: bool,
}

impl LineGeom {
    /// Whether the caret at `offset` is shown on this line: a position on
    /// it, or where it ends when it is its paragraph's last line.
    fn holds(&self, offset: usize, last_line: bool) -> bool {
        let n = self.stops.len();
        self.stops.iter().enumerate().any(|(i, (o, _))| *o == offset && (i + 1 < n || last_line))
    }

    fn x_of(&self, offset: usize) -> f32 {
        if self.vertical {
            self.x
        } else {
            self.stops.iter().find(|(o, _)| *o == offset).map_or(self.x, |(_, x)| self.x + x)
        }
    }

    fn y_of(&self, offset: usize) -> f32 {
        if self.vertical {
            self.stops.iter().find(|(o, _)| *o == offset).map_or(self.y, |(_, y)| self.y + y)
        } else {
            self.y
        }
    }

    /// The position nearest `pos` (x along a horizontal line, y down a vertical line).
    fn nearest(&self, pos: f32) -> usize {
        let origin = if self.vertical { self.y } else { self.x };
        let mut best = (f32::INFINITY, 0);
        for (o, s) in self.stops.iter() {
            let d = (origin + s - pos).abs();
            if d < best.0 {
                best = (d, *o);
            }
        }
        best.1
    }
}

/// The order of the paragraphs, per list of blocks read in turn: the body
/// (with its tables' cells), the header, the footer.
pub(crate) fn order(doc: &FlowDocument) -> Vec<u32> {
    fn walk(blocks: &[Block], out: &mut Vec<u32>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.push(p.id),
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

/// The kind of a character, for moving by words.
fn class(c: char) -> u8 {
    match c {
        c if c.is_whitespace() => 0,
        c if c.is_ascii_punctuation() || matches!(c, '、' | '。' | '，' | '．' | '「' | '」' | '（' | '）') => 1,
        '\u{3040}'..='\u{309f}' => 2,
        '\u{30a0}'..='\u{30ff}' => 3,
        c if c as u32 >= 0x3400 => 4,
        _ => 5,
    }
}

/// Model positions in a deletion remain stable for saving/undo, but are not
/// caret stops in the final view. A boundary shared by hidden text is represented
/// by the end of the hidden run, so Left and Right each cross a visible character.
fn hidden_range(p: &FlowParagraph, offset: usize) -> Option<(usize, usize)> {
    let mut at = 0;
    for r in &p.runs {
        let end = at + r.len();
        if at <= offset && offset < end {
            let is_del = !crate::docx::flow_layout::showing_markup()
                && matches!(r.revision.as_ref().map(|v| &v.kind), Some(ibis_doc_model::RevisionKind::Delete));
            let is_vanish = r.style.hidden && !crate::docx::flow_layout::showing_hidden_text();
            if is_del || is_vanish {
                return Some((at, end));
            }
        }
        at = end;
    }
    None
}

/// A position strictly inside a hidden or deleted run (not at its endpoints).
fn hidden_range_strictly_inside(p: &FlowParagraph, offset: usize) -> Option<(usize, usize)> {
    let mut at = 0;
    for r in &p.runs {
        let end = at + r.len();
        if at < offset && offset < end {
            let is_del = !crate::docx::flow_layout::showing_markup()
                && matches!(r.revision.as_ref().map(|v| &v.kind), Some(ibis_doc_model::RevisionKind::Delete));
            let is_vanish = r.style.hidden && !crate::docx::flow_layout::showing_hidden_text();
            if is_del || is_vanish {
                return Some((at, end));
            }
        }
        at = end;
    }
    None
}

/// In final view, deleted text cannot supply the caret's character format.
/// Prefer the preceding visible run, then following text, then the mark.
pub(super) fn visible_format_at(p: &FlowParagraph, offset: usize) -> (RunFormat, Option<String>, ibis_doc_model::FlowRunStyle) {
    if crate::docx::flow_layout::showing_markup() && crate::docx::flow_layout::showing_hidden_text() || offset > p.len() { return p.format_at(offset); }
    let mut at = 0;
    let mut previous: Option<&ibis_doc_model::FlowRun> = None;
    let value = |r: &ibis_doc_model::FlowRun| (r.format.clone(), r.char_style.clone(), r.style.clone());
    for run in &p.runs {
        let end = at + run.len();
        let is_del = !crate::docx::flow_layout::showing_markup() && matches!(run.revision.as_ref().map(|r| &r.kind), Some(ibis_doc_model::RevisionKind::Delete));
        let is_vanish = run.style.hidden && !crate::docx::flow_layout::showing_hidden_text();
        if run.len() > 0 && !is_del && !is_vanish {
            if offset == 0 || (offset > at && offset <= end) { return value(run); }
            if end <= offset { previous = Some(run); }
            else if offset <= at { return value(previous.unwrap_or(run)); }
        }
        at = end;
    }
    previous.map(value).unwrap_or_else(|| (p.mark_format.clone(), None, p.mark.clone()))
}

/// The insertion point after TrackDelete physically removes older insertions.
pub(super) fn after_tracked_delete(doc: &FlowDocument, from: FlowPos, to: FlowPos) -> FlowPos {
    let Some(p) = flow_command::paragraph(doc, to.para) else { return to };
    let first = if from.para == to.para { from.offset } else { 0 };
    let mut removed = 0;
    let mut offset = 0;
    for run in &p.runs {
        let end = offset + run.len();
        if matches!(run.revision.as_ref().map(|r| &r.kind), Some(ibis_doc_model::RevisionKind::Insert)) {
            removed += end.min(to.offset).saturating_sub(offset.max(first));
        }
        offset = end;
    }
    FlowPos::new(to.para, to.offset.saturating_sub(removed))
}

/// Visible pieces of a single-paragraph selection, in safe deletion order.
pub(super) fn visible_delete_commands(p: &FlowParagraph, from: usize, to: usize) -> Vec<FlowCommand> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (offset, _) in visible_chars(p).into_iter().filter(|(o, _)| from <= *o && *o < to) {
        if let Some(last) = ranges.last_mut().filter(|last| last.1 == offset) { last.1 += 1; }
        else { ranges.push((offset, offset + 1)); }
    }
    ranges.into_iter().rev().map(|(from, to)| FlowCommand::Delete { from: FlowPos::new(p.id, from), to: FlowPos::new(p.id, to) }).collect()
}

pub(super) fn visible_chars(p: &FlowParagraph) -> Vec<(usize, char)> {
    let mut ranges = Vec::new();
    if !crate::docx::flow_layout::showing_markup() {
        let mut at = 0;
        for r in &p.runs {
            let end = at + r.len();
            if matches!(r.revision.as_ref().map(|v| &v.kind), Some(ibis_doc_model::RevisionKind::Delete)) { ranges.push((at, end)); }
            at = end;
        }
    }
    let mut i = 0;
    p.text().chars().enumerate().filter(|(offset, _)| {
        while i < ranges.len() && ranges[i].1 <= *offset { i += 1; }
        i == ranges.len() || *offset < ranges[i].0
    }).collect()
}

fn selected_paragraph_text(p: &FlowParagraph, from: usize, to: usize) -> String {
    visible_chars(p).into_iter().filter(|(o, c)| from <= *o && *o < to && *c != '\u{fffc}')
        .map(|(_, c)| if matches!(c, '\u{b}' | '\u{c}' | '\u{e}') { '\n' } else { c }).collect()
}

impl SlideEditor {
    pub(crate) fn flow(&self) -> Option<&Arc<FlowDocument>> {
        self.presentation().flow.as_ref()
    }

    /// The caret and the other end of the selection.
    pub(crate) fn text_selection(&self) -> Option<(FlowPos, FlowPos)> {
        let s = self.doc_state()?;
        let caret = s.caret?;
        Some((s.anchor.unwrap_or(caret), caret))
    }

    /// The selection in document order (start, end), when there is a caret.
    pub(crate) fn ordered_selection(&self) -> Option<(FlowPos, FlowPos)> {
        let (a, b) = self.text_selection()?;
        let doc = self.flow()?;
        Some(if self.before(doc, b, a) { (b, a) } else { (a, b) })
    }

    fn before(&self, doc: &FlowDocument, a: FlowPos, b: FlowPos) -> bool {
        if a.para == b.para {
            return a.offset < b.offset;
        }
        let order = order(doc);
        let ia = order.iter().position(|&p| p == a.para);
        let ib = order.iter().position(|&p| p == b.para);
        ia < ib
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.text_selection().is_some_and(|(a, b)| a != b)
    }

    /// Put the caret at `pos` (extending the selection if `extend`).
    pub(crate) fn set_caret(&mut self, pos: FlowPos, extend: bool, cx: &mut Context<Self>) {
        let pos = self.visible_position(pos);
        let Some(state) = self.doc_state_mut() else { return };
        if extend {
            if state.anchor.is_none() {
                state.anchor = state.caret;
            }
        } else {
            state.anchor = None;
            state.extra_selections.clear();
        }
        state.caret = Some(pos);
        state.pending = None;
        if state.anchor == state.caret {
            state.anchor = None;
        }
        cx.notify();
    }

    /// The lines of the laid-out pages.
    pub(crate) fn flow_lines(&self) -> Vec<LineGeom> {
        let Some(doc) = self.flow() else { return Vec::new() };
        let pagination = pages(doc, &self.resources);
        let mut out = Vec::new();
        for (page, pg) in pagination.pages.iter().enumerate() {
            for item in &pg.items {
                if let Item::Line { para, x, y, h, stops, vertical } = item {
                    out.push(LineGeom { page, para: *para, x: *x, y: *y, h: *h, stops: stops.clone(), vertical: *vertical });
                }
            }
        }
        out
    }

    /// The line the caret at `pos` is shown on.
    pub(crate) fn line_of(lines: &[LineGeom], pos: FlowPos) -> Option<&LineGeom> {
        let mine: Vec<&LineGeom> = lines.iter().filter(|l| l.para == pos.para).collect();
        let last = mine.len().saturating_sub(1);
        // Header and footer lines are on every page; the first will do.
        let per_page_last = |i: usize| i == last || mine.get(i + 1).is_some_and(|n| n.stops.first().is_some_and(|s| s.0 == 0));
        mine.iter()
            .enumerate()
            .find(|(i, l)| l.holds(pos.offset, per_page_last(*i)))
            .map(|(_, l)| *l)
            .or_else(|| mine.iter().rev().find(|l| l.stops.first().is_some_and(|s| s.0 <= pos.offset)).copied())
    }

    /// The caret's box: page, x, top, height (points).
    pub(crate) fn caret_box(&self, lines: &[LineGeom], pos: FlowPos) -> Option<(usize, f32, f32, f32)> {
        let line = Self::line_of(lines, pos)?;
        if line.vertical {
            Some((line.page, line.x, line.y_of(pos.offset), line.h))
        } else {
            Some((line.page, line.x_of(pos.offset), line.y, line.h))
        }
    }

    /// The position nearest a point of page `page` (points).
    pub(crate) fn position_at(&self, page: usize, x: f32, y: f32) -> Option<FlowPos> {
        let lines = self.flow_lines();
        let on_page: Vec<&LineGeom> = lines.iter().filter(|l| l.page == page).collect();
        // A line under the point, else the nearest by height/width; among those
        // the one nearest sideways (table cells side by side, columns).
        let distance = |l: &LineGeom| {
            if l.vertical {
                let dx = if x < l.x { l.x - x } else if x > l.x + l.h { x - l.x - l.h } else { 0.0 };
                let bottom = l.y + l.stops.last().map_or(0.0, |s| s.1);
                let dy = if y < l.y { l.y - y } else if y > bottom { y - bottom } else { 0.0 };
                (dx, dy)
            } else {
                let dy = if y < l.y { l.y - y } else if y > l.y + l.h { y - l.y - l.h } else { 0.0 };
                let right = l.x + l.stops.last().map_or(0.0, |s| s.1);
                let dx = if x < l.x { l.x - x } else if x > right { x - right } else { 0.0 };
                (dy, dx)
            }
        };
        let line = on_page.iter().min_by(|a, b| {
            let (prim_a, sec_a) = distance(a);
            let (prim_b, sec_b) = distance(b);
            let da = prim_a * 4.0 + sec_a;
            let db = prim_b * 4.0 + sec_b;
            da.total_cmp(&db).then_with(|| {
                let off_a = if a.vertical {
                    (a.y_of(a.nearest(y)) - y).abs()
                } else {
                    (a.x_of(a.nearest(x)) - x).abs()
                };
                let off_b = if b.vertical {
                    (b.y_of(b.nearest(y)) - y).abs()
                } else {
                    (b.x_of(b.nearest(x)) - x).abs()
                };
                off_a.total_cmp(&off_b)
            })
        })?;
        let offset = if line.vertical { line.nearest(y) } else { line.nearest(x) };
        Some(FlowPos::new(line.para, offset))
    }

    pub(super) fn paragraph_of(&self, id: u32) -> Option<&FlowParagraph> {
        flow_command::paragraph(self.flow()?, id)
    }

    fn visible_position(&self, mut pos: FlowPos) -> FlowPos {
        if let Some(p) = self.paragraph_of(pos.para) {
            while let Some((_, end)) = hidden_range(p, pos.offset) { pos.offset = end; }
        }
        pos
    }

    fn selection_endpoint(&self, mut pos: FlowPos) -> FlowPos {
        if let Some(p) = self.paragraph_of(pos.para) {
            while let Some((_, end)) = hidden_range_strictly_inside(p, pos.offset) { pos.offset = end; }
        }
        pos
    }

    /// View changes and undo can leave either selection end inside deleted text.
    pub(super) fn normalize_visible_selection(&mut self) {
        let Some((anchor, caret)) = self.text_selection() else { return };
        if anchor == caret {
            let caret = self.visible_position(caret);
            if let Some(state) = self.doc_state_mut() {
                state.caret = Some(caret);
                state.anchor = None;
            }
        } else {
            let anchor = self.selection_endpoint(anchor);
            let caret = self.selection_endpoint(caret);
            if let Some(state) = self.doc_state_mut() {
                state.caret = Some(caret);
                state.anchor = (anchor != caret).then_some(anchor);
            }
        }
    }

    // ----- Moving ---------------------------------------------------------

    fn step(&self, pos: FlowPos, forward: bool) -> FlowPos {
        let pos = self.visible_position(pos);
        let Some(doc) = self.flow() else { return pos };
        let len = self.paragraph_of(pos.para).map_or(0, |p| p.len());
        // Marks not drawn (a comment's) are passed over with the next
        // position, as Word does.
        let mark = |o: usize| self.paragraph_of(pos.para).is_some_and(|p| p.is_mark(o));
        if forward && pos.offset < len {
            let mut o = pos.offset;
            while o < len {
                if let Some((_, end)) = self.paragraph_of(pos.para).and_then(|p| hidden_range(p, o)) { o = end; continue; }
                let m = mark(o);
                o += 1;
                if !m {
                    break;
                }
            }
            return FlowPos::new(pos.para, o);
        }
        if !forward && pos.offset > 0 {
            let mut o = pos.offset;
            let mut skipped_deletion = false;
            while o > 0 {
                o -= 1;
                if let Some((start, _)) = self.paragraph_of(pos.para).and_then(|p| hidden_range(p, o)) {
                    o = start;
                    skipped_deletion = true;
                    continue;
                }
                if !mark(o) { return FlowPos::new(pos.para, o); }
            }
            // Leading deleted text is the paragraph's visible start: move to
            // the previous paragraph instead of stopping at the same x again.
            if !skipped_deletion { return FlowPos::new(pos.para, o); }
        }
        let (container, index) = flow_command::locate(doc, pos.para).unwrap_or((Container::Body, 0));
        let list = if matches!(container, Container::Header | Container::Footer | Container::Part { .. }) {
            flow_command::blocks(doc, container).map(|b| b.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).collect()).unwrap_or_default()
        } else {
            order(doc)
        };
        let _ = index;
        let i = list.iter().position(|&p| p == pos.para).unwrap_or(0);
        if forward {
            match list.get(i + 1) {
                Some(&next) => FlowPos::new(next, 0),
                None => pos,
            }
        } else if i > 0 {
            let prev = list[i - 1];
            FlowPos::new(prev, self.paragraph_of(prev).map_or(0, |p| p.len()))
        } else {
            pos
        }
    }

    fn word_step(&self, pos: FlowPos, forward: bool) -> FlowPos {
        let Some(p) = self.paragraph_of(pos.para) else { return pos };
        let chars = visible_chars(p);
        let mut o = chars.partition_point(|(offset, _)| *offset < pos.offset);
        if forward {
            if o >= chars.len() { return self.step(pos, true); }
            let k = class(chars[o].1);
            while o < chars.len() && class(chars[o].1) == k { o += 1; }
            while o < chars.len() && class(chars[o].1) == 0 { o += 1; }
        } else {
            if o == 0 { return self.step(pos, false); }
            while o > 0 && class(chars[o - 1].1) == 0 { o -= 1; }
            if o > 0 {
                let k = class(chars[o - 1].1);
                while o > 0 && class(chars[o - 1].1) == k { o -= 1; }
            }
        }
        FlowPos::new(pos.para, chars.get(o).map_or(p.len(), |c| c.0))
    }

    /// The visible word around `pos` (a double click).
    pub(crate) fn word_at(&self, pos: FlowPos) -> (FlowPos, FlowPos) {
        let Some(p) = self.paragraph_of(pos.para) else { return (pos, pos) };
        let chars = visible_chars(p);
        if chars.is_empty() { let pos = self.visible_position(pos); return (pos, pos); }
        let at = chars.partition_point(|(offset, _)| *offset < pos.offset).min(chars.len() - 1);
        let k = class(chars[at].1);
        let (mut a, mut b) = (at, at);
        while a > 0 && class(chars[a - 1].1) == k { a -= 1; }
        while b < chars.len() && class(chars[b].1) == k { b += 1; }
        (FlowPos::new(pos.para, chars[a].0), FlowPos::new(pos.para, chars.get(b).map_or(p.len(), |c| c.0)))
    }

    fn vertical(&self, pos: FlowPos, down: bool) -> (FlowPos, f32) {
        let lines = self.flow_lines();
        let Some(current) = Self::line_of(&lines, pos) else { return (pos, 0.0) };
        let goal = self.doc_state().and_then(|s| s.goal_x).unwrap_or_else(|| {
            if current.vertical {
                current.y_of(pos.offset)
            } else {
                current.x_of(pos.offset)
            }
        });
        let index = lines.iter().position(|l| std::ptr::eq(l, current)).unwrap_or(0);
        // The next line of the body in reading order: the layout lists a
        // page's lines in order, headers and footers after the body.
        let body = order(self.flow().map(|d| d.as_ref()).unwrap_or(&FlowDocument::default()));
        let in_body = |l: &LineGeom| body.contains(&l.para);
        let same_kind = in_body(current);
        let candidates: Box<dyn Iterator<Item = &LineGeom>> = if down { Box::new(lines[index + 1..].iter()) } else { Box::new(lines[..index].iter().rev()) };
        for l in candidates {
            let diff = if current.vertical { (l.x - current.x).abs() } else { (l.y - current.y).abs() };
            if in_body(l) == same_kind && (l.page != current.page || diff > 0.5) {
                return (FlowPos::new(l.para, l.nearest(goal)), goal);
            }
        }
        (pos, goal)
    }

    fn line_edge(&self, pos: FlowPos, end: bool) -> FlowPos {
        let lines = self.flow_lines();
        let Some(line) = Self::line_of(&lines, pos) else { return pos };
        let stops = &line.stops;
        let len = self.paragraph_of(pos.para).map_or(0, |p| p.len());
        let offset = match (end, stops.last()) {
            (false, _) => stops.first().map_or(pos.offset, |s| s.0),
            // A wrapped line's end is before its last character (after it
            // is the next line's start).
            (true, Some(last)) if last.0 < len && stops.len() >= 2 => stops[stops.len() - 2].0,
            (true, Some(last)) => last.0,
            (true, None) => pos.offset,
        };
        FlowPos::new(pos.para, offset)
    }

    fn document_edge(&self, end: bool) -> Option<FlowPos> {
        let doc = self.flow()?;
        let order = order(doc);
        if end {
            let id = *order.last()?;
            Some(FlowPos::new(id, self.paragraph_of(id)?.len()))
        } else {
            Some(FlowPos::new(*order.first()?, 0))
        }
    }

    /// Move the caret (and the selection's end when `extend`).
    pub(crate) fn move_caret(&mut self, how: Move, extend: bool, cx: &mut Context<Self>) {
        let Some((anchor, caret)) = self.text_selection() else {
            if let Some(start) = self.document_edge(false) {
                self.set_caret(start, false, cx);
            }
            return;
        };
        let lines = self.flow_lines();
        let is_vert = Self::line_of(&lines, caret).is_some_and(|l| l.vertical);
        // Without Shift, a selection collapses to its side.
        if !extend && anchor != caret && matches!(how, Move::Left | Move::Right | Move::Up | Move::Down) {
            let (a, b) = self.ordered_selection().unwrap_or((caret, caret));
            let to = match (how, is_vert) {
                (Move::Left, false) | (Move::Up, true) => Some(a),
                (Move::Right, false) | (Move::Down, true) => Some(b),
                _ => None,
            };
            if let Some(to) = to {
                self.set_caret(to, false, cx);
                return;
            }
        }
        let mut goal = None;
        let to = match (how, is_vert) {
            (Move::Left, true) => {
                let (to, g) = self.vertical(caret, true);
                goal = Some(g);
                to
            }
            (Move::Right, true) => {
                let (to, g) = self.vertical(caret, false);
                goal = Some(g);
                to
            }
            (Move::Up, true) => self.step(caret, false),
            (Move::Down, true) => self.step(caret, true),
            (Move::Left, false) => self.step(caret, false),
            (Move::Right, false) => self.step(caret, true),
            (Move::WordLeft, _) => self.word_step(caret, false),
            (Move::WordRight, _) => self.word_step(caret, true),
            (Move::Up, false) | (Move::Down, false) => {
                let (to, g) = self.vertical(caret, how == Move::Down);
                goal = Some(g);
                to
            }
            (Move::LineStart, _) => self.line_edge(caret, false),
            (Move::LineEnd, _) => self.line_edge(caret, true),
            (Move::DocumentStart, _) => self.document_edge(false).unwrap_or(caret),
            (Move::DocumentEnd, _) => self.document_edge(true).unwrap_or(caret),
        };
        self.set_caret(to, extend, cx);
        if let Some(state) = self.doc_state_mut() {
            state.goal_x = goal;
        }
        self.scroll_to_caret(cx);
    }

    pub(crate) fn select_all(&mut self, cx: &mut Context<Self>) {
        let (Some(a), Some(b)) = (self.document_edge(false), self.document_edge(true)) else { return };
        self.set_caret(a, false, cx);
        self.set_caret(b, true, cx);
    }

    /// Scroll so the caret is in view.
    pub(crate) fn scroll_to_caret(&mut self, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let lines = self.flow_lines();
        let Some((page, _, y, h)) = self.caret_box(&lines, caret) else { return };
        self.scroll_into_view(page, y, h, cx);
    }

    // ----- Editing ----------------------------------------------------------

    pub(crate) fn run_flow(&mut self, commands: Vec<FlowCommand>, cx: &mut Context<Self>) -> bool {
        // Edits move the offsets the other ranges of a selection hold.
        if let Some(state) = self.doc_state_mut() {
            state.extra_selections.clear();
        }
        fn is_comment_command(cmd: &FlowCommand) -> bool {
            match cmd {
                FlowCommand::SetComments(_) => true,
                FlowCommand::InsertField { kind, .. } => {
                    matches!(kind, FlowRunKind::CommentStart(_) | FlowRunKind::CommentEnd(_) | FlowRunKind::CommentRef(_))
                }
                FlowCommand::ReplaceBlocks { remove: 1, blocks, .. } => {
                    blocks.iter().all(|b| matches!(b, Block::Paragraph(_)))
                }
                FlowCommand::Many(subcmds) => subcmds.iter().all(is_comment_command),
                _ => false,
            }
        }

        fn is_formatting_command(cmd: &FlowCommand) -> bool {
            match cmd {
                FlowCommand::SetRunFormat { .. }
                | FlowCommand::SetCharStyle { .. }
                | FlowCommand::SetStyle { .. }
                | FlowCommand::SetNoteStyle { .. }
                | FlowCommand::TrackFormat { .. } => true,
                FlowCommand::SetParagraphFormat { change, .. } => {
                    !matches!(change, ParaChange::AddBookmark(_) | ParaChange::RemoveBookmark(_))
                }
                FlowCommand::Many(subcmds) => subcmds.iter().any(is_formatting_command),
                _ => false,
            }
        }

        let is_admin = commands.iter().all(|c| {
            matches!(
                c,
                FlowCommand::SetMarkAsFinal(_)
                    | FlowCommand::SetReadOnlyRecommended(_)
                    | FlowCommand::SetLockTracking { .. }
                    | FlowCommand::SetViewMode(_)
                    | FlowCommand::SetInteractionMode(_)
                    | FlowCommand::SetDocumentProtection(_)
                    | FlowCommand::SetFormattingProtection(_)
            )
        });
        if !is_admin {
            if self.is_mark_as_final() {
                log::warn!("edit rejected: document is marked as final");
                return false;
            }
            if self.is_read_only_recommended() && !self.is_read_only_dismissed() {
                log::warn!("edit rejected: document is read-only recommended");
                return false;
            }
            if self.is_read_mode() || self.is_viewing_mode() {
                log::warn!("edit rejected: document is in viewing / read mode");
                return false;
            }
            if self.is_protection_read_only() {
                log::warn!("edit rejected: document is protected as read-only");
                return false;
            }
            if self.is_protection_comments() {
                let is_comment_cmd = commands.iter().all(is_comment_command);
                if !is_comment_cmd {
                    log::warn!("edit rejected: document is protected for comments only");
                    return false;
                }
            }
            if self.is_protection_forms() {
                let is_form_cmd = commands.iter().all(|c| {
                    matches!(
                        c,
                        FlowCommand::ToggleCheckBox { .. }
                            | FlowCommand::SelectDropDownItem { .. }
                            | FlowCommand::SetDatePickerDate { .. }
                            | FlowCommand::SetContentControlText { .. }
                    )
                });
                if !is_form_cmd {
                    log::warn!("edit rejected: document is protected for forms");
                    return false;
                }
            }
            if self.is_formatting_protected() && commands.iter().any(is_formatting_command) {
                log::warn!("edit rejected: document formatting is protected");
                return false;
            }
        }
        let started = crate::debug_timing::start();
        let before = self.flow().map(|d| marked(d));
        let redefine = self.auto_redefine_targets(&commands);
        let result = self.document.execute(commands.into_iter().map(Command::Flow).collect());
        crate::debug_timing::report("edit", started);
        if let Err(err) = &result {
            log::warn!("edit rejected: {err}");
        }
        if result.is_ok()
            && let Some(before) = before
        {
            self.remove_orphans(&before);
        }
        if result.is_ok() && !redefine.is_empty() {
            self.auto_redefine_styles(&redefine);
        }
        if result.is_ok() {
            self.sync_document_peers(cx);
        }
        cx.notify();
        result.is_ok()
    }

    /// スタイルの自動更新 (`w:autoRedefine`): the paragraphs whose formatting `commands` change by
    /// hand (a paragraph's formatting, or a character formatting over the paragraph's whole text).
    fn auto_redefine_targets(&self, commands: &[FlowCommand]) -> Vec<u32> {
        let Some(doc) = self.flow() else { return Vec::new() };
        if !doc.styles.styles.values().any(|s| s.auto_update) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for command in commands {
            match command {
                FlowCommand::SetRunFormat { from, to, .. } if from.para == to.para && from.offset == 0 && flow_command::paragraph(doc, from.para).is_some_and(|p| p.len() == to.offset && to.offset > 0) => out.push(from.para),
                FlowCommand::SetParagraphFormat { first, last, change } if !matches!(change, ParaChange::Style(_) | ParaChange::List(_) | ParaChange::Level(_) | ParaChange::AddBookmark(_) | ParaChange::RemoveBookmark(_) | ParaChange::SectionBreak(_)) => {
                    let order = order(doc);
                    if let (Some(a), Some(b)) = (order.iter().position(|p| p == first), order.iter().position(|p| p == last)) {
                        out.extend(order[a.min(b)..=a.max(b)].iter().copied());
                    }
                }
                _ => {}
            }
        }
        out.sort_unstable();
        out.dedup();
        out.retain(|&id| flow_command::paragraph(doc, id).and_then(|p| p.style.clone().or_else(|| doc.styles.default_paragraph.clone())).and_then(|s| doc.styles.styles.get(&s)).is_some_and(|s| s.auto_update));
        out
    }

    /// The styles of `paragraphs` take what was changed by hand (in the step of the edit).
    fn auto_redefine_styles(&mut self, paragraphs: &[u32]) {
        for &id in paragraphs {
            let Some(doc) = self.flow() else { return };
            let Some((style_id, style, paragraph)) = flow_command::style_matching(doc, id, 0) else { continue };
            let Some((container, index)) = flow_command::locate(doc, id) else { continue };
            let commands = vec![Command::Flow(FlowCommand::SetStyle { id: style_id, style: Some(style) }), Command::Flow(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(paragraph)] })];
            if self.document.amend_last(commands).is_err() {
                return;
            }
        }
    }

    /// Notes and comments whose marks the edit removed go with them (as
    /// Word does), in the same step of undo. Only those marked before the
    /// edit: what a file has unmarked stays.
    fn remove_orphans(&mut self, before: &Marked) {
        for _ in 0..8 {
            let Some(doc) = self.flow() else { return };
            let now = marked(doc);
            let lost: Vec<u32> = before.comments.difference(&now.comments).copied().collect();
            let Some(&id) = lost.iter().find(|i| doc.comments.iter().any(|c| c.id == **i)) else { break };
            let Some(command) = flow_command::delete_comment(doc, id) else { break };
            if self.document.amend_last(vec![Command::Flow(command)]).is_err() {
                break;
            }
        }
        let Some(doc) = self.flow() else { return };
        let now = marked(doc);
        let lost: std::collections::HashSet<(bool, u32)> = before.notes.difference(&now.notes).copied().collect();
        if lost.is_empty() {
            return;
        }
        let footnotes: Vec<_> = doc.footnotes.iter().filter(|n| !lost.contains(&(false, n.id))).cloned().collect();
        let endnotes: Vec<_> = doc.endnotes.iter().filter(|n| !lost.contains(&(true, n.id))).cloned().collect();
        let _ = self.document.amend_last(vec![Command::Flow(FlowCommand::SetNotes { footnotes, endnotes })]);
    }

    /// 挿入 > 脚注 (文末脚注): a note at the caret, the caret into its text.
    pub(crate) fn insert_note(&mut self, endnote: bool, cx: &mut Context<Self>) {
        let Some((_, end)) = self.ordered_selection() else { return };
        let Some((commands, para)) = self.flow().and_then(|d| flow_command::insert_note(d, end, endnote)) else { return };
        if self.run_flow(commands, cx)
            && let Some(len) = self.flow().and_then(|d| flow_command::paragraph(d, para)).map(|p| p.len())
        {
            self.set_caret(FlowPos::new(para, len), false, cx);
            self.scroll_to_caret(cx);
        }
    }

    /// 脚注を文末脚注に変換 (or back).
    pub(crate) fn convert_notes(&mut self, to_endnotes: bool, cx: &mut Context<Self>) {
        let Some(command) = self.flow().and_then(|d| flow_command::convert_notes(d, to_endnotes)) else { return };
        // The caret leaves a note that moves.
        let in_note = self.text_selection().and_then(|(_, c)| self.flow().and_then(|d| flow_command::locate(d, c.para))).is_some_and(|l| matches!(l.0, Container::Note { .. }));
        if self.run_flow(vec![command], cx)
            && in_note
            && let Some(start) = self.document_edge(false)
        {
            self.set_caret(start, false, cx);
        }
    }

    /// A broad paragraph deletion would also remove invisible source scopes,
    /// or move them into a different paragraph without their source XML.
    /// Keep the document and redo history intact until that writeback exists.
    fn reject_hidden_history_range(&mut self, start: FlowPos, end: FlowPos, cx: &mut Context<Self>) -> bool {
        if start.para == end.para || super::flow_layout::showing_markup() { return false; }
        let Some(doc) = self.flow() else { return false; };
        if doc.track_changes { return false; }
        let (Some((container, first)), Some((other, last))) = (flow_command::locate(doc, start.para), flow_command::locate(doc, end.para)) else { return false; };
        if container != other || first > last { return false; }
        fn deleted(block: &Block) -> bool {
            match block {
                Block::Paragraph(p) => p.runs.iter().any(|r| matches!(r.revision.as_ref().map(|v| &v.kind), Some(ibis_doc_model::RevisionKind::Delete))),
                Block::Table(t) => t.rows.iter().any(|row| row.cells.iter().any(|cell| cell.blocks.iter().any(deleted))),
            }
        }
        if !flow_command::blocks(doc,container).is_some_and(|blocks| blocks[first..=last].iter().any(deleted)) { return false; }
        self.status = Some(crate::shell::i18n::text("docx.editing.replacing-deleting-across-multiple").into());
        cx.notify();
        true
    }

    /// Replace the selection (or insert at the caret) with `text`: `\n`
    /// ends a paragraph, `\t` is a tab, `\u{b}` a line break.
    pub(crate) fn type_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if text == " " && self.toggle_checkbox_around_caret(cx) {
            return;
        }
        let Some((start, mut end)) = self.ordered_selection() else { return };
        if self.reject_hidden_history_range(start, end, cx) { return; }
        let Some(doc) = self.flow() else { return };
        if start == end && self.is_overtype() && !text.contains('\n') && !text.contains('\r') {
            if let Some(p) = flow_command::paragraph(doc, start.para) {
                let remaining = p.len().saturating_sub(start.offset);
                let overwrite_count = text.chars().count().min(remaining);
                if overwrite_count > 0 {
                    end = FlowPos::new(start.para, start.offset + overwrite_count);
                }
            }
        }
        let mut commands = Vec::new();
        let track = doc.track_changes;
        let who = crate::docx::comments::comment_author();
        // With changes tracked, the selection is marked deleted and the text
        // goes after it.
        let at = if track && start != end { after_tracked_delete(doc, start, end) } else { start };
        if start != end {
            if track {
                commands.push(FlowCommand::TrackDelete { from: start, to: end, author: who.author.clone(), date: who.date.clone() });
            } else if start.para == end.para && !super::flow_layout::showing_markup() {
                if let Some(p) = flow_command::paragraph(doc, start.para) { commands.extend(visible_delete_commands(p, start.offset, end.offset)); }
            } else {
                commands.push(FlowCommand::Delete { from: start, to: end });
            }
        }
        let original_start = start;
        let replacing = start != end;
        let start = at;
        // The formatting typed: a button pressed with no selection, else the
        // text before the caret (what the selection started with).
        let format = self.doc_state().and_then(|s| s.pending.clone()).or_else(|| {
            if !replacing && super::flow_layout::showing_markup() { return None; }
            let p = flow_command::paragraph(doc, original_start.para)?;
            let offset = if replacing { original_start.offset + 1 } else { original_start.offset };
            let (format, char_style, _) = visible_format_at(p, offset);
            Some(InsertFormat { format, char_style })
        });
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let breaks = text.matches('\n').count();
        let first_new = doc.new_id();
        let last_segment = text.rsplit('\n').next().unwrap_or("");
        let typed = last_segment.chars().count();
        commands.push(FlowCommand::InsertText { at: start, text: text.clone(), format });
        if track && breaks == 0 && typed > 0 {
            let revision = ibis_doc_model::Revision { kind: ibis_doc_model::RevisionKind::Insert, author: who.author.clone(), date: who.date.clone() };
            commands.push(FlowCommand::MarkRevision { from: start, to: FlowPos::new(start.para, start.offset + typed), revision: Some(revision) });
        }
        if self.run_flow(commands, cx) {
            let caret = if breaks == 0 { FlowPos::new(start.para, start.offset + typed) } else { FlowPos::new(first_new + breaks as u32 - 1, typed) };
            self.set_caret(caret, false, cx);
            self.scroll_to_caret(cx);
        }
    }

    /// The caret's paragraph when it is a list item and the caret is at its
    /// start with nothing selected (where Tab, Enter and Backspace act on
    /// the list, as in Word).
    fn list_item_at_start(&self) -> Option<&FlowParagraph> {
        let (start, end) = self.ordered_selection()?;
        let p = self.paragraph_of(start.para)?;
        (start == end && start.offset == 0 && p.list.is_some()).then_some(p)
    }

    /// Tab: a list item's level down at its start, else a tab.
    pub(crate) fn tab_key(&mut self, cx: &mut Context<Self>) {
        // In a table: to the next cell (a new row after the last), as Word.
        if let Some((table, row, cell)) = self.table_at_caret() {
            let next = |e: &Self| -> Option<Option<u32>> {
                let doc = e.flow()?;
                let t = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table))?;
                let (r, c) = if cell + 1 < t.rows[row].cells.len() { (row, cell + 1) } else { (row + 1, 0) };
                Some(t.rows.get(r).and_then(|x| x.cells.get(c)).and_then(|c| c.blocks.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None })))
            };
            match next(self) {
                Some(Some(id)) => self.set_caret(FlowPos::new(id, 0), false, cx),
                Some(None) => {
                    self.edit_table(|r, _, _| ibis_doc_model::TableEdit::InsertRow { row: r, below: true }, cx);
                    if let Some(Some(id)) = next(self) {
                        self.set_caret(FlowPos::new(id, 0), false, cx);
                    }
                }
                None => {}
            }
            return;
        }
        if let Some(level) = self.list_item_at_start().and_then(|p| p.list).map(|l| l.level) {
            self.apply_para_change(ParaChange::Level((level + 1).min(8)), cx);
        } else {
            self.type_text("\t", cx);
        }
    }

    pub(crate) fn outdent_list(&mut self, cx: &mut Context<Self>) {
        // In a table: back to the previous cell (the last of the row above from the first), as Word.
        if let Some((table, row, cell)) = self.table_at_caret() {
            let previous = self.flow().and_then(|doc| {
                let t = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table))?;
                let (r, c) = if cell > 0 { (row, cell - 1) } else if row > 0 { (row - 1, t.rows[row - 1].cells.len().checked_sub(1)?) } else { return None };
                t.rows.get(r)?.cells.get(c)?.blocks.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None })
            });
            if let Some(id) = previous {
                self.set_caret(FlowPos::new(id, 0), false, cx);
            }
            return;
        }
        let Some((start, _)) = self.ordered_selection() else { return };
        match self.paragraph_of(start.para).and_then(|p| p.list) {
            Some(l) if l.level > 0 => self.apply_para_change(ParaChange::Level(l.level - 1), cx),
            Some(_) => self.apply_para_change(ParaChange::List(None), cx),
            None => {}
        }
    }

    /// Enter: an empty list item stops being one; else a new paragraph.
    pub(crate) fn enter_key(&mut self, cx: &mut Context<Self>) {
        if self.list_item_at_start().is_some_and(|p| p.is_empty()) {
            self.apply_para_change(ParaChange::List(None), cx);
        } else {
            self.type_text("\n", cx);
        }
    }

    /// Toggle checkbox at position `pos`:
    /// Returns true if a checkbox was found at `pos` and toggled.
    pub(crate) fn toggle_checkbox_at(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target_offset = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if matches!(run.kind, FlowRunKind::CheckBox { .. }) {
                    target_offset = Some(offset);
                    break;
                }
            }
            offset += len;
        }
        if let Some(target) = target_offset {
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::ToggleCheckBox { at }], cx);
            true
        } else {
            false
        }
    }

    /// Toggle checkbox around caret (e.g. on Space key or command):
    /// If caret or selection is at a checkbox, toggle it.
    pub(crate) fn toggle_checkbox_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection() else { return false };
        // If selection is 1 char and it's a checkbox:
        if start.para == end.para && end.offset == start.offset + 1 {
            if self.toggle_checkbox_at(start, cx) {
                return true;
            }
        }
        // If caret is collapsed: check at start.offset
        if start == end {
            if self.toggle_checkbox_at(start, cx) {
                return true;
            }
        }
        false
    }

    /// Insert a new checkbox control at the current caret position.
    pub(crate) fn insert_checkbox(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let kind = FlowRunKind::CheckBox {
            checked: false,
            checked_char: '\u{2611}',
            unchecked_char: '\u{2610}',
        };
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + 1), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + 2), false, cx);
    }

    /// Toggle checklist / checkbox on current paragraph or insert one:
    pub(crate) fn toggle_checklist(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(p) = flow_command::paragraph(doc, start.para) else { return };
        if let Some(first_run) = p.runs.first() && matches!(first_run.kind, FlowRunKind::CheckBox { .. }) {
            self.toggle_checkbox_at(FlowPos::new(start.para, 0), cx);
        } else {
            self.insert_checkbox(cx);
        }
    }

    /// ドロップダウン リストまたはコンボボックス項目の選択 (DOCX-FORM-006):
    #[allow(dead_code)]
    pub(crate) fn select_dropdown_item_at(&mut self, pos: FlowPos, index: usize, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target_offset = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if matches!(run.kind, FlowRunKind::DropDownList { .. }) {
                    target_offset = Some(offset);
                    break;
                }
            }
            offset += len;
        }
        if let Some(target) = target_offset {
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::SelectDropDownItem { at, index }], cx);
            true
        } else {
            false
        }
    }

    /// ドロップダウン リスト項目の順送り切り替え (DOCX-FORM-006):
    pub(crate) fn cycle_dropdown_item_at(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target_info = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if let FlowRunKind::DropDownList { items, selected, .. } = &run.kind {
                    if !items.is_empty() {
                        let next = (*selected + 1) % items.len();
                        target_info = Some((offset, next));
                        break;
                    }
                }
            }
            offset += len;
        }
        if let Some((target, next_idx)) = target_info {
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::SelectDropDownItem { at, index: next_idx }], cx);
            true
        } else {
            false
        }
    }

    /// カーソル位置のドロップダウン リスト項目の順送り切り替え:
    pub(crate) fn cycle_dropdown_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        self.cycle_dropdown_item_at(start, cx)
    }

    /// ドロップダウン リスト コントロールの新規挿入 (DOCX-FORM-006):
    pub(crate) fn insert_dropdown_list(&mut self, items: Vec<(String, String)>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let display_len = items.first().map_or(0, |(d, _)| d.chars().count());
        let kind = FlowRunKind::DropDownList {
            items,
            selected: 0,
            combo: false,
        };
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + display_len), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len + 1), false, cx);
    }

    /// コンボ ボックス コントロールの新規挿入 (DOCX-FORM-005):
    pub(crate) fn insert_combo_box(&mut self, items: Vec<(String, String)>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let display_len = items.first().map_or(0, |(d, _)| d.chars().count());
        let kind = FlowRunKind::DropDownList {
            items,
            selected: 0,
            combo: true,
        };
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + display_len), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len + 1), false, cx);
    }

    /// 日付選択コントロールの日付送り (+days) (DOCX-FORM-007):
    pub(crate) fn step_date_picker_at(&mut self, pos: FlowPos, days: i64, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target_info = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if let FlowRunKind::DatePicker { full_date, date_format, .. } = &run.kind {
                    target_info = Some((offset, full_date.clone(), date_format.clone()));
                    break;
                }
            }
            offset += len;
        }
        if let Some((target, full_date, date_format)) = target_info {
            let current_time = ibis_doc_model::date_field::parse_iso8601(&full_date).unwrap_or_else(crate::clock::local_now);
            let next_time = current_time.add_days(days);
            let next_iso = next_time.to_iso_date();
            let next_text = ibis_doc_model::date_field::format_picture(&next_time, &date_format);
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::SetDatePickerDate { at, full_date: next_iso, text: next_text }], cx);
            true
        } else {
            false
        }
    }

    /// カーソル位置の日付選択コントロールの日付送り:
    pub(crate) fn step_date_picker_around_caret(&mut self, days: i64, cx: &mut Context<Self>) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        self.step_date_picker_at(start, days, cx)
    }

    /// 日付選択コントロールの新規挿入 (DOCX-FORM-007):
    pub(crate) fn insert_date_picker(&mut self, date_format: Option<String>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let fmt = date_format.unwrap_or_else(|| "yyyy/M/d".to_string());
        let now = crate::clock::local_now();
        let full_date = now.to_iso_date();
        let text = ibis_doc_model::date_field::format_picture(&now, &fmt);
        let display_len = text.chars().count();
        let kind = FlowRunKind::DatePicker {
            full_date,
            date_format: fmt,
            text,
        };
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + display_len), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len + 1), false, cx);
    }

    /// リッチテキスト コンテンツ コントロールの新規挿入 (DOCX-FORM-001):
    pub(crate) fn insert_rich_text_control(&mut self, text: Option<String>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let text = text.unwrap_or_else(|| crate::shell::i18n::text("_docx.content-control-placeholder").to_string());
        let display_len = text.chars().count();
        let kind = FlowRunKind::RichText { text };
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + display_len), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len + 1), false, cx);
    }

    /// プレーンテキスト コンテンツ コントロールの新規挿入 (DOCX-FORM-002):
    pub(crate) fn insert_plain_text_control(&mut self, text: Option<String>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let text = text.unwrap_or_else(|| crate::shell::i18n::text("_docx.content-control-placeholder").to_string());
        let display_len = text.chars().count();
        let kind = FlowRunKind::PlainText { text };
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + display_len), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len + 1), false, cx);
    }

    /// コンテンツ コントロールのテキスト更新 (DOCX-FORM-001, DOCX-FORM-002):
    pub(crate) fn set_content_control_text_at(&mut self, pos: FlowPos, text: String, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut found_target = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if matches!(run.kind, FlowRunKind::RichText { .. } | FlowRunKind::PlainText { .. }) {
                    found_target = Some(offset);
                    break;
                }
            }
            offset += len;
        }
        if let Some(target) = found_target {
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::SetContentControlText { at, text }], cx);
            true
        } else {
            false
        }
    }

    /// 箇条書き・番号付きリスト: the paragraphs become items of a list of
    /// bullets (or numbers), or stop being list items if they are already.
    pub(crate) fn toggle_list(&mut self, bullet: bool, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow().cloned() else { return };
        let numbering = &doc.styles.numbering;
        let current = self.paragraph_of(start.para).and_then(|p| p.list).and_then(|l| numbering.level(l.num_id, 0)).map(|l| l.format == "bullet");
        let keep = self.text_selection();
        let mut commands = Vec::new();
        let change = if current == Some(bullet) {
            ParaChange::List(None)
        } else {
            let id = match numbering.find(bullet) {
                Some(id) => id,
                None => {
                    let (with, id) = numbering.with_new_list(bullet);
                    commands.push(FlowCommand::SetNumbering { numbering: with });
                    id
                }
            };
            ParaChange::List(Some(id))
        };
        commands.extend(self.selected_paragraph_format_commands(change));
        self.run_flow(commands, cx);
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = (a != c).then_some(a);
            state.caret = Some(c);
        }
    }

    /// Backspace (`forward` false) or Delete.
    pub(crate) fn delete_char(&mut self, forward: bool, cx: &mut Context<Self>) {
        // Backspace at a list item's start takes its number away first.
        if !forward && self.list_item_at_start().is_some() {
            self.apply_para_change(ParaChange::List(None), cx);
            return;
        }
        if self.has_extra_selections() {
            let ranges = self.selection_ranges();
            if ranges.iter().any(|&(a, b)| self.reject_hidden_history_range(a, b, cx)) { return; }
            let track = self.flow().is_some_and(|d| d.track_changes);
            let who = crate::docx::comments::comment_author();
            // From the last range up, so that each leaves the offsets of those before it.
            let commands: Vec<FlowCommand> = ranges.iter().rev().map(|&(from, to)| if track {
                FlowCommand::TrackDelete { from, to, author: who.author.clone(), date: who.date.clone() }
            } else {
                FlowCommand::Delete { from, to }
            }).collect();
            let first = ranges[0].0;
            if self.run_flow(commands, cx) {
                self.set_caret(first, false, cx);
            }
            return;
        }
        let Some((start, end)) = self.ordered_selection() else { return };
        if self.reject_hidden_history_range(start, end, cx) { return; }
        let track = self.flow().is_some_and(|d| d.track_changes);
        let who = crate::docx::comments::comment_author();
        if start != end {
            let commands = if track {
                vec![FlowCommand::TrackDelete { from: start, to: end, author: who.author, date: who.date }]
            } else if start.para == end.para && !crate::docx::flow_layout::showing_markup() {
                visible_delete_commands(self.paragraph_of(start.para).unwrap(), start.offset, end.offset)
            } else { vec![FlowCommand::Delete { from: start, to: end }] };
            if self.run_flow(commands, cx) {
                self.set_caret(start, false, cx);
            }
            return;
        }
        let other = self.step(start, forward);
        let (mut a, mut b) = if forward { (start, other) } else { (other, start) };
        // Delete the visible character, keeping the invisible marks/history
        // that movement passed over. Apply this to tracked deletions as well.
        if a.para == b.para && b.offset > a.offset + 1 {
            if forward { a.offset = b.offset - 1; } else { b.offset = a.offset + 1; }
        }
        if track {
            // A tracked deletion: the character is marked (in one paragraph;
            // across paragraphs the caret just moves).
            if a.para == b.para && a != b {
                let old_len = self.paragraph_of(a.para).map_or(0, |p| p.len());
                if self.run_flow(vec![FlowCommand::TrackDelete { from: a, to: b, author: who.author, date: who.date }], cx) {
                    // A tracked insertion is removed; ordinary text is marked
                    // deleted. In markup view forward Delete stays after it.
                    let len = self.paragraph_of(a.para).map_or(0, |p| p.len());
                    let caret = if forward && len == old_len { b } else { a };
                    self.set_caret(FlowPos::new(caret.para, caret.offset.min(len)), false, cx);
                }
            } else {
                self.set_caret(other, false, cx);
            }
            return;
        }
        if other == start {
            return;
        }
        let caret_after = if forward { start } else { a };
        // Joining paragraphs of different lists (a cell and the body) is
        // not done; the caret just moves.
        let doc = self.flow().cloned();
        let same_list = doc.as_ref().is_some_and(|d| flow_command::locate(d, a.para).map(|l| l.0) == flow_command::locate(d, b.para).map(|l| l.0));
        if !same_list {
            self.set_caret(other, false, cx);
            return;
        }
        if self.run_flow(vec![FlowCommand::Delete { from: a, to: b }], cx) {
            self.set_caret(caret_after, false, cx);
            self.scroll_to_caret(cx);
        }
    }

    /// The direct formatting at the selection's start (or what typing would take).
    pub(crate) fn current_format(&self) -> Option<(RunFormat, ibis_doc_model::FlowRunStyle)> {
        let (start, end) = self.ordered_selection()?;
        let p = self.paragraph_of(start.para)?;
        if let Some(pending) = self.doc_state().and_then(|s| s.pending.clone()) {
            let doc = self.flow()?;
            let (_, base) = doc.styles.paragraph_base(p.style.as_deref(), p.table_style.as_deref());
            let style = doc.styles.run_format(&base, pending.char_style.as_deref(), &pending.format).style();
            return Some((pending.format, style));
        }
        let (format, _, style) = if start != end {
            p.format_at(start.offset + 1)
        } else {
            visible_format_at(p, start.offset)
        };
        Some((format, style))
    }

    /// Apply a character format to the selection, or to what is typed next.
    pub(crate) fn apply_run_change(&mut self, change: RunChange, cx: &mut Context<Self>) {
        // A selection of several ranges: all of them, as one step.
        if self.has_extra_selections() && self.flow().is_some_and(|d| !d.track_changes && (super::flow_layout::showing_markup() || !order(d).iter().filter_map(|&id| flow_command::paragraph(d, id)).any(|p| p.runs.iter().any(|r| r.revision.is_some())))) {
            let ranges = self.selection_ranges();
            let (extras, anchor, caret) = self.doc_state().map(|s| (s.extra_selections.clone(), s.anchor, s.caret)).unwrap_or_default();
            let commands = ranges.into_iter().map(|(from, to)| FlowCommand::SetRunFormat { from, to, change: change.clone() }).collect();
            if self.run_flow(commands, cx) {
                self.restore_extra_selections(extras, anchor, caret);
            }
            return;
        }
        let Some((start, end)) = self.ordered_selection() else { return };
        if start == end {
            let Some(p) = self.paragraph_of(start.para) else { return };
            let (mut format, char_style, _) = match self.doc_state().and_then(|s| s.pending.clone()) {
                Some(f) => (f.format, f.char_style, p.mark.clone()),
                None => visible_format_at(p, start.offset),
            };
            change.apply(&mut format);
            if let Some(state) = self.doc_state_mut() {
                state.pending = Some(InsertFormat { format, char_style });
            }
            cx.notify();
            return;
        }
        let anchor = self.text_selection();
        // The shared command skips whole table blocks between the endpoints.
        // Use the DOCX replacement path for tables, tracked formatting and
        // final-text view; hidden deleted runs must retain their formatting.
        let whole_table = self.flow().is_some_and(|doc| {
            let (Some((container, first)), Some((other, last))) = (flow_command::locate(doc, start.para), flow_command::locate(doc, end.para)) else { return false };
            container == other && first <= last && flow_command::blocks(doc, container).is_some_and(|blocks| blocks[first..=last].iter().any(|b| matches!(b, Block::Table(_))))
        });
        if whole_table || self.flow().is_some_and(|doc| doc.track_changes) || !super::flow_layout::showing_markup() {
            self.apply_selected_text_format(|format| change.apply(format), cx);
            if let (Some((a, c)), Some(state)) = (anchor, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
            }
            return;
        }
        self.run_flow(vec![FlowCommand::SetRunFormat { from: start, to: end, change }], cx);
        if let (Some((a, c)), Some(state)) = (anchor, self.doc_state_mut()) {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
    }

    pub(crate) fn toggle_bold(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.bold);
        self.apply_run_change(RunChange::Bold(!on), cx);
    }

    pub(crate) fn toggle_italic(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.italic);
        self.apply_run_change(RunChange::Italic(!on), cx);
    }

    pub(crate) fn toggle_underline(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.underline);
        self.apply_run_change(RunChange::Underline(!on), cx);
    }

    pub(crate) fn toggle_strike(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.strike);
        self.apply_run_change(RunChange::Strike(!on), cx);
    }

    pub(crate) fn toggle_double_strike(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.double_strike);
        self.apply_run_change(RunChange::DoubleStrike(!on), cx);
    }

    pub(crate) fn toggle_caps(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.caps);
        self.apply_run_change(RunChange::Caps(!on), cx);
    }

    pub(crate) fn toggle_small_caps(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.small_caps);
        self.apply_run_change(RunChange::SmallCaps(!on), cx);
    }

    /// 現在選択のフォーマットが隠し文字（w:vanish）かどうか。
    pub(crate) fn is_hidden(&self) -> bool {
        self.current_format().is_some_and(|(_, s)| s.hidden)
    }

    /// 隠し文字 (w:vanish, DOCX-OPTIONS-008) をトグル.
    pub(crate) fn toggle_hidden(&mut self, cx: &mut Context<Self>) {
        let on = self.is_hidden();
        self.apply_run_change(RunChange::Hidden(!on), cx);
    }

    /// 中抜き・輪郭 (w:outline, DOCX-FONT-017) をトグル.
    pub(crate) fn toggle_outline(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.outline);
        self.apply_run_change(RunChange::Outline(!on), cx);
    }

    /// 影付き (w:shadow, DOCX-FONT-017) をトグル.
    pub(crate) fn toggle_shadow(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.shadow);
        self.apply_run_change(RunChange::Shadow(!on), cx);
    }

    /// 浮き出し (w:emboss, DOCX-FONT-017) をトグル.
    pub(crate) fn toggle_emboss(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.emboss);
        self.apply_run_change(RunChange::Emboss(!on), cx);
    }

    /// 浮き彫り (w:imprint, DOCX-FONT-017) をトグル.
    pub(crate) fn toggle_imprint(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.imprint);
        self.apply_run_change(RunChange::Imprint(!on), cx);
    }

    /// 文字の囲み線 (w:bdr) をトグル.
    pub(crate) fn toggle_character_border(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.border.is_some());
        let border = if on {
            None
        } else {
            Some(ibis_doc_model::FlowBorder {
                style: ibis_doc_model::BorderStyle::Single,
                width: 4 * 12_700 / 8, // 0.5 pt
                color: ibis_doc_model::Rgb(0x000000),
                space: 0,
                art: None,
            })
        };
        self.apply_run_change(RunChange::Border(border), cx);
    }

    /// 文字の網かけ (w:shd, Word 標準 15% グレー 0xD9D9D9) をトグル.
    pub(crate) fn toggle_character_shading(&mut self, cx: &mut Context<Self>) {
        let shading_color = ibis_doc_model::Rgb(0xd9d9d9);
        let on = self.current_format().is_some_and(|(_, s)| s.highlight == Some(shading_color));
        let highlight = if on { None } else { Some(shading_color) };
        self.apply_run_change(RunChange::Highlight(highlight), cx);
    }

    /// 文字種の変換 (DOCX-JA-020).
    pub(crate) fn convert_case(&mut self, conversion: CaseConversion, cx: &mut Context<Self>) {
        let (from, to) = if let Some((s, e)) = self.ordered_selection() && s != e {
            (s, e)
        } else if let Some((_, caret)) = self.text_selection() {
            let (a, b) = self.word_at(caret);
            if a != b {
                (a, b)
            } else {
                return;
            }
        } else {
            return;
        };

        let new_to = if from.para == to.para {
            let selected = self.paragraph_of(from.para).map(|p| selected_paragraph_text(p, from.offset, to.offset)).unwrap_or_default();
            let new_len = conversion.apply(&selected).chars().count();
            FlowPos::new(from.para, from.offset + new_len)
        } else {
            to
        };

        let cmd = FlowCommand::ConvertCase { from, to, conversion };
        if self.run_flow(vec![cmd], cx) {
            self.set_caret(from, false, cx);
            self.set_caret(new_to, true, cx);
            self.scroll_to_caret(cx);
        }
    }

    pub(crate) fn toggle_text_position(&mut self, position: ibis_doc_model::VerticalPosition, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.position == position);
        self.apply_run_change(RunChange::Position(if on { ibis_doc_model::VerticalPosition::Baseline } else { position }), cx);
    }

    pub(crate) fn step_text_size(&mut self, grow: bool, cx: &mut Context<Self>) {
        let size = self.current_format().map_or(1050, |(_, s)| s.size);
        self.apply_run_change(RunChange::Size(ibis_doc_model::step_font_size(size, grow)), cx);
    }

    pub(crate) fn set_text_size(&mut self, size: u32, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::Size(size), cx);
    }

    pub(crate) fn set_complex_script_size(&mut self, size: u32, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::ComplexScriptSize(size), cx);
    }

    pub(crate) fn set_text_color(&mut self, color: Rgb, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::Color(color), cx);
    }

    pub(crate) fn set_theme_color(&mut self, color: Rgb, theme_color: ibis_doc_model::FlowThemeColor, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::ThemeColor { color, theme_color }, cx);
    }

    pub(crate) fn set_highlight(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::Highlight(color), cx);
    }

    pub(crate) fn set_character_spacing(&mut self, spacing: Option<ibis_doc_model::Emu>, cx: &mut Context<Self>) {
        if self.current_format().is_some_and(|(_, s)| s.spacing == spacing.unwrap_or(0)) {
            return;
        }
        self.apply_run_change(RunChange::Spacing(spacing), cx);
    }

    pub(crate) fn set_character_scale(&mut self, scale: Option<u32>, cx: &mut Context<Self>) {
        if self.current_format().is_some_and(|(_, s)| s.scale == scale.unwrap_or(100)) {
            return;
        }
        self.apply_run_change(RunChange::Scale(scale), cx);
    }

    pub(crate) fn set_character_kerning(&mut self, kern: Option<u32>, cx: &mut Context<Self>) {
        if self.current_format().is_some_and(|(f, _)| f.kern == kern) {
            return;
        }
        self.apply_run_change(RunChange::Kern(kern), cx);
    }

    /// The paragraphs the selection touches (their first and last, in one list).
    pub(super) fn selected_paragraphs(&self) -> Option<(u32, u32)> {
        let (start, end) = self.ordered_selection()?;
        let doc = self.flow()?;
        let same = flow_command::locate(doc, start.para).map(|l| l.0) == flow_command::locate(doc, end.para).map(|l| l.0);
        Some(if same { (start.para, end.para) } else { (end.para, end.para) })
    }

    pub(crate) fn paragraph_format_commands(&self, first: u32, last: u32, change: ParaChange) -> Vec<FlowCommand> {
        let mut commands = vec![FlowCommand::SetParagraphFormat { first, last, change: change.clone() }];
        // The shared range command only visits direct paragraphs. Whole selected
        // tables need their cell paragraphs included in the same undo operation.
        if !matches!(change, ParaChange::SectionBreak(_)) && let Some(doc) = self.flow() {
            if let (Some((container, a)), Some((other, b))) = (flow_command::locate(doc, first), flow_command::locate(doc, last)) {
                if container == other && a <= b && let Some(blocks) = flow_command::blocks(doc, container) {
                    for block in &blocks[a..=b] {
                        if matches!(block, Block::Table(_)) {
                            for paragraph in super::block_tree::paragraphs(std::slice::from_ref(block)) {
                                commands.push(FlowCommand::SetParagraphFormat { first: paragraph.id, last: paragraph.id, change: change.clone() });
                            }
                        }
                    }
                }
            }
        }
        commands
    }

    /// Format commands for all touched paragraphs across the selection,
    /// including selections that span multiple table cells or the whole table.
    pub(crate) fn selected_paragraph_format_commands(&self, change: ParaChange) -> Vec<FlowCommand> {
        let Some((start, end)) = self.ordered_selection() else { return Vec::new() };
        let Some(doc) = self.flow() else { return Vec::new() };
        let same = flow_command::locate(doc, start.para).map(|l| l.0) == flow_command::locate(doc, end.para).map(|l| l.0);
        if same {
            return self.paragraph_format_commands(start.para, end.para, change);
        }
        let order = order(doc);
        let (Some(i), Some(j)) = (order.iter().position(|&p| p == start.para), order.iter().position(|&p| p == end.para)) else {
            return self.paragraph_format_commands(end.para, end.para, change);
        };
        let (min, max) = if i <= j { (i, j) } else { (j, i) };
        let mut commands = Vec::new();
        for &para_id in &order[min..=max] {
            commands.push(FlowCommand::SetParagraphFormat { first: para_id, last: para_id, change: change.clone() });
        }
        commands
    }

    pub(crate) fn apply_para_change(&mut self, change: ParaChange, cx: &mut Context<Self>) {
        let keep = self.text_selection();
        let commands = self.selected_paragraph_format_commands(change);
        if !commands.is_empty() {
            self.run_flow(commands, cx);
        }
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = (a != c).then_some(a);
            state.caret = Some(c);
        }
    }

    pub(crate) fn set_alignment(&mut self, align: FlowAlign, cx: &mut Context<Self>) {
        self.apply_para_change(ParaChange::Align(align), cx);
    }

    /// Line spacing as a multiple of single (1.0, 1.15, 1.5, 2.0).
    pub(crate) fn set_line_spacing(&mut self, multiple: f32, cx: &mut Context<Self>) {
        self.apply_para_change(ParaChange::Line(LineHeight::Auto((multiple * 240.0).round() as u32)), cx);
    }

    /// インデント増・減: the left indent by a step of 10.5 pt (Word's with
    /// the default font), not below 0. If current paragraph is a list item,
    /// adjust the list level (0..=8) instead, as Word does.
    pub(crate) fn step_indent(&mut self, increase: bool, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if let Some(list) = self.paragraph_of(start.para).and_then(|p| p.list) {
            let new_level = if increase {
                (list.level + 1).min(8)
            } else {
                list.level.saturating_sub(1)
            };
            if new_level != list.level {
                self.apply_para_change(ParaChange::Level(new_level), cx);
                return;
            } else if !increase && list.level == 0 {
                self.apply_para_change(ParaChange::List(None), cx);
                return;
            }
        }
        let left = self.paragraph_of(start.para).map_or(0, |p| p.left);
        let step = 420 * 635;
        let to = if increase { (left / step + 1) * step } else { ((left - 1).max(0) / step) * step };
        self.apply_para_change(ParaChange::Left(to.max(0)), cx);
    }

    pub(crate) fn apply_style(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.apply_para_change(ParaChange::Style(id), cx);
    }

    pub(crate) fn clear_formatting(&mut self, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::Clear, cx);
    }

    /// 句読点のぶら下げを行う（w:overflowPunct）の切り替え。
    pub(crate) fn toggle_overflow_punct(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_overflow_punct();
        self.apply_para_change(ibis_doc_model::ParaChange::OverflowPunct(!current), cx);
    }

    /// キャレット位置の段落の句読点のぶら下げ設定（既定値 true）。
    pub(crate) fn caret_overflow_punct(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.overflow_punct)
    }

    /// 日本語と英字の間隔を自動調整する（w:autoSpaceDE）の切り替え。
    pub(crate) fn toggle_auto_space_de(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_auto_space_de();
        self.apply_para_change(ibis_doc_model::ParaChange::AutoSpaceDE(!current), cx);
    }

    /// キャレット位置の段落の日本語と英字の間隔自動調整設定（既定値 true）。
    pub(crate) fn caret_auto_space_de(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.auto_space_letters)
    }

    /// 日本語と数字の間隔を自動調整する（w:autoSpaceDN）の切り替え。
    pub(crate) fn toggle_auto_space_dn(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_auto_space_dn();
        self.apply_para_change(ibis_doc_model::ParaChange::AutoSpaceDN(!current), cx);
    }

    /// キャレット位置の段落の日本語と数字の間隔自動調整設定（既定値 true）。
    pub(crate) fn caret_auto_space_dn(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.auto_space_digits)
    }

    /// 禁則処理を行う（w:kinsoku）の切り替え。
    pub(crate) fn toggle_kinsoku(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_kinsoku();
        self.apply_para_change(ibis_doc_model::ParaChange::Kinsoku(!current), cx);
    }

    /// キャレット位置の段落の禁則処理設定（既定値 true）。
    pub(crate) fn caret_kinsoku(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.kinsoku)
    }

    /// 単語の途中で改行しない（w:wordWrap）の切り替え。
    pub(crate) fn toggle_word_wrap(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_word_wrap();
        self.apply_para_change(ibis_doc_model::ParaChange::WordWrap(!current), cx);
    }

    /// キャレット位置の段落の単語途中改行禁止設定（既定値 true）。
    pub(crate) fn caret_word_wrap(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.word_wrap)
    }

    /// 右から左へ記述する（w:bidi）の切り替え。
    pub(crate) fn toggle_bidi(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_bidi();
        self.apply_para_change(ibis_doc_model::ParaChange::Bidi(!current), cx);
    }

    /// キャレット位置の段落の右から左へ記述設定（既定値 false）。
    pub(crate) fn caret_bidi(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        self.paragraph_of(start.para).map_or(false, |p| p.bidi)
    }

    /// 行グリッド線に合わせる（w:snapToGrid）の切り替え。
    pub(crate) fn toggle_snap_to_grid(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_snap_to_grid();
        self.apply_para_change(ibis_doc_model::ParaChange::SnapToGrid(!current), cx);
    }

    /// キャレット位置の段落の行グリッド吸着設定（既定値 true）。
    pub(crate) fn caret_snap_to_grid(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.snap_to_grid)
    }

    /// 単語の途中で改行しない（ハイフネーションを行わない、w:suppressAutoHyphens）の切り替え。
    pub(crate) fn toggle_suppress_auto_hyphens(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_suppress_auto_hyphens();
        self.apply_para_change(ibis_doc_model::ParaChange::SuppressAutoHyphens(!current), cx);
    }

    /// キャレット位置の段落のハイフネーション除外設定（既定値 false）。
    pub(crate) fn caret_suppress_auto_hyphens(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        self.paragraph_of(start.para).map_or(false, |p| p.suppress_auto_hyphens)
    }

    /// Every range of the selection, in document order: the ranges added
    /// with ⌘ and the caret's, when it is not empty.
    pub(crate) fn selection_ranges(&self) -> Vec<(FlowPos, FlowPos)> {
        let mut ranges = self.doc_state().map(|s| s.extra_selections.clone()).unwrap_or_default();
        if let Some((a, b)) = self.ordered_selection().filter(|(a, b)| a != b) {
            ranges.push((a, b));
        }
        if let Some(doc) = self.flow() {
            let order = order(doc);
            let key = |p: FlowPos| (order.iter().position(|&id| id == p.para).unwrap_or(usize::MAX), p.offset);
            ranges.sort_by_key(|&(a, _)| key(a));
        }
        ranges
    }

    /// Whether the selection is made of several ranges.
    pub(crate) fn has_extra_selections(&self) -> bool {
        self.doc_state().is_some_and(|s| !s.extra_selections.is_empty())
    }

    /// Put the ranges `extras` back as the selection's other ranges (after a command
    /// that run_flow cleared them, one that moved no offsets).
    pub(crate) fn restore_extra_selections(&mut self, extras: Vec<(FlowPos, FlowPos)>, anchor: Option<FlowPos>, caret: Option<FlowPos>) {
        if let Some(state) = self.doc_state_mut() {
            state.extra_selections = extras;
            state.anchor = anchor;
            state.caret = caret;
        }
    }

    /// The selected text (paragraphs joined with line feeds).
    pub(crate) fn selected_text(&self) -> String {
        if self.has_extra_selections() {
            return self.selection_ranges().into_iter().map(|(a, b)| self.range_text(a, b)).collect::<Vec<_>>().join("\n");
        }
        let Some((start, end)) = self.ordered_selection() else { return String::new() };
        self.range_text(start, end)
    }

    /// The text from `start` to `end` (paragraphs joined with line feeds).
    fn range_text(&self, start: FlowPos, end: FlowPos) -> String {
        let Some(doc) = self.flow() else { return String::new() };
        let order = order(doc);
        let (Some(i), Some(j)) = (order.iter().position(|&p| p == start.para), order.iter().position(|&p| p == end.para)) else {
            // Header or footer: one paragraph.
            return self.paragraph_of(start.para).map(|p| selected_paragraph_text(p, start.offset, end.offset)).unwrap_or_default();
        };
        let mut out = Vec::new();
        for (k, id) in order[i..=j].iter().enumerate() {
            let Some(p) = self.paragraph_of(*id) else { continue };
            let a = if k == 0 { start.offset } else { 0 };
            let b = if i + k == j { end.offset } else { p.len() };
            out.push(selected_paragraph_text(p, a, b));
        }
        out.join("\n")
    }

    pub(crate) fn copy_text(&mut self, cx: &mut Context<Self>) {
        let text = self.selected_text();
        // Several ranges: their text, one range to a line (no formatting).
        if self.has_extra_selections() {
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            return;
        }
        if self.copy_paragraphs(&text, cx) { return; }
        if !text.is_empty() {
            if self.copy_formatted_text(&text, cx) { return; }
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    pub(crate) fn cut_text(&mut self, cx: &mut Context<Self>) {
        if let Some((start,end)) = self.ordered_selection() && self.reject_hidden_history_range(start,end,cx) { return; }
        if !self.has_selection() {
            return;
        }
        self.copy_text(cx);
        self.delete_char(false, cx);
    }

    pub(crate) fn default_paste_mode(&self) -> crate::shell::editor::DefaultPasteMode {
        self.default_paste_mode
    }

    pub(crate) fn set_default_paste_mode(&mut self, mode: crate::shell::editor::DefaultPasteMode, cx: &mut Context<Self>) {
        self.default_paste_mode = mode;
        cx.notify();
    }

    /// ユーザー名 (Word Options > 全般: ユーザー設定, DOCX-OPTIONS-003).
    pub(crate) fn user_name(&self) -> String {
        if let Some(ref name) = self.custom_user_name {
            if !name.trim().is_empty() {
                return name.clone();
            }
        }
        crate::shell::user_name::user_name()
    }

    /// ユーザー頭文字 (Word Options > 全般: ユーザー設定, DOCX-OPTIONS-003).
    pub(crate) fn user_initials(&self) -> String {
        if let Some(ref inits) = self.custom_user_initials {
            if !inits.trim().is_empty() {
                return inits.clone();
            }
        }
        crate::shell::user_name::user_initials()
    }

    /// ユーザー名と頭文字を設定 (DOCX-OPTIONS-003).
    pub(crate) fn set_user_info(&mut self, name: Option<String>, initials: Option<String>, cx: &mut Context<Self>) {
        self.custom_user_name = name.clone();
        self.custom_user_initials = initials.clone();
        crate::shell::user_name::set_custom_user_info(name, initials);
        cx.notify();
    }

    /// 元の書式を保持して貼り付け (Keep Source Formatting).
    pub(crate) fn paste_keep_source(&mut self, cx: &mut Context<Self>) {
        if let Some((start, end)) = self.ordered_selection() && self.reject_hidden_history_range(start, end, cx) { return; }
        let Some(item) = cx.read_from_clipboard() else { return };
        let Some(text) = item.text() else { return };
        if self.paste_paragraphs(&item, cx) { return; }
        if self.paste_formatted_text(&item, cx) { return; }
        self.type_text(&text, cx);
    }

    /// 校閲 > 比較: the file picked is the original; this document is the revised one, and the changes between them are
    /// marked as tracked changes in it (one step of undo).
    pub(crate) fn pick_compare_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(crate::shell::i18n::text("_shell.open")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| this.compare_with_file(&path, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// The changes from the document in the file at `path` to this one, as tracked changes.
    pub(crate) fn compare_with_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let original = std::fs::read(path).ok().and_then(|data| ibis_pptx::read(&data).ok()).and_then(|p| p.flow);
        let Some(original) = original else {
            self.status = Some(crate::shell::i18n::text("docx.merge-panel.cannot-read-file").into());
            return cx.notify();
        };
        let Some(doc) = self.flow() else { return };
        let who = crate::docx::comments::comment_author();
        let blocks = ibis_doc_model::compare::compare_blocks(&original.body, &doc.body, &who.author, &who.date);
        let remove = doc.body.len();
        if self.run_flow(vec![FlowCommand::ReplaceBlocks { container: Container::Body, index: 0, remove, blocks }], cx) {
            self.status = Some(crate::shell::i18n::text("docx.editing.changes-were-marked-tracked").into());
            cx.notify();
        }
    }

    /// A picture another app copied (the clipboard holds an image and no text) pasted at the caret.
    pub(crate) fn paste_clipboard_image(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(item) = cx.read_from_clipboard() else { return false };
        if item.text().is_some_and(|t| !t.is_empty()) {
            return false;
        }
        let Some(ClipboardEntry::Image(image)) = item.entries().iter().find(|e| matches!(e, ClipboardEntry::Image(_))) else { return false };
        let format = match image.format {
            ImageFormat::Png => ibis_doc_model::MediaFormat::Png,
            ImageFormat::Jpeg => ibis_doc_model::MediaFormat::Jpeg,
            ImageFormat::Gif => ibis_doc_model::MediaFormat::Gif,
            ImageFormat::Bmp => ibis_doc_model::MediaFormat::Bmp,
            ImageFormat::Tiff => ibis_doc_model::MediaFormat::Tiff,
            ImageFormat::Svg => ibis_doc_model::MediaFormat::Svg,
            _ => return false,
        };
        self.doc_insert_picture(image.bytes.clone(), format, cx);
        true
    }

    /// 通常の貼り付け (Word Options の貼付の既定動作に従う, DOCX-OPTIONS-006).
    pub(crate) fn paste_text(&mut self, cx: &mut Context<Self>) {
        // An object copied in its application (OLE) stays one.
        #[cfg(not(target_family = "wasm"))]
        if self.default_paste_mode != crate::shell::editor::DefaultPasteMode::KeepTextOnly && self.paste_embedded(cx) {
            return;
        }
        if self.paste_clipboard_image(cx) {
            return;
        }
        match self.default_paste_mode {
            crate::shell::editor::DefaultPasteMode::KeepSourceFormatting => {
                self.paste_keep_source(cx);
            }
            crate::shell::editor::DefaultPasteMode::MergeFormatting => {
                self.paste_match_format(cx);
            }
            crate::shell::editor::DefaultPasteMode::KeepTextOnly => {
                self.paste_plain_text(cx);
            }
        }
    }

    pub(crate) fn paste_plain_text(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else { return };
        self.type_text(&text, cx);
    }

    pub(crate) fn paste_match_format(&mut self, cx: &mut Context<Self>) {
        if let Some((start, end)) = self.ordered_selection() && self.reject_hidden_history_range(start, end, cx) { return; }
        let Some(item) = cx.read_from_clipboard() else { return };
        if self.paste_matching_format(&item, cx) { return; }
        if let Some(text) = item.text() {
            self.type_text(&text, cx);
        }
    }
}

/// How the caret moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Move {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

/// Tables: what the caret is in, and the table commands of the menus and
/// the toolbar.
impl SlideEditor {
    /// The table, row and cell the caret is in (the innermost table).
    pub(crate) fn table_at_caret(&self) -> Option<(u32, usize, usize)> {
        let (_, caret) = self.text_selection()?;
        match flow_command::locate(self.flow()?, caret.para)? {
            (Container::Cell { table, row, cell }, _) => Some((table, row, cell)),
            _ => None,
        }
    }

    /// Insert a table of `rows` × `columns` at the caret, as wide as the
    /// text area (the caret's cell, in a table).
    pub(crate) fn insert_table(&mut self, rows: usize, columns: usize, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(doc) = self.flow() else { return };
        let width = if let Some((table, row, cell)) = self.table_at_caret() {
            self.flow()
                .and_then(|d| d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table)))
                .and_then(|t| {
                    let col_idx: usize = t.rows.get(row)?.cells[..cell].iter().map(|c| c.span.max(1) as usize).sum();
                    let span = t.rows.get(row)?.cells.get(cell).map_or(1, |c| c.span.max(1) as usize);
                    let col_w: ibis_doc_model::Emu = t.columns.iter().skip(col_idx).take(span).sum();
                    let margins = t.rows.get(row)?.cells.get(cell).map_or(0, |c| c.margins[1] + c.margins[3]);
                    Some((col_w - margins).max(360_000))
                })
                .unwrap_or_else(|| doc.page.size.width - doc.page.left - doc.page.right)
        } else {
            doc.page.size.width - doc.page.left - doc.page.right
        };
        let first = doc.new_id();
        if self.run_flow(vec![FlowCommand::InsertTable { at: caret, rows, columns, width }], cx) {
            // The caret into the first cell (its paragraph is the table's
            // first id after the table's, row's and cell's).
            let para = first + 2;
            if self.flow().is_some_and(|d| flow_command::paragraph(d, para).is_some()) {
                self.set_caret(FlowPos::new(para, 0), false, cx);
            }
        }
    }

    /// A row, column or cell edit of the caret's table.
    /// The caret's row: whether it repeats as a header and whether it is
    /// kept on one page.
    pub(crate) fn caret_row_flags(&self) -> Option<(bool, bool)> {
        let (table, row, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        t.rows.get(row).map(|r| (r.header, r.cant_split))
    }

    /// The caret's cell vertical alignment (anchor).
    pub(crate) fn caret_cell_anchor(&self) -> Option<ibis_doc_model::CellAnchor> {
        let (table, row, cell) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        t.rows.get(row).and_then(|r| r.cells.get(cell)).map(|c| c.anchor)
    }

    /// The caret's cell text direction.
    pub(crate) fn caret_cell_direction(&self) -> Option<ibis_doc_model::CellDirection> {
        let (table, row, cell) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        t.rows.get(row).and_then(|r| r.cells.get(cell)).map(|c| c.direction)
    }

    /// The caret's cell margins (top, right, bottom, left) in EMU.
    pub(crate) fn caret_cell_margins(&self) -> Option<[ibis_doc_model::Emu; 4]> {
        let (table, row, cell) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        t.rows.get(row).and_then(|r| r.cells.get(cell)).map(|c| c.margins)
    }

    /// The caret's cell no-wrap setting (`w:noWrap`).
    pub(crate) fn caret_cell_no_wrap(&self) -> Option<bool> {
        let (table, row, cell) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        t.rows.get(row).and_then(|r| r.cells.get(cell)).map(|c| c.no_wrap)
    }

    /// Toggle cell text wrapping (`w:noWrap`).
    pub(crate) fn toggle_cell_wrap_text(&mut self, cx: &mut gpui::Context<Self>) {
        let current = self.caret_cell_no_wrap().unwrap_or(false);
        self.edit_table(|r, c, _| ibis_doc_model::TableEdit::NoWrap { cells: vec![(r, c)], no_wrap: !current }, cx);
    }

    /// The caret's table alignment.
    pub(crate) fn caret_table_align(&self) -> Option<ibis_doc_model::FlowAlign> {
        let (table, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        Some(t.align)
    }

    /// The caret's table indent in EMU.
    pub(crate) fn caret_table_indent(&self) -> Option<ibis_doc_model::Emu> {
        let (table, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        Some(t.indent)
    }

    /// The caret's table cell spacing in EMU (None if not in a table; Some(None) if 0/none).
    pub(crate) fn caret_table_cell_spacing(&self) -> Option<Option<ibis_doc_model::Emu>> {
        let (table, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        Some(t.cell_spacing)
    }

    /// The caret's table floating placement (`w:tblpPr`).
    pub(crate) fn caret_table_floating(&self) -> Option<Option<ibis_doc_model::TableFloat>> {
        let (table, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        Some(t.floating)
    }

    /// Set or clear table floating (`w:tblpPr`).
    pub(crate) fn set_table_floating(&mut self, floating: Option<ibis_doc_model::TableFloat>, cx: &mut Context<Self>) {
        self.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Floating(floating), cx);
    }

    /// Set table floating distance from surrounding text (`w:tblpPr` left/right/top/bottomFromText).
    pub(crate) fn set_table_float_distance(&mut self, distance: [ibis_doc_model::Emu; 4], cx: &mut Context<Self>) {
        self.edit_table(move |_, _, _| ibis_doc_model::TableEdit::FloatDistance(distance), cx);
    }

    /// Set table floating horizontal alignment (`w:tblpPr tblpXSpec`).
    pub(crate) fn set_table_float_horz_align(&mut self, align: Option<ibis_doc_model::TableAlign>, cx: &mut Context<Self>) {
        let current = self.caret_table_floating().flatten().unwrap_or_else(ibis_doc_model::TableFloat::standard);
        let mut updated = current;
        updated.x_align = align;
        self.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Floating(Some(updated)), cx);
    }

    /// The caret's table style ID (`w:tblStyle`). None if not in a table; Some(None) if no style.
    pub(crate) fn caret_docx_table_style(&self) -> Option<Option<String>> {
        let (table, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        Some(t.style.as_ref().map(|s| s.id.clone()))
    }

    /// Set or clear table style (`w:tblStyle`).
    pub(crate) fn set_docx_table_style(&mut self, style_id: Option<String>, cx: &mut Context<Self>) {
        self.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Style(style_id), cx);
    }

    /// Document's table styles list (ID, display name).
    pub(crate) fn document_table_styles(&self) -> Vec<(String, String)> {
        let mut list = Vec::new();
        let mut seen = std::collections::HashSet::new();
        list.push(("TableGrid".to_string(), "docx.editing.table-grid".to_string()));
        seen.insert("TableGrid".to_string());
        if let Some(d) = self.flow() {
            for (id, style) in &d.styles.styles {
                if style.kind == ibis_doc_model::StyleKind::Table && seen.insert(id.clone()) {
                    let name = if style.name.is_empty() { id.clone() } else { style.name.clone() };
                    list.push((id.clone(), name));
                }
            }
        }
        list
    }
    pub(crate) fn edit_table(&mut self, edit: impl FnOnce(usize, usize, usize) -> ibis_doc_model::TableEdit, cx: &mut Context<Self>) {
        let Some((table, row, cell)) = self.table_at_caret() else { return };
        // The cell's grid column.
        let column = self
            .flow()
            .and_then(|d| {
                d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table)).map(|t| t.rows[row].cells[..cell].iter().map(|c| c.span.max(1) as usize).sum())
            })
            .unwrap_or(cell);
        let caret = self.text_selection().map(|(_, c)| c);
        self.run_flow(vec![FlowCommand::EditTable { table, edit: edit(row, cell, column) }], cx);
        // The caret stays if its paragraph does; else it goes to the start.
        if let Some(c) = caret
            && self.flow().is_some_and(|d| flow_command::paragraph(d, c.para).is_none())
            && let Some(start) = self.document_edge(false)
        {
            self.set_caret(start, false, cx);
        }
    }

    pub(crate) fn delete_table(&mut self, cx: &mut Context<Self>) {
        let Some((table, _, _)) = self.table_at_caret() else { return };
        self.run_flow(vec![FlowCommand::DeleteTable { table }], cx);
        if let Some(start) = self.document_edge(false) {
            self.set_caret(start, false, cx);
        }
    }

    /// 表内の行を並べ替える (Sort rows by current column).
    pub(crate) fn sort_table(&mut self, ascending: bool, has_header: bool, cx: &mut Context<Self>) {
        self.edit_table(|_row, _cell, column| ibis_doc_model::TableEdit::SortRows { column, ascending, has_header }, cx);
    }

    pub(crate) fn can_select_table(&self) -> bool {
        self.table_at_caret().is_some()
    }

    pub(crate) fn select_table_cell(&mut self, cx: &mut Context<Self>) {
        let Some((table_id, r, c)) = self.table_at_caret() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(table) = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table_id)) else { return };
        let Some(cell) = table.rows.get(r).and_then(|row| row.cells.get(c)) else { return };
        let paras = collect_block_paragraphs(&cell.blocks);
        if let Some((start, end)) = self.paragraph_range(&paras) {
            self.set_caret(start, false, cx);
            self.set_caret(end, true, cx);
        }
    }

    pub(crate) fn select_table_row(&mut self, cx: &mut Context<Self>) {
        let Some((table_id, r, _)) = self.table_at_caret() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(table) = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table_id)) else { return };
        let Some(row) = table.rows.get(r) else { return };
        let paras: Vec<u32> = row.cells.iter().flat_map(|cell| collect_block_paragraphs(&cell.blocks)).collect();
        if let Some((start, end)) = self.paragraph_range(&paras) {
            self.set_caret(start, false, cx);
            self.set_caret(end, true, cx);
        }
    }

    pub(crate) fn select_table_column(&mut self, cx: &mut Context<Self>) {
        let Some((table_id, _, c)) = self.table_at_caret() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(table) = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table_id)) else { return };
        let mut paras = Vec::new();
        for row in &table.rows {
            if let Some(cell) = row.cells.get(c) {
                paras.extend(collect_block_paragraphs(&cell.blocks));
            }
        }
        if let Some((start, end)) = self.paragraph_range(&paras) {
            self.set_caret(start, false, cx);
            self.set_caret(end, true, cx);
        }
    }

    pub(crate) fn select_table(&mut self, cx: &mut Context<Self>) {
        let Some((table_id, _, _)) = self.table_at_caret() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(table) = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table_id)) else { return };
        let paras: Vec<u32> = table.rows.iter().flat_map(|r| r.cells.iter().flat_map(|c| collect_block_paragraphs(&c.blocks))).collect();
        if let Some((start, end)) = self.paragraph_range(&paras) {
            self.set_caret(start, false, cx);
            self.set_caret(end, true, cx);
        }
    }

    fn paragraph_range(&self, paras: &[u32]) -> Option<(FlowPos, FlowPos)> {
        let first = *paras.first()?;
        let last = *paras.last()?;
        let last_len = self.paragraph_of(last)?.len();
        Some((FlowPos::new(first, 0), FlowPos::new(last, last_len)))
    }
}

fn collect_block_paragraphs(blocks: &[Block]) -> Vec<u32> {
    fn walk(blocks: &[Block], out: &mut Vec<u32>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.push(p.id),
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
    walk(blocks, &mut out);
    out
}

/// Table `id` in or under `block`.
fn find_table(block: &Block, id: u32) -> Option<&ibis_doc_model::FlowTable> {
    let Block::Table(t) = block else { return None };
    if t.id == id {
        return Some(t);
    }
    t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).find_map(|b| find_table(b, id))
}

/// Parse width and height from SVG data (attributes `width`, `height`, or `viewBox`).
pub(crate) fn svg_pixels(data: &[u8]) -> Option<(u32, u32)> {
    let text = std::str::from_utf8(data).ok()?;
    let svg_start = text.find("<svg")?;
    let svg_tag = &text[svg_start..];
    let tag_end = svg_tag.find('>')?;
    let header = &svg_tag[..tag_end];

    let parse_dim = |attr_name: &str| -> Option<f64> {
        let pattern_dq = format!("{attr_name}=\"");
        let pattern_sq = format!("{attr_name}='");
        let (val_str, quote) = if let Some(pos) = header.find(&pattern_dq) {
            (&header[pos + pattern_dq.len()..], '"')
        } else if let Some(pos) = header.find(&pattern_sq) {
            (&header[pos + pattern_sq.len()..], '\'')
        } else {
            return None;
        };
        let end = val_str.find(quote)?;
        let raw = val_str[..end].trim();
        if raw.ends_with('%') {
            return None;
        }
        let num_str = raw.trim_end_matches(|c: char| c.is_alphabetic());
        let val = num_str.parse::<f64>().ok()?;
        if raw.ends_with("pt") {
            Some(val * 96.0 / 72.0)
        } else if raw.ends_with("in") {
            Some(val * 96.0)
        } else if raw.ends_with("mm") {
            Some(val * 96.0 / 25.4)
        } else if raw.ends_with("cm") {
            Some(val * 96.0 / 2.54)
        } else {
            Some(val)
        }
    };

    let w = parse_dim("width");
    let h = parse_dim("height");
    if let (Some(w), Some(h)) = (w, h) {
        if w > 0.0 && h > 0.0 {
            return Some((w.round() as u32, h.round() as u32));
        }
    }

    // Fallback to viewBox: "min-x min-y width height"
    let pattern_dq = "viewBox=\"";
    let pattern_sq = "viewBox='";
    let (val_str, quote) = if let Some(pos) = header.find(pattern_dq) {
        (&header[pos + pattern_dq.len()..], '"')
    } else if let Some(pos) = header.find(pattern_sq) {
        (&header[pos + pattern_sq.len()..], '\'')
    } else {
        return None;
    };
    let end = val_str.find(quote)?;
    let raw = val_str[..end].trim();
    let parts: Vec<f64> = raw
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    if parts.len() == 4 && parts[2] > 0.0 && parts[3] > 0.0 {
        Some((parts[2].round() as u32, parts[3].round() as u32))
    } else {
        None
    }
}

/// The pixel size of a PNG, JPEG, GIF, BMP or SVG picture, from its header.
pub(crate) fn picture_pixels(data: &[u8]) -> Option<(u32, u32)> {
    if data.starts_with(b"<svg") || data.starts_with(b"<?xml") || (data.len() > 4 && data[..1024.min(data.len())].windows(4).any(|w| w == b"<svg")) {
        if let Some((w, h)) = svg_pixels(data) {
            return Some((w, h));
        }
    }
    let be32 = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    if data.starts_with(b"\x89PNG") && data.len() >= 24 {
        return Some((be32(&data[16..20]), be32(&data[20..24])));
    }
    if data.starts_with(b"GIF8") && data.len() >= 10 {
        return Some((u16::from_le_bytes([data[6], data[7]]) as u32, u16::from_le_bytes([data[8], data[9]]) as u32));
    }
    if data.starts_with(b"BM") && data.len() >= 26 {
        let w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
        let h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
        return Some((w.unsigned_abs(), h.unsigned_abs()));
    }
    if data.starts_with(&[0xff, 0xd8]) {
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xff {
                return None;
            }
            let marker = data[i + 1];
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                return Some((u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32, u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32));
            }
            i += 2 + len;
        }
    }
    None
}

/// Pictures.
impl SlideEditor {
    /// Insert the picture in `data` at the caret: its size at 96 pixels an
    /// inch, made smaller to fit the text's width (as Word does).
    pub(crate) fn doc_insert_picture(&mut self, data: Vec<u8>, format: ibis_doc_model::MediaFormat, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(doc) = self.flow() else { return };
        let (w, h) = picture_pixels(&data).unwrap_or((400, 300));
        let (mut cx_emu, mut cy_emu) = (w as i64 * 9525, h as i64 * 9525);
        let room = doc.page.size.width - doc.page.left - doc.page.right;
        if cx_emu > room && cx_emu > 0 {
            cy_emu = cy_emu * room / cx_emu;
            cx_emu = room;
        }
        // Drawn on screen as well as saved.
        let image_format = match format {
            ibis_doc_model::MediaFormat::Png => Some(ImageFormat::Png),
            ibis_doc_model::MediaFormat::Jpeg => Some(ImageFormat::Jpeg),
            ibis_doc_model::MediaFormat::Gif => Some(ImageFormat::Gif),
            ibis_doc_model::MediaFormat::Bmp => Some(ImageFormat::Bmp),
            ibis_doc_model::MediaFormat::Tiff => Some(ImageFormat::Tiff),
            ibis_doc_model::MediaFormat::Svg => Some(ImageFormat::Svg),
            _ => None,
        };
        let media = self.document.ids().add_media(format, data.clone());
        if let Some(f) = image_format {
            self.resources.images.insert(media, Arc::new(Image::from_bytes(f, data)));
        }
        let size = ibis_doc_model::EmuSize::new(cx_emu, cy_emu);
        if self.run_flow(vec![FlowCommand::InsertPicture { at: caret, media, size }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
        }
    }

    /// 画像 > ファイルから…: pick a picture file and insert it.
    pub(crate) fn doc_pick_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(crate::shell::i18n::text("_shell.insert")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let format = ibis_doc_model::MediaFormat::from_path(&path.to_string_lossy());
                this.update(cx, |this, cx| this.doc_insert_picture(data, format, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// Replace the picture at `at` with new image `data`.
    /// If `keep_size` is true, the current size on screen is preserved;
    /// otherwise, the new image's natural dimensions (adjusted to fit page width) are used.
    pub(crate) fn doc_replace_picture(&mut self, at: FlowPos, data: Vec<u8>, format: ibis_doc_model::MediaFormat, keep_size: bool, cx: &mut Context<Self>) {
        let image_format = match format {
            ibis_doc_model::MediaFormat::Png => Some(ImageFormat::Png),
            ibis_doc_model::MediaFormat::Jpeg => Some(ImageFormat::Jpeg),
            ibis_doc_model::MediaFormat::Gif => Some(ImageFormat::Gif),
            ibis_doc_model::MediaFormat::Bmp => Some(ImageFormat::Bmp),
            ibis_doc_model::MediaFormat::Tiff => Some(ImageFormat::Tiff),
            ibis_doc_model::MediaFormat::Svg => Some(ImageFormat::Svg),
            _ => None,
        };
        let media = self.document.ids().add_media(format, data.clone());
        if let Some(f) = image_format {
            self.resources.images.insert(media, Arc::new(Image::from_bytes(f, data.clone())));
        }
        let size = if keep_size {
            None
        } else {
            let Some(doc) = self.flow() else { return };
            let (w, h) = picture_pixels(&data).unwrap_or((400, 300));
            let (mut cx_emu, mut cy_emu) = (w as i64 * 9525, h as i64 * 9525);
            let room = doc.page.size.width - doc.page.left - doc.page.right;
            if cx_emu > room && cx_emu > 0 {
                cy_emu = cy_emu * room / cx_emu;
                cx_emu = room;
            }
            Some(ibis_doc_model::EmuSize::new(cx_emu, cy_emu))
        };
        self.run_flow(vec![FlowCommand::ReplacePicture { at, media, size }], cx);
    }

    /// 画像の変更…: pick a picture file and replace the picture at the caret.
    pub(crate) fn doc_pick_replacement_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, _, _)) = self.picture_at_caret() else { return };
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(crate::shell::i18n::text("_docx.change-picture").to_string().into()) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let format = ibis_doc_model::MediaFormat::from_path(&path.to_string_lossy());
                this.update(cx, |this, cx| this.doc_replace_picture(target, data, format, true, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }
}

/// ページ罫線のプリセット。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageBorderPreset {
    None,
    BoxThin,
    BoxMedium,
    BoxThick,
    BoxDouble,
    TopBottom,
}

/// Page setup.
impl SlideEditor {
    /// The section the caret is in (the paragraph that ends it; 0 for the
    /// last), in the body or in a section's header or footer.
    pub(crate) fn caret_section(&self) -> u32 {
        let Some((_, caret)) = self.text_selection() else { return 0 };
        let Some(doc) = self.flow() else { return 0 };
        match flow_command::locate(doc, caret.para) {
            Some((Container::Part { section, .. }, _)) => section,
            Some((Container::Header | Container::Footer, _)) | None => 0,
            Some(_) => flow_command::section_of(doc, caret.para),
        }
    }

    /// Change the caret's section's setup (one step of undo).
    pub(crate) fn set_section(&mut self, edit: impl FnOnce(&mut ibis_doc_model::SectionSetup), cx: &mut Context<Self>) {
        let section = self.caret_section();
        let Some(command) = self.flow().and_then(|d| flow_command::set_section(d, section, edit)) else { return };
        self.run_flow(vec![command], cx);
    }

    /// Set the starting page number for the caret's section (`None` to continue
    /// numbering from previous section, `Some(n)` to restart numbering at `n`).
    pub(crate) fn set_page_number_start(&mut self, start: Option<u32>, cx: &mut Context<Self>) {
        self.set_section(
            move |s| match start {
                Some(n) => {
                    s.restart_numbering = true;
                    s.page.first_page = n.max(1);
                }
                None => {
                    s.restart_numbering = false;
                    s.page.first_page = 1;
                }
            },
            cx,
        );
    }

    /// The caret's section's page setup.
    pub(crate) fn caret_page(&self) -> Option<ibis_doc_model::PageSetup> {
        let doc = self.flow()?;
        let section = self.caret_section();
        if section == 0 {
            return Some(doc.page);
        }
        ibis_doc_model::flow_command::section_setup(doc, section).map(|s| s.page)
    }

    /// The caret's section's column settings.
    pub(crate) fn caret_columns(&self) -> ibis_doc_model::PageColumns {
        self.caret_page().map(|p| p.columns).unwrap_or_default()
    }

    /// Change the page setup (the caret's section's).
    pub(crate) fn set_page(&mut self, change: impl FnOnce(&mut ibis_doc_model::PageSetup), cx: &mut Context<Self>) {
        if self.caret_section() != 0 {
            return self.set_section(|s| change(&mut s.page), cx);
        }
        let Some(doc) = self.flow() else { return };
        let mut page = doc.page;
        change(&mut page);
        if page != doc.page {
            self.run_flow(vec![FlowCommand::SetPage { page }], cx);
        }
    }

    /// 行グリッドの行送り（None でグリッドなし）。
    pub(crate) fn set_line_pitch(&mut self, pitch: Option<ibis_doc_model::Emu>, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.line_pitch = pitch;
                p.doc_grid_type = if pitch.is_some() {
                    if p.doc_grid_type == ibis_doc_model::DocGridType::Default {
                        ibis_doc_model::DocGridType::Lines
                    } else {
                        p.doc_grid_type
                    }
                } else {
                    ibis_doc_model::DocGridType::Default
                };
            },
            cx,
        );
    }

    /// 行数・文字数の原稿設定（w:docGrid）。
    pub(crate) fn set_doc_grid(
        &mut self,
        grid_type: ibis_doc_model::DocGridType,
        line_pitch: Option<ibis_doc_model::Emu>,
        char_space: Option<ibis_doc_model::Emu>,
        cx: &mut Context<Self>,
    ) {
        self.set_page(
            |p| {
                p.doc_grid_type = grid_type;
                p.line_pitch = line_pitch;
                p.char_space = char_space;
            },
            cx,
        );
    }

    /// 段組の段数（1..=3 または最大まで）。
    pub(crate) fn set_columns(&mut self, count: u8, cx: &mut Context<Self>) {
        let count = count.clamp(1, ibis_doc_model::PageColumns::MAX as u8);
        self.set_page(
            |p| {
                p.columns.count = count;
                p.columns.widths = None;
                if count == 1 {
                    p.columns.separator = false;
                }
                if p.columns.space == 0 {
                    p.columns.space = 720 * 635;
                }
            },
            cx,
        );
    }

    /// 段間境界線（w:sep）の切り替え。
    pub(crate) fn toggle_column_separator(&mut self, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.columns.separator = !p.columns.separator;
            },
            cx,
        );
    }

    /// 段間境界線（w:sep）の設定。
    #[allow(dead_code)]
    pub(crate) fn set_column_separator(&mut self, separator: bool, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.columns.separator = separator;
            },
            cx,
        );
    }

    /// ページ罫線のプリセット適用。
    pub(crate) fn set_page_border_preset(&mut self, preset: PageBorderPreset, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                use ibis_doc_model::{BorderStyle, FlowBorder, Rgb};
                const PT: i64 = 12_700;
                let default_space = if p.borders_from_text { PT } else { 24 * PT };
                let border_of = |width: i64, style: BorderStyle| FlowBorder {
                    width,
                    color: Rgb::BLACK,
                    style,
                    space: default_space,
                    art: None,
                };
                match preset {
                    PageBorderPreset::None => {
                        p.borders = [None; 4];
                    }
                    PageBorderPreset::BoxThin => {
                        let b = Some(border_of(PT / 2, BorderStyle::Single));
                        p.borders = [b, b, b, b];
                    }
                    PageBorderPreset::BoxMedium => {
                        let b = Some(border_of(PT, BorderStyle::Single));
                        p.borders = [b, b, b, b];
                    }
                    PageBorderPreset::BoxThick => {
                        let b = Some(border_of(2 * PT, BorderStyle::Single));
                        p.borders = [b, b, b, b];
                    }
                    PageBorderPreset::BoxDouble => {
                        let b = Some(border_of(PT * 3 / 2, BorderStyle::Double));
                        p.borders = [b, b, b, b];
                    }
                    PageBorderPreset::TopBottom => {
                        let b = Some(border_of(PT, BorderStyle::Single));
                        p.borders = [b, None, b, None];
                    }
                }
            },
            cx,
        );
    }

    /// ページ罫線の基準位置（本文から / 用紙の端から）の設定。
    pub(crate) fn set_page_border_offset_from(&mut self, from_text: bool, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                const PT: i64 = 12_700;
                p.borders_from_text = from_text;
                let space = if from_text { PT } else { 24 * PT };
                for b in p.borders.iter_mut().flatten() {
                    b.space = space;
                }
            },
            cx,
        );
    }

    /// 現在のページ罫線プリセットを取得。
    pub(crate) fn page_border_preset(&self) -> PageBorderPreset {
        let Some(p) = self.caret_page() else { return PageBorderPreset::None };
        use ibis_doc_model::BorderStyle;
        const PT: i64 = 12_700;
        let [t, r, b, l] = p.borders;
        if t.is_none() && r.is_none() && b.is_none() && l.is_none() {
            return PageBorderPreset::None;
        }
        if t == b && r.is_none() && l.is_none() && t.is_some() {
            return PageBorderPreset::TopBottom;
        }
        if t == r && r == b && b == l {
            if let Some(border) = t {
                if border.style == BorderStyle::Double {
                    return PageBorderPreset::BoxDouble;
                }
                if border.width <= PT * 3 / 4 {
                    return PageBorderPreset::BoxThin;
                }
                if border.width <= PT * 5 / 4 {
                    return PageBorderPreset::BoxMedium;
                }
                return PageBorderPreset::BoxThick;
            }
        }
        PageBorderPreset::None
    }

    /// Portrait or landscape, keeping the paper.
    pub(crate) fn set_orientation(&mut self, landscape: bool, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                let (w, h) = (p.size.width.min(p.size.height), p.size.width.max(p.size.height));
                p.size = if landscape { ibis_doc_model::EmuSize::new(h, w) } else { ibis_doc_model::EmuSize::new(w, h) };
            },
            cx,
        );
    }

    /// A paper size (twips, portrait), keeping the orientation.
    pub(crate) fn set_paper(&mut self, width: i64, height: i64, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                let landscape = p.size.width > p.size.height;
                let (w, h) = (width * 635, height * 635);
                p.size = if landscape { ibis_doc_model::EmuSize::new(h, w) } else { ibis_doc_model::EmuSize::new(w, h) };
            },
            cx,
        );
    }

    /// Margins: top, right, bottom, left (twips).
    pub(crate) fn set_margins(&mut self, [t, r, b, l]: [i64; 4], cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.top = t * 635;
                p.right = r * 635;
                p.bottom = b * 635;
                p.left = l * 635;
            },
            cx,
        );
    }

    /// Text direction: across (LrTb) or vertical (TbRl).
    pub(crate) fn set_text_direction(&mut self, vertical: bool, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.direction = if vertical { ibis_doc_model::CellDirection::TbRl } else { ibis_doc_model::CellDirection::LrTb };
            },
            cx,
        );
    }
    /// Line numbering restart mode (`None` for off).
    pub(crate) fn set_line_numbers(&mut self, restart: Option<ibis_doc_model::LineNumberRestart>, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.line_numbers = restart.map(|r| {
                    let mut ln = p.line_numbers.unwrap_or(ibis_doc_model::LineNumbers {
                        count_by: 1,
                        start: 1,
                        distance: 0,
                        restart: r,
                    });
                    ln.restart = r;
                    ln
                });
            },
            cx,
        );
    }

    /// ページの色 (`w:background`).
    pub(crate) fn set_page_color(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        if self.flow().is_some_and(|d| d.page_color == color) {
            return;
        }
        self.run_flow(vec![ibis_doc_model::FlowCommand::SetPageColor(color)], cx);
    }
}

/// Section breaks.
impl SlideEditor {
    /// 挿入 > セクション区切り: the paragraph is split at the caret, and a
    /// section ends with its first half.
    pub(crate) fn can_insert_section_break(&self) -> bool {
        let Some((start,end))=self.ordered_selection() else{return false};
        let Some(doc)=self.flow() else{return false};
        [start,end].into_iter().all(|pos|flow_command::locate(doc,pos.para).is_some_and(|(container,_)|container==Container::Body))
    }
    pub(crate) fn insert_section_break(&mut self, kind: ibis_doc_model::SectionBreak, cx: &mut Context<Self>) {
        if !self.can_insert_section_break() {return;}
        let Some((start, end)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };
        let next = doc.new_id();
        let mut commands = Vec::new();
        if start != end {
            commands.push(FlowCommand::Delete { from: start, to: end });
        }
        commands.push(FlowCommand::InsertText { at: start, text: "\n".into(), format: None });
        commands.push(FlowCommand::SetParagraphFormat { first: start.para, last: start.para, change: ParaChange::SectionBreak(kind) });
        if self.run_flow(commands, cx) {
            self.set_caret(FlowPos::new(next, 0), false, cx);
            self.scroll_to_caret(cx);
        }
    }
}

/// Headers, footers and page numbers.
impl SlideEditor {
    /// 挿入 > ヘッダーとフッター: the caret into the header (made if the
    /// document has none).
    /// The caret into the header or footer of `kind` the caret's section
    /// shows (its own or an earlier section's), made when there is none.
    pub(crate) fn edit_part(&mut self, kind: ibis_doc_model::PartKind, cx: &mut Context<Self>) {
        use ibis_doc_model::PartKind;
        let section = self.caret_section();
        if section == 0 && matches!(kind, PartKind::Header | PartKind::Footer) && self.flow().is_some_and(|d| !d.linked[kind.index()] || flow_command::sections(d).len() == 1) {
            return self.edit_header(kind == PartKind::Footer, cx);
        }
        let Some(doc) = self.flow().cloned() else { return };
        let order = flow_command::sections(&doc);
        let at = order.iter().position(|s| *s == section).unwrap_or(order.len() - 1);
        let owner = order[..=at].iter().rev().copied().find(|s| flow_command::section_setup(&doc, *s).is_some_and(|x| x.parts[kind.index()].as_ref().is_some_and(|b| !b.is_empty())));
        let owner = match owner {
            Some(o) => o,
            None => {
                let Some(command) = flow_command::link_part(&doc, section, kind, true) else { return };
                if !self.run_flow(vec![command], cx) {
                    return;
                }
                section
            }
        };
        let Some(doc) = self.flow() else { return };
        let container = match (owner, kind) {
            (0, PartKind::Header) => Container::Header,
            (0, PartKind::Footer) => Container::Footer,
            (s, k) => Container::Part { section: s, kind: k },
        };
        if let Some(Block::Paragraph(p)) = flow_command::blocks(doc, container).and_then(|b| b.first()) {
            let pos = FlowPos::new(p.id, p.len());
            self.set_caret(pos, false, cx);
            self.scroll_to_caret(cx);
        }
    }

    /// 前と同じ: the caret's section's header or footer of `kind` made the
    /// previous section's again, or its own (a copy).
    pub(crate) fn toggle_linked(&mut self, kind: ibis_doc_model::PartKind, cx: &mut Context<Self>) {
        let section = self.caret_section();
        let Some(doc) = self.flow() else { return };
        let own = flow_command::section_setup(doc, section).is_some_and(|s| s.parts[kind.index()].is_some());
        let Some(command) = flow_command::link_part(doc, section, kind, !own) else { return };
        // The caret leaves a header that goes.
        let caret_in = self.text_selection().and_then(|(_, c)| flow_command::locate(doc, c.para)).map(|l| l.0);
        if self.run_flow(vec![command], cx)
            && caret_in.is_some_and(|c| matches!(c, Container::Part { .. } | Container::Header | Container::Footer))
            && self.flow().is_some_and(|d| self.text_selection().is_some_and(|(_, c)| flow_command::locate(d, c.para).is_none()))
            && let Some(start) = self.document_edge(false)
        {
            self.set_caret(start, false, cx);
        }
    }

    /// Whether the caret's section's header or footer of `kind` is the
    /// previous section's (前と同じ), and whether there is a previous one.
    pub(crate) fn part_linked(&self, kind: ibis_doc_model::PartKind) -> (bool, bool) {
        let section = self.caret_section();
        let Some(doc) = self.flow() else { return (false, false) };
        let first = flow_command::sections(doc).first() == Some(&section);
        (flow_command::section_setup(doc, section).is_some_and(|s| s.parts[kind.index()].is_none()), !first)
    }

    /// 奇数／偶数ページ別指定.
    pub(crate) fn toggle_even_and_odd(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let command = flow_command::set_even_and_odd(doc, !doc.even_and_odd);
        self.run_flow(vec![command], cx);
    }

    pub(crate) fn edit_header(&mut self, footer: bool, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let list = if footer { &doc.footer } else { &doc.header };
        if list.is_empty() {
            let id = doc.new_id();
            if self.run_flow(vec![FlowCommand::AddHeader { footer }], cx) {
                self.set_caret(FlowPos::new(id, 0), false, cx);
            }
            return;
        }
        if let Some(Block::Paragraph(p)) = list.first() {
            let pos = FlowPos::new(p.id, p.len());
            self.set_caret(pos, false, cx);
        }
    }

    /// 挿入 > ページ番号: insert page number at specified position.
    pub(crate) fn insert_page_number_at(&mut self, pos: PageNumberPosition, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(doc) = self.flow().cloned() else { return };
        if pos == PageNumberPosition::Current {
            if self.run_flow(vec![FlowCommand::InsertPageNumber { at: caret }], cx) {
                self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            }
            return;
        }
        let in_header = matches!(flow_command::locate(&doc, caret.para), Some((Container::Header | Container::Footer | Container::Part { .. }, _)));
        if in_header {
            if self.run_flow(vec![FlowCommand::InsertPageNumber { at: caret }], cx) {
                self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            }
            return;
        }
        let (footer, align) = match pos {
            PageNumberPosition::HeaderLeft => (false, FlowAlign::Left),
            PageNumberPosition::HeaderCenter => (false, FlowAlign::Center),
            PageNumberPosition::HeaderRight => (false, FlowAlign::Right),
            PageNumberPosition::FooterLeft => (true, FlowAlign::Left),
            PageNumberPosition::FooterCenter => (true, FlowAlign::Center),
            PageNumberPosition::FooterRight => (true, FlowAlign::Right),
            PageNumberPosition::Current => unreachable!(),
        };
        let mut commands = Vec::new();
        let target_list = if footer { &doc.footer } else { &doc.header };
        let para = match target_list.first() {
            Some(Block::Paragraph(p)) => {
                commands.push(FlowCommand::InsertPageNumber { at: FlowPos::new(p.id, p.len()) });
                p.id
            }
            Some(_) => return,
            None => {
                let id = doc.new_id();
                commands.push(FlowCommand::AddHeader { footer });
                commands.push(FlowCommand::InsertPageNumber { at: FlowPos::new(id, 0) });
                id
            }
        };
        commands.push(FlowCommand::SetParagraphFormat { first: para, last: para, change: ParaChange::Align(align) });
        self.run_flow(commands, cx);
    }

    /// 挿入 > ページ番号: at the caret in a header or footer; else at the
    /// bottom of the page, centred (a footer made if there is none).
    pub(crate) fn insert_page_number(&mut self, cx: &mut Context<Self>) {
        self.insert_page_number_at(PageNumberPosition::FooterCenter, cx);
    }
}

/// The position where a page number is placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageNumberPosition {
    Current,
    HeaderLeft,
    HeaderCenter,
    HeaderRight,
    FooterLeft,
    FooterCenter,
    FooterRight,
}

/// The notes and comments marked in the body (a comment: its start, end
/// and mark all there).
pub(crate) struct Marked {
    notes: std::collections::HashSet<(bool, u32)>,
    comments: std::collections::HashSet<u32>,
}

fn marked(doc: &FlowDocument) -> Marked {
    let mut out = Marked { notes: Default::default(), comments: Default::default() };
    if doc.footnotes.is_empty() && doc.endnotes.is_empty() && doc.comments.is_empty() {
        return out;
    }
    fn walk(blocks: &[Block], notes: &mut std::collections::HashSet<(bool, u32)>, marks: &mut std::collections::HashMap<u32, [bool; 3]>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        match r.kind {
                            ibis_doc_model::FlowRunKind::NoteRef { endnote, id } => {
                                notes.insert((endnote, id));
                            }
                            ibis_doc_model::FlowRunKind::CommentStart(i) => marks.entry(i).or_default()[0] = true,
                            ibis_doc_model::FlowRunKind::CommentEnd(i) => marks.entry(i).or_default()[1] = true,
                            ibis_doc_model::FlowRunKind::CommentRef(i) => marks.entry(i).or_default()[2] = true,
                            _ => {}
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, notes, marks)),
            }
        }
    }
    let mut marks = std::collections::HashMap::new();
    walk(&doc.body, &mut out.notes, &mut marks);
    out.comments = marks.into_iter().filter(|(_, m)| *m == [true; 3]).map(|(i, _)| i).collect();
    out
}

/// Tables of contents, links to bookmarks, and the outline.
impl SlideEditor {
    /// 見出しに番号を付ける: the heading styles numbered 1 / 1.1 / 1.1.1 (one
    /// step of undo).
    pub(crate) fn number_headings(&mut self, cx: &mut Context<Self>) {
        let Some(commands) = self.flow().and_then(|d| flow_command::heading_numbering(d)) else { return };
        self.run_flow(commands, cx);
    }

    /// フィールドの更新: the captions' numbers and the cross-references
    /// given their values now (one step of undo, or added to the last).
    /// If text is selected, only fields in the selection are updated.
    pub(crate) fn update_fields(&mut self, amend: bool, cx: &mut Context<Self>) {
        let range = self.ordered_selection().filter(|(a, b)| a != b);
        if range.is_none() {
            let caret_para = self.text_selection().map(|(_, c)| c.para);
            if caret_para.and_then(|p| self.toc_start_kind(Some(p))).is_some() {
                self.update_toc(None, cx);
                return;
            }
            // The index is made again with the fields.
            if self.flow().is_some_and(|d| super::index_fields::index_field(d).is_some()) {
                self.update_index(cx);
            }
        }
        let now = crate::clock::local_now();
        let Some(commands) = self.flow().map(|d| flow_command::updated_fields_in_range(d, range, Some(&now))) else { return };
        if commands.is_empty() {
            return;
        }
        if amend {
            let _ = self.document.amend_last(commands.into_iter().map(Command::Flow).collect());
            cx.notify();
        } else {
            self.run_flow(commands, cx);
        }
    }

    /// 文書内のすべてのフィールド（本文・ヘッダー・フッター・目次を含む）を更新する。
    pub(crate) fn update_all_document_fields(&mut self, cx: &mut Context<Self>) {
        if self.toc_start_kind(None).is_some() {
            self.update_toc(None, cx);
        }
        if self.flow().is_some_and(|d| super::index_fields::index_field(d).is_some()) {
            self.update_index(cx);
        }
        let now = crate::clock::local_now();
        let Some(commands) = self.flow().map(|d| flow_command::updated_fields_in_range(d, None, Some(&now))) else { return };
        if !commands.is_empty() {
            self.run_flow(commands, cx);
        }
    }

    /// 印刷前にフィールドを更新するかどうか (Word Options > 表示 > 印刷オプション: <w:updateFields/>).
    pub(crate) fn is_update_fields_before_printing(&self) -> bool {
        self.flow().map_or(false, |d| d.update_fields)
    }

    /// 印刷前のフィールド更新設定を変更する（1ステップの Undo/Redo）。
    pub(crate) fn set_update_fields_before_printing(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.is_update_fields_before_printing() == on {
            return;
        }
        self.run_flow(vec![FlowCommand::SetUpdateFields(on)], cx);
    }

    /// 印刷前のフィールド更新設定を切り替える。
    pub(crate) fn toggle_update_fields_before_printing(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_update_fields_before_printing();
        self.set_update_fields_before_printing(next, cx);
    }

    /// 印刷前にリンクされたデータを更新するかどうか (Word Options > 表示 > 印刷オプション).
    pub(crate) fn is_update_links_before_printing(&self) -> bool {
        self.update_links_before_printing
    }

    /// 印刷前のリンクデータ更新設定を変更する。
    #[allow(dead_code)]
    pub(crate) fn set_update_links_before_printing(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.update_links_before_printing != on {
            self.update_links_before_printing = on;
            cx.notify();
        }
    }

    /// 印刷前のリンクデータ更新設定を切り替える。
    pub(crate) fn toggle_update_links_before_printing(&mut self, cx: &mut Context<Self>) {
        self.update_links_before_printing = !self.update_links_before_printing;
        cx.notify();
    }

    /// 背景の色とイメージを印刷するかどうか (Word Options > 表示 > 印刷オプション DOCX-OPTIONS-010).
    pub(crate) fn is_print_backgrounds(&self) -> bool {
        self.print_backgrounds
    }

    /// 背景の色とイメージの印刷設定を変更する。
    pub(crate) fn set_print_backgrounds(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.print_backgrounds != on {
            self.print_backgrounds = on;
            self.resources.print_backgrounds = on;
            crate::docx::flow_layout::clear_layout_caches();
            cx.notify();
        }
    }

    /// 背景の色とイメージの印刷設定を切り替える。
    pub(crate) fn toggle_print_backgrounds(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_print_backgrounds();
        self.set_print_backgrounds(next, cx);
    }

    /// 隠し文字を印刷するかどうか (Word Options > 表示 > 印刷オプション DOCX-OPTIONS-010).
    pub(crate) fn is_print_hidden_text(&self) -> bool {
        self.print_hidden_text
    }

    /// 隠し文字の印刷設定を変更する。
    pub(crate) fn set_print_hidden_text(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.print_hidden_text != on {
            self.print_hidden_text = on;
            self.resources.print_hidden_text = on;
            crate::docx::flow_layout::clear_layout_caches();
            cx.notify();
        }
    }

    /// 隠し文字の印刷設定を切り替える。
    pub(crate) fn toggle_print_hidden_text(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_print_hidden_text();
        self.set_print_hidden_text(next, cx);
    }

    /// 印刷またはPDF書き出し前の更新準備処理。
    /// 「印刷前にフィールドを更新する」が有効な場合、文書内のフィールドと目次を更新する。
    /// 「印刷前にリンクされたデータを更新する」が有効な場合、リンクされたデータを更新する。
    pub(crate) fn prepare_print_or_export(&mut self, cx: &mut Context<Self>) {
        if self.flow().is_none() {
            return;
        }
        if self.is_update_fields_before_printing() {
            self.update_all_document_fields(cx);
        }
        if self.is_update_links_before_printing() {
            // リンクデータ更新の必要処理
        }
    }

    /// フィールドのリンク解除 (Ctrl+Shift+F9 / ⇧⌘F9): 選択範囲内（またはキャレット位置、選択がなければ文書全体）の
    /// フィールドを静的なプレーンテキストに変換（1ステップの Undo/Redo）。
    pub(crate) fn unlink_fields(&mut self, cx: &mut Context<Self>) {
        let now = crate::clock::local_now();
        let range = self.ordered_selection();
        let Some(commands) = self.flow().map(|d| flow_command::unlink_fields_at(d, range, Some(&now))) else { return };
        if commands.is_empty() {
            return;
        }
        self.run_flow(commands, cx);
    }

    /// フィールドコードの表示/非表示 (Shift+F9 / ⇧F9): 選択範囲内（またはキャレット位置）の
    /// フィールドのコード表示/結果表示をトグル（1ステップの Undo/Redo）。
    pub(crate) fn toggle_field_codes(&mut self, cx: &mut Context<Self>) {
        let range = self.ordered_selection();
        let Some(commands) = self.flow().map(|d| flow_command::toggle_field_codes_at(d, range)) else { return };
        if commands.is_empty() {
            return;
        }
        self.run_flow(commands, cx);
    }

    /// すべてのフィールドコードの表示/非表示 (Alt+F9 / ⌥F9): 文書全体のすべての
    /// フィールドのコード表示/結果表示をトグル（1ステップの Undo/Redo）。
    pub(crate) fn toggle_all_field_codes(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let cmd = flow_command::toggle_all_field_codes(doc);
        self.run_flow(vec![cmd], cx);
    }

    /// フィールドのロック / ロック解除 (Ctrl+F11 / ⌘F11 / Ctrl+Shift+F11 / ⇧⌘F11):
    /// 選択範囲内（またはキャレット位置、選択がなければ文書全体）のフィールドをロック/解除（1ステップの Undo/Redo）。
    pub(crate) fn lock_fields(&mut self, locked: bool, cx: &mut Context<Self>) {
        let range = self.ordered_selection();
        let Some(commands) = self.flow().map(|d| flow_command::lock_fields_at(d, range, locked)) else { return };
        if commands.is_empty() {
            return;
        }
        self.run_flow(commands, cx);
    }

    /// フィールドの挿入 (Ctrl+F9 / ⌘F9):
    /// 選択範囲がある場合は選択テキストをコードとするフィールドに即座に変換し、
    /// 選択がない場合は空のフィールドブラケット `{ }` を挿入してコード編集状態にする。
    pub(crate) fn insert_field(&mut self, cx: &mut Context<Self>) {
        let sel_text = self.selected_text();
        let range = self.ordered_selection();
        let Some(doc) = self.flow() else { return };
        if let Some((cmds, new_pos)) = flow_command::insert_field_at_range(doc, range, &sel_text, true) {
            if self.run_flow(cmds, cx) {
                self.set_caret(new_pos, false, cx);
            }
        }
    }

    /// フィールドの編集: キャレット位置のフィールドのコードを編集する。
    pub(crate) fn edit_field_with_code(&mut self, new_code: &str, cx: &mut Context<Self>) {
        let Some((caret, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };
        if let Some(cmds) = flow_command::edit_field_at(doc, caret, new_code) {
            self.run_flow(cmds, cx);
        }
    }

    /// 表の計算式フィールド（= Formula）を挿入する。
    pub(crate) fn insert_table_formula(&mut self, formula: &str, format: Option<&str>, cx: &mut Context<Self>) {
        let mut instr = formula.trim().to_string();
        if !instr.starts_with('=') {
            instr = format!("= {instr}");
        }
        if let Some(fmt) = format {
            let fmt = fmt.trim();
            if !fmt.is_empty() {
                instr = format!("{instr} \\# \"{fmt}\"");
            }
        }
        let range = self.ordered_selection();
        let Some(doc) = self.flow() else { return };
        if let Some((cmds, new_pos)) = flow_command::insert_field_at_range(doc, range, &instr, false) {
            if self.run_flow(cmds, cx) {
                self.set_caret(new_pos, false, cx);
            }
        }
    }

    /// 現在の表セル位置から Word 標準のデフォルト計算式（=SUM(ABOVE) または =SUM(LEFT) 等）を提案する。
    pub(crate) fn suggest_table_formula(&self) -> String {
        let Some((_, row, col)) = self.table_at_caret() else {
            return "=SUM(ABOVE)".to_string();
        };
        let Some(table) = self.caret_table() else {
            return "=SUM(ABOVE)".to_string();
        };
        let resolver = ibis_doc_model::FlowTableCellResolver { table, row, col };
        use ibis_doc_model::TableCellResolver;
        if !resolver.resolve_direction("ABOVE").is_empty() {
            "=SUM(ABOVE)".to_string()
        } else if !resolver.resolve_direction("LEFT").is_empty() {
            "=SUM(LEFT)".to_string()
        } else {
            "=SUM()".to_string()
        }
    }

    /// 図表番号の詳細挿入: ラベル・説明文・書式・ラベル除外オプションを指定して挿入
    pub(crate) fn insert_caption_detailed(
        &mut self,
        label: &str,
        text: &str,
        format: ibis_doc_model::CaptionNumberFormat,
        exclude_label: bool,
        chapter: Option<(u32, &str)>,
        cx: &mut Context<Self>,
    ) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some((commands, _)) = self.flow().and_then(|d| flow_command::insert_caption_in_chapter(d, start.para, label, text, format, exclude_label, chapter)) else { return };
        let new_id = self.flow().map(|d| d.new_id());
        if self.run_flow(commands, cx) {
            self.update_fields(true, cx);
            if let Some(id) = new_id
                && let Some(len) = self.flow().and_then(|d| flow_command::paragraph(d, id)).map(|p| p.len())
            {
                self.set_caret(FlowPos::new(id, len), false, cx);
            }
        }
    }

    /// 図表番号の挿入: a caption (`label` and its number) after the caret's
    /// paragraph, the caret at its end; the numbers after it go up.
    pub(crate) fn insert_caption(&mut self, label: &str, cx: &mut Context<Self>) {
        self.insert_caption_detailed(label, "", ibis_doc_model::CaptionNumberFormat::Arabic, false, None, cx);
    }

    /// 相互参照: a reference to caption paragraph `caption` at the caret.
    pub(crate) fn insert_cross_reference(&mut self, caption: u32, cx: &mut Context<Self>) {
        let Some((start, end)) = self.ordered_selection() else { return };
        if start != end {
            return;
        }
        let Some(commands) = self.flow().and_then(|d| flow_command::cross_reference(d, start, caption)) else { return };
        if self.run_flow(commands, cx) {
            self.set_caret(FlowPos::new(start.para, start.offset + 1), false, cx);
        }
    }

    /// 日付と時刻: the time now in `picture` (a field's `\@`) at the caret
    /// (over the selection): a `DATE` field (`TIME` when the picture has no
    /// date) when `update` (自動的に更新する), else its text.
    pub(crate) fn insert_date(&mut self, picture: &str, update: bool, cx: &mut Context<Self>) {
        let now = crate::clock::local_now();
        let text = ibis_doc_model::date_field::format_picture(&now, picture);
        if !update {
            self.type_text(&text, cx);
            return;
        }
        let Some((start, end)) = self.ordered_selection() else { return };
        if start != end {
            self.type_text("", cx);
        }
        let Some((at, _)) = self.ordered_selection() else { return };
        let kind = if picture.contains(['y', 'M', 'd', 'g', 'e']) || picture.contains("aaa") { "DATE" } else { "TIME" };
        let field = ibis_doc_model::FlowRunKind::Field { instruction: format!("{kind} \\@ \"{picture}\""), shown: text };
        if self.run_flow(vec![FlowCommand::InsertField { at, kind: field }], cx) {
            self.set_caret(FlowPos::new(at.para, at.offset + 1), false, cx);
        }
    }

    /// 変更履歴の記録: on or off.
    pub(crate) fn toggle_track_changes(&mut self, cx: &mut Context<Self>) {
        if self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("_docx.track-changes-is-locked-so-it-can-t-be-turned-of").into());
            cx.notify();
            return;
        }
        let on = self.flow().is_some_and(|d| d.track_changes);
        self.run_flow(vec![FlowCommand::SetTrackChanges(!on)], cx);
    }

    /// 変更履歴の記録がロックされているか。
    pub(crate) fn is_track_changes_locked(&self) -> bool {
        self.flow().is_some_and(|d| d.lock_tracking)
    }

    /// 変更履歴の記録のロック: on or off.
    pub(crate) fn toggle_lock_tracking(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let next_locked = !doc.lock_tracking;
        self.run_flow(
            vec![FlowCommand::SetLockTracking {
                locked: next_locked,
                track_changes: if next_locked { true } else { doc.track_changes },
            }],
            cx,
        );
    }

    // The shared revision operation walks its body. Apply it to each DOCX
    // story, then map replacements back to their real containers. Keep the
    // common model's body-only traversal unchanged for numbering/captions.
    fn story_revision_commands_filter(
        doc: &FlowDocument,
        range: Option<(FlowPos, FlowPos)>,
        accept: bool,
        filter: Option<&std::collections::HashSet<String>>,
    ) -> Option<Vec<FlowCommand>> {
        fn contains(blocks: &[Block], id: u32) -> bool {
            blocks.iter().any(|b| match b {
                Block::Paragraph(p) => p.id == id,
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).any(|c| contains(&c.blocks, id)),
            })
        }
        let mut stories: Vec<&[Block]> = vec![&doc.body];
        for block in &doc.body {
            if let Block::Paragraph(p) = block && let Some(section) = &p.section {
                stories.extend(section.parts.iter().flatten().map(|b| b.as_slice()));
            }
        }
        stories.extend([doc.header.as_slice(), doc.footer.as_slice(), doc.header_first.as_slice(), doc.footer_first.as_slice(), doc.header_even.as_slice(), doc.footer_even.as_slice()]);
        stories.extend(doc.footnotes.iter().chain(&doc.endnotes).map(|n| n.blocks.as_slice()));
        let mut view = doc.clone();
        let mut seen = std::collections::HashSet::new();
        let mut commands = Vec::new();
        let mut matched = range.is_none();
        for story in stories {
            if range.is_some_and(|(a, b)| !contains(story, a.para) || !contains(story, b.para)) { continue; }
            matched = true;
            view.body = story.to_vec();
            for command in flow_command::resolve_revisions_filter(&view, range, accept, filter) {
                let FlowCommand::ReplaceBlocks { remove, blocks, .. } = command else { return None };
                let Some(Block::Paragraph(p)) = blocks.first() else { return None };
                let Some((container, index)) = flow_command::locate(doc, p.id) else { return None };
                if seen.insert(p.id) { commands.push(FlowCommand::ReplaceBlocks { container, index, remove, blocks }); }
            }
        }
        matched.then_some(commands)
    }

    fn story_revision_commands(doc: &FlowDocument, range: Option<(FlowPos, FlowPos)>, accept: bool) -> Option<Vec<FlowCommand>> {
        Self::story_revision_commands_filter(doc, range, accept, None)
    }

    /// 承諾・却下: the tracked changes in the selection (or at the caret), or
    /// all of them.
    pub(crate) fn resolve_revisions(&mut self, all: bool, accept: bool, cx: &mut Context<Self>) {
        if self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("_docx.cannot-accept-or-reject-changes-because-track-ch").into());
            cx.notify();
            return;
        }
        let range = if all { None } else {
            let Some(selection) = self.ordered_selection() else { return };
            Some(selection)
        };
        let Some(doc) = self.flow() else { return };
        let Some(commands) = Self::story_revision_commands(doc, range, accept) else {
            self.status = Some(crate::shell::i18n::text("docx.editing.select-changes-accept-reject").into());
            cx.notify();
            return;
        };
        if commands.is_empty() {
            return;
        }
        let caret = self.text_selection().map(|(_, c)| c);
        if self.run_flow(commands, cx)
            && let Some(c) = caret
        {
            let len = self.paragraph_of(c.para).map_or(0, |p| p.len());
            self.set_caret(FlowPos::new(c.para, c.offset.min(len)), false, cx);
        }
    }

    /// 承諾・却下: 表示されたすべての変更履歴（校閲者フィルター適用）(DOCX-REVIEW-009)
    pub(crate) fn resolve_revisions_shown(&mut self, accept: bool, cx: &mut Context<Self>) {
        if self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("_docx.cannot-accept-or-reject-changes-because-track-ch").into());
            cx.notify();
            return;
        }
        let Some(doc) = self.flow() else { return };
        let filter = self.reviewer_filter.as_ref();
        let Some(commands) = Self::story_revision_commands_filter(doc, None, accept, filter) else {
            return;
        };
        if commands.is_empty() {
            return;
        }
        let caret = self.text_selection().map(|(_, c)| c);
        if self.run_flow(commands, cx)
            && let Some(c) = caret
        {
            let len = self.paragraph_of(c.para).map_or(0, |p| p.len());
            self.set_caret(FlowPos::new(c.para, c.offset.min(len)), false, cx);
        }
    }

    /// すべての変更を承諾・却下し、変更記録を停止 (DOCX-REVIEW-009)
    pub(crate) fn resolve_revisions_and_stop_tracking(&mut self, accept: bool, cx: &mut Context<Self>) {
        if self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("_docx.cannot-accept-or-reject-changes-because-track-ch").into());
            cx.notify();
            return;
        }
        let Some(doc) = self.flow() else { return };
        let mut commands = Self::story_revision_commands(doc, None, accept).unwrap_or_default();
        if doc.track_changes {
            commands.push(FlowCommand::SetTrackChanges(false));
        }
        if commands.is_empty() {
            return;
        }
        let caret = self.text_selection().map(|(_, c)| c);
        if self.run_flow(commands, cx)
            && let Some(c) = caret
        {
            let len = self.paragraph_of(c.para).map_or(0, |p| p.len());
            self.set_caret(FlowPos::new(c.para, c.offset.min(len)), false, cx);
        }
    }

    /// Whether the caret's paragraph is in a numbered (not bulleted) list.
    pub(crate) fn caret_list_numbered(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        flow_command::paragraph(doc, start.para).and_then(|p| p.list).and_then(|l| doc.styles.numbering.level(l.num_id, l.level)).is_some_and(|l| l.format != "bullet")
    }

    /// 番号の振り直し: the caret's list numbered from its paragraph on as
    /// `how` (one step of undo).
    pub(crate) fn renumber_list(&mut self, how: flow_command::Renumber, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(commands) = self.flow().and_then(|d| flow_command::renumber(d, start.para, how)) else { return };
        let keep = self.text_selection();
        self.run_flow(commands, cx);
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
    }

    /// The table of contents' first paragraph and its kind (headings, figures, or custom styles).
    fn toc_start_kind(&self, at_para: Option<u32>) -> Option<(u32, TocType)> {
        let doc = self.flow()?;
        let parse_kind = |instr: &str| -> TocType {
            if let Some(label) = flow_command::toc_caption_label(instr) {
                TocType::Figures(label)
            } else {
                let instruction = flow_command::parse_toc_instruction(instr);
                if !instruction.custom_styles.is_empty() {
                    TocType::Custom(instruction)
                } else {
                    TocType::Headings(instruction.heading_levels.unwrap_or((1, 3)))
                }
            }
        };

        if let Some(target) = at_para {
            let mut current_toc = None;
            for b in &doc.body {
                if let Block::Paragraph(p) = b {
                    if let Some(instr) = p.field_start.as_ref().filter(|i| i.trim_start().starts_with("TOC")) {
                        current_toc = Some((p.id, parse_kind(instr)));
                    }
                    if p.id == target && current_toc.is_some() {
                        return current_toc;
                    }
                    if p.field_end {
                        current_toc = None;
                    }
                }
            }
            return None;
        }

        doc.body.iter().find_map(|b| match b {
            Block::Paragraph(p) => {
                let instr = p.field_start.as_ref().filter(|i| i.trim_start().starts_with("TOC"))?;
                Some((p.id, parse_kind(instr)))
            }
            _ => None,
        })
    }

    /// The table of contents' first paragraph and its levels.
    #[allow(dead_code)]
    fn toc_start(&self) -> Option<(u32, (u32, u32))> {
        self.toc_start_kind(None).and_then(|(id, k)| match k {
            TocType::Headings(lvls) => Some((id, lvls)),
            TocType::Custom(instr) => Some((id, instr.heading_levels.unwrap_or((1, 3)))),
            TocType::Figures(_) => None,
        })
    }

    /// The commands putting a table of contents with `instruction` before `before`
    /// (or in place of the one starting there), with today's pages.
    fn toc_command(&self, before: u32, replace: bool, instruction: &flow_command::TocInstruction) -> Option<FlowCommand> {
        let doc = self.flow()?;
        let width = doc.page.size.width - doc.page.left - doc.page.right;
        let lines = self.flow_lines();
        let first = doc.page.first_page;
        let page_of = |id: u32| lines.iter().find(|l| l.para == id).map(|l| first + l.page as u32);
        let (entries, bookmarks) = flow_command::toc_entries_with_instruction(doc, instruction, &page_of, doc.new_id(), width);
        Some(FlowCommand::SetToc { before, replace, entries, bookmarks })
    }

    /// Put a table of contents with `instruction` before `before` (or in place of
    /// the one starting there) as one step, defining the styles it uses
    /// (`toc 1` …) the document lacks first. Whether it did.
    fn run_toc(&mut self, before: u32, replace: bool, instruction: &flow_command::TocInstruction, cx: &mut Context<Self>) -> bool {
        let missing = self.flow().map(|d| flow_command::missing_toc_styles_for_levels(&d.styles, instruction.all_levels())).unwrap_or_default();
        let styled = !missing.is_empty();
        if styled && !self.run_flow(missing.into_iter().map(|(id, s)| FlowCommand::SetStyle { id, style: Some(s) }).collect(), cx) {
            return false;
        }
        let Some(command) = self.toc_command(before, replace, instruction) else { return styled };
        if styled { self.document.amend_last(vec![Command::Flow(command)]).is_ok() } else { self.run_flow(vec![command], cx) }
    }

    /// 挿入 > 目次: a table of contents of headings 1 to 3 before the
    /// caret's paragraph, its page numbers those after it is in.
    pub(crate) fn insert_toc(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if self.flow().and_then(|d| flow_command::locate(d, start.para)).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        if self.toc_start_kind(None).is_some() {
            return self.update_toc(None, cx);
        }
        let instr = flow_command::TocInstruction::default();
        if self.run_toc(start.para, false, &instr, cx) {
            // The pages moved with the table of contents: numbered again, in
            // the same step of undo.
            self.relayout_if_changed();
            if let Some((first, kind)) = self.toc_start_kind(None) {
                let latest_instr = match kind {
                    TocType::Custom(i) => i,
                    TocType::Headings(lvls) => flow_command::TocInstruction { heading_levels: Some(lvls), ..Default::default() },
                    _ => instr,
                };
                if let Some(command) = self.toc_command(first, true, &latest_instr) {
                    let _ = self.document.amend_last(vec![Command::Flow(command)]);
                }
            }
            cx.notify();
        }
    }

    /// 挿入 > 手動作成の目次: placeholder entries (levels 1 to 3) before the caret's
    /// paragraph, to type over; the `toc N` styles it uses are defined first, in the same step.
    pub(crate) fn insert_manual_toc(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };
        if flow_command::locate(doc, start.para).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        let missing = flow_command::missing_toc_styles_for_levels(&doc.styles, [1, 2, 3]);
        let styled = !missing.is_empty();
        if styled && !self.run_flow(missing.into_iter().map(|(id, s)| FlowCommand::SetStyle { id, style: Some(s) }).collect(), cx) {
            return;
        }
        let Some(doc) = self.flow() else { return };
        let width = doc.page.size.width - doc.page.left - doc.page.right;
        let entries = flow_command::manual_toc_entries(doc, doc.new_id(), width);
        let command = FlowCommand::SetToc { before: start.para, replace: false, entries, bookmarks: Vec::new() };
        let ok = if styled { self.document.amend_last(vec![Command::Flow(command)]).is_ok() } else { self.run_flow(vec![command], cx) };
        if ok {
            self.relayout_if_changed();
            cx.notify();
        }
    }

    /// The page (its number) each paragraph starts on, for an index.
    fn index_page_of(&self) -> impl Fn(u32) -> Option<u32> + use<> {
        let pages: std::collections::HashMap<u32, u32> = match self.flow() {
            Some(doc) => {
                let laid = super::flow_layout::pages(doc, &self.resources);
                let mut map = std::collections::HashMap::new();
                for pg in &laid.pages {
                    for item in &pg.items {
                        if let super::flow_layout::Item::Line { para, .. } = item {
                            map.entry(*para).or_insert(pg.number);
                        }
                    }
                }
                map
            }
            None => Default::default(),
        };
        move |id| pages.get(&id).copied()
    }

    /// 挿入 > 索引: an index (INDEX) of the entries the XE fields mark, before the caret's paragraph; if the document has one, it is made again.
    pub(crate) fn insert_index(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if self.flow().and_then(|d| flow_command::locate(d, start.para)).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        if super::index_fields::index_field(self.flow().map(|d| &**d).unwrap_or(&FlowDocument::default())).is_some() {
            return self.update_index(cx);
        }
        self.run_index(start.para, false, "INDEX \\h \"A\" \\z \"1041\"", cx);
    }

    /// 索引の更新: the index made again from the XE fields now.
    pub(crate) fn update_index(&mut self, cx: &mut Context<Self>) {
        let Some((first, instruction)) = self.flow().and_then(|d| super::index_fields::index_field(d)) else { return };
        self.run_index(first, true, &instruction, cx);
    }

    fn run_index(&mut self, before: u32, replace: bool, instruction: &str, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let page_of = self.index_page_of();
        let commands = super::index_fields::index_commands(doc, before, replace, instruction, &page_of);
        if self.run_flow(commands, cx) {
            // The pages moved with the index: numbered again, in the same step of undo.
            self.relayout_if_changed();
            if let Some((first, instruction)) = self.flow().and_then(|d| super::index_fields::index_field(d)) {
                let page_of = self.index_page_of();
                if let Some(doc) = self.flow() {
                    let again: Vec<Command> = super::index_fields::index_commands(doc, first, true, &instruction, &page_of).into_iter().filter(|c| matches!(c, FlowCommand::SetToc { .. })).map(Command::Flow).collect();
                    let _ = self.document.amend_last(again);
                }
            }
            cx.notify();
        }
    }

    /// 索引登録: an XE field after the selected text, with that text as the entry (the selection in one paragraph).
    pub(crate) fn mark_index_entry(&mut self, cx: &mut Context<Self>) {
        let Some((start, end)) = self.ordered_selection() else { return };
        if start == end || start.para != end.para {
            self.status = Some(crate::shell::i18n::text("docx.editing.select-text-mark-index-entry").into());
            return cx.notify();
        }
        let text = self.selected_text().replace('"', "\\\"");
        let kind = FlowRunKind::Field { instruction: format!("XE \"{text}\" "), shown: String::new() };
        if self.run_flow(vec![FlowCommand::InsertField { at: end, kind }], cx) {
            self.set_caret(FlowPos::new(end.para, end.offset + 1), false, cx);
        }
    }

    /// 任意スタイル指定（\t スイッチなど）の目次を挿入する。
    pub(crate) fn insert_toc_with_instruction(&mut self, instruction: flow_command::TocInstruction, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if self.flow().and_then(|d| flow_command::locate(d, start.para)).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        if self.run_toc(start.para, false, &instruction, cx) {
            self.relayout_if_changed();
            if let Some((first, _)) = self.toc_start_kind(None) {
                if let Some(command) = self.toc_command(first, true, &instruction) {
                    let _ = self.document.amend_last(vec![Command::Flow(command)]);
                }
            }
            cx.notify();
        }
    }

    /// 挿入 > 図表目次: a table of figures for `label` before the caret's paragraph.
    pub(crate) fn insert_table_of_figures(&mut self, label: &str, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if self.flow().and_then(|d| flow_command::locate(d, start.para)).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        if let Some(first) = self.run_table_of_figures(start.para, false, label, cx) {
            self.relayout_if_changed();
            if let Some(command) = self.table_of_figures_command(first, true, label) {
                let _ = self.document.amend_last(vec![Command::Flow(command)]);
            }
            cx.notify();
        }
    }

    fn table_of_figures_command(&self, before: u32, replace: bool, label: &str) -> Option<FlowCommand> {
        let doc = self.flow()?;
        let width = doc.page.size.width - doc.page.left - doc.page.right;
        let lines = self.flow_lines();
        let first = doc.page.first_page;
        let page_of = |id: u32| lines.iter().find(|l| l.para == id).map(|l| first + l.page as u32);
        let (entries, bookmarks) = flow_command::table_of_figures_entries(doc, label, &page_of, doc.new_id(), width);
        Some(FlowCommand::SetToc { before, replace, entries, bookmarks })
    }

    fn run_table_of_figures(&mut self, before: u32, replace: bool, label: &str, cx: &mut Context<Self>) -> Option<u32> {
        let missing = self.flow().and_then(|d| flow_command::missing_table_of_figures_style(&d.styles));
        let styled = missing.is_some();
        if let Some((id, s)) = missing {
            if !self.run_flow(vec![FlowCommand::SetStyle { id, style: Some(s) }], cx) {
                return None;
            }
        }
        let Some(command) = self.table_of_figures_command(before, replace, label) else {
            return if styled { Some(before) } else { None };
        };
        let first_id = match &command {
            FlowCommand::SetToc { entries, .. } => entries.first().map(|e| e.id),
            _ => None,
        };
        let ok = if styled {
            self.document.amend_last(vec![Command::Flow(command)]).is_ok()
        } else {
            self.run_flow(vec![command], cx)
        };
        if ok { first_id } else { None }
    }

    /// 目次の更新: the entries made again from the headings or figures or custom styles (with `levels`,
    /// or the table's own).
    pub(crate) fn update_toc(&mut self, levels: Option<(u32, u32)>, cx: &mut Context<Self>) {
        let caret_para = self.text_selection().map(|(_, c)| c.para);
        let Some((first, kind)) = caret_para.and_then(|p| self.toc_start_kind(Some(p))).or_else(|| self.toc_start_kind(None)) else { return };
        let caret = self.text_selection();
        match kind {
            TocType::Figures(label) => {
                if let Some(new_first) = self.run_table_of_figures(first, true, &label, cx) {
                    self.relayout_if_changed();
                    if let Some(command) = self.table_of_figures_command(new_first, true, &label) {
                        let _ = self.document.amend_last(vec![Command::Flow(command)]);
                    }
                }
            }
            TocType::Headings(own) => {
                let instr = flow_command::TocInstruction {
                    heading_levels: Some(levels.unwrap_or(own)),
                    ..Default::default()
                };
                if self.run_toc(first, true, &instr, cx) {
                    self.relayout_if_changed();
                    if let Some((first, _)) = self.toc_start_kind(None) {
                        if let Some(command) = self.toc_command(first, true, &instr) {
                            let _ = self.document.amend_last(vec![Command::Flow(command)]);
                        }
                    }
                }
            }
            TocType::Custom(mut instr) => {
                if let Some(lvls) = levels {
                    instr.heading_levels = Some(lvls);
                }
                if self.run_toc(first, true, &instr, cx) {
                    self.relayout_if_changed();
                    if let Some((first, _)) = self.toc_start_kind(None) {
                        if let Some(command) = self.toc_command(first, true, &instr) {
                            let _ = self.document.amend_last(vec![Command::Flow(command)]);
                        }
                    }
                }
            }
        }
        // The caret stays where it was if its paragraph does.
        if let Some((_, c)) = caret
            && self.flow().is_some_and(|d| flow_command::paragraph(d, c.para).is_none())
            && let Some(start) = self.document_edge(false)
        {
            self.set_caret(start, false, cx);
        }
        cx.notify();
    }

    /// 目次をすべて更新する (Update entire table):
    /// 見出しや図表の最新構成から目次全体を再構築する（update_toc と同等）。
    #[allow(dead_code)]
    pub(crate) fn update_toc_entire_table(&mut self, levels: Option<(u32, u32)>, cx: &mut Context<Self>) {
        self.update_toc(levels, cx);
    }

    /// ページ番号だけを更新する (Update page numbers only):
    /// 目次の各エントリーのテキストや構成は変更せず、各項目のページ番号 (PageRef / PAGEREF) のみ
    /// 最新のレイアウトに基づいて更新する（1ステップの Undo/Redo）。
    pub(crate) fn update_toc_page_numbers_only(&mut self, cx: &mut Context<Self>) -> bool {
        let caret_para = self.text_selection().map(|(_, c)| c.para);
        let Some((first, _kind)) = caret_para.and_then(|p| self.toc_start_kind(Some(p))).or_else(|| self.toc_start_kind(None)) else {
            return false;
        };
        let Some(doc) = self.flow() else { return false };
        let Some((container, start_idx)) = flow_command::locate(doc, first) else { return false };
        if container != Container::Body {
            return false;
        }

        // 目次の範囲 (start_idx から field_end まで)
        let mut end_idx = start_idx;
        for i in start_idx..doc.body.len() {
            if let Block::Paragraph(p) = &doc.body[i] {
                end_idx = i;
                if p.field_end {
                    break;
                }
            } else {
                break;
            }
        }

        let lines = self.flow_lines();
        let first_page = doc.page.first_page;
        let page_of = |id: u32| lines.iter().find(|l| l.para == id).map(|l| first_page + l.page as u32);

        let mut updated_blocks = Vec::new();
        let mut any_changed = false;

        for i in start_idx..=end_idx {
            let Block::Paragraph(p) = &doc.body[i] else {
                updated_blocks.push(doc.body[i].clone());
                continue;
            };
            let mut p_updated = p.clone();
            let mut changed = false;
            for run in &mut p_updated.runs {
                match &mut run.kind {
                    FlowRunKind::PageRef { bookmark, shown } => {
                        if let Some(target_id) = find_bookmark(doc, bookmark) {
                            if let Some(pg) = page_of(target_id) {
                                let new_shown = pg.to_string();
                                if *shown != new_shown {
                                    *shown = new_shown;
                                    changed = true;
                                }
                            }
                        }
                    }
                    FlowRunKind::Field { instruction, shown } => {
                        let words = flow_command::field_words(instruction);
                        if words.first().is_some_and(|w| w.eq_ignore_ascii_case("PAGEREF")) {
                            if let Some(bookmark) = words.get(1) {
                                if let Some(target_id) = find_bookmark(doc, bookmark) {
                                    if let Some(pg) = page_of(target_id) {
                                        let new_shown = pg.to_string();
                                        if *shown != new_shown {
                                            *shown = new_shown;
                                            changed = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            if changed {
                any_changed = true;
            }
            updated_blocks.push(Block::Paragraph(p_updated));
        }

        if any_changed {
            let count = end_idx - start_idx + 1;
            let command = FlowCommand::ReplaceBlocks {
                container: Container::Body,
                index: start_idx,
                remove: count,
                blocks: updated_blocks,
            };
            let ok = self.run_flow(vec![command], cx);
            if ok {
                self.relayout_if_changed();
                cx.notify();
            }
            ok
        } else {
            false
        }
    }

    /// Follow the link at `pos` to its bookmark (the caret to the start of
    /// its paragraph, scrolled into view): ⌘-click. Whether it went.
    /// Whether ⌘-clicking at `pos` follows a link to a bookmark.
    pub(crate) fn link_at_position(&self, pos: FlowPos) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut at = 0;
        for run in &p.runs {
            let len = run.len();
            if pos.offset >= at && pos.offset < at + len.max(1) {
                return matches!(&run.link, Some(ibis_doc_model::Link::Anchor(name)) if find_bookmark(doc, name).is_some());
            }
            at += len;
        }
        false
    }

    pub(crate) fn follow_link(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut at = 0;
        let mut link = None;
        for run in &p.runs {
            let len = run.len();
            if pos.offset >= at && pos.offset < at + len.max(1) {
                link = run.link.clone();
                break;
            }
            at += len;
        }
        let Some(ibis_doc_model::Link::Anchor(name)) = link else { return false };
        let Some(target) = find_bookmark(doc, &name) else { return false };
        self.jump_to(target, cx);
        true
    }

    /// The caret to the start of paragraph `id`, scrolled to the top.
    pub(crate) fn jump_to(&mut self, id: u32, cx: &mut Context<Self>) {
        self.set_caret(FlowPos::new(id, 0), false, cx);
        self.scroll_to_caret(cx);
    }

    /// The headings (level, text, paragraph) for the outline.
    pub(crate) fn outline(&self) -> Vec<(u32, String, u32)> {
        let Some(doc) = self.flow() else { return Vec::new() };
        doc.body
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph(p) => flow_command::heading_level(&doc.styles, p).map(|l| (l, p.text().chars().filter(|c| !matches!(c, '\t' | '\u{b}' | '\u{c}' | '\u{e}' | '\u{fffc}')).collect::<String>(), p.id)),
                _ => None,
            })
            .filter(|(_, t, _)| !t.trim().is_empty())
            .collect()
    }

    /// 編集記号の表示／非表示をトグル切替する（¶ボタン）。
    pub(crate) fn toggle_formatting_marks(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_open = !self.formatting_marks_open;
        let show_hidden = self.formatting_marks_options.is_hidden_text_visible(self.formatting_marks_open);
        super::flow_layout::set_show_hidden_text(show_hidden);
        self.relayout_now(cx);
        cx.notify();
    }

    /// 垂直ルーラーの表示／非表示をトグル切替する (DOCX-VIEW-012).
    pub(crate) fn toggle_vertical_ruler(&mut self, cx: &mut Context<Self>) {
        self.vertical_ruler_open = !self.vertical_ruler_open;
        cx.notify();
    }

    /// ルーラーの測定単位を取得する (DOCX-VIEW-013).
    pub(crate) fn ruler_unit(&self) -> RulerUnit {
        self.ruler_unit
    }

    /// ルーラーの測定単位を設定する (DOCX-VIEW-013).
    pub(crate) fn set_ruler_unit(&mut self, unit: RulerUnit, cx: &mut Context<Self>) {
        self.ruler_unit = unit;
        cx.notify();
    }

    /// ルーラーの測定単位を順送りで切り替える (DOCX-VIEW-013).
    pub(crate) fn cycle_ruler_unit(&mut self, cx: &mut Context<Self>) {
        self.ruler_unit = self.ruler_unit.next();
        cx.notify();
    }

    /// ページ間の余白の表示／非表示を取得する (DOCX-VIEW-017).
    pub(crate) fn show_white_space(&self) -> bool {
        self.show_white_space
    }

    /// ページ間の余白の表示／非表示をトグル切替する (DOCX-VIEW-017).
    pub(crate) fn toggle_white_space(&mut self, cx: &mut Context<Self>) {
        self.show_white_space = !self.show_white_space;
        cx.notify();
    }

    /// ページ間の余白の表示／非表示を設定する (DOCX-VIEW-017).
    pub(crate) fn set_show_white_space(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_white_space = show;
        cx.notify();
    }

    /// 現在の表示設定に基づくページ間隔 (pixels) を取得する (DOCX-VIEW-017, DOCX-VIEW-002, DOCX-VIEW-003).
    pub(crate) fn page_gap(&self) -> f32 {
        if self.is_web_layout_mode() || self.is_draft_mode() {
            super::doc_ui::COLLAPSED_PAGE_GAP
        } else if self.show_white_space {
            super::doc_ui::PAGE_GAP
        } else {
            super::doc_ui::COLLAPSED_PAGE_GAP
        }
    }

    /// 1ページ表示のズーム倍率を計算する (DOCX-VIEW-009).
    /// 文書のページ全体（幅と高さ両方）がワークスペース領域内に収まる倍率。
    pub(crate) fn calc_zoom_one_page(&self) -> Option<f32> {
        let pages = self.page_sizes();
        if pages.is_empty() {
            return None;
        }
        let current = self.visible_page().min(pages.len().saturating_sub(1));
        let page_size = pages.get(current).copied().unwrap_or(pages[0]);
        let scale_100 = zoom_scale(1.0);
        let px_at_100_w = page_size.width as f32 * scale_100;
        let px_at_100_h = page_size.height as f32 * scale_100;
        if px_at_100_w <= 0.0 || px_at_100_h <= 0.0 {
            return None;
        }
        let (view_w, view_h) = self.workspace_bounds.map_or((1000.0, 700.0), |ws| (f32::from(ws.size.width), f32::from(ws.size.height)));
        let gap = self.page_gap();
        let avail_w = (view_w - 2.0 * super::doc_ui::PAGE_GAP).max(40.0);
        let avail_h = (view_h - 2.0 * gap).max(40.0);
        let zoom_w = avail_w / px_at_100_w;
        let zoom_h = avail_h / px_at_100_h;
        let zoom = zoom_w.min(zoom_h).clamp(MIN_ZOOM, MAX_ZOOM);
        Some(((zoom * 100.0).round() / 100.0).clamp(MIN_ZOOM, MAX_ZOOM))
    }

    /// 複数ページ（2ページ横並び・縦並び）表示のズーム倍率を計算する (DOCX-VIEW-009).
    /// 複数ページがワークスペース領域内に収まる倍率。
    pub(crate) fn calc_zoom_multiple_pages(&self) -> Option<f32> {
        let pages = self.page_sizes();
        if pages.is_empty() {
            return None;
        }
        let current = self.visible_page().min(pages.len().saturating_sub(1));
        let page_size = pages.get(current).copied().unwrap_or(pages[0]);
        let scale_100 = zoom_scale(1.0);
        let px_at_100_w = page_size.width as f32 * scale_100;
        let px_at_100_h = page_size.height as f32 * scale_100;
        if px_at_100_w <= 0.0 || px_at_100_h <= 0.0 {
            return None;
        }
        let (view_w, view_h) = self.workspace_bounds.map_or((1000.0, 700.0), |ws| (f32::from(ws.size.width), f32::from(ws.size.height)));
        let gap = self.page_gap();
        let avail_w = (view_w - 2.0 * super::doc_ui::PAGE_GAP).max(40.0);
        let avail_h = (view_h - gap).max(40.0);
        let zoom_w = avail_w / px_at_100_w;
        let zoom_h = avail_h / (2.0 * px_at_100_h);
        let one_page_zoom = self.calc_zoom_one_page().unwrap_or(1.0);
        let zoom = zoom_w.min(zoom_h).min(one_page_zoom * 0.7).clamp(MIN_ZOOM, MAX_ZOOM);
        Some(((zoom * 100.0).round() / 100.0).clamp(MIN_ZOOM, MAX_ZOOM))
    }

    /// 現在の表示倍率が「1ページ」表示に該当するか判定する (DOCX-VIEW-009).
    pub(crate) fn is_zoom_one_page(&self) -> bool {
        match (self.zoom, self.calc_zoom_one_page()) {
            (Some(z), Some(target)) => (z - target).abs() < 0.015,
            _ => false,
        }
    }

    /// 現在の表示倍率が「複数ページ」表示に該当するか判定する (DOCX-VIEW-009).
    pub(crate) fn is_zoom_multiple_pages(&self) -> bool {
        match (self.zoom, self.calc_zoom_multiple_pages()) {
            (Some(z), Some(target)) => (z - target).abs() < 0.015,
            _ => false,
        }
    }

    /// 現在の表示倍率が「ページ幅」表示に該当するか判定する (DOCX-VIEW-009).
    pub(crate) fn is_zoom_page_width(&self) -> bool {
        self.zoom.is_none()
    }

    /// 現在の表示倍率が「100%」表示に該当するか判定する (DOCX-VIEW-009).
    pub(crate) fn is_zoom_100(&self) -> bool {
        self.zoom.is_some_and(|z| (z - 1.0).abs() < 0.01)
    }

    /// 1ページ全体が収まるようにズーム倍率を設定する (DOCX-VIEW-009).
    pub(crate) fn zoom_one_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.calc_zoom_one_page() {
            let current = self.visible_page();
            self.set_zoom(Some(target), window, cx);
            self.scroll_to_page(current, cx);
        }
    }

    /// 複数ページが収まるようにズーム倍率を設定する (DOCX-VIEW-009).
    pub(crate) fn zoom_multiple_pages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.calc_zoom_multiple_pages() {
            let current = self.visible_page();
            self.set_zoom(Some(target), window, cx);
            self.scroll_to_page(current, cx);
        }
    }

    /// ページ幅に合わせてズーム倍率を設定する (DOCX-VIEW-009).
    pub(crate) fn zoom_page_width(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(None, window, cx);
    }

    /// 100% ズーム倍率を設定する (DOCX-VIEW-009).
    pub(crate) fn zoom_100_percent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(Some(1.0), window, cx);
    }

    /// スペース記号の表示をトグル切替 (DOCX-OPTIONS-008).
    pub(crate) fn toggle_show_spaces(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_options.show_spaces = !self.formatting_marks_options.show_spaces;
        cx.notify();
    }

    /// タブ記号の表示をトグル切替 (DOCX-OPTIONS-008).
    pub(crate) fn toggle_show_tabs(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_options.show_tabs = !self.formatting_marks_options.show_tabs;
        cx.notify();
    }

    /// 段落記号の表示をトグル切替 (DOCX-OPTIONS-008).
    pub(crate) fn toggle_show_paragraph_marks(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_options.show_paragraph_marks = !self.formatting_marks_options.show_paragraph_marks;
        cx.notify();
    }

    /// 改行記号の表示をトグル切替 (DOCX-OPTIONS-008).
    pub(crate) fn toggle_show_line_breaks(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_options.show_line_breaks = !self.formatting_marks_options.show_line_breaks;
        cx.notify();
    }

    /// 隠し文字の常時表示をトグル切替 (DOCX-OPTIONS-008).
    pub(crate) fn toggle_show_hidden_text(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_options.show_hidden_text = !self.formatting_marks_options.show_hidden_text;
        let show_hidden = self.formatting_marks_options.is_hidden_text_visible(self.formatting_marks_open);
        super::flow_layout::set_show_hidden_text(show_hidden);
        self.relayout_now(cx);
        cx.notify();
    }

    /// 文書が最終版としてマークされているか（Word の「最終版にする」）。
    pub(crate) fn is_mark_as_final(&self) -> bool {
        self.flow().is_some_and(|d| d.mark_as_final)
    }

    /// 読み取り専用を推奨する設定になっているか（Word の「読み取り専用を推奨する」）。
    pub(crate) fn is_read_only_recommended(&self) -> bool {
        self.flow().is_some_and(|d| d.read_only_recommended)
    }

    /// 読み取り専用推奨の情報バーで「編集を有効にする」を押したか。
    pub(crate) fn is_read_only_dismissed(&self) -> bool {
        self.doc_state().is_some_and(|s| s.read_only_dismissed)
    }

    /// 読み取り専用推奨の情報バーを閉じて編集を有効にする。
    pub(crate) fn dismiss_read_only(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.read_only_dismissed = true;
            cx.notify();
        }
    }

    /// 最終版としてマークを設定・解除する。
    pub(crate) fn set_mark_as_final(&mut self, mark: bool, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetMarkAsFinal(mark)], cx);
    }

    /// 最終版としてマークをトグル切替する。
    pub(crate) fn toggle_mark_as_final(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_mark_as_final();
        self.set_mark_as_final(next, cx);
    }

    /// 最終版マークを解除して編集を可能にする（Word の「それでも編集する」）。
    pub(crate) fn edit_anyway(&mut self, cx: &mut Context<Self>) {
        self.set_mark_as_final(false, cx);
    }

    /// 読み取り専用を推奨を設定・解除する。
    pub(crate) fn set_read_only_recommended(&mut self, ro: bool, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetReadOnlyRecommended(ro)], cx);
    }

    /// 読み取り専用を推奨をトグル切替する。
    pub(crate) fn toggle_read_only_recommended(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_read_only_recommended();
        self.set_read_only_recommended(next, cx);
    }

    /// 文書の編集制限（保護モード）を取得。
    pub(crate) fn document_protection(&self) -> DocumentProtection {
        self.flow().map_or(DocumentProtection::None, |d| d.protection)
    }

    /// 文書が読み取り専用で保護されているか。
    pub(crate) fn is_protection_read_only(&self) -> bool {
        self.document_protection() == DocumentProtection::ReadOnly
    }

    /// 文書がコメントのみ許可で保護されているか。
    pub(crate) fn is_protection_comments(&self) -> bool {
        self.document_protection() == DocumentProtection::Comments
    }

    /// 文書がフォーム入力のみ許可で保護されているか。
    pub(crate) fn is_protection_forms(&self) -> bool {
        self.document_protection() == DocumentProtection::Forms
    }

    /// 文書の書式変更が制限されているか。
    pub(crate) fn is_formatting_protected(&self) -> bool {
        self.flow().is_some_and(|d| d.formatting_protection)
    }

    /// 書式変更の制限を設定する。
    pub(crate) fn set_formatting_protection(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetFormattingProtection(enabled)], cx);
    }

    /// 編集制限（保護モード）を設定する。
    pub(crate) fn set_document_protection(&mut self, protection: DocumentProtection, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetDocumentProtection(protection)], cx);
    }

    /// 編集制限を解除する。
    pub(crate) fn stop_protection(&mut self, cx: &mut Context<Self>) {
        let mut commands = Vec::new();
        if self.document_protection() != DocumentProtection::None {
            commands.push(FlowCommand::SetDocumentProtection(DocumentProtection::None));
        }
        if self.is_formatting_protected() {
            commands.push(FlowCommand::SetFormattingProtection(false));
        }
        if !commands.is_empty() {
            self.run_flow(commands, cx);
        }
    }

    /// Word 標準の情報バー（編集制限 / 最終版 / 読み取り専用の推奨 / 閲覧モード）。
    pub(crate) fn render_docx_infobar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.is_protection_read_only() {
            Some(
                div()
                    .id("docx-infobar-protected-readonly")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x323130))
                                    .child(crate::shell::i18n::text("docx.infobar.protect.readonly.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x605e5c))
                                    .child(crate::shell::i18n::text("docx.infobar.protect.readonly.message")),
                            ),
                    )
                    .child(
                        div()
                            .id("infobar-btn-stop-protection")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("docx.infobar.protect.stop"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stop_protection(cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else if self.is_protection_comments() {
            Some(
                div()
                    .id("docx-infobar-protected-comments")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x323130))
                                    .child(crate::shell::i18n::text("docx.infobar.protect.comments.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x605e5c))
                                    .child(crate::shell::i18n::text("docx.infobar.protect.comments.message")),
                            ),
                    )
                    .child(
                        div()
                            .id("infobar-btn-stop-protection")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("docx.infobar.protect.stop"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stop_protection(cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else if self.is_protection_forms() {
            Some(
                div()
                    .id("docx-infobar-protected-forms")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x323130))
                                    .child(crate::shell::i18n::text("docx.infobar.protect.forms.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x605e5c))
                                    .child(crate::shell::i18n::text("docx.infobar.protect.forms.message")),
                            ),
                    )
                    .child(
                        div()
                            .id("infobar-btn-stop-protection")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("docx.infobar.protect.stop"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stop_protection(cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else if self.is_formatting_protected() {
            Some(
                div()
                    .id("docx-infobar-protected-formatting")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x323130))
                                    .child(crate::shell::i18n::text("docx.infobar.protect.formatting.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x605e5c))
                                    .child(crate::shell::i18n::text("docx.infobar.protect.formatting.message")),
                            ),
                    )
                    .child(
                        div()
                            .id("infobar-btn-stop-protection")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("docx.infobar.protect.stop"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stop_protection(cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else if self.is_mark_as_final() {
            Some(
                div()
                    .id("docx-infobar-final")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x323130))
                                    .child(crate::shell::i18n::text("docx.infobar.final.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x605e5c))
                                    .child(crate::shell::i18n::text("docx.infobar.final.message")),
                            ),
                    )
                    .child(
                        div()
                            .id("infobar-btn-edit-anyway")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("docx.infobar.final.edit-anyway"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit_anyway(cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else if self.is_read_only_recommended() && !self.is_read_only_dismissed() {
            Some(
                div()
                    .id("docx-infobar-readonly")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x323130))
                                    .child(crate::shell::i18n::text("docx.infobar.readonly.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x605e5c))
                                    .child(crate::shell::i18n::text("docx.infobar.readonly.message")),
                            ),
                    )
                    .child(
                        div()
                            .id("infobar-btn-enable-editing")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("docx.infobar.readonly.enable-editing"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_read_only(cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else if self.is_read_mode() {
            Some(
                div()
                    .id("docx-infobar-readmode")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x323130))
                                    .child(crate::shell::i18n::text("docx.infobar.readmode.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x605e5c))
                                    .child(crate::shell::i18n::text("docx.infobar.readmode.message")),
                            ),
                    )
                    .child(
                        div()
                            .id("infobar-btn-exit-readmode")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("docx.infobar.readmode.exit"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_read_mode(false, window, cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else if self.is_viewing_mode() {
            Some(
                div()
                    .id("docx-infobar-viewing")
                    .w_full()
                    .bg(rgb(0xfff4ce))
                    .border_b_1()
                    .border_color(rgb(0xedebe9))
                    .px(px(16.))
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x323130))
                                    .child(crate::shell::i18n::text("docx.infobar.viewing.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x605e5c))
                                    .child(crate::shell::i18n::text("docx.infobar.viewing.message")),
                            ),
                    )
                    .child(
                        div()
                            .id("infobar-btn-switch-to-editing")
                            .px(px(12.))
                            .py(px(4.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(0x8a8886))
                            .bg(rgb(0xffffff))
                            .text_size(px(12.))
                            .text_color(rgb(0x323130))
                            .cursor_pointer()
                            .hover(|h| h.bg(rgb(0xf3f2f1)))
                            .child(crate::shell::i18n::text("docx.infobar.viewing.edit"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Editing, window, cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else {
            None
        }
    }

    /// Escape キー押下時の処理 (Focus Mode 解除 > Read Mode / Viewing Mode 解除 > 書式のコピー/貼り付け解除)。
    pub(crate) fn handle_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_focus_mode() {
            self.set_focus_mode(false, window, cx);
            return;
        }
        if self.is_read_mode() || self.is_viewing_mode() {
            self.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Editing, window, cx);
            return;
        }
        self.cancel_doc_format_brush(cx);
    }

    /// 変更履歴の表示モードを取得。
    pub(crate) fn markup_display_mode(&self) -> crate::docx::flow_layout::MarkupDisplayMode {
        crate::docx::flow_layout::markup_display_mode()
    }

    /// 変更履歴の表示モードを設定（シンプルな変更履歴・すべての変更履歴・変更履歴なし・元の文書）。
    pub(crate) fn set_markup_display_mode(&mut self, mode: crate::docx::flow_layout::MarkupDisplayMode, cx: &mut Context<Self>) {
        crate::docx::flow_layout::set_markup_display_mode(mode);
        self.relayout_now(cx);
        cx.notify();
    }

    /// 上書き入力モードかどうか (Word's Overtype mode).
    pub(crate) fn is_overtype(&self) -> bool {
        self.doc_state().is_some_and(|s| s.overtype)
    }

    /// 上書き入力モードをトグル切替。
    pub(crate) fn toggle_overtype(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.overtype = !state.overtype;
        }
        cx.notify();
    }

    /// 上書き入力モードを設定。
    pub(crate) fn set_overtype(&mut self, overtype: bool, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.overtype = overtype;
        }
        cx.notify();
    }

    /// 文書内の変更履歴およびコメントに記録されているすべての校閲者（作成者 author）一覧を収集（DOCX-REVIEW-005）。
    pub(crate) fn revision_authors(&self) -> Vec<String> {
        let Some(doc) = self.flow() else { return Vec::new() };
        let mut authors = std::collections::BTreeSet::new();

        fn collect_from_blocks(blocks: &[ibis_doc_model::Block], authors: &mut std::collections::BTreeSet<String>) {
            for b in blocks {
                match b {
                    ibis_doc_model::Block::Paragraph(p) => {
                        for r in &p.runs {
                            if let Some(rev) = &r.revision {
                                if !rev.author.is_empty() {
                                    authors.insert(rev.author.clone());
                                }
                            }
                        }
                    }
                    ibis_doc_model::Block::Table(t) => {
                        for row in &t.rows {
                            for cell in &row.cells {
                                collect_from_blocks(&cell.blocks, authors);
                            }
                        }
                    }
                }
            }
        }

        collect_from_blocks(&doc.body, &mut authors);
        collect_from_blocks(&doc.header, &mut authors);
        collect_from_blocks(&doc.footer, &mut authors);
        collect_from_blocks(&doc.header_first, &mut authors);
        collect_from_blocks(&doc.footer_first, &mut authors);
        collect_from_blocks(&doc.header_even, &mut authors);
        collect_from_blocks(&doc.footer_even, &mut authors);
        for note in doc.footnotes.iter().chain(doc.endnotes.iter()) {
            collect_from_blocks(&note.blocks, &mut authors);
        }
        for comment in &doc.comments {
            if !comment.author.is_empty() {
                authors.insert(comment.author.clone());
            }
        }
        authors.into_iter().collect()
    }

    /// すべての校閲者が表示対象か（DOCX-REVIEW-005）。
    pub(crate) fn is_all_reviewers(&self) -> bool {
        self.reviewer_filter.is_none()
    }

    /// 特定の校閲者が表示対象か（DOCX-REVIEW-005）。
    pub(crate) fn is_reviewer_shown(&self, author: &str) -> bool {
        self.reviewer_filter.as_ref().map_or(true, |set| set.contains(author))
    }

    /// 「すべての校閲者」の表示・非表示を一括設定（DOCX-REVIEW-005）。
    pub(crate) fn set_all_reviewers(&mut self, on: bool, cx: &mut Context<Self>) {
        if on {
            self.reviewer_filter = None;
        } else {
            self.reviewer_filter = Some(std::collections::HashSet::new());
        }
        crate::docx::flow_layout::set_shown_reviewers(self.reviewer_filter.clone());
        cx.notify();
    }

    /// 特定の校閲者の表示・非表示をトグル切替（DOCX-REVIEW-005）。
    pub(crate) fn toggle_reviewer(&mut self, author: &str, cx: &mut Context<Self>) {
        let all_authors = self.revision_authors();
        let mut set = match &self.reviewer_filter {
            Some(s) => s.clone(),
            None => all_authors.iter().cloned().collect(),
        };
        if set.contains(author) {
            set.remove(author);
        } else {
            set.insert(author.to_string());
        }
        if set.len() == all_authors.len() {
            self.reviewer_filter = None;
        } else {
            self.reviewer_filter = Some(set);
        }
        crate::docx::flow_layout::set_shown_reviewers(self.reviewer_filter.clone());
        cx.notify();
    }
}


/// The paragraph bookmark `name` starts in.
fn find_bookmark(doc: &FlowDocument, name: &str) -> Option<u32> {
    fn walk(blocks: &[Block], name: &str) -> Option<u32> {
        blocks.iter().find_map(|b| match b {
            Block::Paragraph(p) => p.bookmarks.iter().any(|b| b == name).then_some(p.id),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).find_map(|c| walk(&c.blocks, name)),
        })
    }
    walk(&doc.body, name)
}
