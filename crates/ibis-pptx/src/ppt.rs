// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An old PowerPoint presentation (`.ppt`, PowerPoint 97–2003), read as the `.pptx` made from it, as an old workbook
//! (`.xls`) is (see `xls.rs`). The format is Microsoft's [MS-PPT] (with the drawings of [MS-ODRAW] and the
//! compound file of [MS-CFB]).

use std::collections::HashMap;

use crate::Error;
use crate::cfb;

/// The record types used (MS-PPT 2.13.24).
pub(crate) mod rt {
    pub const DOCUMENT: u16 = 0x03E8;
    pub const DOCUMENT_ATOM: u16 = 0x03E9;
    pub const SLIDE: u16 = 0x03EE;
    pub const SLIDE_ATOM: u16 = 0x03EF;
    pub const NOTES: u16 = 0x03F0;
    pub const MAIN_MASTER: u16 = 0x03F8;
    pub const SLIDE_PERSIST_ATOM: u16 = 0x03F3;
    pub const COLOR_SCHEME_ATOM: u16 = 0x07F0;
    pub const PP_DRAWING: u16 = 0x040C;
    pub const PP_DRAWING_GROUP: u16 = 0x040B;
    pub const SLIDE_LIST_WITH_TEXT: u16 = 0x0FF0;
    pub const USER_EDIT_ATOM: u16 = 0x0FF5;
    pub const PERSIST_DIRECTORY_ATOM: u16 = 0x1772;
    pub const TEXT_HEADER_ATOM: u16 = 0x0F9F;
    pub const TEXT_CHARS_ATOM: u16 = 0x0FA0;
    pub const STYLE_TEXT_PROP_ATOM: u16 = 0x0FA1;
    pub const TEXT_MASTER_STYLE_ATOM: u16 = 0x0FA3;
    pub const TEXT_BYTES_ATOM: u16 = 0x0FA8;
    pub const OUTLINE_TEXT_REF_ATOM: u16 = 0x0F9E;
    pub const ENVIRONMENT: u16 = 0x03F2;
    pub const FONT_COLLECTION: u16 = 0x07D5;
    pub const FONT_ENTITY_ATOM: u16 = 0x0FB7;
    pub const PLACEHOLDER_ATOM: u16 = 0x0BC3;
    // Office Art (MS-ODRAW 2.2)
    pub const DGG_CONTAINER: u16 = 0xF000;
    pub const BSTORE_CONTAINER: u16 = 0xF001;
    pub const DG_CONTAINER: u16 = 0xF002;
    pub const SPGR_CONTAINER: u16 = 0xF003;
    pub const SP_CONTAINER: u16 = 0xF004;
    pub const FBSE: u16 = 0xF007;
    pub const FSPGR: u16 = 0xF009;
    pub const FSP: u16 = 0xF00A;
    pub const FOPT: u16 = 0xF00B;
    pub const CLIENT_TEXTBOX: u16 = 0xF00D;
    pub const CHILD_ANCHOR: u16 = 0xF00F;
    pub const CLIENT_ANCHOR: u16 = 0xF010;
    pub const CLIENT_DATA: u16 = 0xF011;
    pub const SECONDARY_FOPT: u16 = 0xF121;
    pub const TERTIARY_FOPT: u16 = 0xF122;
}

/// One record: its version (0xF for a container), instance, type and data.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rec<'a> {
    pub(crate) ver: u8,
    pub(crate) inst: u16,
    pub(crate) ty: u16,
    pub(crate) data: &'a [u8],
}

impl<'a> Rec<'a> {
    pub(crate) fn is_container(&self) -> bool {
        self.ver == 0xF
    }

    /// The records inside a container.
    pub(crate) fn children(&self) -> Vec<Rec<'a>> {
        records(self.data)
    }

    pub(crate) fn child(&self, ty: u16) -> Option<Rec<'a>> {
        self.children().into_iter().find(|r| r.ty == ty)
    }
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

/// The record at `at` in `d` and where the next one begins.
fn record_at(d: &[u8], at: usize) -> Option<(Rec<'_>, usize)> {
    let head = d.get(at..at + 8)?;
    let first = u16::from_le_bytes([head[0], head[1]]);
    let ty = u16::from_le_bytes([head[2], head[3]]);
    let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
    let end = (at + 8).checked_add(len)?;
    let data = d.get(at + 8..end)?;
    Some((Rec { ver: (first & 0xF) as u8, inst: first >> 4, ty, data }, end))
}

/// The records one after another in `d` (it stops at one that does not fit).
pub(crate) fn records(d: &[u8]) -> Vec<Rec<'_>> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some((rec, next)) = record_at(d, at) {
        out.push(rec);
        at = next;
    }
    out
}

fn bad(what: &str) -> Error {
    Error::new(format!("旧形式（.ppt）のプレゼンテーションを読めませんでした（{what}）"))
}

/// Whether `data` is an old presentation: a compound file with a PowerPoint Document stream.
pub fn is_ppt(data: &[u8]) -> bool {
    cfb::is_cfb(data) && cfb::stream(data, "PowerPoint Document").is_ok()
}

/// The persist object directory of the file (MS-PPT 2.1.2): where each persist object is in the PowerPoint Document
/// stream, as the latest user edit that saved it says.
pub(crate) fn persist_directory(doc: &[u8], current: &[u8]) -> Result<(HashMap<u32, usize>, u32), Error> {
    let (atom, _) = record_at(current, 0).ok_or_else(|| bad("Current User がありません"))?;
    if u32_at(atom.data, 4) == Some(ENCRYPTED_TOKEN) {
        return Err(Error::new("パスワードで保護された旧形式（.ppt）のプレゼンテーションです（パスワードが必要です）"));
    }
    persist_directory_any(doc, current)
}

/// The header token of the Current User stream of a presentation with a password.
const ENCRYPTED_TOKEN: u32 = 0xF3D1_C4DF;
/// The header token of one without.
const PLAIN_TOKEN: u32 = 0xE391_C05F;

fn persist_directory_any(doc: &[u8], current: &[u8]) -> Result<(HashMap<u32, usize>, u32), Error> {
    // The Current User stream: the CurrentUserAtom, whose offsetToCurrentEdit is 12 bytes into its data.
    let (atom, _) = record_at(current, 0).ok_or_else(|| bad("Current User がありません"))?;
    let mut edit_at = u32_at(atom.data, 8).ok_or_else(|| bad("Current User が短すぎます"))? as usize;
    let mut map: HashMap<u32, usize> = HashMap::new();
    let mut document = None;
    let mut guard = 0;
    loop {
        let (edit, _) = record_at(doc, edit_at).filter(|(r, _)| r.ty == rt::USER_EDIT_ATOM).ok_or_else(|| bad("UserEditAtom が見つかりません"))?;
        let d = edit.data;
        let last_edit = u32_at(d, 8).ok_or_else(|| bad("UserEditAtom が短すぎます"))? as usize;
        let dir_at = u32_at(d, 12).ok_or_else(|| bad("UserEditAtom が短すぎます"))? as usize;
        if document.is_none() {
            document = u32_at(d, 16);
        }
        let (dir, _) = record_at(doc, dir_at).filter(|(r, _)| r.ty == rt::PERSIST_DIRECTORY_ATOM).ok_or_else(|| bad("PersistDirectoryAtom が見つかりません"))?;
        let mut at = 0;
        while let Some(head) = u32_at(dir.data, at) {
            let (first, count) = (head & 0xF_FFFF, (head >> 20) as usize);
            at += 4;
            for i in 0..count {
                if let Some(offset) = u32_at(dir.data, at + i * 4) {
                    // The newer edit's entry stands.
                    map.entry(first + i as u32).or_insert(offset as usize);
                }
            }
            at += count * 4;
        }
        if last_edit == 0 || guard > 10_000 {
            break;
        }
        edit_at = last_edit;
        guard += 1;
    }
    Ok((map, document.unwrap_or(1)))
}

/// Whether `data` is an old presentation protected with a password (its Current User stream has the encrypted header token).
pub fn is_encrypted(data: &[u8]) -> bool {
    if !is_ppt(data) {
        return false;
    }
    cfb::stream(data, "Current User").ok().and_then(|c| record_at(&c, 0).and_then(|(atom, _)| u32_at(atom.data, 4))) == Some(ENCRYPTED_TOKEN)
}

/// The fields of an `OfficeArtBlip` record and their sizes, in the order they are stored.
fn blip_fields(ty: u16, inst: u16, body_len: usize) -> Vec<usize> {
    let mut sizes = vec![16];
    if inst & 1 == 1 {
        sizes.push(16);
    }
    // The metafile blips (EMF 0xF01A, WMF 0xF01B, PICT 0xF01C) have a 34-byte header of their own; the others, a tag byte.
    sizes.push(if (0xF01A..=0xF01C).contains(&ty) { 34 } else { 1 });
    let used: usize = sizes.iter().sum();
    sizes.push(body_len.saturating_sub(used));
    sizes
}

/// What the key does to the pictures stream: each field of each blip with the first block's cipher, started anew
/// (the same both ways).
fn apply_to_pictures(pictures: &mut [u8], keys: &ibis_ooxml::legacy_crypt::Rc4Keys, decrypting: bool) {
    let mut at = 0;
    while at + 8 <= pictures.len() {
        let mut header = [0u8; 8];
        header.copy_from_slice(&pictures[at..at + 8]);
        // The sizes come from the header as it is plain: read after decrypting it, or before encrypting it.
        let mut applied = header;
        keys.block_cipher(0).apply(&mut applied);
        if decrypting {
            header = applied;
        }
        pictures[at..at + 8].copy_from_slice(&applied);
        let (inst, ty) = (u16::from_le_bytes([header[0], header[1]]) >> 4, u16::from_le_bytes([header[2], header[3]]));
        let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let end = (at + 8 + len).min(pictures.len());
        let mut field_at = at + 8;
        for size in blip_fields(ty, inst, end - (at + 8)) {
            let to = (field_at + size).min(end);
            keys.block_cipher(0).apply(&mut pictures[field_at..to]);
            field_at = to;
        }
        at = end;
    }
}

/// The streams of an old presentation with a password opened: its `.pptx` bytes.
pub fn decrypt(data: &[u8], password: &str) -> Result<Vec<u8>, ibis_ooxml::encryption::DecryptError> {
    use ibis_ooxml::encryption::DecryptError;
    use ibis_ooxml::legacy_crypt::{Rc4Keys, parse_rc4};
    let damaged = |what: &str| DecryptError::Damaged(what.to_string());
    let mut doc = cfb::stream(data, "PowerPoint Document").map_err(|_| damaged("PowerPoint Document"))?;
    let mut current = cfb::stream(data, "Current User").map_err(|_| damaged("Current User"))?;
    let mut pictures = cfb::stream(data, "Pictures").unwrap_or_default();
    let (map, _) = persist_directory_any(&doc, &current).map_err(|e| damaged(&e.to_string()))?;
    // The current edit (not encrypted) names the persist object that holds the encryption's parameters.
    let (atom, _) = record_at(&current, 0).ok_or_else(|| damaged("Current User"))?;
    let edit_at = u32_at(atom.data, 8).ok_or_else(|| damaged("Current User"))? as usize;
    let (edit, _) = record_at(&doc, edit_at).filter(|(r, _)| r.ty == rt::USER_EDIT_ATOM).ok_or_else(|| damaged("UserEditAtom"))?;
    let session_id = u32_at(edit.data, 28).ok_or_else(|| damaged("encryptSessionPersistIdRef"))?;
    let session_at = *map.get(&session_id).ok_or_else(|| damaged("CryptSession10Container"))?;
    let (session, _) = record_at(&doc, session_at).filter(|(r, _)| r.ty == 0x2F14).ok_or_else(|| damaged("CryptSession10Container"))?;
    let params = parse_rc4(session.data)?;
    let keys = Rc4Keys::new(&params, password)?;
    // Each persist object (its header too) is encrypted with the block of its identifier.
    for (&id, &at) in &map {
        if id == session_id {
            continue;
        }
        let Some(head) = doc.get_mut(at..at + 8) else { continue };
        let mut cipher = keys.block_cipher(id);
        cipher.apply(head);
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let end = (at + 8 + len).min(doc.len());
        cipher.apply(&mut doc[at + 8..end]);
    }
    apply_to_pictures(&mut pictures, &keys, true);
    // The Current User stream says it is plain now.
    if let Some(slot) = current.get_mut(12..16) {
        slot.copy_from_slice(&PLAIN_TOKEN.to_le_bytes());
    }
    let ir = read_ppt_streams(&doc, &current, &pictures).map_err(|e| damaged(&e.to_string()))?;
    crate::ppt_write::to_pptx_of(ir).map_err(|e| damaged(&e.to_string()))
}

/// The record a persist object is: the one at its offset.
pub(crate) fn persist_object<'a>(doc: &'a [u8], map: &HashMap<u32, usize>, id: u32) -> Option<Rec<'a>> {
    record_at(doc, *map.get(&id)?).map(|(r, _)| r)
}

/// A readable outline of the records of a presentation, for the one who looks into a file (`examples/ppt_dump.rs`).
pub fn dump(data: &[u8]) -> Result<String, Error> {
    let doc = cfb::stream(data, "PowerPoint Document").map_err(|_| bad("PowerPoint Document がありません"))?;
    let current = cfb::stream(data, "Current User").map_err(|_| bad("Current User がありません"))?;
    let (map, document) = persist_directory(&doc, &current)?;
    let mut out = String::new();
    let mut ids: Vec<_> = map.iter().collect();
    ids.sort();
    for (id, at) in ids {
        out += &format!("persist {id} at {at}\n");
    }
    fn walk(out: &mut String, recs: &[Rec], depth: usize, timing: bool) {
        for r in recs {
            *out += &format!("{}{:04X} ver={} inst={:03X} len={}", "  ".repeat(depth), r.ty, r.ver, r.inst, r.data.len());
            if timing && !r.is_container() {
                let hex: String = r.data.iter().take(48).map(|b| format!("{b:02X}")).collect();
                *out += &format!(" {hex}");
            }
            *out += "\n";
            if r.is_container() {
                walk(out, &r.children(), depth + 1, timing);
            } else if r.ty == 0x138B {
                // The binary tag data of a slide's animation (PowerPoint 2002 and later).
                walk(out, &records(r.data), depth + 1, true);
            }
        }
    }
    if let Some(d) = persist_object(&doc, &map, document) {
        out += "document:\n";
        walk(&mut out, &[d], 1, false);
    }
    if let Ok(ir) = read_ppt(data) {
        out += &format!("fonts: {:?}\n", ir.fonts);
        for (i, slide) in ir.slides.iter().enumerate() {
            fn texts(out: &mut String, shapes: &[ShapeIr]) {
                for s in shapes {
                    let mut props: Vec<_> = s.props.values.iter().collect();
                    props.sort();
                    *out += &format!("  shape {} spt={} frame={:?} props={:X?}\n", s.id, s.spt, s.frame, props);
                    for (id, data) in &s.props.complex {
                        if data.len() < 200 {
                            *out += &format!("    complex {:X}: {:02X?}\n", id, data);
                        }
                    }
                    if let Some(t) = &s.text {
                        *out += &format!("  shape {} spt={} {:?}\n", s.id, s.spt, t);
                    }
                    texts(out, &s.children);
                }
            }
            out += &format!("slide {}:\n", i + 1);
            texts(&mut out, &slide.shapes);
        }
    }
    let mut persisted: Vec<_> = map.keys().copied().filter(|id| *id != document).collect();
    persisted.sort();
    for id in persisted {
        if let Some(r) = persist_object(&doc, &map, id) {
            out += &format!("object {id}:\n");
            walk(&mut out, &[r], 1, false);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------
// Text formatting (MS-PPT 2.9)
// ---------------------------------------------------------------------------------------------------------------

/// A color of a text: an index in the slide's color scheme or an sRGB value (MS-PPT 2.12.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextColor {
    Scheme(u8),
    Rgb(u32),
}

fn text_color(v: u32) -> Option<TextColor> {
    let [r, g, b, index] = v.to_le_bytes();
    match index {
        0xFE => Some(TextColor::Rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))),
        0..=7 => Some(TextColor::Scheme(index)),
        _ => None,
    }
}

/// Paragraph formatting (a `TextPFException`): each field is there when the file gave it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Pf {
    pub(crate) has_bullet: Option<bool>,
    pub(crate) bullet_char: Option<u16>,
    pub(crate) bullet_font: Option<u16>,
    pub(crate) bullet_size: Option<i16>,
    pub(crate) bullet_color: Option<TextColor>,
    pub(crate) align: Option<u16>,
    pub(crate) line_spacing: Option<i16>,
    pub(crate) space_before: Option<i16>,
    pub(crate) space_after: Option<i16>,
    pub(crate) left_margin: Option<i16>,
    pub(crate) indent: Option<i16>,
    pub(crate) default_tab: Option<i16>,
    pub(crate) tabs: Option<Vec<(i16, u16)>>,
    pub(crate) font_align: Option<u16>,
    pub(crate) text_direction: Option<u16>,
}

impl Pf {
    /// This formatting where it is given, else `base`'s.
    pub(crate) fn over(&self, base: &Pf) -> Pf {
        macro_rules! pick {
            ($($f:ident),*) => { Pf { $($f: self.$f.clone().or_else(|| base.$f.clone()),)* } };
        }
        pick!(has_bullet, bullet_char, bullet_font, bullet_size, bullet_color, align, line_spacing, space_before, space_after, left_margin, indent, default_tab, tabs, font_align, text_direction)
    }
}

/// What a click on a shape or a stretch of text does (`InteractiveInfoAtom`, MS-PPT 2.6.10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Interactive {
    /// `InteractiveInfoActionEnum`: 3 jump, 4 hyperlink.
    pub(crate) action: u8,
    /// `InteractiveInfoJumpEnum`.
    pub(crate) jump: u8,
    pub(crate) link_id: u32,
    /// `LinkToEnum`.
    pub(crate) link_type: u8,
}

fn interactive_of(container: &Rec) -> Option<Interactive> {
    let atom = container.children().into_iter().find(|r| r.ty == 0x0FF3)?;
    let d = atom.data;
    let i = Interactive { action: *d.get(8)?, jump: *d.get(10)?, link_id: u32_at(d, 4)?, link_type: d.get(12).copied().unwrap_or(0) };
    matches!(i.action, 3 | 4).then_some(i)
}

/// Character formatting (a `TextCFException`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Cf {
    pub(crate) bold: Option<bool>,
    pub(crate) italic: Option<bool>,
    pub(crate) underline: Option<bool>,
    pub(crate) shadow: Option<bool>,
    pub(crate) font: Option<u16>,
    pub(crate) east_asian_font: Option<u16>,
    pub(crate) ansi_font: Option<u16>,
    pub(crate) symbol_font: Option<u16>,
    pub(crate) size: Option<u16>,
    pub(crate) color: Option<TextColor>,
    pub(crate) position: Option<i16>,
    /// The run grouping of the additional text properties (`StyleTextProp9Atom`).
    pub(crate) pp9rt: Option<u8>,
    /// A click on the characters does this.
    pub(crate) link: Option<Interactive>,
}

impl Cf {
    pub(crate) fn over(&self, base: &Cf) -> Cf {
        Cf {
            bold: self.bold.or(base.bold),
            italic: self.italic.or(base.italic),
            underline: self.underline.or(base.underline),
            shadow: self.shadow.or(base.shadow),
            font: self.font.or(base.font),
            // A typeface the formatting names is the font of all its characters: the base's other fonts do not stay.
            east_asian_font: if self.font.is_some() { self.east_asian_font } else { self.east_asian_font.or(base.east_asian_font) },
            ansi_font: if self.font.is_some() { self.ansi_font } else { self.ansi_font.or(base.ansi_font) },
            symbol_font: if self.font.is_some() { self.symbol_font } else { self.symbol_font.or(base.symbol_font) },
            size: self.size.or(base.size),
            color: self.color.or(base.color),
            position: self.position.or(base.position),
            pp9rt: self.pp9rt.or(base.pp9rt),
            link: self.link.or(base.link),
        }
    }
}

/// A cursor over bytes.
struct Cur<'a> {
    d: &'a [u8],
    at: usize,
}

impl<'a> Cur<'a> {
    fn u16(&mut self) -> Option<u16> {
        let v = u16_at(self.d, self.at)?;
        self.at += 2;
        Some(v)
    }

    fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }

    fn u32(&mut self) -> Option<u32> {
        let v = u32_at(self.d, self.at)?;
        self.at += 4;
        Some(v)
    }
}

/// A `TextPFException` (MS-PPT 2.9.20).
fn parse_pf(c: &mut Cur) -> Option<Pf> {
    let masks = c.u32()?;
    let mut pf = Pf::default();
    if masks & 0xF != 0 {
        let flags = c.u16()?;
        if masks & 0x1 != 0 {
            pf.has_bullet = Some(flags & 0x1 != 0);
        }
    }
    if masks & 0x80 != 0 {
        pf.bullet_char = Some(c.u16()?);
    }
    if masks & 0x10 != 0 {
        pf.bullet_font = Some(c.u16()?);
    }
    if masks & 0x40 != 0 {
        pf.bullet_size = Some(c.i16()?);
    }
    if masks & 0x20 != 0 {
        pf.bullet_color = text_color(c.u32()?);
    }
    if masks & 0x800 != 0 {
        pf.align = Some(c.u16()?);
    }
    if masks & 0x1000 != 0 {
        pf.line_spacing = Some(c.i16()?);
    }
    if masks & 0x2000 != 0 {
        pf.space_before = Some(c.i16()?);
    }
    if masks & 0x4000 != 0 {
        pf.space_after = Some(c.i16()?);
    }
    if masks & 0x100 != 0 {
        pf.left_margin = Some(c.i16()?);
    }
    if masks & 0x400 != 0 {
        pf.indent = Some(c.i16()?);
    }
    if masks & 0x8000 != 0 {
        pf.default_tab = Some(c.i16()?);
    }
    if masks & 0x10_0000 != 0 {
        let count = c.u16()? as usize;
        let mut tabs = Vec::new();
        for _ in 0..count {
            let position = c.i16()?;
            let kind = c.u16()?;
            tabs.push((position, kind));
        }
        pf.tabs = Some(tabs);
    }
    if masks & 0x1_0000 != 0 {
        pf.font_align = Some(c.u16()?);
    }
    if masks & 0xE_0000 != 0 {
        c.u16()?;
    }
    if masks & 0x20_0000 != 0 {
        pf.text_direction = Some(c.u16()?);
    }
    Some(pf)
}

/// A `TextCFException` (MS-PPT 2.9.14).
fn parse_cf(c: &mut Cur) -> Option<Cf> {
    let masks = c.u32()?;
    let mut cf = Cf::default();
    if masks & 0x3FFF != 0 {
        let style = c.u16()?;
        cf.pp9rt = Some(((style >> 10) & 0xF) as u8);
        for (bit, slot) in [(0x1, &mut cf.bold), (0x2, &mut cf.italic), (0x4, &mut cf.underline), (0x10, &mut cf.shadow)] {
            if masks & bit != 0 {
                *slot = Some(u32::from(style) & bit != 0);
            }
        }
    }
    if masks & 0x1_0000 != 0 {
        cf.font = Some(c.u16()?);
    }
    if masks & 0x20_0000 != 0 {
        cf.east_asian_font = Some(c.u16()?);
    }
    if masks & 0x40_0000 != 0 {
        cf.ansi_font = Some(c.u16()?);
    }
    if masks & 0x80_0000 != 0 {
        cf.symbol_font = Some(c.u16()?);
    }
    if masks & 0x2_0000 != 0 {
        cf.size = Some(c.u16()?);
    }
    if masks & 0x4_0000 != 0 {
        cf.color = text_color(c.u32()?);
    }
    if masks & 0x8_0000 != 0 {
        cf.position = Some(c.i16()?);
    }
    Some(cf)
}

/// The formatting of the levels of one kind of text of a master (a `TextMasterStyleAtom`, MS-PPT 2.9.35).
#[derive(Clone, Debug, Default)]
pub(crate) struct MasterStyle {
    pub(crate) levels: Vec<(Pf, Cf)>,
}

fn parse_master_style(rec: &Rec) -> Option<MasterStyle> {
    let mut c = Cur { d: rec.data, at: 0 };
    let count = c.u16()? as usize;
    let mut levels = Vec::new();
    for _ in 0..count.min(5) {
        if rec.inst >= 5 {
            c.u16()?;
        }
        let pf = parse_pf(&mut c)?;
        let cf = parse_cf(&mut c)?;
        levels.push((pf, cf));
    }
    Some(MasterStyle { levels })
}

/// One paragraph of a text, as the file has it.
#[derive(Clone, Debug, Default)]
pub(crate) struct ParaIr {
    pub(crate) level: u16,
    pub(crate) pf: Pf,
    /// An automatic numbering: the scheme (`TextAutoNumberSchemeEnum`) and the first number.
    pub(crate) auto_number: Option<(u16, u16)>,
    /// Runs: text and character formatting. A `\u{B}` is a line break.
    pub(crate) runs: Vec<(String, Cf, Option<Field>)>,
}

/// A field in a text (MS-PPT 2.9.47–2.9.52).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    SlideNumber,
    DateTime,
}

/// The text of a shape: its kind (`TextTypeEnum`) and paragraphs.
#[derive(Clone, Debug, Default)]
pub(crate) struct TextIr {
    pub(crate) kind: u32,
    pub(crate) paragraphs: Vec<ParaIr>,
}

/// The text records of one text (after a `TextHeaderAtom`): its characters, formatting and fields.
#[derive(Default)]
struct TextAtoms<'a> {
    kind: u32,
    chars: Option<String>,
    style: Option<Rec<'a>>,
    fields: Vec<(usize, Field)>,
    /// The additional text properties of the shape (`StyleTextProp9Atom`'s data).
    style9: Option<&'a [u8]>,
    /// Stretches of the text a click does something on: start, end, what.
    links: Vec<(usize, usize, Interactive)>,
}

fn decode_text(rec: &Rec) -> String {
    if rec.ty == rt::TEXT_CHARS_ATOM {
        let units: Vec<u16> = rec.data.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        rec.data.iter().map(|&b| char::from(b)).collect()
    }
}

/// The paragraphs of a text from its atoms and the master's formatting is not applied here (see `resolve`).
fn build_text(atoms: &TextAtoms) -> Option<TextIr> {
    let text = atoms.chars.as_ref()?;
    let chars: Vec<char> = text.chars().collect();
    // Paragraph and character formatting runs (their counts include the paragraph mark of the last paragraph).
    let mut pf_runs: Vec<(usize, u16, Pf)> = Vec::new();
    let mut cf_runs: Vec<(usize, Cf)> = Vec::new();
    if let Some(style) = &atoms.style {
        let mut c = Cur { d: style.data, at: 0 };
        let mut covered = 0usize;
        while covered < chars.len() + 1 {
            let (Some(count), Some(level)) = (c.u32(), c.u16()) else { break };
            let Some(pf) = parse_pf(&mut c) else { break };
            pf_runs.push((count as usize, level, pf));
            covered += count as usize;
        }
        let mut covered = 0usize;
        while covered < chars.len() + 1 {
            let Some(count) = c.u32() else { break };
            let Some(cf) = parse_cf(&mut c) else { break };
            cf_runs.push((count as usize, cf));
            covered += count as usize;
        }
    }
    // The formatting at each character.
    let at_pf = |index: usize| -> (u16, Pf) {
        let mut start = 0;
        for (count, level, pf) in &pf_runs {
            if index < start + count {
                return (*level, pf.clone());
            }
            start += count;
        }
        (0, Pf::default())
    };
    let at_cf = |index: usize| -> Cf {
        let mut start = 0;
        let mut found = Cf::default();
        for (count, cf) in &cf_runs {
            if index < start + count {
                found = cf.clone();
                break;
            }
            start += count;
        }
        if let Some((_, _, link)) = atoms.links.iter().find(|(a, b, _)| index >= *a && index < *b) {
            found.link = Some(*link);
        }
        found
    };
    // The automatic numbering at each character: the items of the additional properties go to the runs that share a grouping.
    let numbering = atoms.style9.map(parse_style9).unwrap_or_default();
    let mut auto_ranges: Vec<(usize, usize, Option<(u16, u16)>)> = Vec::new();
    if !numbering.is_empty() {
        let mut groups: Vec<(usize, usize, u8)> = Vec::new();
        let mut start = 0;
        for (count, cf) in &cf_runs {
            let rt = cf.pp9rt.unwrap_or(0);
            match groups.last_mut() {
                Some(g) if g.2 == rt => g.1 = start + count,
                _ => groups.push((start, start + count, rt)),
            }
            start += count;
        }
        let mut item = 0usize;
        for (from, to, rt) in groups {
            while item < numbering.len() && item % 16 != rt as usize {
                item += 1;
            }
            let Some(n) = numbering.get(item) else { break };
            auto_ranges.push((from, to, *n));
            item += 1;
        }
    }
    let auto_at = |index: usize| auto_ranges.iter().find(|(a, b, _)| index >= *a && index < *b).and_then(|(_, _, n)| *n);
    let field_at = |index: usize| atoms.fields.iter().find(|(p, _)| *p == index).map(|(_, f)| *f);
    let mut paragraphs: Vec<ParaIr> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i <= chars.len() {
        if i == chars.len() || chars[i] == '\r' {
            let (level, pf) = at_pf(start);
            let mut para = ParaIr { level, pf, auto_number: auto_at(start), runs: Vec::new() };
            // Runs: stretches with the same formatting (and a field is a run of its own).
            let mut run_start = start;
            let mut j = start;
            while j < i {
                let field = field_at(j);
                let boundary = j + 1 == i || field.is_some() || field_at(j + 1).is_some() || at_cf(j + 1) != at_cf(j);
                if boundary {
                    let text: String = chars[run_start..=j].iter().collect();
                    para.runs.push((text, at_cf(run_start), field));
                    run_start = j + 1;
                }
                j += 1;
            }
            paragraphs.push(para);
            start = i + 1;
        }
        i += 1;
    }
    Some(TextIr { kind: atoms.kind, paragraphs })
}

/// The automatic numbering of each item of a `StyleTextProp9Atom` (MS-PPT 2.9.67): the scheme and first number, if it has one.
fn parse_style9(d: &[u8]) -> Vec<Option<(u16, u16)>> {
    let mut c = Cur { d, at: 0 };
    let mut out = Vec::new();
    while c.at < d.len() {
        let Some(pf_masks) = c.u32() else { break };
        if pf_masks & 0x80_0000 != 0 && c.u16().is_none() {
            break;
        }
        let has_number = if pf_masks & 0x200_0000 != 0 { c.u16().map(|v| v != 0) } else { Some(false) };
        let scheme = if pf_masks & 0x100_0000 != 0 {
            let (Some(a), Some(b)) = (c.u16(), c.u16()) else { break };
            Some((a, b))
        } else {
            None
        };
        let Some(cf_masks) = c.u32() else { break };
        if cf_masks & 0x10_0000 != 0 && c.u32().is_none() {
            break;
        }
        let Some(si_masks) = c.u32() else { break };
        for bit in [0x1u32, 0x2, 0x4, 0x40] {
            if si_masks & bit != 0 && c.u16().is_none() {
                return out;
            }
        }
        if si_masks & 0x20 != 0 && c.u32().is_none() {
            break;
        }
        if si_masks & 0x200 != 0 {
            let Some(count) = c.u32() else { break };
            c.at += count as usize * 4;
        }
        out.push(if has_number == Some(true) { scheme } else { None });
    }
    out
}

/// The text a `ClientTextbox` holds, or the index of the text it points to in the slide list (`Err`).
fn read_textbox<'a>(textbox: &Rec<'a>, style9: Option<&'a [u8]>) -> Result<Option<TextIr>, usize> {
    let mut atoms = TextAtoms { style9, ..Default::default() };
    let mut any = false;
    let mut last_interactive: Option<Interactive> = None;
    for r in textbox.children() {
        match r.ty {
            0x0FF2 if r.inst == 0 => last_interactive = interactive_of(&r),
            0x0FDF => {
                if let (Some(i), Some(a), Some(b)) = (last_interactive.take(), u32_at(r.data, 0), u32_at(r.data, 4)) {
                    atoms.links.push((a as usize, b as usize, i));
                }
            }
            rt::OUTLINE_TEXT_REF_ATOM => return Err(u32_at(r.data, 0).unwrap_or(0) as usize),
            rt::TEXT_HEADER_ATOM => {
                atoms.kind = u32_at(r.data, 0).unwrap_or(4);
                any = true;
            }
            rt::TEXT_CHARS_ATOM | rt::TEXT_BYTES_ATOM => atoms.chars = Some(decode_text(&r)),
            rt::STYLE_TEXT_PROP_ATOM => atoms.style = Some(r),
            0x0FD8 => {
                if let Some(p) = u32_at(r.data, 0) {
                    atoms.fields.push((p as usize, Field::SlideNumber));
                }
            }
            0x0FF7 | 0x0FF8 => {
                if let Some(p) = u32_at(r.data, 0) {
                    atoms.fields.push((p as usize, Field::DateTime));
                }
            }
            _ => {}
        }
    }
    Ok(if any { build_text(&atoms) } else { None })
}

// ---------------------------------------------------------------------------------------------------------------
// Drawings (MS-ODRAW 2.2, MS-PPT 2.5.13)
// ---------------------------------------------------------------------------------------------------------------

/// Master units (1/576 inch) in EMU.
fn emu(units: i64) -> i64 {
    units * 1587 + units / 2
}

/// The properties whose complex data is an array (`IMsoArray`): vertices, segments, guides, inscribe, handles, connection sites,
/// shade colors and the wrap polygon.
const ARRAY_PROPERTIES: [u16; 9] = [0x145, 0x146, 0x151, 0x152, 0x155, 0x156, 0x157, 0x197, 0x383];

/// The properties of a shape (`OfficeArtFOPT`): each as its value and, when complex, its data.
#[derive(Clone, Debug, Default)]
pub(crate) struct Props {
    values: HashMap<u16, u32>,
    complex: HashMap<u16, Vec<u8>>,
}

impl Props {
    fn parse(rec: &Rec, props: &mut Props) {
        let count = rec.inst as usize;
        let mut entries: Vec<(u16, bool, u32)> = Vec::new();
        for i in 0..count {
            let Some(id) = u16_at(rec.data, i * 6) else { break };
            let Some(op) = u32_at(rec.data, i * 6 + 2) else { break };
            entries.push((id & 0x3FFF, id & 0x8000 != 0, op));
        }
        // The data of the complex ones follows the table, in order.
        let mut at = count * 6;
        for (id, complex, op) in entries {
            if complex {
                let mut size = op as usize;
                // An array (`IMsoArray`) leads with 6 bytes of header that Office's writers leave out of the size.
                if ARRAY_PROPERTIES.contains(&id)
                    && let (Some(n), Some(cb)) = (u16_at(rec.data, at), u16_at(rec.data, at + 4))
                {
                    let element = if cb == 0xFFF0 { 4 } else { cb as usize };
                    if size == n as usize * element {
                        size += 6;
                    }
                }
                let end = at + size;
                if let Some(data) = rec.data.get(at..end) {
                    props.complex.insert(id, data.to_vec());
                }
                at = end;
                props.values.insert(id, op);
            } else {
                props.values.insert(id, op);
            }
        }
    }

    /// The data of a complex property.
    pub(crate) fn complex(&self, id: u16) -> Option<&[u8]> {
        self.complex.get(&id).map(Vec::as_slice)
    }

    pub(crate) fn get(&self, id: u16) -> Option<u32> {
        self.values.get(&id).copied()
    }

    pub(crate) fn int(&self, id: u16) -> Option<i32> {
        self.get(id).map(|v| v as i32)
    }

    /// A boolean property group (the low half the values, the high half which of them are given): the flag at `bit`.
    pub(crate) fn flag(&self, id: u16, bit: u32) -> Option<bool> {
        let v = self.get(id)?;
        (v & (1 << (bit + 16)) != 0).then_some(v & (1 << bit) != 0)
    }
}

/// An `OfficeArtCOLORREF`: an index in the color scheme, or sRGB.
fn art_color(v: u32) -> TextColor {
    let [r, g, b, flags] = v.to_le_bytes();
    if flags & 0x08 != 0 {
        TextColor::Scheme(r)
    } else {
        TextColor::Rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
    }
}

/// A shape read from a drawing.
#[derive(Clone, Debug, Default)]
pub(crate) struct ShapeIr {
    pub(crate) id: u32,
    pub(crate) name: String,
    /// The shape type (`MSOSPT`).
    pub(crate) spt: u16,
    pub(crate) flip_h: bool,
    pub(crate) flip_v: bool,
    pub(crate) is_group: bool,
    pub(crate) is_connector: bool,
    pub(crate) background: bool,
    /// Where it is: `x, y, width, height` in EMU, in the space of the group it is in (the slide's for a top shape).
    pub(crate) frame: Option<(i64, i64, i64, i64)>,
    pub(crate) props: Props,
    pub(crate) placeholder: Option<u8>,
    /// A click on the shape does this.
    pub(crate) interactive: Option<Interactive>,
    /// The animation of PowerPoint 97 to 2000 (`AnimationInfoAtom`).
    pub(crate) anim: Option<crate::ppt_timing::Anim>,
    pub(crate) text: Option<TextIr>,
    /// The text of a placeholder is in the slide list: its index.
    pub(crate) outline_ref: Option<usize>,
    /// A group's children and the space they are in (`x, y, width, height`, EMU).
    pub(crate) children: Vec<ShapeIr>,
    pub(crate) child_space: Option<(i64, i64, i64, i64)>,
}

/// The shape `rec` (an `OfficeArtSpContainer`) is, with the anchor it was given.
fn read_shape(rec: &Rec, in_group: bool, group_space: Option<(i64, i64, i64, i64)>) -> ShapeIr {
    let mut shape = ShapeIr::default();
    let mut space = None;
    let mut anchor_child = None;
    let mut style9: Option<&[u8]> = None;
    let mut textbox: Option<Rec> = None;
    for r in rec.children() {
        match r.ty {
            rt::FSP => {
                shape.spt = r.inst;
                shape.id = u32_at(r.data, 0).unwrap_or(0);
                let flags = u32_at(r.data, 4).unwrap_or(0);
                shape.is_group = flags & 0x1 != 0;
                shape.flip_h = flags & 0x40 != 0;
                shape.flip_v = flags & 0x80 != 0;
                shape.is_connector = flags & 0x100 != 0;
                shape.background = flags & 0x400 != 0;
            }
            rt::FSPGR => {
                let v: Vec<i64> = (0..4).map(|i| i64::from(u32_at(r.data, i * 4).unwrap_or(0) as i32)).collect();
                if v.len() == 4 {
                    space = Some((v[0], v[1], v[2], v[3]));
                }
            }
            rt::FOPT | rt::SECONDARY_FOPT | rt::TERTIARY_FOPT => Props::parse(&r, &mut shape.props),
            rt::CLIENT_ANCHOR => {
                // A rectangle of top, left, right, bottom: 16-bit, or 32-bit.
                let get = |i: usize| -> Option<i64> {
                    if r.data.len() >= 16 { u32_at(r.data, i * 4).map(|v| i64::from(v as i32)) } else { u16_at(r.data, i * 2).map(|v| i64::from(v as i16)) }
                };
                if let (Some(top), Some(left), Some(right), Some(bottom)) = (get(0), get(1), get(2), get(3)) {
                    shape.frame = Some((emu(left), emu(top), emu(right - left), emu(bottom - top)));
                }
            }
            rt::CHILD_ANCHOR => {
                let v: Vec<i64> = (0..4).map(|i| i64::from(u32_at(r.data, i * 4).unwrap_or(0) as i32)).collect();
                if v.len() == 4 {
                    anchor_child = Some((v[0], v[1], v[2] - v[0], v[3] - v[1]));
                }
            }
            rt::CLIENT_DATA => {
                for d in r.children() {
                    if d.ty == rt::PLACEHOLDER_ATOM && u32_at(d.data, 0) != Some(0xFFFF_FFFF) {
                        shape.placeholder = d.data.get(4).copied();
                    }
                    if d.ty == 0x0FF2 && d.inst == 0 {
                        shape.interactive = interactive_of(&d);
                    }
                    if d.ty == 0x1014 {
                        shape.anim = d.child(0x1015).and_then(|a| crate::ppt_timing::Anim::read(a.data));
                    }
                    if d.ty == 0x1388 {
                        // Programmable tags: the PowerPoint 2000 ones hold the additional text properties.
                        for tag in d.children() {
                            for data in tag.children().iter().filter(|x| x.ty == 0x138B) {
                                if let Some(atom) = records(data.data).into_iter().find(|x| x.ty == 0x0FAC) {
                                    style9 = Some(atom.data);
                                }
                            }
                        }
                    }
                }
            }
            rt::CLIENT_TEXTBOX => textbox = Some(r),
            _ => {}
        }
    }
    if let Some(r) = textbox {
        match read_textbox(&r, style9) {
            Ok(text) => shape.text = text,
            Err(index) => shape.outline_ref = Some(index),
        }
    }
    if let Some((x, y, w, h)) = anchor_child {
        // In the coordinates of the group's space; kept as they are (in master units times the EMU per unit).
        let _ = group_space;
        shape.frame = Some((emu(x), emu(y), emu(w), emu(h)));
    }
    shape.child_space = space.map(|(l, t, r, b)| (emu(l), emu(t), emu(r - l), emu(b - t)));
    let _ = in_group;
    shape.name = shape.props.complex.get(&0x0380).map(|d| utf16_string(d)).unwrap_or_default();
    shape
}

fn utf16_string(d: &[u8]) -> String {
    let units: Vec<u16> = d.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).take_while(|u| *u != 0).collect();
    String::from_utf16_lossy(&units)
}

/// The shapes of a group container (`OfficeArtSpgrContainer`), its first shape being the group itself. For the
/// patriarch (the whole drawing) the group's own shape is dropped.
fn read_group(rec: &Rec, patriarch: bool) -> Vec<ShapeIr> {
    let children = rec.children();
    let mut out = Vec::new();
    let mut it = children.iter();
    let mut group_shape: Option<ShapeIr> = None;
    if let Some(first) = children.first() {
        if first.ty == rt::SP_CONTAINER {
            group_shape = Some(read_shape(first, false, None));
            it.next();
        }
    }
    let mut members = Vec::new();
    for r in it {
        match r.ty {
            rt::SP_CONTAINER => members.push(read_shape(r, true, None)),
            rt::SPGR_CONTAINER => {
                let mut inner = read_group(r, false);
                members.append(&mut inner);
            }
            _ => {}
        }
    }
    if patriarch {
        out = members;
    } else if let Some(mut g) = group_shape {
        g.is_group = true;
        g.children = members;
        out.push(g);
    }
    out
}

/// The shapes of a drawing (`DrawingContainer` / PPDrawing) and its background shape.
pub(crate) fn read_drawing(drawing: &Rec) -> (Vec<ShapeIr>, Option<ShapeIr>) {
    let Some(dg) = drawing.child(rt::DG_CONTAINER) else { return (Vec::new(), None) };
    let mut shapes = Vec::new();
    let mut background = None;
    for r in dg.children() {
        match r.ty {
            rt::SPGR_CONTAINER => shapes = read_group(&r, true),
            rt::SP_CONTAINER => {
                let s = read_shape(&r, false, None);
                if s.background {
                    background = Some(s);
                }
            }
            _ => {}
        }
    }
    (shapes, background)
}

// ---------------------------------------------------------------------------------------------------------------
// The presentation
// ---------------------------------------------------------------------------------------------------------------

/// A picture of the file: its extension and bytes.
#[derive(Clone, Debug)]
pub(crate) struct Blip {
    pub(crate) ext: &'static str,
    #[allow(dead_code)]
    pub(crate) mime: &'static str,
    pub(crate) data: Vec<u8>,
}

/// A slide or a main master.
#[derive(Clone, Debug, Default)]
pub(crate) struct PageIr {
    /// The slide's identifier (the one a slide's `master` names for a master).
    pub(crate) id: u32,
    pub(crate) geom: u32,
    pub(crate) master_ref: u32,
    pub(crate) follow_master_objects: bool,
    pub(crate) follow_master_scheme: bool,
    pub(crate) follow_master_background: bool,
    /// The color scheme this page has (background, text, shadow, title, fill, accent 1–3), if it has one.
    pub(crate) scheme: Option<[u32; 8]>,
    pub(crate) styles: HashMap<u16, MasterStyle>,
    pub(crate) shapes: Vec<ShapeIr>,
    pub(crate) background: Option<ShapeIr>,
    /// The texts the slide list (`SlideListWithText`) holds for this slide, which its placeholders point to.
    pub(crate) outline: Vec<TextIr>,
    /// The notes page this slide has (`SlideAtom.notesIdRef`).
    pub(crate) notes_ref: u32,
    /// The slide show information (`SlideShowSlideInfoAtom`): transition, timing and whether the slide is hidden.
    pub(crate) show: Option<SlideShow>,
    /// The animation (the data of the `___PPT10` binary tag: the time node tree).
    pub(crate) timing: Option<Vec<u8>>,
}

/// What a slide does in a show.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SlideShow {
    /// The time before the next slide, in milliseconds, when it advances by itself.
    pub(crate) auto_advance: Option<u32>,
    pub(crate) manual_advance: bool,
    pub(crate) hidden: bool,
    pub(crate) effect: u8,
    pub(crate) direction: u8,
    /// 0: 0.75 s, 1: 0.5 s, 2: 0.25 s.
    pub(crate) speed: u8,
}

/// An old presentation, read.
#[derive(Debug, Default)]
pub(crate) struct PptIr {
    /// The slide size, in EMU.
    pub(crate) size: (i64, i64),
    pub(crate) fonts: Vec<String>,
    pub(crate) defaults: HashMap<u16, MasterStyle>,
    pub(crate) masters: Vec<PageIr>,
    pub(crate) slides: Vec<PageIr>,
    /// The targets of the hyperlinks, by their identifiers.
    pub(crate) hyperlinks: HashMap<u32, String>,
    /// The notes pages, by their identifiers.
    pub(crate) notes: HashMap<u32, PageIr>,
    /// The pictures, in the order of the picture store (a picture property counts from 1).
    pub(crate) blips: Vec<Option<Blip>>,
    /// The shape properties the drawing group gives every shape that does not say otherwise.
    pub(crate) default_props: Props,
}

fn scheme_of(rec: &Rec) -> Option<[u32; 8]> {
    let mut colors = [0u32; 8];
    for (i, c) in colors.iter_mut().enumerate() {
        let b = rec.data.get(i * 4..i * 4 + 3)?;
        *c = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
    }
    Some(colors)
}

/// A slide or master container read as a page.
fn read_page(rec: &Rec) -> PageIr {
    let mut page = PageIr { follow_master_objects: true, follow_master_scheme: true, follow_master_background: true, ..Default::default() };
    for r in rec.children() {
        match r.ty {
            rt::SLIDE_ATOM => {
                page.geom = u32_at(r.data, 0).unwrap_or(1);
                page.master_ref = u32_at(r.data, 12).unwrap_or(0);
                page.notes_ref = u32_at(r.data, 16).unwrap_or(0);
                let flags = u16_at(r.data, 20).unwrap_or(7);
                page.follow_master_objects = flags & 0x1 != 0;
                page.follow_master_scheme = flags & 0x2 != 0;
                page.follow_master_background = flags & 0x4 != 0;
            }
            rt::COLOR_SCHEME_ATOM if r.inst == 1 => page.scheme = scheme_of(&r),
            // The programmable tags: the one named ___PPT10 holds the animation.
            0x1388 => {
                for tag in r.children().iter().filter(|t| t.ty == 0x138A) {
                    let kids = tag.children();
                    let name: Vec<u16> = kids.iter().find(|k| k.ty == 0x0FBA).map(|k| k.data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect()).unwrap_or_default();
                    if String::from_utf16_lossy(&name) == "___PPT10" {
                        if let Some(data) = kids.iter().find(|k| k.ty == 0x138B) {
                            page.timing = Some(data.data.to_vec());
                        }
                    }
                }
            }
            0x03F9 => {
                let flags = u16_at(r.data, 10).unwrap_or(0);
                page.show = Some(SlideShow {
                    auto_advance: (flags & 0x400 != 0).then(|| u32_at(r.data, 0).unwrap_or(0)),
                    manual_advance: flags & 0x1 != 0,
                    hidden: flags & 0x4 != 0,
                    direction: r.data.get(8).copied().unwrap_or(0),
                    effect: r.data.get(9).copied().unwrap_or(0),
                    speed: r.data.get(12).copied().unwrap_or(1),
                });
            }
            rt::TEXT_MASTER_STYLE_ATOM => {
                if let Some(style) = parse_master_style(&r) {
                    page.styles.insert(r.inst, style);
                }
            }
            rt::PP_DRAWING => {
                let (shapes, background) = read_drawing(&r);
                page.shapes = shapes;
                page.background = background;
            }
            _ => {}
        }
    }
    page
}

/// The pictures of the drawing group's store, in order.
fn read_blips(drawing_group: &Rec, pictures: &[u8]) -> Vec<Option<Blip>> {
    let Some(dgg) = drawing_group.child(rt::DGG_CONTAINER) else { return Vec::new() };
    let Some(store) = dgg.child(rt::BSTORE_CONTAINER) else { return Vec::new() };
    store
        .children()
        .iter()
        .map(|fbse| {
            if fbse.ty != rt::FBSE {
                return None;
            }
            let name_len = fbse.data.get(33).copied().unwrap_or(0) as usize;
            let embedded_at = 36 + name_len;
            if fbse.data.len() > embedded_at {
                return blip_of(&fbse.data[embedded_at..], 0);
            }
            let offset = u32_at(fbse.data, 28)?;
            if offset == 0xFFFF_FFFF {
                return None;
            }
            blip_of(pictures, offset as usize)
        })
        .collect()
}

/// The picture of the `OfficeArtBlip` record at `at` in `d`.
fn blip_of(d: &[u8], at: usize) -> Option<Blip> {
    let (rec, _) = record_at(d, at)?;
    // The record's unique identifiers: one, or two when the instance is odd.
    let skip = if rec.inst & 1 == 1 { 32 } else { 16 };
    let body = rec.data;
    match rec.ty {
        0xF01D | 0xF02A => Some(Blip { ext: "jpeg", mime: "image/jpeg", data: body.get(skip + 1..)?.to_vec() }),
        0xF01E => Some(Blip { ext: "png", mime: "image/png", data: body.get(skip + 1..)?.to_vec() }),
        0xF029 => Some(Blip { ext: "tiff", mime: "image/tiff", data: body.get(skip + 1..)?.to_vec() }),
        0xF01F => Some(Blip { ext: "bmp", mime: "image/bmp", data: dib_to_bmp(body.get(skip + 1..)?) }),
        0xF01A | 0xF01B => {
            // A metafile: a 34-byte header and the data, deflated unless the header says it is not.
            let header = body.get(skip..skip + 34)?;
            let compression = header[32];
            let data = body.get(skip + 34..)?;
            let data = if compression == 0 { miniz_oxide::inflate::decompress_to_vec_zlib(data).ok()? } else { data.to_vec() };
            if rec.ty == 0xF01A {
                Some(Blip { ext: "emf", mime: "image/x-emf", data })
            } else {
                Some(Blip { ext: "wmf", mime: "image/x-wmf", data })
            }
        }
        _ => None,
    }
}

/// A device-independent bitmap (no file header) as a `.bmp` file.
fn dib_to_bmp(dib: &[u8]) -> Vec<u8> {
    let header_size = u32_at(dib, 0).unwrap_or(40) as usize;
    let bits = u16_at(dib, 14).unwrap_or(24) as u32;
    let compression = u32_at(dib, 16).unwrap_or(0);
    let used = u32_at(dib, 32).unwrap_or(0);
    let colors = if used != 0 { used } else if bits <= 8 { 1u32 << bits } else { 0 };
    let masks = if compression == 3 && header_size == 40 { 12 } else { 0 };
    let offset = 14 + header_size + masks + colors as usize * 4;
    let mut out = Vec::with_capacity(dib.len() + 14);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((dib.len() + 14) as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(offset as u32).to_le_bytes());
    out.extend_from_slice(dib);
    out
}

/// The slide list of one kind (`SlideListWithText`, instance 0 slides, 1 masters, 2 notes): for each entry its
/// persist object and the texts that follow it.
fn read_slide_list<'a>(list: &Rec<'a>) -> Vec<(u32, u32, Vec<TextIr>)> {
    let mut out: Vec<(u32, u32, Vec<TextIr>)> = Vec::new();
    let mut atoms: Option<TextAtoms<'a>> = None;
    let finish = |atoms: &mut Option<TextAtoms>, out: &mut Vec<(u32, u32, Vec<TextIr>)>| {
        if let Some(a) = atoms.take() {
            if let (Some(text), Some(last)) = (build_text(&a), out.last_mut()) {
                last.2.push(text);
            }
        }
    };
    for r in list.children() {
        match r.ty {
            rt::SLIDE_PERSIST_ATOM => {
                finish(&mut atoms, &mut out);
                out.push((u32_at(r.data, 0).unwrap_or(0), u32_at(r.data, 12).unwrap_or(0), Vec::new()));
            }
            rt::TEXT_HEADER_ATOM => {
                finish(&mut atoms, &mut out);
                atoms = Some(TextAtoms { kind: u32_at(r.data, 0).unwrap_or(4), ..Default::default() });
            }
            rt::TEXT_CHARS_ATOM | rt::TEXT_BYTES_ATOM => {
                if let Some(a) = atoms.as_mut() {
                    a.chars = Some(decode_text(&r));
                }
            }
            rt::STYLE_TEXT_PROP_ATOM => {
                if let Some(a) = atoms.as_mut() {
                    a.style = Some(r);
                }
            }
            _ => {}
        }
    }
    finish(&mut atoms, &mut out);
    out
}

/// Read the presentation of an old file.
pub(crate) fn read_ppt(data: &[u8]) -> Result<PptIr, Error> {
    let doc = cfb::stream(data, "PowerPoint Document").map_err(|_| bad("PowerPoint Document がありません"))?;
    let current = cfb::stream(data, "Current User").map_err(|_| bad("Current User がありません"))?;
    let pictures = cfb::stream(data, "Pictures").unwrap_or_default();
    read_ppt_streams(&doc, &current, &pictures)
}

/// The presentation of the three streams of an old file.
pub(crate) fn read_ppt_streams(doc: &[u8], current: &[u8], pictures: &[u8]) -> Result<PptIr, Error> {
    let (doc, current, pictures) = (doc, current, pictures);
    let (map, document_id) = persist_directory(doc, current)?;
    let document = persist_object(&doc, &map, document_id).filter(|r| r.ty == rt::DOCUMENT).ok_or_else(|| bad("DocumentContainer が見つかりません"))?;
    let mut ir = PptIr::default();
    let children = document.children();
    if let Some(atom) = children.iter().find(|r| r.ty == rt::DOCUMENT_ATOM) {
        let (x, y) = (u32_at(atom.data, 0).unwrap_or(5760) as i64, u32_at(atom.data, 4).unwrap_or(4320) as i64);
        ir.size = (emu(x), emu(y));
    }
    if let Some(env) = children.iter().find(|r| r.ty == rt::ENVIRONMENT) {
        for r in env.children() {
            match r.ty {
                rt::FONT_COLLECTION => {
                    for f in r.children().iter().filter(|f| f.ty == rt::FONT_ENTITY_ATOM) {
                        ir.fonts.push(utf16_string(f.data.get(..64).unwrap_or(f.data)));
                    }
                }
                rt::TEXT_MASTER_STYLE_ATOM => {
                    if let Some(style) = parse_master_style(&r) {
                        ir.defaults.insert(r.inst, style);
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(list) = children.iter().find(|r| r.ty == 0x0409) {
        for link in list.children().iter().filter(|r| r.ty == 0x0FD7) {
            let parts = link.children();
            let id = parts.iter().find(|r| r.ty == 0x0FD3).and_then(|r| u32_at(r.data, 0));
            let target = parts.iter().find(|r| r.ty == 0x0FBA && r.inst == 1).map(|r| utf16_string(r.data));
            if let (Some(id), Some(target)) = (id, target) {
                ir.hyperlinks.insert(id, target);
            }
        }
    }
    if let Some(group) = children.iter().find(|r| r.ty == rt::PP_DRAWING_GROUP) {
        ir.blips = read_blips(group, &pictures);
        if let Some(dgg) = group.child(rt::DGG_CONTAINER) {
            for r in dgg.children().iter().filter(|r| r.ty == rt::FOPT) {
                Props::parse(r, &mut ir.default_props);
            }
        }
    }
    for list in children.iter().filter(|r| r.ty == rt::SLIDE_LIST_WITH_TEXT) {
        let entries = read_slide_list(list);
        for (persist, slide_id, outline) in entries {
            let Some(rec) = persist_object(&doc, &map, persist) else { continue };
            let mut page = read_page(&rec);
            page.id = slide_id;
            page.outline = outline;
            match (list.inst, rec.ty) {
                (1, rt::MAIN_MASTER) => ir.masters.push(page),
                (0, rt::SLIDE) => ir.slides.push(page),
                (2, rt::NOTES) => {
                    ir.notes.insert(slide_id, page);
                }
                _ => {}
            }
        }
    }
    if ir.slides.is_empty() {
        return Err(bad("スライドがありません"));
    }
    Ok(ir)
}

/// The color an `OfficeArtCOLORREF` value is (for the writer).
pub(crate) fn art_color_of(v: u32) -> TextColor {
    art_color(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ver: u8, inst: u16, ty: u16, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&((u16::from(ver)) | (inst << 4)).to_le_bytes());
        out.extend_from_slice(&ty.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    /// The text of a placeholder that is in the slide list (as PowerPoint writes titles and bodies), in one-byte
    /// characters, with a paragraph and a character formatting run.
    #[test]
    fn a_text_in_the_slide_list_is_read_by_its_index() {
        // The slide list: a slide, then the texts that follow it.
        let mut list = Vec::new();
        list.extend(rec(0, 0, rt::SLIDE_PERSIST_ATOM, &[2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 7, 1, 0, 0, 0, 0, 0, 0]));
        list.extend(rec(0, 0, rt::TEXT_HEADER_ATOM, &0u32.to_le_bytes()));
        list.extend(rec(0, 0, rt::TEXT_BYTES_ATOM, b"Title\rline"));
        // Paragraph runs: 6 characters then 5 (and the last mark); character runs: 6 plain, 5 bold.
        let mut style = Vec::new();
        style.extend_from_slice(&6u32.to_le_bytes());
        style.extend_from_slice(&0u16.to_le_bytes());
        style.extend_from_slice(&0u32.to_le_bytes());
        style.extend_from_slice(&6u32.to_le_bytes());
        style.extend_from_slice(&1u16.to_le_bytes());
        style.extend_from_slice(&0u32.to_le_bytes());
        style.extend_from_slice(&6u32.to_le_bytes());
        style.extend_from_slice(&0u32.to_le_bytes());
        style.extend_from_slice(&5u32.to_le_bytes());
        style.extend_from_slice(&1u32.to_le_bytes());
        style.extend_from_slice(&1u16.to_le_bytes());
        list.extend(rec(0, 0, rt::STYLE_TEXT_PROP_ATOM, &style));
        list.extend(rec(0, 0, rt::TEXT_HEADER_ATOM, &1u32.to_le_bytes()));
        list.extend(rec(0, 0, rt::TEXT_CHARS_ATOM, &"本文".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>()));
        let container = rec(0xF, 0, rt::SLIDE_LIST_WITH_TEXT, &list);
        let parsed = records(&container);
        let entries = read_slide_list(&parsed[0]);
        assert_eq!(entries.len(), 1);
        let (persist, slide_id, texts) = &entries[0];
        assert_eq!((*persist, *slide_id), (2, 263));
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].kind, 0);
        assert_eq!(texts[0].paragraphs.len(), 2);
        assert_eq!(texts[0].paragraphs[1].level, 1);
        assert_eq!(texts[0].paragraphs[0].runs[0].0, "Title");
        assert_eq!(texts[0].paragraphs[1].runs[0].0, "line");
        assert_eq!(texts[0].paragraphs[1].runs[0].1.bold, Some(true));
        assert_eq!(texts[1].kind, 1);
        assert_eq!(texts[1].paragraphs[0].runs[0].0, "本文");
        // A client textbox that points to the first of them.
        let textbox = rec(0xF, 0, rt::CLIENT_TEXTBOX, &rec(0, 0, rt::OUTLINE_TEXT_REF_ATOM, &1u32.to_le_bytes()));
        let parsed = records(&textbox);
        assert_eq!(read_textbox(&parsed[0], None).unwrap_err(), 1);
    }

    #[test]
    fn a_color_index_and_an_srgb_color_are_told_apart() {
        assert_eq!(text_color(u32::from_le_bytes([1, 2, 3, 0xFE])), Some(TextColor::Rgb(0x010203)));
        assert_eq!(text_color(u32::from_le_bytes([0, 0, 0, 3])), Some(TextColor::Scheme(3)));
        assert_eq!(text_color(u32::from_le_bytes([0, 0, 0, 0xFF])), None);
    }

    /// The presentation of `data` written as an encrypted one (RC4 CryptoAPI), by the rules of [MS-PPT] 2.3.7: one edit, whose
    /// persist object directory has the persist object of the encryption's parameters too; each persist object and each field of
    /// each picture encrypted. (Only to read back what the reading above takes apart.)
    fn encrypted(data: &[u8], password: &str, key_bits: u32) -> Vec<u8> {
        use ibis_ooxml::legacy_crypt::{Rc4Keys, make_cryptoapi_info, parse_rc4};
        let mut doc = cfb::stream(data, "PowerPoint Document").unwrap();
        let mut current = cfb::stream(data, "Current User").unwrap();
        let mut pictures = cfb::stream(data, "Pictures").unwrap_or_default();
        let (mut map, _) = persist_directory(&doc, &current).unwrap();
        let info = make_cryptoapi_info(password, *b"0123456789abcdef", *b"fedcba9876543210", key_bits);
        let keys = Rc4Keys::new(&parse_rc4(&info).unwrap(), password).unwrap();
        // The persist objects, encrypted where they are.
        for (&id, &at) in &map {
            let len = u32::from_le_bytes(doc[at + 4..at + 8].try_into().unwrap()) as usize;
            keys.block_cipher(id).apply(&mut doc[at..at + 8 + len]);
        }
        apply_to_pictures(&mut pictures, &keys, false);
        // The persist object of the parameters (its header not encrypted), the directory of all and the one edit.
        let session_id = map.keys().max().unwrap() + 1;
        let session_at = doc.len();
        doc.extend(rec(0xF, 0, 0x2F14, &info));
        map.insert(session_id, session_at);
        let mut ids: Vec<u32> = map.keys().copied().collect();
        ids.sort();
        let mut dir = Vec::new();
        for id in ids {
            dir.extend_from_slice(&(id | (1 << 20)).to_le_bytes());
            dir.extend_from_slice(&(map[&id] as u32).to_le_bytes());
        }
        let dir_at = doc.len();
        doc.extend(rec(0, 0, rt::PERSIST_DIRECTORY_ATOM, &dir));
        let (atom, _) = record_at(&current, 0).unwrap();
        let old_edit_at = u32_at(atom.data, 8).unwrap() as usize;
        let (old_edit, _) = record_at(&doc, old_edit_at).unwrap();
        let mut edit = old_edit.data[..28].to_vec();
        edit[8..12].copy_from_slice(&0u32.to_le_bytes());
        edit[12..16].copy_from_slice(&(dir_at as u32).to_le_bytes());
        edit[20..24].copy_from_slice(&(session_id + 1).to_le_bytes());
        edit.extend_from_slice(&session_id.to_le_bytes());
        let edit_at = doc.len();
        doc.extend(rec(0, 0, rt::USER_EDIT_ATOM, &edit));
        current[12..16].copy_from_slice(&ENCRYPTED_TOKEN.to_le_bytes());
        current[16..20].copy_from_slice(&(edit_at as u32).to_le_bytes());
        ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream("PowerPoint Document", doc), ibis_ooxml::cfb::Item::stream("Current User", current), ibis_ooxml::cfb::Item::stream("Pictures", pictures)])
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name)).unwrap()
    }

    #[test]
    fn a_presentation_with_a_password_is_opened_with_it() {
        let plain = fixture("pptx-legacy.ppt");
        assert!(!is_encrypted(&plain));
        assert!(!cfb::stream(&plain, "Pictures").unwrap_or_default().is_empty(), "the pictures are in the Pictures stream");
        for bits in [40, 128] {
            let locked = encrypted(&plain, "secret", bits);
            assert!(is_ppt(&locked) && is_encrypted(&locked), "{bits}");
            // Without the password the presentation is not read (and says why).
            let err = to_pptx_message(&locked);
            assert!(err.contains("パスワード"), "{err}");
            assert_eq!(decrypt(&locked, "wrong").unwrap_err(), ibis_ooxml::encryption::DecryptError::WrongPassword);
            let opened = decrypt(&locked, "secret").unwrap();
            // The same presentation as the one that was not locked: the same text, shapes and pictures.
            let (a, b) = (crate::read(&opened).unwrap(), crate::read(&plain).unwrap());
            assert_eq!(a.slides.len(), b.slides.len());
            let texts = |p: &ibis_doc_model::Presentation| p.slides.iter().map(|s| s.shapes.iter().filter_map(|sh| sh.text_body().map(|t| t.text())).collect::<Vec<_>>()).collect::<Vec<_>>();
            assert_eq!(texts(&a), texts(&b));
            let pictures = |p: &ibis_doc_model::Presentation| p.slides.iter().flat_map(|s| &s.shapes).filter(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_))).count();
            assert_eq!(pictures(&a), pictures(&b));
            assert!(pictures(&a) > 0);
            let media = |p: &ibis_doc_model::Presentation| p.media.iter().map(|m| m.data.len()).collect::<Vec<_>>();
            assert_eq!(media(&a), media(&b), "the pictures are decrypted");
        }
    }

    fn to_pptx_message(data: &[u8]) -> String {
        crate::ppt_write::to_pptx(data).unwrap_err().to_string()
    }
}
