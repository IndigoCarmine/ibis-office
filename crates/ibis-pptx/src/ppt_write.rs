// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An old presentation (`ppt.rs`) written as the `.pptx` the rest of the crate reads: one master (with one layout)
//! for each main master of the file, and the slides with their shapes, pictures and texts. Text formatting the
//! file takes from the master is written out on the text itself.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::Write as _;

use crate::Error;
use crate::ppt::{Cf, Field, Interactive, MasterStyle, PageIr, Pf, PptIr, Props, ShapeIr, SlideShow, TextColor, TextIr, read_ppt};

const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;

/// The default color scheme of a page that has none (the Office one).
const DEFAULT_SCHEME: [u32; 8] = [0xFFFFFF, 0x000000, 0x808080, 0x000000, 0xBBE0E3, 0x333399, 0x00CCCC, 0x3333CC];

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn rels(items: &[(String, String, String)]) -> String {
    // (A target that begins with `ext:` is outside the package.)
    let list: String = items
        .iter()
        .map(|(id, kind, target)| match target.strip_prefix("ext:") {
            Some(url) => format!(r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{}" TargetMode="External"/>"#, esc(url)),
            None => format!(r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{target}"/>"#),
        })
        .collect();
    format!(r#"{HEAD}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{list}</Relationships>"#)
}

fn rel(id: &str, kind: &str, target: &str) -> (String, String, String) {
    (id.to_string(), kind.to_string(), target.to_string())
}

/// What is known while one page is written.
struct Ctx<'a> {
    ir: &'a PptIr,
    scheme: [u32; 8],
    /// The formatting of the page's master (by text type), over the document's defaults.
    styles: &'a HashMap<u16, MasterStyle>,
    /// The pictures this page uses: store index → relationship id.
    pictures: HashMap<usize, String>,
    next_rel: Cell<u32>,
    /// Relationships the page's links need: id, kind and target.
    extra_rels: RefCell<Vec<(String, String, String)>>,
    /// The ids of the slides, with their place in the file (for links to a slide).
    slide_places: &'a HashMap<u32, usize>,
    /// Whether the page being written is a master.
    #[allow(dead_code)]
    master: bool,
}

impl Ctx<'_> {
    fn rgb(&self, c: TextColor) -> u32 {
        match c {
            TextColor::Rgb(v) => v,
            TextColor::Scheme(i) => self.scheme.get(i as usize).copied().unwrap_or(0),
        }
    }

    /// The `a:hlinkClick` for a click that does `i`, if it can be written.
    fn link_xml(&self, i: Interactive) -> Option<String> {
        let rid = |kind: &str, target: String| -> String {
            let mut rels = self.extra_rels.borrow_mut();
            if let Some((id, _, _)) = rels.iter().find(|(_, k, t)| k == kind && *t == target) {
                return id.clone();
            }
            let id = format!("rId{}", self.next_rel.get());
            self.next_rel.set(self.next_rel.get() + 1);
            rels.push((id.clone(), kind.to_string(), target));
            id
        };
        match i.action {
            3 => {
                let jump = match i.jump {
                    1 => "nextslide",
                    2 => "previousslide",
                    3 => "firstslide",
                    4 => "lastslide",
                    6 => "endshow",
                    _ => return None,
                };
                Some(format!(r#"<a:hlinkClick r:id="" action="ppaction://hlinkshowjump?jump={jump}"/>"#))
            }
            4 => {
                let target = self.ir.hyperlinks.get(&i.link_id)?;
                if i.link_type == 7 {
                    // A slide: its identifier is the target's first number.
                    let id: u32 = target.split(',').next()?.trim().parse().ok()?;
                    let place = self.slide_places.get(&id)?;
                    let r = rid("slide", format!("slides/slide{}.xml", place + 1));
                    Some(format!(r#"<a:hlinkClick r:id="{r}" action="ppaction://hlinksldjump"/>"#))
                } else if target.is_empty() {
                    None
                } else {
                    let r = rid("hyperlink", format!("ext:{target}"));
                    Some(format!(r#"<a:hlinkClick r:id="{r}"/>"#))
                }
            }
            _ => None,
        }
    }

    fn color_xml(&self, c: TextColor) -> String {
        format!(r#"<a:solidFill><a:srgbClr val="{:06X}"/></a:solidFill>"#, self.rgb(c))
    }

    fn font(&self, index: Option<u16>) -> Option<&str> {
        let name = self.ir.fonts.get(index? as usize)?.as_str();
        (!name.is_empty()).then_some(name)
    }

    /// The master formatting of a level of a kind of text: the master's over the document's default.
    fn level_style(&self, kind: u32, level: u16) -> (Pf, Cf) {
        // A kind without styles of its own takes those of the kind it is a size of.
        let family = match kind {
            6 => 0,
            5 | 7 | 8 => 1,
            k => k as u16,
        };
        let pick = |map: &HashMap<u16, MasterStyle>| map.get(&family).and_then(|s| s.levels.get(level as usize).cloned());
        let from_master = pick(self.styles);
        let from_default = pick(&self.ir.default_props_styles());
        match (from_master, from_default) {
            (Some((pf, cf)), Some((dpf, dcf))) => (pf.over(&dpf), cf.over(&dcf)),
            (Some(x), None) | (None, Some(x)) => x,
            (None, None) => (Pf::default(), Cf::default()),
        }
    }
}

impl PptIr {
    fn default_props_styles(&self) -> HashMap<u16, MasterStyle> {
        self.defaults.clone()
    }
}

fn units_to_emu(v: i64) -> i64 {
    v * 1587 + v / 2
}

/// A paragraph-properties element (`tag` is `pPr`, or `lvl1pPr` and so on), with a `defRPr` from `cf` when it is given.
fn ppr_xml(ctx: &Ctx, tag: &str, pf: &Pf, cf: Option<&Cf>, auto: Option<(u16, u16)>) -> String {
    let mut attrs = String::new();
    let left = i64::from(pf.left_margin.unwrap_or(0));
    if pf.left_margin.is_some() {
        let _ = write!(attrs, r#" marL="{}""#, units_to_emu(left));
    }
    if let Some(indent) = pf.indent {
        let _ = write!(attrs, r#" indent="{}""#, units_to_emu(i64::from(indent) - left));
    }
    if let Some(align) = pf.align {
        let a = match align {
            1 => "ctr",
            2 => "r",
            3 => "just",
            4 => "dist",
            _ => "l",
        };
        let _ = write!(attrs, r#" algn="{a}""#);
    }
    let mut kids = String::new();
    let spacing = |name: &str, v: i16| -> String {
        if v >= 0 {
            format!(r#"<a:{name}><a:spcPct val="{}"/></a:{name}>"#, i64::from(v) * 1000)
        } else {
            format!(r#"<a:{name}><a:spcPts val="{}"/></a:{name}>"#, (-i64::from(v)) * 100 / 8)
        }
    };
    if let Some(v) = pf.line_spacing {
        kids += &spacing("lnSpc", v);
    }
    if let Some(v) = pf.space_before {
        kids += &spacing("spcBef", v);
    }
    if let Some(v) = pf.space_after {
        kids += &spacing("spcAft", v);
    }
    if pf.has_bullet == Some(true) {
        if let Some(color) = pf.bullet_color {
            kids += &format!("<a:buClr><a:srgbClr val=\"{:06X}\"/></a:buClr>", ctx.rgb(color));
        }
        if let Some(size) = pf.bullet_size {
            if size > 0 {
                let _ = write!(kids, r#"<a:buSzPct val="{}"/>"#, i64::from(size) * 1000);
            } else if size < 0 {
                let _ = write!(kids, r#"<a:buSzPts val="{}"/>"#, i64::from(-size) * 100);
            }
        }
        if let (Some(font), None) = (ctx.font(pf.bullet_font), auto) {
            let _ = write!(kids, r#"<a:buFont typeface="{}"/>"#, esc(font));
        }
        if let Some((scheme, start)) = auto {
            let kind = match scheme {
                0 => "alphaLcPeriod",
                1 => "alphaUcPeriod",
                2 => "arabicParenR",
                4 => "romanLcParenBoth",
                5 => "romanLcParenR",
                6 => "romanLcPeriod",
                7 => "romanUcPeriod",
                8 => "alphaLcParenBoth",
                9 => "alphaLcParenR",
                10 => "alphaUcParenBoth",
                11 => "alphaUcParenR",
                12 => "arabicParenBoth",
                13 => "arabicPlain",
                _ => "arabicPeriod",
            };
            let at = if start > 1 { format!(r#" startAt="{start}""#) } else { String::new() };
            let _ = write!(kids, r#"<a:buFont typeface="+mj-lt"/><a:buAutoNum type="{kind}"{at}/>"#);
        } else {
            let ch = pf.bullet_char.and_then(|c| char::from_u32(u32::from(c))).filter(|c| *c != '\0').unwrap_or('•');
            let _ = write!(kids, r#"<a:buChar char="{}"/>"#, esc(&ch.to_string()));
        }
    } else if pf.has_bullet == Some(false) {
        kids += "<a:buNone/>";
    }
    if let Some(tabs) = &pf.tabs {
        kids += "<a:tabLst>";
        for (position, kind) in tabs {
            let algn = match kind {
                1 => "ctr",
                2 => "r",
                3 => "dec",
                _ => "l",
            };
            let _ = write!(kids, r#"<a:tab pos="{}" algn="{algn}"/>"#, units_to_emu(i64::from(left) + i64::from(*position)));
        }
        kids += "</a:tabLst>";
    }
    if let Some(cf) = cf {
        kids += &rpr_xml(ctx, "defRPr", cf, "");
    }
    format!("<a:{tag}{attrs}>{kids}</a:{tag}>")
}

/// A run-properties element.
fn rpr_xml(ctx: &Ctx, tag: &str, cf: &Cf, extra_attrs: &str) -> String {
    let mut attrs = String::from(r#" lang="ja-JP" altLang="en-US""#);
    if let Some(size) = cf.size {
        let _ = write!(attrs, r#" sz="{}""#, u32::from(size) * 100);
    }
    if let Some(b) = cf.bold {
        let _ = write!(attrs, r#" b="{}""#, u8::from(b));
    }
    if let Some(i) = cf.italic {
        let _ = write!(attrs, r#" i="{}""#, u8::from(i));
    }
    if let Some(u) = cf.underline {
        let _ = write!(attrs, r#" u="{}""#, if u { "sng" } else { "none" });
    }
    if let Some(p) = cf.position {
        if p != 0 {
            let _ = write!(attrs, r#" baseline="{}""#, i32::from(p) * 1000);
        }
    }
    attrs += extra_attrs;
    let mut kids = String::new();
    if let Some(color) = cf.color {
        kids += &ctx.color_xml(color);
    }
    if let Some(font) = ctx.font(cf.ansi_font.or(cf.font)) {
        let _ = write!(kids, r#"<a:latin typeface="{}"/>"#, esc(font));
    }
    if let Some(font) = ctx.font(cf.east_asian_font.or(cf.font)) {
        let _ = write!(kids, r#"<a:ea typeface="{}"/>"#, esc(font));
    }
    if let Some(font) = ctx.font(cf.symbol_font) {
        let _ = write!(kids, r#"<a:sym typeface="{}"/>"#, esc(font));
    }
    if let Some(link) = cf.link.and_then(|l| ctx.link_xml(l)) {
        kids += &link;
    }
    if kids.is_empty() { format!("<a:{tag}{attrs}/>") } else { format!("<a:{tag}{attrs}>{kids}</a:{tag}>") }
}

/// The text of a shape as a `p:txBody` / `a:txBody` content: `a:bodyPr`, `a:lstStyle` and the paragraphs.
fn text_body(ctx: &Ctx, shape: &ShapeIr, text: &TextIr) -> String {
    let p = &shape.props;
    let d = &ctx.ir.default_props;
    let get = |id: u16| p.get(id).or_else(|| d.get(id));
    let inset = |id: u16, default: i64| get(id).map_or(default, |v| i64::from(v as i32));
    let anchor = match get(0x87).unwrap_or(0) {
        1 | 4 => "ctr",
        2 | 5 => "b",
        _ => "t",
    };
    let anchor_ctr = if matches!(get(0x87), Some(3..=5)) { r#" anchorCtr="1""# } else { "" };
    let wrap = if get(0x85) == Some(2) { "none" } else { "square" };
    let vert = match get(0x88).unwrap_or(0) {
        1 | 3 => r#" vert="vert""#,
        2 => r#" vert="vert270""#,
        _ => "",
    };
    let fit = if p.flag(0xBF, 1) == Some(true) { "<a:spAutoFit/>" } else { "" };
    let mut out = format!(
        r#"<a:bodyPr wrap="{wrap}" lIns="{}" tIns="{}" rIns="{}" bIns="{}" rtlCol="0" anchor="{anchor}"{anchor_ctr}{vert}>{fit}</a:bodyPr><a:lstStyle/>"#,
        inset(0x81, 91440),
        inset(0x82, 45720),
        inset(0x83, 91440),
        inset(0x84, 45720),
    );
    if text.paragraphs.is_empty() {
        out += "<a:p><a:endParaRPr lang=\"ja-JP\" altLang=\"en-US\"/></a:p>";
    }
    for para in &text.paragraphs {
        let (mpf, mcf) = ctx.level_style(text.kind, para.level);
        let pf = para.pf.over(&mpf);
        let mut ppr_attrs = String::new();
        if para.level > 0 {
            let _ = write!(ppr_attrs, r#" lvl="{}""#, para.level);
        }
        let ppr = ppr_xml(ctx, "pPr", &pf, None, para.auto_number);
        let ppr = if ppr_attrs.is_empty() { ppr } else { ppr.replacen("<a:pPr", &format!("<a:pPr{ppr_attrs}"), 1) };
        out += "<a:p>";
        out += &ppr;
        let mut last_cf = mcf.clone();
        for (run, cf, field) in &para.runs {
            let eff = cf.over(&mcf);
            last_cf = eff.clone();
            match field {
                Some(Field::SlideNumber) => {
                    let _ = write!(out, r#"<a:fld id="{{B6F15528-21DE-4FAA-801E-634DDDAF4B2B}}" type="slidenum">{}<a:t>‹#›</a:t></a:fld>"#, rpr_xml(ctx, "rPr", &eff, ""));
                }
                Some(Field::DateTime) => {
                    let _ = write!(out, r#"<a:fld id="{{B6F15528-21DE-4FAA-801E-634DDDAF4B2C}}" type="datetime1">{}<a:t>{}</a:t></a:fld>"#, rpr_xml(ctx, "rPr", &eff, ""), esc(run));
                }
                None => {
                    // A vertical tab is a line break.
                    for (i, piece) in run.split('\u{B}').enumerate() {
                        if i > 0 {
                            let _ = write!(out, "<a:br>{}</a:br>", rpr_xml(ctx, "rPr", &eff, ""));
                        }
                        if !piece.is_empty() {
                            let _ = write!(out, "<a:r>{}<a:t>{}</a:t></a:r>", rpr_xml(ctx, "rPr", &eff, ""), esc(piece));
                        }
                    }
                }
            }
        }
        out += &rpr_xml(ctx, "endParaRPr", &last_cf, "");
        out += "</a:p>";
    }
    out
}

/// The preset geometry of a shape type (`MSOSPT`).
pub(crate) fn preset(spt: u16) -> Option<&'static str> {
    Some(match spt {
        1 | 202 => "rect",
        2 => "roundRect",
        3 => "ellipse",
        4 => "diamond",
        5 => "triangle",
        6 => "rtTriangle",
        7 => "parallelogram",
        8 => "trapezoid",
        9 => "hexagon",
        10 => "octagon",
        11 => "plus",
        12 => "star5",
        13 | 14 => "rightArrow",
        15 => "homePlate",
        16 => "cube",
        17 => "wedgeRoundRectCallout",
        18 => "star16",
        19 => "arc",
        20 => "line",
        21 => "plaque",
        22 => "can",
        23 => "donut",
        32 => "straightConnector1",
        33 => "bentConnector2",
        34 => "bentConnector3",
        35 => "bentConnector4",
        36 => "bentConnector5",
        37 => "curvedConnector2",
        38 => "curvedConnector3",
        39 => "curvedConnector4",
        40 => "curvedConnector5",
        41 => "callout1",
        42 => "callout2",
        43 => "callout3",
        44 => "accentCallout1",
        45 => "accentCallout2",
        46 => "accentCallout3",
        47 => "borderCallout1",
        48 => "borderCallout2",
        49 => "borderCallout3",
        50 => "accentBorderCallout1",
        51 => "accentBorderCallout2",
        52 => "accentBorderCallout3",
        53 => "ribbon2",
        54 => "ribbon",
        55 => "chevron",
        56 => "pentagon",
        57 => "noSmoking",
        58 => "star8",
        59 => "star16",
        60 => "star32",
        61 => "wedgeRectCallout",
        62 => "wedgeRoundRectCallout",
        63 => "wedgeEllipseCallout",
        64 => "wave",
        65 => "foldedCorner",
        66 => "leftArrow",
        67 => "downArrow",
        68 => "upArrow",
        69 => "leftRightArrow",
        70 => "upDownArrow",
        71 => "irregularSeal1",
        72 => "irregularSeal2",
        73 => "lightningBolt",
        74 => "heart",
        75 => "rect",
        76 => "quadArrow",
        77 => "leftArrowCallout",
        78 => "rightArrowCallout",
        79 => "upArrowCallout",
        80 => "downArrowCallout",
        81 => "leftRightArrowCallout",
        82 => "upDownArrowCallout",
        83 => "quadArrowCallout",
        84 => "bevel",
        85 => "leftBracket",
        86 => "rightBracket",
        87 => "leftBrace",
        88 => "rightBrace",
        89 => "leftUpArrow",
        90 => "bentUpArrow",
        91 => "bentArrow",
        92 => "star24",
        93 => "stripedRightArrow",
        94 => "notchedRightArrow",
        95 => "blockArc",
        96 => "smileyFace",
        97 => "verticalScroll",
        98 => "horizontalScroll",
        99 | 100 => "circularArrow",
        101 => "uturnArrow",
        102 => "curvedRightArrow",
        103 => "curvedLeftArrow",
        104 => "curvedUpArrow",
        105 => "curvedDownArrow",
        106 => "cloudCallout",
        107 => "ellipseRibbon2",
        108 => "ellipseRibbon",
        109 => "flowChartProcess",
        110 => "flowChartDecision",
        111 => "flowChartInputOutput",
        112 => "flowChartPredefinedProcess",
        113 => "flowChartInternalStorage",
        114 => "flowChartDocument",
        115 => "flowChartMultidocument",
        116 => "flowChartTerminator",
        117 => "flowChartPreparation",
        118 => "flowChartManualInput",
        119 => "flowChartManualOperation",
        120 => "flowChartConnector",
        121 => "flowChartPunchedCard",
        122 => "flowChartPunchedTape",
        123 => "flowChartSummingJunction",
        124 => "flowChartOr",
        125 => "flowChartCollate",
        126 => "flowChartSort",
        127 => "flowChartExtract",
        128 => "flowChartMerge",
        129 => "flowChartOfflineStorage",
        130 => "flowChartOnlineStorage",
        131 => "flowChartMagneticTape",
        132 => "flowChartMagneticDisk",
        133 => "flowChartMagneticDrum",
        134 => "flowChartDisplay",
        135 => "flowChartDelay",
        _ => return None,
    })
}

/// The placeholder `type` (and `idx`) a placement id (`PlaceholderEnum`) is.
fn placeholder_kind(id: u8) -> Option<(&'static str, Option<u32>)> {
    Some(match id {
        0x01 | 0x0D | 0x11 => ("title", None),
        0x02 | 0x0E | 0x12 | 0x0C | 0x06 => ("body", Some(1)),
        0x03 | 0x0F => ("ctrTitle", None),
        0x04 | 0x10 => ("subTitle", Some(1)),
        0x05 | 0x0B => ("sldImg", None),
        0x07 => ("dt", Some(10)),
        0x08 => ("sldNum", Some(12)),
        0x09 => ("ftr", Some(11)),
        0x0A => ("hdr", None),
        0x13..=0x19 => ("obj", Some(1)),
        _ => return None,
    })
}

/// An `IMsoArray` of the property `id` as its elements (each `cb` bytes), or `None`.
fn ms_array(props: &Props, id: u16) -> Option<(usize, Vec<&[u8]>)> {
    let d = props.complex(id)?;
    let n = u16::from_le_bytes([*d.first()?, *d.get(1)?]) as usize;
    let cb = u16::from_le_bytes([*d.get(4)?, *d.get(5)?]);
    let size = if cb == 0xFFF0 { 4 } else { cb as usize };
    if size == 0 {
        return None;
    }
    let items: Vec<&[u8]> = (0..n).filter_map(|i| d.get(6 + i * size..6 + (i + 1) * size)).collect();
    Some((size, items))
}

/// A freeform's geometry (`a:custGeom`) from the vertices and segments of its properties (MS-ODRAW 2.3.6).
fn custom_geometry(shape: &ShapeIr) -> Option<String> {
    let p = &shape.props;
    let (size, items) = ms_array(p, 0x145)?;
    let points: Vec<(i64, i64)> = items
        .iter()
        .map(|b| {
            if size >= 8 {
                let v = |at: usize| i64::from(i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]));
                (v(0), v(4))
            } else {
                (i64::from(i16::from_le_bytes([b[0], b[1]])), i64::from(i16::from_le_bytes([b[2], b[3]])))
            }
        })
        .map(|(x, y)| (if (0x8000_0000u32 as i64..=0x8000_007Fu32 as i64).contains(&(x & 0xFFFF_FFFF)) { 0 } else { x }, if (0x8000_0000u32 as i64..=0x8000_007Fu32 as i64).contains(&(y & 0xFFFF_FFFF)) { 0 } else { y }))
        .collect();
    if points.is_empty() {
        return None;
    }
    // The space of the points: the geometry rectangle, else 21600 square as the file's default.
    let mut left = p.int(0x140).map_or(0, i64::from);
    let mut top = p.int(0x141).map_or(0, i64::from);
    let mut right = p.int(0x142).map_or(21600, i64::from);
    let mut bottom = p.int(0x143).map_or(21600, i64::from);
    if right <= left || bottom <= top {
        // (A geometry rectangle that is empty: the vertices' own.)
        left = points.iter().map(|q| q.0).min().unwrap_or(0);
        right = points.iter().map(|q| q.0).max().unwrap_or(1);
        top = points.iter().map(|q| q.1).min().unwrap_or(0);
        bottom = points.iter().map(|q| q.1).max().unwrap_or(1);
    }
    let (w, h) = ((right - left).max(1), (bottom - top).max(1));
    let pt = |(x, y): (i64, i64)| format!(r#"<a:pt x="{}" y="{}"/>"#, x - left, y - top);
    let mut path = String::new();
    let mut at = 0usize;
    let take = |at: &mut usize, n: usize| -> Vec<(i64, i64)> {
        let out = points.get(*at..(*at + n).min(points.len())).unwrap_or(&[]).to_vec();
        *at += n;
        out
    };
    match ms_array(p, 0x146) {
        Some((_, segments)) if !segments.is_empty() => {
            for seg in segments {
                let v = u16::from_le_bytes([seg[0], *seg.get(1).unwrap_or(&0)]);
                let (kind, count) = (v >> 13, (v & 0x1FFF) as usize);
                match kind {
                    0 => {
                        for q in take(&mut at, count) {
                            let _ = write!(path, "<a:lnTo>{}</a:lnTo>", pt(q));
                        }
                    }
                    1 => {
                        for _ in 0..count {
                            let c = take(&mut at, 3);
                            if c.len() == 3 {
                                let _ = write!(path, "<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(c[0]), pt(c[1]), pt(c[2]));
                            }
                        }
                    }
                    2 => {
                        if let Some(q) = take(&mut at, 1).first() {
                            let _ = write!(path, "<a:moveTo>{}</a:moveTo>", pt(*q));
                        }
                    }
                    3 => path += "<a:close/>",
                    5 | 6 => {
                        // An escape: its points are skipped (the arcs and ellipses are not drawn).
                        at += (v & 0xFF) as usize;
                    }
                    _ => {}
                }
            }
        }
        _ => {
            let shape_path = p.get(0x144).unwrap_or(1);
            let all = take(&mut at, points.len());
            if let Some(first) = all.first() {
                let _ = write!(path, "<a:moveTo>{}</a:moveTo>", pt(*first));
                if shape_path == 2 || shape_path == 3 {
                    for c in all[1..].chunks_exact(3) {
                        let _ = write!(path, "<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(c[0]), pt(c[1]), pt(c[2]));
                    }
                } else {
                    for q in &all[1..] {
                        let _ = write!(path, "<a:lnTo>{}</a:lnTo>", pt(*q));
                    }
                }
                if shape_path == 1 || shape_path == 3 {
                    path += "<a:close/>";
                }
            }
        }
    }
    Some(format!(r#"<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="r" b="b"/><a:pathLst><a:path w="{w}" h="{h}">{path}</a:path></a:pathLst></a:custGeom>"#))
}

/// The fill of a shape as `spPr` content.
fn fill_xml(ctx: &mut Ctx, shape: &ShapeIr, has_text_or_placeholder: bool) -> String {
    let d = &ctx.ir.default_props;
    let p = &shape.props;
    let get = |id: u16| p.get(id).or_else(|| d.get(id));
    let filled = p.flag(0x1BF, 4).or_else(|| d.flag(0x1BF, 4)).unwrap_or(!has_text_or_placeholder);
    if !filled {
        return "<a:noFill/>".into();
    }
    let color = |v: u32| -> u32 { ctx.rgb(crate::ppt::art_color_of(v)) };
    let fill_type = get(0x180).unwrap_or(0);
    let alpha = get(0x182).map(|v| (f64::from(v) / 65536.0 * 100_000.0).round() as u32).filter(|a| *a < 100_000);
    let alpha_xml = alpha.map(|a| format!(r#"<a:alpha val="{a}"/>"#)).unwrap_or_default();
    match fill_type {
        // A picture fill.
        2 | 3 => {
            if let Some(index) = p.get(0x186).map(|v| v as usize).filter(|i| *i > 0) {
                if let Some(rid) = ctx.picture(index) {
                    return format!(r#"<a:blipFill rotWithShape="1"><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>"#);
                }
            }
            "<a:noFill/>".into()
        }
        // A gradient.
        4..=8 => {
            let first = color(get(0x181).unwrap_or(0xFFFFFF));
            let second = color(get(0x183).unwrap_or(0xFFFFFF));
            let angle = get(0x18B).map_or(0.0, |v| f64::from(v as i32) / 65536.0);
            // The file's angle is of the gradient on the page: with the shape turned it is the preset's angle (clockwise from the
            // right) taken from a right angle, less the shape's turn.
            let turn = shape.props.get(0x04).map_or(0.0, |v| f64::from(v as i32) / 65536.0);
            let ooxml = ((90.0 - angle - turn).rem_euclid(360.0) * 60000.0).round() as i64;
            format!(
                r#"<a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:srgbClr val="{first:06X}">{alpha_xml}</a:srgbClr></a:gs><a:gs pos="100000"><a:srgbClr val="{second:06X}">{alpha_xml}</a:srgbClr></a:gs></a:gsLst><a:lin ang="{ooxml}" scaled="0"/></a:gradFill>"#
            )
        }
        _ => {
            let c = color(get(0x181).unwrap_or(0xFFFFFF));
            format!(r#"<a:solidFill><a:srgbClr val="{c:06X}">{alpha_xml}</a:srgbClr></a:solidFill>"#)
        }
    }
}

/// The shadow of a shape (`a:effectLst`), when it has one (MS-ODRAW 2.3.13).
fn shadow_xml(ctx: &Ctx, shape: &ShapeIr) -> String {
    let d = &ctx.ir.default_props;
    let p = &shape.props;
    let get = |id: u16| p.get(id).or_else(|| d.get(id));
    if !p.flag(0x23F, 1).or_else(|| d.flag(0x23F, 1)).unwrap_or(false) {
        return String::new();
    }
    let (dx, dy) = (get(0x205).map_or(25400.0, |v| f64::from(v as i32)), get(0x206).map_or(25400.0, |v| f64::from(v as i32)));
    let dist = (dx * dx + dy * dy).sqrt().round() as i64;
    let dir = (dy.atan2(dx).to_degrees().rem_euclid(360.0) * 60000.0).round() as i64;
    let color = ctx.rgb(crate::ppt::art_color_of(get(0x201).unwrap_or(0x808080)));
    let alpha = get(0x204).map_or(100_000, |v| (f64::from(v) / 65536.0 * 100_000.0).round() as i64).clamp(0, 100_000);
    format!(r#"<a:effectLst><a:outerShdw blurRad="0" dist="{dist}" dir="{dir}" algn="tl" rotWithShape="0"><a:srgbClr val="{color:06X}"><a:alpha val="{alpha}"/></a:srgbClr></a:outerShdw></a:effectLst>"#)
}

/// The outline of a shape.
fn line_xml(ctx: &Ctx, shape: &ShapeIr, default_on: bool) -> String {
    let d = &ctx.ir.default_props;
    let p = &shape.props;
    let get = |id: u16| p.get(id).or_else(|| d.get(id));
    let on = p.flag(0x1FF, 3).or_else(|| d.flag(0x1FF, 3)).unwrap_or(default_on);
    if !on {
        return "<a:ln><a:noFill/></a:ln>".into();
    }
    let width = get(0x1CB).map_or(9525, |v| i64::from(v as i32));
    let color = ctx.rgb(crate::ppt::art_color_of(get(0x1C0).unwrap_or(0)));
    let dash = match get(0x1CE).unwrap_or(0) {
        1 => "sysDash",
        2 => "sysDot",
        3 => "dashDot",
        4 => "lgDashDotDot",
        5 => "dash",
        6 => "lgDash",
        7 => "lgDashDot",
        8 => "lgDashDotDot",
        _ => "solid",
    };
    let arrow = |id: u16, tag: &str| -> String {
        let kind = match get(id).unwrap_or(0) {
            1 => "triangle",
            2 => "stealth",
            3 => "diamond",
            4 => "oval",
            5 => "arrow",
            _ => return String::new(),
        };
        format!(r#"<a:{tag} type="{kind}"/>"#)
    };
    format!(
        r#"<a:ln w="{width}"><a:solidFill><a:srgbClr val="{color:06X}"/></a:solidFill><a:prstDash val="{dash}"/>{}{}</a:ln>"#,
        arrow(0x1D0, "headEnd"),
        arrow(0x1D1, "tailEnd")
    )
}

impl Ctx<'_> {
    /// The relationship id of the picture `index` (1-based in the store), adding it for this page.
    fn picture(&mut self, index: usize) -> Option<String> {
        self.ir.blips.get(index - 1)?.as_ref()?;
        if let Some(r) = self.pictures.get(&index) {
            return Some(r.clone());
        }
        let id = format!("rId{}", self.next_rel.get());
        self.next_rel.set(self.next_rel.get() + 1);
        self.pictures.insert(index, id.clone());
        Some(id)
    }
}

/// A shape as XML. `space` is the (offset, extent) it is in when it is a child of a group, else `None`.
fn shape_xml(ctx: &mut Ctx, shape: &ShapeIr, page: &PageIr, outline: &[TextIr], depth: usize) -> String {
    let id = shape.id.max(2);
    let name = if shape.name.is_empty() { format!("Shape {id}") } else { esc(&shape.name) };
    let Some((x, y, w, h)) = shape.frame else {
        return String::new();
    };
    let rotation = shape.props.get(0x04).map_or(0, |v| ((f64::from(v as i32) / 65536.0) * 60000.0).round() as i64);
    let mut xfrm_attrs = String::new();
    if rotation != 0 {
        let _ = write!(xfrm_attrs, r#" rot="{}""#, rotation.rem_euclid(21_600_000));
    }
    if shape.flip_h {
        xfrm_attrs += r#" flipH="1""#;
    }
    if shape.flip_v {
        xfrm_attrs += r#" flipV="1""#;
    }
    if shape.is_group {
        let (cx, cy, cw, ch) = shape.child_space.unwrap_or((x, y, w, h));
        let kids: String = shape.children.iter().map(|c| shape_xml(ctx, c, page, outline, depth + 1)).collect();
        return format!(
            r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm{xfrm_attrs}><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/><a:chOff x="{cx}" y="{cy}"/><a:chExt cx="{cw}" cy="{ch}"/></a:xfrm></p:grpSpPr>{kids}</p:grpSp>"#
        );
    }
    let xfrm = format!(r#"<a:xfrm{xfrm_attrs}><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm>"#);
    // A picture: a picture frame with a picture, or any shape with only a picture.
    let picture_index = shape.props.get(0x104).map(|v| v as usize).filter(|i| *i > 0);
    if let (Some(index), true) = (picture_index, shape.spt == 75 || shape.props.get(0x180).is_none()) {
        if shape.text.is_none() && shape.placeholder.is_none() {
            if let Some(rid) = ctx.picture(index) {
                let line = line_xml(ctx, shape, false);
                // Cropped: the fractions of the picture left out at each side (16.16 fixed point).
                let crop = |id: u16| shape.props.int(id).map_or(0, |v| (f64::from(v) / 65536.0 * 100_000.0).round() as i64);
                let src_rect = if (0x100..=0x103).any(|id| crop(id) != 0) { format!(r#"<a:srcRect l="{}" t="{}" r="{}" b="{}"/>"#, crop(0x102), crop(0x100), crop(0x103), crop(0x101)) } else { String::new() };
                return format!(
                    r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="{name}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{rid}"/>{src_rect}<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{xfrm}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{line}</p:spPr></p:pic>"#
                );
            }
        }
    }
    // Its text: its own, or what the slide list holds for it.
    let text: Option<TextIr> = match (&shape.text, shape.outline_ref) {
        (Some(t), _) => Some(t.clone()),
        (None, Some(i)) => outline.get(i).cloned(),
        _ => None,
    };
    let ph = shape.placeholder.and_then(placeholder_kind);
    let is_line = matches!(shape.spt, 20) || shape.is_connector || (32..=40).contains(&shape.spt);
    let prst = preset(shape.spt).unwrap_or("rect");
    let mut av = String::new();
    if shape.spt == 2 {
        if let Some(adj) = shape.props.int(0x147) {
            // The corner radius: the file's 21600ths of the shorter side, as the preset's 100000ths.
            let _ = write!(av, r#"<a:gd name="adj" fmla="val {}"/>"#, (i64::from(adj) * 100_000 * 2 / 3 / 21_600 * 3 / 2).clamp(0, 50_000) * 2 / 3 * 3 / 2);
        }
    }
    let geometry = custom_geometry(shape).unwrap_or_else(|| format!(r#"<a:prstGeom prst="{prst}"><a:avLst>{av}</a:avLst></a:prstGeom>"#));
    let textual = text.is_some() || ph.is_some();
    let fill = if is_line { String::new() } else { fill_xml(ctx, shape, textual) };
    let line = line_xml(ctx, shape, !textual || is_line);
    let sp_pr = format!("<p:spPr>{xfrm}{geometry}{fill}{line}{}</p:spPr>", shadow_xml(ctx, shape));
    if is_line {
        return format!(r#"<p:cxnSp><p:nvCxnSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr>{sp_pr}</p:cxnSp>"#);
    }
    let nv_pr = match ph {
        Some((kind, Some(idx))) => format!(r#"<p:ph type="{kind}" idx="{idx}"/>"#),
        Some((kind, None)) => format!(r#"<p:ph type="{kind}"/>"#),
        None => String::new(),
    };
    let cnv_sp_pr = if ph.is_some() {
        r#"<p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr>"#.to_string()
    } else if shape.spt == 202 {
        r#"<p:cNvSpPr txBox="1"/>"#.to_string()
    } else {
        "<p:cNvSpPr/>".to_string()
    };
    let body = match &text {
        Some(t) => format!("<p:txBody>{}</p:txBody>", text_body(ctx, shape, t)),
        None => String::new(),
    };
    let link = shape.interactive.and_then(|i| ctx.link_xml(i)).unwrap_or_default();
    let c_nv_pr = if link.is_empty() { format!(r#"<p:cNvPr id="{id}" name="{name}"/>"#) } else { format!(r#"<p:cNvPr id="{id}" name="{name}">{link}</p:cNvPr>"#) };
    format!(r#"<p:sp><p:nvSpPr>{c_nv_pr}{cnv_sp_pr}<p:nvPr>{nv_pr}</p:nvPr></p:nvSpPr>{sp_pr}{body}</p:sp>"#)
}

/// The transition of a slide (`p:transition`) from its show information.
fn transition_xml(show: &SlideShow) -> String {
    let dir8 = |d: u8| ["l", "u", "r", "d", "lu", "ru", "ld", "rd"].get(d as usize).copied().unwrap_or("l");
    let dir4 = |d: u8| ["l", "u", "r", "d"].get(d as usize).copied().unwrap_or("l");
    let orient = |d: u8| if d == 0 { "horz" } else { "vert" };
    let effect = match show.effect {
        // (No transition at all is a cut that is not through black: nothing to write unless the timing says something.)
        0 if show.direction != 1 && show.auto_advance.is_none() && show.manual_advance => return String::new(),
        0 => format!("<p:cut{}/>", if show.direction == 1 { r#" thruBlk="1""# } else { "" }),
        1 => "<p:random/>".to_string(),
        2 => format!(r#"<p:blinds dir="{}"/>"#, if show.direction == 0 { "vert" } else { "horz" }),
        3 => format!(r#"<p:checker dir="{}"/>"#, orient(show.direction)),
        4 => format!(r#"<p:cover dir="{}"/>"#, dir8(show.direction)),
        5 => "<p:dissolve/>".to_string(),
        6 | 23 => format!("<p:fade{}/>", if show.direction == 1 { r#" thruBlk="1""# } else { "" }),
        7 => format!(r#"<p:pull dir="{}"/>"#, dir8(show.direction)),
        8 => format!(r#"<p:randomBar dir="{}"/>"#, orient(show.direction)),
        10 => format!(r#"<p:wipe dir="{}"/>"#, dir4(show.direction)),
        11 => format!(r#"<p:zoom dir="{}"/>"#, if show.direction == 0 { "out" } else { "in" }),
        13 => format!(r#"<p:split orient="{}" dir="{}"/>"#, if show.direction < 2 { "horz" } else { "vert" }, if show.direction % 2 == 0 { "out" } else { "in" }),
        17 => "<p:diamond/>".to_string(),
        18 => "<p:plus/>".to_string(),
        19 => "<p:wedge/>".to_string(),
        20 => format!(r#"<p:push dir="{}"/>"#, dir4(show.direction)),
        21 => format!(r#"<p:comb dir="{}"/>"#, orient(show.direction)),
        22 => "<p:newsflash/>".to_string(),
        26 => format!(r#"<p:wheel spokes="{}"/>"#, show.direction.max(1)),
        27 => "<p:circle/>".to_string(),
        _ => return String::new(),
    };
    let mut attrs = String::from(match show.speed {
        0 => r#" spd="slow""#,
        2 => r#" spd="fast""#,
        _ => r#" spd="med""#,
    });
    if !show.manual_advance {
        attrs += r#" advClick="0""#;
    }
    if let Some(ms) = show.auto_advance {
        let _ = write!(attrs, r#" advTm="{ms}""#);
    }
    format!("<p:transition{attrs}>{effect}</p:transition>")
}

/// A page's background as `p:bg`.
fn background_xml(ctx: &mut Ctx, page: &PageIr) -> String {
    let Some(bg) = &page.background else { return String::new() };
    let fill = fill_xml(ctx, bg, false);
    format!("<p:bg><p:bgPr>{fill}<a:effectLst/></p:bgPr></p:bg>")
}

fn tree(shapes: &str) -> String {
    format!(
        r#"<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree>"#
    )
}

/// The theme of a page: Office's, with the page's color scheme.
fn theme_of(scheme: &[u32; 8]) -> String {
    let mut t = crate::new_file::theme();
    let hex = |v: u32| format!("{v:06X}");
    t = t.replacen(
        r#"<a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>"#,
        &format!(r#"<a:dk1><a:srgbClr val="{}"/></a:dk1>"#, hex(scheme[1])),
        1,
    );
    t = t.replacen(
        r#"<a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>"#,
        &format!(r#"<a:lt1><a:srgbClr val="{}"/></a:lt1>"#, hex(scheme[0])),
        1,
    );
    for (name, old, new) in [("dk2", "44546A", scheme[3]), ("lt2", "E7E6E6", scheme[4]), ("accent1", "4472C4", scheme[5]), ("accent2", "ED7D31", scheme[6]), ("accent3", "A5A5A5", scheme[7])] {
        t = t.replacen(&format!(r#"<a:{name}><a:srgbClr val="{old}"/></a:{name}>"#), &format!(r#"<a:{name}><a:srgbClr val="{}"/></a:{name}>"#, hex(new)), 1);
    }
    t
}

/// A master's `p:txStyles` from its text styles (title, body and other text).
fn tx_styles(ctx: &Ctx) -> String {
    let style = |tag: &str, kind: u32| -> String {
        let levels: String = (0..5)
            .map(|level| {
                let (pf, cf) = ctx.level_style(kind, level);
                ppr_xml(ctx, &format!("lvl{}pPr", level + 1), &pf, Some(&cf), None)
            })
            .collect();
        format!("<p:{tag}>{levels}</p:{tag}>")
    };
    format!("<p:txStyles>{}{}{}</p:txStyles>", style("titleStyle", 0), style("bodyStyle", 1), style("otherStyle", 4))
}

/// The `.pptx` made from the old presentation in `data`.
pub fn to_pptx(data: &[u8]) -> Result<Vec<u8>, Error> {
    to_pptx_of(read_ppt(data)?)
}

/// The `.pptx` made from a presentation read.
pub(crate) fn to_pptx_of(ir: crate::ppt::PptIr) -> Result<Vec<u8>, Error> {
    let (width, height) = ir.size;
    let slide_places: HashMap<u32, usize> = ir.slides.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut content_overrides = String::new();
    let mut extensions: Vec<&'static str> = Vec::new();
    let mut master_rels: Vec<(String, String, String)> = Vec::new();
    let mut pres_rels: Vec<(String, String, String)> = Vec::new();
    let mut master_ids = String::new();
    let mut slide_ids = String::new();
    // Pictures the pages use, by store index: written once as media.
    let mut media: HashMap<usize, String> = HashMap::new();

    let masters: Vec<&PageIr> = if ir.masters.is_empty() { Vec::new() } else { ir.masters.iter().collect() };
    let page_media = |ctx: &Ctx, media: &mut HashMap<usize, String>, files: &mut Vec<(String, Vec<u8>)>, extensions: &mut Vec<&'static str>| -> Vec<(String, String, String)> {
        let mut rels_out = Vec::new();
        let mut used: Vec<(&usize, &String)> = ctx.pictures.iter().collect();
        used.sort();
        for (index, rid) in used {
            let Some(Some(blip)) = ir.blips.get(index - 1) else { continue };
            let name = media.entry(*index).or_insert_with(|| {
                let name = format!("image{}.{}", index, blip.ext);
                files.push((format!("ppt/media/{name}"), blip.data.clone()));
                if !extensions.contains(&blip.ext) {
                    extensions.push(blip.ext);
                }
                name
            });
            rels_out.push((rid.clone(), "image".to_string(), format!("../media/{name}")));
        }
        rels_out
    };
    // The masters: master i has theme i and layout i.
    let mut master_index_of: HashMap<u32, usize> = HashMap::new();
    for (i, m) in masters.iter().enumerate() {
        let n = i + 1;
        master_index_of.insert(m.id, i);
        let scheme = m.scheme.unwrap_or(DEFAULT_SCHEME);
        let mut ctx = Ctx { ir: &ir, scheme, styles: &m.styles, pictures: HashMap::new(), next_rel: Cell::new(3), extra_rels: RefCell::new(Vec::new()), slide_places: &slide_places, master: true };
        // The master's shapes: placeholders (as the master's) and the others.
        let mut shapes = String::new();
        let mut layout_shapes = String::new();
        let title_master = m.geom == 2;
        for s in &m.shapes {
            let mut shape = s.clone();
            // The master's placeholder kinds are the master's own; the layout of a title master has the center ones.
            let kind = shape.placeholder;
            if let Some(k) = kind {
                // A master has title, body, date, footer and number placeholders.
                let master_kind = match k {
                    0x03 => 0x01,
                    0x04 => 0x02,
                    other => other,
                };
                if title_master && matches!(k, 0x03 | 0x04) {
                    // (The title master's center title and subtitle are the layout's.)
                    let layout_kind = k;
                    let mut ls = shape.clone();
                    ls.placeholder = Some(layout_kind);
                    layout_shapes += &shape_xml(&mut ctx, &ls, m, &[], 0);
                }
                shape.placeholder = Some(master_kind);
            }
            shapes += &shape_xml(&mut ctx, &shape, m, &[], 0);
        }
        // The layout: placeholders of the master's kinds without a place (they take the master's), or the center title and subtitle.
        if !title_master {
            layout_shapes = String::from(
                r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="ja-JP" altLang="en-US"/></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Content"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="ja-JP" altLang="en-US"/></a:p></p:txBody></p:sp>"#,
            );
        }
        let bg = background_xml(&mut ctx, m);
        let master_xml = format!(
            r#"{HEAD}<p:sldMaster xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld>{bg}{}</p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="{}" r:id="rId1"/></p:sldLayoutIdLst>{}</p:sldMaster>"#,
            tree(&shapes),
            2_147_483_649u64 + (i as u64) * 2,
            tx_styles(&ctx)
        );
        let kind = if title_master { "title" } else { "obj" };
        let layout_xml = format!(
            r#"{HEAD}<p:sldLayout xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" type="{kind}" preserve="1"><p:cSld name="{}">{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
            if title_master { "Title Slide" } else { "Title and Content" },
            tree(&layout_shapes)
        );
        let mut rels_of_master = vec![rel("rId1", "slideLayout", &format!("../slideLayouts/slideLayout{n}.xml")), rel("rId2", "theme", &format!("../theme/theme{n}.xml"))];
        rels_of_master.extend(page_media(&ctx, &mut media, &mut files, &mut extensions));
        rels_of_master.extend(ctx.extra_rels.borrow().iter().cloned());
        files.push((format!("ppt/slideMasters/slideMaster{n}.xml"), master_xml.into_bytes()));
        files.push((format!("ppt/slideMasters/_rels/slideMaster{n}.xml.rels"), rels(&rels_of_master).into_bytes()));
        files.push((format!("ppt/slideLayouts/slideLayout{n}.xml"), layout_xml.into_bytes()));
        files.push((format!("ppt/slideLayouts/_rels/slideLayout{n}.xml.rels"), rels(&[rel("rId1", "slideMaster", &format!("../slideMasters/slideMaster{n}.xml"))]).into_bytes()));
        files.push((format!("ppt/theme/theme{n}.xml"), theme_of(&scheme).into_bytes()));
        let _ = write!(content_overrides, r#"<Override PartName="/ppt/slideMasters/slideMaster{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#);
        master_rels.push(rel(&format!("rId{n}"), "slideMaster", &format!("slideMasters/slideMaster{n}.xml")));
        let _ = write!(master_ids, r#"<p:sldMasterId id="{}" r:id="rId{n}"/>"#, 2_147_483_648u64 + (i as u64) * 2);
        // (Layout ids follow the master ids: they must all differ.)
    }
    // A file without a master gets the blank new-presentation's.
    if masters.is_empty() {
        return Err(Error::new("旧形式（.ppt）のプレゼンテーションに母艦（マスター）がありません"));
    }
    let master_count = masters.len();
    let mut have_notes = false;
    let mut notes_frames: Option<(Option<(i64, i64, i64, i64)>, Option<(i64, i64, i64, i64)>)> = None;
    for (i, slide) in ir.slides.iter().enumerate() {
        let n = i + 1;
        let master_i = master_index_of.get(&slide.master_ref).copied().unwrap_or(0);
        let master = masters[master_i];
        let own_scheme = if slide.follow_master_scheme { None } else { slide.scheme };
        let scheme = own_scheme.or(master.scheme).unwrap_or(DEFAULT_SCHEME);
        let mut ctx = Ctx { ir: &ir, scheme, styles: &master.styles, pictures: HashMap::new(), next_rel: Cell::new(2), extra_rels: RefCell::new(Vec::new()), slide_places: &slide_places, master: false };
        let shapes: String = slide.shapes.iter().map(|s| shape_xml(&mut ctx, s, slide, &slide.outline, 0)).collect();
        let bg = if slide.follow_master_background { String::new() } else { background_xml(&mut ctx, slide) };
        let mut show = String::new();
        if !slide.follow_master_objects {
            show += r#" showMasterSp="0""#;
        }
        if slide.show.is_some_and(|x| x.hidden) {
            show += r#" show="0""#;
        }
        let transition = slide.show.map(|x| transition_xml(&x)).unwrap_or_default();
        let timing = match slide.timing.as_deref().and_then(crate::ppt_timing::timing_xml) {
            Some(x) => x,
            None => {
                // PowerPoint 97 to 2000: only the effect of each shape.
                fn gather(shapes: &[crate::ppt::ShapeIr], out: &mut Vec<(u32, crate::ppt_timing::Anim)>) {
                    for s in shapes {
                        if let Some(a) = s.anim {
                            out.push((s.id, a));
                        }
                        gather(&s.children, out);
                    }
                }
                let mut effects = Vec::new();
                gather(&slide.shapes, &mut effects);
                crate::ppt_timing::simple_timing(&effects).unwrap_or_default()
            }
        };
        let transition = transition + &timing;
        let slide_xml = format!(
            r#"{HEAD}<p:sld xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"{show}><p:cSld>{bg}{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{transition}</p:sld>"#,
            tree(&shapes)
        );
        let mut slide_rels = vec![rel("rId1", "slideLayout", &format!("../slideLayouts/slideLayout{}.xml", master_i + 1))];
        slide_rels.extend(page_media(&ctx, &mut media, &mut files, &mut extensions));
        slide_rels.extend(ctx.extra_rels.borrow().iter().cloned());
        // The notes page, when it has text.
        if let Some(np) = ir.notes.get(&slide.notes_ref).filter(|np| np.shapes.iter().any(|s| s.placeholder == Some(0x0C) && (s.text.as_ref().is_some_and(|t| t.paragraphs.iter().any(|p| p.runs.iter().any(|r| !r.0.trim().is_empty()))) || s.outline_ref.is_some()))) {
            let mut nctx = Ctx { ir: &ir, scheme: np.scheme.unwrap_or(DEFAULT_SCHEME), styles: &np.styles, pictures: HashMap::new(), next_rel: Cell::new(3), extra_rels: RefCell::new(Vec::new()), slide_places: &slide_places, master: false };
            let nshapes: String = np.shapes.iter().filter(|s| matches!(s.placeholder, Some(0x0B | 0x0C))).map(|s| shape_xml(&mut nctx, s, np, &np.outline, 0)).collect();
            let notes_xml = format!(
                r#"{HEAD}<p:notes xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld>{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#,
                tree(&nshapes)
            );
            let mut notes_rels = vec![rel("rId1", "notesMaster", "../notesMasters/notesMaster1.xml"), rel("rId2", "slide", &format!("../slides/slide{n}.xml"))];
            notes_rels.extend(page_media(&nctx, &mut media, &mut files, &mut extensions));
            notes_rels.extend(nctx.extra_rels.borrow().iter().cloned());
            files.push((format!("ppt/notesSlides/notesSlide{n}.xml"), notes_xml.into_bytes()));
            files.push((format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"), rels(&notes_rels).into_bytes()));
            let _ = write!(content_overrides, r#"<Override PartName="/ppt/notesSlides/notesSlide{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"/>"#);
            let id = format!("rId{}", ctx.next_rel.get().max(slide_rels.len() as u32 + 1));
            slide_rels.push(rel(&id, "notesSlide", &format!("../notesSlides/notesSlide{n}.xml")));
            have_notes = true;
            if notes_frames.is_none() {
                notes_frames = Some((np.shapes.iter().find(|s| s.placeholder == Some(0x0B)).and_then(|s| s.frame), np.shapes.iter().find(|s| s.placeholder == Some(0x0C)).and_then(|s| s.frame)));
            }
        }
        files.push((format!("ppt/slides/slide{n}.xml"), slide_xml.into_bytes()));
        files.push((format!("ppt/slides/_rels/slide{n}.xml.rels"), rels(&slide_rels).into_bytes()));
        let _ = write!(content_overrides, r#"<Override PartName="/ppt/slides/slide{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#);
        pres_rels.push(rel(&format!("rId{}", master_count + n), "slide", &format!("slides/slide{n}.xml")));
        let _ = write!(slide_ids, r#"<p:sldId id="{}" r:id="rId{}"/>"#, 255 + n, master_count + n);
    }
    let extra = master_count + ir.slides.len();
    pres_rels.extend(master_rels);
    pres_rels.push(rel(&format!("rId{}", extra + 1), "presProps", "presProps.xml"));
    pres_rels.push(rel(&format!("rId{}", extra + 2), "viewProps", "viewProps.xml"));
    pres_rels.push(rel(&format!("rId{}", extra + 3), "theme", "theme/theme1.xml"));
    let mut notes_master_list = String::new();
    if have_notes {
        // The notes master: the slide image and the notes text, where the notes pages have them.
        let (image, body) = notes_frames.unwrap_or((None, None));
        let frame = |f: Option<(i64, i64, i64, i64)>, d: (i64, i64, i64, i64)| {
            let (x, y, w, h) = f.unwrap_or(d);
            format!(r#"<p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>"#)
        };
        let shapes = format!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg" idx="2"/></p:nvPr></p:nvSpPr>{}</p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" sz="quarter" idx="3"/></p:nvPr></p:nvSpPr>{}<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="ja-JP" altLang="en-US"/></a:p></p:txBody></p:sp>"#,
            frame(image, (685_800, 1_143_000, 5_486_400, 3_086_100)),
            frame(body, (685_800, 4_400_550, 5_486_400, 3_600_450))
        );
        let notes_master = format!(
            r#"{HEAD}<p:notesMaster xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg>{}</p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/></p:notesMaster>"#,
            tree(&shapes)
        );
        let theme_n = master_count + 1;
        files.push(("ppt/notesMasters/notesMaster1.xml".to_string(), notes_master.into_bytes()));
        files.push(("ppt/notesMasters/_rels/notesMaster1.xml.rels".to_string(), rels(&[rel("rId1", "theme", &format!("../theme/theme{theme_n}.xml"))]).into_bytes()));
        files.push((format!("ppt/theme/theme{theme_n}.xml"), theme_of(&DEFAULT_SCHEME).into_bytes()));
        let _ = write!(content_overrides, r#"<Override PartName="/ppt/notesMasters/notesMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"/><Override PartName="/ppt/theme/theme{theme_n}.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#);
        pres_rels.push(rel(&format!("rId{}", extra + 4), "notesMaster", "notesMasters/notesMaster1.xml"));
        notes_master_list = format!(r#"<p:notesMasterIdLst><p:notesMasterId r:id="rId{}"/></p:notesMasterIdLst>"#, extra + 4);
    }
    let presentation = format!(
        r#"{HEAD}<p:presentation xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" saveSubsetFonts="1"><p:sldMasterIdLst>{master_ids}</p:sldMasterIdLst>{notes_master_list}<p:sldIdLst>{slide_ids}</p:sldIdLst><p:sldSz cx="{width}" cy="{height}"/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle><a:defPPr><a:defRPr lang="ja-JP"/></a:defPPr></p:defaultTextStyle></p:presentation>"#
    );
    let mut defaults = String::from(r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#);
    for ext in &extensions {
        let mime = match *ext {
            "jpeg" => "image/jpeg",
            "png" => "image/png",
            "bmp" => "image/bmp",
            "tiff" => "image/tiff",
            "emf" => "image/x-emf",
            "wmf" => "image/x-wmf",
            _ => "application/octet-stream",
        };
        let _ = write!(defaults, r#"<Default Extension="{ext}" ContentType="{mime}"/>"#);
    }
    let content_types = format!(
        r#"{HEAD}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{defaults}<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/presProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presProps+xml"/><Override PartName="/ppt/viewProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml"/>{content_overrides}</Types>"#
    );
    // presentation.xml.rels points to theme1 once more (the first master's), as a presentation does.
    let mut writer = ibis_ooxml::zip::Writer::new();
    let mut add = |name: &str, data: &[u8]| writer.add(name, ibis_ooxml::zip::EntryMeta::default(), data).map_err(|e| Error::new(format!("{e}")));
    add("[Content_Types].xml", content_types.as_bytes())?;
    add("_rels/.rels", rels(&[rel("rId1", "officeDocument", "ppt/presentation.xml")]).as_bytes())?;
    add("ppt/presentation.xml", presentation.as_bytes())?;
    add("ppt/_rels/presentation.xml.rels", rels(&pres_rels).as_bytes())?;
    add("ppt/presProps.xml", format!(r#"{HEAD}<p:presentationPr xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"/>"#).as_bytes())?;
    add("ppt/viewProps.xml", format!(r#"{HEAD}<p:viewPr xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:normalViewPr><p:restoredLeft sz="15620"/><p:restoredTop sz="94660"/></p:normalViewPr></p:viewPr>"#).as_bytes())?;
    for (name, data) in &files {
        add(name, data)?;
    }
    writer.finish().map_err(|e| Error::new(format!("{e}")))
}

#[allow(dead_code)]
fn unused(_: &Props) {}
