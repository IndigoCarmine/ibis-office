// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Whether a formula can give more than one value (a dynamic array
//! formula), as Excel 365 decides whether to save it as one (`t="array"`
//! with the dynamic array metadata) or as a plain formula: a range or an
//! array where one value is wanted, a function that gives an array, or a
//! function lifted over an array (`IF(A1:A3>0,…)`). Functions that take
//! ranges as they are (SUM, COUNTIF's range, VLOOKUP's table, …) give one
//! value. When unsure, an array: a formula saved plain where Excel would
//! spill it would show another result there.

use super::parser::{Expr, parse};

/// Functions that always give an array (or may give a range).
const ARRAY_FUNCTIONS: &[&str] = &[
    "SEQUENCE", "FILTER", "SORT", "SORTBY", "UNIQUE", "RANDARRAY", "TRANSPOSE", "MMULT", "MINVERSE", "MUNIT", "FREQUENCY", "LINEST", "LOGEST", "TREND", "GROWTH", "TEXTSPLIT", "VSTACK", "HSTACK", "TOCOL",
    "TOROW", "WRAPROWS", "WRAPCOLS", "TAKE", "DROP", "CHOOSECOLS", "CHOOSEROWS", "EXPAND", "MAKEARRAY", "MAP", "SCAN", "BYROW", "BYCOL", "OFFSET", "INDIRECT", "XLOOKUP", "GROUPBY", "PIVOTBY", "ANCHORARRAY",
    "LET", "LAMBDA", "REGEXEXTRACT", "TRIMRANGE", "IMAGE", "STOCKHISTORY", "FIELDVALUE",
];

/// Functions that take ranges or arrays as they are, at these argument
/// places (`None`: every argument), giving one value for them.
fn range_args(name: &str) -> Option<Option<&'static [usize]>> {
    const ALL: Option<&[usize]> = None;
    Some(match name {
        "SUM" | "SUMSQ" | "PRODUCT" | "AVERAGE" | "AVERAGEA" | "COUNT" | "COUNTA" | "COUNTBLANK" | "MAX" | "MAXA" | "MIN" | "MINA" | "MEDIAN" | "MODE" | "MODE.SNGL" | "STDEV" | "STDEV.S" | "STDEV.P" | "STDEVA"
        | "STDEVP" | "STDEVPA" | "VAR" | "VAR.S" | "VAR.P" | "VARA" | "VARP" | "VARPA" | "DEVSQ" | "AVEDEV" | "GEOMEAN" | "HARMEAN" | "KURT" | "SKEW" | "SKEW.P" | "AND" | "OR" | "XOR" | "CONCAT" | "SUMPRODUCT"
        | "SUMX2MY2" | "SUMX2PY2" | "SUMXMY2" | "CORREL" | "COVAR" | "COVARIANCE.P" | "COVARIANCE.S" | "PEARSON" | "RSQ" | "SLOPE" | "INTERCEPT" | "STEYX" | "IRR" | "ROWS" | "COLUMNS" | "AREAS" | "SUBTOTAL"
        | "NPV" | "COUNTIFS" | "SUMIFS" | "AVERAGEIFS" | "MAXIFS" | "MINIFS" | "TEXTJOIN" | "AGGREGATE" => ALL,
        "SUMIF" | "AVERAGEIF" => Some(&[0, 2]),
        "COUNTIF" | "LARGE" | "SMALL" | "PERCENTILE" | "PERCENTILE.INC" | "PERCENTILE.EXC" | "QUARTILE" | "QUARTILE.INC" | "QUARTILE.EXC" | "MIRR" => Some(&[0]),
        "VLOOKUP" | "HLOOKUP" | "MATCH" | "XMATCH" | "RANK" | "RANK.EQ" | "RANK.AVG" | "PERCENTRANK" | "PERCENTRANK.INC" | "PERCENTRANK.EXC" => Some(&[1]),
        "LOOKUP" | "FORECAST" | "FORECAST.LINEAR" => Some(&[1, 2]),
        _ => return None,
    })
}

/// Whether `formula` (without `=`) can give an array; `multi_name` says
/// whether a defined name stands for more than one cell.
pub fn can_return_array(formula: &str, multi_name: &dyn Fn(&str) -> bool) -> bool {
    match parse(formula) {
        Ok(e) => newer(&e) || array(&e, multi_name),
        Err(_) => true,
    }
}

/// A spill reference (`A1#`), an explicit `@` or a function of dynamic
/// arrays anywhere in it: kept a dynamic array formula, as the file wrote
/// it, whatever it gives.
fn newer(e: &Expr) -> bool {
    match e {
        Expr::Spill(_) | Expr::Implicit(_) | Expr::Invoke { .. } => true,
        Expr::Call { name, args } => {
            let n = name.trim_start_matches("_XLFN.").trim_start_matches("_XLWS.");
            ARRAY_FUNCTIONS.contains(&n) || args.iter().any(newer)
        }
        Expr::Array(rows) => rows.iter().flatten().any(newer),
        Expr::Neg(a) | Expr::Plus(a) | Expr::Percent(a) => newer(a),
        Expr::Binary(_, a, b) | Expr::Range(a, b) | Expr::Intersect(a, b) => newer(a) || newer(b),
        Expr::Union(list) => list.iter().any(newer),
        _ => false,
    }
}

fn array(e: &Expr, multi_name: &dyn Fn(&str) -> bool) -> bool {
    match e {
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Missing | Expr::Implicit(_) => false,
        Expr::Array(rows) => rows.len() > 1 || rows.first().is_some_and(|r| r.len() > 1),
        Expr::Ref(r) => !r.is_cell(),
        Expr::Name { name, .. } => multi_name(name),
        // `[@Col]` is the row's cell; a column or the table is a range.
        Expr::Structured { spec, .. } => !spec.contains('@') && !spec.to_lowercase().contains("#this row"),
        Expr::Spill(_) | Expr::Range(..) | Expr::Union(_) | Expr::Invoke { .. } => true,
        Expr::Intersect(a, b) => array(a, multi_name) && array(b, multi_name),
        Expr::Neg(a) | Expr::Plus(a) | Expr::Percent(a) => array(a, multi_name),
        Expr::Binary(_, a, b) => array(a, multi_name) || array(b, multi_name),
        Expr::Call { name, args } => {
            let name = name.trim_start_matches("_XLFN.").trim_start_matches("_XLWS.");
            if ARRAY_FUNCTIONS.contains(&name) {
                return true;
            }
            if name == "INDEX" {
                // One cell of a range, or a whole row or column of it (0 or left out).
                if !args.first().is_some_and(|a| array(a, multi_name)) {
                    return args.iter().skip(1).any(|a| array(a, multi_name));
                }
                let given = |i: usize| match args.get(i) {
                    Some(Expr::Number(n)) => *n != 0.0,
                    None | Some(Expr::Missing) => false,
                    Some(a) => !array(a, multi_name),
                };
                return !(given(1) && (args.len() == 2 || given(2)));
            }
            match range_args(name) {
                Some(None) => false,
                Some(Some(places)) => args.iter().enumerate().any(|(i, a)| !places.contains(&i) && array(a, multi_name)),
                None => args.iter().any(|a| array(a, multi_name)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::can_return_array;

    fn arr(f: &str) -> bool {
        can_return_array(f, &|n| n == "範囲")
    }

    #[test]
    fn plain_and_dynamic_formulas() {
        assert!(arr("SUM(B2#)") && arr("@A1:A3*2") && arr("SUM(SEQUENCE(3))"), "kept dynamic");
        for plain in ["SUM(A1:A3)", "SUBTOTAL(109,売上[金額])", "A1*2", "VLOOKUP(A1,B1:C9,2,FALSE)", "COUNTIF(A1:A9,\">0\")", "SUMIFS(C1:C9,A1:A9,\"x\")", "IF(A1>0,1,2)", "INDEX(A1:C3,2,3)", "[@金額]*2", "SUM(範囲)", "1+税率", "AVERAGE(A:A)"] {
            assert!(!arr(plain), "{plain}");
        }
        for dynamic in ["A1:A3", "A1:A3*2", "SEQUENCE(3)", "IF(A1:A3>0,1,2)", "FILTER(A1:A9,B1:B9>0)", "A1#", "{1,2,3}", "INDEX(A1:C3,0,1)", "範囲*2", "売上[金額]", "VLOOKUP(A1:A3,B1:C9,2,FALSE)", "LEN(A1:A3)"] {
            assert!(arr(dynamic), "{dynamic}");
        }
    }
}
