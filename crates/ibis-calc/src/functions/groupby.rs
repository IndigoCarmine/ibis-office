// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GROUPBY and PIVOTBY: values grouped by the values of row fields (and,
//! for PIVOTBY, of column fields), each group given to a function (SUM, a
//! LAMBDA…), as a spilled table with totals. Written from Excel's output
//! on the coverage tests (the totals' label is Excel's, in Japanese).

use std::cmp::Ordering;
use std::rc::Rc;

use super::{Func, arg_array, array_value, err};
use crate::eval::{Array, Ctx, Value, compare, to_text};
use crate::formula::Expr;
use crate::value::CellError;

type Res<T> = Result<T, CellError>;

const TOTAL: &str = "合計";
const GRAND_TOTAL: &str = "総計";

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "GROUPBY" => groupby,
        "PIVOTBY" => pivotby,
        _ => return None,
    })
}

// ----- Order and equality of keys ------------------------------------------

fn rank(v: &Value) -> u8 {
    match v {
        Value::Number(_) => 0,
        Value::Text(_) => 1,
        Value::Bool(_) => 2,
        Value::Error(_) => 3,
        _ => 4,
    }
}

/// Excel's order: numbers, text, logical values, errors (by kind), blanks.
fn order(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Error(x), Value::Error(y)) => x.type_number().unwrap_or(f64::INFINITY).total_cmp(&y.type_number().unwrap_or(f64::INFINITY)),
        _ if rank(a) != rank(b) => rank(a).cmp(&rank(b)),
        (Value::Empty | Value::Omitted, _) => Ordering::Equal,
        _ => compare(a, b),
    }
}

fn same(a: &Value, b: &Value) -> bool {
    order(a, b) == Ordering::Equal
}

fn same_keys(a: &[Value], b: &[Value]) -> bool {
    a.iter().zip(b).all(|(x, y)| same(x, y))
}

// ----- Arguments ------------------------------------------------------------

fn given(a: &[Expr], i: usize) -> Option<&Expr> {
    a.get(i).filter(|e| !matches!(e, Expr::Missing))
}

/// An integer option: `None` when left out; text, logical values, blanks and fractions are `#VALUE!`.
fn int_option(c: &mut Ctx, a: &[Expr], i: usize) -> Res<Option<i64>> {
    let Some(e) = given(a, i) else { return Ok(None) };
    match c.eval_scalar(e) {
        Value::Number(n) if n.fract() == 0.0 && n.abs() < 1e9 => Ok(Some(n as i64)),
        Value::Error(e) => Err(e),
        _ => Err(CellError::Value),
    }
}

/// The columns a sort goes by (1-based, negative for descending): a number or an array of them.
fn sort_option(c: &mut Ctx, a: &[Expr], i: usize, columns: usize) -> Res<Option<Vec<i64>>> {
    let Some(e) = given(a, i) else { return Ok(None) };
    let arr = arg_array(c, e)?;
    let mut out = Vec::new();
    for v in &arr.data {
        match v {
            Value::Number(n) if n.fract() == 0.0 && *n != 0.0 && (n.abs() as usize) <= columns => out.push(*n as i64),
            Value::Error(e) => return Err(*e),
            _ => return Err(CellError::Value),
        }
    }
    Ok(Some(out))
}

/// The rows to take: logical values or numbers (not 0), one for each, or one for all.
fn filter_option(c: &mut Ctx, a: &[Expr], i: usize, rows: usize) -> Res<Vec<bool>> {
    let Some(e) = given(a, i) else { return Ok(vec![true; rows]) };
    let arr = arg_array(c, e)?;
    let one = |v: &Value| -> Res<bool> {
        match v {
            Value::Number(n) => Ok(*n != 0.0),
            Value::Bool(b) => Ok(*b),
            Value::Error(e) => Err(*e),
            _ => Err(CellError::Value),
        }
    };
    if arr.data.len() == 1 {
        return Ok(vec![one(&arr.data[0])?; rows]);
    }
    if arr.columns != 1 || arr.rows != rows {
        return Err(CellError::Value);
    }
    arr.data.iter().map(one).collect()
}

/// The function to give each group to.
fn function_arg(c: &mut Ctx, e: &Expr) -> Res<Value> {
    match c.eval(e) {
        f @ Value::Lambda(_) => Ok(f),
        Value::Error(e) => Err(e),
        _ => Err(CellError::Value),
    }
}

/// `f` of one column of values.
fn aggregate(c: &mut Ctx, f: &Value, column: Vec<Value>) -> Value {
    let n = column.len().max(1);
    let arg = Value::Array(Rc::new(Array::new(n, 1, if column.is_empty() { vec![Value::Empty] } else { column })));
    if let Value::Lambda(l) = f
        && l.eta.is_none()
        && l.params.len() != 1
    {
        return Value::Error(CellError::Value);
    }
    match c.invoke_values(f, vec![arg]) {
        Value::Array(x) if x.data.len() == 1 => x.data[0].clone(),
        Value::Array(_) => Value::Error(CellError::Calc),
        Value::Ref(_) => Value::Error(CellError::Value),
        v => v,
    }
}

fn column_of(arr: &Array, col: usize, rows: &[usize]) -> Vec<Value> {
    rows.iter().map(|&r| arr.get(r, col).clone()).collect()
}

/// A total depth: 0 none, 1 at the end, -1 at the start; 2 and -2 add subtotals, with more than one field.
fn total_depth(c: &mut Ctx, a: &[Expr], i: usize, fields: usize) -> Res<i64> {
    let d = int_option(c, a, i)?.unwrap_or(1);
    match d {
        0 | 1 | -1 => Ok(d),
        2 | -2 if fields >= 2 => Ok(d),
        _ => Err(CellError::Value),
    }
}

/// Groups of rows by their keys, in the order of their first row.
fn group_rows(keys: &Array, rows: &[usize]) -> Vec<(Vec<Value>, Vec<usize>)> {
    let mut groups: Vec<(Vec<Value>, Vec<usize>)> = Vec::new();
    for &r in rows {
        let key: Vec<Value> = (0..keys.columns).map(|k| keys.get(r, k).clone()).collect();
        match groups.iter_mut().find(|(k, _)| same_keys(k, &key)) {
            Some((_, members)) => members.push(r),
            None => groups.push((key, vec![r])),
        }
    }
    groups
}

fn by_keys(a: &[Value], b: &[Value]) -> Ordering {
    a.iter().zip(b).map(|(x, y)| order(x, y)).find(|o| *o != Ordering::Equal).unwrap_or(Ordering::Equal)
}

/// A sort of columns of values (negative: down, the blanks and errors still last).
fn by_columns(spec: &[i64], a: &[Value], b: &[Value]) -> Ordering {
    for &s in spec {
        let col = s.unsigned_abs() as usize - 1;
        let o = order(&a[col], &b[col]);
        let o = if s < 0 && !matches!((&a[col], &b[col]), (Value::Empty | Value::Error(_), _) | (_, Value::Empty | Value::Error(_))) { o.reverse() } else { o };
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// A header's text: a number as text.
fn header_text(v: &Value) -> Value {
    match v {
        Value::Number(_) => to_text(v).map(Value::Text).unwrap_or_else(|e| Value::Error(e)),
        other => other.clone(),
    }
}

/// How the header rows of the data are taken: not there, there and not shown, generated, or there and shown.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Headers {
    None,
    Hidden,
    Generated,
    Shown,
}

fn headers_option(c: &mut Ctx, a: &[Expr], i: usize, first_row_is_text: impl Fn() -> bool) -> Res<Headers> {
    Ok(match int_option(c, a, i)? {
        None => if first_row_is_text() { Headers::Shown } else { Headers::None },
        Some(0) => Headers::None,
        Some(1) => Headers::Hidden,
        Some(2) => Headers::Generated,
        Some(3) => Headers::Shown,
        _ => return Err(CellError::Value),
    })
}

// ----- GROUPBY --------------------------------------------------------------

fn groupby(c: &mut Ctx, a: &[Expr]) -> Value {
    match groupby_of(c, a) {
        Ok(v) => v,
        Err(e) => err(e),
    }
}

fn groupby_of(c: &mut Ctx, a: &[Expr]) -> Res<Value> {
    let rf = arg_array(c, &a[0])?;
    let vals = arg_array(c, &a[1])?;
    let f = function_arg(c, &a[2])?;
    if rf.rows != vals.rows {
        return Err(CellError::Value);
    }
    let (k, v) = (rf.columns, vals.columns);
    let text_row = || rf.rows >= 2 && (0..v).all(|j| matches!(vals.get(0, j), Value::Text(_))) && (0..v).all(|j| !matches!(vals.get(1, j), Value::Text(_)));
    let headers = headers_option(c, a, 3, text_row)?;
    let depth = total_depth(c, a, 4, k)?;
    let spec = sort_option(c, a, 5, k + v)?;
    let filter = filter_option(c, a, 6, rf.rows)?;
    let table = match int_option(c, a, 7)? {
        None | Some(0) => false,
        Some(1) => true,
        _ => return Err(CellError::Value),
    };
    // Subtotals are of a hierarchy only.
    if table && depth.abs() == 2 {
        return Err(CellError::Value);
    }
    let skip = usize::from(matches!(headers, Headers::Hidden | Headers::Shown));
    let rows: Vec<usize> = (skip..rf.rows).filter(|&r| filter[r]).collect();
    if rows.is_empty() {
        return Err(CellError::Calc);
    }
    let mut groups = group_rows(&rf, &rows);
    // Each group's values.
    let mut table_rows: Vec<(Vec<Value>, Vec<Value>, Vec<usize>)> = Vec::new();
    for (key, members) in groups.drain(..) {
        let values: Vec<Value> = (0..v).map(|j| aggregate(c, &f, column_of(&vals, j, &members))).collect();
        table_rows.push((key, values, members));
    }
    match &spec {
        Some(spec) => table_rows.sort_by(|x, y| {
            let (p, q): (Vec<Value>, Vec<Value>) = (x.0.iter().chain(&x.1).cloned().collect(), y.0.iter().chain(&y.1).cloned().collect());
            by_columns(spec, &p, &q)
        }),
        None => table_rows.sort_by(|x, y| by_keys(&x.0, &y.0)),
    }
    let mut out: Vec<Vec<Value>> = Vec::new();
    match headers {
        Headers::Generated => {
            let mut row: Vec<Value> = (1..=k).map(|i| Value::Text(format!("行フィールド {i}"))).collect();
            row.extend((1..=v).map(|j| Value::Text(format!("値 {j}"))));
            out.push(row);
        }
        Headers::Shown => {
            let mut row: Vec<Value> = (0..k).map(|i| rf.get(0, i).clone()).collect();
            row.extend((0..v).map(|j| vals.get(0, j).clone()));
            out.push(row);
        }
        _ => {}
    }
    let blank = || Value::Text(String::new());
    let total = |c: &mut Ctx, label: Value, members: &[usize]| -> Vec<Value> {
        let mut row = vec![label];
        row.extend((1..k).map(|_| blank()));
        row.extend((0..v).map(|j| aggregate(c, &f, column_of(&vals, j, members))));
        row
    };
    // The grand total is "総計" where subtotals are, "合計" otherwise.
    let grand_label = if depth.abs() == 2 { GRAND_TOTAL } else { TOTAL };
    let grand = (depth != 0).then(|| total(c, Value::Text(grand_label.into()), &rows));
    if let (Some(g), true) = (&grand, depth < 0) {
        out.push(g.clone());
    }
    let subtotals = depth.abs() == 2;
    let mut i = 0;
    while i < table_rows.len() {
        // A subtotal covers the rows of one value of the first field; it comes before them when the totals do.
        let top = table_rows[i].0[0].clone();
        let end = (i..table_rows.len()).find(|&j| !same(&table_rows[j].0[0], &top)).unwrap_or(table_rows.len());
        if subtotals && depth < 0 {
            let members: Vec<usize> = table_rows[i..end].iter().flat_map(|r| r.2.iter().copied()).collect();
            out.push(total(c, top.clone(), &members));
        }
        for (key, values, _) in &table_rows[i..end] {
            let mut row: Vec<Value> = key.clone();
            row.extend(values.iter().cloned());
            out.push(row);
        }
        if subtotals && depth > 0 {
            let members: Vec<usize> = table_rows[i..end].iter().flat_map(|r| r.2.iter().copied()).collect();
            out.push(total(c, top.clone(), &members));
        }
        i = end;
    }
    if let (Some(g), true) = (&grand, depth > 0) {
        out.push(g.clone());
    }
    Ok(rows_to_array(out))
}

fn rows_to_array(rows: Vec<Vec<Value>>) -> Value {
    let columns = rows.first().map_or(0, Vec::len);
    let n = rows.len();
    array_value(n, columns, rows.into_iter().flatten().collect())
}

// ----- PIVOTBY --------------------------------------------------------------

fn pivotby(c: &mut Ctx, a: &[Expr]) -> Value {
    match pivotby_of(c, a) {
        Ok(v) => v,
        Err(e) => err(e),
    }
}

fn pivotby_of(c: &mut Ctx, a: &[Expr]) -> Res<Value> {
    let rf = arg_array(c, &a[0])?;
    let cf = arg_array(c, &a[1])?;
    let vals = arg_array(c, &a[2])?;
    let f = function_arg(c, &a[3])?;
    if rf.rows != vals.rows || cf.rows != vals.rows || vals.columns != 1 || cf.columns != 1 {
        return Err(CellError::Value);
    }
    let k = rf.columns;
    let text_row = || rf.rows >= 2 && matches!(vals.get(0, 0), Value::Text(_)) && !matches!(vals.get(1, 0), Value::Text(_));
    let headers = headers_option(c, a, 4, text_row)?;
    let row_depth = total_depth(c, a, 5, k)?;
    let row_sort = sort_option(c, a, 6, k + 1)?;
    let col_depth = total_depth(c, a, 7, 1.max(cf.columns))?;
    let col_sort = sort_option(c, a, 8, cf.columns + 1)?;
    let filter = filter_option(c, a, 9, rf.rows)?;
    if !matches!(int_option(c, a, 10)?, None | Some(0)) {
        return Err(CellError::Value);
    }
    let skip = usize::from(matches!(headers, Headers::Hidden | Headers::Shown));
    let rows: Vec<usize> = (skip..rf.rows).filter(|&r| filter[r]).collect();
    if rows.is_empty() {
        return Err(CellError::Calc);
    }
    // Row keys and column keys, each with its rows and total.
    let mut row_groups: Vec<(Vec<Value>, Vec<usize>, Value)> = Vec::new();
    for (key, members) in group_rows(&rf, &rows) {
        let total = aggregate(c, &f, column_of(&vals, 0, &members));
        row_groups.push((key, members, total));
    }
    let mut col_groups: Vec<(Vec<Value>, Vec<usize>, Value)> = Vec::new();
    for (key, members) in group_rows(&cf, &rows) {
        let total = aggregate(c, &f, column_of(&vals, 0, &members));
        col_groups.push((key, members, total));
    }
    // A sort of the rows by the totals counts only where the totals' column is shown (Excel's columns sort by their totals either way).
    let row_sort = row_sort.filter(|spec| col_depth != 0 || spec.iter().all(|s| s.unsigned_abs() as usize <= k));
    match &row_sort {
        Some(spec) => row_groups.sort_by(|x, y| by_columns(spec, &x.0.iter().cloned().chain([x.2.clone()]).collect::<Vec<_>>(), &y.0.iter().cloned().chain([y.2.clone()]).collect::<Vec<_>>())),
        None => row_groups.sort_by(|x, y| by_keys(&x.0, &y.0)),
    }
    match &col_sort {
        Some(spec) => col_groups.sort_by(|x, y| by_columns(spec, &x.0.iter().cloned().chain([x.2.clone()]).collect::<Vec<_>>(), &y.0.iter().cloned().chain([y.2.clone()]).collect::<Vec<_>>())),
        None => col_groups.sort_by(|x, y| by_keys(&x.0, &y.0)),
    }
    let blank = || Value::Text(String::new());
    let grand = aggregate(c, &f, column_of(&vals, 0, &rows));
    // The body: a row for each row key, with the subtotals of the first field's values (the hierarchy).
    let cells_of = |c: &mut Ctx, members: &[usize], total: &Value| -> Vec<Value> {
        let mut cells: Vec<Value> = col_groups
            .iter()
            .map(|(_, cm, _)| {
                let both: Vec<usize> = members.iter().copied().filter(|r| cm.contains(r)).collect();
                if both.is_empty() { blank() } else { aggregate(c, &f, column_of(&vals, 0, &both)) }
            })
            .collect();
        match col_depth {
            1 => cells.push(total.clone()),
            -1 => cells.insert(0, total.clone()),
            _ => {}
        }
        cells
    };
    let mut body: Vec<Vec<Value>> = Vec::new();
    let subtotals = row_depth.abs() == 2;
    let mut i = 0;
    while i < row_groups.len() {
        let top = row_groups[i].0[0].clone();
        let end = if subtotals { (i..row_groups.len()).find(|&j| !same(&row_groups[j].0[0], &top)).unwrap_or(row_groups.len()) } else { i + 1 };
        let subtotal = |c: &mut Ctx| -> Vec<Value> {
            let members: Vec<usize> = row_groups[i..end].iter().flat_map(|g| g.1.iter().copied()).collect();
            let total = aggregate(c, &f, column_of(&vals, 0, &members));
            let mut row = vec![top.clone()];
            row.extend((1..k).map(|_| blank()));
            row.extend(cells_of(c, &members, &total));
            row
        };
        if subtotals && row_depth < 0 {
            body.push(subtotal(c));
        }
        for (key, members, total) in &row_groups[i..end] {
            let mut row = key.clone();
            row.extend(cells_of(c, members, total));
            body.push(row);
        }
        if subtotals && row_depth > 0 {
            body.push(subtotal(c));
        }
        i = end;
    }
    // The column totals' row.
    if row_depth != 0 {
        let mut cells: Vec<Value> = col_groups.iter().map(|(_, _, t)| t.clone()).collect();
        match col_depth {
            1 => cells.push(grand.clone()),
            -1 => cells.insert(0, grand.clone()),
            _ => {}
        }
        let mut row = vec![Value::Text(if subtotals { GRAND_TOTAL } else { TOTAL }.into())];
        row.extend((1..k).map(|_| blank()));
        row.extend(cells);
        if row_depth > 0 { body.push(row) } else { body.insert(0, row) }
    }
    // The header rows.
    let mut col_labels: Vec<Value> = col_groups.iter().map(|(key, _, _)| key[0].clone()).collect();
    match col_depth {
        1 => col_labels.push(Value::Text(TOTAL.into())),
        -1 => col_labels.insert(0, Value::Text(TOTAL.into())),
        _ => {}
    }
    let mut out: Vec<Vec<Value>> = Vec::new();
    let lead = |first: Value| -> Vec<Value> {
        let mut row = vec![first];
        row.extend((1..k).map(|_| blank()));
        row
    };
    match headers {
        Headers::Generated | Headers::Shown => {
            let (col_header, row_headers, value_header): (Value, Vec<Value>, Value) = if headers == Headers::Shown {
                (header_text(cf.get(0, 0)), (0..k).map(|i| rf.get(0, i).clone()).collect(), vals.get(0, 0).clone())
            } else {
                (Value::Text("列フィールド 1".into()), (1..=k).map(|i| Value::Text(format!("行フィールド {i}"))).collect(), Value::Text("値 1".into()))
            };
            let mut first = lead(blank());
            first.push(col_header);
            first.extend((1..col_labels.len()).map(|_| blank()));
            out.push(first);
            let mut second = lead(blank());
            second.extend(col_labels.iter().cloned());
            out.push(second);
            let mut third = row_headers;
            third.extend(col_labels.iter().map(|_| value_header.clone()));
            out.push(third);
        }
        _ => {
            let mut first = lead(blank());
            first.extend(col_labels);
            out.push(first);
        }
    }
    out.extend(body);
    Ok(rows_to_array(out))
}

