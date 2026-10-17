// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An embedded OLE object's storage (`oleObjectN.bin`), from the public
//! specification (MS-OLEDS): its class, the `\u{1}CompObj` and `\u{1}Ole`
//! streams the container keeps, and the content the object's application
//! owns. No COM: the object is a compound file read and written like any
//! other, so an editor on any platform can change its content.

use crate::Error;
use crate::cfb::{self, Item};

const COMP_OBJ: &str = "\u{1}CompObj";
const OLE: &str = "\u{1}Ole";
const UNICODE_MARKER: u32 = 0x71B2_39F4;

/// An OLE object's storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OleStorage {
    /// The class of the object (the root storage's CLSID).
    pub clsid: [u8; 16],
    /// The ProgID, from `\u{1}CompObj` (as `ChemDraw.Document.6.0`).
    pub prog_id: Option<String>,
    /// The name of the kind of object shown to people, from `\u{1}CompObj`.
    pub user_type: Option<String>,
    /// Every storage and stream, each storage before what is in it.
    pub items: Vec<Item>,
}

/// Whether the item at `path` is the container's, not the application's: the
/// top-level names starting with a control character (`\u{1}CompObj`,
/// `\u{1}Ole`, `\u{3}ObjInfo`, `\u{2}OlePres000`, …).
pub fn is_container_item(path: &str) -> bool {
    path.split('/').next().and_then(|n| n.chars().next()).is_some_and(|c| (c as u32) < 0x20)
}

impl OleStorage {
    /// The storage in `data` (an `oleObjectN.bin`).
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let contents = cfb::read(data)?;
        let mut storage = Self { clsid: contents.clsid, prog_id: None, user_type: None, items: contents.items };
        if let Some(comp_obj) = storage.items.iter().find(|i| i.path == COMP_OBJ).and_then(|i| i.data.as_deref()) {
            let (user_type, prog_id) = parse_comp_obj(comp_obj);
            storage.user_type = user_type;
            storage.prog_id = prog_id;
        }
        Ok(storage)
    }

    /// A new object of the class `clsid` holding `content` (the application's storages and streams).
    pub fn new(clsid: [u8; 16], prog_id: &str, user_type: &str, content: Vec<Item>) -> Self {
        let mut storage = Self { clsid, prog_id: None, user_type: None, items: vec![Item::stream(OLE, ole_stream())] };
        storage.set_class(clsid, prog_id, user_type);
        storage.replace_content(content);
        storage
    }

    /// The application's storages and streams: what an editor is given.
    pub fn content(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| !is_container_item(&i.path))
    }

    /// The application's part replaced by `content`; the container's streams
    /// stay, except a cached picture of the old content (`\u{2}OlePres…`), which
    /// would now show the wrong thing.
    pub fn replace_content(&mut self, content: Vec<Item>) {
        self.items.retain(|i| is_container_item(&i.path) && !i.path.starts_with("\u{2}OlePres"));
        self.items.extend(content.into_iter().filter(|i| !is_container_item(&i.path)));
        if !self.items.iter().any(|i| i.path == OLE) {
            self.items.insert(0, Item::stream(OLE, ole_stream()));
        }
        sort_parents_first(&mut self.items);
    }

    /// The class, ProgID and user type, written to the root and `\u{1}CompObj`.
    pub fn set_class(&mut self, clsid: [u8; 16], prog_id: &str, user_type: &str) {
        self.clsid = clsid;
        self.prog_id = Some(prog_id.to_string());
        self.user_type = Some(user_type.to_string());
        let comp_obj = comp_obj_stream(clsid, user_type, prog_id);
        match self.items.iter_mut().find(|i| i.path == COMP_OBJ) {
            Some(item) => item.data = Some(comp_obj),
            None => self.items.insert(0, Item::stream(COMP_OBJ, comp_obj)),
        }
    }

    /// The compound file.
    pub fn to_bytes(&self) -> Vec<u8> {
        cfb::write_with(self.clsid, &self.items)
    }
}

/// Storages before what is in them, otherwise in the order given.
fn sort_parents_first(items: &mut [Item]) {
    items.sort_by_key(|i| i.path.matches('/').count());
}

/// `\u{1}Ole` for an embedded (not linked) object (MS-OLEDS 2.3.3).
fn ole_stream() -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(&0x0200_0001u32.to_le_bytes());
    out.extend_from_slice(&[0; 16]);
    out
}

fn ansi(out: &mut Vec<u8>, s: &str) {
    // Length-prefixed, with the terminating zero; names outside ASCII are replaced.
    let bytes: Vec<u8> = s.chars().map(|c| if c.is_ascii() { c as u8 } else { b'?' }).collect();
    out.extend_from_slice(&(bytes.len() as u32 + 1).to_le_bytes());
    out.extend_from_slice(&bytes);
    out.push(0);
}

fn unicode(out: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.extend_from_slice(&(units.len() as u32 + 1).to_le_bytes());
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
}

/// `\u{1}CompObj` (MS-OLEDS 2.3.8): the class, the user type and the ProgID,
/// in ANSI and again in Unicode.
fn comp_obj_stream(clsid: [u8; 16], user_type: &str, prog_id: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0xFFFE_0001u32.to_le_bytes());
    out.extend_from_slice(&0x0000_0A03u32.to_le_bytes());
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    out.extend_from_slice(&clsid);
    ansi(&mut out, user_type);
    out.extend_from_slice(&0u32.to_le_bytes()); // no clipboard format
    ansi(&mut out, prog_id);
    out.extend_from_slice(&UNICODE_MARKER.to_le_bytes());
    unicode(&mut out, user_type);
    out.extend_from_slice(&0u32.to_le_bytes());
    unicode(&mut out, prog_id);
    out
}

/// The user type and ProgID in a `\u{1}CompObj`, the Unicode ones when there.
fn parse_comp_obj(d: &[u8]) -> (Option<String>, Option<String>) {
    struct Cursor<'a>(&'a [u8], usize);
    impl Cursor<'_> {
        fn u32(&mut self) -> Option<u32> {
            let v = u32::from_le_bytes(self.0.get(self.1..self.1 + 4)?.try_into().ok()?);
            self.1 += 4;
            Some(v)
        }
        fn ansi(&mut self) -> Option<String> {
            let len = self.u32()? as usize;
            let bytes = self.0.get(self.1..self.1.checked_add(len)?)?;
            self.1 += len;
            let text: String = bytes.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
            Some(text)
        }
        fn unicode(&mut self) -> Option<String> {
            let len = (self.u32()? as usize).checked_mul(2)?;
            let bytes = self.0.get(self.1..self.1.checked_add(len)?)?;
            self.1 += len;
            let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
            Some(String::from_utf16_lossy(&units))
        }
        /// A clipboard format: none, a standard one by number, or a registered one by name.
        fn format(&mut self, wide: bool) -> Option<()> {
            match self.u32()? {
                0 => {}
                0xFFFF_FFFF | 0xFFFF_FFFE => {
                    self.u32()?;
                }
                len => {
                    let len = if wide { (len as usize).checked_mul(2)? } else { len as usize };
                    self.1 = self.1.checked_add(len)?;
                }
            }
            Some(())
        }
    }
    let nonempty = |s: Option<String>| s.filter(|s| !s.is_empty());
    let mut c = Cursor(d, 28);
    let user_type = nonempty(c.ansi());
    if c.format(false).is_none() {
        return (user_type, None);
    }
    let prog_id = nonempty(c.ansi());
    if c.u32() != Some(UNICODE_MARKER) {
        return (user_type, prog_id);
    }
    let wide_user_type = nonempty(c.unicode());
    let wide_prog_id = c.format(true).and_then(|_| nonempty(c.unicode()));
    (wide_user_type.or(user_type), wide_prog_id.or(prog_id))
}

/// A CLSID as written in the registry and in XML (`{41BA6D21-A02E-11CE-8FD9-0020AFD1F20C}`):
/// the first three fields are little-endian in the file.
pub fn clsid_to_string(c: &[u8; 16]) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        u32::from_le_bytes([c[0], c[1], c[2], c[3]]),
        u16::from_le_bytes([c[4], c[5]]),
        u16::from_le_bytes([c[6], c[7]]),
        c[8], c[9], c[10], c[11], c[12], c[13], c[14], c[15]
    )
}

/// The bytes of a CLSID written as [`clsid_to_string`] writes it (braces optional).
pub fn clsid_from_str(s: &str) -> Option<[u8; 16]> {
    let hex: String = s.trim().trim_start_matches('{').trim_end_matches('}').chars().filter(|&c| c != '-').collect();
    if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok();
    let mut be = [0u8; 16];
    for (i, b) in be.iter_mut().enumerate() {
        *b = byte(i)?;
    }
    let mut out = be;
    out[0..4].reverse();
    out[4..6].reverse();
    out[6..8].reverse();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLSID: &str = "{41BA6D21-A02E-11CE-8FD9-0020AFD1F20C}";

    #[test]
    fn a_clsid_reads_back() {
        let c = clsid_from_str(CLSID).unwrap();
        assert_eq!(c[0], 0x21);
        assert_eq!(clsid_to_string(&c), CLSID);
        assert!(clsid_from_str("{nope}").is_none());
    }

    #[test]
    fn a_new_object_reads_back() {
        let clsid = clsid_from_str(CLSID).unwrap();
        let object = OleStorage::new(clsid, "Example.Document.1", "Example Document", vec![Item::stream("CONTENTS", b"abc".to_vec())]);
        let back = OleStorage::parse(&object.to_bytes()).unwrap();
        assert_eq!(back.clsid, clsid);
        assert_eq!(back.prog_id.as_deref(), Some("Example.Document.1"));
        assert_eq!(back.user_type.as_deref(), Some("Example Document"));
        let content: Vec<_> = back.content().collect();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].data.as_deref(), Some(&b"abc"[..]));
        assert!(back.items.iter().any(|i| i.path == OLE));
    }

    #[test]
    fn new_content_keeps_the_container_streams() {
        let clsid = clsid_from_str(CLSID).unwrap();
        let mut object = OleStorage::new(clsid, "Example.Document.1", "Example", vec![Item::stream("CONTENTS", b"old".to_vec()), Item::stream("Extra", b"x".to_vec())]);
        object.items.push(Item::stream("\u{3}ObjInfo", vec![0, 0, 3, 0, 4, 0]));
        object.items.push(Item::stream("\u{2}OlePres000", vec![1, 2, 3]));
        object.replace_content(vec![Item::storage("S"), Item::stream("S/CONTENTS", b"new".to_vec())]);
        let back = OleStorage::parse(&object.to_bytes()).unwrap();
        let mut paths: Vec<_> = back.items.iter().map(|i| i.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, ["\u{1}CompObj", "\u{1}Ole", "\u{3}ObjInfo", "S", "S/CONTENTS"]);
        assert_eq!(back.prog_id.as_deref(), Some("Example.Document.1"));
    }

    #[test]
    fn a_comp_obj_with_only_ansi_names_reads() {
        // As some applications write it: no Unicode part.
        let mut d = vec![0u8; 28];
        ansi(&mut d, "Old Type");
        d.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        d.extend_from_slice(&3u32.to_le_bytes());
        ansi(&mut d, "Old.Prog.1");
        assert_eq!(parse_comp_obj(&d), (Some("Old Type".into()), Some("Old.Prog.1".into())));
        assert_eq!(parse_comp_obj(&[0; 10]), (None, None));
    }
}
