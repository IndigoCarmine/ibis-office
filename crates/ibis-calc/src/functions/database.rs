// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Database functions (DSUM and the like): a table with a header row, a
//! field, and a criteria range (conditions in a row all hold; any row does).

use super::stat::{Criterion, mean, parse_criterion, variance};
use super::{Func, arg_array, err, number};
use crate::eval::{Array, Ctx, Value, to_number};
use crate::formula::Expr;
use crate::value::CellError;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "DSUM" => |c, a| db(c, a, |v, _| Ok(super::math::kahan(&v))),
        "DAVERAGE" => |c, a| db(c, a, |v, _| mean(&v)),
        "DCOUNT" => |c, a| db(c, a, |v, _| Ok(v.len() as f64)),
        "DCOUNTA" => |c, a| db(c, a, |_, all| Ok(all.iter().filter(|v| !matches!(v, Value::Empty)).count() as f64)),
        "DMAX" => |c, a| db(c, a, |v, _| Ok(v.iter().copied().fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x)))).unwrap_or(0.0))),
        "DMIN" => |c, a| db(c, a, |v, _| Ok(v.iter().copied().fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x)))).unwrap_or(0.0))),
        "DPRODUCT" => |c, a| db(c, a, |v, _| Ok(if v.is_empty() { 0.0 } else { v.iter().product() })),
        "DSTDEV" => |c, a| db(c, a, |v, _| variance(&v, true).map(f64::sqrt)),
        "DSTDEVP" => |c, a| db(c, a, |v, _| variance(&v, false).map(f64::sqrt)),
        "DVAR" => |c, a| db(c, a, |v, _| variance(&v, true)),
        "DVARP" => |c, a| db(c, a, |v, _| variance(&v, false)),
        "DGET" => |c, a| {
            let (table, field, rows) = match select(c, a) {
                Ok(x) => x,
                Err(e) => return err(e),
            };
            let Some(field) = field else { return err(CellError::Value) };
            match rows.as_slice() {
                [] => err(CellError::Value),
                [r] => match table.get(*r, field) {
                    Value::Empty => number(0.0),
                    v => v.clone(),
                },
                _ => err(CellError::Num),
            }
        },
        _ => return None,
    })
}

/// The table, the field's column (none: whole rows), and the rows (below
/// the header) that meet the criteria.
fn select(c: &mut Ctx, a: &[Expr]) -> Result<(std::rc::Rc<Array>, Option<usize>, Vec<usize>), CellError> {
    let table = arg_array(c, &a[0])?;
    let criteria = arg_array(c, &a[2])?;
    let field = match &a[1] {
        Expr::Missing => None,
        e => match c.eval_scalar(e) {
            Value::Error(e) => return Err(e),
            Value::Number(n) => {
                let i = n.trunc();
                if i < 1.0 || i as usize > table.columns {
                    return Err(CellError::Value);
                }
                Some(i as usize - 1)
            }
            Value::Text(t) => Some(column_named(&table, &t).ok_or(CellError::Value)?),
            _ => return Err(CellError::Value),
        },
    };
    if criteria.rows < 1 || table.rows < 1 {
        return Err(CellError::Value);
    }
    // Each criteria row: the conditions, by table column.
    let date1904 = c.date1904();
    let mut conditions: Vec<Vec<(usize, Criterion, bool)>> = Vec::new();
    for r in 1..criteria.rows {
        let mut row = Vec::new();
        for col in 0..criteria.columns {
            let v = criteria.get(r, col);
            if matches!(v, Value::Empty) {
                continue;
            }
            let Value::Text(header) = criteria.get(0, col) else { continue };
            let Some(tc) = column_named(&table, header) else { return Err(CellError::Value) };
            // Text without an operator matches the start of the value.
            let starts = matches!(v, Value::Text(t) if !t.starts_with(['=', '<', '>']));
            row.push((tc, parse_criterion(v, date1904), starts));
        }
        conditions.push(row);
    }
    let matches_row = |r: usize| {
        conditions.is_empty()
            || conditions.iter().any(|row| {
                row.iter().all(|(col, crit, starts)| {
                    let v = table.get(r, *col);
                    match (crit, starts) {
                        (Criterion::Text(std::cmp::Ordering::Equal, false, t), true) => match v {
                            Value::Text(s) => super::wildcard_match(&format!("{t}*"), s),
                            _ => false,
                        },
                        _ => crit.matches(v, date1904),
                    }
                })
            })
    };
    let rows: Vec<usize> = (1..table.rows).filter(|&r| matches_row(r)).collect();
    Ok((table, field, rows))
}

fn column_named(table: &Array, name: &str) -> Option<usize> {
    (0..table.columns).find(|&c| matches!(table.get(0, c), Value::Text(h) if h.to_lowercase() == name.to_lowercase()))
}

fn db(c: &mut Ctx, a: &[Expr], f: fn(Vec<f64>, Vec<Value>) -> Result<f64, CellError>) -> Value {
    let (table, field, rows) = match select(c, a) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    let mut nums = Vec::new();
    let mut all = Vec::new();
    for r in rows {
        match field {
            Some(col) => {
                let v = table.get(r, col).clone();
                if let Value::Number(n) = v {
                    nums.push(n);
                }
                all.push(v);
            }
            None => {
                // No field: DCOUNT counts the rows, DCOUNTA the non-empty rows.
                all.push(Value::Number(1.0));
                nums.push(1.0);
            }
        }
    }
    let _ = to_number;
    f(nums, all).map_or_else(err, number)
}
