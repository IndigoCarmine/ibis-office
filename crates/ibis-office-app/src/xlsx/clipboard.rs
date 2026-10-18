// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Cutting, copying and pasting cells. Copied cells go on the system
//! clipboard as text (tab-separated, as Excel puts them), and are kept
//! here with their formulas and formats: pasting what was copied here puts
//! the formulas (their relative references moved) and the formats; a cut
//! pasted on its sheet moves the cells, and formulas referring to them
//! follow (as in Excel); pasting other text reads it as typed.

use gpui_kit::*;
use ibis_calc::input::{Typed, parse_input};
use ibis_doc_model::{
    CellEdit, CellInput, CellRange, CellStyle, Command, PutCell, SheetCell, SheetCommand, SlideId, Validation,
};

use crate::shell::editor::SlideEditor;

/// Cells copied or cut.
#[derive(Clone, Debug)]
pub(crate) struct Clip {
    pub(crate) slide: SlideId,
    pub(crate) range: CellRange,
    /// Each cell: its place in the range, the cell and its format.
    pub(crate) cells: Vec<(u32, u32, SheetCell, CellStyle)>,
    pub(crate) cut: bool,
    /// The text put on the system clipboard (to know it is still ours).
    pub(crate) text: String,
    /// Validations in the copied range: (relative range within clip, rule).
    pub(crate) validations: Vec<(CellRange, Validation)>,
}

impl SlideEditor {
    pub(crate) fn copy_cells(&mut self, cut: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let (Some(range), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) else { return };
        let mut cells = Vec::new();
        let mut text = String::new();
        for row in range.first_row..=range.last_row {
            for column in range.first_column..=range.last_column {
                let cell = sheet.cell(row, column);
                if column > range.first_column {
                    text.push('\t');
                }
                if let Some(c) = cell {
                    text.push_str(&c.text);
                    cells.push((row - range.first_row, column - range.first_column, c.clone(), sheet.styles[c.style as usize].clone()));
                }
            }
            text.push('\n');
        }
        let mut validations = Vec::new();
        for v in &sheet.validations {
            for vr in &v.ranges {
                let r0 = vr.first_row.max(range.first_row);
                let r1 = vr.last_row.min(range.last_row);
                let c0 = vr.first_column.max(range.first_column);
                let c1 = vr.last_column.min(range.last_column);
                if r0 <= r1 && c0 <= c1 {
                    let rel = CellRange {
                        first_row: r0 - range.first_row,
                        first_column: c0 - range.first_column,
                        last_row: r1 - range.first_row,
                        last_column: c1 - range.first_column,
                    };
                    let mut rule = v.clone();
                    rule.ranges = vec![rel];
                    validations.push((rel, rule));
                }
            }
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        let slide = self.current_slide;
        if let Some(state) = self.sheet_state_mut() {
            state.clip = Some(Clip { slide, range, cells, cut, text, validations });
        }
    }

    /// Paste at the selected cell (`plain`: values only, no formulas or formats).
    pub(crate) fn paste_cells(&mut self, plain: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let slide = self.current_slide;
        let item = cx.read_from_clipboard();
        let text = item.as_ref().and_then(|c| c.text()).unwrap_or_default();
        let clip = self.sheet_state().and_then(|s| s.clip.clone()).filter(|c| c.text == text);
        // An object copied in its application (OLE) stays one.
        #[cfg(not(target_family = "wasm"))]
        if clip.is_none() && !plain && self.paste_embedded(cx) {
            return;
        }
        if clip.is_none() && text.is_empty() {
            if let Some(item) = &item {
                if let Some(ClipboardEntry::Image(image)) = item.entries().iter().find(|e| matches!(e, ClipboardEntry::Image(_))) {
                    if self.insert_sheet_picture("clipboard.png", image.bytes.clone(), cx).is_some() {
                        self.status = Some(crate::shell::i18n::text("_xlsx.pasted-the-image").into());
                        return cx.notify();
                    }
                }
            }
        }
        let mut commands = Vec::new();
        match clip {
            // A cut pasted on its own sheet moves the cells: formulas that
            // refer to them follow (Excel).
            Some(clip) if clip.cut && clip.slide == slide && !plain => {
                commands.push(Command::Sheet(SheetCommand::MoveCells { slide, from: clip.range, row, column }));
                if let Some(state) = self.sheet_state_mut() {
                    state.clip = None;
                }
            }
            Some(clip) => {
                let (h, w) = (clip.range.last_row - clip.range.first_row + 1, clip.range.last_column - clip.range.first_column + 1);
                // Copied cells pasted over a larger selection that holds them
                // a whole number of times fill it, as in Excel (not a cut).
                let (times_down, times_across) = match self.selected_range() {
                    Some(sel) if !clip.cut => {
                        let (sh, sw) = (sel.last_row - sel.first_row + 1, sel.last_column - sel.first_column + 1);
                        if sh % h == 0 && sw % w == 0 { (sh / h, sw / w) } else { (1, 1) }
                    }
                    _ => (1, 1),
                };
                let (row, column) = match self.selected_range() {
                    Some(sel) if (times_down, times_across) != (1, 1) => (sel.first_row, sel.first_column),
                    _ => (row, column),
                };
                let mut cells = Vec::new();
                for td in 0..times_down {
                    for ta in 0..times_across {
                        let (row, column) = (row + td * h, column + ta * w);
                        let (dr, dc) = (row as i64 - clip.range.first_row as i64, column as i64 - clip.range.first_column as i64);
                        // The whole range: empty source cells empty the target.
                        for r in 0..h {
                            for c in 0..w {
                                let source = clip.cells.iter().find(|(sr, sc, _, _)| (*sr, *sc) == (r, c));
                                let input = match source {
                                    Some((_, _, cell, _)) => match &cell.formula {
                                        Some(_) if plain => CellInput::Value(cell.value.clone()),
                                        Some(f) if clip.cut => CellInput::Formula(f.clone()),
                                        Some(f) => CellInput::Formula(ibis_calc::formula::shift(f, dr, dc)),
                                        None if cell.value.is_empty() => CellInput::Clear,
                                        None => CellInput::Value(cell.value.clone()),
                                    },
                                    None => CellInput::Clear,
                                };
                                let style = if plain { None } else { source.map(|(_, _, _, s)| s.clone()) };
                                cells.push(PutCell { row: row + r, column: column + c, input, style });
                            }
                        }
                    }
                }
                if clip.cut {
                    // The cut cells go (those not pasted over).
                    let target = CellRange {
                        first_row: row,
                        first_column: column,
                        last_row: row + clip.range.last_row - clip.range.first_row,
                        last_column: column + clip.range.last_column - clip.range.first_column,
                    };
                    let inside = |r: u32, c: u32| clip.slide == slide && (target.first_row..=target.last_row).contains(&r) && (target.first_column..=target.last_column).contains(&c);
                    let edits: Vec<CellEdit> = clip
                        .cells
                        .iter()
                        .map(|(r, c, _, _)| (clip.range.first_row + r, clip.range.first_column + c))
                        .filter(|&(r, c)| !inside(r, c))
                        .map(|(r, c)| CellEdit { row: r, column: c, input: CellInput::Clear })
                        .collect();
                    if !edits.is_empty() {
                        commands.push(Command::Sheet(SheetCommand::SetCells { slide: clip.slide, edits }));
                        let ranges = vec![clip.range];
                        commands.push(Command::Sheet(SheetCommand::ClearFormats { slide: clip.slide, ranges }));
                        commands.push(Command::Sheet(SheetCommand::SetValidation { slide: clip.slide, ranges: vec![clip.range], rule: None }));
                    }
                    if let Some(state) = self.sheet_state_mut() {
                        state.clip = None;
                    }
                }
                if !plain {
                    for td in 0..times_down {
                        for ta in 0..times_across {
                            let (tile_row, tile_col) = (row + td * h, column + ta * w);
                            for (rel, rule) in &clip.validations {
                                let target_range = CellRange {
                                    first_row: tile_row + rel.first_row,
                                    first_column: tile_col + rel.first_column,
                                    last_row: tile_row + rel.last_row,
                                    last_column: tile_col + rel.last_column,
                                };
                                let mut new_rule = rule.clone();
                                new_rule.ranges = vec![target_range];
                                commands.push(Command::Sheet(SheetCommand::SetValidation {
                                    slide,
                                    ranges: vec![target_range],
                                    rule: Some(new_rule),
                                }));
                            }
                        }
                    }
                }
                commands.insert(0, Command::Sheet(SheetCommand::PutCells { slide, cells }));
            }
            None => {
                let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
                let mut edits = Vec::new();
                let mut formats = Vec::new();
                for (r, line) in text.trim_end_matches(['\n', '\r']).split('\n').enumerate() {
                    for (c, field) in line.trim_end_matches('\r').split('\t').enumerate() {
                        let at = (row + r as u32, column + c as u32);
                        let input = if field.is_empty() {
                            CellInput::Clear
                        } else {
                            match parse_input(field, date1904) {
                                Typed::Formula(f) => CellInput::Formula(f),
                                // A date, a percentage or an amount keeps how it was written.
                                Typed::Value(v, format) => {
                                    if let Some(code) = format {
                                        formats.push((at, code));
                                    }
                                    CellInput::Value(v)
                                }
                            }
                        };
                        edits.push(CellEdit { row: at.0, column: at.1, input });
                    }
                }
                if edits.is_empty() {
                    return;
                }
                commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
                for ((r, c), code) in formats {
                    let range = CellRange { first_row: r, last_row: r, first_column: c, last_column: c };
                    commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: ibis_doc_model::StyleChange::NumberFormat(code.to_string()) }));
                }
            }
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("paste rejected: {err}");
            self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
        }
        cx.notify();
    }

    /// Take the formats off the selected cells.
    pub(crate) fn clear_cell_formats(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::ClearFormats { slide, ranges: vec![range] })]) {
            log::warn!("clearing formats rejected: {err}");
        }
        cx.notify();
    }
}

/// 形式を選択して貼り付け: what of the copied cells goes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasteSpecial {
    /// Everything: formulas (or values) and formats.
    All,
    Values,
    Formats,
    Formulas,
    /// データの入力規則.
    Validation,
    /// Rows and columns swapped (values, formulas and formats).
    Transpose,
    /// The copied numbers added to (subtracted from, …) the cells pasted on.
    Add,
    Subtract,
    Multiply,
    Divide,
    /// Formulas referring to the copied cells.
    Link,
}

impl SlideEditor {
    /// Paste what was copied here, as `kind` says, at the selected cell.
    pub(crate) fn paste_special(&mut self, kind: PasteSpecial, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let slide = self.current_slide;
        let Some(clip) = self.sheet_state().and_then(|s| s.clip.clone()) else {
            self.status = Some(crate::shell::i18n::text("_xlsx.to-use-paste-special-copy-cells-first").into());
            return cx.notify();
        };
        let Some((_, sheet)) = self.current_sheet() else { return };
        // The copied sheet's name, for links to another sheet.
        let source_sheet = self.presentation().workbook.as_ref().and_then(|w| w.sheets.iter().find(|s| s.slide == Some(clip.slide)).map(|s| s.name.clone()));
        let (h, w) = (clip.range.last_row - clip.range.first_row + 1, clip.range.last_column - clip.range.first_column + 1);
        if kind == PasteSpecial::Validation {
            let mut commands = Vec::new();
            for (rel, rule) in &clip.validations {
                let target_range = CellRange {
                    first_row: row + rel.first_row,
                    first_column: column + rel.first_column,
                    last_row: row + rel.last_row,
                    last_column: column + rel.last_column,
                };
                let mut new_rule = rule.clone();
                new_rule.ranges = vec![target_range];
                commands.push(Command::Sheet(SheetCommand::SetValidation {
                    slide,
                    ranges: vec![target_range],
                    rule: Some(new_rule),
                }));
            }
            if !commands.is_empty() {
                if let Err(err) = self.document.execute(commands) {
                    log::warn!("paste validation rejected: {err}");
                    self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                }
                cx.notify();
            }
            return;
        }
        let skip_blanks = self.sheet_state().is_some_and(|s| s.panels.skip_blanks);
        let mut cells = Vec::new();
        for r in 0..h {
            for c in 0..w {
                let source = clip.cells.iter().find(|(sr, sc, _, _)| (*sr, *sc) == (r, c));
                // 空白セルを無視する: an empty copied cell leaves its target.
                if skip_blanks && kind != PasteSpecial::Link && source.is_none_or(|s| s.2.value.is_empty() && s.2.formula.is_none()) {
                    continue;
                }
                let (tr, tc) = if kind == PasteSpecial::Transpose { (row + c, column + r) } else { (row + r, column + c) };
                let target = sheet.cell(tr, tc);
                let keep = || match target {
                    Some(t) => match &t.formula {
                        Some(f) => CellInput::Formula(f.clone()),
                        None if t.value.is_empty() => CellInput::Clear,
                        None => CellInput::Value(t.value.clone()),
                    },
                    None => CellInput::Clear,
                };
                let shifted = |f: &str| ibis_calc::formula::shift(f, tr as i64 - (clip.range.first_row + r) as i64, tc as i64 - (clip.range.first_column + c) as i64);
                let (input, style) = match kind {
                    PasteSpecial::Values => (source.map_or(CellInput::Clear, |s| if s.2.value.is_empty() { CellInput::Clear } else { CellInput::Value(s.2.value.clone()) }), None),
                    PasteSpecial::Formats => (keep(), Some(source.map_or_else(CellStyle::default, |s| s.3.clone()))),
                    PasteSpecial::Formulas | PasteSpecial::Transpose | PasteSpecial::All => {
                        let input = match source {
                            Some((_, _, cell, _)) => match &cell.formula {
                                Some(f) => CellInput::Formula(shifted(f)),
                                None if cell.value.is_empty() => CellInput::Clear,
                                None => CellInput::Value(cell.value.clone()),
                            },
                            None => CellInput::Clear,
                        };
                        (input, matches!(kind, PasteSpecial::Transpose | PasteSpecial::All).then(|| source.map_or_else(CellStyle::default, |s| s.3.clone())))
                    }
                    PasteSpecial::Add | PasteSpecial::Subtract | PasteSpecial::Multiply | PasteSpecial::Divide => {
                        let Some(ibis_doc_model::CellValue::Number(b)) = source.map(|s| s.2.value.clone()) else { continue };
                        let op = match kind {
                            PasteSpecial::Add => '+',
                            PasteSpecial::Subtract => '-',
                            PasteSpecial::Multiply => '*',
                            _ => '/',
                        };
                        let number = ibis_calc::eval::number_to_text(b);
                        let input = match target {
                            Some(t) if t.formula.is_some() => CellInput::Formula(format!("({}){op}{number}", t.formula.as_deref().unwrap_or_default())),
                            Some(t) => match t.value {
                                ibis_doc_model::CellValue::Number(a) => {
                                    let v = match op {
                                        '+' => a + b,
                                        '-' => a - b,
                                        '*' => a * b,
                                        _ if b == 0.0 => {
                                            cells.push(PutCell { row: tr, column: tc, input: CellInput::Value(ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Div0)), style: None });
                                            continue;
                                        }
                                        _ => a / b,
                                    };
                                    CellInput::Value(ibis_doc_model::CellValue::Number(v))
                                }
                                ibis_doc_model::CellValue::Empty => CellInput::Value(ibis_doc_model::CellValue::Number(match op {
                                    '+' => b,
                                    '-' => -b,
                                    _ => 0.0,
                                })),
                                _ => continue,
                            },
                            None => CellInput::Value(ibis_doc_model::CellValue::Number(match op {
                                '+' => b,
                                '-' => -b,
                                _ => 0.0,
                            })),
                        };
                        (input, None)
                    }
                    PasteSpecial::Link => {
                        let reference = ibis_doc_model::cell_name(clip.range.first_column + c, clip.range.first_row + r);
                        let formula = match (&source_sheet, clip.slide == slide) {
                            (Some(name), false) => format!("{}!{reference}", ibis_calc::formula::quote_sheet(name)),
                            _ => reference,
                        };
                        (CellInput::Formula(formula), None)
                    }
                    PasteSpecial::Validation => unreachable!(),
                };
                cells.push(PutCell { row: tr, column: tc, input, style });
            }
        }
        if cells.is_empty() {
            return;
        }
        let mut commands = vec![Command::Sheet(SheetCommand::PutCells { slide, cells })];
        if kind == PasteSpecial::All {
            for (rel, rule) in &clip.validations {
                let target_range = CellRange {
                    first_row: row + rel.first_row,
                    first_column: column + rel.first_column,
                    last_row: row + rel.last_row,
                    last_column: column + rel.last_column,
                };
                let mut new_rule = rule.clone();
                new_rule.ranges = vec![target_range];
                commands.push(Command::Sheet(SheetCommand::SetValidation {
                    slide,
                    ranges: vec![target_range],
                    rule: Some(new_rule),
                }));
            }
        } else if kind == PasteSpecial::Transpose {
            for (rel, rule) in &clip.validations {
                let target_range = CellRange {
                    first_row: row + rel.first_column,
                    first_column: column + rel.first_row,
                    last_row: row + rel.last_column,
                    last_column: column + rel.last_row,
                };
                let mut new_rule = rule.clone();
                new_rule.ranges = vec![target_range];
                commands.push(Command::Sheet(SheetCommand::SetValidation {
                    slide,
                    ranges: vec![target_range],
                    rule: Some(new_rule),
                }));
            }
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("paste special rejected: {err}");
            self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
        }
        cx.notify();
    }
}

impl SlideEditor {
    /// 書式のコピー/貼り付け: pick up the selected cells' formats (or, on
    /// again, give it up); the next range selected takes them.
    pub(crate) fn toggle_format_paint(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        if self.sheet_state_mut().and_then(|s| s.painter.take()).is_some() {
            return cx.notify();
        }
        let (Some(range), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) else { return };
        let default = self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
        let mut styles = Vec::new();
        for row in range.first_row..=range.last_row {
            let line: Vec<CellStyle> = (range.first_column..=range.last_column).map(|column| sheet.cell(row, column).map_or(default.clone(), |c| sheet.styles[c.style as usize].clone())).collect();
            styles.push(line);
        }
        if let Some(state) = self.sheet_state_mut() {
            state.painter = Some(styles);
        }
        cx.notify();
    }

    /// The range selected after 書式のコピー: it takes the formats picked up
    /// (repeated over it), its values staying.
    pub(crate) fn finish_format_paint(&mut self, cx: &mut Context<Self>) {
        let Some(styles) = self.sheet_state_mut().and_then(|s| s.painter.take()) else { return };
        let (Some(range), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) else { return };
        let (h, w) = (styles.len() as u32, styles.first().map_or(0, |l| l.len()) as u32);
        if h == 0 || w == 0 {
            return;
        }
        // A single cell clicked takes the whole block from there.
        let (last_row, last_column) = if range.first_row == range.last_row && range.first_column == range.last_column { (range.first_row + h - 1, range.first_column + w - 1) } else { (range.last_row, range.last_column) };
        let mut cells = Vec::new();
        for row in range.first_row..=last_row {
            for column in range.first_column..=last_column {
                let style = styles[((row - range.first_row) % h) as usize][((column - range.first_column) % w) as usize].clone();
                let input = match sheet.cell(row, column) {
                    Some(t) => match &t.formula {
                        Some(f) => CellInput::Formula(f.clone()),
                        None if t.value.is_empty() => CellInput::Clear,
                        None => CellInput::Value(t.value.clone()),
                    },
                    None => CellInput::Clear,
                };
                cells.push(PutCell { row, column, input, style: Some(style) });
            }
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::PutCells { slide, cells })]) {
            log::warn!("format paint rejected: {err}");
        }
        cx.notify();
    }
}
