// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > フラッシュ フィル (⌃E): the rest of a column filled after the
//! way its first cells were typed from the cells beside them, as Excel's
//! Flash Fill: each example is read as pieces, each piece either a part of
//! one of the row's cells (a run of letters, of digits or of other
//! characters, counted from the start or from the end; as it is, in
//! capitals, in small letters or with a capital first) or text of its own;
//! the simplest way that makes every example is used for the rows below.

/// A part of a cell: its runs of the same kind of character.
fn tokens(text: &str) -> Vec<String> {
    #[derive(PartialEq, Clone, Copy)]
    enum Kind {
        Letter,
        Digit,
        Space,
        Other,
    }
    let kind = |c: char| {
        if c.is_ascii_digit() || c.is_numeric() && !c.is_alphabetic() {
            Kind::Digit
        } else if c.is_alphabetic() {
            Kind::Letter
        } else if c.is_whitespace() {
            Kind::Space
        } else {
            Kind::Other
        }
    };
    let mut out: Vec<String> = Vec::new();
    let mut last: Option<Kind> = None;
    for c in text.chars() {
        let k = kind(c);
        // Other characters each a piece (`-`, `@`, `.`); the others in runs.
        if last == Some(k) && k != Kind::Other {
            out.last_mut().expect("a piece").push(c);
        } else {
            out.push(c.to_string());
        }
        last = Some(k);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Case {
    AsIs,
    Upper,
    Lower,
    Proper,
}

fn apply_case(text: &str, case: Case) -> String {
    match case {
        Case::AsIs => text.to_string(),
        Case::Upper => text.to_uppercase(),
        Case::Lower => text.to_lowercase(),
        Case::Proper => {
            let mut chars = text.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars.flat_map(|c| c.to_lowercase())).collect(),
                None => String::new(),
            }
        }
    }
}

/// One piece of an answer.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Piece {
    /// The `index`th part of cell `column` (from the end if `from_end`).
    Part { column: usize, index: usize, from_end: bool, case: Case },
    /// The whole cell.
    Cell { column: usize, case: Case },
    /// The first character of the `index`th part (an initial).
    Initial { column: usize, index: usize, from_end: bool, case: Case },
    Text(String),
}

impl Piece {
    fn eval(&self, cells: &[Vec<String>], whole: &[String]) -> Option<String> {
        match self {
            Piece::Text(t) => Some(t.clone()),
            Piece::Cell { column, case } => Some(apply_case(whole.get(*column)?, *case)),
            Piece::Initial { column, index, from_end, case } => {
                let parts = cells.get(*column)?;
                let i = if *from_end { parts.len().checked_sub(index + 1)? } else { *index };
                Some(apply_case(&parts.get(i)?.chars().next()?.to_string(), *case))
            }
            Piece::Part { column, index, from_end, case } => {
                let parts = cells.get(*column)?;
                let i = if *from_end { parts.len().checked_sub(index + 1)? } else { *index };
                Some(apply_case(parts.get(i)?, *case))
            }
        }
    }

    /// Lower is simpler: a whole cell, then parts, then text.
    fn cost(&self) -> usize {
        match self {
            Piece::Cell { case, .. } => 1 + usize::from(*case != Case::AsIs),
            Piece::Part { from_end, case, .. } => 2 + usize::from(*from_end) + usize::from(*case != Case::AsIs),
            Piece::Initial { from_end, case, .. } => 3 + usize::from(*from_end) + usize::from(*case != Case::AsIs),
            // Letters and digits of its own are rarely meant: they come from the cells.
            Piece::Text(t) => 2 + t.chars().count() + 10 * t.chars().filter(|c| c.is_alphanumeric()).count(),
        }
    }
}

/// The ways (up to a limit) the pieces of one row make `output`.
fn programs(cells: &[Vec<String>], whole: &[String], output: &str) -> Vec<Vec<Piece>> {
    let chars: Vec<char> = output.chars().collect();
    let n = chars.len();
    // Candidate pieces: what each makes.
    let mut candidates: Vec<(Piece, String)> = Vec::new();
    for case in [Case::AsIs, Case::Upper, Case::Lower, Case::Proper] {
        for (column, text) in whole.iter().enumerate() {
            if !text.is_empty() {
                candidates.push((Piece::Cell { column, case }, apply_case(text, case)));
            }
        }
        for (column, parts) in cells.iter().enumerate() {
            for index in 0..parts.len() {
                for from_end in [false, true] {
                    for piece in [Piece::Part { column, index, from_end, case }, Piece::Initial { column, index, from_end, case }] {
                        if let Some(made) = piece.eval(cells, whole)
                            && !made.is_empty()
                        {
                            candidates.push((piece, made));
                        }
                    }
                }
            }
        }
    }
    // The cheapest ways to make each end of the output (a few each).
    const KEEP: usize = 24;
    let mut best: Vec<Vec<(usize, Vec<Piece>)>> = vec![Vec::new(); n + 1];
    best[0].push((0, Vec::new()));
    for at in 0..n {
        if best[at].is_empty() {
            continue;
        }
        let here = best[at].clone();
        let rest: String = chars[at..].iter().collect();
        let mut steps: Vec<(Piece, usize)> = candidates.iter().filter(|(_, made)| rest.starts_with(made.as_str())).map(|(p, made)| (p.clone(), made.chars().count())).collect();
        // Text of its own: up to the next character a part could start.
        for len in 1..=(n - at).min(8) {
            steps.push((Piece::Text(chars[at..at + len].iter().collect()), len));
        }
        for (piece, len) in steps {
            for (cost, list) in &here {
                let mut list = list.clone();
                let cost = cost + piece.cost();
                // Text after text joins it.
                if let (Piece::Text(t), Some(Piece::Text(last))) = (&piece, list.last_mut()) {
                    last.push_str(t);
                } else {
                    list.push(piece.clone());
                }
                let slot = &mut best[at + len];
                if !slot.iter().any(|(_, l)| *l == list) {
                    slot.push((cost, list));
                    slot.sort_by_key(|(c, _)| *c);
                    slot.truncate(KEEP);
                }
            }
        }
    }
    best[n].iter().map(|(_, l)| l.clone()).collect()
}

/// The answers for `rows` (each row's cells beside the column), learnt
/// from `examples` (the first rows' cells and what was typed for them).
/// `None` when no one way makes every example.
pub(crate) fn flash_fill(examples: &[(Vec<String>, String)], rows: &[Vec<String>]) -> Option<Vec<String>> {
    let (first, rest) = examples.split_first()?;
    let split = |row: &[String]| row.iter().map(|c| tokens(c)).collect::<Vec<_>>();
    let candidates = programs(&split(&first.0), &first.0, &first.1);
    let works = |program: &Vec<Piece>, row: &[String], out: &str| -> bool {
        let cells = split(row);
        program.iter().map(|p| p.eval(&cells, row)).collect::<Option<String>>().as_deref() == Some(out)
    };
    let program = candidates.into_iter().find(|program| {
        // A piece of text only would not use the row at all.
        program.iter().any(|p| !matches!(p, Piece::Text(_))) && rest.iter().all(|(row, out)| works(program, row, out))
    })?;
    Some(
        rows.iter()
            .map(|row| {
                let cells = split(row);
                program.iter().map(|p| p.eval(&cells, row)).collect::<Option<String>>().unwrap_or_default()
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::flash_fill;

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn first_names_from_full_names() {
        let examples = [(row(&["Nancy Davolio"]), "Nancy".to_string())];
        let rows = [row(&["Andrew Fuller"]), row(&["Janet Leverling"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["Andrew", "Janet"]);
    }

    #[test]
    fn last_name_comma_first_initial() {
        let examples = [(row(&["Nancy", "Davolio"]), "DAVOLIO, N.".to_string())];
        let rows = [row(&["Andrew", "Fuller"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["FULLER, A."]);
    }

    #[test]
    fn user_from_email_and_joined_columns() {
        let examples = [(row(&["nancy.davolio@example.com"]), "nancy".to_string())];
        assert_eq!(flash_fill(&examples, &[row(&["a.fuller@example.com"])]).unwrap(), ["a"]);
        let examples = [(row(&["東京都", "千代田区"]), "東京都千代田区".to_string())];
        assert_eq!(flash_fill(&examples, &[row(&["大阪府", "北区"])]).unwrap(), ["大阪府北区"]);
    }

    #[test]
    fn two_examples_pick_the_way_both_fit() {
        // "03" alone could be the third digits or the last; the second
        // example says the last.
        let examples = [(row(&["2026-10-03"]), "03".to_string()), (row(&["2025-1-15"]), "15".to_string())];
        assert_eq!(flash_fill(&examples, &[row(&["2024-12-31"])]).unwrap(), ["31"]);
    }
}

impl crate::shell::editor::SlideEditor {
    /// フラッシュ フィル on the selected cell's column: learnt from the
    /// filled cells of the column in the current region, the empty ones
    /// filled (one step), the number filled told in the status bar.
    pub(crate) fn flash_fill_column(&mut self, cx: &mut gpui_kit::Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};
        self.commit_cell_edit(cx);
        let (Some((column, row)), Some((_, sheet))) = (self.current_cell(), self.current_sheet()) else { return };
        let region = super::goto_special::find(sheet, super::goto_special::Special::CurrentRegion, super::goto_special::Types::ALL, super::goto_special::Scope::All, super::goto_special::TraceLevel::DirectOnly, ibis_doc_model::CellRange { first_row: row, last_row: row, first_column: column, last_column: column }, (column, row), None)[0];
        let sources: Vec<u32> = (region.first_column..=region.last_column).filter(|&c| c != column).collect();
        let text = |r: u32, c: u32| sheet.cell(r, c).map(|x| x.text.clone()).unwrap_or_default();
        let source_row = |r: u32| sources.iter().map(|&c| text(r, c)).collect::<Vec<_>>();
        let rows: Vec<u32> = (region.first_row..=region.last_row).filter(|&r| source_row(r).iter().any(|t| !t.is_empty())).collect();
        let examples: Vec<(u32, Vec<String>, String)> = rows.iter().filter(|&&r| !text(r, column).is_empty()).map(|&r| (r, source_row(r), text(r, column))).collect();
        let targets: Vec<u32> = rows.iter().copied().filter(|&r| text(r, column).is_empty()).collect();
        let learn = |skip: usize| {
            let ex: Vec<(Vec<String>, String)> = examples.iter().skip(skip).map(|(_, s, o)| (s.clone(), o.clone())).collect();
            flash_fill(&ex, &targets.iter().map(|&r| source_row(r)).collect::<Vec<_>>())
        };
        // The first row may be the headings, not an example.
        let filled = if examples.is_empty() || targets.is_empty() { None } else { learn(0).or_else(|| learn(1)) };
        let Some(values) = filled else {
            self.status = Some(crate::shell::i18n::text("_xlsx.flash-fill-could-not-determine-values"));
            return cx.notify();
        };
        let edits: Vec<CellEdit> = targets.iter().zip(&values).filter(|(_, v)| !v.is_empty()).map(|(&r, v)| CellEdit { row: r, column, input: CellInput::Value(CellValue::Text(v.clone())) }).collect();
        let count = edits.len();
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })]) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
            return cx.notify();
        }
        self.status = Some(crate::shell::i18n::format("flash-fill.done", &[("count", &count)]).into());
        cx.notify();
    }
}
