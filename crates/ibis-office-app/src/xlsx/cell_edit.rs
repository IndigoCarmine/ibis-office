// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Typing into a cell, as in Excel: typing on a selected cell replaces
//! its content ("Enter" mode, where the arrows finish the entry and move),
//! F2 or a double-click edits what is there ("Edit" mode, where ← and →
//! move the caret). Enter, Tab and the arrows finish the entry and move;
//! Esc gives it up. Japanese input goes through the shell's input hooks
//! (`shell/input.rs`), which call the functions here.
//!
//! Finishing an entry reads it as Excel does (`ibis_calc::input`) and
//! makes it one editing step: the value or formula, and the number format
//! Excel gives a General cell for what was typed (`1,234`, `50%`,
//! `2026/9/30`).

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::*;
use ibis_calc::input::{Typed, parse_input};
use ibis_doc_model::{CellRun, CellEdit as ModelEdit, CellInput, CellRange, CellValue, Command, SheetCommand, SlideId, StyleChange};

use crate::shell::editor::SlideEditor;

/// How the arrows act while typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditMode {
    /// Typing replaced the content: the arrows finish the entry and move.
    Enter,
    /// F2 or a double-click: ← and → move the caret.
    Edit,
}

/// A cell being typed into.
#[derive(Clone, Debug)]
pub(crate) struct CellEdit {
    pub(crate) slide: SlideId,
    pub(crate) column: u32,
    pub(crate) row: u32,
    pub(crate) text: String,
    /// The caret, in UTF-16 units.
    pub(crate) caret: usize,
    /// Text the input method is composing, in UTF-16 units.
    pub(crate) marked: Option<Range<usize>>,
    pub(crate) mode: EditMode,
    /// The other end of the selected text (UTF-16 units), the caret being one.
    pub(crate) anchor: Option<usize>,
    /// The text's parts in their own fonts (rich text), their texts together `text`.
    pub(crate) runs: Option<Vec<CellRun>>,
    /// A reference put in by clicking a cell: its byte span in `text` and
    /// the cell clicked (a drag makes it a range from there).
    pub(crate) pointed: Option<(usize, usize, (u32, u32))>,
    /// Where the text is on screen (set when painted), for the input method.
    pub(crate) rect: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Pixels per point of the text on screen (set when painted), to find the
    /// character under the pointer.
    pub(crate) pixels_per_point: Rc<Cell<f32>>,
    /// Pressed on the text and dragging: the position (UTF-16 units) it began at.
    pub(crate) dragging: Option<usize>,
    /// Whether the press being dragged began in the formula bar (not in the cell).
    pub(crate) dragging_in_bar: bool,
    /// Font changes asked for with the caret alone (⌘B…): they go to the text typed next.
    pub(crate) pending: Option<PendingFormat>,
}

/// Font changes for the next typed text (Excel's ⌘B with the caret alone),
/// with the cell's style they are made against.
#[derive(Clone, Debug)]
pub(crate) struct PendingFormat {
    pub(crate) changes: Vec<StyleChange>,
    pub(crate) style: ibis_doc_model::CellStyle,
}

/// `runs` with the bytes `b0..b1` of their text replaced by `text`, which
/// takes the font of the part it lands in (of the one before it at a seam).
pub(crate) fn runs_replace(runs: &mut Vec<CellRun>, b0: usize, b1: usize, text: &str) {
    let mut out: Vec<CellRun> = Vec::new();
    let mut at = 0usize;
    let mut inserted = false;
    let mut last_format: Option<CellRun> = None;
    for run in runs.iter() {
        let (start, end) = (at, at + run.text.len());
        at = end;
        let format = CellRun { text: String::new(), ..run.clone() };
        let before = &run.text[..b0.clamp(start, end) - start];
        let after = &run.text[b1.clamp(start, end) - start..];
        if !before.is_empty() {
            out.push(CellRun { text: before.to_string(), ..format.clone() });
        }
        // The new text goes where the replaced range starts: in this run, or right after the one it ends.
        if !inserted && b0 >= start && (b0 < end || (b0 == end && std::ptr::eq(run, runs.last().unwrap()))) {
            if !text.is_empty() {
                out.push(CellRun { text: text.to_string(), ..format.clone() });
            }
            inserted = true;
        }
        if !after.is_empty() && b1 < end {
            out.push(CellRun { text: after.to_string(), ..format.clone() });
        }
        last_format = Some(format);
    }
    if !inserted && !text.is_empty() {
        out.push(CellRun { text: text.to_string(), ..last_format.unwrap_or_default() });
    }
    *runs = merge_runs(out);
}

/// Neighbouring parts of the same font as one; empty parts gone.
pub(crate) fn merge_runs(runs: Vec<CellRun>) -> Vec<CellRun> {
    let mut out: Vec<CellRun> = Vec::new();
    for run in runs.into_iter().filter(|r| !r.text.is_empty()) {
        match out.last_mut() {
            Some(last) if CellRun { text: String::new(), ..last.clone() } == (CellRun { text: String::new(), ..run.clone() }) => last.text.push_str(&run.text),
            _ => out.push(run),
        }
    }
    out
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The byte offset of UTF-16 position `u` in `s`.
fn byte_at(s: &str, u: usize) -> usize {
    let mut count = 0;
    for (i, c) in s.char_indices() {
        if count >= u {
            return i;
        }
        count += c.len_utf16();
    }
    s.len()
}

impl CellEdit {
    /// The caret as a byte offset into `text`.
    pub(crate) fn caret_byte(&self) -> usize {
        byte_at(&self.text, self.caret)
    }

    /// The selected text, in UTF-16 units (none when the caret is alone).
    pub(crate) fn selection(&self) -> Option<Range<usize>> {
        let a = self.anchor?;
        let (s, e) = (a.min(self.caret), a.max(self.caret));
        (s != e).then_some(s..e)
    }

    /// The text in three parts for the formula bar: before the selection (or
    /// the caret), the selected text, and after it; and whether the caret
    /// is at the start of the selected text (else at its end).
    pub(crate) fn bar_parts(&self) -> (String, String, String, bool) {
        match self.selection() {
            Some(r) => {
                let (b0, b1) = (byte_at(&self.text, r.start), byte_at(&self.text, r.end));
                (self.text[..b0].to_string(), self.text[b0..b1].to_string(), self.text[b1..].to_string(), self.caret < r.end)
            }
            None => {
                let b = self.caret_byte();
                (self.text[..b].to_string(), String::new(), self.text[b..].to_string(), false)
            }
        }
    }

    fn replace(&mut self, range: Range<usize>, text: &str) -> Range<usize> {
        let len = utf16_len(&self.text);
        let (start, end) = (range.start.min(len), range.end.min(len).max(range.start.min(len)));
        let (b0, b1) = (byte_at(&self.text, start), byte_at(&self.text, end));
        let pending = if text.is_empty() { None } else { self.pending.take() };
        if let Some(p) = &pending {
            if self.text.is_empty() {
                self.runs = Some(vec![whole_run("", &p.style)]);
            } else if self.runs.is_none() {
                self.runs = Some(vec![whole_run(&self.text, &p.style)]);
            }
        }
        if let Some(runs) = self.runs.as_mut() {
            if self.text.is_empty() && pending.is_some() {
                // Nothing to take a place in: the first run holds the new text.
                if let Some(first) = runs.first_mut() {
                    first.text = text.to_string();
                }
            } else {
                runs_replace(runs, b0, b1, text);
            }
            if let Some(p) = &pending {
                for change in &p.changes {
                    let taken = std::mem::take(runs);
                    *runs = apply_to_runs(taken, b0, b0 + text.len(), change, &p.style);
                }
            }
        }
        self.anchor = None;
        self.text.replace_range(b0..b1, text);
        let inserted = start..start + utf16_len(text);
        self.caret = inserted.end;
        inserted
    }
}

/// The bytes of the word at byte `at` of `text`: a run of letters and
/// digits, or of the same kind of other character (a double-click selects it).
pub(crate) fn word_range(text: &str, at: usize) -> (usize, usize) {
    let kind = |c: char| -> u8 {
        if c.is_whitespace() {
            0
        } else if c.is_alphanumeric() && c.is_ascii() || c == '_' || c == '.' {
            1
        } else if c.is_alphanumeric() {
            // Kana, kanji and so on: a run of the same script.
            match c {
                '\u{3040}'..='\u{309f}' => 2,
                '\u{30a0}'..='\u{30ff}' | '\u{ff66}'..='\u{ff9f}' => 3,
                _ => 4,
            }
        } else {
            5
        }
    };
    let at = at.min(text.len());
    let next = text[at..].chars().next();
    let prev = text[..at].chars().next_back();
    let Some(c) = next.or(prev) else { return (0, 0) };
    let k = kind(c);
    let mut start = if next.is_some() { at } else { at - c.len_utf8() };
    let mut end = if next.is_some() { at + c.len_utf8() } else { at };
    while let Some(p) = text[..start].chars().next_back().filter(|&p| kind(p) == k) {
        start -= p.len_utf8();
    }
    while let Some(n) = text[end..].chars().next().filter(|&n| kind(n) == k) {
        end += n.len_utf8();
    }
    (start, end)
}

impl SlideEditor {
    /// The caret position (UTF-16 units) the pointer is at in the text being
    /// typed: the boundary between characters nearest to it. `None` before
    /// the text has been painted.
    pub(crate) fn cell_text_index_at(&self, position: Point<Pixels>, cx: &App) -> Option<usize> {
        let edit = self.cell_edit()?;
        let rect = edit.rect.get()?;
        let per_point = edit.pixels_per_point.get();
        if per_point <= 0.0 {
            return None;
        }
        let (_, sheet) = self.current_sheet()?;
        let base = sheet.cell(edit.row, edit.column).map(|c| sheet.styles[c.style as usize].clone()).unwrap_or_default();
        let ts = cx.text_system();
        // The text starts after the border and the padding (2 + 2 px).
        let x = f32::from(position.x - rect.origin.x) - 4.0;
        let mut acc = 0.0f32;
        let mut index = 0usize;
        let mut byte = 0usize;
        let mut run_start = 0usize;
        let mut runs = edit.runs.iter().flatten();
        let mut run = runs.next();
        for c in edit.text.chars() {
            while let Some(r) = run {
                if byte >= run_start + r.text.len() {
                    run_start += r.text.len();
                    run = runs.next();
                } else {
                    break;
                }
            }
            let mut style = base.clone();
            if let Some(r) = run {
                style.bold = r.bold.unwrap_or(style.bold);
                style.italic = r.italic.unwrap_or(style.italic);
                if let Some(size) = r.size {
                    style.font_size = size as _;
                }
                if let Some(f) = &r.font {
                    style.font = Some(f.clone());
                }
            }
            let w = super::sheet_view::text_width_pt(&c.to_string(), &style, &self.resources, ts) * per_point;
            if x < acc + w / 2.0 {
                return Some(index);
            }
            acc += w;
            index += c.len_utf16();
            byte += c.len_utf8();
        }
        Some(index)
    }

    /// The caret position (UTF-16 units) the pointer is at in the formula
    /// bar's text (the window's font at the bar's size; the text starts
    /// 8 px in from the field's edge).
    pub(crate) fn bar_text_index_at(&self, position: Point<Pixels>, window: &Window) -> Option<usize> {
        let edit = self.cell_edit()?;
        let rect = self.sheet_state()?.bar_bounds.get()?;
        let style = window.text_style();
        let size = window.rem_size() * 0.875;
        // The bar shows one line: a line break as a space.
        let shown = edit.text.replace(['\r', '\n'], " ");
        let run = TextRun { len: shown.len(), font: style.font(), color: style.color, background_color: None, underline: None, strikethrough: None };
        let line = window.text_system().shape_line(SharedString::from(shown.clone()), size, &[run], None);
        let byte = line.closest_index_for_x(position.x - rect.origin.x - px(8.)).min(shown.len());
        let byte = (0..=byte).rev().find(|&b| shown.is_char_boundary(b)).unwrap_or(0);
        Some(utf16_len(&shown[..byte]))
    }

    /// A press on the formula bar's text, as `press_in_cell_text` does on the cell's.
    pub(crate) fn press_in_bar_text(&mut self, position: Point<Pixels>, click_count: usize, shift: bool, window: &Window, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.bar_text_index_at(position, window) else { return false };
        self.place_caret(index, click_count, shift, true, cx);
        true
    }

    /// A press on the text being typed: the caret goes there (Shift extends
    /// the selection, a double-click selects the word) and dragging on
    /// selects; typing goes on in Edit mode. False if it could not tell where.
    pub(crate) fn press_in_cell_text(&mut self, position: Point<Pixels>, click_count: usize, shift: bool, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.cell_text_index_at(position, cx) else { return false };
        self.place_caret(index, click_count, shift, false, cx);
        true
    }

    fn place_caret(&mut self, index: usize, click_count: usize, shift: bool, in_bar: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut()) else { return };
        edit.dragging_in_bar = in_bar;
        edit.mode = EditMode::Edit;
        edit.pending = None;
        edit.marked = None;
        if click_count >= 2 {
            let b = byte_at(&edit.text, index);
            let (b0, b1) = word_range(&edit.text, b);
            edit.anchor = Some(utf16_len(&edit.text[..b0]));
            edit.caret = utf16_len(&edit.text[..b1]);
            edit.dragging = None;
        } else if shift {
            edit.anchor = Some(edit.anchor.unwrap_or(edit.caret));
            edit.caret = index;
            edit.dragging = edit.anchor;
        } else {
            edit.anchor = None;
            edit.caret = index;
            edit.dragging = Some(index);
        }
        cx.notify();
    }

    /// The pointer moved with the button down on the text being typed: the
    /// selection runs from where it was pressed to the pointer.
    pub(crate) fn drag_in_cell_text(&mut self, position: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let in_bar = self.cell_edit().is_some_and(|e| e.dragging_in_bar);
        let Some(index) = (if in_bar { self.bar_text_index_at(position, window) } else { self.cell_text_index_at(position, cx) }) else { return };
        if let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut())
            && let Some(from) = edit.dragging
        {
            edit.anchor = Some(from);
            edit.caret = index;
            cx.notify();
        }
    }

    pub(crate) fn cell_edit(&self) -> Option<&CellEdit> {
        self.sheet_state().and_then(|s| s.cell_edit.as_ref())
    }

    fn cell_edit_mut(&mut self) -> Option<&mut CellEdit> {
        self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut())
    }

    /// Whether the keys go to the cells now (a workbook, a cell selected,
    /// no shape being edited, the name box and ジャンプ not in use).
    pub(crate) fn cells_take_typing(&self, window: &Window, cx: &App) -> bool {
        let Some(state) = self.sheet_state() else { return false };
        self.selection.is_none()
            && self.editing.is_none()
            && self.slideshow.is_none()
            && !state.jump.goto_open
            && !state.jump.name_box.read(cx).focus_handle(cx).is_focused(window)
            && state.jump.renaming.is_none()
            && self.current_cell().is_some()
    }

    /// Start typing into the selected cell: with its content (F2), or empty.
    pub(crate) fn begin_cell_edit(&mut self, mode: EditMode, keep_content: bool, cx: &mut Context<Self>) {
        // A locked cell of a protected sheet is not typed into.
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some((column, row)) = self.current_cell() else { return };
        let text = if keep_content {
            self.current_sheet().and_then(|(_, sheet)| sheet.cell(row, column)).map(|c| match &c.formula {
                Some(f) => format!("={f}"),
                None => match &c.value {
                    // A number as typed, not as shown.
                    CellValue::Number(n) => ibis_calc::eval::number_to_text(*n),
                    _ => c.text.clone(),
                },
            })
        } else {
            None
        }
        .unwrap_or_default();
        let caret = utf16_len(&text);
        let slide = self.current_slide;
        // A text constant with parts in their own fonts keeps them while edited.
        let runs = if keep_content {
            self.current_sheet().and_then(|(_, sheet)| sheet.cell(row, column)).filter(|c| c.formula.is_none()).and_then(|c| c.runs.as_ref().map(|r| r.to_vec())).filter(|r| r.iter().map(|x| x.text.as_str()).collect::<String>() == text)
        } else {
            None
        };
        if let Some(state) = self.sheet_state_mut() {
            state.cell_edit = Some(CellEdit { slide, column, row, text, caret, marked: None, mode, anchor: None, runs, pointed: None, rect: Rc::new(Cell::new(None)), pixels_per_point: Rc::new(Cell::new(0.0)), dragging: None, dragging_in_bar: false, pending: None });
        }
        cx.notify();
    }

    /// Give up typing (Esc).
    pub(crate) fn cancel_cell_edit(&mut self, cx: &mut Context<Self>) -> bool {
        let had = self.sheet_state_mut().and_then(|s| s.cell_edit.take()).is_some();
        if had {
            cx.notify();
        }
        had
    }

    /// Finish typing: put what was typed into the cell as one step.
    pub(crate) fn commit_cell_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.take()) else { return };
        if let Some(s) = self.sheet_state_mut() {
            s.last_fill = None;
        }
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let sheet = self.presentation().slide(edit.slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide);
        let (input, format): (CellInput, Option<String>) = if edit.text.is_empty() {
            (CellInput::Clear, None)
        } else {
            match parse_input(&edit.text, date1904) {
                Typed::Formula(f) => {
                    let format = sheet.and_then(|sheet| referenced_format(sheet, &f));
                    (CellInput::Formula(f), format)
                }
                Typed::Value(v, f) => (CellInput::Value(v), f.map(str::to_string)),
            }
        };
        // データの入力規則: a value it refuses stays being typed, with the message.
        let mut warning = None;
        let check_val = match &input {
            CellInput::Value(v) => Some(v),
            CellInput::Clear => Some(&CellValue::Empty),
            _ => None,
        };
        if let Some(value) = check_val {
            match self.check_entry(edit.row, edit.column, value) {
                Err(message) => {
                    self.status = Some(message.into());
                    if let Some(state) = self.sheet_state_mut() {
                        state.cell_edit = Some(edit);
                    }
                    cx.notify();
                    return;
                }
                Ok(w) => warning = w,
            }
        }
        let sheet = self.presentation().slide(edit.slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide);
        // The number format Excel gives what was typed, if the cell has none.
        let general = sheet
            .map(|sheet| sheet.cell(edit.row, edit.column).map_or("General".to_string(), |c| sheet.styles[c.style as usize].number_format.clone()))
            .is_some_and(|f| f == "General");
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide: edit.slide, edits: vec![ModelEdit { row: edit.row, column: edit.column, input: input.clone() }] })];
        let auto_format = matches!(input, CellInput::Value(CellValue::Number(_))) && format.is_some() && general;
        if let (Some(code), true) = (format.clone(), general) {
            let range = CellRange { first_row: edit.row, first_column: edit.column, last_row: edit.row, last_column: edit.column };
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide: edit.slide, ranges: vec![range], change: StyleChange::NumberFormat(code) }));
        }
        // A number typed with a format Excel gives it (a date, `1,234`, `¥1,000`)
        // that is wider than its column widens the column, as Excel does.
        if auto_format
            && let (Some(code), Some(sheet), CellInput::Value(value)) = (format.as_ref(), sheet, &input)
        {
            let mut style = sheet.cell(edit.row, edit.column).map(|c| sheet.styles[c.style as usize].clone()).unwrap_or_else(|| self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default());
            style.number_format = code.clone();
            let shown = ibis_calc::numfmt::format(value, code, date1904).text;
            let ts = cx.text_system().clone();
            let needed = super::headers::fitted_width(self.excel_text_width(&shown, &style, &ts));
            let current = sheet.columns.get(edit.column as usize).copied().unwrap_or(0);
            if needed > current && current > 0 {
                commands.push(Command::Sheet(SheetCommand::SetSize { slide: edit.slide, axis: ibis_doc_model::sheet_command::Axis::Columns, first: edit.column, last: edit.column, size: needed }));
            }
        }
        // オートコレクト: a URL or an email address typed alone becomes a link
        // (the link's colour and underline, one step with the entry).
        if let CellInput::Value(CellValue::Text(t)) = &input
            && let Some(target) = link_target_of_typed(t)
        {
            let range = CellRange { first_row: edit.row, first_column: edit.column, last_row: edit.row, last_column: edit.column };
            let link = ibis_doc_model::Hyperlink { range, target: Some(target), location: None, tooltip: None };
            commands.push(Command::Sheet(SheetCommand::SetHyperlink { slide: edit.slide, range, link: Some(link) }));
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide: edit.slide, ranges: vec![range], change: StyleChange::Color(ibis_doc_model::Rgb(0x0563C1)) }));
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide: edit.slide, ranges: vec![range], change: StyleChange::Underline(true) }));
        }
        // Text with a line break (⌥Enter): the cell wraps and its row grows
        // to the lines, as Excel's do (in the same step).
        let broken = matches!(&input, CellInput::Value(CellValue::Text(t)) if t.contains('\n'));
        if broken && sheet.is_some_and(|sheet| !sheet.cell(edit.row, edit.column).is_some_and(|c| sheet.styles[c.style as usize].wrap)) {
            let range = CellRange { first_row: edit.row, first_column: edit.column, last_row: edit.row, last_column: edit.column };
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide: edit.slide, ranges: vec![range], change: StyleChange::Wrap(true) }));
        }
        // Parts in their own fonts (set on a selection while typing) go with the text.
        if let (Some(runs), CellInput::Value(CellValue::Text(t))) = (&edit.runs, &input)
            && runs.iter().map(|r| r.text.as_str()).collect::<String>() == *t
        {
            commands.push(Command::Sheet(SheetCommand::SetRuns { slide: edit.slide, row: edit.row, column: edit.column, runs: runs.clone() }));
        }
        let mut result = self.document.execute(commands.clone());
        if broken && result.is_ok() && edit.slide == self.current_slide {
            let ts = cx.text_system().clone();
            let rows = self.current_sheet().map(|(_, s)| s.rows.get(edit.row as usize).copied().unwrap_or(0)).unwrap_or(0);
            let fit: Vec<Command> = self
                .fit_rows_commands(edit.row, edit.row, &ts)
                .into_iter()
                .filter(|c| matches!(c, Command::Sheet(SheetCommand::SetSize { size, .. }) if *size > rows))
                .collect();
            if !fit.is_empty() {
                self.document.undo();
                commands.extend(fit);
                result = self.document.execute(commands);
            }
        }
        match result {
            Ok(_) => {
                self.after_table_entry(edit.slide, edit.row, edit.column, &input, cx);
                self.sync_shape_formulas(cx);
            }
            Err(err) => log::warn!("cell entry rejected: {err}"),
        }
        if let Some(w) = warning {
            self.status = Some(w.into());
        }
        self.sync_workbook_peers(cx);
        cx.notify();
    }

    /// Clear the selected cells' values (Delete when not typing).
    pub(crate) fn clear_selected_cells(&mut self, cx: &mut Context<Self>) {
        // With the ranges 条件を選択してジャンプ selected with it.
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let Some((_, sheet)) = self.current_sheet() else { return };
        let edits: Vec<ModelEdit> = sheet
            .cells
            .iter()
            .filter(|c| ranges.iter().any(|range| (range.first_row..=range.last_row).contains(&c.row) && (range.first_column..=range.last_column).contains(&c.column)))
            .filter(|c| !c.value.is_empty() || c.formula.is_some())
            .map(|c| ModelEdit { row: c.row, column: c.column, input: CellInput::Clear })
            .collect();
        if edits.is_empty() {
            return;
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })]) {
            log::warn!("clearing rejected: {err}");
        }
        self.sync_workbook_peers(cx);
        cx.notify();
    }

    /// Backspace or Delete: a character while typing, the cells' values otherwise.
    pub(crate) fn cell_delete(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.cell_edit_mut() else {
            return self.clear_selected_cells(cx);
        };
        if let Some(sel) = edit.selection() {
            edit.replace(sel, "");
            return cx.notify();
        }
        let caret = edit.caret;
        let range = if forward {
            let b0 = byte_at(&edit.text, caret);
            match edit.text[b0..].chars().next() {
                Some(c) => caret..caret + c.len_utf16(),
                None => return,
            }
        } else {
            let b1 = byte_at(&edit.text, caret);
            match edit.text[..b1].chars().next_back() {
                Some(c) => caret - c.len_utf16()..caret,
                None => return,
            }
        };
        edit.replace(range, "");
        cx.notify();
    }

    /// ← or → while typing in Edit mode: move the caret. False when the
    /// arrow is for moving between cells.
    pub(crate) fn cell_caret(&mut self, dx: i32, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.cell_edit_mut() else { return false };
        if edit.mode != EditMode::Edit || dx == 0 {
            return false;
        }
        let len = utf16_len(&edit.text);
        // With a selection, ← and → go to its start and end.
        edit.caret = match (edit.selection(), dx < 0) {
            (Some(sel), true) => sel.start,
            (Some(sel), false) => sel.end,
            (None, true) => edit.caret.saturating_sub(1),
            (None, false) => (edit.caret + 1).min(len),
        };
        edit.anchor = None;
        edit.pending = None;
        cx.notify();
        true
    }

    /// Shift+← or →: the selection of the text being edited grows or shrinks by a character. False when not editing in Edit mode.
    pub(crate) fn cell_extend_selection(&mut self, dx: i32, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.cell_edit_mut() else { return false };
        if edit.mode != EditMode::Edit || dx == 0 {
            return false;
        }
        let len = utf16_len(&edit.text);
        let anchor = edit.anchor.unwrap_or(edit.caret);
        edit.caret = if dx < 0 { edit.caret.saturating_sub(1) } else { (edit.caret + 1).min(len) };
        edit.anchor = Some(anchor);
        cx.notify();
        true
    }

    /// ⌘A while editing: the whole text selected.
    pub(crate) fn cell_select_all_text(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.cell_edit_mut() else { return false };
        edit.anchor = Some(0);
        edit.caret = utf16_len(&edit.text);
        cx.notify();
        true
    }

    /// A font change on the selected text while editing (太字・斜体・下線・取り消し線・文字の色・サイズ): the selected part gets it, in its own run. False when there is no selection (the whole cell is formatted then).
    pub(crate) fn format_edit_selection(&mut self, change: &StyleChange, cx: &mut Context<Self>) -> bool {
        let slide = self.current_slide;
        let style = self.current_style();
        let Some(edit) = self.cell_edit_mut() else { return false };
        if edit.slide != slide || edit.text.starts_with('=') {
            return false;
        }
        if !matches!(change, StyleChange::Bold(_) | StyleChange::Italic(_) | StyleChange::Underline(_) | StyleChange::UnderlineKind(_) | StyleChange::Strike(_) | StyleChange::Color(_) | StyleChange::ColorTheme(..) | StyleChange::FontSize(_)) {
            return false;
        }
        // The caret alone: the change waits for the text typed next (the same toggle again takes it back).
        let Some(sel) = edit.selection() else {
            let pending = edit.pending.get_or_insert_with(|| PendingFormat { changes: Vec::new(), style: style.clone() });
            let same = |a: &StyleChange, b: &StyleChange| std::mem::discriminant(a) == std::mem::discriminant(b);
            let toggles = matches!(change, StyleChange::Bold(_) | StyleChange::Italic(_) | StyleChange::Underline(_) | StyleChange::Strike(_));
            if toggles && pending.changes.iter().any(|c| same(c, change)) {
                pending.changes.retain(|c| !same(c, change));
            } else {
                pending.changes.retain(|c| !same(c, change));
                pending.changes.push(change.clone());
            }
            if pending.changes.is_empty() {
                edit.pending = None;
            }
            cx.notify();
            return true;
        };
        let (b0, b1) = (byte_at(&edit.text, sel.start), byte_at(&edit.text, sel.end));
        let runs = edit.runs.take().unwrap_or_else(|| vec![whole_run(&edit.text, &style)]);
        edit.runs = Some(apply_to_runs(runs, b0, b1, change, &style));
        cx.notify();
        true
    }

    /// A click on a cell while typing a formula where a reference may go:
    /// its reference put in at the caret (and remembered, for a drag to
    /// make it a range). False when the click is for leaving the entry.
    pub(crate) fn point_reference(&mut self, column: u32, row: u32, cx: &mut Context<Self>) -> bool {
        let slide = self.current_slide;
        let Some(edit) = self.cell_edit_mut() else { return false };
        if edit.slide != slide || !edit.text.starts_with('=') {
            return false;
        }
        let caret = byte_at(&edit.text, edit.caret);
        let before = edit.text[..caret].trim_end();
        // Right after a reference put in by pointing, a new click replaces it.
        let replacing = edit.pointed.filter(|&(s, e, _)| e == caret && s <= caret);
        let open = replacing.is_some() || before.ends_with(['=', '(', ',', '+', '-', '*', '/', '^', '&', '<', '>', ':']);
        if !open {
            return false;
        }
        let reference = ibis_doc_model::cell_name(column, row);
        let start = replacing.map_or(caret, |(s, _, _)| s);
        edit.text.replace_range(start..caret, &reference);
        let end = start + reference.len();
        edit.caret = utf16_len(&edit.text[..end]);
        edit.pointed = Some((start, end, (column, row)));
        cx.notify();
        true
    }

    /// The button up after pointing: the reference becomes the range to
    /// the cell let go on.
    pub(crate) fn finish_pointing(&mut self, column: u32, row: u32, cx: &mut Context<Self>) {
        let Some(edit) = self.cell_edit_mut() else { return };
        let Some((start, end, anchor)) = edit.pointed else { return };
        if anchor == (column, row) {
            return;
        }
        let (c0, c1) = (anchor.0.min(column), anchor.0.max(column));
        let (r0, r1) = (anchor.1.min(row), anchor.1.max(row));
        let range = format!("{}:{}", ibis_doc_model::cell_name(c0, r0), ibis_doc_model::cell_name(c1, r1));
        edit.text.replace_range(start..end, &range);
        let new_end = start + range.len();
        edit.caret = utf16_len(&edit.text[..new_end]);
        edit.pointed = Some((start, new_end, anchor));
        cx.notify();
    }

    /// F4 while typing a formula: the reference at (or just before) the
    /// caret cycles `A1` → `$A$1` → `A$1` → `$A1` → `A1`, as in Excel.
    pub(crate) fn cycle_reference(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.cell_edit_mut() else { return false };
        if !edit.text.starts_with('=') {
            return false;
        }
        let caret = byte_at(&edit.text, edit.caret);
        let Some((start, end, new)) = next_reference_form(&edit.text, caret) else { return false };
        edit.text.replace_range(start..end, &new);
        edit.caret = utf16_len(&edit.text[..start + new.len()]);
        cx.notify();
        true
    }

    // ----- The input hooks (shell/input.rs) -----------------------------

    pub(crate) fn cell_text_for_range(&self, range: Range<usize>) -> Option<String> {
        let text = self.cell_edit().map_or("", |e| e.text.as_str());
        let (b0, b1) = (byte_at(text, range.start), byte_at(text, range.end));
        Some(text[b0..b1.max(b0)].to_string())
    }

    pub(crate) fn cell_selected_range(&self) -> Option<UTF16Selection> {
        let caret = self.cell_edit().map_or(0, |e| e.caret);
        Some(UTF16Selection { range: caret..caret, reversed: false })
    }

    pub(crate) fn cell_marked_range(&self) -> Option<Range<usize>> {
        self.cell_edit().and_then(|e| e.marked.clone())
    }

    pub(crate) fn cell_unmark(&mut self) {
        if let Some(e) = self.cell_edit_mut() {
            e.marked = None;
        }
    }

    /// The bytes `start..end` of the text being typed replaced with `text`,
    /// the caret after it.
    pub(crate) fn replace_cell_text(&mut self, start: usize, end: usize, text: &str, cx: &mut Context<Self>) {
        let Some(edit) = self.cell_edit_mut() else { return };
        let (a, b) = (utf16_len(&edit.text[..start.min(edit.text.len())]), utf16_len(&edit.text[..end.min(edit.text.len())]));
        edit.pointed = None;
        edit.marked = None;
        edit.replace(a..b, text);
        cx.notify();
    }

    /// ⌥Enter: a line break at the caret while typing (the cell then wraps
    /// when the entry is finished, as Excel's does).
    pub(crate) fn cell_line_break(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.cell_edit_mut() else { return };
        edit.pointed = None;
        let range = edit.marked.take().unwrap_or(edit.caret..edit.caret);
        edit.replace(range, "\n");
        cx.notify();
    }

    /// Text typed (or committed by the input method).
    pub(crate) fn cell_replace(&mut self, range: Option<Range<usize>>, text: &str, mark: Option<Option<Range<usize>>>, cx: &mut Context<Self>) {
        // Keys that finish or move are actions, not text.
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if self.cell_edit().is_none() {
            if text.is_empty() && mark.is_none() {
                return;
            }
            self.begin_cell_edit(EditMode::Enter, false, cx);
        }
        let Some(edit) = self.cell_edit_mut() else { return };
        edit.pointed = None;
        let range = range.or_else(|| edit.marked.clone()).or_else(|| edit.selection()).unwrap_or(edit.caret..edit.caret);
        let inserted = edit.replace(range, &text);
        match mark {
            Some(selected) => {
                edit.marked = (!inserted.is_empty()).then_some(inserted.clone());
                if let Some(sel) = selected {
                    edit.caret = inserted.start + sel.end.min(inserted.len());
                }
            }
            None => edit.marked = None,
        }
        cx.notify();
    }

    pub(crate) fn cell_bounds_for_range(&self, element: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
        let edit = self.cell_edit()?;
        Some(edit.rect.get().unwrap_or(Bounds::new(element.origin, size(px(100.), px(20.)))))
    }
}

/// The number format a formula typed into a General cell takes, as Excel
/// gives it: that of the first cell it refers to on its own sheet (not for
/// the COUNT functions, whose results are counts).
/// A piece of the text being typed, for painting it.
pub(crate) enum EditItem {
    Text { text: String, run: Option<CellRun>, selected: bool },
    Caret,
}

impl CellEdit {
    /// The text as pieces (cut where a part, the selection or the caret begins), the caret among them.
    pub(crate) fn items(&self) -> Vec<EditItem> {
        let len = self.text.len();
        let caret = self.caret_byte();
        let sel = self.selection().map(|s| (byte_at(&self.text, s.start), byte_at(&self.text, s.end)));
        let mut cuts = vec![0, len, caret];
        if let Some((a, b)) = sel {
            cuts.extend([a, b]);
        }
        let mut at = 0;
        for run in self.runs.iter().flatten() {
            at += run.text.len();
            cuts.push(at.min(len));
        }
        cuts.sort_unstable();
        cuts.dedup();
        let run_at = |b: usize| -> Option<CellRun> {
            let mut start = 0;
            for run in self.runs.iter().flatten() {
                let end = start + run.text.len();
                if b >= start && b < end {
                    return Some(run.clone());
                }
                start = end;
            }
            None
        };
        let mut out = Vec::new();
        for w in cuts.windows(2) {
            if w[0] == caret {
                out.push(EditItem::Caret);
            }
            out.push(EditItem::Text { text: self.text[w[0]..w[1]].to_string(), run: run_at(w[0]), selected: sel.is_some_and(|(a, b)| w[0] >= a && w[1] <= b) });
        }
        if caret == len {
            out.push(EditItem::Caret);
        }
        out
    }
}

/// One run of all of `text` in the cell's font, every property set (as Excel writes a run).
fn whole_run(text: &str, style: &ibis_doc_model::CellStyle) -> CellRun {
    CellRun {
        text: text.to_string(),
        font: None,
        size: Some(style.font_size),
        bold: Some(style.bold),
        italic: Some(style.italic),
        underline: Some(style.underline),
        strike: Some(style.strike),
        color: Some(style.color),
    }
}

/// `runs` with `change` applied to the bytes `b0..b1` of their text (a toggle
/// goes off when the whole selection has it on, else on).
fn apply_to_runs(runs: Vec<CellRun>, b0: usize, b1: usize, change: &StyleChange, style: &ibis_doc_model::CellStyle) -> Vec<CellRun> {
    // Split at both ends.
    let mut split: Vec<CellRun> = Vec::new();
    let mut at = 0usize;
    for run in runs {
        let (start, end) = (at, at + run.text.len());
        at = end;
        let mut cuts = vec![start];
        cuts.extend([b0, b1].into_iter().filter(|&b| b > start && b < end));
        cuts.push(end);
        for w in cuts.windows(2) {
            split.push(CellRun { text: run.text[w[0] - start..w[1] - start].to_string(), ..run.clone() });
        }
    }
    let inside = |i: usize, split: &[CellRun]| {
        let start: usize = split[..i].iter().map(|r| r.text.len()).sum();
        start >= b0 && start + split[i].text.len() <= b1
    };
    let all = |f: &dyn Fn(&CellRun) -> bool| split.iter().enumerate().filter(|(i, _)| inside(*i, &split)).all(|(_, r)| f(r));
    let bold_on = !all(&|r| r.bold.unwrap_or(style.bold));
    let italic_on = !all(&|r| r.italic.unwrap_or(style.italic));
    let underline_on = !all(&|r| r.underline.unwrap_or(style.underline).enabled());
    let strike_on = !all(&|r| r.strike.unwrap_or(style.strike));
    let flags: Vec<bool> = (0..split.len()).map(|i| inside(i, &split)).collect();
    for (run, hit) in split.iter_mut().zip(flags) {
        if !hit {
            continue;
        }
        match change {
            StyleChange::Bold(_) => run.bold = Some(bold_on),
            StyleChange::Italic(_) => run.italic = Some(italic_on),
            StyleChange::Underline(_) => run.underline = Some(if underline_on { ibis_doc_model::CellUnderline::Single } else { ibis_doc_model::CellUnderline::None }),
            StyleChange::UnderlineKind(u) => run.underline = Some(*u),
            StyleChange::Strike(_) => run.strike = Some(strike_on),
            StyleChange::Color(c) | StyleChange::ColorTheme(c, _) => run.color = Some(*c),
            StyleChange::FontSize(s) => run.size = Some(*s),
            _ => {}
        }
    }
    merge_runs(split)
}

fn referenced_format(sheet: &ibis_doc_model::Sheet, formula: &str) -> Option<String> {
    // Excel gives the result the format of the cell it refers to for
    // arithmetic and a few functions (not for counts, DATEDIF, EDATE, …).
    let upper = formula.trim_start().to_ascii_uppercase();
    if let Some(paren) = upper.find('(') {
        let name = upper[..paren].trim_start_matches(['+', '-', '@']);
        let inherits = ["SUM", "AVERAGE", "MIN", "MAX", "ROUND", "ROUNDUP", "ROUNDDOWN", "ABS", "IF", "MEDIAN", "SUBTOTAL"];
        if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_') && !name.is_empty() && !inherits.contains(&name) {
            return None;
        }
    }
    let mut first = None;
    ibis_calc::formula::rewrite_references(formula, |r| {
        if first.is_none() && r.sheet.is_none() && !r.whole_columns && !r.whole_rows {
            first = Some((r.start.row, r.start.column));
        }
        Some(r.clone())
    });
    let (row, column) = first?;
    let code = &sheet.styles[sheet.cell(row, column)?.style as usize].number_format;
    (code != "General" && code != "@").then(|| code.clone())
}

/// The cell reference touching byte `caret` in `text` (`$A$1`, `A1`, a
/// range's either end) and its next form for F4: (start, end, new text).
pub(crate) fn next_reference_form(text: &str, caret: usize) -> Option<(usize, usize, String)> {
    let bytes = text.as_bytes();
    let is_part = |b: u8| b.is_ascii_alphanumeric() || b == b'$';
    // The run of reference characters around the caret.
    let mut start = caret.min(bytes.len());
    while start > 0 && is_part(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = caret.min(bytes.len());
    while end < bytes.len() && is_part(bytes[end]) {
        end += 1;
    }
    let word = &text[start..end];
    let plain = word.replace('$', "");
    let letters = plain.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let digits = &plain[letters..];
    if letters == 0 || letters > 3 || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (column, row) = (&plain[..letters], digits);
    let column_fixed = word.starts_with('$');
    let row_fixed = word[1..].contains('$');
    let next = match (column_fixed, row_fixed) {
        (false, false) => format!("${column}${row}"),
        (true, true) => format!("{column}${row}"),
        (false, true) => format!("${column}{row}"),
        (true, false) => format!("{column}{row}"),
    };
    Some((start, end, next))
}

#[cfg(test)]
mod tests {
    use super::next_reference_form;

    #[test]
    fn f4() {
        let step = |t: &str, c: usize| next_reference_form(t, c).map(|(s, e, n)| format!("{}{}{}", &t[..s], n, &t[e..]));
        assert_eq!(step("=A1*2", 3).as_deref(), Some("=$A$1*2"));
        assert_eq!(step("=$A$1*2", 5).as_deref(), Some("=A$1*2"));
        assert_eq!(step("=A$1*2", 4).as_deref(), Some("=$A1*2"));
        assert_eq!(step("=$A1*2", 4).as_deref(), Some("=A1*2"));
        assert_eq!(step("=SUM(B2:C3)", 10).as_deref(), Some("=SUM(B2:$C$3)"));
        assert_eq!(step("=SUM(", 5), None);
    }
}

/// The address a typed text is a link to, as Excel's オートコレクト makes
/// one: `http://…`, `https://…`, `ftp://…`, `www.…` (made `http://www.…`),
/// `mailto:…` or an email address (made `mailto:…`), all in one word.
pub(crate) fn link_target_of_typed(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    for scheme in ["http://", "https://", "ftp://", "file://"] {
        if lower.starts_with(scheme) && t.len() > scheme.len() {
            return Some(t.to_string());
        }
    }
    if lower.starts_with("mailto:") && t.len() > 7 {
        return Some(t.to_string());
    }
    if lower.starts_with("www.") && t.len() > 4 && t[4..].contains('.') {
        return Some(format!("http://{t}"));
    }
    if let Some((local, domain)) = t.split_once('@')
        && !local.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && local.chars().all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c))
        && domain.chars().all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))
    {
        return Some(format!("mailto:{t}"));
    }
    None
}

#[cfg(test)]
mod link_tests {
    use super::link_target_of_typed as l;

    #[test]
    fn typed_addresses_become_links() {
        assert_eq!(l("https://example.com/a?b=1").as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(l("www.example.com").as_deref(), Some("http://www.example.com"));
        assert_eq!(l("user@example.com").as_deref(), Some("mailto:user@example.com"));
        assert_eq!(l("mailto:a@b.jp").as_deref(), Some("mailto:a@b.jp"));
        assert_eq!(l("see https://x.jp"), None);
        assert_eq!(l("abc"), None);
        assert_eq!(l("a@b"), None);
        assert_eq!(l("www.x"), None);
    }
}

impl SlideEditor {
    /// ⌃; and ⌘;: today's date (or the time now) in the selected cell, as a
    /// value (it does not change), or at the caret while typing.
    pub(crate) fn insert_now_stamp(&mut self, date: bool, cx: &mut Context<Self>) {
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let Some(now) = ibis_calc::eval::convert::now_serial(date1904) else { return };
        let (value, code) = if date { (now.floor(), "yyyy/m/d") } else { (now - now.floor(), "h:mm") };
        let text = ibis_calc::numfmt::format(&CellValue::Number(value), code, date1904).text;
        if self.cell_edit().is_some() {
            return self.cell_replace(None, &text, None, cx);
        }
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.begin_cell_edit(EditMode::Enter, false, cx);
        self.cell_replace(None, &text, None, cx);
        self.commit_cell_edit(cx);
    }
}

impl SlideEditor {
    /// ⌘C and ⌘X while typing into a cell: the selected text on the system
    /// clipboard (nothing selected: nothing happens), cut taking it out.
    pub(crate) fn edit_copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.cell_edit() else { return };
        let Some(sel) = edit.selection() else { return };
        let (b0, b1) = (byte_at(&edit.text, sel.start), byte_at(&edit.text, sel.end));
        let text = edit.text[b0..b1].to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        if cut {
            self.replace_cell_text(b0, b1, "", cx);
        }
        cx.notify();
    }

    /// ⌘V while typing into a cell: the clipboard's text at the caret (over
    /// the selection), its line breaks kept as the cell's own.
    pub(crate) fn edit_paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else { return };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let text = text.trim_end_matches('\n').to_string();
        if text.is_empty() {
            return;
        }
        let Some(edit) = self.cell_edit() else { return };
        let range = edit.selection().unwrap_or(edit.caret..edit.caret);
        let (b0, b1) = (byte_at(&edit.text, range.start), byte_at(&edit.text, range.end));
        self.replace_cell_text(b0, b1, &text, cx);
    }
}
