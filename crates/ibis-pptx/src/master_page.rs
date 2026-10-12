// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スライド マスター表示: a slide master or a layout edited as if it were a
//! slide. `build` makes, from a presentation's file, another one holding a
//! single slide whose shapes are those of the master (or layout), which the
//! ordinary editor opens; `apply` puts the shapes of that slide, as edited,
//! back into the master's (or layout's) part of the file, with the pictures
//! and links they use. What else the part holds (the background, the
//! text styles, the transition, the other layouts, the theme) is left as it was.

use ibis_ooxml::{NodeId, Package, SaveOptions, XmlDocument};

use crate::Error;
use crate::save::pptx::{next_part_name, relative};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_OFFICE_DOCUMENT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const REL_SLIDE_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
const REL_SLIDE_LAYOUT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const REL_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
const CT_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";

/// A page of the master view: a slide master, or one of its layouts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// The part of the file.
    pub part: String,
    /// The layout's name, or the master's (its theme's).
    pub name: String,
    pub master: bool,
    /// The master it belongs to (a master's own part).
    pub of: String,
}

fn err(what: impl Into<String>) -> Error {
    Error::new(what)
}

fn main_part(package: &Package) -> Result<String, Error> {
    package.relationships("")?.into_iter().find(|r| r.kind == REL_OFFICE_DOCUMENT && !r.external).and_then(|r| r.target_part("")).ok_or_else(|| err("プレゼンテーションの本体がありません"))
}

fn rels_part(part: &str) -> String {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") }
}

/// The layouts of master `master`, in its order.
fn layouts_of(package: &Package, master: &str) -> Result<Vec<String>, Error> {
    let rels = package.relationships(master)?;
    let doc = package.xml(master)?;
    let root = doc.root_element().ok_or_else(|| err("マスターが空です"))?;
    let Some(list) = doc.child(root, "sldLayoutIdLst") else { return Ok(Vec::new()) };
    Ok(doc
        .elements(list)
        .filter_map(|e| rel_id(&doc, e))
        .filter_map(|rid| rels.iter().find(|r| r.id == rid && r.kind == REL_SLIDE_LAYOUT).and_then(|r| r.target_part(master)))
        .filter(|p| package.contains(p))
        .collect())
}

/// The value of the `r:id` of `node`, whatever prefix the relationships' namespace has.
fn rel_id(doc: &XmlDocument, node: NodeId) -> Option<String> {
    doc.attributes(node).iter().find(|a| a.name.rsplit_once(':').is_some_and(|(p, l)| l == "id" && doc.resolve_prefix(node, Some(p)) == Some(NS_R))).map(|a| a.value.clone())
}

/// The master pages of `data`: each master, then its layouts.
pub fn pages(data: &[u8]) -> Result<Vec<Page>, Error> {
    let package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    let mut out = Vec::new();
    for rel in package.relationships(&main)?.into_iter().filter(|r| r.kind == REL_SLIDE_MASTER && !r.external) {
        let Some(master) = rel.target_part(&main).filter(|p| package.contains(p)) else { continue };
        let name = package
            .relationships(&master)?
            .into_iter()
            .find(|r| r.kind == REL_THEME && !r.external)
            .and_then(|r| r.target_part(&master))
            .and_then(|t| package.xml(&t).ok())
            .and_then(|d| d.root_element().and_then(|r| d.attr(r, "name").map(str::to_string)))
            .unwrap_or_default();
        out.push(Page { part: master.clone(), name, master: true, of: master.clone() });
        for layout in layouts_of(&package, &master)? {
            let doc = package.xml(&layout)?;
            let name = doc.root_element().and_then(|r| doc.child(r, "cSld")).and_then(|c| doc.attr(c, "name")).unwrap_or("").to_string();
            out.push(Page { part: layout, name, master: false, of: master.clone() });
        }
    }
    Ok(out)
}

/// The tree of shapes of `doc` (`p:cSld/p:spTree`).
fn tree(doc: &XmlDocument) -> Option<NodeId> {
    let root = doc.root_element()?;
    doc.path(root, &["cSld", "spTree"])
}

/// What of a shape tree is not a shape: its own properties (`nvGrpSpPr`, `grpSpPr`) and extensions.
fn is_tree_property(doc: &XmlDocument, node: NodeId) -> bool {
    matches!(doc.local_name(node), "nvGrpSpPr" | "grpSpPr" | "extLst")
}

/// The shapes of tree `tree` taken out (the elements that are not its properties).
fn empty_tree(doc: &mut XmlDocument, tree: NodeId) {
    for child in doc.children(tree).to_vec() {
        if doc.is_element(child) && !is_tree_property(doc, child) {
            doc.detach(child);
        }
    }
}

/// A file like `data` with one slide: the master (or layout) `part` as a slide
/// of its own, the shapes of the part and of the layout it shows over taken out so
/// that they are not drawn twice.
pub fn build(data: &[u8], part: &str) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    let root_name = {
        let doc = package.xml(part)?;
        let root = doc.root_element().ok_or_else(|| err("ページが空です"))?;
        doc.local_name(root).to_string()
    };
    let is_master = match root_name.as_str() {
        "sldMaster" => true,
        "sldLayout" => false,
        _ => return Err(err("マスターでもレイアウトでもありません")),
    };
    // The layout the slide follows: the layout itself, or a layout of the master.
    let (master, layout) = if is_master {
        let layout = layouts_of(&package, part)?.into_iter().next().ok_or_else(|| err("レイアウトの無いマスターです"))?;
        (part.to_string(), layout)
    } else {
        let master = package.relationships(part)?.into_iter().find(|r| r.kind == REL_SLIDE_MASTER && !r.external).and_then(|r| r.target_part(part)).ok_or_else(|| err("マスターの無いレイアウトです"))?;
        (master, part.to_string())
    };
    let _ = master;
    // The slide: the part's own tree, as a `p:sld`.
    let slide_part = next_part_name(&package, "ppt/slides/slide1.xml");
    let mut slide = package.xml(part)?;
    let slide_root = slide.root_element().ok_or_else(|| err("ページが空です"))?;
    let prefix = slide.prefix(slide_root).map(|p| format!("{p}:")).unwrap_or_default();
    slide.rename(slide_root, &format!("{prefix}sld"));
    for name in ["type", "preserve", "userDrawn", "matchingName", "showMasterSp"] {
        slide.remove_attr(slide_root, name);
    }
    // (A layout that does not show its master's shapes: its slide does not either.)
    for child in slide.children(slide_root).to_vec() {
        if slide.is_element(child) && matches!(slide.local_name(child), "clrMap" | "sldLayoutIdLst" | "hf" | "txStyles") {
            slide.detach(child);
        }
    }
    package.set_xml(&slide_part, slide);
    package.set_override(&slide_part, CT_SLIDE)?;
    // Its relationships: those of the part (the pictures, the links), and the layout.
    let rels = package.relationships(part)?;
    for rel in rels.iter().filter(|r| !matches!(r.kind.as_str(), k if k == REL_SLIDE_MASTER || k == REL_SLIDE_LAYOUT)) {
        // The same id, for what refers to it.
        let target = if rel.external { rel.target.clone() } else { match rel.target_part(part) { Some(t) => relative(&slide_part, &t), None => continue } };
        add_relationship_with_id(&mut package, &slide_part, &rel.id, &rel.kind, &target, rel.external)?;
    }
    package.add_relationship(&slide_part, REL_SLIDE_LAYOUT, &relative(&slide_part, &layout), false)?;
    // The part and the layout without their shapes.
    {
        let doc = package.xml_mut(part)?;
        if let Some(t) = tree(doc) {
            empty_tree(doc, t);
        }
    }
    if is_master {
        let doc = package.xml_mut(&layout)?;
        if let Some(t) = tree(doc) {
            empty_tree(doc, t);
        }
    }
    // The presentation: this slide only.
    let rid = package.add_relationship(&main, REL_SLIDE, &relative(&main, &slide_part), false)?;
    let doc = package.xml_mut(&main)?;
    let root = doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    let list = doc.child(root, "sldIdLst").ok_or_else(|| err("スライドの一覧がありません"))?;
    for child in doc.children(list).to_vec() {
        doc.detach(child);
    }
    let x = crate::save::pptx_new::Prefixes::at(doc, list);
    let entry = x.import(doc, &format!("<{p}sldId id=\"256\" {r}id=\"{rid}\"/>", p = x.p, r = x.r))?;
    doc.insert(list, 0, entry);
    // Sections and custom shows name slides that are not there.
    if let Some(shows) = doc.child(root, "custShowLst") {
        doc.detach(shows);
    }
    if let Some(ext) = doc.child(root, "extLst") {
        for e in doc.elements(ext).collect::<Vec<_>>() {
            if doc.descendants(e).into_iter().any(|d| doc.is_element(d) && doc.local_name(d) == "sectionLst") {
                doc.detach(e);
            }
        }
    }
    Ok(package.save(SaveOptions::default())?)
}

/// A relationship of `source` with the id `id` (what refers to it keeps working).
fn add_relationship_with_id(package: &mut Package, source: &str, id: &str, kind: &str, target: &str, external: bool) -> Result<(), Error> {
    let rels = rels_part(source);
    if !package.contains(&rels) {
        let doc = XmlDocument::parse(b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>").map_err(|e| err(format!("関係を作れません: {e}")))?;
        package.set_xml(&rels, doc);
    }
    let doc = package.xml_mut(&rels)?;
    let root = doc.root_element().ok_or_else(|| err("関係が空です"))?;
    let rel = doc.append_element(root, "Relationship");
    doc.set_attr(rel, "Id", id);
    doc.set_attr(rel, "Type", kind);
    doc.set_attr(rel, "Target", target);
    if external {
        doc.set_attr(rel, "TargetMode", "External");
    }
    Ok(())
}

/// The slide part of a file made by `build` (its only slide), and the document of it.
fn only_slide(package: &Package) -> Result<(String, XmlDocument), Error> {
    let main = main_part(package)?;
    let main_doc = package.xml(&main)?;
    let root = main_doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    let list = main_doc.child(root, "sldIdLst").ok_or_else(|| err("スライドがありません"))?;
    let first = main_doc.elements(list).next().ok_or_else(|| err("スライドがありません"))?;
    let rid = rel_id(&main_doc, first).ok_or_else(|| err("スライドの関係がありません"))?;
    let slide = package.relationships(&main)?.into_iter().find(|r| r.id == rid).and_then(|r| r.target_part(&main)).ok_or_else(|| err("スライドの部品がありません"))?;
    let doc = package.xml(&slide)?;
    Ok((slide, doc))
}

/// `data` with the shapes of master (or layout) `part` those of the slide of `edited` (a file
/// `build` made from `data`, then edited).
pub fn apply(data: &[u8], edited: &[u8], part: &str) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let edited = Package::open(edited.to_vec())?;
    let (slide_part, slide_doc) = only_slide(&edited)?;
    let slide_tree = tree(&slide_doc).ok_or_else(|| err("スライドの図形の木がありません"))?;
    let shapes: Vec<NodeId> = slide_doc.children(slide_tree).iter().copied().filter(|&n| slide_doc.is_element(n) && !is_tree_property(&slide_doc, n)).collect();
    // The part's tree emptied, then given the shapes.
    let mut doc = package.xml(part)?;
    let tree_id = tree(&doc).ok_or_else(|| err("図形の木がありません"))?;
    empty_tree(&mut doc, tree_id);
    let slide_rels = edited.relationships(&slide_part)?;
    let mut mapped: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut added: Vec<(String, String, String, bool)> = Vec::new();
    let at = doc.children(tree_id).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(tree_id).len());
    for (i, shape) in shapes.iter().enumerate() {
        let copy = doc.import(&slide_doc, *shape);
        // (In the tree first: the prefixes of its attributes are found from its ancestors.)
        doc.insert(tree_id, at + i, copy);
        // The relationships the shape names, made the part's own.
        for node in std::iter::once(copy).chain(doc.descendants(copy)) {
            if !doc.is_element(node) {
                continue;
            }
            let names: Vec<(String, String)> = doc
                .attributes(node)
                .iter()
                .filter(|a| a.name.rsplit_once(':').is_some_and(|(p, _)| doc.resolve_prefix(node, Some(p)) == Some(NS_R)))
                .map(|a| (a.name.clone(), a.value.clone()))
                .collect();
            for (name, value) in names {
                if value.is_empty() {
                    continue;
                }
                let Some(rel) = slide_rels.iter().find(|r| r.id == value) else { continue };
                let new_id = match mapped.get(&value) {
                    Some(id) => id.clone(),
                    None => {
                        let target = if rel.external { rel.target.clone() } else { rel.target_part(&slide_part).map(|t| format!("part:{t}")).unwrap_or_default() };
                        // (Added to the package after the tree is set: ids are made then.)
                        let id = format!("ibis-new-{}", mapped.len());
                        mapped.insert(value.clone(), id.clone());
                        added.push((id.clone(), rel.kind.clone(), target, rel.external));
                        id
                    }
                };
                doc.set_attr(node, &name, &new_id);
            }
        }
    }
    package.set_xml(part, doc);
    // The relationships made, and the parts they name copied from the edited file.
    let mut real: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (placeholder, kind, target, external) in added {
        let new_id = if external {
            package.add_relationship(part, &kind, &target, true)?
        } else {
            let source = target.strip_prefix("part:").unwrap_or(&target).to_string();
            let bytes = edited.read(&source)?;
            // The same part if the file has it with the same bytes, else a copy.
            let name = if package.contains(&source) && package.read(&source)? == bytes {
                source.clone()
            } else if package.contains(&source) {
                next_part_name(&package, &source)
            } else {
                source.clone()
            };
            if name != source || !package.contains(&name) {
                package.set_bytes(&name, bytes);
                if let Some(ct) = edited.content_type(&source)? {
                    let default = package.content_type(&name)?;
                    if default.as_deref() != Some(ct.as_str()) {
                        package.set_override(&name, &ct)?;
                    }
                }
            }
            package.add_relationship(part, &kind, &relative(part, &name), false)?
        };
        real.insert(placeholder, new_id);
    }
    // The placeholders' ids replaced by the real ones.
    if !real.is_empty() {
        let doc = package.xml_mut(part)?;
        let root = doc.root_element().ok_or_else(|| err("ページが空です"))?;
        for node in std::iter::once(root).chain(doc.descendants(root)) {
            if !doc.is_element(node) {
                continue;
            }
            let swaps: Vec<(String, String)> = doc.attributes(node).iter().filter(|a| real.contains_key(&a.value)).map(|a| (a.name.clone(), real[&a.value].clone())).collect();
            for (name, value) in swaps {
                doc.set_attr(node, &name, &value);
            }
        }
    }
    Ok(package.save(SaveOptions::default())?)
}

/// Which text style of a master (`p:txStyles`): the title's, the body's (the
/// placeholders of the text of a slide), or the other texts'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleKind {
    Title,
    Body,
    Other,
}

impl StyleKind {
    fn element(self) -> &'static str {
        match self {
            StyleKind::Title => "titleStyle",
            StyleKind::Body => "bodyStyle",
            StyleKind::Other => "otherStyle",
        }
    }
}

/// How the text of a level looks (`a:defRPr`): what is not stated is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LevelStyle {
    /// The size in hundredths of a point (`sz`).
    pub size: Option<u32>,
    pub bold: Option<bool>,
    /// The colour, `0xRRGGBB`, when it is a plain one (a theme colour is `None`).
    pub color: Option<u32>,
    /// The typeface (`a:latin`, or `a:ea` if there is no latin one); `+mj-lt`-like theme fonts as written.
    pub font: Option<String>,
}

/// The text styles of master `master`: for each style and level (1 to 9) it states.
pub fn text_styles(data: &[u8], master: &str) -> Result<Vec<(StyleKind, u8, LevelStyle)>, Error> {
    let package = Package::open(data.to_vec())?;
    let doc = package.xml(master)?;
    let root = doc.root_element().ok_or_else(|| err("マスターが空です"))?;
    let Some(styles) = doc.child(root, "txStyles") else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for kind in [StyleKind::Title, StyleKind::Body, StyleKind::Other] {
        let Some(style) = doc.child(styles, kind.element()) else { continue };
        for level in 1..=9u8 {
            let Some(def) = doc.child(style, &format!("lvl{level}pPr")).and_then(|l| doc.child(l, "defRPr")) else { continue };
            let color = doc.child(def, "solidFill").and_then(|f| doc.child(f, "srgbClr")).and_then(|c| doc.attr(c, "val")).and_then(|v| u32::from_str_radix(v, 16).ok());
            let font = doc.child(def, "latin").or_else(|| doc.child(def, "ea")).and_then(|f| doc.attr(f, "typeface")).map(str::to_string);
            out.push((kind, level, LevelStyle { size: doc.attr(def, "sz").and_then(|v| v.parse().ok()), bold: doc.attr(def, "b").map(|v| v == "1" || v == "true"), color, font }));
        }
    }
    Ok(out)
}

/// `data` with level `level` (1 to 9) of `kind` in master `master` given the look `change` states
/// (what it leaves `None` is left as it is): for every slide that does not say otherwise itself.
pub fn set_text_style(data: &[u8], master: &str, kind: StyleKind, level: u8, change: &LevelStyle) -> Result<Vec<u8>, Error> {
    if !(1..=9).contains(&level) {
        return Err(err("レベルは 1〜9 です"));
    }
    let mut package = Package::open(data.to_vec())?;
    let doc = package.xml_mut(master)?;
    let root = doc.root_element().ok_or_else(|| err("マスターが空です"))?;
    let styles = doc.child(root, "txStyles").ok_or_else(|| err("文字の書式がありません"))?;
    let style = doc.child(styles, kind.element()).ok_or_else(|| err("この文字の書式がありません"))?;
    let x = crate::save::pptx_new::Prefixes::at(doc, style);
    let name = format!("lvl{level}pPr");
    // The level, made in its place if the master has none.
    let lvl = match doc.child(style, &name) {
        Some(l) => l,
        None => {
            let at = doc.children(style).iter().position(|&c| doc.is_element(c) && doc.local_name(c).strip_prefix("lvl").and_then(|r| r.strip_suffix("pPr")).and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| n > level)).unwrap_or(doc.children(style).len());
            let l = x.import(doc, &format!("<{a}{name}/>", a = x.a))?;
            doc.insert(style, at, l);
            l
        }
    };
    let def = match doc.child(lvl, "defRPr") {
        Some(d) => d,
        None => {
            let d = x.import(doc, &format!("<{a}defRPr/>", a = x.a))?;
            // After the paragraph properties' own children (`a:buNone` and the like): last.
            let end = doc.children(lvl).len();
            doc.insert(lvl, end, d);
            d
        }
    };
    if let Some(size) = change.size {
        doc.set_attr(def, "sz", &size.clamp(100, 400_000).to_string());
    }
    if let Some(bold) = change.bold {
        doc.set_attr(def, "b", if bold { "1" } else { "0" });
    }
    if let Some(color) = change.color {
        for fill in doc.children(def).to_vec() {
            if doc.is_element(fill) && matches!(doc.local_name(fill), "solidFill" | "gradFill" | "noFill" | "pattFill" | "blipFill" | "grpFill") {
                doc.detach(fill);
            }
        }
        let fill = x.import(doc, &format!("<{a}solidFill><{a}srgbClr val=\"{color:06X}\"/></{a}solidFill>", a = x.a, color = color & 0xFF_FFFF))?;
        // After the outline (`a:ln`), before the effects and the typefaces.
        let at = doc.children(def).iter().position(|&c| doc.is_element(c) && doc.local_name(c) != "ln").unwrap_or(doc.children(def).len());
        doc.insert(def, at, fill);
    }
    if let Some(font) = &change.font {
        for tag in ["latin", "ea"] {
            match doc.child(def, tag) {
                Some(f) => doc.set_attr(f, "typeface", font),
                None => {
                    let f = x.import(doc, &format!("<{a}{tag} typeface=\"{}\"/>", crate::save::pptx_new::escape(font), a = x.a))?;
                    // Before the other typefaces and the links.
                    let order = ["latin", "ea", "cs", "sym", "hlinkClick", "hlinkMouseOver", "rtl", "extLst"];
                    let mine = order.iter().position(|o| *o == tag).unwrap_or(0);
                    let at = doc.children(def).iter().position(|&c| doc.is_element(c) && order.iter().position(|o| *o == doc.local_name(c)).is_some_and(|p| p > mine)).unwrap_or(doc.children(def).len());
                    doc.insert(def, at, f);
                }
            }
        }
    }
    Ok(package.save(SaveOptions::default())?)
}
