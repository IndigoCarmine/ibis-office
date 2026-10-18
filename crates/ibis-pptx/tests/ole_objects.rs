// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Embedded OLE objects in presentations: read with their compound file,
//! inserted, edited (a new object and preview) and saved, with no COM.

use ibis_doc_model::{Command, Document, EmuRect, MediaFormat, Picture, Presentation, Shape, ShapeKind};
use ibis_ooxml::cfb::Item;
use ibis_ooxml::ole::{OleStorage, clsid_from_str};
use ibis_ooxml::Package;
use ibis_pptx::SaveOptions;

const PROG_ID: &str = "Example.Drawing.1";
const CLSID: &str = "{12345678-9ABC-DEF0-1122-334455667788}";

fn png(rgb: [u8; 3]) -> Vec<u8> {
    // A 1×1 PNG of one colour.
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn adler(data: &[u8]) -> u32 {
        let (mut a, mut b) = (1u32, 0u32);
        for &d in data {
            a = (a + d as u32) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc(&body).to_be_bytes());
    }
    let raw = [0, rgb[0], rgb[1], rgb[2]];
    let mut zlib = vec![0x78, 0x01, 1, raw.len() as u8, 0, !(raw.len() as u8), 0xFF];
    zlib.extend_from_slice(&raw);
    zlib.extend_from_slice(&adler(&raw).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn object(content: &[u8]) -> Vec<u8> {
    OleStorage::new(clsid_from_str(CLSID).unwrap(), PROG_ID, "Example Drawing", vec![Item::stream("CONTENTS", content.to_vec())]).to_bytes()
}

/// The embedded objects' pictures of the first slide.
fn objects(p: &Presentation) -> Vec<&Picture> {
    p.slides[0].shapes.iter().filter_map(|s| match &s.kind {
        ShapeKind::Picture(pic) if pic.ole.is_some() => Some(pic),
        _ => None,
    }).collect()
}

fn content(p: &Presentation, pic: &Picture) -> Vec<u8> {
    let ole = p.ole(pic.ole.unwrap()).unwrap();
    assert_eq!(ole.prog_id, PROG_ID);
    let storage = OleStorage::parse(&ole.data).unwrap();
    assert_eq!(storage.clsid, clsid_from_str(CLSID).unwrap());
    storage.items.iter().find(|i| i.path == "CONTENTS").and_then(|i| i.data.clone()).unwrap()
}

fn parts(file: &[u8], prefix: &str) -> Vec<String> {
    let package = Package::open(file.to_vec()).unwrap();
    package.names().filter(|n| n.starts_with(prefix)).map(str::to_string).collect()
}

fn slide_xml(file: &[u8]) -> String {
    let package = Package::open(file.to_vec()).unwrap();
    String::from_utf8(package.read("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap()
}

/// A presentation with one embedded object inserted and saved.
fn with_object() -> Vec<u8> {
    let opened = ibis_pptx::open(ibis_pptx::new_presentation().unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let slide = document.presentation().slides[0].id;
    let media = document.ids().add_media(MediaFormat::Png, png([255, 0, 0]));
    let ole = document.ids().add_ole(PROG_ID, object(b"first"), false);
    let shape = document.ids().new_shape_id();
    let mut picture = Picture::new(media);
    picture.ole = Some(ole);
    let frame = EmuRect::new(1_000_000, 1_000_000, 2_000_000, 1_500_000);
    let index = document.presentation().slides[0].shapes.len();
    document
        .execute(vec![Command::InsertShape { slide, index, shape: Shape { id: shape, frame, rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Picture(picture), floating: None } }])
        .unwrap();
    opened.source.save(document.presentation(), SaveOptions::default()).unwrap()
}

#[test]
fn an_inserted_object_reads_back() {
    let saved = with_object();
    let p = ibis_pptx::read(&saved).unwrap();
    let pics = objects(&p);
    assert_eq!(pics.len(), 1);
    assert_eq!(content(&p, pics[0]), b"first");
    assert_eq!(p.media(pics[0].media).unwrap().format, MediaFormat::Png);
    let xml = slide_xml(&saved);
    assert!(xml.contains("progId=\"Example.Drawing.1\"") && xml.contains("<p:embed/>"), "{xml}");
    let package = Package::open(saved.clone()).unwrap();
    assert_eq!(package.content_type("ppt/embeddings/oleObject1.bin").unwrap().as_deref(), Some("application/vnd.openxmlformats-officedocument.oleObject"));
}

/// The object of the first slide edited: a new compound file and preview.
fn edit(file: &[u8], new_content: &[u8]) -> Vec<u8> {
    let opened = ibis_pptx::open(file.to_vec()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let slide = document.presentation().slides[0].id;
    let shape = document.presentation().slides[0].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.ole.is_some())).unwrap().clone();
    let ShapeKind::Picture(mut picture) = shape.kind else { unreachable!() };
    picture.media = document.ids().add_media(MediaFormat::Png, png([0, 0, 255]));
    picture.ole = Some(document.ids().add_ole(PROG_ID, object(new_content), false));
    document.execute(vec![Command::SetKind { slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) }]).unwrap();
    opened.source.save(document.presentation(), SaveOptions::default()).unwrap()
}

#[test]
fn an_edited_object_replaces_the_old_one() {
    let edited = edit(&with_object(), b"second");
    let p = ibis_pptx::read(&edited).unwrap();
    let pics = objects(&p);
    assert_eq!(pics.len(), 1);
    assert_eq!(content(&p, pics[0]), b"second");
    assert_eq!(p.media(pics[0].media).unwrap().data.as_ref(), png([0, 0, 255]).as_slice());
    // The old object's part and picture are gone.
    assert_eq!(parts(&edited, "ppt/embeddings/").len(), 1);
    assert_eq!(parts(&edited, "ppt/media/").len(), 1);
}

#[test]
fn powerpoints_vml_alternative_is_dropped_when_edited() {
    // As PowerPoint writes an object: a VML choice (no picture) and a fallback with the picture.
    let saved = with_object();
    let mut package = Package::open(saved).unwrap();
    let xml = String::from_utf8(package.read("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap();
    let start = xml.find("<p:oleObj").unwrap();
    let end = xml.find("</p:oleObj>").unwrap() + "</p:oleObj>".len();
    let object_xml = &xml[start..end];
    let rid = object_xml.split("r:id=\"").nth(1).unwrap().split('"').next().unwrap();
    let alternate = format!(
        r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:v="urn:schemas-microsoft-com:vml" Requires="v"><p:oleObj spid="_x0000_s1026" name="Object" r:id="{rid}" imgW="1000" imgH="1000" progId="{PROG_ID}"><p:embed/></p:oleObj></mc:Choice><mc:Fallback>{object_xml}</mc:Fallback></mc:AlternateContent>"#
    );
    let changed = format!("{}{alternate}{}", &xml[..start], &xml[end..]);
    package.set_bytes("ppt/slides/slide1.xml", changed.into_bytes());
    let file = package.save(Default::default()).unwrap();
    let read = ibis_pptx::read(&file).unwrap();
    assert_eq!(content(&read, objects(&read)[0]), b"first");

    let edited = edit(&file, b"third");
    let xml = slide_xml(&edited);
    assert!(!xml.contains("AlternateContent") && !xml.contains("spid="), "{xml}");
    let p = ibis_pptx::read(&edited).unwrap();
    assert_eq!(content(&p, objects(&p)[0]), b"third");
    assert_eq!(parts(&edited, "ppt/embeddings/").len(), 1);
}

#[test]
fn a_moved_object_keeps_its_part() {
    let saved = with_object();
    let opened = ibis_pptx::open(saved.clone()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let slide = document.presentation().slides[0].id;
    let shape = objects_shape(document.presentation());
    let frame = EmuRect::new(2_000_000, 2_000_000, 2_000_000, 1_500_000);
    document.execute(vec![Command::SetFrame { slide, shape, frame }]).unwrap();
    let moved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let before = Package::open(saved).unwrap();
    let after = Package::open(moved.clone()).unwrap();
    assert_eq!(before.read("ppt/embeddings/oleObject1.bin").unwrap(), after.read("ppt/embeddings/oleObject1.bin").unwrap());
    let p = ibis_pptx::read(&moved).unwrap();
    assert_eq!(p.slides[0].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.ole.is_some())).unwrap().frame, frame);
}

/// The size the object's application draws it at (`imgW`, `imgH`) is read,
/// and written for a new object and for an edited one, so Office keeps the
/// object's scale when its application redraws it.
#[test]
fn an_objects_natural_size_is_kept() {
    let with_size = |file: &[u8], natural: (i64, i64), content: &[u8]| {
        let opened = ibis_pptx::open(file.to_vec()).unwrap();
        let mut document = Document::new(opened.presentation.clone());
        let slide = document.presentation().slides[0].id;
        let shape = document.presentation().slides[0].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.ole.is_some())).unwrap().clone();
        let ShapeKind::Picture(mut picture) = shape.kind else { unreachable!() };
        let ole = document.ids().add_ole(PROG_ID, object(content), false);
        document.ids().set_ole_natural(ole, Some(natural));
        picture.ole = Some(ole);
        document.execute(vec![Command::SetKind { slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) }]).unwrap();
        opened.source.save(document.presentation(), SaveOptions::default()).unwrap()
    };
    let natural = |file: &[u8]| {
        let p = ibis_pptx::read(file).unwrap();
        p.ole(objects(&p)[0].ole.unwrap()).unwrap().natural
    };
    assert_eq!(natural(&with_object()), None, "not known, not written");
    assert!(!slide_xml(&with_object()).contains("imgW="));

    let first = with_size(&with_object(), (500_000, 375_000), b"second");
    assert!(slide_xml(&first).contains(r#"imgW="500000" imgH="375000""#), "{}", slide_xml(&first));
    assert_eq!(natural(&first), Some((500_000, 375_000)));
    let again = with_size(&first, (600_000, 375_000), b"third");
    let xml = slide_xml(&again);
    assert!(xml.contains(r#"imgW="600000""#) && !xml.contains(r#"imgW="500000""#), "{xml}");
    assert_eq!(natural(&again), Some((600_000, 375_000)));
}

fn objects_shape(p: &Presentation) -> ibis_doc_model::ShapeId {
    p.slides[0].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.ole.is_some())).unwrap().id
}

// ----- Word -----------------------------------------------------------------

use ibis_doc_model::{Block, FlowCommand, FlowPos, FlowRunKind, PictureLook};

/// The document's inline pictures showing objects: (media, look).
fn word_objects(p: &Presentation) -> Vec<(ibis_doc_model::MediaId, PictureLook)> {
    let flow = p.flow.as_ref().unwrap();
    flow.body.iter().filter_map(|b| match b { Block::Paragraph(p) => Some(p), _ => None }).flat_map(|p| p.runs.iter()).filter_map(|r| match &r.kind {
        FlowRunKind::Picture { media, look, .. } if look.ole.is_some() => Some((*media, look.clone())),
        _ => None,
    }).collect()
}

fn first_para(p: &Presentation) -> u32 {
    p.flow.as_ref().unwrap().body.iter().find_map(|b| match b { Block::Paragraph(p) => Some(p.id), _ => None }).unwrap()
}

fn word_with_object() -> Vec<u8> {
    let opened = ibis_pptx::open(ibis_pptx::blank_file(ibis_pptx::Kind::Document).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let para = first_para(document.presentation());
    let media = document.ids().add_media(MediaFormat::Png, png([255, 0, 0]));
    let ole = document.ids().add_ole(PROG_ID, object(b"first"), false);
    let at = FlowPos::new(para, 0);
    let look = PictureLook { ole: Some(ole), ..Default::default() };
    document
        .execute(vec![
            Command::Flow(FlowCommand::InsertPicture { at, media, size: ibis_doc_model::EmuSize::new(1_270_000, 635_000) }),
            Command::Flow(FlowCommand::SetPictureLook { at, look }),
        ])
        .unwrap();
    opened.source.save(document.presentation(), SaveOptions::default()).unwrap()
}

fn word_xml(file: &[u8]) -> String {
    let package = Package::open(file.to_vec()).unwrap();
    String::from_utf8(package.read("word/document.xml").unwrap().to_vec()).unwrap()
}

#[test]
fn a_word_object_reads_back() {
    let saved = word_with_object();
    let xml = word_xml(&saved);
    assert!(xml.contains("<w:object") && xml.contains("ProgID=\"Example.Drawing.1\"") && xml.contains("width:100.00pt;height:50.00pt"), "{xml}");
    let p = ibis_pptx::read(&saved).unwrap();
    let found = word_objects(&p);
    assert_eq!(found.len(), 1);
    let ole = p.ole(found[0].1.ole.unwrap()).unwrap();
    assert_eq!(ole.prog_id, PROG_ID);
    let storage = OleStorage::parse(&ole.data).unwrap();
    assert_eq!(storage.items.iter().find(|i| i.path == "CONTENTS").and_then(|i| i.data.clone()).unwrap(), b"first");
    // Saved again unchanged: the same file.
    let opened = ibis_pptx::open(saved.clone()).unwrap();
    assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), saved);
}

#[test]
fn a_word_object_edited_replaces_the_old_one() {
    let opened = ibis_pptx::open(word_with_object()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let para = first_para(document.presentation());
    let (_, mut look) = word_objects(document.presentation()).remove(0);
    let at = FlowPos::new(para, 0);
    let media = document.ids().add_media(MediaFormat::Png, png([0, 0, 255]));
    look.ole = Some(document.ids().add_ole(PROG_ID, object(b"second"), false));
    document
        .execute(vec![Command::Flow(FlowCommand::ReplacePicture { at, media, size: None }), Command::Flow(FlowCommand::SetPictureLook { at, look })])
        .unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let p = ibis_pptx::read(&edited).unwrap();
    let found = word_objects(&p);
    assert_eq!(found.len(), 1);
    let storage = OleStorage::parse(&p.ole(found[0].1.ole.unwrap()).unwrap().data).unwrap();
    assert_eq!(storage.items.iter().find(|i| i.path == "CONTENTS").and_then(|i| i.data.clone()).unwrap(), b"second");
    assert_eq!(p.media(found[0].0).unwrap().data.as_ref(), png([0, 0, 255]).as_slice());
    assert_eq!(parts(&edited, "word/embeddings/").len(), 1, "the old object's part is gone");
}

// ----- Excel ----------------------------------------------------------------

/// A workbook with an object on its first sheet, as Excel 2010 writes one
/// (the `mc:AlternateContent` with the anchored `objectPr`; no VML here).
fn workbook_with_object() -> Vec<u8> {
    let mut package = Package::open(ibis_pptx::blank_file(ibis_pptx::Kind::Workbook).unwrap()).unwrap();
    let sheet = package.names().find(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml")).unwrap().to_string();
    package.set_bytes("xl/embeddings/oleObject1.bin", object(b"first"));
    package.set_override("xl/embeddings/oleObject1.bin", "application/vnd.openxmlformats-officedocument.oleObject").unwrap();
    package.set_bytes("xl/media/image1.png", png([255, 0, 0]));
    package.set_override("xl/media/image1.png", "image/png").unwrap();
    let ole = package.add_relationship(&sheet, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject", "../embeddings/oleObject1.bin", false).unwrap();
    let image = package.add_relationship(&sheet, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image", "../media/image1.png", false).unwrap();
    let mut xml = String::from_utf8(package.read(&sheet).unwrap().to_vec()).unwrap();
    let objects = format!(
        r#"<oleObjects><mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="x14"><oleObject progId="{PROG_ID}" shapeId="1025" r:id="{ole}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><objectPr defaultSize="0" autoPict="0" r:id="{image}"><anchor moveWithCells="1"><from><xdr:col xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">1</xdr:col><xdr:colOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:colOff><xdr:row xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">1</xdr:row><xdr:rowOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:rowOff></from><to><xdr:col xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">3</xdr:col><xdr:colOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:colOff><xdr:row xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">5</xdr:row><xdr:rowOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:rowOff></to></anchor></objectPr></oleObject></mc:Choice><mc:Fallback><oleObject progId="{PROG_ID}" shapeId="1025" r:id="{ole}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></mc:Fallback></mc:AlternateContent></oleObjects>"#
    );
    let end = xml.rfind("</worksheet>").unwrap();
    xml.insert_str(end, &objects);
    package.set_bytes(&sheet, xml.into_bytes());
    package.save(Default::default()).unwrap()
}

fn sheet_objects(p: &Presentation) -> Vec<(ibis_doc_model::SlideId, Shape)> {
    p.slides.iter().flat_map(|s| s.shapes.iter().filter(|sh| matches!(&sh.kind, ShapeKind::Picture(p) if p.ole.is_some())).map(move |sh| (s.id, sh.clone()))).collect()
}

#[test]
fn a_sheets_object_reads_and_saves_edited() {
    let file = workbook_with_object();
    let opened = ibis_pptx::open(file).unwrap();
    let found = sheet_objects(&opened.presentation);
    assert_eq!(found.len(), 1);
    let (slide, shape) = found[0].clone();
    let ShapeKind::Picture(mut picture) = shape.kind.clone() else { unreachable!() };
    assert_eq!(content(&opened.presentation, &picture), b"first");
    assert!(shape.frame.width > 0 && shape.frame.height > 0);

    let mut document = Document::new(opened.presentation.clone());
    picture.media = document.ids().add_media(MediaFormat::Png, png([0, 0, 255]));
    picture.ole = Some(document.ids().add_ole(PROG_ID, object(b"second"), false));
    document.execute(vec![Command::SetKind { slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) }]).unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let p = ibis_pptx::read(&edited).unwrap();
    let found = sheet_objects(&p);
    assert_eq!(found.len(), 1);
    let ShapeKind::Picture(after) = &found[0].1.kind else { unreachable!() };
    assert_eq!(content(&p, after), b"second");
    assert_eq!(p.media(after.media).unwrap().data.as_ref(), png([0, 0, 255]).as_slice());
    assert_eq!(found[0].1.frame, shape.frame);
    assert_eq!(parts(&edited, "xl/media/").len(), 1, "the old picture is gone");

    // Moved (an object with no VML shape): its anchor follows.
    let moved = move_sheet_object(&edited);
    let p = ibis_pptx::read(&moved).unwrap();
    assert_eq!(sheet_objects(&p)[0].1.frame, moved_frame(&p));
}

/// The first sheet's part and XML.
fn sheet_xml(file: &[u8]) -> (String, String) {
    let package = Package::open(file.to_vec()).unwrap();
    let part = package.names().find(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml")).unwrap().to_string();
    let xml = String::from_utf8(package.read(&part).unwrap().to_vec()).unwrap();
    (part, xml)
}

fn vml_xml(file: &[u8]) -> Option<String> {
    let package = Package::open(file.to_vec()).unwrap();
    let part = package.names().find(|n| n.starts_with("xl/drawings/vmlDrawing"))?.to_string();
    Some(String::from_utf8(package.read(&part).unwrap().to_vec()).unwrap())
}

/// A frame on cell boundaries of the first sheet: columns 2–4, rows 3–7.
fn cell_frame(p: &Presentation, columns: (usize, usize), rows: (usize, usize)) -> EmuRect {
    let sheet = p.slides[0].shapes.iter().find_map(|s| match &s.kind { ShapeKind::Sheet(sheet) => Some(sheet), _ => None }).unwrap();
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    EmuRect::new(xs[columns.0], ys[rows.0], xs[columns.1] - xs[columns.0], ys[rows.1] - ys[rows.0])
}

fn moved_frame(p: &Presentation) -> EmuRect {
    cell_frame(p, (4, 6), (2, 9))
}

fn move_sheet_object(file: &[u8]) -> Vec<u8> {
    let opened = ibis_pptx::open(file.to_vec()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let (slide, shape) = sheet_objects(&opened.presentation)[0].clone();
    let frame = moved_frame(&opened.presentation);
    document.execute(vec![Command::SetFrame { slide, shape: shape.id, frame }]).unwrap();
    opened.source.save(document.presentation(), SaveOptions::default()).unwrap()
}

/// A blank workbook with an object inserted on its first sheet.
fn workbook_inserted() -> (Vec<u8>, EmuRect) {
    let opened = ibis_pptx::open(ibis_pptx::blank_file(ibis_pptx::Kind::Workbook).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let slide = document.presentation().slides[0].id;
    let media = document.ids().add_media(MediaFormat::Png, png([255, 0, 0]));
    let ole = document.ids().add_ole(PROG_ID, object(b"first"), false);
    let shape = document.ids().new_shape_id();
    let mut picture = Picture::new(media);
    picture.ole = Some(ole);
    let frame = cell_frame(document.presentation(), (1, 3), (2, 6));
    let index = document.presentation().slides[0].shapes.len();
    document
        .execute(vec![Command::InsertShape { slide, index, shape: Shape { id: shape, frame, rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Picture(picture), floating: None } }])
        .unwrap();
    (opened.source.save(document.presentation(), SaveOptions::default()).unwrap(), frame)
}

#[test]
fn an_object_inserted_on_a_sheet_reads_back() {
    let (saved, frame) = workbook_inserted();
    let p = ibis_pptx::read(&saved).unwrap();
    let found = sheet_objects(&p);
    assert_eq!(found.len(), 1);
    let ShapeKind::Picture(picture) = &found[0].1.kind else { unreachable!() };
    assert_eq!(content(&p, picture), b"first");
    assert_eq!(p.media(picture.media).unwrap().data.as_ref(), png([255, 0, 0]).as_slice());
    assert_eq!(found[0].1.frame, frame);

    let (_, xml) = sheet_xml(&saved);
    assert!(xml.contains("Requires=\"x14\"") && xml.contains("<mc:Fallback>") && xml.contains("progId=\"Example.Drawing.1\""), "{xml}");
    // CT_Worksheet's order: pageMargins … legacyDrawing … oleObjects.
    let at = |name: &str| xml.find(name).unwrap_or_else(|| panic!("{name} in {xml}"));
    assert!(at("<pageMargins") < at("<legacyDrawing") && at("<legacyDrawing") < at("<oleObjects"), "{xml}");
    let shape_id = xml.split("shapeId=\"").nth(1).unwrap().split('"').next().unwrap();
    let vml = vml_xml(&saved).expect("a VML drawing");
    assert!(vml.contains(&format!("id=\"_x0000_s{shape_id}\"")) && vml.contains("ObjectType=\"Pict\"") && vml.contains("<x:Anchor>\r\n   1, 0, 2, 0, 3, 0, 6, 0</x:Anchor>"), "{vml}");

    let package = Package::open(saved.clone()).unwrap();
    assert_eq!(package.content_type("xl/embeddings/oleObject1.bin").unwrap().as_deref(), Some("application/vnd.openxmlformats-officedocument.oleObject"));
    assert_eq!(package.content_type("xl/drawings/vmlDrawing1.vml").unwrap().as_deref(), Some("application/vnd.openxmlformats-officedocument.vmlDrawing"));
    // The VML shape's picture is related from the drawing.
    let vml_rels = package.relationships("xl/drawings/vmlDrawing1.vml").unwrap();
    assert_eq!(vml_rels.len(), 1);
    assert_eq!(vml_rels[0].target_part("xl/drawings/vmlDrawing1.vml").as_deref(), Some("xl/media/image1.png"));

    // Saved again unchanged: the same file.
    let opened = ibis_pptx::open(saved.clone()).unwrap();
    assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), saved);
}

#[test]
fn an_inserted_sheet_object_moves_and_edits() {
    let (saved, _) = workbook_inserted();
    let moved = move_sheet_object(&saved);
    let p = ibis_pptx::read(&moved).unwrap();
    let found = sheet_objects(&p);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.frame, moved_frame(&p));
    let vml = vml_xml(&moved).unwrap();
    assert!(vml.contains("<x:Anchor>\r\n   4, 0, 2, 0, 6, 0, 9, 0</x:Anchor>"), "{vml}");
    assert_eq!(Package::open(saved).unwrap().read("xl/embeddings/oleObject1.bin").unwrap(), Package::open(moved.clone()).unwrap().read("xl/embeddings/oleObject1.bin").unwrap());

    // Edited after: the VML shape shows the new picture, the old one is gone.
    let opened = ibis_pptx::open(moved).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let (slide, shape) = sheet_objects(&opened.presentation)[0].clone();
    let ShapeKind::Picture(mut picture) = shape.kind else { unreachable!() };
    picture.media = document.ids().add_media(MediaFormat::Png, png([0, 0, 255]));
    picture.ole = Some(document.ids().add_ole(PROG_ID, object(b"second"), false));
    document.execute(vec![Command::SetKind { slide, shape: shape.id, kind: Box::new(ShapeKind::Picture(picture)) }]).unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let p = ibis_pptx::read(&edited).unwrap();
    let ShapeKind::Picture(after) = &sheet_objects(&p)[0].1.kind else { unreachable!() };
    assert_eq!(content(&p, after), b"second");
    assert_eq!(parts(&edited, "xl/media/").len(), 1, "the old picture is gone");
    let package = Package::open(edited).unwrap();
    let vml_rels = package.relationships("xl/drawings/vmlDrawing1.vml").unwrap();
    assert_eq!(vml_rels.len(), 1);
    assert_eq!(package.read(&vml_rels[0].target_part("xl/drawings/vmlDrawing1.vml").unwrap()).unwrap(), png([0, 0, 255]));
}

#[test]
fn a_deleted_sheet_object_leaves_nothing_behind() {
    let (saved, _) = workbook_inserted();
    let before = Package::open(ibis_pptx::blank_file(ibis_pptx::Kind::Workbook).unwrap()).unwrap();
    let opened = ibis_pptx::open(saved).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let (slide, shape) = sheet_objects(&opened.presentation)[0].clone();
    document.execute(vec![Command::RemoveShape { slide, shape: shape.id }]).unwrap();
    let deleted = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    assert!(sheet_objects(&ibis_pptx::read(&deleted).unwrap()).is_empty());
    let (part, xml) = sheet_xml(&deleted);
    assert!(!xml.contains("oleObject") && !xml.contains("legacyDrawing"), "{xml}");
    for prefix in ["xl/embeddings/", "xl/media/", "xl/drawings/"] {
        assert!(parts(&deleted, prefix).is_empty(), "{prefix}: {:?}", parts(&deleted, prefix));
    }
    let after = Package::open(deleted).unwrap();
    assert_eq!(after.relationships(&part).unwrap().len(), before.relationships(&part).unwrap().len());
    for name in ["xl/embeddings/oleObject1.bin", "xl/drawings/vmlDrawing1.vml"] {
        assert_eq!(after.content_type(name).unwrap(), before.content_type(name).unwrap(), "{name}");
    }
}

#[test]
fn deleting_an_excel_object_keeps_the_others() {
    // Two objects: Excel 2010's (no VML) and one inserted beside it.
    let file = workbook_with_object();
    let opened = ibis_pptx::open(file).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let slide = document.presentation().slides[0].id;
    let media = document.ids().add_media(MediaFormat::Png, png([0, 255, 0]));
    let ole = document.ids().add_ole(PROG_ID, object(b"second"), false);
    let id = document.ids().new_shape_id();
    let mut picture = Picture::new(media);
    picture.ole = Some(ole);
    let frame = cell_frame(document.presentation(), (5, 7), (1, 4));
    let index = document.presentation().slides[0].shapes.len();
    document.execute(vec![Command::InsertShape { slide, index, shape: Shape { id, frame, rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Picture(picture), floating: None } }]).unwrap();
    let two = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let (_, xml) = sheet_xml(&two);
    assert!(xml.contains("shapeId=\"1025\"") && xml.contains("shapeId=\"1026\""), "{xml}");

    let opened = ibis_pptx::open(two).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let found = sheet_objects(&opened.presentation);
    assert_eq!(found.len(), 2);
    let first = found.iter().find(|(_, s)| s.frame != frame).unwrap().clone();
    document.execute(vec![Command::RemoveShape { slide: first.0, shape: first.1.id }]).unwrap();
    let one = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let p = ibis_pptx::read(&one).unwrap();
    let left = sheet_objects(&p);
    assert_eq!(left.len(), 1);
    let ShapeKind::Picture(picture) = &left[0].1.kind else { unreachable!() };
    assert_eq!(content(&p, picture), b"second");
    assert_eq!(left[0].1.frame, frame);
    assert_eq!(parts(&one, "xl/embeddings/").len(), 1);
    assert_eq!(parts(&one, "xl/media/").len(), 1);
    let vml = vml_xml(&one).unwrap();
    assert!(vml.contains("_x0000_s1026"), "{vml}");
}
