// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Word's EQ field (`{ EQ \f(1,2) }`, ECMA-376 Part 1 §17.16.5.21) read as a formula, to be typeset as one is: fractions
//! (`\f`), roots (`\r`), superscripts and subscripts (`\s\up`, `\s\do`), brackets (`\b`), integrals, sums and products (`\i`),
//! arrays (`\a`), lists (`\l`) and overstrikes (`\o`) with their elements.

use crate::math::{FracKind, Math, stacked_by_default};

/// The formula an EQ field's instruction (`EQ \f(1,2)`) makes; `None` for another field or an EQ whose switches it does not
/// read (the enclosed characters `\o` makes are the field format's, not this one's).
pub fn eq_math(instruction: &str) -> Option<Math> {
    let trimmed = instruction.trim_start();
    let rest = trimmed.strip_prefix("EQ").or_else(|| trimmed.strip_prefix("eq"))?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let chars: Vec<char> = rest.trim().chars().collect();
    let mut at = 0;
    let items = parse_items(&chars, &mut at, false)?;
    // Only a formula some switch made (plain text is shown as the field's result).
    items.iter().any(|m| !matches!(m, Math::Text(_))).then(|| row(items))
}

fn row(mut items: Vec<Math>) -> Math {
    if items.len() == 1 { items.remove(0) } else { Math::Row(items) }
}

/// Items up to the end of the text, or (`in_args`) up to a `,` `;` or `)` at the level of the arguments, which is left to the caller.
fn parse_items(chars: &[char], at: &mut usize, in_args: bool) -> Option<Vec<Math>> {
    let mut items: Vec<Math> = Vec::new();
    let mut text = String::new();
    fn flush(text: &mut String, items: &mut Vec<Math>) {
        if !text.is_empty() {
            items.push(Math::Text(std::mem::take(text)));
        }
    }
    while *at < chars.len() {
        let c = chars[*at];
        match c {
            ',' | ';' | ')' if in_args => break,
            '\\' => {
                let next = *chars.get(*at + 1)?;
                if !next.is_ascii_alphabetic() {
                    // `\,` `\(` `\\`: the character itself.
                    text.push(next);
                    *at += 2;
                    continue;
                }
                flush(&mut text, &mut items);
                *at += 1;
                let element = parse_element(chars, at, &mut items)?;
                if let Some(m) = element {
                    items.push(m);
                }
            }
            c => {
                text.push(c);
                *at += 1;
            }
        }
    }
    flush(&mut text, &mut items);
    Some(items)
}

/// The options after a switch's letter (`\bc\{`, `\co2`, `\up6`) up to its `(`, as (name, value) pairs; the value is a number or
/// the one character after the option.
fn options(chars: &[char], at: &mut usize) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    loop {
        match chars.get(*at)? {
            '(' => {
                *at += 1;
                return Some(out);
            }
            '\\' => {
                *at += 1;
                let mut name = String::new();
                while let Some(c) = chars.get(*at).filter(|c| c.is_ascii_alphabetic()) {
                    name.push(*c);
                    *at += 1;
                }
                // The options that take a character (`\lc\{`) or a number (`\co2`, `\up6`, `\fo10`).
                let mut value = String::new();
                if matches!(name.as_str(), "lc" | "rc" | "bc" | "fc" | "vc" | "hc") {
                    if chars.get(*at) == Some(&'\\') {
                        *at += 1;
                    }
                    if let Some(c) = chars.get(*at) {
                        value.push(*c);
                        *at += 1;
                    }
                } else {
                    while let Some(c) = chars.get(*at).filter(|c| c.is_ascii_digit() || **c == '-') {
                        value.push(*c);
                        *at += 1;
                    }
                }
                out.push((name, value));
            }
            ' ' => *at += 1,
            _ => return None,
        }
    }
}

/// The arguments of a switch up to its `)`: each a formula of its own.
fn args(chars: &[char], at: &mut usize) -> Option<Vec<Math>> {
    let mut out = Vec::new();
    loop {
        let items = parse_items(chars, at, true)?;
        out.push(if items.is_empty() { Math::Text(String::new()) } else { row(items) });
        match chars.get(*at)? {
            ',' | ';' => *at += 1,
            ')' => {
                *at += 1;
                return Some(out);
            }
            _ => return None,
        }
    }
}

/// One switch (the `\` and its letter already read). `items` are what came before it, for `\s` which belongs to the item before.
fn parse_element(chars: &[char], at: &mut usize, items: &mut Vec<Math>) -> Option<Option<Math>> {
    let letter = chars.get(*at)?.to_ascii_lowercase();
    *at += 1;
    let opts = options(chars, at)?;
    let opt = |name: &str| opts.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
    let has = |name: &str| opts.iter().any(|(n, _)| n == name);
    let args = args(chars, at)?;
    let boxed = |m: Math| Box::new(m);
    let math = match letter {
        'f' if args.len() == 2 => {
            let mut a = args.into_iter();
            Math::Frac { num: boxed(a.next()?), den: boxed(a.next()?), kind: FracKind::Bar }
        }
        'r' => {
            let mut a = args.into_iter();
            match (a.next(), a.next()) {
                (Some(degree), Some(body)) => Math::Radical { degree: (!matches!(&degree, Math::Text(t) if t.is_empty())).then(|| boxed(degree)), body: boxed(body) },
                (Some(body), None) => Math::Radical { degree: None, body: boxed(body) },
                _ => return None,
            }
        }
        's' => {
            // `\s\up6(x)`, `\s\do3(x)`: the item before is the base.
            let up = has("up");
            let down = has("do");
            if !up && !down {
                return None;
            }
            let script = row(args);
            let base = items.pop().map_or_else(|| Math::Text(String::new()), |m| m);
            // A base that has a script already takes the other one.
            return Some(Some(match base {
                Math::Script { base, sub, sup } if (up && sup.is_none()) || (down && sub.is_none()) => Math::Script { base, sub: if down { Some(boxed(script.clone())) } else { sub }, sup: if up { Some(boxed(script)) } else { sup } },
                base => Math::Script { base: boxed(base), sub: down.then(|| boxed(script.clone())), sup: up.then(|| boxed(script)) },
            }));
        }
        'b' => {
            // `\bc\[` both brackets of a kind, `\lc` and `\rc` each side; none: parentheses.
            let pair = |c: &str| match c {
                "(" => ")".to_string(),
                "[" => "]".to_string(),
                "{" => "}".to_string(),
                "<" => ">".to_string(),
                other => other.to_string(),
            };
            let (open, close) = match (opt("lc").or(opt("bc")), opt("rc")) {
                (Some(l), Some(r)) => (l.to_string(), r.to_string()),
                (Some(l), None) => (l.to_string(), pair(l)),
                (None, Some(r)) => ("(".to_string(), r.to_string()),
                (None, None) => ("(".to_string(), ")".to_string()),
            };
            Math::Delimited { open, close, sep: String::new(), items: args }
        }
        'i' => {
            let op = opt("fc").or(opt("vc")).or(opt("hc")).map(str::to_string).unwrap_or_else(|| if has("su") { "∑".into() } else if has("pr") { "∏".into() } else { "∫".into() });
            let stacked = if has("in") { false } else if has("su") || has("pr") { true } else { stacked_by_default(&op) };
            let mut a = args.into_iter();
            let (sub, sup, body) = (a.next(), a.next(), a.next());
            let empty = |m: &Math| matches!(m, Math::Text(t) if t.is_empty());
            Math::Nary { op, sub: sub.filter(|m| !empty(m)).map(boxed), sup: sup.filter(|m| !empty(m)).map(boxed), body: boxed(body.unwrap_or_else(|| Math::Text(String::new()))), stacked }
        }
        'a' => {
            let columns = opt("co").and_then(|v| v.parse::<usize>().ok()).filter(|c| *c > 0).unwrap_or(1);
            let rows: Vec<Vec<Math>> = args.chunks(columns).map(|r| r.to_vec()).collect();
            Math::Matrix(rows)
        }
        'l' => {
            let mut parts = Vec::new();
            for (i, a) in args.into_iter().enumerate() {
                if i > 0 {
                    parts.push(Math::Text(",".into()));
                }
                parts.push(a);
            }
            Math::Row(parts)
        }
        // Overstrikes, displacements and boxes: the elements one after the other.
        'o' | 'd' | 'x' => row(args),
        _ => return None,
    };
    Some(Some(math))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Math {
        Math::Text(s.into())
    }

    #[test]
    fn eq_switches_make_formulas() {
        assert_eq!(eq_math("EQ \\f(1,2)"), Some(Math::Frac { num: Box::new(t("1")), den: Box::new(t("2")), kind: FracKind::Bar }));
        assert_eq!(eq_math(" EQ \\r(,x)"), Some(Math::Radical { degree: None, body: Box::new(t("x")) }));
        assert_eq!(eq_math("EQ \\r(3,x)"), Some(Math::Radical { degree: Some(Box::new(t("3"))), body: Box::new(t("x")) }));
        // x² from `x\s\up6(2)`, and a subscript on it.
        assert_eq!(eq_math("EQ x\\s\\up6(2)"), Some(Math::Script { base: Box::new(t("x")), sub: None, sup: Some(Box::new(t("2"))) }));
        assert_eq!(eq_math("EQ x\\s\\do3(i)\\s\\up6(2)"), Some(Math::Script { base: Box::new(t("x")), sub: Some(Box::new(t("i"))), sup: Some(Box::new(t("2"))) }));
        assert_eq!(eq_math("EQ \\b(a+b)"), Some(Math::Delimited { open: "(".into(), close: ")".into(), sep: String::new(), items: vec![t("a+b")] }));
        assert_eq!(eq_math("EQ \\b\\bc\\[(x)"), Some(Math::Delimited { open: "[".into(), close: "]".into(), sep: String::new(), items: vec![t("x")] }));
        assert_eq!(eq_math("EQ \\i(0,1,x)"), Some(Math::Nary { op: "∫".into(), sub: Some(Box::new(t("0"))), sup: Some(Box::new(t("1"))), body: Box::new(t("x")), stacked: false }));
        assert!(matches!(eq_math("EQ \\i\\su(k=1,n,k)"), Some(Math::Nary { stacked: true, .. })));
        // Nested: a fraction in a root.
        assert!(matches!(eq_math("EQ \\r(,\\f(1,2))"), Some(Math::Radical { .. })));
        assert_eq!(eq_math("EQ \\a\\co2(a,b,c,d)"), Some(Math::Matrix(vec![vec![t("a"), t("b")], vec![t("c"), t("d")]])));
        // Not a formula: plain text, another field.
        assert_eq!(eq_math("EQ abc"), None);
        assert_eq!(eq_math("PAGE"), None);
    }
}
