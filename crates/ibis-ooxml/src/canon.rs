// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Comparing packages by meaning: every part of two files, the XML parts
//! normalized so that only differences of meaning show.
//!
//! Normalizing an XML part:
//!
//! - Element and attribute names are compared by namespace and local name,
//!   so the prefixes a producer chose do not matter; namespace declarations
//!   themselves are left out.
//! - Attributes are compared as a set (their order does not matter).
//! - Whitespace-only text between elements (indentation) is left out; text
//!   in an element with no element children is kept exactly.
//! - The prefixes named in markup-compatibility attributes
//!   (`mc:Ignorable="x14 x14ac"`, `mc:Choice Requires="…"`) are replaced by
//!   their namespaces.
//! - Comments and processing instructions are kept; the XML declaration is
//!   compared by its content.
//!
//! Other parts are compared byte for byte.

use crate::package::{Package, is_xml_name};
use crate::xml::{Document, NodeId, NodeKind};
use crate::Error;

const MC_NAMESPACE: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

/// How one part differs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartDiff {
    /// Only in the first package.
    Removed,
    /// Only in the second package.
    Added,
    /// A non-XML part whose bytes differ.
    Bytes { a_len: usize, b_len: usize },
    /// An XML part whose normalized form differs.
    Xml {
        /// Where the first difference is, as a path of `{namespace}local[index]`.
        path: String,
        a: String,
        b: String,
        /// Normalized items (start tags, texts, …) of each side.
        a_items: usize,
        b_items: usize,
        /// Items that differ at the same position (a rough size of the change).
        differing: usize,
    },
    /// A part that should be XML but one side could not be parsed.
    Unparsable { a_error: Option<String>, b_error: Option<String> },
}

/// The comparison of two packages.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Parts compared (in either package).
    pub parts: usize,
    /// XML parts whose normalized forms are the same.
    pub same_xml: usize,
    /// Other parts with the same bytes.
    pub same_bytes: usize,
    /// Parts that differ, by name, in the first package's order (then the second's).
    pub differences: Vec<(String, PartDiff)>,
    /// Parts left out of the comparison (by the caller's request).
    pub ignored: Vec<String>,
}

impl Report {
    pub fn is_same(&self) -> bool {
        self.differences.is_empty()
    }

    /// The report as JSON.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{");
        out.push_str(&format!("\"same\":{},\"parts\":{},\"same_xml\":{},\"same_bytes\":{},", self.is_same(), self.parts, self.same_xml, self.same_bytes));
        out.push_str("\"ignored\":[");
        out.push_str(&self.ignored.iter().map(|s| json_string(s)).collect::<Vec<_>>().join(","));
        out.push_str("],\"differences\":[");
        let items: Vec<String> = self
            .differences
            .iter()
            .map(|(name, diff)| {
                let body = match diff {
                    PartDiff::Removed => "\"kind\":\"removed\"".to_string(),
                    PartDiff::Added => "\"kind\":\"added\"".to_string(),
                    PartDiff::Bytes { a_len, b_len } => format!("\"kind\":\"bytes\",\"a_len\":{a_len},\"b_len\":{b_len}"),
                    PartDiff::Xml { path, a, b, a_items, b_items, differing } => format!(
                        "\"kind\":\"xml\",\"path\":{},\"a\":{},\"b\":{},\"a_items\":{a_items},\"b_items\":{b_items},\"differing\":{differing}",
                        json_string(path),
                        json_string(a),
                        json_string(b)
                    ),
                    PartDiff::Unparsable { a_error, b_error } => format!(
                        "\"kind\":\"unparsable\",\"a_error\":{},\"b_error\":{}",
                        a_error.as_deref().map_or("null".into(), json_string),
                        b_error.as_deref().map_or("null".into(), json_string)
                    ),
                };
                format!("{{\"part\":{},{body}}}", json_string(name))
            })
            .collect();
        out.push_str(&items.join(","));
        out.push_str("]}");
        out
    }
}

/// A string as a JSON string literal.
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Compare two packages part by part, leaving out the parts `ignore` names.
pub fn diff_packages(a: &[u8], b: &[u8], ignore: &[&str]) -> Result<Report, Error> {
    let pa = Package::open(a.to_vec())?;
    let pb = Package::open(b.to_vec())?;
    let mut report = Report::default();
    let mut names: Vec<String> = pa.names().map(str::to_string).collect();
    names.extend(pb.names().filter(|n| !pa.contains(n)).map(str::to_string));
    for name in names {
        if ignore.contains(&name.as_str()) {
            report.ignored.push(name);
            continue;
        }
        report.parts += 1;
        let diff = match (pa.contains(&name), pb.contains(&name)) {
            (true, false) => Some(PartDiff::Removed),
            (false, true) => Some(PartDiff::Added),
            _ => {
                let (x, y) = (pa.read(&name)?, pb.read(&name)?);
                if is_xml_name(&name) {
                    match diff_xml(&x, &y) {
                        None => {
                            report.same_xml += 1;
                            None
                        }
                        d => d,
                    }
                } else if x == y {
                    report.same_bytes += 1;
                    None
                } else {
                    Some(PartDiff::Bytes { a_len: x.len(), b_len: y.len() })
                }
            }
        };
        if let Some(diff) = diff {
            report.differences.push((name, diff));
        }
    }
    Ok(report)
}

/// How two XML texts differ after normalizing, or `None` if they do not.
/// Texts that do not parse are compared byte for byte.
pub fn diff_xml(a: &[u8], b: &[u8]) -> Option<PartDiff> {
    if a == b {
        return None;
    }
    let (da, db) = (Document::parse(a), Document::parse(b));
    let (da, db) = match (da, db) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(_), Err(_)) if a == b => return None,
        (x, y) => return Some(PartDiff::Unparsable { a_error: x.err().map(|e| e.to_string()), b_error: y.err().map(|e| e.to_string()) }),
    };
    let (ia, ib) = (canonical_items(&da), canonical_items(&db));
    let first = ia.iter().zip(&ib).position(|(x, y)| x.text != y.text);
    let first = match first {
        Some(i) => i,
        None if ia.len() == ib.len() => return None,
        None => ia.len().min(ib.len()),
    };
    let differing = ia.iter().zip(&ib).filter(|(x, y)| x.text != y.text).count() + ia.len().abs_diff(ib.len());
    let at = |items: &[Item], i: usize| items.get(i).map_or(("(終わり)".to_string(), String::new()), |it| (it.text.clone(), it.path.clone()));
    let (a_text, a_path) = at(&ia, first);
    let (b_text, b_path) = at(&ib, first);
    Some(PartDiff::Xml {
        path: if a_path.is_empty() { b_path } else { a_path },
        a: truncate(&a_text),
        b: truncate(&b_text),
        a_items: ia.len(),
        b_items: ib.len(),
        differing,
    })
}

fn truncate(s: &str) -> String {
    const MAX: usize = 300;
    if s.chars().count() <= MAX { s.to_string() } else { format!("{}…", s.chars().take(MAX).collect::<String>()) }
}

/// One normalized item: a start tag with its attributes, an end tag, a text,
/// a comment or a processing instruction; with where it is.
struct Item {
    text: String,
    path: String,
    /// For a text item, the text unquoted (to join adjacent texts).
    plain: Option<String>,
}

/// The normalized form of `doc`, as a list of items.
pub fn canonical(doc: &Document) -> Vec<String> {
    canonical_items(doc).into_iter().map(|i| i.text).collect()
}

fn canonical_items(doc: &Document) -> Vec<Item> {
    let mut items = Vec::new();
    if let Some(decl) = doc.declaration() {
        items.push(Item { text: format!("?xml {}", normalize_declaration(decl)), path: String::new(), plain: None });
    }
    enum Step {
        Open(NodeId, String),
        Close(String),
    }
    let mut stack: Vec<Step> = doc.children(Document::ROOT).iter().rev().map(|&c| Step::Open(c, String::new())).collect();
    while let Some(step) = stack.pop() {
        let (id, parent_path) = match step {
            Step::Close(path) => {
                items.push(Item { text: "/".into(), path, plain: None });
                continue;
            }
            Step::Open(id, path) => (id, path),
        };
        match doc.kind(id) {
            NodeKind::Element => {
                let name = expanded(doc, id, doc.name(id), true);
                let index = doc.parent(id).map_or(0, |p| doc.elements(p).take_while(|&c| c != id).filter(|&c| doc.name(c) == doc.name(id)).count());
                let path = format!("{parent_path}/{name}[{index}]");
                let mut attributes: Vec<String> = doc
                    .attributes(id)
                    .iter()
                    .filter(|a| a.name != "xmlns" && !a.name.starts_with("xmlns:"))
                    .map(|a| {
                        let attr_name = expanded(doc, id, &a.name, false);
                        let value = if is_mc_prefix_list(doc, id, &a.name) { prefixes_to_namespaces(doc, id, &a.value) } else { a.value.clone() };
                        format!("{attr_name}={value:?}")
                    })
                    .collect();
                attributes.sort();
                items.push(Item { text: format!("<{name} {}", attributes.join(" ")), path: path.clone(), plain: None });
                let has_elements = doc.elements(id).next().is_some();
                stack.push(Step::Close(path.clone()));
                for &child in doc.children(id).iter().rev() {
                    let skip = has_elements && doc.kind(child) == NodeKind::Text && doc.text(child).chars().all(|c| c.is_ascii_whitespace());
                    if !skip {
                        stack.push(Step::Open(child, path.clone()));
                    }
                }
            }
            NodeKind::Text | NodeKind::CData => {
                if doc.parent(id) == Some(Document::ROOT) {
                    continue; // whitespace around the root element
                }
                // Adjacent text and CDATA are one text.
                let text = doc.text(id);
                match items.last_mut() {
                    Some(Item { text: quoted, path, plain: Some(plain) }) if *path == parent_path => {
                        plain.push_str(&text);
                        *quoted = format!("{plain:?}");
                    }
                    _ => items.push(Item { text: format!("{text:?}"), path: parent_path, plain: Some(text) }),
                }
            }
            NodeKind::Comment => items.push(Item { text: format!("<!--{}", doc.raw_content(id).unwrap_or("")), path: parent_path, plain: None }),
            NodeKind::ProcessingInstruction => items.push(Item { text: format!("<?{}", doc.raw_content(id).unwrap_or("").trim()), path: parent_path, plain: None }),
            NodeKind::Doctype => items.push(Item { text: doc.raw_content(id).unwrap_or("").to_string(), path: parent_path, plain: None }),
            NodeKind::Declaration | NodeKind::Document => {}
        }
    }
    items
}

/// `{namespace}local` for a name as written at element `at`. An unprefixed
/// attribute has no namespace; an unprefixed element has the default one.
fn expanded(doc: &Document, at: NodeId, name: &str, element: bool) -> String {
    let (prefix, local) = match name.split_once(':') {
        Some((p, l)) => (Some(p), l),
        None => (None, name),
    };
    if prefix.is_none() && !element {
        return local.to_string();
    }
    match doc.resolve_prefix(at, prefix) {
        Some(ns) => format!("{{{ns}}}{local}"),
        None => match prefix {
            Some(p) => format!("{{?{p}}}{local}"),
            None => local.to_string(),
        },
    }
}

/// Whether attribute `name` of `at` holds a list of namespace prefixes.
fn is_mc_prefix_list(doc: &Document, at: NodeId, name: &str) -> bool {
    let (prefix, local) = match name.split_once(':') {
        Some((p, l)) => (Some(p), l),
        None => (None, name),
    };
    match prefix {
        Some(p) => {
            doc.resolve_prefix(at, Some(p)) == Some(MC_NAMESPACE)
                && matches!(local, "Ignorable" | "ProcessContent" | "PreserveElements" | "PreserveAttributes" | "MustUnderstand")
        }
        // `Requires` of `mc:Choice`.
        None => local == "Requires" && doc.namespace(at) == Some(MC_NAMESPACE),
    }
}

fn prefixes_to_namespaces(doc: &Document, at: NodeId, value: &str) -> String {
    let mut tokens: Vec<String> = value
        .split_ascii_whitespace()
        .map(|token| {
            // `ProcessContent` names elements: `prefix:local`.
            let (prefix, rest) = match token.split_once(':') {
                Some((p, r)) => (p, Some(r)),
                None => (token, None),
            };
            let ns = doc.resolve_prefix(at, Some(prefix)).map_or_else(|| format!("?{prefix}"), str::to_string);
            match rest {
                Some(r) => format!("{{{ns}}}{r}"),
                None => format!("{{{ns}}}"),
            }
        })
        .collect();
    tokens.sort();
    tokens.join(" ")
}

/// The declaration's pseudo-attributes, normalized (`encoding` in lower case).
fn normalize_declaration(decl: &str) -> String {
    let body = decl.strip_prefix("xml").unwrap_or(decl);
    let mut parts: Vec<String> = Vec::new();
    for piece in body.split_ascii_whitespace() {
        if let Some((k, v)) = piece.split_once('=') {
            let v = v.trim_matches(['"', '\'']);
            let v = if k == "encoding" { v.to_ascii_lowercase() } else { v.to_string() };
            parts.push(format!("{k}={v}"));
        }
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_order_and_indentation_do_not_matter() {
        let a = br#"<?xml version="1.0" encoding="UTF-8"?><a:r xmlns:a="urn:a" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:x="urn:x" mc:Ignorable="x" y="1" z="2"><a:t> keep </a:t></a:r>"#;
        let b = br#"<?xml version='1.0' encoding='utf-8'?>
<q:r z="2" y="1" m:Ignorable="w" xmlns:q="urn:a" xmlns:w="urn:x" xmlns:m="http://schemas.openxmlformats.org/markup-compatibility/2006">
  <q:t> keep </q:t>
</q:r>"#;
        assert_eq!(diff_xml(a, b), None);
    }

    #[test]
    fn meaning_changes_are_found() {
        let a = b"<r xmlns=\"urn:a\"><t>x</t><u v=\"1\"/></r>";
        for b in [&b"<r xmlns=\"urn:a\"><t>y</t><u v=\"1\"/></r>"[..], b"<r xmlns=\"urn:b\"><t>x</t><u v=\"1\"/></r>", b"<r xmlns=\"urn:a\"><t>x</t><u v=\"2\"/></r>", b"<r xmlns=\"urn:a\"><t>x</t></r>", b"<r xmlns=\"urn:a\"><t> x</t><u v=\"1\"/></r>"] {
            assert!(diff_xml(a, b).is_some(), "{}", String::from_utf8_lossy(b));
        }
        match diff_xml(a, b"<r xmlns=\"urn:a\"><t>x</t><u v=\"2\"/></r>").unwrap() {
            PartDiff::Xml { path, .. } => assert_eq!(path, "/{urn:a}r[0]/{urn:a}u[0]"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn text_split_by_cdata_is_one_text() {
        assert_eq!(diff_xml(b"<a>x<![CDATA[<y>]]>z</a>", b"<a>x&lt;y&gt;z</a>"), None);
    }

    #[test]
    fn json_escapes() {
        assert_eq!(json_string("a\"b\n\u{1}"), "\"a\\\"b\\n\\u0001\"");
    }
}
