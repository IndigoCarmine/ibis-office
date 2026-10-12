// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! XML parsing and XPath evaluation for Excel's FILTERXML function.

use crate::eval::{Ctx, Value, to_text};
use crate::formula::Expr;
use crate::functions::{array_value, err};
use crate::value::CellError;

#[derive(Clone, Debug, PartialEq)]
pub struct XmlElement {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<XmlNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum XmlNode {
    Element(XmlElement),
    Text(String),
}

impl XmlElement {
    pub fn string_value(&self) -> String {
        let mut out = String::new();
        for c in &self.children {
            match c {
                XmlNode::Text(t) => out.push_str(t),
                XmlNode::Element(el) => out.push_str(&el.string_value()),
            }
        }
        out
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    pub fn local_name(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }
}

fn unescape_xml(s: &str) -> Result<String, CellError> {
    if !s.contains('&') {
        return Ok(s.to_string());
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            let mut entity = String::new();
            let mut closed = false;
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == ';' {
                    closed = true;
                    break;
                }
                entity.push(next);
                if entity.len() > 10 {
                    return Err(CellError::Value);
                }
            }
            if !closed {
                return Err(CellError::Value);
            }
            match entity.as_str() {
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "amp" => out.push('&'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                    let num = u32::from_str_radix(&entity[2..], 16).map_err(|_| CellError::Value)?;
                    let ch = char::from_u32(num).ok_or(CellError::Value)?;
                    out.push(ch);
                }
                _ if entity.starts_with('#') => {
                    let num = entity[1..].parse::<u32>().map_err(|_| CellError::Value)?;
                    let ch = char::from_u32(num).ok_or(CellError::Value)?;
                    out.push(ch);
                }
                _ => return Err(CellError::Value),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

pub fn parse_xml(input: &str) -> Result<XmlElement, CellError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(CellError::Value);
    }
    let mut chars = s.char_indices().peekable();
    let mut stack: Vec<XmlElement> = Vec::new();
    let mut root: Option<XmlElement> = None;

    while let Some((_, c)) = chars.next() {
        if c == '<' {
            let Some(&(_, next)) = chars.peek() else {
                return Err(CellError::Value);
            };
            if next == '?' {
                // Processing instruction <? ... ?>
                chars.next();
                let mut closed = false;
                while let Some((_, ch)) = chars.next() {
                    if ch == '?' && chars.peek().map(|&(_, c)| c) == Some('>') {
                        chars.next();
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err(CellError::Value);
                }
            } else if next == '!' {
                chars.next();
                if let Some(&(_, ch2)) = chars.peek() {
                    if ch2 == '-' {
                        // Comment <!-- ... -->
                        chars.next();
                        if chars.next().map(|(_, c)| c) != Some('-') {
                            return Err(CellError::Value);
                        }
                        let mut dashes = 0;
                        let mut closed = false;
                        while let Some((_, ch)) = chars.next() {
                            if ch == '-' {
                                dashes += 1;
                            } else if ch == '>' && dashes >= 2 {
                                closed = true;
                                break;
                            } else {
                                dashes = 0;
                            }
                        }
                        if !closed {
                            return Err(CellError::Value);
                        }
                    } else if s[chars.peek().map_or(0, |&(i, _)| i)..].starts_with("[CDATA[") {
                        for _ in 0..7 {
                            chars.next();
                        }
                        let mut cdata = String::new();
                        let mut closed = false;
                        while let Some((_, ch)) = chars.next() {
                            if ch == ']' && chars.peek().map(|&(_, c)| c) == Some(']') {
                                chars.next();
                                if chars.peek().map(|&(_, c)| c) == Some('>') {
                                    chars.next();
                                    closed = true;
                                    break;
                                } else {
                                    cdata.push(']');
                                    cdata.push(']');
                                }
                            } else {
                                cdata.push(ch);
                            }
                        }
                        if !closed {
                            return Err(CellError::Value);
                        }
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(XmlNode::Text(cdata));
                        } else {
                            return Err(CellError::Value);
                        }
                    } else {
                        // DOCTYPE or other <! ... >
                        let mut depth = 1;
                        while let Some((_, ch)) = chars.next() {
                            if ch == '<' {
                                depth += 1;
                            } else if ch == '>' {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                        }
                        if depth != 0 {
                            return Err(CellError::Value);
                        }
                    }
                }
            } else if next == '/' {
                // End tag </name>
                chars.next();
                let mut tag = String::new();
                while let Some(&(_, ch)) = chars.peek() {
                    if ch.is_whitespace() || ch == '>' {
                        break;
                    }
                    tag.push(ch);
                    chars.next();
                }
                while let Some(&(_, ch)) = chars.peek() {
                    if ch == '>' {
                        chars.next();
                        break;
                    }
                    if !ch.is_whitespace() {
                        return Err(CellError::Value);
                    }
                    chars.next();
                }
                let Some(finished) = stack.pop() else {
                    return Err(CellError::Value);
                };
                if finished.name != tag {
                    return Err(CellError::Value);
                }
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(XmlNode::Element(finished));
                } else if root.is_none() {
                    root = Some(finished);
                } else {
                    return Err(CellError::Value);
                }
            } else {
                // Start tag <name ...> or <name ... />
                let mut tag = String::new();
                while let Some(&(_, ch)) = chars.peek() {
                    if ch.is_whitespace() || ch == '>' || ch == '/' {
                        break;
                    }
                    tag.push(ch);
                    chars.next();
                }
                if tag.is_empty() {
                    return Err(CellError::Value);
                }
                let mut attrs = Vec::new();
                let mut empty_el = false;
                loop {
                    while let Some(&(_, ch)) = chars.peek() {
                        if ch.is_whitespace() {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    let Some(&(_, ch)) = chars.peek() else {
                        return Err(CellError::Value);
                    };
                    if ch == '>' {
                        chars.next();
                        break;
                    } else if ch == '/' {
                        chars.next();
                        if chars.peek().map(|&(_, c)| c) == Some('>') {
                            chars.next();
                            empty_el = true;
                            break;
                        } else {
                            return Err(CellError::Value);
                        }
                    }
                    let mut attr_name = String::new();
                    while let Some(&(_, c)) = chars.peek() {
                        if c == '=' || c.is_whitespace() || c == '>' || c == '/' {
                            break;
                        }
                        attr_name.push(c);
                        chars.next();
                    }
                    while let Some(&(_, c)) = chars.peek() {
                        if c.is_whitespace() {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if chars.next().map(|(_, c)| c) != Some('=') {
                        return Err(CellError::Value);
                    }
                    while let Some(&(_, c)) = chars.peek() {
                        if c.is_whitespace() {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    let quote = chars.next().map(|(_, c)| c);
                    if quote != Some('"') && quote != Some('\'') {
                        return Err(CellError::Value);
                    }
                    let quote = quote.unwrap();
                    let mut attr_val = String::new();
                    let mut quote_closed = false;
                    while let Some((_, c)) = chars.next() {
                        if c == quote {
                            quote_closed = true;
                            break;
                        }
                        attr_val.push(c);
                    }
                    if !quote_closed {
                        return Err(CellError::Value);
                    }
                    let unescaped_val = unescape_xml(&attr_val)?;
                    attrs.push((attr_name, unescaped_val));
                }
                let el = XmlElement { name: tag, attributes: attrs, children: Vec::new() };
                if empty_el {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(XmlNode::Element(el));
                    } else if root.is_none() {
                        root = Some(el);
                    } else {
                        return Err(CellError::Value);
                    }
                } else {
                    if root.is_some() && stack.is_empty() {
                        return Err(CellError::Value);
                    }
                    stack.push(el);
                }
            }
        } else {
            let mut text = String::new();
            text.push(c);
            while let Some(&(_, ch)) = chars.peek() {
                if ch == '<' {
                    break;
                }
                text.push(ch);
                chars.next();
            }
            if let Some(parent) = stack.last_mut() {
                let unescaped = unescape_xml(&text)?;
                parent.children.push(XmlNode::Text(unescaped));
            } else if !text.trim().is_empty() {
                return Err(CellError::Value);
            }
        }
    }

    if !stack.is_empty() {
        return Err(CellError::Value);
    }
    root.ok_or(CellError::Value)
}

#[derive(Clone, Debug)]
struct XPathStep {
    descendant: bool,
    target: String,
    predicate: Option<String>,
}

fn parse_xpath(mut expr: &str) -> Result<Vec<XPathStep>, CellError> {
    expr = expr.trim();
    if expr.is_empty() {
        return Err(CellError::Value);
    }
    let mut steps = Vec::new();
    let mut at = 0;
    let bytes = expr.as_bytes();

    while at < bytes.len() {
        let descendant = if bytes[at..].starts_with(b"//") {
            at += 2;
            true
        } else if bytes[at] == b'/' {
            at += 1;
            false
        } else if steps.is_empty() {
            false
        } else {
            return Err(CellError::Value);
        };

        // Read target up to '[', '/', or end
        let start = at;
        while at < bytes.len() && bytes[at] != b'/' && bytes[at] != b'[' {
            at += 1;
        }
        let target = expr[start..at].trim().to_string();
        if target.is_empty() {
            return Err(CellError::Value);
        }

        let mut predicate = None;
        if at < bytes.len() && bytes[at] == b'[' {
            at += 1;
            let pred_start = at;
            let mut depth = 1;
            while at < bytes.len() {
                if bytes[at] == b'[' {
                    depth += 1;
                } else if bytes[at] == b']' {
                    depth -= 1;
                    if depth == 0 {
                        predicate = Some(expr[pred_start..at].trim().to_string());
                        at += 1;
                        break;
                    }
                }
                at += 1;
            }
            if depth != 0 {
                return Err(CellError::Value);
            }
        }

        steps.push(XPathStep { descendant, target, predicate });
    }

    Ok(steps)
}

#[derive(Clone, Copy)]
enum MatchItem<'a> {
    Element(&'a XmlElement),
    Attr(&'a str),
}

fn collect_descendants<'a>(el: &'a XmlElement, out: &mut Vec<&'a XmlElement>) {
    out.push(el);
    for child in &el.children {
        if let XmlNode::Element(child_el) = child {
            collect_descendants(child_el, out);
        }
    }
}

fn matches_target(el: &XmlElement, target: &str) -> bool {
    if target == "*" {
        true
    } else {
        el.name == target || el.local_name() == target
    }
}

fn eval_predicate(el: &XmlElement, pred: &str) -> bool {
    let p = pred.trim();
    // Index check: 1-based integer handled at step level
    // Predicate: [@attr = 'val'] or [@attr = "val"]
    if p.starts_with('@') {
        let rest = &p[1..];
        if let Some((name, val_part)) = rest.split_once('=') {
            let name = name.trim();
            let val = val_part.trim().trim_matches(|c| c == '\'' || c == '"');
            return el.attr(name) == Some(val);
        } else {
            return el.attr(rest.trim()).is_some();
        }
    }
    // Predicate: [local-name() = 'val']
    if p.starts_with("local-name()") {
        let rest = &p["local-name()".len()..];
        if let Some((_, val_part)) = rest.split_once('=') {
            let val = val_part.trim().trim_matches(|c| c == '\'' || c == '"');
            return el.local_name() == val;
        }
    }
    // Predicate: [tag = 'val']
    if let Some((tag, val_part)) = p.split_once('=') {
        let tag = tag.trim();
        let val = val_part.trim().trim_matches(|c| c == '\'' || c == '"');
        for child in &el.children {
            if let XmlNode::Element(child_el) = child {
                if (child_el.name == tag || child_el.local_name() == tag) && child_el.string_value() == val {
                    return true;
                }
            }
        }
        return false;
    }
    // Predicate: [contains(text(), 'val')] or [contains(@attr, 'val')]
    if p.starts_with("contains(") && p.ends_with(')') {
        let inner = &p[9..p.len() - 1];
        if let Some((arg1, arg2)) = inner.split_once(',') {
            let arg1 = arg1.trim();
            let needle = arg2.trim().trim_matches(|c| c == '\'' || c == '"');
            let hay = if arg1 == "text()" || arg1 == "." {
                el.string_value()
            } else if arg1.starts_with('@') {
                el.attr(&arg1[1..]).unwrap_or("").to_string()
            } else {
                String::new()
            };
            return hay.contains(needle);
        }
    }
    true
}

pub fn eval_xpath(root: &XmlElement, mut xpath: &str) -> Result<Value, CellError> {
    xpath = xpath.trim();
    if xpath.is_empty() {
        return Err(CellError::Value);
    }

    // Functions: count(...)
    if xpath.starts_with("count(") && xpath.ends_with(')') {
        let inner = &xpath[6..xpath.len() - 1];
        let nodes = match query_nodes(root, inner) {
            Ok(v) => v,
            Err(_) => return Ok(Value::Number(0.0)),
        };
        return Ok(Value::Number(nodes.len() as f64));
    }
    if xpath.starts_with("string-length(") && xpath.ends_with(')') {
        let inner = &xpath[14..xpath.len() - 1];
        let nodes = query_nodes(root, inner)?;
        let len = nodes.first().map_or(0, |m| match m {
            MatchItem::Element(el) => el.string_value().chars().count(),
            MatchItem::Attr(s) => s.chars().count(),
        });
        return Ok(Value::Number(len as f64));
    }
    if xpath.starts_with("boolean(") && xpath.ends_with(')') {
        let inner = &xpath[8..xpath.len() - 1];
        let nodes = match query_nodes(root, inner) {
            Ok(v) => v,
            Err(_) => Vec::new(),
        };
        return Ok(Value::Bool(!nodes.is_empty()));
    }

    let items = query_nodes(root, xpath)?;
    if items.is_empty() {
        return Err(CellError::Value);
    }
    let texts: Vec<String> = items.into_iter().map(|item| match item {
        MatchItem::Element(el) => el.string_value(),
        MatchItem::Attr(s) => s.to_string(),
    }).collect();

    if texts.len() == 1 {
        Ok(Value::Text(texts.into_iter().next().unwrap()))
    } else {
        let rows = texts.len();
        let data = texts.into_iter().map(Value::Text).collect();
        Ok(array_value(rows, 1, data))
    }
}

fn query_nodes<'a>(root: &'a XmlElement, xpath: &str) -> Result<Vec<MatchItem<'a>>, CellError> {
    let steps = parse_xpath(xpath)?;
    let mut current_elements: Vec<&'a XmlElement> = vec![root];

    for (step_idx, step) in steps.iter().enumerate() {
        let is_last = step_idx + 1 == steps.len();

        if step.target.starts_with('@') {
            if !is_last {
                return Err(CellError::Value);
            }
            let attr_name = &step.target[1..];
            let mut out = Vec::new();
            let pool = if step.descendant {
                let mut all = Vec::new();
                for el in current_elements {
                    collect_descendants(el, &mut all);
                }
                all
            } else {
                current_elements
            };
            for el in pool {
                if attr_name == "*" {
                    for (_, val) in &el.attributes {
                        out.push(MatchItem::Attr(val.as_str()));
                    }
                } else if let Some(val) = el.attr(attr_name) {
                    out.push(MatchItem::Attr(val));
                }
            }
            return Ok(out);
        }

        // Element target
        let mut next = Vec::new();
        if step.descendant {
            let mut all = Vec::new();
            for el in current_elements {
                collect_descendants(el, &mut all);
            }
            // Filter elements matching target
            for el in all {
                // If it's step 0 (e.g. "//item") and el is root, does root match?
                if matches_target(el, &step.target) {
                    next.push(el);
                }
            }
        } else if step_idx == 0 && matches_target(root, &step.target) {
            next.push(root);
        } else {
            for el in current_elements {
                for child in &el.children {
                    if let XmlNode::Element(child_el) = child {
                        if matches_target(child_el, &step.target) {
                            next.push(child_el);
                        }
                    }
                }
            }
        }

        // Apply predicate
        if let Some(pred) = &step.predicate {
            if let Ok(idx) = pred.trim().parse::<usize>() {
                next = if idx >= 1 && idx <= next.len() {
                    vec![next[idx - 1]]
                } else {
                    Vec::new()
                };
            } else if pred.trim() == "last()" {
                next = next.pop().into_iter().collect();
            } else {
                next.retain(|el| eval_predicate(el, pred));
            }
        }

        current_elements = next;
    }

    Ok(current_elements.into_iter().map(MatchItem::Element).collect())
}

pub(crate) fn filterxml(c: &mut Ctx, a: &[Expr]) -> Value {
    let xml_text = match to_text(&c.eval_scalar(&a[0])) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let xpath_text = match to_text(&c.eval_scalar(&a[1])) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let root = match parse_xml(&xml_text) {
        Ok(r) => r,
        Err(e) => return err(e),
    };
    match eval_xpath(&root, &xpath_text) {
        Ok(v) => v,
        Err(e) => err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_eval() {
        let xml = "<root><item id='1'>Apple</item><item id='2'>Banana</item></root>";
        let root = parse_xml(xml).unwrap();
        assert_eq!(root.name, "root");
        assert_eq!(root.children.len(), 2);

        // //item
        let res = eval_xpath(&root, "//item").unwrap();
        match res {
            Value::Array(a) => {
                assert_eq!(a.rows, 2);
                assert_eq!(a.columns, 1);
                assert_eq!(a.data[0], Value::Text("Apple".into()));
                assert_eq!(a.data[1], Value::Text("Banana".into()));
            }
            _ => panic!("expected array"),
        }

        // //item[2]
        let res = eval_xpath(&root, "//item[2]").unwrap();
        assert_eq!(res, Value::Text("Banana".into()));

        // //item/@id
        let res = eval_xpath(&root, "//item/@id").unwrap();
        match res {
            Value::Array(a) => {
                assert_eq!(a.data[0], Value::Text("1".into()));
                assert_eq!(a.data[1], Value::Text("2".into()));
            }
            _ => panic!("expected array"),
        }

        // count(//item)
        let res = eval_xpath(&root, "count(//item)").unwrap();
        assert_eq!(res, Value::Number(2.0));
    }
}
