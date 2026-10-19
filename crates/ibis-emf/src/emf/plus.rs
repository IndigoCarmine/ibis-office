// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The EMF+ records ([MS-EMFPLUS]) carried in the comments of an Enhanced Metafile: graphics objects (brushes, pens, paths,
//! fonts, images, regions), drawing, the world and page transforms, the clip and the graphics state stack.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::{Player, f32_at, i32_at, u16_at, u32_at};
use crate::ImageFormat;
use crate::svg::{Font, IDENTITY, Matrix, Paint, Stroke, apply, scale_of, then};

type Point = (f64, f64);

#[derive(Clone, Debug)]
enum Brush {
    Solid(u32),
    Linear { rect: [f64; 4], start: u32, wrap: i32, transform: Option<Matrix>, stops: Vec<(f64, u32)> },
    /// A hatch: the two colours.
    Hatch(u32, u32),
    Radial { center: Point, center_color: u32, bounds: [f64; 4], transform: Option<Matrix>, stops: Vec<(f64, u32)> },
    /// A texture: only its presence is known.
    Texture,
}

#[derive(Clone, Debug)]
struct Pen {
    width: f64,
    unit: u32,
    brush: Brush,
    cap: u32,
    dash_cap: u32,
    join: u32,
    miter: Option<f64>,
    dash: Vec<f64>,
}

#[derive(Clone, Debug, Default)]
struct PathObj {
    points: Vec<Point>,
    types: Vec<u8>,
}

#[derive(Clone, Debug)]
struct FontObj {
    em: f64,
    unit: u32,
    style: i32,
    family: String,
}

#[derive(Clone, Debug, Default)]
struct Format {
    align: u32,
    line_align: u32,
}

#[derive(Clone, Debug)]
struct ImageObj {
    mime: &'static str,
    format: ImageFormat,
    data: Vec<u8>,
    size: Option<(f64, f64)>,
}

/// A region reduced to what a clip can use: a rectangle or a path in world units.
#[derive(Clone, Debug)]
enum RegionObj {
    Infinite,
    Empty,
    Rect([f64; 4]),
    Path(PathObj),
}

#[derive(Clone, Debug)]
enum Obj {
    Brush(Brush),
    Pen(Pen),
    Path(PathObj),
    Font(FontObj),
    Format(Format),
    Image(ImageObj),
    Region(RegionObj),
}

#[derive(Clone, Debug)]
struct Saved {
    world: Matrix,
    unit: u32,
    scale: f64,
    clip: Option<usize>,
    clip_box: Option<[f64; 4]>,
}

#[derive(Default)]
pub(super) struct Plus {
    /// An EMF+ header was seen.
    pub(super) present: bool,
    /// The metafile is EMF+ Dual (it has GDI records for the same drawing too).
    pub(super) dual: bool,
    /// The GDI records that follow are part of the drawing (EmfPlusGetDC).
    pub(super) get_dc: bool,
    /// A record that draws was not played.
    pub(super) unsupported: bool,
    /// How many drawing records were played.
    pub(super) draws: usize,
    /// The image of the only drawing record, when that is a whole bitmap at its own size.
    pub(super) sole_image: Option<(ImageFormat, Vec<u8>)>,
    dpi: (f64, f64),
    world: Matrix,
    unit: u32,
    scale: f64,
    stack: Vec<(u32, Saved)>,
    objects: HashMap<u8, Obj>,
    pending: Option<(u16, usize, Vec<u8>)>,
    clip: Option<usize>,
    clip_box: Option<[f64; 4]>,
}

/// The colour as a Windows `COLORREF` (red in the low byte) from the stored `0xAARRGGBB`, and its opacity.
fn argb(c: u32) -> (u32, f64) {
    (((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF), f64::from(c >> 24) / 255.0)
}

fn lerp_color(a: u32, b: u32, t: f64) -> u32 {
    let mix = |sh: u32| {
        let (x, y) = (f64::from((a >> sh) & 0xFF), f64::from((b >> sh) & 0xFF));
        ((x + (y - x) * t).round() as u32).min(255)
    };
    (mix(24) << 24) | (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

fn matrix_at(d: &[u8], at: usize) -> Option<Matrix> {
    Some([f32_at(d, at)?, f32_at(d, at + 4)?, f32_at(d, at + 8)?, f32_at(d, at + 12)?, f32_at(d, at + 16)?, f32_at(d, at + 20)?])
}

fn rectf_at(d: &[u8], at: usize) -> Option<[f64; 4]> {
    Some([f32_at(d, at)?, f32_at(d, at + 4)?, f32_at(d, at + 8)?, f32_at(d, at + 12)?])
}

fn read_f32_vec(d: &[u8], at: usize, n: usize) -> Option<Vec<f64>> {
    (0..n).map(|i| f32_at(d, at + i * 4)).collect()
}

/// EmfPlusPoint (16-bit integers) or EmfPlusPointF.
fn read_points(d: &[u8], at: usize, count: usize, compressed: bool) -> Option<Vec<Point>> {
    (0..count)
        .map(|i| {
            if compressed {
                Some((f64::from(u16_at(d, at + i * 4)? as i16), f64::from(u16_at(d, at + i * 4 + 2)? as i16)))
            } else {
                Some((f32_at(d, at + i * 8)?, f32_at(d, at + i * 8 + 4)?))
            }
        })
        .collect()
}

/// EmfPlusPointR: each coordinate is a 7-bit or a 15-bit signed integer; the points are relative to the previous one.
fn read_relative_points(d: &[u8], at: usize, count: usize) -> Option<(Vec<Point>, usize)> {
    let mut p = at;
    let mut out = Vec::with_capacity(count);
    let (mut x, mut y) = (0.0, 0.0);
    let one = |p: &mut usize| -> Option<f64> {
        let b = *d.get(*p)?;
        if b & 0x80 == 0 {
            *p += 1;
            let v = i32::from(b & 0x7F);
            Some(f64::from(if v >= 0x40 { v - 0x80 } else { v }))
        } else {
            let lo = *d.get(*p + 1)?;
            *p += 2;
            let v = (i32::from(b & 0x7F) << 8) | i32::from(lo);
            Some(f64::from(if v >= 0x4000 { v - 0x8000 } else { v }))
        }
    };
    for _ in 0..count {
        x += one(&mut p)?;
        y += one(&mut p)?;
        out.push((x, y));
    }
    Some((out, p))
}

/// EmfPlusRect (16-bit integers) or EmfPlusRectF.
fn read_rects(d: &[u8], at: usize, count: usize, compressed: bool) -> Option<Vec<[f64; 4]>> {
    (0..count)
        .map(|i| {
            if compressed {
                let o = at + i * 8;
                Some([f64::from(u16_at(d, o)? as i16), f64::from(u16_at(d, o + 2)? as i16), f64::from(u16_at(d, o + 4)? as i16), f64::from(u16_at(d, o + 6)? as i16)])
            } else {
                rectf_at(d, at + i * 16)
            }
        })
        .collect()
}

/// An EmfPlusPath object; `size` is how many bytes it takes.
fn parse_path(d: &[u8]) -> Option<(PathObj, usize)> {
    let count = u32_at(d, 4)? as usize;
    let flags = u32_at(d, 8)?;
    if count > 1_000_000 {
        return None;
    }
    let relative = flags & 0x0800 != 0;
    let compressed = flags & 0x4000 != 0;
    let (points, mut at) = if relative {
        read_relative_points(d, 12, count)?
    } else if compressed {
        (read_points(d, 12, count, true)?, 12 + count * 4)
    } else {
        (read_points(d, 12, count, false)?, 12 + count * 8)
    };
    let mut types = Vec::with_capacity(count);
    if relative {
        while types.len() < count {
            let (run, kind) = (*d.get(at)?, *d.get(at + 1)?);
            at += 2;
            let n = usize::from(run & 0x3F);
            let kind = if run & 0x80 != 0 { (kind & 0xF0) | 3 } else { kind };
            types.extend(std::iter::repeat_n(kind, n.min(count - types.len())));
        }
    } else {
        types.extend(d.get(at..at + count)?);
        at += count;
    }
    Some((PathObj { points, types }, (at + 3) & !3))
}

fn path_d(path: &PathObj, map: &dyn Fn(Point) -> Point) -> String {
    let mut d = String::new();
    let mut i = 0;
    let n = path.points.len().min(path.types.len());
    let put = |d: &mut String, cmd: &str, p: Point| {
        let (x, y) = map(p);
        let _ = write!(d, "{cmd}{} {} ", super::fmt(x), super::fmt(y));
    };
    while i < n {
        let ty = path.types[i];
        match ty & 0x0F {
            0 => put(&mut d, "M", path.points[i]),
            1 => put(&mut d, "L", path.points[i]),
            3 => {
                if i + 2 < n {
                    let (a, b, c) = (map(path.points[i]), map(path.points[i + 1]), map(path.points[i + 2]));
                    let _ = write!(d, "C{} {} {} {} {} {} ", super::fmt(a.0), super::fmt(a.1), super::fmt(b.0), super::fmt(b.1), super::fmt(c.0), super::fmt(c.1));
                    // The flags of the last point of the curve.
                    if path.types[i + 2] & 0x80 != 0 {
                        d.push_str("Z ");
                    }
                    i += 3;
                    continue;
                }
                break;
            }
            _ => {}
        }
        if ty & 0x80 != 0 {
            d.push_str("Z ");
        }
        i += 1;
    }
    d
}

/// Cubic Bézier pieces for an elliptical arc (angles in degrees, clockwise from the x axis).
fn arc_pieces(rect: [f64; 4], start: f64, sweep: f64) -> Vec<[Point; 4]> {
    let (cx, cy, rx, ry) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0, rect[2] / 2.0, rect[3] / 2.0);
    let sweep = sweep.clamp(-360.0, 360.0);
    let pieces = ((sweep.abs() / 90.0).ceil() as usize).max(1);
    let step = sweep.to_radians() / pieces as f64;
    let a0 = start.to_radians();
    let at = |a: f64| (cx + rx * a.cos(), cy + ry * a.sin());
    (0..pieces)
        .map(|i| {
            let (s, e) = (a0 + step * i as f64, a0 + step * (i + 1) as f64);
            let k = 4.0 / 3.0 * ((e - s) / 4.0).tan();
            [at(s), (cx + rx * (s.cos() - k * s.sin()), cy + ry * (s.sin() + k * s.cos())), (cx + rx * (e.cos() + k * e.sin()), cy + ry * (e.sin() - k * e.cos())), at(e)]
        })
        .collect()
}

/// Bézier pieces of a cardinal spline through the points.
fn spline_pieces(points: &[Point], tension: f64, closed: bool) -> Vec<[Point; 4]> {
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }
    let at = |i: isize| -> Point {
        if closed {
            points[i.rem_euclid(n as isize) as usize]
        } else {
            points[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let tangent = |i: isize| -> Point {
        let (a, b) = (at(i - 1), at(i + 1));
        (tension * (b.0 - a.0), tension * (b.1 - a.1))
    };
    let segments = if closed { n } else { n - 1 };
    (0..segments as isize)
        .map(|i| {
            let (p0, p1) = (at(i), at(i + 1));
            let (t0, t1) = (tangent(i), tangent(i + 1));
            [p0, (p0.0 + t0.0 / 3.0, p0.1 + t0.1 / 3.0), (p1.0 - t1.0 / 3.0, p1.1 - t1.1 / 3.0), p1]
        })
        .collect()
}

fn pieces_to_path(pieces: &[[Point; 4]], from_start: bool) -> PathObj {
    let mut path = PathObj::default();
    for (i, p) in pieces.iter().enumerate() {
        if i == 0 && from_start {
            path.points.push(p[0]);
            path.types.push(0);
        }
        path.points.extend([p[1], p[2], p[3]]);
        path.types.extend([3, 3, 3]);
    }
    path
}

fn polyline_path(points: &[Point], close: bool) -> PathObj {
    let mut path = PathObj::default();
    for (i, p) in points.iter().enumerate() {
        path.points.push(*p);
        path.types.push(if i == 0 { 0 } else { 1 });
    }
    if close {
        if let Some(last) = path.types.last_mut() {
            *last |= 0x80;
        }
    }
    path
}

fn rect_path(r: [f64; 4]) -> PathObj {
    polyline_path(&[(r[0], r[1]), (r[0] + r[2], r[1]), (r[0] + r[2], r[1] + r[3]), (r[0], r[1] + r[3])], true)
}

/// How many pixels one `unit` is, with `dpi` pixels to the inch.
fn unit_pixels(unit: u32, dpi: f64) -> f64 {
    match unit {
        3 => dpi / 72.0,
        4 => dpi,
        5 => dpi / 300.0,
        6 => dpi / 25.4,
        _ => 1.0,
    }
}

fn image_dimensions(data: &[u8], format: ImageFormat) -> Option<(f64, f64)> {
    match format {
        ImageFormat::Png => Some((f64::from(u32::from_be_bytes(data.get(16..20)?.try_into().ok()?)), f64::from(u32::from_be_bytes(data.get(20..24)?.try_into().ok()?)))),
        ImageFormat::Jpeg => {
            let mut at = 2;
            while at + 9 < data.len() {
                if data[at] != 0xFF {
                    at += 1;
                    continue;
                }
                let marker = data[at + 1];
                if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                    let h = u16::from_be_bytes(data.get(at + 5..at + 7)?.try_into().ok()?);
                    let w = u16::from_be_bytes(data.get(at + 7..at + 9)?.try_into().ok()?);
                    return Some((f64::from(w), f64::from(h)));
                }
                if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
                    at += 2;
                } else {
                    at += 2 + usize::from(u16::from_be_bytes(data.get(at + 2..at + 4)?.try_into().ok()?));
                }
            }
            None
        }
        _ => None,
    }
}

fn parse_brush(d: &[u8]) -> Option<Brush> {
    let kind = u32_at(d, 4)?;
    let b = d.get(8..)?;
    match kind {
        0 => Some(Brush::Solid(u32_at(b, 0)?)),
        1 => {
            // EmfPlusHatchBrushData: the style, then the foreground and background colours.
            Some(Brush::Hatch(u32_at(b, 4)?, u32_at(b, 8)?))
        }
        2 => Some(Brush::Texture),
        4 => {
            let flags = u32_at(b, 0)?;
            let wrap = i32_at(b, 4)?;
            let rect = rectf_at(b, 8)?;
            let (start, end) = (u32_at(b, 24)?, u32_at(b, 28)?);
            let mut at = 40;
            let transform = if flags & 0x02 != 0 {
                let m = matrix_at(b, at);
                at += 24;
                m
            } else {
                None
            };
            let mut stops = vec![(0.0, start), (1.0, end)];
            if flags & 0x04 != 0 {
                let n = u32_at(b, at)? as usize;
                if n > 1 && n < 10_000 {
                    let positions = read_f32_vec(b, at + 4, n)?;
                    stops = (0..n).map(|i| Some((positions[i], u32_at(b, at + 4 + n * 4 + i * 4)?))).collect::<Option<Vec<_>>>()?;
                }
            } else if flags & 0x08 != 0 {
                let n = u32_at(b, at)? as usize;
                if n > 1 && n < 10_000 {
                    let positions = read_f32_vec(b, at + 4, n)?;
                    let factors = read_f32_vec(b, at + 4 + n * 4, n)?;
                    // A factor is the share of the start colour.
                    stops = (0..n).map(|i| (positions[i], lerp_color(end, start, factors[i]))).collect();
                }
            }
            Some(Brush::Linear { rect, start, wrap, transform, stops })
        }
        3 => {
            let flags = u32_at(b, 0)?;
            let center_color = u32_at(b, 8)?;
            let center = (f32_at(b, 12)?, f32_at(b, 16)?);
            let n = u32_at(b, 20)? as usize;
            if n > 10_000 {
                return None;
            }
            let edge_color = u32_at(b, 24).filter(|_| n > 0).unwrap_or(center_color);
            let mut at = 24 + n * 4;
            let points = if flags & 0x01 != 0 {
                // A boundary path.
                let size = u32_at(b, at)? as usize;
                let (path, _) = parse_path(b.get(at + 4..at + 4 + size)?)?;
                at += 4 + size;
                path.points
            } else {
                let count = u32_at(b, at)? as usize;
                let points = read_points(b, at + 4, count.min(100_000), false)?;
                at += 4 + count * 8;
                points
            };
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for p in &points {
                x0 = x0.min(p.0);
                y0 = y0.min(p.1);
                x1 = x1.max(p.0);
                y1 = y1.max(p.1);
            }
            if x0 > x1 {
                return None;
            }
            let transform = if flags & 0x02 != 0 { matrix_at(b, at) } else { None };
            Some(Brush::Radial { center, center_color, bounds: [x0, y0, x1 - x0, y1 - y0], transform, stops: vec![(0.0, edge_color), (1.0, center_color)] })
        }
        _ => None,
    }
}

fn parse_pen(d: &[u8]) -> Option<Pen> {
    let flags = u32_at(d, 8)?;
    let unit = u32_at(d, 12)?;
    let width = f32_at(d, 16)?;
    let mut at = 20;
    if flags & 0x01 != 0 {
        at += 24;
    }
    let mut cap = 0;
    if flags & 0x02 != 0 {
        cap = u32_at(d, at)?;
        at += 4;
    }
    if flags & 0x04 != 0 {
        at += 4;
    }
    let mut join = 0;
    if flags & 0x08 != 0 {
        join = u32_at(d, at)?;
        at += 4;
    }
    let mut miter = None;
    if flags & 0x10 != 0 {
        miter = Some(f32_at(d, at)?);
        at += 4;
    }
    let mut style = 0;
    if flags & 0x20 != 0 {
        style = u32_at(d, at)?;
        at += 4;
    }
    let mut dash_cap = 0;
    if flags & 0x40 != 0 {
        dash_cap = u32_at(d, at)?;
        at += 4;
    }
    if flags & 0x80 != 0 {
        at += 4;
    }
    let mut dash = match style {
        1 => vec![3.0, 1.0],
        2 => vec![1.0, 1.0],
        3 => vec![3.0, 1.0, 1.0, 1.0],
        4 => vec![3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        _ => Vec::new(),
    };
    if flags & 0x100 != 0 {
        let n = u32_at(d, at)? as usize;
        if n < 1000 {
            dash = read_f32_vec(d, at + 4, n)?;
        }
        at += 4 + n * 4;
    }
    if flags & 0x200 != 0 {
        at += 4;
    }
    if flags & 0x400 != 0 {
        let n = u32_at(d, at)? as usize;
        at += 4 + n * 4;
    }
    if flags & 0x800 != 0 {
        let n = u32_at(d, at)? as usize;
        at += 4 + n;
    }
    if flags & 0x1000 != 0 {
        let n = u32_at(d, at)? as usize;
        at += 4 + n;
    }
    let brush = parse_brush(d.get(at..)?)?;
    Some(Pen { width, unit, brush, cap, dash_cap, join, miter, dash })
}

fn parse_font(d: &[u8]) -> Option<FontObj> {
    let em = f32_at(d, 4)?;
    let unit = u32_at(d, 8)?;
    let style = i32_at(d, 12)?;
    let len = u32_at(d, 20)? as usize;
    let units: Vec<u16> = (0..len.min(256)).filter_map(|i| u16_at(d, 24 + i * 2)).collect();
    Some(FontObj { em, unit, style, family: String::from_utf16_lossy(&units) })
}

fn parse_format(d: &[u8]) -> Option<Format> {
    Some(Format { align: u32_at(d, 12)?, line_align: u32_at(d, 16)? })
}

fn parse_image(d: &[u8]) -> Option<ImageObj> {
    let (_, (format, data)) = crate::emf_plus_image(d)?;
    let size = image_dimensions(&data, format);
    let mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::Bmp => "image/bmp",
        ImageFormat::Tiff => "image/tiff",
    };
    Some(ImageObj { mime, format, data, size })
}

fn parse_region(d: &[u8]) -> Option<RegionObj> {
    // A region of one node: infinite, empty, a rectangle or a path. Trees of operations are reduced to their first node.
    fn node(d: &[u8], at: usize) -> Option<RegionObj> {
        match u32_at(d, at)? {
            0x1000_0000 => Some(RegionObj::Rect(rectf_at(d, at + 4)?)),
            0x1000_0001 => {
                let size = u32_at(d, at + 4)? as usize;
                parse_path(d.get(at + 8..at + 8 + size)?).map(|(p, _)| RegionObj::Path(p))
            }
            0x1000_0002 => Some(RegionObj::Empty),
            0x1000_0003 => Some(RegionObj::Infinite),
            // The children follow; the first one stands for the whole.
            1..=5 => node(d, at + 4),
            _ => None,
        }
    }
    node(d, 8)
}

impl Player {
    /// The matrix from world units to device pixels.
    fn plus_matrix(&self) -> Matrix {
        let p = &self.plus;
        let s = unit_pixels(p.unit, p.dpi.0) * p.scale;
        then(p.world, [s, 0.0, 0.0, s, 0.0, 0.0])
    }

    fn plus_dev(&self, pt: Point) -> Point {
        apply(&self.plus_matrix(), pt.0, pt.1)
    }

    fn plus_object(&self, id: u32) -> Option<&Obj> {
        self.plus.objects.get(&((id & 0xFF) as u8))
    }

    /// The EMF+ records of one comment.
    pub(super) fn plus_comment(&mut self, r: &[u8]) {
        let len = u32_at(r, 8).unwrap_or(0) as usize;
        let data = &r[16.min(r.len())..(12 + len).min(r.len()).max(16.min(r.len()))];
        self.plus.get_dc = false;
        let mut at = 0;
        while let (Some(kind), Some(flags), Some(size), Some(data_size)) = (u16_at(data, at), u16_at(data, at + 2), u32_at(data, at + 4), u32_at(data, at + 8)) {
            let size = size as usize;
            if size < 12 || at + size > data.len() {
                break;
            }
            let body = &data[at + 12..at + 12 + (data_size as usize).min(size - 12)];
            self.plus_record(kind, flags, body);
            at += size;
        }
    }

    fn plus_record(&mut self, kind: u16, flags: u16, d: &[u8]) {
        match kind {
            0x4001 => {
                self.plus.present = true;
                self.plus.dual = flags & 1 != 0;
                self.plus.dpi = (f64::from(u32_at(d, 8).unwrap_or(96)).max(1.0), f64::from(u32_at(d, 12).unwrap_or(96)).max(1.0));
                self.plus.world = IDENTITY;
                self.plus.unit = 2;
                self.plus.scale = 1.0;
            }
            0x4004 => self.plus.get_dc = true,
            0x4008 => self.plus_object_record(flags, d),
            0x4009 => {
                let (c, a) = argb(u32_at(d, 0).unwrap_or(0));
                if a > 0.0 {
                    let (x, y, w, h) = self.view;
                    let path = format!("M{} {} L{} {} L{} {} L{} {} Z", super::fmt(x), super::fmt(y), super::fmt(x + w), super::fmt(y), super::fmt(x + w), super::fmt(y + h), super::fmt(x), super::fmt(y + h));
                    let paint = Paint { fill: Some(c), fill_url: None, fill_alpha: a, even_odd: false, stroke: None };
                    self.canvas.path(&IDENTITY, None, &path, &paint);
                    self.plus.draws += 1;
                }
            }
            0x400A | 0x400B => self.plus_rects(kind == 0x400A, flags, d),
            0x400C => self.plus_polygon(flags, d),
            0x400D => self.plus_lines(flags, d),
            0x400E | 0x400F => self.plus_ellipse(kind == 0x400E, flags, d),
            0x4010 | 0x4011 | 0x4012 => self.plus_pie(kind, flags, d),
            0x4013 => self.plus_fill_region(flags, d),
            0x4014 => {
                let path = self.plus_path_object(flags).cloned();
                if let (Some(path), Some(brush)) = (path, self.plus_brush(flags, u32_at(d, 0).unwrap_or(0))) {
                    self.plus_paint(&path, Some(&brush), None, false);
                }
            }
            0x4015 => {
                let path = self.plus_path_object(flags).cloned();
                if let (Some(path), Some(Obj::Pen(pen))) = (path, self.plus_object(u32_at(d, 0).unwrap_or(0)).cloned()) {
                    self.plus_paint(&path, None, Some(&pen), false);
                }
            }
            0x4016 | 0x4017 | 0x4018 | 0x4019 => self.plus_curve(kind, flags, d),
            0x401A => self.plus_image(flags, d),
            0x401B => self.plus_image_points(flags, d),
            0x401C => self.plus_string(flags, d),
            0x4036 => self.plus_driver_string(flags, d),
            0x4025 | 0x4028 => {
                let index = u32_at(d, 0).unwrap_or(0);
                let saved = self.plus_saved();
                self.plus.stack.push((index, saved));
            }
            0x4027 => {
                let index = u32_at(d, 36).unwrap_or(0);
                let saved = self.plus_saved();
                self.plus.stack.push((index, saved));
                // The destination rectangle is where the source one is drawn: scale and move.
                if let (Some(dst), Some(src)) = (rectf_at(d, 0), rectf_at(d, 16)) {
                    if src[2] != 0.0 && src[3] != 0.0 {
                        let (sx, sy) = (dst[2] / src[2], dst[3] / src[3]);
                        let map = [sx, 0.0, 0.0, sy, dst[0] - src[0] * sx, dst[1] - src[1] * sy];
                        self.plus.world = then(map, self.plus.world);
                    }
                }
            }
            0x4026 | 0x4029 => {
                let index = u32_at(d, 0).unwrap_or(0);
                if let Some(pos) = self.plus.stack.iter().rposition(|(i, _)| *i == index) {
                    let (_, saved) = self.plus.stack[pos].clone();
                    self.plus.stack.truncate(pos);
                    self.plus.world = saved.world;
                    self.plus.unit = saved.unit;
                    self.plus.scale = saved.scale;
                    self.plus.clip = saved.clip;
                    self.plus.clip_box = saved.clip_box;
                }
            }
            0x402A => {
                if let Some(m) = matrix_at(d, 0) {
                    self.plus.world = m;
                }
            }
            0x402B => self.plus.world = IDENTITY,
            0x402C | 0x402D | 0x402E | 0x402F => {
                let op = match kind {
                    0x402C => matrix_at(d, 0),
                    0x402D => Some([1.0, 0.0, 0.0, 1.0, f32_at(d, 0).unwrap_or(0.0), f32_at(d, 4).unwrap_or(0.0)]),
                    0x402E => Some([f32_at(d, 0).unwrap_or(1.0), 0.0, 0.0, f32_at(d, 4).unwrap_or(1.0), 0.0, 0.0]),
                    _ => {
                        let a = f32_at(d, 0).unwrap_or(0.0).to_radians();
                        Some([a.cos(), a.sin(), -a.sin(), a.cos(), 0.0, 0.0])
                    }
                };
                if let Some(op) = op {
                    // Post-multiplied: applied after the current transform; else before it.
                    self.plus.world = if flags & 0x2000 != 0 { then(self.plus.world, op) } else { then(op, self.plus.world) };
                }
            }
            0x4030 => {
                self.plus.unit = u32::from(flags & 0xFF);
                self.plus.scale = f32_at(d, 0).unwrap_or(1.0);
            }
            0x4031 => {
                self.plus.clip = None;
                self.plus.clip_box = None;
            }
            0x4032 => {
                if let Some(rect) = rectf_at(d, 0) {
                    self.plus_clip(flags, RegionObj::Rect(rect));
                }
            }
            0x4033 => {
                if let Some(Obj::Path(path)) = self.plus_object(u32::from(flags & 0xFF)).cloned() {
                    self.plus_clip(flags, RegionObj::Path(path));
                }
            }
            0x4034 => {
                if let Some(Obj::Region(region)) = self.plus_object(u32::from(flags & 0xFF)).cloned() {
                    self.plus_clip(flags, region);
                }
            }
            // The state that does not change what is drawn here, and the end.
            0x4002 | 0x4003 | 0x401D..=0x4024 | 0x4035 | 0x4038..=0x403A => {}
            _ => {}
        }
    }

    fn plus_saved(&self) -> Saved {
        let p = &self.plus;
        Saved { world: p.world, unit: p.unit, scale: p.scale, clip: p.clip, clip_box: p.clip_box }
    }

    fn plus_object_record(&mut self, flags: u16, d: &[u8]) {
        let id = (flags & 0xFF) as u8;
        let kind = ((flags >> 8) & 0x7F) as u8;
        // An object too big for one record continues in the next: the first four bytes give the whole size.
        let whole: Vec<u8>;
        let body: &[u8] = if flags & 0x8000 != 0 {
            let Some(total) = u32_at(d, 0) else { return };
            let part = &d[4..];
            match &mut self.plus.pending {
                Some((pid, _, buf)) if *pid == flags & 0x7FFF => buf.extend_from_slice(part),
                _ => self.plus.pending = Some((flags & 0x7FFF, total as usize, part.to_vec())),
            }
            match &self.plus.pending {
                Some((_, total, buf)) if buf.len() >= *total => {
                    whole = buf.clone();
                    self.plus.pending = None;
                    &whole
                }
                _ => return,
            }
        } else {
            d
        };
        let obj = match kind {
            1 => parse_brush(body).map(Obj::Brush),
            2 => parse_pen(body).map(Obj::Pen),
            3 => parse_path(body).map(|(p, _)| Obj::Path(p)),
            4 => parse_region(body).map(Obj::Region),
            5 => parse_image(body).map(Obj::Image),
            6 => parse_font(body).map(Obj::Font),
            7 => parse_format(body).map(Obj::Format),
            _ => None,
        };
        match obj {
            Some(obj) => {
                self.plus.objects.insert(id, obj);
            }
            None => {
                self.plus.objects.remove(&id);
                // An image or brush that is not understood leaves later drawing incomplete.
                if matches!(kind, 1 | 2 | 5) {
                    self.plus.unsupported = true;
                }
            }
        }
    }

    fn plus_path_object(&self, flags: u16) -> Option<&PathObj> {
        match self.plus_object(u32::from(flags & 0xFF))? {
            Obj::Path(p) => Some(p),
            _ => None,
        }
    }

    /// The brush a record names: a colour (the `S` flag, 0x8000) or an object.
    fn plus_brush(&self, flags: u16, id: u32) -> Option<Brush> {
        if flags & 0x8000 != 0 {
            return Some(Brush::Solid(id));
        }
        match self.plus_object(id)? {
            Obj::Brush(b) => Some(b.clone()),
            _ => None,
        }
    }

    fn plus_clip(&mut self, flags: u16, region: RegionObj) {
        let mode = (flags >> 8) & 0x0F;
        let map = |p: Point| self.plus_dev(p);
        let (d, aabb): (String, Option<[f64; 4]>) = match &region {
            RegionObj::Infinite => {
                if mode == 0 {
                    self.plus.clip = None;
                    self.plus.clip_box = None;
                }
                return;
            }
            RegionObj::Empty => ("M0 0 ".to_string(), Some([0.0, 0.0, 0.0, 0.0])),
            RegionObj::Rect(r) => {
                let corners = [map((r[0], r[1])), map((r[0] + r[2], r[1])), map((r[0] + r[2], r[1] + r[3])), map((r[0], r[1] + r[3]))];
                let axis_aligned = (corners[0].1 - corners[1].1).abs() < 1e-6 && (corners[1].0 - corners[2].0).abs() < 1e-6 || (corners[0].0 - corners[1].0).abs() < 1e-6 && (corners[1].1 - corners[2].1).abs() < 1e-6;
                let (x0, y0) = corners.iter().fold((f64::MAX, f64::MAX), |a, p| (a.0.min(p.0), a.1.min(p.1)));
                let (x1, y1) = corners.iter().fold((f64::MIN, f64::MIN), |a, p| (a.0.max(p.0), a.1.max(p.1)));
                (path_d(&rect_path(*r), &map), if axis_aligned { Some([x0, y0, x1, y1]) } else { None })
            }
            RegionObj::Path(p) => (path_d(p, &map), None),
        };
        match mode {
            // Replace (0), intersect (1): rectangles cut each other down; else the new shape stands.
            0 | 1 => {
                let new_box = match (mode, self.plus.clip_box, aabb) {
                    (1, Some(old), Some(new)) => Some([old[0].max(new[0]), old[1].max(new[1]), old[2].min(new[2]), old[3].min(new[3])]),
                    (_, _, new) => new,
                };
                let d = match new_box {
                    Some(b) if mode == 1 && aabb.is_some() => format!("M{} {} L{} {} L{} {} L{} {} Z", super::fmt(b[0]), super::fmt(b[1]), super::fmt(b[2]), super::fmt(b[1]), super::fmt(b[2]), super::fmt(b[3]), super::fmt(b[0]), super::fmt(b[3])),
                    _ => d,
                };
                self.plus.clip = Some(self.canvas.clip_path(&d, false));
                self.plus.clip_box = new_box;
            }
            _ => {
                // Union, xor, exclude and complement are not played.
                self.plus.unsupported = true;
            }
        }
    }

    /// The SVG paint for a brush filling `bounds` (device units).
    fn plus_fill(&mut self, brush: &Brush) -> (Option<u32>, Option<String>, f64) {
        match brush {
            Brush::Solid(c) => {
                let (c, a) = argb(*c);
                (Some(c), None, a)
            }
            Brush::Hatch(fg, bg) => {
                let (mixed, a) = argb(lerp_color(*bg, *fg, 0.5));
                (Some(mixed), None, a)
            }
            Brush::Texture => {
                self.plus.unsupported = true;
                (Some(0x80_8080), None, 0.5)
            }
            Brush::Linear { rect, wrap, transform, stops, .. } => {
                let dev = self.plus_matrix();
                let gt = match transform {
                    Some(t) => then(*t, dev),
                    None => dev,
                };
                let spread = match wrap {
                    0 => "repeat",
                    1..=3 => "reflect",
                    _ => "pad",
                };
                let stops: String = stops
                    .iter()
                    .map(|(o, c)| {
                        let (rgbv, a) = argb(*c);
                        format!(r##"<stop offset="{}" stop-color="{}" stop-opacity="{}"/>"##, super::fmt(o.clamp(0.0, 1.0)), crate::svg::color(rgbv), super::fmt(a))
                    })
                    .collect();
                let xml = format!(
                    r#"<linearGradient id="{{id}}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}" gradientTransform="matrix({} {} {} {} {} {})" spreadMethod="{spread}">{stops}</linearGradient>"#,
                    super::fmt(rect[0]),
                    super::fmt(rect[1]),
                    super::fmt(rect[0] + rect[2]),
                    super::fmt(rect[1]),
                    gt[0],
                    gt[1],
                    gt[2],
                    gt[3],
                    gt[4],
                    gt[5]
                );
                (None, Some(self.canvas.paint_server(&xml)), 1.0)
            }
            Brush::Radial { center, bounds, transform, stops, .. } => {
                let dev = self.plus_matrix();
                let gt = match transform {
                    Some(t) => then(*t, dev),
                    None => dev,
                };
                let r = (bounds[2].max(bounds[3]) / 2.0).max(0.001);
                // The colour at the centre first; the boundary last.
                let stops: String = stops
                    .iter()
                    .rev()
                    .map(|(o, c)| {
                        let (rgbv, a) = argb(*c);
                        format!(r##"<stop offset="{}" stop-color="{}" stop-opacity="{}"/>"##, super::fmt(1.0 - o.clamp(0.0, 1.0)), crate::svg::color(rgbv), super::fmt(a))
                    })
                    .collect();
                let xml = format!(
                    r#"<radialGradient id="{{id}}" gradientUnits="userSpaceOnUse" cx="{}" cy="{}" r="{}" gradientTransform="matrix({} {} {} {} {} {})">{stops}</radialGradient>"#,
                    super::fmt(center.0),
                    super::fmt(center.1),
                    super::fmt(r),
                    gt[0],
                    gt[1],
                    gt[2],
                    gt[3],
                    gt[4],
                    gt[5]
                );
                (None, Some(self.canvas.paint_server(&xml)), 1.0)
            }
        }
    }

    fn plus_stroke(&self, pen: &Pen) -> Stroke {
        let dev = self.plus_matrix();
        let width = if pen.unit == 0 { pen.width * scale_of(&dev) } else { pen.width * unit_pixels(pen.unit, self.plus.dpi.0) };
        let width = if width <= 0.0 { 1.0 } else { width };
        let (color, alpha) = match &pen.brush {
            Brush::Solid(c) => argb(*c),
            Brush::Linear { start, .. } => argb(*start),
            Brush::Hatch(fg, _) => argb(*fg),
            Brush::Radial { center_color, .. } => argb(*center_color),
            Brush::Texture => (0x80_8080, 1.0),
        };
        let cap = match if pen.dash.is_empty() { pen.cap & 0xF } else { pen.dash_cap } {
            1 => "square",
            2 => "round",
            _ => "butt",
        };
        let join = match pen.join {
            1 => "bevel",
            2 => "round",
            _ => "miter",
        };
        Stroke { color, alpha, miter: pen.miter.map(|m| m.max(1.0)), width, dash: pen.dash.iter().map(|d| d * width).collect(), cap, join }
    }

    /// Fill and/or stroke a path (in world units).
    fn plus_paint(&mut self, path: &PathObj, brush: Option<&Brush>, pen: Option<&Pen>, winding: bool) {
        let map = |p: Point| self.plus_dev(p);
        let d = path_d(path, &map);
        let extent: Vec<Point> = path.points.iter().map(|&p| self.plus_dev(p)).collect();
        let (fill, fill_url, fill_alpha) = match brush {
            Some(b) => self.plus_fill(b),
            None => (None, None, 1.0),
        };
        let stroke = pen.map(|p| self.plus_stroke(p));
        let paint = Paint { fill, fill_url, fill_alpha, even_odd: !winding, stroke };
        let clip = self.plus.clip;
        self.canvas.grow(&IDENTITY, &extent);
        self.canvas.path(&IDENTITY, clip, &d, &paint);
        self.plus.draws += 1;
    }

    fn plus_pen(&self, flags: u16) -> Option<Pen> {
        match self.plus_object(u32::from(flags & 0xFF))? {
            Obj::Pen(p) => Some(p.clone()),
            _ => None,
        }
    }

    fn plus_rects(&mut self, fill: bool, flags: u16, d: &[u8]) {
        let (brush_id, at) = if fill { (u32_at(d, 0).unwrap_or(0), 4) } else { (0, 0) };
        let Some(count) = u32_at(d, at) else { return };
        let Some(rects) = read_rects(d, at + 4, (count as usize).min(100_000), flags & 0x4000 != 0) else { return };
        let (brush, pen) = if fill { (self.plus_brush(flags, brush_id), None) } else { (None, self.plus_pen(flags)) };
        if (fill && brush.is_none()) || (!fill && pen.is_none()) {
            self.plus.unsupported = true;
            return;
        }
        for r in rects {
            self.plus_paint(&rect_path(r), brush.as_ref(), pen.as_ref(), false);
        }
    }

    fn plus_polygon(&mut self, flags: u16, d: &[u8]) {
        let Some(count) = u32_at(d, 4) else { return };
        let points = self.plus_points(flags, d, 8, count as usize);
        let Some(brush) = self.plus_brush(flags, u32_at(d, 0).unwrap_or(0)) else { return };
        if let Some(points) = points {
            self.plus_paint(&polyline_path(&points, true), Some(&brush), None, flags & 0x2000 != 0);
        }
    }

    /// The points of a record: relative (flag 0x0800), 16-bit integers (0x4000) or floats.
    fn plus_points(&self, flags: u16, d: &[u8], at: usize, count: usize) -> Option<Vec<Point>> {
        let count = count.min(d.len());
        if flags & 0x0800 != 0 {
            read_relative_points(d, at, count).map(|(p, _)| p)
        } else {
            read_points(d, at, count, flags & 0x4000 != 0)
        }
    }

    fn plus_lines(&mut self, flags: u16, d: &[u8]) {
        let Some(count) = u32_at(d, 0) else { return };
        let Some(points) = self.plus_points(flags, d, 4, count as usize) else { return };
        let Some(pen) = self.plus_pen(flags) else { return };
        self.plus_paint(&polyline_path(&points, flags & 0x2000 != 0), None, Some(&pen), false);
    }

    fn plus_ellipse(&mut self, fill: bool, flags: u16, d: &[u8]) {
        let at = if fill { 4 } else { 0 };
        let Some(rect) = read_rects(d, at, 1, flags & 0x4000 != 0).and_then(|r| r.first().copied()) else { return };
        let pieces = arc_pieces(rect, 0.0, 360.0);
        let mut path = pieces_to_path(&pieces, true);
        if let Some(last) = path.types.last_mut() {
            *last |= 0x80;
        }
        if fill {
            if let Some(brush) = self.plus_brush(flags, u32_at(d, 0).unwrap_or(0)) {
                self.plus_paint(&path, Some(&brush), None, false);
            }
        } else if let Some(pen) = self.plus_pen(flags) {
            self.plus_paint(&path, None, Some(&pen), false);
        }
    }

    /// FillPie (0x4010), DrawPie (0x4011), DrawArc (0x4012).
    fn plus_pie(&mut self, kind: u16, flags: u16, d: &[u8]) {
        let fill = kind == 0x4010;
        let at = if fill { 4 } else { 0 };
        let (Some(start), Some(sweep)) = (f32_at(d, at), f32_at(d, at + 4)) else { return };
        let Some(rect) = read_rects(d, at + 8, 1, flags & 0x4000 != 0).and_then(|r| r.first().copied()) else { return };
        let pieces = arc_pieces(rect, start, sweep);
        let mut path = pieces_to_path(&pieces, true);
        if kind != 0x4012 {
            // A pie runs back through the centre.
            path.points.push((rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0));
            path.types.push(1);
            if let Some(last) = path.types.last_mut() {
                *last |= 0x80;
            }
        }
        if fill {
            if let Some(brush) = self.plus_brush(flags, u32_at(d, 0).unwrap_or(0)) {
                self.plus_paint(&path, Some(&brush), None, false);
            }
        } else if let Some(pen) = self.plus_pen(flags) {
            self.plus_paint(&path, None, Some(&pen), false);
        }
    }

    fn plus_fill_region(&mut self, flags: u16, d: &[u8]) {
        let Some(brush) = self.plus_brush(flags, u32_at(d, 0).unwrap_or(0)) else { return };
        match self.plus_object(u32::from(flags & 0xFF)).cloned() {
            Some(Obj::Region(RegionObj::Rect(r))) => self.plus_paint(&rect_path(r), Some(&brush), None, false),
            Some(Obj::Region(RegionObj::Path(p))) => self.plus_paint(&p, Some(&brush), None, false),
            Some(Obj::Region(RegionObj::Infinite)) => {
                let (x, y, w, h) = self.view;
                let paint_path = format!("M{} {} L{} {} L{} {} L{} {} Z", super::fmt(x), super::fmt(y), super::fmt(x + w), super::fmt(y), super::fmt(x + w), super::fmt(y + h), super::fmt(x), super::fmt(y + h));
                let (fill, fill_url, fill_alpha) = self.plus_fill(&brush);
                let clip = self.plus.clip;
                self.canvas.path(&IDENTITY, clip, &paint_path, &Paint { fill, fill_url, fill_alpha, even_odd: false, stroke: None });
                self.plus.draws += 1;
            }
            _ => {}
        }
    }

    /// FillClosedCurve (0x4016), DrawClosedCurve (0x4017), DrawCurve (0x4018), DrawBeziers (0x4019).
    fn plus_curve(&mut self, kind: u16, flags: u16, d: &[u8]) {
        let fill = kind == 0x4016;
        let (brush, pen, mut at) = if fill {
            (self.plus_brush(flags, u32_at(d, 0).unwrap_or(0)), None, 4)
        } else {
            (None, self.plus_pen(flags), 0)
        };
        if (fill && brush.is_none()) || (!fill && pen.is_none()) {
            return;
        }
        let tension = if kind == 0x4019 {
            0.0
        } else {
            let t = f32_at(d, at).unwrap_or(0.5);
            at += 4;
            t
        };
        let (mut offset, mut segments) = (0usize, 0usize);
        if kind == 0x4018 {
            offset = u32_at(d, at).unwrap_or(0) as usize;
            segments = u32_at(d, at + 4).unwrap_or(0) as usize;
            at += 8;
        }
        let Some(count) = u32_at(d, at) else { return };
        let Some(points) = self.plus_points(flags, d, at + 4, count as usize) else { return };
        let path = match kind {
            0x4019 => {
                let pieces: Vec<[Point; 4]> = points.windows(4).step_by(3).map(|w| [w[0], w[1], w[2], w[3]]).collect();
                pieces_to_path(&pieces, true)
            }
            0x4018 => {
                let end = (offset + segments + 1).min(points.len());
                let slice = points.get(offset..end).unwrap_or(&[]);
                pieces_to_path(&spline_pieces(slice, tension, false), true)
            }
            _ => {
                let mut p = pieces_to_path(&spline_pieces(&points, tension, true), true);
                if let Some(last) = p.types.last_mut() {
                    *last |= 0x80;
                }
                p
            }
        };
        self.plus_paint(&path, brush.as_ref(), pen.as_ref(), flags & 0x2000 != 0);
    }

    fn plus_image_object(&self, id: u32) -> Option<ImageObj> {
        match self.plus_object(id)? {
            Obj::Image(i) => Some(i.clone()),
            _ => {
                None
            }
        }
    }

    /// Draw the part `src` of the image into the parallelogram given as the matrix `map` from the image's own pixels to world units.
    fn plus_place_image(&mut self, image: &ImageObj, src: [f64; 4], map: Matrix) {
        let Some((iw, ih)) = image.size.or_else(|| Some((src[0] + src[2], src[1] + src[3]))) else { return };
        let dev = then(map, self.plus_matrix());
        let full = src[0] <= 0.0 && src[1] <= 0.0 && src[2] >= iw - 0.5 && src[3] >= ih - 0.5 || image.size.is_none();
        let corners = [(src[0], src[1]), (src[0] + src[2], src[1]), (src[0] + src[2], src[1] + src[3]), (src[0], src[1] + src[3])];
        let extent: Vec<Point> = corners.iter().map(|&p| apply(&dev, p.0, p.1)).collect();
        self.canvas.grow(&IDENTITY, &extent);
        let clip = self.plus.clip;
        if full {
            self.canvas.image(&dev, clip, 0.0, 0.0, iw, ih, image.mime, &image.data);
        } else {
            // The part of the image shown: the whole image, cut to the source rectangle.
            let d = format!("M{} {} L{} {} L{} {} L{} {} Z", super::fmt(extent[0].0), super::fmt(extent[0].1), super::fmt(extent[1].0), super::fmt(extent[1].1), super::fmt(extent[2].0), super::fmt(extent[2].1), super::fmt(extent[3].0), super::fmt(extent[3].1));
            let cut = self.canvas.clip_path(&d, false);
            self.canvas.open_group(clip);
            self.canvas.image(&dev, Some(cut), 0.0, 0.0, iw, ih, image.mime, &image.data);
            self.canvas.close_group();
        }
        self.plus.draws += 1;
    }

    fn plus_image(&mut self, flags: u16, d: &[u8]) {
        let Some(image) = self.plus_image_object(u32::from(flags & 0xFF)) else {
            self.plus.unsupported = true;
            return;
        };
        let (Some(src), Some(dst)) = (rectf_at(d, 8), read_rects(d, 24, 1, flags & 0x4000 != 0).and_then(|r| r.first().copied())) else { return };
        if src[2] <= 0.0 || src[3] <= 0.0 {
            return;
        }
        let (sx, sy) = (dst[2] / src[2], dst[3] / src[3]);
        let map = [sx, 0.0, 0.0, sy, dst[0] - src[0] * sx, dst[1] - src[1] * sy];
        let draws_before = self.plus.draws;
        self.plus_place_image(&image, src, map);
        // A whole bitmap, one to one, as the only drawing: the file is just the picture.
        let one_to_one = (sx - 1.0).abs() < 1e-6 && (sy - 1.0).abs() < 1e-6 && dst[0].abs() < 1e-6 && dst[1].abs() < 1e-6;
        let world_plain = self.plus.world == IDENTITY;
        self.plus.sole_image = if draws_before == 0 && one_to_one && world_plain && self.plus.scale == 1.0 && self.plus.unit == 2 && image.size.is_some_and(|(w, h)| (w - src[2]).abs() < 0.5 && (h - src[3]).abs() < 0.5) && src[0] == 0.0 && src[1] == 0.0 {
            Some((image.format, image.data.clone()))
        } else {
            None
        };
    }

    fn plus_image_points(&mut self, flags: u16, d: &[u8]) {
        let Some(image) = self.plus_image_object(u32::from(flags & 0xFF)) else {
            self.plus.unsupported = true;
            return;
        };
        let (Some(src), Some(count)) = (rectf_at(d, 8), u32_at(d, 24)) else { return };
        let Some(points) = self.plus_points(flags, d, 28, count as usize) else { return };
        if points.len() < 3 || src[2] <= 0.0 || src[3] <= 0.0 {
            return;
        }
        // Upper left, upper right and lower left corners of the parallelogram.
        let (ul, ur, ll) = (points[0], points[1], points[2]);
        let (ax, ay) = ((ur.0 - ul.0) / src[2], (ur.1 - ul.1) / src[2]);
        let (bx, by) = ((ll.0 - ul.0) / src[3], (ll.1 - ul.1) / src[3]);
        let map = [ax, ay, bx, by, ul.0 - ax * src[0] - bx * src[1], ul.1 - ay * src[0] - by * src[1]];
        self.plus_place_image(&image, src, map);
        self.plus.sole_image = None;
    }

    /// The size of the font in pixels and the pixel size of one world unit's scale.
    fn plus_font(&self, font: &FontObj) -> Font {
        let dev = self.plus_matrix();
        let size = if font.unit == 0 {
            font.em * scale_of(&dev)
        } else {
            // Units of the page (inches, points, pixels): converted into pixels directly.
            font.em * unit_pixels(font.unit, self.plus.dpi.0) * self.plus.scale * scale_of(&self.plus.world)
        };
        Font { face: font.family.clone(), size, bold: font.style & 1 != 0, italic: font.style & 2 != 0, underline: font.style & 4 != 0, strike: font.style & 8 != 0, angle: 0.0 }
    }

    /// EmfPlusDrawString: the layout rectangle, the string format's alignment.
    fn plus_string(&mut self, flags: u16, d: &[u8]) {
        let (Some(brush_id), Some(format_id), Some(len), Some(rect)) = (u32_at(d, 0), u32_at(d, 4), u32_at(d, 8), rectf_at(d, 12)) else { return };
        let len = len as usize;
        let units: Vec<u16> = (0..len.min(100_000)).filter_map(|i| u16_at(d, 28 + i * 2)).collect();
        let text = String::from_utf16_lossy(&units);
        let Some(Obj::Font(font)) = self.plus_object(u32::from(flags & 0xFF)).cloned() else { return };
        let Some(brush) = self.plus_brush(flags, brush_id) else { return };
        let (color, alpha) = match &brush {
            Brush::Solid(c) => argb(*c),
            Brush::Linear { start, .. } => argb(*start),
            Brush::Hatch(fg, _) => argb(*fg),
            Brush::Radial { center_color, .. } => argb(*center_color),
            Brush::Texture => (0, 1.0),
        };
        let format = match self.plus_object(format_id) {
            Some(Obj::Format(f)) if format_id != 0xFFFF_FFFF => f.clone(),
            _ => Format::default(),
        };
        let f = self.plus_font(&font);
        let matrix = self.plus_matrix();
        let rot = matrix[1].atan2(matrix[0]).to_degrees();
        let mut f_rot = f.clone();
        f_rot.angle = -rot;
        let lines: Vec<&str> = text.split(['\n', '\r']).filter(|l| !l.is_empty()).collect();
        let (ascent, descent) = crate::svg::font_metrics(&font.family, f.bold, f.italic);
        let line_height = f.size * (ascent + descent);
        let total = line_height * lines.len() as f64;
        // The layout rectangle in device units: its anchor, by the alignments.
        let ax = match format.align {
            1 => rect[0] + rect[2] / 2.0,
            2 => rect[0] + rect[2],
            _ => rect[0],
        };
        let anchor = match format.align {
            1 => "middle",
            2 => "end",
            _ => "start",
        };
        // Vertical alignment in world units: the block of lines in the height (when the height is given).
        let scale = scale_of(&matrix).max(1e-9);
        let block_top = match format.line_align {
            1 => rect[1] + (rect[3] - total / scale) / 2.0,
            2 => rect[1] + rect[3] - total / scale,
            _ => rect[1],
        };
        let clip = self.plus.clip;
        for (i, line) in lines.iter().enumerate() {
            let top = block_top + (line_height * i as f64) / scale;
            let (x, y) = self.plus_dev((ax, top));
            // The baseline: the ascent below the top of the line.
            let (bx, by) = (x - f.size * ascent * rot.to_radians().sin(), y + f.size * ascent * rot.to_radians().cos());
            self.canvas.grow(&IDENTITY, &[(x, y)]);
            self.canvas.text(&IDENTITY, clip, &[bx], by, (bx, by), line, &f_rot, color, alpha, anchor);
            self.plus.draws += 1;
        }
    }

    /// EmfPlusDrawDriverString: glyphs (characters, when the options say the glyphs are Unicode) with their positions.
    fn plus_driver_string(&mut self, flags: u16, d: &[u8]) {
        let (Some(brush_id), Some(options), Some(has_matrix), Some(count)) = (u32_at(d, 0), u32_at(d, 4), u32_at(d, 8), u32_at(d, 12)) else { return };
        let count = (count as usize).min(100_000);
        if options & 1 == 0 {
            // The glyphs are indexes into the font; without the font they cannot be shown.
            self.plus.unsupported = true;
            return;
        }
        let Some(Obj::Font(font)) = self.plus_object(u32::from(flags & 0xFF)).cloned() else { return };
        let Some(brush) = self.plus_brush(flags, brush_id) else { return };
        let (color, alpha) = match &brush {
            Brush::Solid(c) => argb(*c),
            Brush::Linear { start, .. } => argb(*start),
            Brush::Hatch(fg, _) => argb(*fg),
            Brush::Radial { center_color, .. } => argb(*center_color),
            Brush::Texture => (0, 1.0),
        };
        let units: Vec<u16> = (0..count).filter_map(|i| u16_at(d, 16 + i * 2)).collect();
        let text = String::from_utf16_lossy(&units);
        let pos_at = 16 + ((count * 2 + 3) & !3);
        let Some(positions) = read_points(d, pos_at, count, false) else { return };
        let local = if has_matrix != 0 { matrix_at(d, pos_at + count * 8).unwrap_or(IDENTITY) } else { IDENTITY };
        let f = self.plus_font(&font);
        let matrix = then(local, self.plus_matrix());
        let rot = matrix[1].atan2(matrix[0]).to_degrees();
        let mut f_rot = f.clone();
        f_rot.angle = -rot;
        let xs_dev: Vec<Point> = positions.iter().map(|&p| apply(&matrix, p.0, p.1)).collect();
        if xs_dev.is_empty() {
            return;
        }
        // The positions are the origins of the glyphs at the baseline; one text element per glyph keeps their places.
        let clip = self.plus.clip;
        for (ch, &(x, y)) in text.chars().zip(xs_dev.iter()) {
            self.canvas.grow(&IDENTITY, &[(x, y)]);
            self.canvas.text(&IDENTITY, clip, &[x], y, (x, y), &ch.to_string(), &f_rot, color, alpha, "start");
        }
        self.plus.draws += 1;
    }
}

