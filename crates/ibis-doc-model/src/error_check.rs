// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! エラー チェック: the cells Excel marks with a green triangle at their
//! top left, and the errors a person tells it to ignore
//! (`ignoredErrors`, ECMA-376 Part 1 §18.3.1.51).
//!
//! The rules checked: 数値が文字列として保存されている (a text constant that
//! reads as a number), 数式にエラーがある (a formula's result is an error
//! value) and 不整合な数式 (a formula unlike the ones above and below it,
//! or left and right of it, which agree with each other).

use crate::sheet::{CellRange, CellValue, Sheet, SheetCell};

/// A kind of error a cell can be marked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// 数値が文字列として保存されています (`numberStoredAsText`).
    NumberAsText,
    /// この数式の結果はエラーです (`evalError`).
    EvalError,
    /// 不整合な数式 (`formula`).
    InconsistentFormula,
}

impl ErrorKind {
    /// The attribute of `ignoredError` that ignores it.
    pub fn attribute(self) -> &'static str {
        match self {
            Self::NumberAsText => "numberStoredAsText",
            Self::EvalError => "evalError",
            Self::InconsistentFormula => "formula",
        }
    }

    pub fn from_attribute(name: &str) -> Option<Self> {
        [Self::NumberAsText, Self::EvalError, Self::InconsistentFormula].into_iter().find(|k| k.attribute() == name)
    }
}

/// Errors ignored in cells (`ignoredError`): the ranges, and the kinds
/// (attributes that are 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IgnoredError {
    pub ranges: Vec<CellRange>,
    /// The attributes set, such as `numberStoredAsText`; kinds this program
    /// does not check (`formulaRange`, `unlockedFormula`, …) are kept too.
    pub attributes: Vec<String>,
}

impl IgnoredError {
    fn covers(&self, row: u32, column: u32) -> bool {
        self.ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column))
    }
}

fn ignored(sheet: &Sheet, row: u32, column: u32, kind: ErrorKind) -> bool {
    sheet.ignored_errors.iter().any(|i| i.covers(row, column) && i.attributes.iter().any(|a| a == kind.attribute()))
}

/// Whether the text of a text constant reads as a number (not a date or a
/// time), as typing it would: it is stored as text, which Excel flags.
fn reads_as_number(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    match ibis_calc::input::parse_input(t, false) {
        ibis_calc::input::Typed::Value(CellValue::Number(_), format) => !format.is_some_and(ibis_calc::numfmt::is_date_format),
        _ => false,
    }
}

/// The formula of the cell at (row, column), as it would be in (to_row, to_column).
fn formula_moved(sheet: &Sheet, row: u32, column: u32, to_row: u32, to_column: u32) -> Option<String> {
    let f = sheet.cell(row, column)?.formula.as_deref()?;
    Some(ibis_calc::formula::shift(f, to_row as i64 - row as i64, to_column as i64 - column as i64))
}

fn inconsistent(sheet: &Sheet, cell: &SheetCell) -> bool {
    let (row, column) = (cell.row, cell.column);
    let Some(own) = cell.formula.as_deref() else { return false };
    let pairs = [(row.checked_sub(1).map(|r| (r, column)), Some((row + 1, column))), (column.checked_sub(1).map(|c| (row, c)), Some((row, column + 1)))];
    pairs.into_iter().any(|(before, after)| {
        let (Some((br, bc)), Some((ar, ac))) = (before, after) else { return false };
        match (formula_moved(sheet, br, bc, row, column), formula_moved(sheet, ar, ac, row, column)) {
            (Some(a), Some(b)) => a == b && a != own,
            _ => false,
        }
    })
}

/// The errors of the cell at (row, column) that are not ignored.
pub fn errors_at(sheet: &Sheet, row: u32, column: u32) -> Vec<ErrorKind> {
    let Some(cell) = sheet.cell(row, column) else { return Vec::new() };
    errors_of(sheet, cell)
}

fn errors_of(sheet: &Sheet, cell: &SheetCell) -> Vec<ErrorKind> {
    let mut out = Vec::new();
    let mut add = |kind: ErrorKind, found: bool| {
        if found && !ignored(sheet, cell.row, cell.column, kind) {
            out.push(kind);
        }
    };
    add(ErrorKind::NumberAsText, cell.formula.is_none() && matches!(&cell.value, CellValue::Text(t) if reads_as_number(t)));
    add(ErrorKind::EvalError, cell.formula.is_some() && matches!(cell.value, CellValue::Error(_)));
    add(ErrorKind::InconsistentFormula, inconsistent(sheet, cell));
    out
}

/// The cells among `cells` (of `sheet`) with an error that is not ignored,
/// with their first error, in order.
pub fn flagged_among(sheet: &Sheet, cells: &[SheetCell]) -> Vec<(u32, u32, ErrorKind)> {
    cells.iter().filter_map(|c| errors_of(sheet, c).first().map(|&k| (c.row, c.column, k))).collect()
}

/// Every cell with an error that is not ignored, with its first error, in order.
pub fn flagged_cells(sheet: &Sheet) -> Vec<(u32, u32, ErrorKind)> {
    sheet.cells.iter().filter_map(|c| errors_of(sheet, c).first().map(|&k| (c.row, c.column, k))).collect()
}

/// The message Excel shows for an error.
pub fn message(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NumberAsText => "このセルの数値は、文字列形式で保存されています",
        ErrorKind::EvalError => "このセルの数式の結果はエラーです",
        ErrorKind::InconsistentFormula => "この数式は、周囲の数式と異なっています",
    }
}

impl Sheet {
    /// Ignore errors of `kind` in `range` from now on.
    pub fn ignore_errors(&mut self, range: CellRange, kind: ErrorKind) {
        let attribute = kind.attribute().to_string();
        match self.ignored_errors.iter_mut().find(|i| i.ranges == [range]) {
            Some(i) => {
                if !i.attributes.contains(&attribute) {
                    i.attributes.push(attribute);
                }
            }
            None => self.ignored_errors.push(IgnoredError { ranges: vec![range], attributes: vec![attribute] }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::{CellKind, SheetCell};

    fn cell(row: u32, column: u32, value: CellValue, formula: Option<&str>) -> SheetCell {
        let text = match &value {
            CellValue::Text(t) => t.clone(),
            CellValue::Number(n) => n.to_string(),
            _ => String::new(),
        };
        SheetCell { runs: None, phonetic: None, row, column, value, text, kind: CellKind::Text, formula: formula.map(str::to_string), style: 0 }
    }

    fn sheet(cells: Vec<SheetCell>) -> Sheet {
        let mut s = Sheet::default();
        for c in cells {
            s.put_cell(c.row, c.column, Some(c));
        }
        s
    }

    #[test]
    fn numbers_stored_as_text_errors_and_inconsistent_formulas_are_marked() {
        let mut s = sheet(vec![
            cell(0, 0, CellValue::Text("123".into()), None),
            cell(1, 0, CellValue::Text("abc".into()), None),
            cell(2, 0, CellValue::Text("2026/1/5".into()), None),
            cell(3, 0, CellValue::Number(5.0), None),
            cell(0, 1, CellValue::Number(1.0), Some("A1+1")),
            cell(1, 1, CellValue::Number(2.0), Some("A2+1")),
            cell(2, 1, CellValue::Number(9.0), Some("A3*2")),
            cell(3, 1, CellValue::Number(4.0), Some("A4+1")),
            cell(0, 2, CellValue::Error(crate::sheet::CellError::Div0), Some("1/0")),
        ]);
        let flagged = flagged_cells(&s);
        assert_eq!(flagged, vec![(0, 0, ErrorKind::NumberAsText), (0, 2, ErrorKind::EvalError), (2, 1, ErrorKind::InconsistentFormula)]);
        // Ignored in a range: gone there.
        s.ignore_errors(CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 2 }, ErrorKind::NumberAsText);
        s.ignore_errors(CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 2 }, ErrorKind::EvalError);
        assert_eq!(flagged_cells(&s), vec![(2, 1, ErrorKind::InconsistentFormula)]);
        assert_eq!(s.ignored_errors.len(), 1, "one entry with both kinds");
    }
}
