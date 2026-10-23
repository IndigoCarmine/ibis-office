// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The host's side of the objects (feature `host`): an embedded object's
//! compound file to the [`OleContent`] an editor is given and back, and an
//! editor's [`Preview`] to the PNG Office shows.

use ibis_ooxml::cfb::Item;
use ibis_ooxml::ole::{OleStorage, is_container_item};

use crate::{Clsid, Error, OleContent, OleStream, Preview, Result};

/// What an editor is given of the object in `data` (an `oleObjectN.bin`):
/// its class and its content streams, without the container's.
pub fn content_of(data: &[u8]) -> Result<OleContent> {
    let storage = OleStorage::parse(data).map_err(|e| Error::Invalid(e.message().to_string()))?;
    Ok(OleContent {
        clsid: Clsid(storage.clsid),
        prog_id: storage.prog_id.clone().unwrap_or_default(),
        user_type: storage.user_type.clone().unwrap_or_default(),
        streams: storage
            .content()
            .filter_map(|i| Some(OleStream { path: i.path.clone(), bytes: i.data.clone()? }))
            .collect(),
    })
}

/// The compound file of `content`: `original`'s (the object as it was)
/// with its content replaced, so what the container keeps of it stays, or a
/// new one.
pub fn object_of(content: &OleContent, original: Option<&[u8]>) -> Result<Vec<u8>> {
    if content.prog_id.is_empty() {
        return Err(Error::Invalid("the object has no ProgID".into()));
    }
    if let Some(bad) = content.streams.iter().find(|s| s.path.is_empty() || s.path.split('/').any(str::is_empty) || is_container_item(&s.path)) {
        return Err(Error::Invalid(format!("a stream may not be called {:?}", bad.path)));
    }
    // The storages the streams are in, each before what is in it.
    let mut items: Vec<Item> = Vec::new();
    for stream in &content.streams {
        let mut prefix = String::new();
        for part in stream.path.split('/').collect::<Vec<_>>().split_last().map_or(&[][..], |(_, dirs)| dirs) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            if !items.iter().any(|i| i.path == prefix) {
                items.push(Item::storage(&prefix));
            }
        }
        if items.iter().any(|i| i.path == stream.path) {
            return Err(Error::Invalid(format!("the stream {:?} is there twice", stream.path)));
        }
        items.push(Item::stream(&stream.path, stream.bytes.clone()));
    }
    let mut storage = original.and_then(|d| OleStorage::parse(d).ok()).unwrap_or_else(|| OleStorage::new(content.clsid.0, &content.prog_id, &content.user_type, Vec::new()));
    // The storages' own classes, where they were there before.
    for item in items.iter_mut().filter(|i| !i.is_stream()) {
        if let Some(old) = storage.items.iter().find(|o| o.path == item.path && !o.is_stream()) {
            item.clsid = old.clsid;
        }
    }
    let user_type = if content.user_type.is_empty() { storage.user_type.clone().unwrap_or_default() } else { content.user_type.clone() };
    storage.set_class(content.clsid.0, &content.prog_id, &user_type);
    storage.replace_content(items);
    Ok(storage.to_bytes())
}

/// The longest side of a preview drawn from SVG, in pixels.
const MAX_SIDE: f32 = 2400.0;

/// The PNG to show for `preview`: the editor's, or its SVG drawn at twice
/// the size it shows at (`size_emu`, else the SVG's own), at most
/// [`MAX_SIDE`] pixels a side.
pub fn preview_png(preview: &Preview, size_emu: Option<(i64, i64)>) -> Result<Vec<u8>> {
    if let Some(png) = &preview.png {
        return Ok(png.clone());
    }
    let svg = preview.svg.as_deref().ok_or_else(|| Error::Invalid("the editor gave no preview".into()))?;
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(svg, &options).map_err(|e| Error::Invalid(format!("the preview is not SVG: {e}")))?;
    let size = tree.size();
    // 96 pixels an inch, 914 400 EMU an inch; twice that for screens and printing.
    let wanted = size_emu.map_or(size.width() * 2.0, |(w, _)| w as f32 / 914_400.0 * 96.0 * 2.0);
    let scale = (wanted / size.width()).min(MAX_SIDE / size.width().max(size.height())).max(0.01);
    let (w, h) = ((size.width() * scale).ceil().max(1.0) as u32, (size.height() * scale).ceil().max(1.0) as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or_else(|| Error::Invalid("the preview is too large".into()))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| Error::Invalid(format!("the preview could not be drawn: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(streams: &[(&str, &[u8])]) -> OleContent {
        OleContent {
            clsid: Clsid([7; 16]),
            prog_id: "Example.Drawing.1".into(),
            user_type: "Example Drawing".into(),
            streams: streams.iter().map(|(p, b)| OleStream { path: p.to_string(), bytes: b.to_vec() }).collect(),
        }
    }

    #[test]
    fn content_goes_out_and_comes_back() {
        let first = object_of(&content(&[("CONTENTS", b"one"), ("Sub/Inner", b"two")]), None).unwrap();
        let back = content_of(&first).unwrap();
        assert_eq!(back.clsid, Clsid([7; 16]));
        assert_eq!(back.prog_id, "Example.Drawing.1");
        assert_eq!(back.user_type, "Example Drawing");
        assert_eq!(back.stream("CONTENTS"), Some(&b"one"[..]));
        assert_eq!(back.stream("Sub/Inner"), Some(&b"two"[..]));
        assert_eq!(back.streams.len(), 2);
    }

    #[test]
    fn the_container_streams_stay_when_edited() {
        let mut storage = OleStorage::parse(&object_of(&content(&[("CONTENTS", b"one")]), None).unwrap()).unwrap();
        storage.items.push(Item::stream("\u{3}ObjInfo", vec![1, 2, 3, 4, 5, 6]));
        let original = storage.to_bytes();
        let mut edited = content(&[("CONTENTS", b"two")]);
        edited.user_type.clear();
        let after = OleStorage::parse(&object_of(&edited, Some(&original)).unwrap()).unwrap();
        assert!(after.items.iter().any(|i| i.path == "\u{3}ObjInfo"));
        assert_eq!(after.user_type.as_deref(), Some("Example Drawing"));
        assert_eq!(content_of(&after.to_bytes()).unwrap().stream("CONTENTS"), Some(&b"two"[..]));
    }

    /// An object whose application wrote its own `\x01CompObj` (here ANSI
    /// names with a clipboard format, the Unicode part left empty): edited,
    /// the class unchanged, it stays as the application wrote it.
    #[test]
    fn the_applications_comp_obj_stays_when_edited() {
        let ansi = |out: &mut Vec<u8>, s: &str| {
            out.extend_from_slice(&(s.len() as u32 + 1).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
            out.push(0);
        };
        let mut comp_obj = [0xFFFE_0001u32, 0x0000_0A03, 0xFFFF_FFFF].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
        comp_obj.extend_from_slice(&[7; 16]);
        ansi(&mut comp_obj, "Example Drawing");
        ansi(&mut comp_obj, "Example Native Format");
        ansi(&mut comp_obj, "Example.Drawing.1");
        comp_obj.extend_from_slice(&0x71B2_39F4u32.to_le_bytes());
        comp_obj.extend_from_slice(&[0; 12]);
        let mut storage = OleStorage::parse(&object_of(&content(&[("CONTENTS", b"one")]), None).unwrap()).unwrap();
        storage.items.iter_mut().find(|i| i.path == "\u{1}CompObj").unwrap().data = Some(comp_obj.clone());
        let original = storage.to_bytes();

        let edited = content(&[("CONTENTS", b"two")]);
        let after = OleStorage::parse(&object_of(&edited, Some(&original)).unwrap()).unwrap();
        let comp_obj_after = after.items.iter().find(|i| i.path == "\u{1}CompObj").and_then(|i| i.data.clone());
        assert_eq!(comp_obj_after, Some(comp_obj), "byte for byte");
        assert_eq!(content_of(&after.to_bytes()).unwrap().stream("CONTENTS"), Some(&b"two"[..]));
        // Another class: ibisOffice's own.
        let mut other = edited.clone();
        other.prog_id = "Other.Drawing.2".into();
        let after = OleStorage::parse(&object_of(&other, Some(&original)).unwrap()).unwrap();
        assert_eq!(after.prog_id.as_deref(), Some("Other.Drawing.2"));
    }

    #[test]
    fn bad_streams_are_refused() {
        assert!(object_of(&content(&[("\u{1}CompObj", b"x")]), None).is_err());
        assert!(object_of(&content(&[("a//b", b"x")]), None).is_err());
        assert!(object_of(&content(&[("A", b"x"), ("A", b"y")]), None).is_err());
        let mut no_prog_id = content(&[]);
        no_prog_id.prog_id.clear();
        assert!(object_of(&no_prog_id, None).is_err());
    }

    #[test]
    fn an_svg_preview_is_drawn() {
        let preview = Preview::svg(r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect width="100" height="50" fill="red"/></svg>"#);
        let png = preview_png(&preview, Some((914_400, 457_200))).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        // One inch at 96 dpi, twice: 192 × 96.
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 192);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 96);
        assert_eq!(preview_png(&Preview::png(vec![1, 2]), None).unwrap(), vec![1, 2]);
        assert!(preview_png(&Preview::default(), None).is_err());
    }
}
