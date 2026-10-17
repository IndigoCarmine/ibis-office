// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Embedded objects on a sheet added, moved or removed. Excel keeps one in
//! three places: the sheet's `oleObjects` (an `mc:AlternateContent` whose
//! x14 Choice has the `objectPr` with the preview and the anchor), the
//! object's part (`xl/embeddings/oleObjectN.bin`), and a shape of the
//! sheet's VML drawing (`legacyDrawing`) whose number is the object's
//! `shapeId`.

use ibis_doc_model::{EmuRect, Presentation, Shape, ShapeKind, Sheet};
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::{REL_NS, XlsxObjectSource, detached, prefix_of, unsupported};
use crate::Error;

const REL_OLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject";
const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const REL_VML: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing";
const CT_OLE: &str = "application/vnd.openxmlformats-officedocument.oleObject";
const CT_VML: &str = "application/vnd.openxmlformats-officedocument.vmlDrawing";
const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const NS_XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const EMU_PER_PIXEL: i64 = 9525;
const EMU_PER_POINT: f64 = 12700.0;

/// A corner of an anchor: column, its offset, row, its offset (EMU).
type Corner = (usize, i64, usize, i64);

/// The cells the corners of `frame` fall in.
fn corners(sheet: &Sheet, frame: EmuRect) -> (Corner, Corner) {
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let cell = |offsets: &[i64], v: i64| -> (usize, i64) {
        let i = offsets.partition_point(|&o| o <= v).saturating_sub(1).min(offsets.len().saturating_sub(2));
        (i, v - offsets.get(i).copied().unwrap_or(0))
    };
    let ((c0, co0), (r0, ro0)) = (cell(&xs, frame.x), cell(&ys, frame.y));
    let ((c1, co1), (r1, ro1)) = (cell(&xs, frame.x + frame.width), cell(&ys, frame.y + frame.height));
    ((c0, co0, r0, ro0), (c1, co1, r1, ro1))
}

/// A new object's shape put on the sheet: its part, its preview, its VML
/// shape and its `oleObjects` entry.
pub(super) fn add_object(package: &mut Package, sheet_part: &str, sheet: &Sheet, shape: &Shape, current: &Presentation) -> Result<(), Error> {
    let ShapeKind::Picture(picture) = &shape.kind else { return Err(unsupported("この埋め込みオブジェクトの追加")) };
    let object = picture.ole.and_then(|id| current.ole(id)).ok_or_else(|| Error::new("埋め込みオブジェクトのデータがありません"))?;
    if shape.rotation != 0 || shape.flip_h || shape.flip_v {
        return Err(unsupported("回転・反転した埋め込みオブジェクトの追加"));
    }
    // The object's part and the preview, related from the sheet.
    let ole_part = super::super::pptx::next_part_name(package, "xl/embeddings/oleObject1.bin");
    package.set_bytes(&ole_part, object.data.clone());
    package.set_override(&ole_part, CT_OLE)?;
    let ole_rel = package.add_relationship(sheet_part, REL_OLE, &super::super::pptx::relative(sheet_part, &ole_part), false)?;
    let image_part = super::shape_edits::write_image(package, current, picture.media)?;
    let image_rel = package.add_relationship(sheet_part, REL_IMAGE, &super::super::pptx::relative(sheet_part, &image_part), false)?;

    let (from, to) = corners(sheet, shape.frame);
    let shape_id = add_vml_shape(package, sheet_part, &image_part, shape.frame, from, to)?;

    let doc = package.xml_mut(sheet_part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let r = r_prefix(doc, root);
    let p = prefix_of(doc, root);
    let prog_id = super::super::pptx_new::escape(&object.prog_id);
    let aspect = if object.as_icon { " dvAspect=\"DVASPECT_ICON\"" } else { "" };
    let corner = |name: &str, (c, co, row, ro): Corner| format!("<{p}{name}><xdr:col>{c}</xdr:col><xdr:colOff>{co}</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>{ro}</xdr:rowOff></{p}{name}>");
    let entry = format!(
        "<mc:AlternateContent xmlns:mc=\"{NS_MC}\"><mc:Choice xmlns:x14=\"{NS_X14}\" Requires=\"x14\"><{p}oleObject progId=\"{prog_id}\"{aspect} shapeId=\"{shape_id}\" {r}:id=\"{ole_rel}\"><{p}objectPr defaultSize=\"0\" autoPict=\"0\" {r}:id=\"{image_rel}\"><{p}anchor xmlns:xdr=\"{NS_XDR}\" moveWithCells=\"1\">{}{}</{p}anchor></{p}objectPr></{p}oleObject></mc:Choice><mc:Fallback><{p}oleObject progId=\"{prog_id}\"{aspect} shapeId=\"{shape_id}\" {r}:id=\"{ole_rel}\"/></mc:Fallback></mc:AlternateContent>",
        corner("from", from),
        corner("to", to),
    );
    let node = import(doc, root, &entry)?;
    let list = match doc.child(root, "oleObjects") {
        Some(l) => l,
        None => {
            let l = detached(doc, &format!("{p}oleObjects"));
            place(doc, root, l, &["controls", "webPublishItems", "tableParts", "extLst"]);
            l
        }
    };
    let n = doc.children(list).len();
    doc.insert(list, n, node);
    Ok(())
}

/// A moved or resized object's anchors (its `objectPr`'s and its VML
/// shape's) set to `frame`.
pub(super) fn move_object(package: &mut Package, source: &XlsxObjectSource, sheet: &Sheet, frame: EmuRect) -> Result<(), Error> {
    let (from, to) = corners(sheet, frame);
    let doc = package.xml_mut(&source.sheet)?;
    let objects = object_nodes(doc, &source.object_rel);
    let anchors: Vec<NodeId> = objects.iter().filter_map(|&o| doc.child(o, "objectPr")).filter_map(|pr| doc.child(pr, "anchor")).collect();
    if anchors.is_empty() {
        return Err(unsupported("位置の無い埋め込みオブジェクトの移動"));
    }
    for anchor in anchors {
        for (name, (c, co, r, ro)) in [("from", from), ("to", to)] {
            let Some(corner) = doc.child(anchor, name) else { continue };
            for (local, value) in [("col", c as i64), ("colOff", co), ("row", r as i64), ("rowOff", ro)] {
                if let Some(e) = doc.child(corner, local) {
                    doc.set_text(e, &value.to_string());
                }
            }
        }
    }
    let shape_id = shape_id(doc, &objects);
    if let Some(n) = shape_id {
        edit_vml_shape(package, &source.sheet, n, |shape| Some(moved_shape(shape, frame, from, to)))?;
    }
    Ok(())
}

/// A changed preview shown by the object's VML shape too: the VML drawing's
/// relationship to `image_part` set in place of the old one.
pub(super) fn set_vml_image(package: &mut Package, source: &XlsxObjectSource, image_part: &str) -> Result<(), Error> {
    let doc = package.xml(&source.sheet)?;
    let Some(n) = shape_id(&doc, &object_nodes(&doc, &source.object_rel)) else { return Ok(()) };
    let Some(vml) = vml_part(package, &source.sheet)? else { return Ok(()) };
    let text = vml_text(package, &vml)?;
    let Some((a, b)) = find_shape(&text, n) else { return Ok(()) };
    let Some(old) = attr_value(&text[a..b], "o:relid") else { return Ok(()) };
    let rel = package.add_relationship(&vml, REL_IMAGE, &super::super::pptx::relative(&vml, image_part), false)?;
    let shape = text[a..b].replacen(&format!("o:relid=\"{old}\""), &format!("o:relid=\"{rel}\""), 1);
    package.set_bytes(&vml, format!("{}{shape}{}", &text[..a], &text[b..]).into_bytes());
    drop_relationship(package, &vml, &old)?;
    Ok(())
}

/// A removed object gone from the sheet: its `oleObjects` entry (and the
/// list, left empty), its VML shape, and its part and preview when nothing
/// else relates to them.
pub(super) fn remove_object(package: &mut Package, source: &XlsxObjectSource) -> Result<(), Error> {
    let doc = package.xml_mut(&source.sheet)?;
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let objects = object_nodes(doc, &source.object_rel);
    if objects.is_empty() {
        return Err(unsupported("この埋め込みオブジェクトの削除"));
    }
    let shape_id = shape_id(doc, &objects);
    // The images the object's entries show (Choice's and any other).
    let mut rels = vec![source.object_rel.clone()];
    for &o in &objects {
        if let Some(pr) = doc.child(o, "objectPr")
            && let Some(id) = rel_id(doc, pr)
        {
            rels.push(id);
        }
    }
    let list = doc.child(root, "oleObjects");
    for &o in &objects {
        // The entry: the list's child holding the object.
        let mut at = o;
        while let Some(parent) = doc.parent(at) {
            if Some(parent) == list {
                doc.detach(at);
                break;
            }
            at = parent;
        }
    }
    if let Some(l) = list
        && doc.elements(l).next().is_none()
    {
        doc.detach(l);
    }
    rels.dedup();
    for rel in rels {
        drop_relationship(package, &source.sheet, &rel)?;
    }
    if let Some(n) = shape_id {
        let mut image = None;
        edit_vml_shape(package, &source.sheet, n, |shape| {
            image = attr_value(shape, "o:relid");
            None
        })?;
        if let (Some(rel), Some(vml)) = (image, vml_part(package, &source.sheet)?) {
            drop_relationship(package, &vml, &rel)?;
        }
        remove_empty_vml(package, &source.sheet)?;
    }
    Ok(())
}

// ----- The sheet's XML -----------------------------------------------------

/// The `r:` prefix of the sheet (declared on its root if it has none).
fn r_prefix(doc: &mut XmlDocument, root: NodeId) -> String {
    match doc.lookup_prefix(root, REL_NS) {
        Some(p) if !p.is_empty() => p,
        _ => {
            doc.set_attr(root, "xmlns:r", REL_NS);
            "r".into()
        }
    }
}

/// `xml` (one element written with the sheet's prefixes) parsed and
/// imported into `doc`, detached.
fn import(doc: &mut XmlDocument, root: NodeId, xml: &str) -> Result<NodeId, Error> {
    let main = doc.namespace(root).unwrap_or(super::MAIN_NS).to_string();
    let p = prefix_of(doc, root);
    let r = doc.lookup_prefix(root, REL_NS).unwrap_or_else(|| "r".into());
    let main_decl = match p.strip_suffix(':') {
        Some(p) => format!("xmlns:{p}=\"{main}\""),
        None => format!("xmlns=\"{main}\""),
    };
    let wrapper = format!("<w {main_decl} xmlns:{r}=\"{REL_NS}\">{xml}</w>");
    let parsed = XmlDocument::parse(wrapper.as_bytes()).map_err(|e| Error::new(format!("新しい要素を作れません: {e}")))?;
    let w = parsed.root_element().ok_or_else(|| Error::new("新しい要素を作れません"))?;
    let element = parsed.elements(w).next().ok_or_else(|| Error::new("新しい要素を作れません"))?;
    Ok(doc.import(&parsed, element))
}

/// Put `e` among `parent`'s children before the first of `before` present, or at the end.
fn place(doc: &mut XmlDocument, parent: NodeId, e: NodeId, before: &[&str]) {
    let anchor = doc.elements(parent).find(|&c| before.contains(&doc.local_name(c)));
    match anchor {
        Some(a) => doc.insert_before(a, e),
        None => {
            let n = doc.children(parent).len();
            doc.insert(parent, n, e);
        }
    }
}

/// The value of an element's `r:id` (whatever its prefix).
fn rel_id(doc: &XmlDocument, node: NodeId) -> Option<String> {
    doc.attributes(node).iter().find(|a| a.name.split_once(':').is_some_and(|(p, l)| l == "id" && p != "xmlns")).map(|a| a.value.clone())
}

/// The `oleObject` elements of the sheet for relationship `rel` (the
/// Choice's and the Fallback's).
fn object_nodes(doc: &XmlDocument, rel: &str) -> Vec<NodeId> {
    let Some(root) = doc.root_element() else { return Vec::new() };
    let Some(list) = doc.child(root, "oleObjects") else { return Vec::new() };
    doc.descendants(list).into_iter().filter(|&n| doc.is_element(n) && doc.local_name(n) == "oleObject" && rel_id(doc, n).as_deref() == Some(rel)).collect()
}

fn shape_id(doc: &XmlDocument, objects: &[NodeId]) -> Option<u32> {
    objects.iter().find_map(|&o| doc.attr(o, "shapeId")?.trim().parse().ok())
}

/// Relationship `id` of `source` removed, and the part it went to when no
/// other relationship goes there (with its content type override and its
/// own relationships part).
fn drop_relationship(package: &mut Package, source: &str, id: &str) -> Result<(), Error> {
    let Some(rel) = package.relationships(source)?.into_iter().find(|r| r.id == id) else { return Ok(()) };
    package.remove_relationship(source, id)?;
    if rel.external {
        return Ok(());
    }
    let Some(target) = rel.target_part(source) else { return Ok(()) };
    if !used(package, &target)? {
        package.remove_override(&target)?;
        package.remove(&ibis_ooxml::package::relationships_part(&target));
        package.remove(&target);
    }
    Ok(())
}

/// Whether some relationship goes to `part`.
fn used(package: &Package, part: &str) -> Result<bool, Error> {
    let rels: Vec<String> = package.names().filter(|n| n.ends_with(".rels")).map(str::to_string).collect();
    for rels in rels {
        let (dir, file) = rels.rsplit_once('/').unwrap_or(("", rels.as_str()));
        let Some(file) = file.strip_suffix(".rels") else { continue };
        let Some(dir) = dir.strip_suffix("_rels") else { continue };
        let dir = dir.trim_end_matches('/');
        let owner = if dir.is_empty() { file.to_string() } else { format!("{dir}/{file}") };
        if package.relationships(&owner)?.iter().any(|r| !r.external && r.target_part(&owner).as_deref() == Some(part)) {
            return Ok(true);
        }
    }
    Ok(false)
}

// ----- The VML drawing ----------------------------------------------------

/// The picture frame's shape type Excel gives objects.
const PICTURE_TYPE: &str = "<v:shapetype id=\"_x0000_t75\" coordsize=\"21600,21600\" o:spt=\"75\" o:preferrelative=\"t\"\r\n  path=\"m@4@5l@4@11@9@11@9@5xe\" filled=\"f\" stroked=\"f\">\r\n  <v:stroke joinstyle=\"miter\"/>\r\n  <v:formulas>\r\n   <v:f eqn=\"if lineDrawn pixelLineWidth 0\"/>\r\n   <v:f eqn=\"sum @0 1 0\"/>\r\n   <v:f eqn=\"sum 0 0 @1\"/>\r\n   <v:f eqn=\"prod @2 1 2\"/>\r\n   <v:f eqn=\"prod @3 21600 pixelWidth\"/>\r\n   <v:f eqn=\"prod @3 21600 pixelHeight\"/>\r\n   <v:f eqn=\"sum @0 0 1\"/>\r\n   <v:f eqn=\"prod @6 1 2\"/>\r\n   <v:f eqn=\"prod @7 21600 pixelWidth\"/>\r\n   <v:f eqn=\"sum @8 21600 0\"/>\r\n   <v:f eqn=\"prod @7 21600 pixelHeight\"/>\r\n   <v:f eqn=\"sum @10 21600 0\"/>\r\n  </v:formulas>\r\n  <v:path o:extrusionok=\"f\" gradientshapeok=\"t\" o:connecttype=\"rect\"/>\r\n  <o:lock v:ext=\"edit\" aspectratio=\"t\"/>\r\n </v:shapetype>";

/// The sheet's VML drawing part, if it has one.
fn vml_part(package: &Package, sheet_part: &str) -> Result<Option<String>, Error> {
    Ok(package.relationships(sheet_part)?.into_iter().find(|r| r.kind == REL_VML && !r.external).and_then(|r| r.target_part(sheet_part)))
}

fn vml_text(package: &Package, part: &str) -> Result<String, Error> {
    Ok(String::from_utf8_lossy(&package.read(part)?).into_owned())
}

/// The shapes (`<v:shape …>…</v:shape>`) of a VML drawing, with where they are.
fn vml_shapes(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = text[at..].find("<v:shape ").map(|i| i + at) {
        let Some(end) = text[start..].find("</v:shape>").map(|i| i + start + "</v:shape>".len()) else { break };
        out.push((start, end));
        at = end;
    }
    out
}

/// The number of a VML shape (`id`'s or `o:spid`'s `_x0000_sN`).
fn shape_number(shape: &str) -> Option<u32> {
    ["o:spid", "id"].iter().filter_map(|name| attr_value(shape, name)).find_map(|v| v.strip_prefix("_x0000_s")?.parse().ok())
}

fn find_shape(text: &str, n: u32) -> Option<(usize, usize)> {
    vml_shapes(text).into_iter().find(|&(a, b)| shape_number(&text[a..b]) == Some(n))
}

/// The value of attribute `name` of the start tag at the beginning of `xml`.
fn attr_value(xml: &str, name: &str) -> Option<String> {
    let tag_end = xml.find('>').unwrap_or(xml.len());
    let tag = &xml[..tag_end];
    let mut at = 0;
    while let Some(i) = tag[at..].find(name).map(|i| i + at) {
        let before_ok = i > 0 && tag.as_bytes()[i - 1].is_ascii_whitespace();
        let rest = &tag[i + name.len()..];
        if before_ok && let Some(rest) = rest.strip_prefix('=') {
            let quote = rest.chars().next()?;
            let value = &rest[1..];
            return value.find(quote).map(|e| value[..e].to_string());
        }
        at = i + name.len();
    }
    // The image's `o:relid` is on `v:imagedata`, inside the shape.
    if name == "o:relid" {
        let i = xml.find("<v:imagedata")?;
        return attr_value(&xml[i..], name);
    }
    None
}

/// The VML shape of object number `n` replaced by what `edit` makes of it
/// (removed for `None`).
fn edit_vml_shape(package: &mut Package, sheet_part: &str, n: u32, mut edit: impl FnMut(&str) -> Option<String>) -> Result<(), Error> {
    let Some(vml) = vml_part(package, sheet_part)? else { return Ok(()) };
    let text = vml_text(package, &vml)?;
    let Some((a, b)) = find_shape(&text, n) else { return Ok(()) };
    let shape = edit(&text[a..b]).unwrap_or_default();
    package.set_bytes(&vml, format!("{}{shape}{}", &text[..a], &text[b..]).into_bytes());
    Ok(())
}

/// The sheet's VML drawing removed (with its relationship, its
/// relationships part and the sheet's `legacyDrawing`) when no shape is left.
fn remove_empty_vml(package: &mut Package, sheet_part: &str) -> Result<(), Error> {
    let Some(vml) = vml_part(package, sheet_part)? else { return Ok(()) };
    if !vml_shapes(&vml_text(package, &vml)?).is_empty() {
        return Ok(());
    }
    let ids: Vec<String> = package.relationships(sheet_part)?.into_iter().filter(|r| r.kind == REL_VML && !r.external).map(|r| r.id).collect();
    for rel in package.relationships(&vml)? {
        drop_relationship(package, &vml, &rel.id)?;
    }
    for id in ids {
        drop_relationship(package, sheet_part, &id)?;
    }
    let doc = package.xml_mut(sheet_part)?;
    if let Some(e) = doc.root_element().and_then(|r| doc.child(r, "legacyDrawing")) {
        doc.detach(e);
    }
    Ok(())
}

fn points(emu: i64) -> String {
    let s = format!("{:.2}", emu as f64 / EMU_PER_POINT);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// VML's anchor: columns and rows with offsets in pixels.
fn vml_anchor(from: Corner, to: Corner) -> String {
    let px = |e: i64| e / EMU_PER_PIXEL;
    format!("{}, {}, {}, {}, {}, {}, {}, {}", from.0, px(from.1), from.2, px(from.3), to.0, px(to.1), to.2, px(to.3))
}

fn style(frame: EmuRect, z: u32) -> String {
    format!("position:absolute;margin-left:{}pt;margin-top:{}pt;width:{}pt;height:{}pt;z-index:{z}", points(frame.x), points(frame.y), points(frame.width), points(frame.height))
}

/// A VML shape with its position, size and anchor set anew.
fn moved_shape(shape: &str, frame: EmuRect, from: Corner, to: Corner) -> String {
    let mut out = shape.to_string();
    if let Some(old) = attr_value(shape, "style") {
        let mut parts: Vec<String> = old.split(';').filter(|s| !s.trim().is_empty()).map(str::to_string).collect();
        for (key, value) in [("margin-left", points(frame.x)), ("margin-top", points(frame.y)), ("width", points(frame.width)), ("height", points(frame.height))] {
            let item = format!("{key}:{value}pt");
            match parts.iter_mut().find(|p| p.split(':').next().map(str::trim) == Some(key)) {
                Some(p) => *p = item,
                None => parts.push(item),
            }
        }
        out = out.replacen(&old, &parts.join(";"), 1);
    }
    if let (Some(a), Some(b)) = (out.find("<x:Anchor>"), out.find("</x:Anchor>")) {
        out.replace_range(a + "<x:Anchor>".len()..b, &format!("\r\n   {}", vml_anchor(from, to)));
    }
    out
}

/// The object's VML shape added to the sheet's VML drawing (made, with the
/// sheet's `legacyDrawing`, when it has none), showing `image_part`.
/// Returns its number (the object's `shapeId`).
fn add_vml_shape(package: &mut Package, sheet_part: &str, image_part: &str, frame: EmuRect, from: Corner, to: Corner) -> Result<u32, Error> {
    let existing = vml_part(package, sheet_part)?;
    let part = match &existing {
        Some(p) => p.clone(),
        None => super::super::pptx::next_part_name(package, "xl/drawings/vmlDrawing1.vml"),
    };
    let file_number = part.trim_end_matches(".vml").rsplit(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse::<u32>().ok()).unwrap_or(1).max(1);
    let mut text = match &existing {
        Some(p) => vml_text(package, p)?,
        None => format!("<xml xmlns:v=\"urn:schemas-microsoft-com:vml\"\r\n xmlns:o=\"urn:schemas-microsoft-com:office:office\"\r\n xmlns:x=\"urn:schemas-microsoft-com:office:excel\">\r\n <o:shapelayout v:ext=\"edit\">\r\n  <o:idmap v:ext=\"edit\" data=\"{file_number}\"/>\r\n </o:shapelayout></xml>"),
    };
    // A number past the drawing's (and the sheet's objects') shapes, in its idmap's block.
    let idmap = text.find("<o:idmap").and_then(|i| attr_value(&text[i..], "data")).and_then(|d| d.split(',').next()?.trim().parse::<u32>().ok()).unwrap_or(file_number);
    let in_vml = vml_shapes(&text).into_iter().filter_map(|(a, b)| shape_number(&text[a..b])).max().unwrap_or(0);
    let in_sheet = {
        let doc = package.xml(sheet_part)?;
        doc.root_element().and_then(|r| doc.child(r, "oleObjects")).map(|l| doc.descendants(l).into_iter().filter(|&n| doc.is_element(n) && doc.local_name(n) == "oleObject").filter_map(|n| doc.attr(n, "shapeId")?.parse::<u32>().ok()).max().unwrap_or(0)).unwrap_or(0)
    };
    let n = in_vml.max(in_sheet).max(idmap * 1024) + 1;
    if !text.contains("_x0000_t75") {
        let at = text.find("</o:shapelayout>").map(|i| i + "</o:shapelayout>".len()).or_else(|| text.rfind("</xml>")).unwrap_or(text.len());
        text.insert_str(at, PICTURE_TYPE);
    }
    let image_rel = {
        if existing.is_none() {
            package.set_bytes(&part, Vec::new());
        }
        package.add_relationship(&part, REL_IMAGE, &super::super::pptx::relative(&part, image_part), false)?
    };
    let shape = format!(
        "<v:shape id=\"_x0000_s{n}\" type=\"#_x0000_t75\" style='{}' filled=\"t\" fillcolor=\"window [65]\" stroked=\"t\" strokecolor=\"windowText [64]\" o:insetmode=\"auto\">\r\n <v:fill color2=\"window [65]\"/>\r\n <v:imagedata o:relid=\"{image_rel}\" o:title=\"\"/>\r\n <x:ClientData ObjectType=\"Pict\">\r\n  <x:SizeWithCells/>\r\n  <x:Anchor>\r\n   {}</x:Anchor>\r\n  <x:CF>Pict</x:CF>\r\n  <x:AutoPict/>\r\n </x:ClientData>\r\n</v:shape>",
        style(frame, n % 1024),
        vml_anchor(from, to),
    );
    let at = text.rfind("</xml>").unwrap_or(text.len());
    text.insert_str(at, &shape);
    package.set_bytes(&part, text.into_bytes());
    if existing.is_none() {
        if package.content_type(&part)?.is_none() {
            package.set_override(&part, CT_VML)?;
        }
        let rel = package.add_relationship(sheet_part, REL_VML, &super::super::pptx::relative(sheet_part, &part), false)?;
        let doc = package.xml_mut(sheet_part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
        if let Some(old) = doc.child(root, "legacyDrawing") {
            doc.detach(old);
        }
        let r = r_prefix(doc, root);
        let p = prefix_of(doc, root);
        let e = detached(doc, &format!("{p}legacyDrawing"));
        doc.set_attr(e, &format!("{r}:id"), &rel);
        place(doc, root, e, &["legacyDrawingHF", "drawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"]);
    }
    Ok(n)
}
