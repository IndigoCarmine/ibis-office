// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Structured references (`Table1[Column]`, `[@Column]`,
//! `Table1[[#Headers],[A]:[C]]`): the cells of a table they name
//! (ECMA-376 Part 1, 18.17.6.3 and Excel's documentation of structured
//! references).

use super::TableInfo;
use crate::value::CellError;

/// The rows a structured reference's special items take.
#[derive(Clone, Copy, PartialEq)]
enum Rows {
    All,
    Data,
    Headers,
    Totals,
    ThisRow,
}

/// The area (`first_row`, `first_column`, `last_row`, `last_column`) the
/// brackets `spec` (as written, with their outer brackets) name in
/// `table`, for a formula in `row`.
pub fn area(table: &TableInfo, spec: &str, row: u32) -> Result<(u32, u32, u32, u32), CellError> {
    let inner = spec.trim().strip_prefix('[').and_then(|s| s.strip_suffix(']')).ok_or(CellError::Ref)?;
    let mut rows: Vec<Rows> = Vec::new();
    let mut columns: Option<(usize, usize)> = None;
    let mut items: Vec<String> = Vec::new();
    let inner = inner.trim();
    if let Some(rest) = inner.strip_prefix('@') {
        // `[@]`, `[@Column]`, `[@[Column]]`, `[@[A]:[B]]`.
        rows.push(Rows::ThisRow);
        let rest = rest.trim();
        if !rest.is_empty() {
            items.push(if rest.starts_with('[') { rest.to_string() } else { format!("[{rest}]") });
        }
    } else if inner.starts_with('[') {
        items = split_items(inner)?;
    } else if !inner.is_empty() {
        items.push(format!("[{inner}]"));
    }
    for item in items {
        let item = item.trim();
        if let Some(special) = special(item) {
            rows.push(special);
            continue;
        }
        // `[A]` or `[A]:[B]`.
        let (a, b) = match split_range(item) {
            Some((a, b)) => (a, b),
            None => (item.to_string(), item.to_string()),
        };
        let index = |name: &str| -> Result<usize, CellError> {
            let name = unescape(name.trim().strip_prefix('[').and_then(|s| s.strip_suffix(']')).ok_or(CellError::Ref)?);
            table.columns.iter().position(|c| c.trim().eq_ignore_ascii_case(name.trim())).ok_or(CellError::Ref)
        };
        let (i, j) = (index(&a)?, index(&b)?);
        let (i, j) = (i.min(j), i.max(j));
        columns = Some(match columns {
            Some((x, y)) => (x.min(i), y.max(j)),
            None => (i, j),
        });
    }
    let (c0, c1) = match columns {
        Some((i, j)) => (table.first_column + i as u32, table.first_column + j as u32),
        None => (table.first_column, table.last_column),
    };
    let header_end = table.first_row + table.header_rows;
    let totals_start = table.last_row + 1 - table.totals_rows.min(table.last_row + 1 - table.first_row);
    let data = (header_end, totals_start.saturating_sub(1));
    let span = |r: Rows| -> Result<(u32, u32), CellError> {
        Ok(match r {
            Rows::All => (table.first_row, table.last_row),
            Rows::Data => data,
            Rows::Headers if table.header_rows > 0 => (table.first_row, header_end - 1),
            Rows::Totals if table.totals_rows > 0 => (totals_start, table.last_row),
            Rows::ThisRow if row >= data.0 && row <= data.1 => (row, row),
            _ => return Err(if r == Rows::ThisRow { CellError::Value } else { CellError::Ref }),
        })
    };
    let (r0, r1) = if rows.is_empty() {
        data
    } else {
        let mut out: Option<(u32, u32)> = None;
        for r in rows {
            let (a, b) = span(r)?;
            out = Some(match out {
                Some((x, y)) => (x.min(a), y.max(b)),
                None => (a, b),
            });
        }
        out.unwrap_or(data)
    };
    if r1 < r0 {
        return Err(CellError::Ref);
    }
    Ok((r0, c0, r1, c1))
}

fn special(item: &str) -> Option<Rows> {
    let name = item.strip_prefix('[')?.strip_suffix(']')?.trim();
    Some(match name.to_ascii_lowercase().as_str() {
        "#all" => Rows::All,
        "#data" => Rows::Data,
        "#headers" => Rows::Headers,
        "#totals" => Rows::Totals,
        "#this row" => Rows::ThisRow,
        _ => return None,
    })
}

/// The top-level bracketed items of `[A],[B]:[C]` (`'` escapes a bracket).
fn split_items(s: &str) -> Result<Vec<String>, CellError> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                cur.push(c);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            '[' => {
                depth += 1;
                cur.push(c);
            }
            ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if depth != 0 {
        return Err(CellError::Ref);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    Ok(out)
}

/// `[A]:[B]` as its two ends.
fn split_range(s: &str) -> Option<(String, String)> {
    let mut depth = 0;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\'' => escaped = true,
            '[' => depth += 1,
            ']' => depth -= 1,
            ':' if depth == 0 => return Some((s[..i].to_string(), s[i + 1..].to_string())),
            _ => {}
        }
    }
    None
}

/// A column name as written, its `'` escapes removed.
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> TableInfo {
        // A1:C6: a header row, four data rows, a totals row.
        TableInfo { sheet: 0, first_row: 0, first_column: 0, last_row: 5, last_column: 2, header_rows: 1, totals_rows: 1, columns: vec!["Item".into(), "Qty".into(), "Price [JPY]".into()] }
    }

    #[test]
    fn columns_and_special_items() {
        let t = table();
        assert_eq!(area(&t, "[Qty]", 0), Ok((1, 1, 4, 1)));
        assert_eq!(area(&t, "[]", 0), Ok((1, 0, 4, 2)));
        assert_eq!(area(&t, "[#All]", 0), Ok((0, 0, 5, 2)));
        assert_eq!(area(&t, "[#Headers]", 0), Ok((0, 0, 0, 2)));
        assert_eq!(area(&t, "[#Totals]", 0), Ok((5, 0, 5, 2)));
        assert_eq!(area(&t, "[[#Headers],[Qty]]", 0), Ok((0, 1, 0, 1)));
        assert_eq!(area(&t, "[[#Data],[#Totals],[Item]:[Qty]]", 0), Ok((1, 0, 5, 1)));
        assert_eq!(area(&t, "[[Price '[JPY']]]", 0), Ok((1, 2, 4, 2)));
        assert_eq!(area(&t, "[qty]", 0), Ok((1, 1, 4, 1)));
        assert_eq!(area(&t, "[Nope]", 0), Err(CellError::Ref));
    }

    #[test]
    fn this_row() {
        let t = table();
        assert_eq!(area(&t, "[@Qty]", 3), Ok((3, 1, 3, 1)));
        assert_eq!(area(&t, "[@[Price '[JPY']]]", 2), Ok((2, 2, 2, 2)));
        assert_eq!(area(&t, "[@]", 2), Ok((2, 0, 2, 2)));
        assert_eq!(area(&t, "[[#This Row],[Item]]", 4), Ok((4, 0, 4, 0)));
        // Outside the data rows: #VALUE!.
        assert_eq!(area(&t, "[@Qty]", 0), Err(CellError::Value));
    }
}
