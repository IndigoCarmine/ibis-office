// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A read-write XML tree that keeps what it does not change.
//!
//! Saving an Office file changes a few elements of a part and must leave the
//! rest as it was: elements, attributes and namespace declarations the
//! reader does not know, `mc:AlternateContent`, extension lists, comments,
//! processing instructions, CDATA and the whitespace between elements. This
//! tree keeps all of them:
//!
//! - Every node is kept, in order, including whitespace-only text.
//! - Names are kept as written (`a:t`, with the prefix); namespaces are
//!   resolved on demand from the `xmlns` attributes in scope.
//! - Attributes keep their order.
//! - A start tag, an end tag and a text node remember their exact source
//!   text. A node that is not changed is written back byte for byte; a
//!   changed one is written from its parts (attributes in double quotes,
//!   separated by one space).
//!
//! [`Document::to_bytes`] writes the tree, reusing the source text of what
//! did not change, so an untouched document comes out identical to its
//! input. [`Document::to_bytes_regenerated`] writes every node from its
//! parts instead, which is what a changed node goes through; the save tests
//! use it to check that writing the whole tree keeps the meaning.
//!
//! The nodes live in an arena and are named by [`NodeId`]s, which stay valid
//! while the tree changes (a removed node is only detached).

use std::fmt;

/// The namespace the `xml` prefix is bound to.
pub const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    fn index(self) -> usize {
        self.0 as usize
    }
}

/// An attribute, its name as written (`r:id`) and its value unescaped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct Element {
    name: String,
    attributes: Vec<Attribute>,
    /// Written as `<name/>` (no content) rather than `<name></name>`.
    empty: bool,
    /// The start tag as written, until the name or attributes change.
    raw_start: Option<String>,
    /// The end tag as written (`</name >`), until the name changes.
    raw_end: Option<String>,
}

#[derive(Clone, Debug)]
enum Data {
    Document,
    Element(Element),
    /// Text, unescaped; `raw` is its source text when that is not simply
    /// the value (it had references, carriage returns or `>`).
    Text { value: String, raw: Option<String> },
    CData(String),
    Comment(String),
    /// A processing instruction: what is between `<?` and `?>`.
    Pi(String),
    /// The XML declaration: what is between `<?` and `?>` (`xml version=…`).
    Declaration(String),
    /// A document type declaration, as written (`<!DOCTYPE …>`).
    Doctype(String),
}

#[derive(Clone, Debug)]
struct Node {
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    data: Data,
}

/// What kind of node a [`NodeId`] names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Document,
    Element,
    Text,
    CData,
    Comment,
    ProcessingInstruction,
    Declaration,
    Doctype,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Byte offset in the input (after a byte order mark).
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "XML の {} バイト目: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

/// An XML document.
#[derive(Clone, Debug)]
pub struct Document {
    nodes: Vec<Node>,
    /// The input began with a UTF-8 byte order mark, written back.
    bom: bool,
}

impl Document {
    /// The document node, parent of the root element and of what is around it.
    pub const ROOT: NodeId = NodeId(0);

    /// An empty document with an XML declaration, for a new part.
    pub fn new_with_declaration() -> Self {
        let mut doc = Self { nodes: vec![Node { parent: None, children: Vec::new(), data: Data::Document }], bom: false };
        let decl = doc.push(Data::Declaration("xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"".into()));
        doc.attach(Self::ROOT, None, decl);
        let newline = doc.push(Data::Text { value: "\r\n".into(), raw: Some("\r\n".into()) });
        doc.attach(Self::ROOT, None, newline);
        doc
    }

    /// Parse `input` (UTF-8, with or without a byte order mark).
    pub fn parse(input: &[u8]) -> Result<Self, ParseError> {
        let (bom, input) = match input.strip_prefix(b"\xef\xbb\xbf") {
            Some(rest) => (true, rest),
            None => (false, input),
        };
        let text = std::str::from_utf8(input)
            .map_err(|e| ParseError { offset: e.valid_up_to(), message: "UTF-8 ではありません".into() })?;
        Parser { input: text, pos: 0, doc: Self { nodes: vec![Node { parent: None, children: Vec::new(), data: Data::Document }], bom } }.run()
    }

    // ----- Reading --------------------------------------------------------

    pub fn kind(&self, id: NodeId) -> NodeKind {
        match self.node(id).data {
            Data::Document => NodeKind::Document,
            Data::Element(_) => NodeKind::Element,
            Data::Text { .. } => NodeKind::Text,
            Data::CData(_) => NodeKind::CData,
            Data::Comment(_) => NodeKind::Comment,
            Data::Pi(_) => NodeKind::ProcessingInstruction,
            Data::Declaration(_) => NodeKind::Declaration,
            Data::Doctype(_) => NodeKind::Doctype,
        }
    }

    pub fn is_element(&self, id: NodeId) -> bool {
        matches!(self.node(id).data, Data::Element(_))
    }

    /// The root element.
    pub fn root_element(&self) -> Option<NodeId> {
        self.children(Self::ROOT).iter().copied().find(|&c| self.is_element(c))
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).parent
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.node(id).children
    }

    /// The element children of `id`, in order.
    pub fn elements(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.children(id).iter().copied().filter(|&c| self.is_element(c))
    }

    /// The first element child of `id` with local name `local`.
    pub fn child(&self, id: NodeId, local: &str) -> Option<NodeId> {
        self.elements(id).find(|&c| self.local_name(c) == local)
    }

    /// The element children of `id` with local name `local`.
    pub fn children_named<'a>(&'a self, id: NodeId, local: &'a str) -> impl Iterator<Item = NodeId> + 'a {
        self.elements(id).filter(move |&c| self.local_name(c) == local)
    }

    /// Follow a path of local names from `id`.
    pub fn path(&self, id: NodeId, locals: &[&str]) -> Option<NodeId> {
        locals.iter().try_fold(id, |n, local| self.child(n, local))
    }

    /// `id` and its descendants, in document order.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.children(n).iter().rev());
        }
        out
    }

    fn element(&self, id: NodeId) -> Option<&Element> {
        match &self.node(id).data {
            Data::Element(e) => Some(e),
            _ => None,
        }
    }

    fn element_mut(&mut self, id: NodeId) -> Option<&mut Element> {
        match &mut self.node_mut(id).data {
            Data::Element(e) => Some(e),
            _ => None,
        }
    }

    /// The element's name as written (`p:sp`); "" for other nodes.
    pub fn name(&self, id: NodeId) -> &str {
        self.element(id).map_or("", |e| &e.name)
    }

    /// The element's name without its prefix (`sp`).
    pub fn local_name(&self, id: NodeId) -> &str {
        let name = self.name(id);
        name.split_once(':').map_or(name, |(_, local)| local)
    }

    /// The element's prefix, if it has one.
    pub fn prefix(&self, id: NodeId) -> Option<&str> {
        self.name(id).split_once(':').map(|(prefix, _)| prefix)
    }

    /// The namespace of the element.
    pub fn namespace(&self, id: NodeId) -> Option<&str> {
        self.resolve_prefix(id, self.prefix(id))
    }

    /// The namespace `prefix` (`None` for the default namespace) is bound to
    /// at `id`, from the declarations on it and its ancestors.
    pub fn resolve_prefix(&self, id: NodeId, prefix: Option<&str>) -> Option<&str> {
        if prefix == Some("xml") {
            return Some(XML_NAMESPACE);
        }
        let mut at = Some(id);
        while let Some(n) = at {
            if let Some(e) = self.element(n) {
                for a in &e.attributes {
                    let declared = match a.name.strip_prefix("xmlns") {
                        Some("") => None,
                        Some(rest) => match rest.strip_prefix(':') {
                            Some(p) => Some(p),
                            None => continue,
                        },
                        None => continue,
                    };
                    if declared == prefix {
                        // `xmlns=""` undeclares the default namespace.
                        return if a.value.is_empty() { None } else { Some(&a.value) };
                    }
                }
            }
            at = self.parent(n);
        }
        None
    }

    /// A prefix bound to `namespace` at `id`, to name a new element or
    /// attribute with (`Some("")` for the default namespace).
    pub fn lookup_prefix(&self, id: NodeId, namespace: &str) -> Option<String> {
        let mut at = Some(id);
        while let Some(n) = at {
            if let Some(e) = self.element(n) {
                for a in &e.attributes {
                    if a.value != namespace {
                        continue;
                    }
                    let prefix = match a.name.strip_prefix("xmlns") {
                        Some("") => Some(""),
                        Some(rest) => rest.strip_prefix(':'),
                        None => None,
                    };
                    // Only if no nearer declaration rebinds it.
                    if let Some(p) = prefix
                        && self.resolve_prefix(id, if p.is_empty() { None } else { Some(p) }) == Some(namespace)
                    {
                        return Some(p.to_string());
                    }
                }
            }
            at = self.parent(n);
        }
        None
    }

    pub fn attributes(&self, id: NodeId) -> &[Attribute] {
        self.element(id).map_or(&[], |e| &e.attributes)
    }

    /// The attribute named `name` as written (`r:id`, `val`).
    pub fn attr(&self, id: NodeId, name: &str) -> Option<&str> {
        self.attributes(id).iter().find(|a| a.name == name).map(|a| a.value.as_str())
    }

    /// The first attribute with local name `local`, whatever its prefix.
    pub fn attr_local(&self, id: NodeId, local: &str) -> Option<&str> {
        self.attributes(id)
            .iter()
            .find(|a| a.name.rsplit_once(':').map_or(a.name.as_str(), |(_, l)| l) == local && !a.name.starts_with("xmlns"))
            .map(|a| a.value.as_str())
    }

    /// The text of a text or CDATA node, the content of a comment, or the
    /// concatenated text of an element's descendants.
    pub fn text(&self, id: NodeId) -> String {
        match &self.node(id).data {
            Data::Text { value, .. } => value.clone(),
            Data::CData(value) | Data::Comment(value) => value.clone(),
            Data::Element(_) | Data::Document => {
                let mut out = String::new();
                for n in self.descendants(id) {
                    match &self.node(n).data {
                        Data::Text { value, .. } | Data::CData(value) => out.push_str(value),
                        _ => {}
                    }
                }
                out
            }
            _ => String::new(),
        }
    }

    /// The position of `id` from the root element: the index among its
    /// parent's element children, at each level. It names the same element
    /// in another parse of the same text (as roxmltree does), so a reader can
    /// record where a thing came from and the writer find it again.
    pub fn element_path(&self, id: NodeId) -> Vec<u32> {
        let mut path = Vec::new();
        let mut at = id;
        while let Some(parent) = self.parent(at) {
            if parent == Self::ROOT {
                break;
            }
            let index = self.elements(parent).position(|c| c == at).unwrap_or(0);
            path.push(index as u32);
            at = parent;
        }
        path.reverse();
        path
    }

    /// The element at `path` (see [`element_path`](Self::element_path)).
    pub fn by_path(&self, path: &[u32]) -> Option<NodeId> {
        let mut at = self.root_element()?;
        for &index in path {
            at = self.elements(at).nth(index as usize)?;
        }
        Some(at)
    }

    // ----- Changing -------------------------------------------------------

    /// Set an attribute (added at the end if new). The start tag is then
    /// written from its parts.
    pub fn set_attr(&mut self, id: NodeId, name: &str, value: &str) {
        let Some(e) = self.element_mut(id) else { return };
        match e.attributes.iter_mut().find(|a| a.name == name) {
            Some(a) if a.value == value => return,
            Some(a) => a.value = value.to_string(),
            None => e.attributes.push(Attribute { name: name.to_string(), value: value.to_string() }),
        }
        e.raw_start = None;
    }

    pub fn remove_attr(&mut self, id: NodeId, name: &str) {
        let Some(e) = self.element_mut(id) else { return };
        let before = e.attributes.len();
        e.attributes.retain(|a| a.name != name);
        if e.attributes.len() != before {
            e.raw_start = None;
        }
    }

    /// Rename an element (`name` as written, with its prefix).
    pub fn rename(&mut self, id: NodeId, name: &str) {
        if let Some(e) = self.element_mut(id)
            && e.name != name
        {
            e.name = name.to_string();
            e.raw_start = None;
            e.raw_end = None;
        }
    }

    /// Replace the value of a text or CDATA node.
    pub fn set_text_value(&mut self, id: NodeId, text: &str) {
        match &mut self.node_mut(id).data {
            Data::Text { value, raw } => {
                if value != text {
                    *value = text.to_string();
                    *raw = None;
                }
            }
            Data::CData(value) => *value = text.to_string(),
            _ => {}
        }
    }

    /// Replace the content of element `id` with one text node (none for "").
    pub fn set_text(&mut self, id: NodeId, text: &str) {
        let children: Vec<NodeId> = self.children(id).to_vec();
        if let [only] = children[..]
            && self.kind(only) == NodeKind::Text
        {
            return self.set_text_value(only, text);
        }
        for child in children {
            self.detach(child);
        }
        if !text.is_empty() {
            self.append_text(id, text);
        }
    }

    /// A new element named `name` (with its prefix) as the last child of `parent`.
    pub fn append_element(&mut self, parent: NodeId, name: &str) -> NodeId {
        let index = self.children(parent).len();
        self.insert_element(parent, index, name)
    }

    /// A new element named `name` at `index` among all children of `parent`.
    pub fn insert_element(&mut self, parent: NodeId, index: usize, name: &str) -> NodeId {
        let id = self.push(Data::Element(Element { name: name.to_string(), attributes: Vec::new(), empty: true, raw_start: None, raw_end: None }));
        self.attach(parent, Some(index), id);
        id
    }

    /// A new text node as the last child of `parent`.
    pub fn append_text(&mut self, parent: NodeId, text: &str) -> NodeId {
        let id = self.push(Data::Text { value: text.to_string(), raw: None });
        let index = self.children(parent).len();
        self.attach(parent, Some(index), id);
        id
    }

    /// Move a detached node (from [`deep_clone`](Self::deep_clone) or
    /// [`detach`](Self::detach)) to `index` among the children of `parent`.
    pub fn insert(&mut self, parent: NodeId, index: usize, id: NodeId) {
        if self.parent(id).is_some() {
            self.detach(id);
        }
        self.attach(parent, Some(index), id);
    }

    /// Insert detached `id` just before `sibling`.
    pub fn insert_before(&mut self, sibling: NodeId, id: NodeId) {
        let Some(parent) = self.parent(sibling) else { return };
        let index = self.children(parent).iter().position(|&c| c == sibling).unwrap_or(0);
        self.insert(parent, index, id);
    }

    /// Insert detached `id` just after `sibling`.
    pub fn insert_after(&mut self, sibling: NodeId, id: NodeId) {
        let Some(parent) = self.parent(sibling) else { return };
        let index = self.children(parent).iter().position(|&c| c == sibling).map_or(0, |i| i + 1);
        self.insert(parent, index, id);
    }

    /// Take `id` out of the tree. It can be inserted again.
    pub fn detach(&mut self, id: NodeId) {
        let Some(parent) = self.node(id).parent else { return };
        self.node_mut(parent).children.retain(|&c| c != id);
        self.node_mut(id).parent = None;
    }

    /// A detached copy of `id` and its descendants (they keep their source
    /// text, so an unchanged copy is written as the original was).
    pub fn deep_clone(&mut self, id: NodeId) -> NodeId {
        let data = self.node(id).data.clone();
        let copy = self.push(data);
        for child in self.children(id).to_vec() {
            let c = self.deep_clone(child);
            self.node_mut(c).parent = Some(copy);
            self.node_mut(copy).children.push(c);
        }
        copy
    }

    /// Copy `id` of `other` (with its descendants) into this document, detached.
    pub fn import(&mut self, other: &Document, id: NodeId) -> NodeId {
        let copy = self.push(other.node(id).data.clone());
        for &child in other.children(id) {
            let c = self.import(other, child);
            self.node_mut(c).parent = Some(copy);
            self.node_mut(copy).children.push(c);
        }
        copy
    }

    // ----- Writing --------------------------------------------------------

    /// The document as text, unchanged nodes as they were written.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.write(false)
    }

    /// The document with every node written from its parts, as a changed
    /// node is.
    pub fn to_bytes_regenerated(&self) -> Vec<u8> {
        self.write(true)
    }

    fn write(&self, regenerate: bool) -> Vec<u8> {
        let mut out = String::with_capacity(self.nodes.len() * 16);
        if self.bom {
            out.push('\u{feff}');
        }
        // Iterative, as parts nest deeply enough (drawings in tables in
        // tables) to worry a recursive writer's stack.
        enum Step {
            Open(NodeId),
            Close(NodeId),
        }
        let mut stack: Vec<Step> = self.children(Self::ROOT).iter().rev().map(|&c| Step::Open(c)).collect();
        while let Some(step) = stack.pop() {
            match step {
                Step::Open(id) => match &self.node(id).data {
                    Data::Document => {}
                    Data::Element(e) => {
                        let children = self.children(id);
                        let empty = e.empty && children.is_empty();
                        match (&e.raw_start, regenerate) {
                            // A tag written `<a/>` that now has content, or
                            // `<a></a>` that now has none, is written anew.
                            (Some(raw), false) if raw.ends_with("/>") == empty => out.push_str(raw),
                            _ => {
                                out.push('<');
                                out.push_str(&e.name);
                                for a in &e.attributes {
                                    out.push(' ');
                                    out.push_str(&a.name);
                                    out.push_str("=\"");
                                    escape_attribute(&a.value, &mut out);
                                    out.push('"');
                                }
                                out.push_str(if empty { "/>" } else { ">" });
                            }
                        }
                        if !empty {
                            stack.push(Step::Close(id));
                            stack.extend(children.iter().rev().map(|&c| Step::Open(c)));
                        }
                    }
                    Data::Text { value, raw } => match (raw, regenerate) {
                        (Some(raw), false) => out.push_str(raw),
                        _ => escape_text(value, &mut out),
                    },
                    Data::CData(value) => {
                        out.push_str("<![CDATA[");
                        out.push_str(value);
                        out.push_str("]]>");
                    }
                    Data::Comment(value) => {
                        out.push_str("<!--");
                        out.push_str(value);
                        out.push_str("-->");
                    }
                    Data::Pi(value) | Data::Declaration(value) => {
                        out.push_str("<?");
                        out.push_str(value);
                        out.push_str("?>");
                    }
                    Data::Doctype(raw) => out.push_str(raw),
                },
                Step::Close(id) => {
                    let Some(e) = self.element(id) else { continue };
                    match (&e.raw_end, regenerate) {
                        (Some(raw), false) => out.push_str(raw),
                        _ => {
                            out.push_str("</");
                            out.push_str(&e.name);
                            out.push('>');
                        }
                    }
                }
            }
        }
        out.into_bytes()
    }

    /// The XML declaration's content (`xml version="1.0" …`), if any.
    pub fn declaration(&self) -> Option<&str> {
        self.children(Self::ROOT).iter().find_map(|&c| match &self.node(c).data {
            Data::Declaration(d) => Some(d.as_str()),
            _ => None,
        })
    }

    /// The content of a processing instruction or comment node.
    pub fn raw_content(&self, id: NodeId) -> Option<&str> {
        match &self.node(id).data {
            Data::Pi(s) | Data::Comment(s) | Data::Declaration(s) | Data::Doctype(s) | Data::CData(s) => Some(s),
            _ => None,
        }
    }

    // ----- Arena ----------------------------------------------------------

    fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }

    fn push(&mut self, data: Data) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node { parent: None, children: Vec::new(), data });
        id
    }

    fn attach(&mut self, parent: NodeId, index: Option<usize>, id: NodeId) {
        let children = &mut self.node_mut(parent).children;
        let index = index.unwrap_or(children.len()).min(children.len());
        children.insert(index, id);
        self.node_mut(id).parent = Some(parent);
        if let Data::Element(e) = &mut self.node_mut(parent).data {
            e.empty = false;
        }
    }
}

// ----- Escaping --------------------------------------------------------------

fn escape_text(value: &str, out: &mut String) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            // A carriage return would be read back as a line feed.
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
}

fn escape_attribute(value: &str, out: &mut String) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            // Whitespace other than a space would be read back as a space.
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
}

// ----- Parsing ---------------------------------------------------------------

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    doc: Document,
}

impl Parser<'_> {
    fn error<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError { offset: self.pos, message: message.into() })
    }

    fn rest(&self) -> &str {
        &self.input[self.pos..]
    }

    fn find(&self, pattern: &str) -> Result<usize, ParseError> {
        match self.rest().find(pattern) {
            Some(i) => Ok(self.pos + i),
            None => self.error(format!("{pattern} がありません")),
        }
    }

    fn add(&mut self, parent: NodeId, data: Data) -> NodeId {
        let id = self.doc.push(data);
        self.doc.node_mut(parent).children.push(id);
        self.doc.node_mut(id).parent = Some(parent);
        id
    }

    fn run(mut self) -> Result<Document, ParseError> {
        let mut stack: Vec<NodeId> = vec![Document::ROOT];
        let mut seen_root = false;
        while self.pos < self.input.len() {
            let parent = *stack.last().expect("the document node");
            let rest = self.rest();
            if rest.starts_with("<?") {
                let end = self.find("?>")?;
                let content = self.input[self.pos + 2..end].to_string();
                let is_declaration = content.starts_with("xml") && content[3..].starts_with(|c: char| c.is_ascii_whitespace()) && self.doc.nodes.len() == 1;
                self.add(parent, if is_declaration { Data::Declaration(content) } else { Data::Pi(content) });
                self.pos = end + 2;
            } else if rest.starts_with("<!--") {
                let end = self.find("-->")?;
                let content = self.input[self.pos + 4..end].to_string();
                self.add(parent, Data::Comment(content));
                self.pos = end + 3;
            } else if rest.starts_with("<![CDATA[") {
                if parent == Document::ROOT {
                    return self.error("要素の外に CDATA があります");
                }
                let end = self.find("]]>")?;
                let content = self.input[self.pos + 9..end].to_string();
                self.add(parent, Data::CData(content));
                self.pos = end + 3;
            } else if rest.starts_with("<!DOCTYPE") {
                // Up to the '>' outside the internal subset's brackets.
                let mut depth = 0i32;
                let mut end = None;
                for (i, b) in rest.bytes().enumerate() {
                    match b {
                        b'[' => depth += 1,
                        b']' => depth -= 1,
                        b'>' if depth <= 0 => {
                            end = Some(self.pos + i);
                            break;
                        }
                        _ => {}
                    }
                }
                let Some(end) = end else { return self.error("DOCTYPE が閉じていません") };
                let content = self.input[self.pos..=end].to_string();
                self.add(parent, Data::Doctype(content));
                self.pos = end + 1;
            } else if rest.starts_with("</") {
                let start = self.pos;
                self.pos += 2;
                let name = self.name()?;
                self.skip_whitespace();
                if !self.rest().starts_with('>') {
                    return self.error("終了タグが閉じていません");
                }
                self.pos += 1;
                if parent == Document::ROOT || self.doc.name(parent) != name {
                    return self.error(format!("終了タグ </{name}> が開始タグと合いません"));
                }
                let raw = self.input[start..self.pos].to_string();
                if let Some(e) = self.doc.element_mut(parent) {
                    e.raw_end = Some(raw);
                }
                stack.pop();
            } else if rest.starts_with('<') {
                if parent == Document::ROOT && seen_root {
                    return self.error("ルート要素が 2 つあります");
                }
                let start = self.pos;
                self.pos += 1;
                let name = self.name()?;
                let mut attributes = Vec::new();
                let empty = loop {
                    let before = self.pos;
                    self.skip_whitespace();
                    let rest = self.rest();
                    if rest.starts_with("/>") {
                        self.pos += 2;
                        break true;
                    }
                    if rest.starts_with('>') {
                        self.pos += 1;
                        break false;
                    }
                    if self.pos == before {
                        return self.error("属性の前に空白がありません");
                    }
                    let attr_name = self.name()?;
                    self.skip_whitespace();
                    if !self.rest().starts_with('=') {
                        return self.error(format!("属性 {attr_name} に = がありません"));
                    }
                    self.pos += 1;
                    self.skip_whitespace();
                    let quote = match self.rest().chars().next() {
                        Some(q @ ('"' | '\'')) => q,
                        _ => return self.error(format!("属性 {attr_name} の値が引用符で囲まれていません")),
                    };
                    self.pos += 1;
                    let Some(len) = self.rest().find(quote) else { return self.error("属性の値が閉じていません") };
                    let raw = &self.input[self.pos..self.pos + len];
                    if raw.contains('<') {
                        return self.error("属性の値に < があります");
                    }
                    let value = self.unescape(raw, true)?;
                    self.pos += len + 1;
                    if attributes.iter().any(|a: &Attribute| a.name == attr_name) {
                        return self.error(format!("属性 {attr_name} が重なっています"));
                    }
                    attributes.push(Attribute { name: attr_name, value });
                };
                let raw_start = Some(self.input[start..self.pos].to_string());
                let id = self.add(parent, Data::Element(Element { name, attributes, empty, raw_start, raw_end: None }));
                if parent == Document::ROOT {
                    seen_root = true;
                }
                if !empty {
                    stack.push(id);
                }
            } else {
                let len = rest.find('<').unwrap_or(rest.len());
                let raw = &self.input[self.pos..self.pos + len];
                if parent == Document::ROOT && !raw.chars().all(|c| c.is_ascii_whitespace()) {
                    return self.error("要素の外に文字があります");
                }
                let value = self.unescape(raw, false)?;
                let raw = (raw.contains(['&', '\r', '>'])).then(|| raw.to_string());
                self.add(parent, Data::Text { value, raw });
                self.pos += len;
            }
        }
        if stack.len() > 1 {
            let open = self.doc.name(*stack.last().expect("an element")).to_string();
            return self.error(format!("<{open}> が閉じていません"));
        }
        if !seen_root {
            return self.error("ルート要素がありません");
        }
        Ok(self.doc)
    }

    fn skip_whitespace(&mut self) {
        let rest = self.rest();
        self.pos += rest.len() - rest.trim_start_matches([' ', '\t', '\r', '\n']).len();
    }

    fn name(&mut self) -> Result<String, ParseError> {
        let rest = self.rest();
        let len = rest
            .find(|c: char| c.is_ascii_whitespace() || matches!(c, '>' | '/' | '=' | '<' | '"' | '\''))
            .unwrap_or(rest.len());
        if len == 0 {
            return self.error("名前がありません");
        }
        let name = rest[..len].to_string();
        self.pos += len;
        Ok(name)
    }

    /// Replace references, and normalize line ends (and, in an attribute
    /// value, whitespace) as an XML processor does.
    fn unescape(&self, raw: &str, attribute: bool) -> Result<String, ParseError> {
        if !raw.contains(['&', '\r']) && !(attribute && raw.contains(['\t', '\n'])) {
            return Ok(raw.to_string());
        }
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(c) = rest.chars().next() {
            match c {
                '&' => {
                    let Some(end) = rest.find(';') else { return self.error("参照が ; で終わっていません") };
                    let reference = &rest[1..end];
                    let c = match reference {
                        "lt" => '<',
                        "gt" => '>',
                        "amp" => '&',
                        "quot" => '"',
                        "apos" => '\'',
                        r if r.starts_with("#x") => {
                            u32::from_str_radix(&r[2..], 16).ok().and_then(char::from_u32).ok_or(ParseError { offset: self.pos, message: format!("文字参照 &{r}; が正しくありません") })?
                        }
                        r if r.starts_with('#') => {
                            r[1..].parse::<u32>().ok().and_then(char::from_u32).ok_or(ParseError { offset: self.pos, message: format!("文字参照 &{r}; が正しくありません") })?
                        }
                        r => return self.error(format!("知らない実体参照 &{r}; です")),
                    };
                    out.push(c);
                    rest = &rest[end + 1..];
                }
                '\r' => {
                    // CR LF and a lone CR are a line feed.
                    rest = rest[1..].strip_prefix('\n').unwrap_or(&rest[1..]);
                    out.push(if attribute { ' ' } else { '\n' });
                }
                '\t' | '\n' if attribute => {
                    out.push(' ');
                    rest = &rest[1..];
                }
                c => {
                    out.push(c);
                    rest = &rest[c.len_utf8()..];
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n\
<p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"urn:p\"   xmlns:mc='urn:mc' mc:Ignorable=\"x14\">\
<!-- a comment --><?pi data?>\
<p:cSld><p:spTree>\n  <p:sp><a:t xml:space=\"preserve\"> a &amp; b &#x3042; &gt; </a:t><a:empty/><a:e2></a:e2></p:sp>\
<mc:AlternateContent><mc:Choice Requires=\"x14\"><x14:foo xmlns:x14=\"urn:x14\" v='1&#10;2'/></mc:Choice></mc:AlternateContent>\
<![CDATA[<raw>]]></p:spTree></p:cSld></p:sld >";

    #[test]
    fn untouched_document_is_written_as_read() {
        let doc = Document::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(String::from_utf8(doc.to_bytes()).unwrap(), SAMPLE);
    }

    #[test]
    fn regenerated_document_reads_back_the_same() {
        let doc = Document::parse(SAMPLE.as_bytes()).unwrap();
        let again = Document::parse(&doc.to_bytes_regenerated()).unwrap();
        let t = again.descendants(Document::ROOT).into_iter().find(|&n| again.name(n) == "a:t").unwrap();
        assert_eq!(again.text(t), " a & b あ > ");
        let foo = again.descendants(Document::ROOT).into_iter().find(|&n| again.local_name(n) == "foo").unwrap();
        assert_eq!(again.attr(foo, "v"), Some("1\n2"));
        assert_eq!(again.to_bytes_regenerated(), doc.to_bytes_regenerated());
    }

    #[test]
    fn namespaces_resolve_from_ancestors() {
        let doc = Document::parse(SAMPLE.as_bytes()).unwrap();
        let t = doc.descendants(Document::ROOT).into_iter().find(|&n| doc.name(n) == "a:t").unwrap();
        assert_eq!(doc.namespace(t), Some("http://schemas.openxmlformats.org/drawingml/2006/main"));
        assert_eq!(doc.resolve_prefix(t, Some("xml")), Some(XML_NAMESPACE));
        assert_eq!(doc.lookup_prefix(t, "urn:p").as_deref(), Some("p"));
    }

    #[test]
    fn changing_a_node_rewrites_only_it() {
        let mut doc = Document::parse(SAMPLE.as_bytes()).unwrap();
        let t = doc.descendants(Document::ROOT).into_iter().find(|&n| doc.name(n) == "a:t").unwrap();
        doc.set_text(t, "x < y");
        let out = String::from_utf8(doc.to_bytes()).unwrap();
        assert_eq!(out, SAMPLE.replace(" a &amp; b &#x3042; &gt; ", "x &lt; y"));
        let empty = doc.descendants(Document::ROOT).into_iter().find(|&n| doc.name(n) == "a:empty").unwrap();
        doc.append_text(empty, "now");
        doc.set_attr(empty, "b", "\"q\"");
        let out = String::from_utf8(doc.to_bytes()).unwrap();
        assert!(out.contains("<a:empty b=\"&quot;q&quot;\">now</a:empty>"), "{out}");
    }

    #[test]
    fn paths_find_the_same_element() {
        let doc = Document::parse(SAMPLE.as_bytes()).unwrap();
        let t = doc.descendants(Document::ROOT).into_iter().find(|&n| doc.name(n) == "a:t").unwrap();
        let path = doc.element_path(t);
        assert_eq!(path, vec![0, 0, 0, 0]);
        assert_eq!(doc.by_path(&path), Some(t));
    }

    #[test]
    fn clone_and_move() {
        let mut doc = Document::parse(b"<r><a x=\"1\"/><b/></r>").unwrap();
        let root = doc.root_element().unwrap();
        let a = doc.child(root, "a").unwrap();
        let copy = doc.deep_clone(a);
        doc.set_attr(copy, "x", "2");
        doc.insert(root, 2, copy);
        let b = doc.child(root, "b").unwrap();
        doc.detach(b);
        assert_eq!(doc.to_bytes(), b"<r><a x=\"1\"/><a x=\"2\"/></r>");
    }

    #[test]
    fn rejects_malformed_input() {
        for bad in ["<a>", "<a></b>", "<a x=1/>", "<a x='1' x='2'/>", "text", "<a/><b/>", "<a>&bogus;</a>", "<a b=\"<\"/>"] {
            assert!(Document::parse(bad.as_bytes()).is_err(), "{bad}");
        }
    }

    #[test]
    fn line_ends_and_attribute_whitespace_are_normalized() {
        let doc = Document::parse(b"<a v=\"1\t2\">x\r\ny</a>").unwrap();
        let a = doc.root_element().unwrap();
        assert_eq!(doc.attr(a, "v"), Some("1 2"));
        assert_eq!(doc.text(a), "x\ny");
        assert_eq!(doc.to_bytes(), b"<a v=\"1\t2\">x\r\ny</a>");
    }
}
