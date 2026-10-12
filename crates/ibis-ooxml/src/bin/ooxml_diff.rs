// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibis-ooxml-diff: compare two Office files part by part, the XML parts by
//! meaning (see `ibis_ooxml::canon`).
//!
//!   ibis-ooxml-diff <a.pptx|xlsx|docx> <b> [--json] [--ignore <part>]...
//!
//! Exit status: 0 when the files mean the same, 1 when they differ, 2 on
//! errors.

use std::process::ExitCode;

use ibis_ooxml::canon::{PartDiff, diff_packages};

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(err) => {
            eprintln!("ibis-ooxml-diff: {err}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool, String> {
    let mut files = Vec::new();
    let mut ignore = Vec::new();
    let mut json = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--ignore" => ignore.push(args.next().ok_or("--ignore の値がありません")?),
            other if other.starts_with("--") => return Err(format!("知らないオプションです: {other}")),
            _ => files.push(arg),
        }
    }
    let [a, b] = &files[..] else { return Err("使い方: ibis-ooxml-diff <a> <b> [--json] [--ignore <part>]...".into()) };
    let read = |p: &str| std::fs::read(p).map_err(|e| format!("{p}: {e}"));
    let ignore: Vec<&str> = ignore.iter().map(String::as_str).collect();
    let report = diff_packages(&read(a)?, &read(b)?, &ignore).map_err(|e| e.to_string())?;
    if json {
        println!("{}", report.to_json());
    } else {
        for (part, diff) in &report.differences {
            match diff {
                PartDiff::Removed => println!("{part}: 1 つ目にだけある"),
                PartDiff::Added => println!("{part}: 2 つ目にだけある"),
                PartDiff::Bytes { a_len, b_len } => println!("{part}: バイト列が違う（{a_len} / {b_len} バイト）"),
                PartDiff::Xml { path, a, b, differing, .. } => println!("{part}: XML が違う（{differing} 項目）{path}\n  - {a}\n  + {b}"),
                PartDiff::Unparsable { a_error, b_error } => println!("{part}: 読めない XML（{a_error:?} / {b_error:?}）"),
            }
        }
        println!("{} パーツ中 {} が違う（同じ XML {}、同じバイト列 {}）", report.parts, report.differences.len(), report.same_xml, report.same_bytes);
    }
    Ok(report.is_same())
}
