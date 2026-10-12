// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Themes: the colour scheme, the colour map and theme fonts.

use std::collections::HashMap;

use ibis_doc_model::{Color, Rgb};
use roxmltree::Node;

use crate::xml::{attr, child, children, elements};

const DRAWING_NS:&str="http://schemas.openxmlformats.org/drawingml/2006/main";
fn colour_attribute<'a>(node:Node<'a,'_>,name:&str)->Option<&'a str> {
    node.attributes().find(|a|a.namespace().is_none() && a.name()==name).map(|a|a.value())
}

/// Theme colours by name (`dk1`, `accent1`, …) and theme fonts.
#[derive(Clone, Debug, Default)]
pub(crate) struct Theme {
    colors: HashMap<String, Rgb>,
    major_latin: Option<String>,
    major_east_asian: Option<String>,
    minor_latin: Option<String>,
    minor_east_asian: Option<String>,
    /// The fonts for Japanese text (`<a:font script="Jpan">`), which Word
    /// uses when the East Asian font is left empty.
    major_japanese: Option<String>,
    minor_japanese: Option<String>,
    major_complex: Option<String>,
    minor_complex: Option<String>,
}

impl Theme {
    /// The Japanese font of the theme's major (headings) or minor (body)
    /// fonts, if it names one (`<a:font script="Jpan">`, else `ea`).
    pub(crate) fn japanese_font(&self, major: bool) -> Option<&str> {
        let (jpan, ea) = if major { (&self.major_japanese, &self.major_east_asian) } else { (&self.minor_japanese, &self.minor_east_asian) };
        jpan.as_deref().filter(|f| !f.is_empty()).or(ea.as_deref().filter(|f| !f.is_empty()))
    }

    /// The colour scheme, by name.
    pub(crate) fn colors(&self) -> Vec<(String, Rgb)> {
        let mut out: Vec<(String, Rgb)> = self.colors.iter().map(|(k, v)| (k.clone(), *v)).collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// A theme of these colours only (no fonts).
    pub(crate) fn from_colors(colors: &[(String, Rgb)]) -> Self {
        Self { colors: colors.iter().cloned().collect(), ..Self::default() }
    }

    /// A theme of these colours and fonts.
    pub(crate) fn from_colors_and_fonts(colors: &[(String, Rgb)], fonts: Option<&ibis_doc_model::ThemeFonts>) -> Self {
        let mut t = Self::from_colors(colors);
        if let Some(f) = fonts {
            t.major_latin = f.major.latin.clone();
            t.major_east_asian = f.major.east_asian.clone();
            t.major_complex = f.major.complex.clone();
            t.minor_latin = f.minor.latin.clone();
            t.minor_east_asian = f.minor.east_asian.clone();
            t.minor_complex = f.minor.complex.clone();
        }
        t
    }

    pub(crate) fn read(theme: Node) -> Self {
        let mut out = Self::default();
        let Some(elements) = child(theme, "themeElements") else { return out };
        if let Some(scheme) = child(elements, "clrScheme") {
            for entry in scheme.children().filter(Node::is_element) {
                // Theme colours are srgbClr or sysClr, without modifiers.
                let Some(value) = entry.children().find(Node::is_element) else { continue };
                let hex = if value.has_tag_name("sysClr") { attr(value, "lastClr") } else { attr(value, "val") };
                if let Some(rgb) = hex.and_then(parse_hex) {
                    out.colors.insert(entry.tag_name().name().to_string(), rgb);
                }
            }
        }
        if let Some(fonts) = child(elements, "fontScheme") {
            let typeface = |group: &str, script: &str| {
                child(fonts, group)
                    .and_then(|g| child(g, script))
                    .and_then(|f| attr(f, "typeface"))
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
            };
            out.major_latin = typeface("majorFont", "latin");
            out.major_east_asian = typeface("majorFont", "ea");
            out.minor_latin = typeface("minorFont", "latin");
            out.minor_east_asian = typeface("minorFont", "ea");
            let japanese = |group: &str| {
                child(fonts, group)
                    .and_then(|g| children(g, "font").find(|f| attr(*f, "script") == Some("Jpan")))
                    .and_then(|f| attr(f, "typeface"))
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
            };
            out.major_japanese = japanese("majorFont");
            out.minor_japanese = japanese("minorFont");
            out.major_complex = typeface("majorFont", "cs");
            out.minor_complex = typeface("minorFont", "cs");
        }
        out
    }

    /// The theme's fonts, for the model.
    pub(crate) fn fonts(&self) -> ibis_doc_model::ThemeFonts {
        let set = |latin: &Option<String>, east_asian: &Option<String>, complex: &Option<String>| ibis_doc_model::FontSet {
            latin: latin.clone(),
            east_asian: east_asian.clone(),
            complex: complex.clone(),
        };
        ibis_doc_model::ThemeFonts {
            major: set(&self.major_latin, &self.major_east_asian, &self.major_complex),
            minor: set(&self.minor_latin, &self.minor_east_asian, &self.minor_complex),
        }
    }

    /// A scheme colour by its own name (`dk1`, `accent1`, …).
    pub(crate) fn color(&self, name: &str) -> Option<Rgb> {
        self.colors.get(name).copied()
    }

    /// Resolve a typeface that may name a theme font (`+mj-lt`, `+mn-ea`, …).
    pub(crate) fn typeface(&self, name: &str) -> Option<String> {
        let resolved = match name {
            "+mj-lt" => self.major_latin.clone(),
            "+mj-ea" => self.major_east_asian.clone().or_else(|| self.major_latin.clone()),
            "+mj-cs" => self.major_complex.clone().or_else(|| self.major_latin.clone()),
            "+mn-lt" => self.minor_latin.clone(),
            "+mn-ea" => self.minor_east_asian.clone().or_else(|| self.minor_latin.clone()),
            "+mn-cs" => self.minor_complex.clone().or_else(|| self.minor_latin.clone()),
            "" => None,
            other => Some(other.to_string()),
        };
        resolved.filter(|t| !t.is_empty())
    }

    /// For the theme's East Asian font `name` (`+mn-ea`, `+mj-ea`) left
    /// empty by the theme: its Japanese font, if it names one.
    pub(crate) fn empty_east_asian_japanese(&self, name: &str) -> Option<String> {
        let (ea, jpan) = match name {
            "+mn-ea" => (&self.minor_east_asian, &self.minor_japanese),
            "+mj-ea" => (&self.major_east_asian, &self.major_japanese),
            _ => return None,
        };
        ea.as_deref().filter(|f| !f.is_empty()).is_none().then(|| jpan.clone()).flatten()
    }

    /// A Word document's theme East Asian font (`+mj-ea`, `+mn-ea`): the
    /// `ea` font, else the Japanese script's font, else the Latin one (a
    /// document written in Japanese, as the reader assumes).
    pub(crate) fn east_asian_typeface(&self, major: bool) -> Option<String> {
        let (ea, jpan) = if major { (&self.major_east_asian, &self.major_japanese) } else { (&self.minor_east_asian, &self.minor_japanese) };
        ea.clone().or_else(|| jpan.clone()).or_else(|| self.typeface(if major { "+mj-lt" } else { "+mn-lt" }))
    }

    /// A Word document's theme Complex Script font (`+mj-cs`, `+mn-cs`): the
    /// `cs` font, else the Latin one.
    pub(crate) fn complex_typeface(&self, major: bool) -> Option<String> {
        let cs = if major { &self.major_complex } else { &self.minor_complex };
        cs.clone().or_else(|| self.typeface(if major { "+mj-cs" } else { "+mn-cs" }))
    }
}

/// Resolves colour elements for one slide: its theme plus the colour map that
/// turns `tx1`, `bg1`, … into theme colour names.
#[derive(Clone, Debug)]
pub(crate) struct Colors<'t> {
    theme: &'t Theme,
    map: HashMap<String, String>,
}

impl<'t> Colors<'t> {
    /// The colour map of a master (`p:clrMap`), optionally overridden by a
    /// layout's or slide's `p:clrMapOvr`.
    pub(crate) fn new(theme: &'t Theme, master_map: Option<Node>, overrides: &[Option<Node>]) -> Self {
        let mut map: HashMap<String, String> = [
            ("bg1", "lt1"),
            ("tx1", "dk1"),
            ("bg2", "lt2"),
            ("tx2", "dk2"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let mut apply = |node: Node| {
            for a in node.attributes() {
                map.insert(a.name().to_string(), a.value().to_string());
            }
        };
        if let Some(node) = master_map {
            apply(node);
        }
        for over in overrides.iter().flatten() {
            if let Some(node) = child(*over, "overrideClrMapping") {
                apply(node);
            }
        }
        Self { theme, map }
    }

    pub(crate) fn theme(&self) -> &Theme {
        self.theme
    }

    /// A theme colour by name (`accent1`, `tx1`, …), through the colour map.
    pub(crate) fn named(&self, name: &str) -> Color {
        Color::opaque(self.scheme(name).unwrap_or(Rgb::BLACK))
    }

    pub(crate) fn scheme(&self, name: &str) -> Option<Rgb> {
        let mapped = self.map.get(name).map(String::as_str).unwrap_or(name);
        self.theme.colors.get(mapped).copied()
    }

    /// The colour given by the first colour element among `parent`'s children.
    /// `placeholder` is the colour `phClr` stands for, from a style reference.
    pub(crate) fn in_parent(&self, parent: Node, placeholder: Option<Color>) -> Option<Color> {
        parent.children().filter(Node::is_element).find_map(|n| self.resolve(n, placeholder))
    }

    /// Resolve one colour element (`a:srgbClr`, `a:schemeClr`, …) with its
    /// modifiers. `None` if `node` is not a colour element.
    pub(crate) fn resolve(&self, node: Node, placeholder: Option<Color>) -> Option<Color> {
        if node.tag_name().namespace()!=Some(DRAWING_NS) {return None;}
        let base = match node.tag_name().name() {
            "srgbClr" => Color::opaque(colour_attribute(node, "val").and_then(parse_hex)?),
            "sysClr" => Color::opaque(colour_attribute(node, "lastClr").and_then(parse_hex).unwrap_or(Rgb::BLACK)),
            "schemeClr" => match colour_attribute(node, "val")? {
                "phClr" => placeholder.unwrap_or(Color::BLACK),
                name => Color::opaque(self.scheme(name).unwrap_or(Rgb::BLACK)),
            },
            "prstClr" => Color::opaque(preset_color(colour_attribute(node, "val")?)),
            "scrgbClr" => {
                let channel = |name| {
                    let linear = colour_attribute(node, name).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 100_000.0;
                    (linear_to_srgb(linear) * 255.0).round() as u8
                };
                Color::opaque(Rgb::from_rgb(channel("r"), channel("g"), channel("b")))
            }
            "hslClr" => {
                let get = |name| colour_attribute(node, name).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                let (r, g, b) = hsl_to_rgb(get("hue") / 21_600_000.0, get("sat") / 100_000.0, get("lum") / 100_000.0);
                Color::opaque(Rgb::from_rgb(r, g, b))
            }
            _ => return None,
        };
        Some(apply_modifiers(base, node))
    }
}

fn parse_hex(value: &str) -> Option<Rgb> {
    (value.len() == 6).then(|| u32::from_str_radix(value, 16).ok()).flatten().map(Rgb)
}

fn preset_color(name: &str) -> Rgb {
    Rgb(match name {
        "white" => 0xffffff,
        "red" => 0xff0000,
        "green" => 0x008000,
        "blue" => 0x0000ff,
        "yellow" => 0xffff00,
        "gray" | "grey" => 0x808080,
        "ltGray" | "ltGrey" => 0xd3d3d3,
        "dkGray" | "dkGrey" => 0xa9a9a9,
        "orange" => 0xffa500,
        _ => 0x000000,
    })
}

/// Apply DrawingML colour transforms in document order as PowerPoint does.
fn apply_modifiers(mut color: Color, node: Node) -> Color {
    for m in elements(node).filter(|m| m.tag_name().namespace() == Some(DRAWING_NS)) {
        let raw_val = colour_attribute(m, "val").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        let val = raw_val / 100_000.0;
        let (r, g, b) = (color.rgb.r() as f32 / 255.0, color.rgb.g() as f32 / 255.0, color.rgb.b() as f32 / 255.0);
        let (r, g, b) = match m.tag_name().name() {
            "lum" => {
                let (h, s, _) = rgb_to_hsl(r, g, b);
                hsl_to_rgb_f32(h, s, val.clamp(0.0, 1.0))
            }
            "lumMod" => {
                let (h, s, mut l) = rgb_to_hsl(r, g, b);
                l *= val;
                hsl_to_rgb_f32(h, s, l.clamp(0.0, 1.0))
            }
            "lumOff" => {
                let (h, s, mut l) = rgb_to_hsl(r, g, b);
                l += val;
                hsl_to_rgb_f32(h, s, l.clamp(0.0, 1.0))
            }
            "sat" => {
                let (h, _, l) = rgb_to_hsl(r, g, b);
                hsl_to_rgb_f32(h, val.clamp(0.0, 1.0), l)
            }
            "satMod" => {
                let (h, mut s, l) = rgb_to_hsl(r, g, b);
                s *= val;
                hsl_to_rgb_f32(h, s.clamp(0.0, 1.0), l)
            }
            "satOff" => {
                let (h, mut s, l) = rgb_to_hsl(r, g, b);
                s += val;
                hsl_to_rgb_f32(h, s.clamp(0.0, 1.0), l)
            }
            "hue" => {
                let (_, s, l) = rgb_to_hsl(r, g, b);
                let h = (raw_val / 21_600_000.0).rem_euclid(1.0);
                hsl_to_rgb_f32(h, s, l)
            }
            "hueMod" => {
                let (mut h, s, l) = rgb_to_hsl(r, g, b);
                h = (h * val).rem_euclid(1.0);
                hsl_to_rgb_f32(h, s, l)
            }
            "hueOff" => {
                let (mut h, s, l) = rgb_to_hsl(r, g, b);
                h = (h + raw_val / 21_600_000.0).rem_euclid(1.0);
                hsl_to_rgb_f32(h, s, l)
            }
            "comp" => {
                let (h, s, l) = rgb_to_hsl(r, g, b);
                let h = (h + 0.5).rem_euclid(1.0);
                hsl_to_rgb_f32(h, s, l)
            }
            "red" => (val.clamp(0.0, 1.0), g, b),
            "redMod" => ((r * val).clamp(0.0, 1.0), g, b),
            "redOff" => ((r + val).clamp(0.0, 1.0), g, b),
            "green" => (r, val.clamp(0.0, 1.0), b),
            "greenMod" => (r, (g * val).clamp(0.0, 1.0), b),
            "greenOff" => (r, (g + val).clamp(0.0, 1.0), b),
            "blue" => (r, g, val.clamp(0.0, 1.0)),
            "blueMod" => (r, g, (b * val).clamp(0.0, 1.0)),
            "blueOff" => (r, g, (b + val).clamp(0.0, 1.0)),
            "gamma" => (linear_to_srgb(r), linear_to_srgb(g), linear_to_srgb(b)),
            "invGamma" => (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)),
            // Tint mixes towards white, shade towards black (in linear light).
            "tint" => map_linear(r, g, b, |c| 1.0 - (1.0 - c) * val),
            "shade" => map_linear(r, g, b, |c| c * val),
            "alpha" => {
                color.alpha = (val.clamp(0.0, 1.0) * 255.0).round() as u8;
                (r, g, b)
            }
            "alphaOff" => {
                color.alpha = (color.alpha as f32 + val * 255.0).clamp(0.0, 255.0).round() as u8;
                (r, g, b)
            }
            "alphaMod" => {
                color.alpha = (color.alpha as f32 * val).clamp(0.0, 255.0).round() as u8;
                (r, g, b)
            }
            "gray" => {
                let y = 0.299 * r + 0.587 * g + 0.114 * b;
                (y, y, y)
            }
            "inv" => map_linear(r, g, b, |c| 1.0 - c),
            _ => (r, g, b),
        };
        let to8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        color.rgb = Rgb::from_rgb(to8(r), to8(g), to8(b));
    }
    color
}

fn map_linear(r: f32, g: f32, b: f32, f: impl Fn(f32) -> f32) -> (f32, f32, f32) {
    let m = |c: f32| linear_to_srgb(f(srgb_to_linear(c)));
    (m(r), m(g), m(b))
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

/// `h`, `s` and `l` in 0..=1. Returns `(r, g, b)` in 0.0..=1.0.
fn hsl_to_rgb_f32(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s <= 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
}

/// `h`, `s` and `l` in 0..=1.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let (r, g, b) = hsl_to_rgb_f32(h, s, l);
    let to8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    (to8(r), to8(g), to8(b))
}

/// Line widths of the theme's `a:lnStyleLst`, for `p:style/a:lnRef idx`.
pub(crate) fn line_style<'a, 'i>(theme: Node<'a, 'i>, idx: usize) -> Option<Node<'a, 'i>> {
    let list = child(child(child(theme, "themeElements")?, "fmtScheme")?, "lnStyleLst")?;
    children(list, "ln").nth(idx.checked_sub(1)?)
}

/// The fill style `idx` refers to: 1–999 index `a:fillStyleLst`, 1001 and up
/// `a:bgFillStyleLst`.
pub(crate) fn fill_style<'a, 'i>(theme: Node<'a, 'i>, idx: usize) -> Option<Node<'a, 'i>> {
    let scheme = child(child(theme, "themeElements")?, "fmtScheme")?;
    let (list, index) = if idx > 1000 { ("bgFillStyleLst", idx - 1001) } else { ("fillStyleLst", idx.checked_sub(1)?) };
    elements(child(scheme, list)?).nth(index)
}

/// The background fill style `idx` refers to: 1–999 index `a:bgFillStyleLst`,
/// 1001 and up `a:fillStyleLst`.
pub(crate) fn bg_fill_style<'a, 'i>(theme: Node<'a, 'i>, idx: usize) -> Option<Node<'a, 'i>> {
    let scheme = child(child(theme, "themeElements")?, "fmtScheme")?;
    let (list, index) = if idx > 1000 { ("fillStyleLst", idx - 1001) } else { ("bgFillStyleLst", idx.checked_sub(1)?) };
    elements(child(scheme, list)?).nth(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color_of(xml: &str) -> Color {
        // Colour fixtures use the same DrawingML URI as a real Office part.
        let xml=format!("<root xmlns=\"{DRAWING_NS}\">{xml}</root>");
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let theme = Theme::default();
        let colors = Colors::new(&theme, None, &[]);
        colors.resolve(doc.root_element().first_element_child().unwrap(), None).unwrap()
    }

    #[test]
    fn lum_mod_and_off_lighten() {
        // 0x4472C4 at lumMod 60% / lumOff 40% is PowerPoint's 0x8FAADC.
        let c = color_of(r#"<srgbClr val="4472C4"><lumMod val="60000"/><lumOff val="40000"/></srgbClr>"#);
        let (r, g, b) = (c.rgb.r() as i32, c.rgb.g() as i32, c.rgb.b() as i32);
        assert!((r - 0x8f).abs() <= 2 && (g - 0xaa).abs() <= 2 && (b - 0xdc).abs() <= 2, "{:06x}", c.rgb.0);
    }

    #[test]
    fn alpha_modifier() {
        let c = color_of(r#"<srgbClr val="FF0000"><alpha val="50000"/></srgbClr>"#);
        assert_eq!(c.rgb, Rgb(0xff0000));
        assert_eq!(c.alpha, 128);
    }

    #[test]
    fn alpha_offset_is_ordered_clamped_and_namespace_scoped() {
        for (transforms,expected) in [
            (r#"<alpha val="50000"/><alphaOff val="10000"/>"#,154),
            (r#"<alpha val="50000"/><alphaMod val="50000"/><alphaOff val="10000"/>"#,90),
            (r#"<alpha val="50000"/><alphaOff val="10000"/><alphaMod val="50000"/>"#,77),
            (r#"<alpha val="50000"/><alphaOff val="-100000"/>"#,0),
            (r#"<alpha val="50000"/><alphaOff val="100000"/>"#,255),
            (r#"<alpha val="50000"/><x:alphaOff xmlns:x="urn:ibis:test:alpha" val="100000"/>"#,128),
            (r#"<alpha val="50000"/><alphaOff xmlns:x="urn:ibis:test:alpha" x:val="100000" val="0"/>"#,128),
        ] {
            let color=color_of(&format!("<srgbClr val=\"112233\">{transforms}</srgbClr>"));
            assert_eq!(color.rgb,Rgb(0x112233));assert_eq!(color.alpha,expected,"{transforms}");
        }
    }

    #[test]
    fn scheme_colors_follow_color_map() {
        let doc = roxmltree::Document::parse(
            r#"<theme xmlns="http://schemas.openxmlformats.org/drawingml/2006/main"><themeElements><clrScheme><dk1><sysClr lastClr="000000"/></dk1><lt1><srgbClr val="FFFFFF"/></lt1></clrScheme></themeElements></theme>"#,
        )
        .unwrap();
        let theme = Theme::read(doc.root_element());
        let map = roxmltree::Document::parse(r#"<clrMap bg1="dk1" tx1="lt1"/>"#).unwrap();
        let colors = Colors::new(&theme, Some(map.root_element()), &[]);
        let tx1 = roxmltree::Document::parse(r#"<schemeClr xmlns="http://schemas.openxmlformats.org/drawingml/2006/main" val="tx1"/>"#).unwrap();
        assert_eq!(colors.resolve(tx1.root_element(), None).unwrap().rgb, Rgb(0xffffff));
    }
}
