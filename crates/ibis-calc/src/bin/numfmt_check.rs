// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Checks the number format engine against Excel's display text: reads
//! the TSV of `scripts/calc-check/numfmt_tsv.py` (suite, id, code, value,
//! type, Excel's text) and formats each value.
//!
//! ```text
//! ibis-numfmt-check answers.tsv [--show N] [--suite NAME] [--tsv out.tsv]
//! ```
//!
//! A text of `#` only is Excel's "does not fit" (a date out of range):
//! it matches when the engine also says the value cannot be shown. A fill
//! (`*x`) repeats to the column's width: it matches when Excel's text is
//! the engine's with the fill character repeated at its place.

use std::collections::BTreeMap;
use std::io::Write;

use ibis_calc::numfmt::{builtin, format};
use ibis_calc::{CellError, CellValue};

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.remove(0);
    let mut show = 0usize;
    let mut suite_filter: Option<String> = None;
    let mut tsv: Option<std::fs::File> = None;
    while !args.is_empty() {
        let a = args.remove(0);
        match a.as_str() {
            "--show" => show = args.remove(0).parse().unwrap_or(30),
            "--suite" => suite_filter = Some(args.remove(0)),
            "--tsv" => tsv = Some(std::fs::File::create(args.remove(0)).unwrap()),
            _ => {}
        }
    }
    let text = std::fs::read_to_string(&path).unwrap();
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut shown = 0;
    for line in text.lines() {
        let f: Vec<String> = line.split('\t').map(unescape).collect();
        if f.len() < 6 {
            continue;
        }
        let (suite, id, code, value, kind, expected) = (&f[0], &f[1], &f[2], &f[3], &f[4], &f[5]);
        if suite_filter.as_ref().is_some_and(|s| s != suite) {
            continue;
        }
        let code = match code.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()) {
            Some(n) if suite == "XNUM-builtin" => builtin(n).unwrap_or("General").to_string(),
            _ => code.clone(),
        };
        let v = match kind.as_str() {
            "n" => CellValue::Number(value.parse().unwrap_or(0.0)),
            "b" => CellValue::Bool(value.eq_ignore_ascii_case("TRUE") || value == "1"),
            "e" => CellValue::Error(CellError::parse(value).unwrap_or(CellError::Value)),
            _ if value.is_empty() && kind.is_empty() => CellValue::Empty,
            _ => CellValue::Text(value.clone()),
        };
        let got = format(&v, &code, false);
        let ok = if !expected.is_empty() && expected.chars().all(|c| c == '#') {
            got.overflow
        } else if let Some((at, ch)) = got.fill {
            let (a, b) = got.text.split_at(at);
            expected.starts_with(a) && expected.ends_with(b) && expected.len() >= a.len() + b.len() && expected[a.len()..expected.len() - b.len()].chars().all(|c| c == ch)
        } else {
            !got.overflow && got.text == *expected
        };
        let t = tally.entry(suite.clone()).or_default();
        t.0 += 1;
        if ok {
            t.1 += 1;
        } else if shown < show {
            shown += 1;
            println!("{id}\t{code:?}\t{value:?}({kind})\texcel={expected:?}\tibis={:?}{}", got.text, if got.overflow { " (###)" } else { "" });
        }
        if let Some(out) = tsv.as_mut() {
            writeln!(out, "{id}\t{}\t{}", if ok { "match" } else { "differ" }, if ok { String::new() } else { format!("excel={expected:?} ibis={:?}", got.text) }).unwrap();
        }
    }
    let (mut total, mut matched) = (0, 0);
    println!("suite\ttotal\tmatch");
    for (s, (t, m)) in &tally {
        println!("{s}\t{t}\t{m}\t({:.1}%)", *m as f64 * 100.0 / (*t).max(1) as f64);
        total += t;
        matched += m;
    }
    println!("all\t{total}\t{matched}\t({:.1}%)", matched as f64 * 100.0 / total.max(1) as f64);
}
