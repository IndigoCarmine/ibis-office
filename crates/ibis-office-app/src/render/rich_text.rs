// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A paragraph whose runs differ in size, laid out and wrapped as one text.
//!
//! GPUI's styled text has one font size, so a paragraph that mixes sizes
//! (a large number with a small unit, a superscript note mark) is laid out
//! here: it is broken into lines across its runs, each line is as high as
//! its largest run, and the runs of a line sit on a shared baseline or line
//! up as `a:pPr/@fontAlgn` asks.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::*;
use ibis_doc_model::{FontAlign, TextAlign};

use crate::render::font_metrics::{POWERPOINT_SINGLE, natural_below, powerpoint_below_baseline};

/// A run of the paragraph, ready to draw.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub(crate) text: String,
    pub(crate) font: Font,
    /// The size drawn, in pixels (smaller than the run's size for a
    /// superscript or subscript).
    pub(crate) size: f32,
    pub(crate) color: Hsla,
    pub(crate) underline: bool,
    /// How the underline is drawn (`a:rPr/@u`).
    pub(crate) underline_kind: ibis_doc_model::Underline,
    /// How far the run is raised above the baseline, in pixels (negative
    /// lowers it).
    pub(crate) raise: f32,
    /// Added after each character, in pixels.
    pub(crate) spacing: f32,
    /// Background colour behind the characters (a highlighter).
    pub(crate) highlight: Option<Hsla>,
    /// Outline around the characters: colour and width in pixels.
    pub(crate) outline: Option<(Hsla, f32)>,
    /// Shadow behind the characters: colour and offset (dx, dy) in pixels.
    pub(crate) shadow: Option<(Hsla, f32, f32)>,
    /// The characters painted with a gradient or a pattern (`a:gradFill`,
    /// `a:pattFill`) instead of `color`, across the run's stretch of a line.
    pub(crate) fill: Option<ibis_doc_model::Fill>,
    /// The font's ascent and descent at `size`, in pixels.
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    /// A line through the characters (`a:rPr/@strike`).
    pub(crate) strike: ibis_doc_model::Strike,
    /// The run's size the strike is placed for, in pixels (the same for a
    /// small capital as for the capitals around it).
    pub(crate) strike_size: f32,
}

/// How much smaller PowerPoint draws a small capital than a capital.
pub(crate) const SMALL_CAPS_SCALE: f32 = 0.8;

/// `text` drawn with capitals `caps`: the pieces, each with the fraction of
/// the run's size it is drawn at. Each character becomes one (a letter whose
/// capital is several characters, like ß, is kept), so that the characters
/// keep their positions in the text.
pub(crate) fn capitals(text: &str, caps: ibis_doc_model::Caps) -> Vec<(String, f32)> {
    let upper = |c: char| {
        let mut u = c.to_uppercase();
        match (u.next(), u.next()) {
            (Some(one), None) => one,
            _ => c,
        }
    };
    match caps {
        ibis_doc_model::Caps::None => vec![(text.to_string(), 1.0)],
        ibis_doc_model::Caps::All => vec![(text.chars().map(upper).collect(), 1.0)],
        ibis_doc_model::Caps::Small => {
            let mut out: Vec<(String, f32)> = Vec::new();
            for c in text.chars() {
                let small = c.is_lowercase() && upper(c) != c;
                let k = if small { SMALL_CAPS_SCALE } else { 1.0 };
                match out.last_mut() {
                    Some((t, last)) if *last == k => t.push(upper(c)),
                    _ => out.push((upper(c).to_string(), k)),
                }
            }
            if out.is_empty() {
                out.push((String::new(), 1.0));
            }
            out
        }
    }
}

/// The lines through a piece's characters (`a:rPr/@strike`), as rectangles
/// (x, y, width, height) for a stretch from the baseline's start (`x`, `y`),
/// `advance` wide.
fn strike_rects(piece: &Piece, (x, y): (f32, f32), advance: f32) -> Vec<(f32, f32, f32, f32)> {
    let size = piece.strike_size;
    let thickness = (size * STRIKE_THICKNESS).max(0.5);
    let middle = y - size * STRIKE_HEIGHT;
    match piece.strike {
        ibis_doc_model::Strike::None => Vec::new(),
        ibis_doc_model::Strike::Single => vec![(x, middle - thickness / 2.0, advance, thickness)],
        ibis_doc_model::Strike::Double => {
            let gap = size * DOUBLE_STRIKE_GAP / 2.0;
            vec![(x, middle - gap - thickness / 2.0, advance, thickness), (x, middle + gap - thickness / 2.0, advance, thickness)]
        }
    }
}

/// Where PowerPoint puts a strike's middle above the baseline, and how thick
/// it draws it, in ems; and how far apart the middles of a double strike's
/// lines are (measured in PowerPoint's PDF: 20 pt Calibri and Yu Gothic, a
/// 1 pt line 4.35 pt above the baseline; a double strike's 2 pt apart).
const STRIKE_HEIGHT: f32 = 0.2175;
const STRIKE_THICKNESS: f32 = 0.05;
const DOUBLE_STRIKE_GAP: f32 = 0.1;

/// An underline's shape below the baseline, as PowerPoint for Mac draws it
/// (measured in its PDF at 14, 20, 28 and 32 pt, Calibri and 游ゴシック):
/// its lines are `t` thick, Calibri's underline thickness (0.0654 em) down
/// to whole points; a single line's top is the font's underline position
/// rounded to whole points below the baseline (`p`: Calibri's 0.1133 em,
/// 游ゴシック's 0.1001), a heavy one's, a double one's band's and a wave's `t`
/// higher; dashes are measured in `t` from where the underline starts.
/// Lengths here are in pixels.
enum UnderlineShape {
    /// Lines (top, height) across, dashed as `dashes` (on, off, …; none:
    /// solid).
    Lines { lines: Vec<(f32, f32)>, dashes: &'static [f32] },
    /// Zigzag bands: each band's top, how far it goes down and back (the
    /// zigzag's depth) and its vertical thickness; it peaks every `2t`.
    Wave { tops: Vec<f32>, depth: f32, thickness: f32 },
}

/// A run's underline thickness in pixels for a run `size` pixels high at
/// `px_per_point` (0: not rounded to points).
fn underline_thickness(size: f32, px_per_point: f32) -> f32 {
    const THICKNESS: f32 = 0.0654;
    if px_per_point > 0.0 {
        (size / px_per_point * THICKNESS).floor().max(1.0) * px_per_point
    } else {
        (size * THICKNESS).max(0.5)
    }
}

/// The underline position below the baseline, in ems: Calibri's, and
/// 游ゴシック's for East Asian text.
const UNDERLINE_POSITION: f32 = 0.1133;
const EAST_ASIAN_UNDERLINE_POSITION: f32 = 0.1001;

/// An underline's top below the baseline (`p` of `underline_shape`), in
/// pixels.
fn underline_position(piece: &Piece, px_per_point: f32) -> f32 {
    let em = if piece.text.chars().any(is_cjk) { EAST_ASIAN_UNDERLINE_POSITION } else { UNDERLINE_POSITION };
    let p = piece.strike_size * em;
    if px_per_point > 0.0 { (p / px_per_point).round() * px_per_point } else { p }
}

fn underline_shape(kind: ibis_doc_model::Underline, t: f32, p: f32, point: f32) -> UnderlineShape {
    use ibis_doc_model::Underline as U;
    const SOLID: &[f32] = &[];
    const DOTTED: &[f32] = &[1.0, 1.0];
    const DASH: &[f32] = &[3.0, 2.0];
    const LONG: &[f32] = &[7.0, 4.0];
    const DOT_DASH: &[f32] = &[3.0, 2.0, 1.0, 2.0];
    const DOT_DOT_DASH: &[f32] = &[3.0, 2.0, 1.0, 2.0, 1.0, 2.0];
    let thin = |dashes| UnderlineShape::Lines { lines: vec![(p, t)], dashes };
    let heavy = |dashes| UnderlineShape::Lines { lines: vec![(p - t, t)], dashes };
    match kind {
        U::Single | U::Words => thin(SOLID),
        U::Dotted => thin(DOTTED),
        U::Dash => thin(DASH),
        U::DashLong => thin(LONG),
        U::DotDash => thin(DOT_DASH),
        U::DotDotDash => thin(DOT_DOT_DASH),
        // The heavy dashed lines are as thick as the thin ones (measured).
        U::DottedHeavy => heavy(DOTTED),
        U::DashHeavy => heavy(DASH),
        U::DashLongHeavy => heavy(LONG),
        U::DotDashHeavy => heavy(DOT_DASH),
        U::DotDotDashHeavy => heavy(DOT_DOT_DASH),
        U::Heavy => UnderlineShape::Lines { lines: vec![(p - t, t + point.max(0.0))], dashes: SOLID },
        U::Double => {
            // Two lines at the top and the bottom of a band `2t` high.
            let line = (t / 2.0).max(point.min(t));
            UnderlineShape::Lines { lines: vec![(p - t, line), (p + t - line, line)], dashes: SOLID }
        }
        U::Wavy => UnderlineShape::Wave { tops: vec![p - t], depth: t, thickness: t },
        U::WavyHeavy => UnderlineShape::Wave { tops: vec![p - t], depth: t, thickness: 2.0 * t },
        U::WavyDouble => UnderlineShape::Wave { tops: vec![p - 2.0 * t, p], depth: t, thickness: t },
    }
}

/// Paint a stretch's underline and strike: from the baseline's start
/// (`x`, `y`), `advance` wide, the underline's dashes and waves running on
/// from `origin`; turned by `turn`.
fn paint_decorations(piece: &Piece, (x, y): (f32, f32), advance: f32, origin: f32, px_per_point: f32, turn: Option<Turn>, window: &mut Window) {
    let rect = |window: &mut Window, (rx, ry, rw, rh): (f32, f32, f32, f32)| match turn {
        Some(turn) => paint_turned_rect(window, (rx, ry, rw, rh), piece.color, turn),
        None => window.paint_quad(fill(Bounds::new(point(px(rx), px(ry)), size(px(rw), px(rh))), piece.color)),
    };
    if piece.underline && advance > 0.0 {
        let t = underline_thickness(piece.strike_size, px_per_point);
        let unit = t;
        let end = x + advance;
        match underline_shape(piece.underline_kind, t, underline_position(piece, px_per_point), px_per_point) {
            UnderlineShape::Lines { lines, dashes } => {
                for (top, height) in lines {
                    let top = y + top;
                    if dashes.is_empty() {
                        rect(window, (x, top, advance, height));
                        continue;
                    }
                    let period: f32 = dashes.iter().sum::<f32>() * unit;
                    let mut at = origin + ((x - origin) / period).floor() * period;
                    while at < end {
                        let mut d = at;
                        for (k, len) in dashes.iter().enumerate() {
                            let next = d + len * unit;
                            if k % 2 == 0 {
                                let (a, b) = (d.max(x), next.min(end));
                                if b > a {
                                    rect(window, (a, top, b - a, height));
                                }
                            }
                            d = next;
                        }
                        at += period;
                    }
                }
            }
            UnderlineShape::Wave { tops, depth, thickness } => {
                // A zigzag band: its top edge at a peak every `2t` from
                // where the underline starts, `depth` lower between, its
                // bottom edge `thickness` below its top edge.
                let half = unit;
                for top in tops {
                    let top = y + top;
                    let at = |a: f32| {
                        let phase = ((a - origin) / half).rem_euclid(2.0);
                        top + depth * (1.0 - (phase - 1.0).abs())
                    };
                    let mut xs = vec![x];
                    let mut k = ((x - origin) / half).floor() + 1.0;
                    while origin + k * half < end {
                        xs.push(origin + k * half);
                        k += 1.0;
                    }
                    xs.push(end);
                    let upper: Vec<(f32, f32)> = xs.iter().map(|&a| (a, at(a))).collect();
                    let mut corners: Vec<(f32, f32)> = upper.clone();
                    corners.extend(upper.iter().rev().map(|&(a, b)| (a, b + thickness)));
                    let corners: Vec<Point<Pixels>> = corners
                        .into_iter()
                        .map(|(a, b)| {
                            let (a, b) = turn.map_or((a, b), |t| t.apply(a, b));
                            point(px(a), px(b))
                        })
                        .collect();
                    if let Some(path) = crate::render::shape_view::polygon(&corners, None, true) {
                        window.paint_path(path, piece.color);
                    }
                }
            }
        }
    }
    for r in strike_rects(piece, (x, y), advance) {
        rect(window, r);
    }
}

/// The height of a line.
#[derive(Clone, Copy, Debug)]
pub(crate) enum LineRule {
    /// A multiple of the line's largest size.
    Multiple(f32),
    /// Exactly this many pixels.
    Fixed(f32),
}

#[derive(Clone, Debug)]
pub(crate) struct RichParagraph {
    pub(crate) pieces: Vec<Piece>,
    pub(crate) align: TextAlign,
    pub(crate) font_align: FontAlign,
    pub(crate) line: LineRule,
    pub(crate) wrap: bool,
    /// Where the first line starts, from the element's left edge (negative
    /// for a hanging indent); the other lines start at the edge.
    pub(crate) first_line: f32,
    /// The width the lines are broken to, in pixels, when it is known
    /// exactly (else the element's width, which the layout rounds to
    /// pixels, so that the lines would break differently at each zoom).
    pub(crate) room: Option<f32>,
    /// Pixels per point: the baselines are put on whole points from the
    /// top of the text, as PowerPoint puts them (0: not rounded).
    pub(crate) px_per_point: f32,
    /// The text the paragraph is in, which places it and which its
    /// baselines are rounded from the top of (`None`: its own top).
    pub(crate) text_top: Option<TextTop>,
    /// The bullet, when the text is turned (`TextBlock::turn`).
    pub(crate) bullet: Option<BulletMark>,
    /// Where the paragraph's characters and lines are painted, written at
    /// each paint for a caret, a selection or a click (`None`: not kept).
    pub(crate) positions: Option<Rc<RefCell<ParagraphPositions>>>,
    /// Whether this is the last paragraph of its text: its last line ends
    /// at its font's descent below the baseline instead of at the line's
    /// bottom (PowerPoint's text is as high as that, for a text centred or
    /// at the bottom of its box).
    pub(crate) last: bool,
    /// East Asian vertical writing (`eaVert`): the text is turned a quarter
    /// turn, its East Asian characters drawn upright.
    pub(crate) upright: bool,
    /// Lines from left to right in vertical writing (`mongolianVert`):
    /// placed mirrored across the text's area.
    pub(crate) mirror: bool,
    /// Where the paragraph's text is meant to start (window pixels, unrounded
    /// by layout): corrects for GPUI layout snapping to whole device pixels.
    pub(crate) at_x: Option<f32>,
}

/// The paragraphs of one text, placed together: the layout rounds each
/// paragraph's height to pixels, which adds up over the paragraphs (and
/// moves a text centred or at the bottom of its box), so the paragraphs
/// are painted from their unrounded heights instead.
#[derive(Clone, Debug, Default)]
pub(crate) struct TextBlock {
    /// Each paragraph's height with its spaces above and below (pixels,
    /// unrounded), recorded as it is prepainted.
    pub(crate) heights: Rc<RefCell<Vec<f32>>>,
    /// Where the text's first line starts (window pixels), set after the
    /// paragraphs are prepainted (`TextBlock::place`).
    pub(crate) top: Rc<std::cell::Cell<f32>>,
    /// Where the next paragraph starts below `top`, pixels (as they are
    /// painted in order).
    pub(crate) next: Rc<std::cell::Cell<f32>>,
    /// How the text is turned (window pixels), set with `top`.
    pub(crate) turn: Rc<std::cell::Cell<Option<Turn>>>,
    /// The area the text is placed in (top and height, window pixels), set
    /// with `top`: lines laid out from left to right (`mongolianVert`) are
    /// mirrored in it.
    pub(crate) area: Rc<std::cell::Cell<(f32, f32)>>,
}

impl TextBlock {
    /// Put the text in the area from `top` down `height` (window pixels),
    /// `anchor` of the way down the room left (0 top, 0.5 middle, 1 bottom).
    pub(crate) fn place(&self, top: f32, height: f32, anchor: f32) {
        let text: f32 = self.heights.borrow().iter().sum();
        self.top.set(top + (height - text) * anchor);
        self.area.set((top, height));
        self.next.set(0.0);
    }

    /// Turn the text as `turn` says, its area's middle at (`cx`, `cy`).
    pub(crate) fn turn(&self, cx: f32, cy: f32, turn: Option<TextTurn>) {
        let turn = turn
            .map(|t| {
                let mut trn = Turn::about(cx, cy, t.text).then(Turn::about(cx + t.frame.0, cy + t.frame.1, t.shape));
                trn.shape_turned = t.shape_turned;
                trn
            })
            .filter(Turn::turns);
        self.turn.set(turn);
    }
}

/// A turn of painted text, in window pixels: a rotation about some point
/// (`p` → `R·p + t`, clockwise as the screen's y runs down). Text is never
/// mirrored: PowerPoint turns a flipped shape's text instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Turn {
    pub(crate) cos: f32,
    pub(crate) sin: f32,
    pub(crate) tx: f32,
    pub(crate) ty: f32,
    pub(crate) shape_turned: bool,
}

impl Turn {
    /// `degrees` clockwise about (`cx`, `cy`).
    pub(crate) fn about(cx: f32, cy: f32, degrees: f32) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self { cos, sin, tx: cx - cos * cx + sin * cy, ty: cy - sin * cx - cos * cy, shape_turned: false }
    }

    /// This turn, then `next`.
    pub(crate) fn then(self, next: Turn) -> Self {
        let (tx, ty) = next.apply(self.tx, self.ty);
        Self {
            cos: next.cos * self.cos - next.sin * self.sin,
            sin: next.sin * self.cos + next.cos * self.sin,
            tx,
            ty,
            shape_turned: self.shape_turned || next.shape_turned,
        }
    }

    /// Where the turn takes (`x`, `y`).
    pub(crate) fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.cos * x - self.sin * y + self.tx, self.sin * x + self.cos * y + self.ty)
    }

    /// The point the turn takes to (`x`, `y`).
    pub(crate) fn invert(&self, x: f32, y: f32) -> (f32, f32) {
        let (dx, dy) = (x - self.tx, y - self.ty);
        (self.cos * dx + self.sin * dy, -self.sin * dx + self.cos * dy)
    }

    pub(crate) fn radians(&self) -> f32 {
        self.sin.atan2(self.cos)
    }

    /// Whether it moves anything.
    pub(crate) fn turns(&self) -> bool {
        self.sin.abs() > 1e-6 || self.cos < 0.0 || self.tx.abs() > 1e-3 || self.ty.abs() > 1e-3
    }

    /// Shift the pivot of the turn so that the text line's visual center (baseline - ascent / 2)
    /// stays in place, as PowerPoint rotates text about its line center rather than the box center.
    pub(crate) fn with_visual_pivot(self, baseline_y: f32, ascent: f32) -> Self {
        if !self.shape_turned {
            return self;
        }
        let one_minus_cos = 1.0 - self.cos;
        let det = 2.0 * one_minus_cos;
        if det.abs() < 1e-4 {
            return self;
        }
        let cy = (self.sin * self.tx + one_minus_cos * self.ty) / det;
        let visual_y = baseline_y - ascent / 2.0;
        let delta = (visual_y - cy) / 2.0;
        Self {
            cos: self.cos,
            sin: self.sin,
            tx: self.tx + delta * self.sin,
            ty: self.ty + delta * one_minus_cos,
            shape_turned: self.shape_turned,
        }
    }
}

/// How a text is turned with its shape (`TextBlock::place`): `text`
/// degrees about the middle of its area, then `shape` degrees about the
/// shape's middle, `frame` pixels from the area's middle (before turning).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextTurn {
    pub(crate) text: f32,
    pub(crate) shape: f32,
    pub(crate) frame: (f32, f32),
    pub(crate) shape_turned: bool,
}

/// A paragraph's bullet, painted with turned text (unturned text has its
/// bullet as an element of its own): its text and font, `x` from the
/// paragraph's left edge, and `drop` below the first line's baseline.
#[derive(Clone, Debug)]
pub(crate) struct BulletMark {
    pub(crate) text: String,
    pub(crate) font: Font,
    pub(crate) size: f32,
    pub(crate) color: Hsla,
    pub(crate) x: f32,
    pub(crate) drop: f32,
}

/// A paragraph's place in its text (`RichParagraph::text_top`).
#[derive(Clone, Debug)]
pub(crate) struct TextTop {
    pub(crate) block: TextBlock,
    pub(crate) index: usize,
    /// The space above and below this paragraph, pixels.
    pub(crate) before: f32,
    pub(crate) after: f32,
}

/// Where a painted paragraph's characters and lines are, in window
/// pixels, for drawing a caret or a selection and for finding the
/// character at a click; the same for every format.
///
/// Characters are counted as the document model counts them
/// (`TextBody::splice`): the Unicode scalar values of the paragraph's text,
/// its runs' texts joined in order. An empty paragraph has one line and no
/// characters.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // For the formats' text editing, which is moving to it.
pub(crate) struct ParagraphPositions {
    /// The paragraph's element.
    pub(crate) bounds: Bounds<Pixels>,
    /// Each character's left edge and advance, and its line.
    pub(crate) chars: Vec<CharPosition>,
    pub(crate) lines: Vec<LinePosition>,
    /// How the text is turned. `bounds`, `chars` and `lines` are in the
    /// text's own coordinates: the window's before this turn (the same
    /// when it is `None`). Map them with `to_window` to draw (a turned
    /// caret or selection is a turned rectangle), and a click with
    /// `text_point` before `index_at`.
    pub(crate) turn: Option<Turn>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CharPosition {
    pub(crate) x: f32,
    pub(crate) advance: f32,
    pub(crate) line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LinePosition {
    /// The characters of the line (`start..end`; the line's trailing spaces
    /// are in it).
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// Where the line starts (after alignment and indent) and its top,
    /// baseline and height.
    pub(crate) x: f32,
    pub(crate) top: f32,
    pub(crate) baseline: f32,
    pub(crate) height: f32,
}

#[allow(dead_code)] // For the formats' text editing, which is moving to it.
impl ParagraphPositions {
    /// (`x`, `y`) of the text's coordinates in the window.
    pub(crate) fn to_window(&self, x: f32, y: f32) -> (f32, f32) {
        self.turn.map_or((x, y), |t| t.apply(x, y))
    }

    /// A point of the window (`x`, `y`) in the text's coordinates.
    pub(crate) fn text_point(&self, x: f32, y: f32) -> (f32, f32) {
        self.turn.map_or((x, y), |t| t.invert(x, y))
    }

    /// The four corners (window pixels, clockwise from the top left before
    /// the turn) of a rectangle of the text's coordinates.
    pub(crate) fn corners(&self, x: f32, y: f32, w: f32, h: f32) -> [(f32, f32); 4] {
        [self.to_window(x, y), self.to_window(x + w, y), self.to_window(x + w, y + h), self.to_window(x, y + h)]
    }

    /// Where a caret before character `index` goes (`index` = the number
    /// of characters: after the last): its x and line.
    pub(crate) fn caret(&self, index: usize) -> (f32, usize) {
        if let Some(c) = self.chars.get(index) {
            // At a line's start, the caret is on that line.
            return (c.x, c.line);
        }
        match self.chars.last() {
            Some(c) => (c.x + c.advance, c.line),
            None => (self.lines.first().map_or(f32::from(self.bounds.origin.x), |l| l.x), 0),
        }
    }

    /// The character a click at (`x`, `y`) puts the caret before: on the
    /// line at `y` (the nearest one), before the character whose middle is
    /// right of `x` (after the line's last character when none is; a
    /// wrapped line's end is before its first character on the next line,
    /// so it is the end of its text without its trailing space).
    pub(crate) fn index_at(&self, x: f32, y: f32) -> usize {
        let Some(line) = self
            .lines
            .iter()
            .find(|l| y < l.top + l.height)
            .or(self.lines.last())
        else {
            return 0;
        };
        let wrapped = line.end < self.chars.len();
        for i in line.start..line.end {
            let c = self.chars[i];
            if x < c.x + c.advance / 2.0 {
                return i;
            }
        }
        if wrapped && line.end > line.start { line.end - 1 } else { line.end }
    }
}

/// Where the paragraphs of a text body are painted
/// (`shape_view::text_body_element`): one `ParagraphPositions` a paragraph,
/// in order, each written as its paragraph is painted.
#[derive(Clone, Debug, Default)]
pub(crate) struct TextPositions {
    pub(crate) paragraphs: Rc<RefCell<Vec<Rc<RefCell<ParagraphPositions>>>>>,
}

impl TextPositions {
    /// The slot of each of `count` paragraphs (made or dropped to match).
    pub(crate) fn slots(&self, count: usize) -> Vec<Rc<RefCell<ParagraphPositions>>> {
        let mut list = self.paragraphs.borrow_mut();
        list.resize_with(count, Default::default);
        list.clone()
    }
}

/// One character of the paragraph.
#[derive(Clone, Copy, Debug)]
struct Char {
    piece: usize,
    /// Byte range in the piece's text.
    start: usize,
    end: usize,
    c: char,
    advance: f32,
}

#[derive(Clone, Debug)]
struct Line {
    /// Range in the paragraph's characters.
    start: usize,
    end: usize,
    /// Without trailing spaces.
    width: f32,
    height: f32,
    /// Of the baseline, from the top of the line.
    baseline: f32,
    /// How far below the baseline the line's font reaches in single
    /// spacing (where the text ends after its last line).
    natural: f32,
}

#[derive(Default)]
struct Layout {
    width: Option<f32>,
    chars: Vec<Char>,
    lines: Vec<Line>,
}

pub(crate) fn rich_paragraph(paragraph: RichParagraph) -> RichParagraphElement {
    RichParagraphElement { paragraph: Rc::new(paragraph), layout: Default::default() }
}

pub(crate) struct RichParagraphElement {
    paragraph: Rc<RichParagraph>,
    layout: Rc<RefCell<Layout>>,
}

impl IntoElement for RichParagraphElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for RichParagraphElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, ()) {
        let paragraph = self.paragraph.clone();
        let layout = self.layout.clone();
        let id = window.request_measured_layout(Style::default(), move |known, available, window, _| {
            let width = if paragraph.wrap {
                known.width.or(match available.width {
                    AvailableSpace::Definite(w) => Some(w),
                    _ => None,
                })
            } else {
                None
            };
            let mut layout = layout.borrow_mut();
            *layout = lay_out(&paragraph, width.map(f32::from), window);
            let widest = layout.lines.iter().map(|l| l.width).fold(0.0, f32::max);
            let height = text_height(&paragraph, &layout.lines);
            size(known.width.unwrap_or(px(widest.ceil())), px(height))
        });
        (id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) {
        let paragraph = &self.paragraph;
        let box_width = f32::from(bounds.size.width);
        let stale = paragraph.wrap && self.layout.borrow().width.is_none_or(|w| (w - box_width).abs() > 0.5);
        if stale {
            *self.layout.borrow_mut() = lay_out(paragraph, paragraph.wrap.then_some(box_width), window);
        }
        if let Some(t) = &paragraph.text_top {
            let mut heights = t.block.heights.borrow_mut();
            if heights.len() <= t.index {
                heights.resize(t.index + 1, 0.0);
            }
            heights[t.index] = t.before + text_height(paragraph, &self.layout.borrow().lines) + t.after;
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let paragraph = &self.paragraph;
        let box_width = f32::from(bounds.size.width);
        let stale = {
            let layout = self.layout.borrow();
            paragraph.wrap && layout.width.is_none_or(|w| (w - box_width).abs() > 0.5)
        };
        if stale {
            *self.layout.borrow_mut() = lay_out(paragraph, paragraph.wrap.then_some(box_width), window);
        }
        let layout = self.layout.borrow();
        let origin = f32::from(bounds.origin.y);
        // Baselines are rounded from the top of the text, the paragraph
        // starting where the ones above end (not where the layout, rounded
        // to pixels, puts it).
        let (text_top, mut top) = match &paragraph.text_top {
            Some(t) => (t.block.top.get(), t.block.next.get() + t.before),
            None => (origin, 0.0),
        };
        if let Some(t) = &paragraph.text_top {
            t.block.next.set(top + text_height(paragraph, &layout.lines) + t.after);
        }
        let turn = paragraph.text_top.as_ref().and_then(|t| t.block.turn.get());
        let mut positions = paragraph.positions.as_ref().map(|_| ParagraphPositions { bounds, turn, ..Default::default() });
        let snap_device = |v: f32, sf: f32| {
            let d = v * sf;
            (if (d - d.trunc()).abs() == 0.5 { d.trunc() } else { d.round() }) / sf
        };
        let x_correction = match (paragraph.at_x, paragraph.align) {
            (Some(at_x), TextAlign::Left) => at_x - snap_device(at_x, window.scale_factor()),
            _ => 0.0,
        };
        let line_count = layout.lines.len();
        for (n, line) in layout.lines.iter().enumerate() {
            let start = if n == 0 { paragraph.first_line } else { 0.0 };
            let room = if paragraph.room == Some(0.0) { 0.0 } else { box_width } - start;
            // 両端揃え (but the last line) and 均等割り付け: the room left
            // shared out between the characters that show (a line of one is
            // centred).
            let shown_chars = layout.chars[line.start..line.end].iter().rposition(|c| !c.c.is_whitespace()).map_or(0, |k| k + 1);
            let shown_width: f32 = layout.chars[line.start..line.start + shown_chars].iter().map(|c| c.advance).sum();
            let spread = match paragraph.align {
                TextAlign::Distributed => true,
                TextAlign::Justify => n + 1 < line_count,
                _ => false,
            } && room > shown_width
                && paragraph.room != Some(0.0);
            let extra = if spread && shown_chars > 1 { (room - shown_width) / (shown_chars - 1) as f32 } else { 0.0 };
            let mut x = f32::from(bounds.origin.x)
                + x_correction
                + start
                + match paragraph.align {
                    TextAlign::Left | TextAlign::Justify => 0.0,
                    TextAlign::Distributed if shown_chars == 1 && spread => (room - shown_width) / 2.0,
                    TextAlign::Distributed => 0.0,
                    TextAlign::Center => (room - line.width) / 2.0,
                    TextAlign::Right => room - line.width,
                };
            let mut baseline = text_top + snap(top + line.baseline, paragraph.px_per_point);
            let mut line_top = text_top + top;
            if paragraph.mirror && let Some(t) = &paragraph.text_top {
                // Lines from left to right: each line where it would be
                // mirrored in the text's area (its characters are not).
                let (area_top, area_height) = t.block.area.get();
                let mirrored = area_top + area_height - (line_top - area_top) - line.height;
                baseline += mirrored - line_top;
                line_top = mirrored;
            }
            if let Some(p) = &mut positions {
                let n = p.lines.len();
                p.lines.push(LinePosition { start: line.start, end: line.end, x, top: line_top, baseline, height: line.height });
                let mut at = x;
                for c in &layout.chars[line.start..line.end] {
                    p.chars.push(CharPosition { x: at, advance: c.advance + extra, line: n });
                    at += c.advance + extra;
                }
            }
            let reference = reference(paragraph, &layout.chars[line.start..line.end]);
            let line_turn = turn.map(|t| t.with_visual_pivot(baseline, reference.0));
            if let (0, Some(turn), Some(b)) = (n, turn, &paragraph.bullet) {
                paint_turned_bullet(b, f32::from(bounds.origin.x) + b.x, baseline + b.drop, turn, window);
            }
            // Where text painted with a gradient or a pattern spans along
            // the line: each run of stretches with the same fill (pieces of
            // one run), from its first character to its last, and the line's
            // ascent and descent.
            let mut fill_boxes: Vec<(usize, usize, f32, f32)> = Vec::new();
            {
                let (mut k, mut at) = (line.start, x);
                while k < line.end {
                    let fill = &paragraph.pieces[layout.chars[k].piece].fill;
                    let start = (k, at);
                    while k < line.end && paragraph.pieces[layout.chars[k].piece].fill == *fill {
                        at += layout.chars[k].advance + extra;
                        k += 1;
                    }
                    if fill.is_some() {
                        fill_boxes.push((start.0, k, start.1, at));
                    }
                }
            }
            let fill_box = |i: usize| {
                fill_boxes.iter().find(|b| b.0 <= i && i < b.1).map(|b| (b.2, baseline - reference.0, (b.3 - b.2).max(1.0), (reference.0 + reference.1).max(1.0)))
            };
            // Where the underline being drawn started on this line (its
            // dashes and waves run on from there across the pieces).
            let ul_state: std::cell::Cell<Option<(f32, f32, ibis_doc_model::Underline, f32)>> = std::cell::Cell::new(None);
            let ul_origin = |piece: &Piece, x: f32| -> f32 {
                let key = (piece.underline_kind, piece.strike_size);
                match ul_state.get() {
                    Some((origin, end, kind, size)) if piece.underline && (kind, size) == key && (end - x).abs() < 0.5 => {
                        origin
                    }
                    _ => x,
                }
            };
            // Each stretch of the line that belongs to one piece.
            let mut i = line.start;
            while i < line.end {
                let piece_index = layout.chars[i].piece;
                let mut j = i;
                while j < line.end && layout.chars[j].piece == piece_index {
                    j += 1;
                }
                let piece = &paragraph.pieces[piece_index];
                let text = &piece.text[layout.chars[i].start..layout.chars[j - 1].end];
                let run = TextRun {
                    len: text.len(),
                    font: piece.font.clone(),
                    color: piece.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(text.to_string().into(), px(piece.size), &[run], None);
                let y = baseline - piece.raise + align_offset(paragraph.font_align, piece, reference);
                let turn = line_turn;
                let advance: f32 = layout.chars[i..j].iter().map(|c| c.advance + extra).sum();
                // A strike stops at the line's last character that shows
                // (not through the spaces a wrapped line ends with).
                let shown = layout.chars[line.start..line.end].iter().rposition(|c| !c.c.is_whitespace()).map_or(line.start, |k| line.start + k + 1);
                let end = j.min(shown).max(i);
                // (No room after the last character that shows.)
                let struck: f32 = layout.chars[i..end].iter().map(|c| c.advance + extra).sum::<f32>() - if end == shown && end > i { extra } else { 0.0 };
                let origin = ul_origin(piece, x);
                ul_state.set(piece.underline.then_some((origin, x + advance, piece.underline_kind, piece.strike_size)));
                if let (Some(fill), Some(area)) = (&piece.fill, fill_box(i))
                    && paint_filled(&shaped, text, piece, fill, (x, y), area, turn, paragraph.px_per_point, window)
                {
                    paint_decorations(piece, (x, baseline - piece.raise), struck, origin, paragraph.px_per_point, turn, window);
                    x += advance;
                    i = j;
                    continue;
                }
                if let Some(turn) = turn {
                    paint_turned(&shaped, text, piece, (x, y), advance, turn, paragraph.upright, window);
                    paint_decorations(piece, (x, baseline - piece.raise), struck, origin, paragraph.px_per_point, Some(turn), window);
                    x += advance;
                    i = j;
                    continue;
                }
                if let Some(color) = piece.highlight {
                    // Across the stretch, from the font's ascent to its descent.
                    let area = Bounds::new(point(px(x), px(y - piece.ascent)), size(px(advance), px(piece.ascent + piece.descent)));
                    window.paint_quad(fill(area, color));
                }
                if let Some((color, dx, dy)) = piece.shadow {
                    let at = point(px(x + dx), px(y + dy));
                    for run in &shaped.runs {
                        for glyph in &run.glyphs {
                            let _ = window.paint_glyph(at + glyph.position, run.font_id, glyph.id, px(piece.size), color);
                        }
                    }
                }
                if piece.spacing == 0.0 && extra == 0.0 {
                    let height = shaped.ascent + shaped.descent;
                    let _ = shaped.paint(point(px(x), px(y) - shaped.ascent), height, gpui_kit::TextAlign::Left, None, window, cx);
                } else if extra == 0.0 {
                    paint_spaced(&shaped, text, piece, point(px(x), px(y)), window);
                } else {
                    let spread = Piece { spacing: piece.spacing + extra, ..piece.clone() };
                    paint_spaced(&shaped, text, &spread, point(px(x), px(y)), window);
                }
                if let Some((color, width)) = piece.outline {
                    // The outline, over the text as PowerPoint strokes it: the
                    // text painted around itself in the outline's colour, out to
                    // half the line's width (the line straddles the edge).
                    let r = width / 2.0;
                    // Rings out to the line's half width, so the outline is solid.
                    let rings = (r / 0.75).ceil().max(1.0) as usize;
                    let spots = (1..=rings).flat_map(|ring| {
                        let radius = r * ring as f32 / rings as f32;
                        (0..12).map(move |k| (k as f32 / 12.0 * std::f32::consts::TAU).sin_cos()).map(move |(s, c)| (c * radius, s * radius))
                    });
                    for (ox, oy) in spots {
                        let at = point(px(x + ox), px(y + oy));
                        for run in &shaped.runs {
                            for glyph in &run.glyphs {
                                let _ = window.paint_glyph(at + glyph.position, run.font_id, glyph.id, px(piece.size), color);
                            }
                        }
                    }
                }
                paint_decorations(piece, (x, baseline - piece.raise), struck, origin, paragraph.px_per_point, None, window);
                x += advance;
                i = j;
            }
            top += line.height;
        }
        if let (Some(slot), Some(p)) = (&paragraph.positions, positions) {
            *slot.borrow_mut() = p;
        }
    }
}

/// Paint glyph `glyph` of `font_id` with its origin at (`x`, `y`) of the
/// text's coordinates, turned by `turn`: moved to where the turn takes its
/// origin and turned about it there (so its unturned box is where it ends
/// up, which is what clipping looks at).
fn paint_turned_glyph(window: &mut Window, (x, y): (f32, f32), font_id: FontId, glyph: GlyphId, size: f32, color: Hsla, turn: Turn) {
    let (mut tx, mut ty) = turn.apply(x, y);
    let is_net_rotated = (turn.cos - 1.0).abs() > 1e-3 || turn.sin.abs() > 1e-3;
    let is_orthogonal = turn.sin.abs() < 1e-3 || turn.cos.abs() < 1e-3;
    if is_net_rotated && is_orthogonal {
        let sf = window.scale_factor();
        tx = (tx * sf).round() / sf;
        ty = (ty * sf).round() / sf;
    }
    paint_glyph_at(window, (tx, ty), turn.radians(), font_id, glyph, size, color);
}

/// Where an upright East Asian character of vertical writing is painted:
/// its glyph at (`x`, `y`) of a line laid out across (`size` pixels) turned
/// back a quarter turn about the middle of its em square, which stays where
/// it is. The glyph's origin and the angle it is painted at.
fn upright_glyph((x, y): (f32, f32), size: f32, turn: Turn) -> ((f32, f32), f32) {
    // The em square: a full width across, from 0.88 em above the baseline
    // to 0.12 em below.
    let (cx, cy) = (x + size / 2.0, y - 0.38 * size);
    let (ox, oy) = (cx + (y - cy), cy - (x - cx));
    (turn.apply(ox, oy), turn.radians() - std::f32::consts::FRAC_PI_2)
}

/// Paint `glyph` with its origin at (`tx`, `ty`) (window pixels), turned
/// `angle` radians clockwise about its origin.
fn paint_glyph_at(window: &mut Window, (tx, ty): (f32, f32), angle: f32, font_id: FontId, glyph: GlyphId, size: f32, color: Hsla) {
    let scale = window.scale_factor();
    let pivot = point(ScaledPixels(tx * scale), ScaledPixels(ty * scale));
    let transformation = TransformationMatrix::unit()
        .translate(pivot)
        .rotate(Radians(angle))
        .translate(point(ScaledPixels(-pivot.x.0), ScaledPixels(-pivot.y.0)));
    let _ = window.paint_glyph_transformed(point(px(tx), px(ty)), font_id, glyph, px(size), color, transformation);
}

/// A rectangle of the text's coordinates painted turned.
fn paint_turned_rect(window: &mut Window, (x, y, w, h): (f32, f32, f32, f32), color: Hsla, turn: Turn) {
    let corners: Vec<Point<Pixels>> = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
        .into_iter()
        .map(|(a, b)| {
            let (a, b) = turn.apply(a, b);
            point(px(a), px(b))
        })
        .collect();
    if let Some(path) = crate::render::shape_view::polygon(&corners, None, true) {
        window.paint_path(path, color);
    }
}

/// A stretch of a turned line: `shaped` (of `text`, in `piece`) from its
/// baseline's start (`x`, `y`), `advance` wide, with its highlight and
/// outline (`paint_decorations` draws its underline and strike).
fn paint_turned(shaped: &ShapedLine, text: &str, piece: &Piece, (x, y): (f32, f32), advance: f32, turn: Turn, upright: bool, window: &mut Window) {
    if let Some(color) = piece.highlight {
        paint_turned_rect(window, (x, y - piece.ascent, advance, piece.ascent + piece.descent), color, turn);
    }
    let glyph_at = |index: usize, position: Point<Pixels>| {
        let before = text[..index.min(text.len())].chars().count() as f32;
        (x + f32::from(position.x) + piece.spacing * before, y + f32::from(position.y))
    };
    if let Some((color, width)) = piece.outline {
        let r = width / 2.0;
        let rings = (r / 0.75).ceil().max(1.0) as usize;
        for ring in 1..=rings {
            let radius = r * ring as f32 / rings as f32;
            for k in 0..12 {
                let (s, c) = (k as f32 / 12.0 * std::f32::consts::TAU).sin_cos();
                for run in &shaped.runs {
                    for glyph in &run.glyphs {
                        let (gx, gy) = glyph_at(glyph.index, glyph.position);
                        paint_turned_glyph(window, (gx + c * radius, gy + s * radius), run.font_id, glyph.id, piece.size, color, turn);
                    }
                }
            }
        }
    }
    for run in &shaped.runs {
        for glyph in &run.glyphs {
            let (gx, gy) = glyph_at(glyph.index, glyph.position);
            if glyph.is_emoji {
                // Emoji are drawn unturned, where they belong.
                let (tx, ty) = turn.apply(gx, gy);
                let _ = window.paint_emoji(point(px(tx), px(ty)), run.font_id, glyph.id, px(piece.size));
            } else if upright && text[glyph.index.min(text.len())..].chars().next().is_some_and(is_cjk) {
                let (at, angle) = upright_glyph((gx, gy), piece.size, turn);
                paint_glyph_at(window, at, angle, run.font_id, glyph.id, piece.size, piece.color);
            } else {
                paint_turned_glyph(window, (gx, gy), run.font_id, glyph.id, piece.size, piece.color, turn);
            }
        }
    }

}

/// A stretch of text painted with a gradient or a pattern (`fill`): its
/// glyphs as polygons, the fill laid over `area` (x, y, width, height: the
/// run's extent along the line), turned by `turn`. False (nothing painted)
/// when a glyph's outline cannot be read: the text is then drawn in its
/// colour.
#[allow(clippy::too_many_arguments)]
fn paint_filled(
    shaped: &ShapedLine,
    text: &str,
    piece: &Piece,
    fill: &ibis_doc_model::Fill,
    (x, y): (f32, f32),
    (bx, by, bw, bh): (f32, f32, f32, f32),
    turn: Option<Turn>,
    px_per_point: f32,
    window: &mut Window,
) -> bool {
    // The glyphs' contours, in the area's own coordinates.
    let mut contours: Vec<Vec<(f32, f32)>> = Vec::new();
    for run in &shaped.runs {
        let Some((data, index)) = window.text_system().font_data(run.font_id) else { return false };
        for glyph in &run.glyphs {
            let before = text[..glyph.index.min(text.len())].chars().count() as f32;
            let (gx, gy) = (x + f32::from(glyph.position.x) + piece.spacing * before, y + f32::from(glyph.position.y));
            let Some(polygons) = crate::render::glyph_outline::glyph_polygons(&data, index, glyph.id.0, piece.size) else { return false };
            contours.extend(polygons.into_iter().map(|p| p.into_iter().map(|(px_, py)| (gx + px_ - bx, gy + py - by)).collect()));
        }
    }
    let to_window = move |pts: &[(f32, f32)]| -> Vec<Point<Pixels>> {
        pts.iter()
            .map(|&(a, b)| {
                let (a, b) = (a + bx, b + by);
                let (a, b) = turn.map_or((a, b), |t| t.apply(a, b));
                point(px(a), px(b))
            })
            .collect()
    };
    match fill {
        ibis_doc_model::Fill::Pattern { background, .. } => {
            let windowed: Vec<Vec<Point<Pixels>>> = contours.iter().map(|c| to_window(c)).collect();
            let closed: Vec<(Vec<Point<Pixels>>, bool)> = windowed.iter().map(|c| (c.clone(), true)).collect();
            if let Some(path) = crate::render::shape_view::compound_path(&closed, None) {
                window.paint_path(path, crate::render::shape_view::hsla(*background));
            }
            // The pattern's cells from the area's corner, 1 pt each.
            let scale = px_per_point.max(0.01) / ibis_doc_model::EMU_PER_POINT as f32;
            let (ox, oy) = turn.map_or((bx, by), |t| t.apply(bx, by));
            crate::render::shape_view::paint_pattern(window, fill, &windowed, point(px(ox), px(oy)), scale);
        }
        _ => crate::render::gradient::paint_gradient(window, fill, &contours, (bw, bh), 1.0, &to_window),
    }
    true
}

/// A turned text's bullet, its baseline's start at (`x`, `y`).
fn paint_turned_bullet(b: &BulletMark, x: f32, y: f32, turn: Turn, window: &mut Window) {
    let run = TextRun { len: b.text.len(), font: b.font.clone(), color: b.color, background_color: None, underline: None, strikethrough: None };
    let shaped = window.text_system().shape_line(b.text.clone().into(), px(b.size), &[run], None);
    for run in &shaped.runs {
        for glyph in &run.glyphs {
            paint_turned_glyph(window, (x + f32::from(glyph.position.x), y + f32::from(glyph.position.y)), run.font_id, glyph.id, b.size, b.color, turn);
        }
    }
}

/// Paint `shaped` (the line of `text`) glyph by glyph from `baseline`,
/// with the piece's character spacing after each character.
fn paint_spaced(shaped: &ShapedLine, text: &str, piece: &Piece, baseline: Point<Pixels>, window: &mut Window) {
    for run in &shaped.runs {
        for glyph in &run.glyphs {
            let before = text[..glyph.index.min(text.len())].chars().count() as f32;
            let at = point(baseline.x + glyph.position.x + px(piece.spacing * before), baseline.y + glyph.position.y);
            let _ = if glyph.is_emoji {
                window.paint_emoji(at, run.font_id, glyph.id, px(piece.size))
            } else {
                window.paint_glyph(at, run.font_id, glyph.id, px(piece.size), piece.color)
            };
        }
    }

}

/// The ascent and descent of the largest piece among `chars`, which the
/// others line up with.
fn reference(paragraph: &RichParagraph, chars: &[Char]) -> (f32, f32) {
    chars
        .iter()
        .map(|c| &paragraph.pieces[c.piece])
        .map(|p| (p.ascent, p.descent))
        .fold((0.0, 0.0), |m, v| if v.0 + v.1 > m.0 + m.1 { v } else { m })
}

/// How far below the shared baseline `piece` goes to line up with the
/// largest piece of its line.
fn align_offset(align: FontAlign, piece: &Piece, reference: (f32, f32)) -> f32 {
    match align {
        FontAlign::Top => piece.ascent - reference.0,
        FontAlign::Center => ((piece.ascent - piece.descent) - (reference.0 - reference.1)) / 2.0,
        FontAlign::Bottom => reference.1 - piece.descent,
        // Auto provisionally uses offset 0 (the previous baseline placement);
        // Office's automatic alignment semantics are not yet verified.
        FontAlign::Auto | FontAlign::Baseline => 0.0,
    }
}

fn lay_out(paragraph: &RichParagraph, width: Option<f32>, window: &mut Window) -> Layout {
    let mut chars = Vec::new();
    for (index, piece) in paragraph.pieces.iter().enumerate() {
        if piece.text.is_empty() {
            continue;
        }
        let run = TextRun {
            len: piece.text.len(),
            font: piece.font.clone(),
            color: piece.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(piece.text.clone().into(), px(piece.size), &[run], None);
        let bounds: Vec<(usize, char)> = piece.text.char_indices().collect();
        for (k, &(start, c)) in bounds.iter().enumerate() {
            let end = bounds.get(k + 1).map_or(piece.text.len(), |b| b.0);
            let x0 = f32::from(shaped.x_for_index(start));
            let x1 = if end == piece.text.len() { f32::from(shaped.width) } else { f32::from(shaped.x_for_index(end)) };
            let raw = (x1 - x0).max(0.0);
            let adv = if c == ' ' && piece.spacing == 0.0 && piece.text.chars().all(|ch| ch == ' ') {
                // Office (DirectWrite/GDI) snaps space advances to 96 DPI half-pixels:
                let size_pt = piece.size / paragraph.px_per_point.max(1.0);
                if size_pt > 0.0 {
                    let em_adv = raw / piece.size;
                    let px96 = (size_pt * (96.0 / 72.0)) * em_adv;
                    let snapped_px96 = (px96 * 2.0).round() / 2.0;
                    let snapped_pt = snapped_px96 * (72.0 / 96.0);
                    snapped_pt * paragraph.px_per_point
                } else {
                    raw
                }
            } else {
                raw + piece.spacing
            };
            chars.push(Char { piece: index, start, end, c, advance: adv });
        }
    }
    // Text that fills its box exactly (full-width characters, a box sized
    // to them) stays on one line despite rounding: a fiftieth of a point
    // over the exact width, or half a pixel over the rounded one.
    let (room, slack) = match (width, paragraph.room) {
        (Some(_), Some(room)) => (Some(room), 0.02 * paragraph.px_per_point.max(0.05)),
        _ => (width, 0.5),
    };
    let breaks = break_lines(&chars, room, paragraph.first_line, slack);
    let lines = breaks
        .into_iter()
        .map(|(start, end)| {
            let visible = chars[start..end].iter().rposition(|c| !c.c.is_whitespace()).map_or(start, |p| start + p + 1);
            let width = chars[start..visible].iter().map(|c| c.advance).sum();
            // An empty paragraph's line is its (empty) first piece's.
            let (a, d, largest) = match paragraph.pieces.first() {
                Some(p) if start == end => (p.ascent, p.descent, p.size),
                _ => {
                    let (a, d) = reference(paragraph, &chars[start..end]);
                    (a, d, chars[start..end].iter().map(|c| paragraph.pieces[c.piece].size).fold(0.0, f32::max))
                }
            };
            let single = POWERPOINT_SINGLE * largest;
            let height = match paragraph.line {
                LineRule::Multiple(m) => m * largest,
                LineRule::Fixed(h) => h,
            };
            let natural = natural_below(single, a, d);
            let baseline = height - powerpoint_below_baseline(height, single, natural);
            Line { start, end, width, height, baseline, natural }
        })
        .collect();
    Layout { width, chars, lines }
}

/// The height of the paragraph's `lines`: the last paragraph of a text
/// ends at its last line's font's descent below the baseline.
fn text_height(paragraph: &RichParagraph, lines: &[Line]) -> f32 {
    let mut height: f32 = lines.iter().map(|l| l.height).sum();
    if let (true, Some(last)) = (paragraph.last, lines.last()) {
        height += last.baseline + last.natural - last.height;
    }
    height
}

/// `y` (pixels) on the nearest whole point, `px_per_point` pixels a point
/// (as it is when that is 0).
fn snap(y: f32, px_per_point: f32) -> f32 {
    if px_per_point > 0.0 { (y / px_per_point).round() * px_per_point } else { y }
}

/// Ranges of `chars` that make up each line within `width` (and `slack`
/// over it), the first line starting `first_line` in.
fn break_lines(chars: &[Char], width: Option<f32>, first_line: f32, slack: f32) -> Vec<(usize, usize)> {
    if chars.is_empty() {
        return vec![(0, 0)];
    }
    let Some(width) = width else { return vec![(0, chars.len())] };
    let mut lines = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    // The last place a line may end before the current character.
    let mut last_break = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if i > start && may_break_before(chars[i - 1].c, c.c) {
            last_break = Some(i);
        }
        // Spaces, and Japanese commas and full stops (PowerPoint's hanging
        // punctuation), may hang past the edge.
        let room = if lines.is_empty() { width - first_line } else { width };
        if !c.c.is_whitespace() && !hangs(c.c) && used + c.advance > room + slack && i > start {
            let end = last_break.filter(|&b| b > start).unwrap_or(i);
            lines.push((start, end));
            start = end;
            used = chars[start..i].iter().map(|c| c.advance).sum();
            last_break = None;
            continue;
        }
        used += c.advance;
        i += 1;
    }
    lines.push((start, chars.len()));
    lines
}

/// Whether a line may end between `before` and `after`.
fn may_break_before(before: char, after: char) -> bool {
    if after.is_whitespace() {
        return false;
    }
    if before.is_whitespace() {
        return true;
    }
    if !(is_cjk(before) || is_cjk(after)) {
        return false;
    }
    // Kinsoku: characters that do not start or end a line.
    const NO_START: &str = "、。，．・：；？！ー―‐〜）」』】〕〉》］｝〙〗’”ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ々ゝゞヽヾ%％),.:;?!]}"; // i18n-ok（禁則処理の文字集合（表示文字ではない））
    const NO_END: &str = "（「『【〔〈《［｛〘〖‘“([{"; // i18n-ok（禁則処理の文字集合（表示文字ではない））
    !NO_START.contains(after) && !NO_END.contains(before)
}

/// Punctuation that may hang past the end of a line.
fn hangs(c: char) -> bool {
    matches!(c, '、' | '。' | '，' | '．')
}

pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x2e80..=0x9fff | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0xff00..=0xffef | 0x20000..=0x3ffff)
}

#[cfg(test)]
mod tests {
    use super::{Char, CharPosition, LinePosition, ParagraphPositions, break_lines};

    /// "ab c" wrapped after "ab " (10 px a character), then "de".
    fn positions() -> ParagraphPositions {
        let chars = [(0.0, 0), (10.0, 0), (20.0, 0), (0.0, 1), (10.0, 1)]
            .map(|(x, line)| CharPosition { x, advance: 10.0, line })
            .to_vec();
        let lines = vec![
            LinePosition { start: 0, end: 3, x: 0.0, top: 0.0, baseline: 8.0, height: 12.0 },
            LinePosition { start: 3, end: 5, x: 0.0, top: 12.0, baseline: 20.0, height: 12.0 },
        ];
        ParagraphPositions { chars, lines, ..Default::default() }
    }

    #[test]
    fn turns_compose_and_invert() {
        use super::Turn;
        let close = |a: (f32, f32), b: (f32, f32)| assert!((a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3, "{a:?} {b:?}");
        // A quarter turn clockwise about (10, 10): right of it goes below.
        let q = Turn::about(10.0, 10.0, 90.0);
        close(q.apply(20.0, 10.0), (10.0, 20.0));
        close(q.invert(10.0, 20.0), (20.0, 10.0));
        // Half a turn about one point, then back about another: a move.
        let t = Turn::about(0.0, 0.0, 180.0).then(Turn::about(5.0, 0.0, 180.0));
        close(t.apply(1.0, 1.0), (11.0, 1.0));
        assert!(t.turns() && t.radians().abs() < 1e-3);
        assert!(!Turn::about(3.0, 4.0, 0.0).turns());
    }

    #[test]
    fn caret_positions() {
        let p = positions();
        assert_eq!(p.caret(0), (0.0, 0));
        assert_eq!(p.caret(3), (0.0, 1));
        assert_eq!(p.caret(5), (20.0, 1));
    }

    #[test]
    fn click_positions() {
        let p = positions();
        assert_eq!(p.index_at(4.0, 5.0), 0);
        assert_eq!(p.index_at(6.0, 5.0), 1);
        // Past a wrapped line's end: before its trailing space.
        assert_eq!(p.index_at(100.0, 5.0), 2);
        assert_eq!(p.index_at(100.0, 15.0), 5);
        assert_eq!(p.index_at(12.0, 100.0), 4);
    }

    fn chars(text: &str, advance: f32) -> Vec<Char> {
        text.char_indices().map(|(i, c)| Char { piece: 0, start: i, end: i + c.len_utf8(), c, advance }).collect()
    }

    #[test]
    fn breaks_at_spaces() {
        let c = chars("ab cd ef", 1.0);
        // The space after "cd" hangs past the edge.
        assert_eq!(break_lines(&c, Some(5.0), 0.0, 0.5), vec![(0, 6), (6, 8)]);
    }

    #[test]
    fn breaks_between_japanese_characters_but_not_before_punctuation() {
        let c = chars("あいうーえお", 1.0);
        // "うー" stays together: the first line breaks before "う".
        assert_eq!(break_lines(&c, Some(3.0), 0.0, 0.5), vec![(0, 2), (2, 5), (5, 6)]);
    }

    #[test]
    fn a_hanging_indent_gives_the_first_line_more_room() {
        let c = chars("abcdef", 1.0);
        assert_eq!(break_lines(&c, Some(4.0), -2.0, 0.5), vec![(0, 6)]);
        assert_eq!(break_lines(&c, Some(4.0), 1.0, 0.5), vec![(0, 3), (3, 6)]);
    }

    #[test]
    fn a_comma_hangs_past_the_end_of_a_line() {
        let c = chars("あいう、え", 1.0);
        assert_eq!(break_lines(&c, Some(3.0), 0.0, 0.5), vec![(0, 4), (4, 5)]);
    }

    #[test]
    fn a_long_word_breaks_where_it_must() {
        let c = chars("abcdef", 1.0);
        assert_eq!(break_lines(&c, Some(4.0), 0.0, 0.5), vec![(0, 4), (4, 6)]);
    }
}
