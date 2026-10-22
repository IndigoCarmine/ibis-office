// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Helpers over `roxmltree` that match elements and attributes by local name.
//!
//! OOXML element names are unique enough per context that the namespace
//! prefix never needs checking, and matching by local name keeps the reader
//! independent of the prefixes a producer chose.

use roxmltree::{Document, Node};

use crate::Error;

/// Parse the part `name`.
///
/// A DOCTYPE is accepted, as some producers write one, but not one that
/// declares entities: roxmltree bounds only references nested inside other
/// entities, so one large entity referenced many times from the content
/// expands a few kilobytes of input into gigabytes. Parts with a DTD are
/// rare (ECMA-376 Part 2 does not allow one) and every entity declaration,
/// general or parameter, is spelled `<!ENTITY`, so a part that has a DTD
/// and contains that text anywhere is refused before anything is expanded.
/// Without entity declarations nothing can expand (no external DTD is
/// loaded), and the DTD itself is then harmless.
pub(crate) fn parse<'i>(name: &str, text: &'i str) -> Result<Document<'i>, Error> {
    match Document::parse(text) {
        Err(roxmltree::Error::DtdDetected) if text.contains("<!ENTITY") => {
            Err(Error::new(format!("{name} を読めません: 実体宣言（<!ENTITY）のある DTD には対応していません")))
        }
        Err(roxmltree::Error::DtdDetected) => {
            let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
            Document::parse_with_options(text, options).map_err(|e| Error::xml(name, e))
        }
        parsed => parsed.map_err(|e| Error::xml(name, e)),
    }
}

pub(crate) fn elements<'a, 'i>(node: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children().filter(Node::is_element)
}

pub(crate) fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    elements(node).find(|n| n.tag_name().name() == name)
}

pub(crate) fn children<'a, 'i>(node: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    elements(node).filter(move |n| n.tag_name().name() == name)
}

/// Follow a path of child element names.
pub(crate) fn path<'a, 'i>(node: Node<'a, 'i>, names: &[&str]) -> Option<Node<'a, 'i>> {
    names.iter().try_fold(node, |n, name| child(n, name))
}

/// An attribute by local name, whatever its namespace (`r:embed` is "embed").
pub(crate) fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == name).map(|a| a.value())
}

pub(crate) fn attr_i64(node: Node, name: &str) -> Option<i64> {
    attr(node, name)?.trim().parse().ok()
}

pub(crate) fn attr_bool(node: Node, name: &str) -> Option<bool> {
    match attr(node, name)? {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
    }
}

/// The position of `node` from the root element: its index among its
/// parent's element children, at each level. `ibis_ooxml`'s tree finds the
/// same element by it (`XmlDocument::by_path`), which is how the writer
/// finds what a reader read.
pub(crate) fn element_path(node: Node) -> Vec<u32> {
    let mut path = Vec::new();
    let mut at = node;
    while let Some(parent) = at.parent() {
        if !parent.is_element() {
            break;
        }
        path.push(elements(parent).position(|c| c == at).unwrap_or(0) as u32);
        at = parent;
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

    /// A minimal `.docx` whose main part is `document`.
    fn docx(document: &str) -> Vec<u8> {
        let types = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#;
        let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        crate::zip::stored_archive(&[
            ("[Content_Types].xml", types.as_bytes()),
            ("_rels/.rels", rels.as_bytes()),
            ("word/document.xml", document.as_bytes()),
        ])
    }

    /// A document part with `doctype` before its root and `text` in its
    /// one run.
    fn document(doctype: &str, text: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>{doctype}<w:document xmlns:w="{W}"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"#
        )
    }

    /// One large entity referenced many times at the top level: roxmltree's
    /// loop detector bounds only nested references, so this would expand
    /// without limit (here 1,000 x 1,000 characters, kept small so the test
    /// itself never allocates much; the same part scaled up reaches
    /// gigabytes from a few kilobytes of zipped input).
    #[test]
    fn document_declaring_entities_is_refused() {
        let doctype = format!("<!DOCTYPE w:document [<!ENTITY a \"{}\">]>", "x".repeat(1000));
        let data = docx(&document(&doctype, &"&a;".repeat(1000)));
        let error = crate::read_docx(&data).err().expect("a part declaring entities is refused");
        assert!(error.to_string().contains("<!ENTITY"), "{error}");
    }

    #[test]
    fn document_with_doctype_but_no_entities_is_read() {
        for doctype in ["", "<!DOCTYPE w:document>", "<!DOCTYPE w:document [<!ELEMENT w:document ANY>]>"] {
            let data = docx(&document(doctype, "hello &amp; goodbye"));
            assert!(crate::read_docx(&data).is_ok(), "doctype {doctype:?}");
        }
    }

    #[test]
    fn parameter_entities_are_refused_too() {
        let text = document("<!DOCTYPE w:document [<!ENTITY % p \"<!ELEMENT w:document ANY>\"> %p;]>", "");
        assert!(super::parse("word/document.xml", &text).is_err());
    }

    /// The text `<!ENTITY` counts only in a part with a DTD: elsewhere it
    /// can only be text in a comment or CDATA section, and declares nothing.
    #[test]
    fn entity_text_without_a_dtd_is_read() {
        let text = format!(r#"<?xml version="1.0"?><!-- <!ENTITY a "b"> --><w:document xmlns:w="{W}"><w:t><![CDATA[<!ENTITY>]]></w:t></w:document>"#);
        let doc = super::parse("word/document.xml", &text).unwrap();
        assert_eq!(doc.root_element().first_child().and_then(|t| t.text()), Some("<!ENTITY>"));
    }
}
