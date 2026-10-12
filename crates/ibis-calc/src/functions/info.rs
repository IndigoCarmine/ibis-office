// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Information functions.

use super::{Func, arg_num, err, number};
use crate::eval::{Ctx, Value, to_number};
use crate::formula::Expr;
use crate::value::CellError;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "NA" => |_, _| err(CellError::NA),
        "ISBLANK" => |c, a| is(c, a, |v| matches!(v, Value::Empty)),
        "ISERR" => |c, a| is(c, a, |v| matches!(v, Value::Error(e) if *e != CellError::NA)),
        "ISERROR" => |c, a| is(c, a, |v| matches!(v, Value::Error(_))),
        "ISNA" => |c, a| is(c, a, |v| matches!(v, Value::Error(CellError::NA))),
        "ISLOGICAL" => |c, a| is(c, a, |v| matches!(v, Value::Bool(_))),
        "ISNUMBER" => |c, a| is(c, a, |v| matches!(v, Value::Number(_))),
        "ISTEXT" => |c, a| is(c, a, |v| matches!(v, Value::Text(_))),
        "ISNONTEXT" => |c, a| is(c, a, |v| !matches!(v, Value::Text(_))),
        "ISEVEN" => |c, a| parity(c, a, true),
        "ISODD" => |c, a| parity(c, a, false),
        "ISREF" => |c, a| Value::Bool(matches!(c.eval(&a[0]), Value::Ref(_))),
        "ISOMITTED" => |c, a| Value::Bool(matches!(a[0], Expr::Missing) || matches!(c.eval(&a[0]), Value::Omitted)),
        "ISFORMULA" => |c, a| match c.eval(&a[0]) {
            Value::Ref(r) => {
                let area = r[0];
                Value::Bool(c.book.formula(area.sheet, area.first_row, area.first_column).is_some())
            }
            Value::Error(e) => err(e),
            _ => err(CellError::Value),
        },
        "N" => |c, a| {
            let v = match c.eval(&a[0]) {
                Value::Ref(r) if !r.is_empty() => {
                    let area = r[0];
                    c.cell_value(area.sheet, area.first_row, area.first_column)
                }
                Value::Ref(_) => return err(CellError::Ref),
                Value::Array(a) => a.data.first().cloned().unwrap_or(Value::Empty),
                v => v,
            };
            match v {
                Value::Number(n) => number(n),
                Value::Bool(b) => number(if b { 1.0 } else { 0.0 }),
                Value::Error(e) => err(e),
                _ => number(0.0),
            }
        },
        "TYPE" => |c, a| {
            let v = match c.eval(&a[0]) {
                Value::Ref(r) if r.len() > 1 => return err(CellError::Value),
                Value::Ref(r) => c.deref_implicit(&r),
                v => v,
            };
            number(match v {
                Value::Number(_) | Value::Empty | Value::Omitted => 1.0,
                Value::Text(_) => 2.0,
                Value::Bool(_) => 4.0,
                Value::Error(_) => 16.0,
                Value::Array(_) => 64.0,
                Value::Lambda(_) => 128.0,
                Value::Ref(_) => 64.0,
            })
        },
        "ERROR.TYPE" => |c, a| match c.eval_scalar(&a[0]) {
            Value::Error(e) => e.type_number().map_or(err(CellError::NA), number),
            _ => err(CellError::NA),
        },
        "SHEET" => |c, a| {
            if a.is_empty() {
                return number(c.sheet as f64 + 1.0);
            }
            match c.eval(&a[0]) {
                Value::Ref(r) => number(r[0].sheet as f64 + 1.0),
                Value::Text(t) => match c.book.sheet_index(&t) {
                    Some(i) => number(i as f64 + 1.0),
                    None => err(CellError::NA),
                },
                Value::Error(e) => err(e),
                _ => err(CellError::NA),
            }
        },
        "SHEETS" => |c, a| {
            if a.is_empty() {
                return number(c.book.sheet_count() as f64);
            }
            match c.eval(&a[0]) {
                Value::Ref(r) => number(r.iter().map(|x| x.last_sheet - x.sheet + 1).sum::<usize>() as f64),
                Value::Error(e) => err(e),
                _ => err(CellError::NA),
            }
        },
        "INFO" => |c, a| {
            let t = match super::arg_text(c, a, 0) {
                Ok(t) => t.to_lowercase(),
                Err(e) => return err(e),
            };
            match t.as_str() {
                "numfile" => number(c.book.sheet_count() as f64),
                "recalc" => Value::Text("自動".into()),
                "system" => Value::Text("mac".into()),
                "memavail" | "memused" | "totmem" => err(CellError::NA),
                "origin" => Value::Text("$A:$A$1".into()),
                "directory" | "osversion" | "release" => {
                    c.unsupported = true;
                    err(CellError::NA)
                }
                _ => err(CellError::Value),
            }
        },
        "CELL" => cell,
        _ => return None,
    })
}

fn is(c: &mut Ctx, a: &[Expr], f: fn(&Value) -> bool) -> Value {
    let v = c.eval_scalar_or_array(&a[0]);
    c.map1(v, |_, v| Value::Bool(f(&v)))
}

fn parity(c: &mut Ctx, a: &[Expr], even: bool) -> Value {
    let v = c.eval_scalar_or_array(&a[0]);
    let date1904 = c.date1904();
    c.map1(v, |_, v| {
        if let Value::Bool(_) = v {
            return err(CellError::Value);
        }
        match to_number(&v, date1904) {
            Ok(n) => Value::Bool((n.trunc() % 2.0 == 0.0) == even),
            Err(e) => err(e),
        }
    })
}

fn cell(c: &mut Ctx, a: &[Expr]) -> Value {
    let what = match super::arg_text(c, a, 0) {
        Ok(t) => t.to_lowercase(),
        Err(e) => return err(e),
    };
    let area = match a.get(1).map(|e| c.eval(e)) {
        Some(Value::Ref(r)) => r[0],
        Some(Value::Error(e)) => return err(e),
        Some(_) => return err(CellError::Value),
        None => crate::eval::Area::cell(c.sheet, c.row, c.column),
    };
    let _ = arg_num;
    match what.as_str() {
        "row" => number(area.first_row as f64 + 1.0),
        "col" => number(area.first_column as f64 + 1.0),
        "address" => {
            let r = crate::formula::AreaRef {
                sheet: None,
                start: crate::formula::Corner { row: area.first_row, column: area.first_column, row_fixed: true, column_fixed: true },
                end: crate::formula::Corner { row: area.first_row, column: area.first_column, row_fixed: true, column_fixed: true },
                whole_columns: false,
                whole_rows: false,
            };
            let mut text = crate::formula::write_reference(&r);
            if area.sheet != c.sheet {
                text = format!("{}!{text}", crate::formula::quote_sheet(c.book.sheet_name(area.sheet)));
            }
            Value::Text(text)
        }
        "contents" => match c.cell_value(area.sheet, area.first_row, area.first_column) {
            Value::Empty => number(0.0),
            v => v,
        },
        "type" => Value::Text(
            match c.cell_value(area.sheet, area.first_row, area.first_column) {
                Value::Empty => "b",
                Value::Text(_) => "l",
                _ => "v",
            }
            .into(),
        ),
        "color" | "filename" | "format" | "parentheses" | "prefix" | "protect" | "width" => {
            // Valid options not known to this engine keep imported caches.
            c.unsupported = true;
            err(CellError::Value)
        }
        _ => err(CellError::Value),
    }
}
