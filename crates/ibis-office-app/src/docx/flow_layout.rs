// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Laying out a flowing document (Word) into pages: lines are broken with
//! the characters' widths in the typefaces the document asks for, and
//! paragraphs, table rows, headers and footers are placed on pages.
//!
//! ＭＳ 明朝 and ＭＳ ゴシック are fixed-width (a full-width character one em,
//! a half-width one half an em); where they are not installed the lines are
//! still broken by those widths, and each character is drawn in its place
//! with the substitute, so lines end where Word ends them.

use std::sync::Arc;

use gpui_kit::*;
use ibis_doc_model::{
    Block, CellAnchor, PreferredWidth, EMU_PER_POINT, Emu, FlowAlign, FlowBorder, FlowDocument, FlowParagraph, FlowRunKind, FlowRunStyle,
    FlowTable, LineHeight, ListSuffix, MediaId, Rgb, RowHeight, SectionBreak, ShadingPattern, TabAlign, TextStyle, UnderlineStyle, VerticalMerge, VerticalPosition,
};

use crate::docx::art_border::{self, ArtPart};
use crate::render::shape_view::{Resources, hsla};

/// Something drawn on a page, in points from the page's top-left corner.
#[derive(Clone, Debug)]
pub(crate) enum Item {
    /// Characters starting at `x` on the baseline `y`, each at its own
    /// offset from `x`.
    Text { x: f32, y: f32, text: SharedString, offsets: Arc<[f32]>, font: Font, size: f32, color: Hsla },
    /// Characters on a baseline from `x`, `y` turned a quarter (`turns`: 1
    /// clockwise, -1 anticlockwise) about that point (a cell's vertical
    /// text).
    Turned { x: f32, y: f32, text: SharedString, font: Font, size: f32, color: Hsla, turns: i8 },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Hsla },
    /// A picture of an art page border, `image_w` × `image_h` from (`x`,
    /// `y`), cut to the `w` × `h` there; `tint` recolours a grey one.
    Art { art: ibis_doc_model::ArtBorder, part: ArtPart, x: f32, y: f32, w: f32, h: f32, image_w: f32, image_h: f32, tint: Option<Rgb> },
    Picture { media: MediaId, x: f32, y: f32, w: f32, h: f32 },
    Shape { shape: Box<ibis_doc_model::Shape>, x: f32, y: f32, w: f32, h: f32 },
    /// A straight line from (`x`, `y`) to (`x2`, `y2`), `w` wide.
    Stroke { x: f32, y: f32, x2: f32, y2: f32, w: f32, color: Hsla },
    /// Not drawn: where the positions of a line of paragraph `para` are,
    /// for the caret and the selection. The line's box starts at `x`, `y`
    /// and is `h` high; `stops` are the positions on it and their distance
    /// from `x` (the last one is where the line's text ends).
    Line { para: u32, x: f32, y: f32, h: f32, stops: Arc<[(usize, f32)]>, vertical: bool },
    /// A two-color shading pattern across `(x, y, w, h)`, expanded into
    /// absolute page coordinates before rendering.
    Shading { pattern: ShadingPattern, color: Hsla, x: f32, y: f32, w: f32, h: f32 },
    /// A comment speech-bubble icon drawn outside the right edge of the paper.
    CommentBubble { x: f32, y: f32, size: f32 },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Page {
    pub(crate) items: Vec<Item>,
    /// Number of items that belong to the document body (before headers and footers).
    pub(crate) body_items_count: usize,
    /// The paper's size (points), its section's.
    pub(crate) width: f32,
    pub(crate) height: f32,
    /// The section the page is in (its index), its page number, and whether
    /// it is the section's first page.
    pub(crate) section: usize,
    pub(crate) number: u32,
    pub(crate) first_in_section: bool,
    /// The footnotes whose marks are on the page (drawn at its bottom).
    pub(crate) notes: Vec<u32>,
}

/// The pages of a document.
#[derive(Debug, Default)]
pub(crate) struct Pagination {
    pub(crate) pages: Vec<Page>,
}

pub(crate) fn pt(emu: i64) -> f32 {
    emu as f32 / EMU_PER_POINT as f32
}

/// How far right of where its margins put it Word draws a page's content:
/// none (the coverage tests' references, Word's PDFs: the leftmost text of
/// DBDY, DRUN, DCEL, DPAR and DTBB at 36.0 pt, the margin). It was 0.49 pt
/// (an earlier measurement on Word for Mac's PDF of one document), which
/// the coverage tests' strict matches show was wrong (357 → 405 at 0).
const TEXT_ORIGIN_X: f32 = 0.0;


/// Tracked changes display mode (Word's Display for Review: Simple, All, None, Original).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub(crate) enum MarkupDisplayMode {
    #[default]
    All = 0,        // すべての変更履歴 (All Markup)
    Simple = 1,     // シンプルな変更履歴 (Simple Markup)
    None = 2,       // 変更履歴なし (No Markup / 最終版)
    Original = 3,   // 元の文書 (Original / 原文)
}

static MARKUP_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub(crate) fn markup_display_mode() -> MarkupDisplayMode {
    match MARKUP_MODE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => MarkupDisplayMode::Simple,
        2 => MarkupDisplayMode::None,
        3 => MarkupDisplayMode::Original,
        _ => MarkupDisplayMode::All,
    }
}

pub(crate) fn clear_layout_caches() {
    PIECES.with(|c| c.borrow_mut().clear());
    PAGINATIONS.with(|c| c.borrow_mut().clear());
}

pub(crate) fn set_markup_display_mode(mode: MarkupDisplayMode) {
    MARKUP_MODE.store(mode as u8, std::sync::atomic::Ordering::Relaxed);
    clear_layout_caches();
}

pub(crate) fn showing_markup() -> bool {
    matches!(markup_display_mode(), MarkupDisplayMode::All | MarkupDisplayMode::Simple)
}

/// Show tracked changes or not: the pages are laid out again.
#[allow(dead_code)]
pub(crate) fn show_markup(on: bool) {
    set_markup_display_mode(if on { MarkupDisplayMode::All } else { MarkupDisplayMode::None });
}

static SHOW_HIDDEN_TEXT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn showing_hidden_text() -> bool {
    SHOW_HIDDEN_TEXT.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn set_show_hidden_text(on: bool) {
    SHOW_HIDDEN_TEXT.store(on, std::sync::atomic::Ordering::Relaxed);
    clear_layout_caches();
}

static SHOWN_REVIEWERS: std::sync::Mutex<Option<std::collections::HashSet<String>>> = std::sync::Mutex::new(None);

pub(crate) fn is_reviewer_shown(author: &str) -> bool {
    let guard = SHOWN_REVIEWERS.lock().unwrap();
    guard.as_ref().map_or(true, |set| set.contains(author))
}

pub(crate) fn set_shown_reviewers(reviewers: Option<std::collections::HashSet<String>>) {
    *SHOWN_REVIEWERS.lock().unwrap() = reviewers;
    clear_layout_caches();
}

/// The colour tracked changes are shown in (Word's first author's).
const REVISION_COLOR: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0xC00000);
/// The colour tracked moves are shown in (Word's standard green).
pub(crate) const MOVE_REVISION_COLOR: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0x008000);

/// The highlight colour for commented text ranges (Word-compatible pale amber/yellow).
pub(crate) const COMMENT_HIGHLIGHT_COLOR: ibis_doc_model::Color = ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0xfff2ac), alpha: 180 };
/// The background colour of the comment indicator bubble in the margin.
#[allow(dead_code)]
pub(crate) const COMMENT_BUBBLE_BG: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0xfef7e0);
/// The border colour of the comment indicator bubble in the margin.
#[allow(dead_code)]
pub(crate) const COMMENT_BUBBLE_BORDER: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0xf9ab00);
/// The icon colour inside the comment indicator bubble in the margin.
pub(crate) const COMMENT_BUBBLE_ICON: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0xd9822b);

/// The Latin typeface drawn for ＭＳ 明朝's half-width characters.
const MINCHO_LATIN: &str = "Times New Roman";

/// A typeface whose characters are one em (full-width) or half an em wide.
fn fixed_width(name: &str) -> bool {
    matches!(name, "ＭＳ 明朝" | "ＭＳ ゴシック" | "MS Mincho" | "MS Gothic" | "ＭＳ明朝" | "ＭＳゴシック") // i18n-ok（フォント名の判定（固定幅フォント）。訳さない）
}

/// A character drawn full-width in a Japanese fixed-width font.
fn full_width(c: char) -> bool {
    let u = c as u32;
    u >= 0x2000 && !(0xff61..=0xff9f).contains(&u) && !(0x2000..=0x200b).contains(&u)
}

/// Characters a line may not start with (closing brackets, punctuation,
/// The strict characters a line may not start with (禁則文字の設定の「厳密」: 小書き仮名や長音記号等を含む).
pub(crate) const STRICT_NO_LINE_START: &str = "、。，．・：；？！゛゜ヽヾゝゞ々ー）］｝」』】〕〉》’”)]}.,:;!?ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ―…‥％%"; // i18n-ok（行頭禁則の文字の集合。文字の並び）
/// The normal/standard characters a line may not start with (禁則文字の設定の「標準」: 小書き仮名や長音記号を行頭に許す).
pub(crate) const NORMAL_NO_LINE_START: &str = "、。，．・：；？！゛゜ヽヾゝゞ々）］｝」』】〕〉》’”)]}.,:;!?―…‥％%"; // i18n-ok（行頭禁則の文字の集合。文字の並び）
/// Backward compatible alias for strict line-breaking start characters.
#[allow(dead_code)]
pub(crate) const NO_LINE_START: &str = STRICT_NO_LINE_START;
/// The standard characters a line may not end with.
pub(crate) const NO_LINE_END: &str = "（［｛「『【〔〈《‘“([{"; // i18n-ok（行末禁則の文字の集合。文字の並び）

/// Characters a line may not start with (closing brackets, punctuation,
/// small kana when strict).
fn no_line_start(c: char, strict: bool) -> bool {
    if strict {
        STRICT_NO_LINE_START.contains(c)
    } else {
        NORMAL_NO_LINE_START.contains(c)
    }
}

/// Characters a line may not end with (opening brackets).
fn no_line_end(c: char) -> bool {
    NO_LINE_END.contains(c)
}

fn is_cjk(c: char) -> bool {
    c as u32 >= 0x2e80
}

/// One unit of a paragraph's text.
#[derive(Clone, Debug)]
enum Unit {
    /// `inset` centres the glyph in a fixed-width cell it does not fill.
    /// `line` and `ascent` are the height a line of the character's typeface
    /// takes and its ascent (points), as Word measures them.
    Char { c: char, width: f32, inset: f32, font: Font, size: f32, style: usize, line: f32, ascent: f32, lead: f32, auto_space: f32 },
    Tab,
    /// The tab after a list item's number: to the hanging indent (the text's
    /// left edge) when that is ahead, else as a tab.
    ListTab,
    Picture { media: MediaId, w: f32, h: f32 },
    Shape { shape: Box<ibis_doc_model::Shape>, w: f32, h: f32 },
    /// A typeset formula (`w`, `h` its width and height, `descent` the part of `h` below its baseline).
    Math { boxed: Arc<crate::render::math_view::MathBox>, w: f32, h: f32, descent: f32, font: Font, color: Hsla, display: bool },
    LineBreak,
    PageBreak,
    ColumnBreak,
}

/// A laid-out line: its height, where its baseline is, what is on it (x
/// from the paragraph's left edge) and whether a page break follows it.
#[derive(Clone, Debug, Default)]
struct LineBox {
    height: f32,
    baseline: f32,
    items: Vec<Item>,
    page_break: bool,
    column_break: bool,
    has_revision: bool,
    comment_marks: Vec<u32>,
}

#[derive(Clone, Debug)]
struct WrapObstacle {
    shape_left: f32,
    shape_right: f32,
    y_top: f32,
    y_bottom: f32,
    wrap_text: String,
    wrap_polygon: Option<(ibis_doc_model::WrapPolygon, f32, f32, f32, f32, f32, f32)>,
}

#[derive(Clone, Debug)]
struct WrapObstacleInPara {
    shape_left: f32,
    shape_right: f32,
    rel_top: f32,
    rel_bottom: f32,
    wrap_text: String,
    wrap_polygon: Option<(ibis_doc_model::WrapPolygon, f32, f32, f32, f32, f32, f32)>,
    shape_to_emit: Option<(ibis_doc_model::Shape, f32, f32, f32, f32, bool)>,
    drop_cap_to_emit: Option<Item>,
    /// Other obstacles on the page the lines keep off too (left, right, top,
    /// bottom; the text wraps round them on both sides).
    extra: Vec<(f32, f32, f32, f32)>,
}

/// A shape placed above its paragraph (see `Layouter::raised`): the page's
/// index, its obstacle, the shape as placed and its id.
type RaisedShape = (usize, WrapObstacle, Item, u64);

/// The paragraph's shape the text wraps round (square, tight, through),
/// but one of deleted text when the view leaves that out (`hide_deleted`).
fn square_shape(p: &FlowParagraph, hide_deleted: bool) -> Option<&ibis_doc_model::Shape> {
    p.runs.iter().filter(|r| !run_deleted(r, hide_deleted)).find_map(|r| match &r.kind {
        FlowRunKind::Shape(shape) if shape.floating.as_ref().is_some_and(|f| matches!(f.wrap.as_str(), "square" | "tight" | "through")) => Some(&**shape),
        _ => None,
    })
}

/// Whether a run is deleted text the view leaves out: when it hides
/// deletions (Simple, No Markup), and always a hidden reviewer's.
fn run_deleted(run: &ibis_doc_model::FlowRun, hide_deleted: bool) -> bool {
    run.revision.as_ref().is_some_and(|r| r.kind.is_delete() && (hide_deleted || !is_reviewer_shown(&r.author)))
}

pub(crate) struct Layouter<'r> {
    res: &'r Resources,
    doc: &'r FlowDocument,
    /// The numbers and bullets of list items, by paragraph.
    labels: std::collections::HashMap<u32, String>,
    /// How page numbers are written where laying out is (the section's).
    page_format: std::cell::RefCell<Option<String>>,
    /// The chapter number and its separator in front of the page number of the page whose header and
    /// footer are being laid out (`1-`; see `page_chapter`).
    chapter_prefix: std::cell::RefCell<String>,
    /// The line grid's pitch for the current section.
    line_pitch: std::cell::Cell<Option<Emu>>,
    /// The width (points) of the line the paragraph being laid out breaks into (`f32::MAX` where it is not known).
    line_room: std::cell::Cell<f32>,
    /// The number of pages of the document and of the current section, and
    /// the section's number, for `NUMPAGES`, `SECTIONPAGES` and `SECTION`,
    /// once the body is paginated (the headers and footers); before, the
    /// fields show their saved result.
    page_counts: std::cell::Cell<Option<(u32, u32, u32)>>,
    /// The body's: the number of pages and of each section's pages of a
    /// first laying out (for `NUMPAGES` and `SECTIONPAGES` in the body),
    /// and the section being laid out.
    known_pages: Option<(u32, Vec<u32>)>,
    body_section: std::cell::Cell<usize>,
    /// The footnotes' and endnotes' numbers (by the order of their marks),
    /// as shown.
    note_numbers: std::collections::HashMap<(bool, u32), String>,
    /// The number of the note being laid out, for its own mark.
    current_note: std::cell::RefCell<String>,
    /// 同じスタイルの段落間にスペースを追加しない: the paragraphs whose
    /// space before (first) or after (second) is not added, by paragraph.
    contextual: std::collections::HashMap<u32, (bool, bool)>,
    /// Floating shapes put off to the top of the next page (their paragraph's
    /// place left no room for them): their paragraphs leave them out.
    deferred_shapes: std::cell::RefCell<std::collections::HashSet<u64>>,
    /// How far the column being laid out is from the left margin (points),
    /// for placing floating shapes on the page from it.
    column_dx: std::cell::Cell<f32>,
    /// Shapes the text wraps round placed above their paragraph, over text
    /// laid out before them (the page index, the obstacle, the shape as
    /// placed, its id): a second laying out has them from the page's start.
    raised: std::cell::RefCell<Vec<RaisedShape>>,
    /// Those of a first laying out, placed at their pages' starts.
    seeds: Vec<RaisedShape>,
    /// The table and cell coordinate (table_id, row_index, grid_col_index) being laid out, for formula evaluation.
    current_cell: std::cell::Cell<Option<(u32, usize, usize)>>,
    /// The paragraph whose fields are being made into text (a `STYLEREF` in the body looks back from it).
    field_paragraph: std::cell::Cell<Option<u32>>,
    /// Active comment IDs covering run `(para_id, run_index)` in the document.
    comment_runs: std::collections::HashMap<(u32, usize), Vec<u32>>,
}

fn map_comment_runs(doc: &FlowDocument) -> std::collections::HashMap<(u32, usize), Vec<u32>> {
    let mut map: std::collections::HashMap<(u32, usize), Vec<u32>> = std::collections::HashMap::new();
    let mut active: Vec<u32> = Vec::new();
    for p in ibis_doc_model::all_paragraphs(doc) {
        for (idx, r) in p.runs.iter().enumerate() {
            match r.kind {
                FlowRunKind::CommentStart(id) => {
                    active.push(id);
                }
                FlowRunKind::CommentEnd(id) => {
                    active.retain(|&c| c != id);
                }
                FlowRunKind::CommentRef(_) => {}
                _ => {
                    if !active.is_empty() {
                        map.insert((p.id, idx), active.clone());
                    }
                }
            }
        }
    }
    map
}

/// The run index of a list item's number (drawn in `list_style`).
const LABEL: usize = usize::MAX;

#[derive(Clone, Copy)]
struct UnderlineBox {
    style: UnderlineStyle,
    size: f32,
    x: f32,
    w: f32,
    baseline: f32,
    color: Hsla,
}

#[derive(Clone, Copy)]
struct StrikeBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: Hsla,
}

#[derive(Clone, Copy)]
struct BorderBox {
    border: FlowBorder,
    x_start: f32,
    x_end: f32,
    y_top: f32,
    y_bottom: f32,
}

fn run_border_items(
    b: &FlowBorder,
    x_left: f32,
    y_top: f32,
    x_right: f32,
    y_bottom: f32,
) -> Vec<Item> {
    // A wave is a line along each side, as a paragraph's.
    if matches!(b.style, ibis_doc_model::BorderStyle::Wave | ibis_doc_model::BorderStyle::DoubleWave) {
        let half = pt(b.total()) / 2.0;
        let mut out = border_items(b, true, true, y_top + half, x_left, x_right, true, true);
        out.extend(border_items(b, true, false, y_bottom - half, x_left, x_right, true, true));
        out.extend(border_items(b, false, true, x_left + half, y_top, y_bottom, true, true));
        out.extend(border_items(b, false, false, x_right - half, y_top, y_bottom, true, true));
        return out;
    }
    let color = hsla(ibis_doc_model::Color::opaque(b.color));
    let mut out = Vec::new();
    let dashes = b.dashes().map(|d| d.into_iter().map(pt).collect::<Vec<f32>>());
    let mut offset = 0.0;
    for (width, drawn) in b.bands() {
        let t = pt(width).max(0.25);
        if drawn {
            let xl = x_left + offset;
            let xr = (x_right - offset).max(xl);
            let yt = y_top + offset;
            let yb = (y_bottom - offset).max(yt);
            let w = xr - xl;
            let h = yb - yt;
            if w > 0.0 && h > 0.0 {
                match &dashes {
                    Some(pattern) if pattern.iter().sum::<f32>() > 0.1 => {
                        let mut draw_dashed = |horizontal: bool, at: f32, start: f32, end: f32| {
                            let mut a = start;
                            let mut i = 0;
                            while a < end {
                                let len = pattern[i % pattern.len()];
                                if i % 2 == 0 {
                                    let z = (a + len).min(end);
                                    if horizontal {
                                        out.push(Item::Rect { x: a, y: at, w: z - a, h: t, color });
                                    } else {
                                        out.push(Item::Rect { x: at, y: a, w: t, h: z - a, color });
                                    }
                                }
                                a += len;
                                i += 1;
                            }
                        };
                        draw_dashed(true, yt, xl, xr);
                        draw_dashed(true, yb - t, xl, xr);
                        if h > 2.0 * t {
                            draw_dashed(false, xl, yt + t, yb - t);
                            draw_dashed(false, xr - t, yt + t, yb - t);
                        }
                    }
                    _ => {
                        out.push(Item::Rect { x: xl, y: yt, w, h: t, color });
                        out.push(Item::Rect { x: xl, y: yb - t, w, h: t, color });
                        if h > 2.0 * t {
                            out.push(Item::Rect { x: xl, y: yt + t, w: t, h: h - 2.0 * t, color });
                            out.push(Item::Rect { x: xr - t, y: yt + t, w: t, h: h - 2.0 * t, color });
                        }
                    }
                }
            }
        }
        offset += pt(width);
    }
    out
}

/// The height of a line with Hangul (ems) and how much of what it adds to a Latin or Japanese line is above the text.
const HANGUL_LINE: f32 = 1.51;
const HANGUL_ASCENT_SHARE: f32 = 0.8;

fn is_hangul(c: char) -> bool {
    matches!(c, '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}' | '\u{ac00}'..='\u{d7af}')
}

fn push_styled_text_items(
    items: &mut Vec<Item>,
    s: &ibis_doc_model::FlowRunStyle,
    x0: f32,
    y_base: f32,
    text: String,
    offsets: Vec<f32>,
    font: Font,
    size: f32,
    color: Hsla,
) {
    // Word 2010's effects: a glow and a shadow behind, the outline around (a ring of copies), a gradient fill
    // by the character.
    if let Some(e) = &s.effects {
        let rgba = |rgb: ibis_doc_model::Rgb, alpha: u8| {
            let mut c = hsla(ibis_doc_model::Color::opaque(rgb));
            c.a *= alpha as f32 / 255.0;
            c
        };
        let copy = |items: &mut Vec<Item>, dx: f32, dy: f32, color: Hsla| items.push(Item::Text { x: x0 + dx, y: y_base + dy, text: text.clone().into(), offsets: offsets.clone().into(), font: font.clone(), size, color });
        if let Some((radius, rgb, alpha)) = e.glow {
            let r = (radius as f32 / 12_700.0).min(1.5);
            if r > 0.0 {
                for ring in 1..=3 {
                    let d = r * ring as f32 / 3.0;
                    for k in 0..12 {
                        let angle = k as f32 * std::f32::consts::TAU / 12.0;
                        copy(items, d * angle.cos(), d * angle.sin(), rgba(rgb, (alpha as f32 * 0.25) as u8));
                    }
                }
            }
        }
        if let Some(sh) = e.shadow {
            let dist = sh.distance as f32 / 12_700.0;
            let angle = (sh.direction as f32 / 60_000.0).to_radians();
            let (dx, dy) = (dist * angle.cos(), dist * angle.sin());
            let blur = sh.blur as f32 / 12_700.0;
            if blur > 0.5 {
                for (ox, oy) in [(-blur / 3.0, 0.0), (blur / 3.0, 0.0), (0.0, -blur / 3.0), (0.0, blur / 3.0)] {
                    copy(items, dx + ox, dy + oy, rgba(sh.color, (sh.alpha as f32 * 0.25) as u8));
                }
            } else {
                copy(items, dx, dy, rgba(sh.color, sh.alpha));
            }
        }
        if let Some((width, rgb)) = e.outline {
            let r = (width as f32 / 12_700.0 / 2.0).max(0.25);
            for k in 0..8 {
                let angle = k as f32 * std::f32::consts::TAU / 8.0;
                copy(items, r * angle.cos(), r * angle.sin(), rgba(rgb, 255));
            }
        }
        if let Some((stops, _angle)) = &e.gradient {
            // The colour at a position across the run (left to right).
            let total = offsets.last().copied().unwrap_or(0.0).max(0.001);
            let at = |t: f32| -> ibis_doc_model::Rgb {
                let t = (t * 100_000.0).clamp(0.0, 100_000.0);
                let upper = stops.iter().position(|(p, _)| *p as f32 >= t).unwrap_or(stops.len() - 1);
                if upper == 0 {
                    return stops[0].1;
                }
                let (p0, c0) = stops[upper - 1];
                let (p1, c1) = stops[upper];
                let f = if p1 > p0 { ((t - p0 as f32) / (p1 - p0) as f32).clamp(0.0, 1.0) } else { 0.0 };
                let mix = |a: u32, b: u32| ((a as f32) * (1.0 - f) + (b as f32) * f).round() as u32;
                ibis_doc_model::Rgb((mix(c0.0 >> 16 & 255, c1.0 >> 16 & 255) << 16) | (mix(c0.0 >> 8 & 255, c1.0 >> 8 & 255) << 8) | mix(c0.0 & 255, c1.0 & 255))
            };
            for ((i, c), start) in text.char_indices().zip(offsets.iter()) {
                let _ = i;
                let width_of_char = offsets.get(text[..i].chars().count() + 1).map_or(0.0, |next| next - start);
                let colour = rgba(at((start + width_of_char / 2.0) / total), 255);
                items.push(Item::Text { x: x0 + start, y: y_base, text: c.to_string().into(), offsets: vec![0.0].into(), font: font.clone(), size, color: colour });
            }
            return;
        }
    }
    if s.shadow {
        let dx = (size * 0.05).max(0.6);
        let dy = (size * 0.05).max(0.6);
        let shadow_color = Hsla { h: color.h, s: color.s * 0.2, l: 0.5, a: 0.4 };
        items.push(Item::Text { x: x0 + dx, y: y_base + dy, text: text.clone().into(), offsets: offsets.clone().into(), font: font.clone(), size, color: shadow_color });
    }
    if s.outline {
        let od = (size * 0.03).max(0.5);
        for (ox, oy) in [(-od, 0.0), (od, 0.0), (0.0, -od), (0.0, od)] {
            items.push(Item::Text { x: x0 + ox, y: y_base + oy, text: text.clone().into(), offsets: offsets.clone().into(), font: font.clone(), size, color });
        }
        items.push(Item::Text { x: x0, y: y_base, text: text.into(), offsets: offsets.into(), font, size, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffffff))) });
    } else if s.emboss || s.imprint {
        // Word's PDF: the text in its colour with a copy of it in 60 % grey (#999999) a point down and right (an
        // emboss) or up and left (an imprint), behind it.
        let ed = if s.emboss { 1.0f32 } else { -1.0f32 };
        items.push(Item::Text { x: x0 + ed, y: y_base + ed, text: text.clone().into(), offsets: offsets.clone().into(), font: font.clone(), size, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x999999))) });
        // The strokes of the text itself are heavier than plain text's (a third more black on the page).
        for (ox, oy) in [(-0.3f32, -0.3f32), (0.3, -0.3), (-0.3, 0.3)] {
            items.push(Item::Text { x: x0 + ox, y: y_base + oy, text: text.clone().into(), offsets: offsets.clone().into(), font: font.clone(), size, color });
        }
        items.push(Item::Text { x: x0, y: y_base, text: text.into(), offsets: offsets.into(), font, size, color });
    } else {
        items.push(Item::Text { x: x0, y: y_base, text: text.into(), offsets: offsets.into(), font, size, color });
    }
}

#[derive(Default)]
struct LineDecorations {
    highlights: Vec<Item>,
    underlines: Vec<UnderlineBox>,
    strikes: Vec<StrikeBox>,
    borders: Vec<BorderBox>,
}

impl LineDecorations {
    fn finish(self, line: &mut LineBox) {
        let mut b_items = self.highlights;
        for u in self.underlines {
            b_items.extend(underline_items(u.style, u.size, u.x, u.w, u.baseline, u.color));
        }
        for b in self.borders {
            let extra_bw = (pt(b.border.total()) - pt(b.border.width)).max(0.0);
            let pad_h = pt(b.border.space) + 0.75 + extra_bw;
            let x_left = b.x_start - pad_h;
            let x_right = b.x_end + pad_h;
            let y_top = b.y_top;
            let y_bottom = b.y_bottom;
            b_items.extend(run_border_items(&b.border, x_left, y_top, x_right, y_bottom));
        }
        for s in self.strikes {
            b_items.push(Item::Rect { x: s.x, y: s.y, w: s.w, h: s.h, color: s.color });
        }
        let pos = line.items.iter().position(|it| matches!(it, Item::Text { .. })).unwrap_or(line.items.len());
        line.items.splice(pos..pos, b_items);
    }
}

impl<'r> Layouter<'r> {
    /// The default tab stops' spacing (points): the settings', else 42.55 pt (851 twips).
    fn default_tab_pt(&self) -> f32 {
        self.doc.default_tab_stop.map_or(851.0 / 20.0, pt)
    }

    /// Effective markup display mode for this layout pass (honours print_markup during printing DOCX-SEC-014).
    fn markup_mode(&self) -> MarkupDisplayMode {
        if self.res.print && !self.res.print_markup {
            MarkupDisplayMode::None
        } else {
            markup_display_mode()
        }
    }

    /// Whether this view leaves deleted text out.
    fn hides_deleted(&self) -> bool {
        matches!(self.markup_mode(), MarkupDisplayMode::Simple | MarkupDisplayMode::None)
    }

    /// The lines of the text in text boxes on the line grid (`w:docGrid type="lines"`), as Word puts
    /// them: each paragraph's line is as many `linePitch`es high as its text needs (a 14 pt 游明朝
    /// line in two grids of 20 pt), as the lines of the body text are.
    fn grid_text_boxes(&self, pagination: &mut Pagination) {
        let Some(pitch) = self.doc.page.line_pitch.filter(|p| *p > 0).map(pt) else { return };
        for page in &mut pagination.pages {
            for item in &mut page.items {
                let Item::Shape { shape, .. } = item else { continue };
                let body = match &mut shape.kind {
                    ibis_doc_model::ShapeKind::Auto(a) => a.text.as_mut(),
                    ibis_doc_model::ShapeKind::TextBox(body) => Some(body),
                    _ => None,
                };
                let Some(body) = body else { continue };
                if body.vertical {
                    continue;
                }
                // The first paragraph's baseline: where the text box draws it in an exact line, and
                // where Word does in a grid line (see `baseline_in_line`); the text is moved by the
                // difference.
                let mut shift: Option<f32> = None;
                for paragraph in &mut body.paragraphs {
                    if matches!(paragraph.line_spacing, Some(ibis_doc_model::LineSpacing::Points(_))) {
                        continue;
                    }
                    let multiple = match paragraph.line_spacing {
                        Some(ibis_doc_model::LineSpacing::Percent(n)) => n as f32 / 100_000.0,
                        _ => 1.0,
                    };
                    // The tallest line of the runs: its ascent, its line, the room Word leaves below it
                    // (游明朝: 1.43 em in all) and the size.
                    let mut tall = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                    for r in paragraph.runs.iter().filter(|r| !r.text.is_empty()) {
                        let size = r.style.font_size.unwrap_or(body.style.font_size) as f32 / 100.0;
                        let cjk = r.text.chars().any(crate::render::rich_text::is_cjk);
                        let name = if cjk { r.style.font.as_deref() } else { r.style.fallback_font.as_deref().or(r.style.font.as_deref()) };
                        let font = self.res.font(&TextStyle { font_size: (size * 100.0) as u32, bold: r.style.bold.unwrap_or(false), italic: r.style.italic.unwrap_or(false), font: name.map(str::to_string), ..TextStyle::default() });
                        let (ascent, line) = self.line_metrics(name, &font, size);
                        let mincho = name.map(str::trim).is_some_and(|n| n.starts_with("游明朝") || n.starts_with("Yu Mincho") || n.starts_with("YuMincho")); // i18n-ok（フォント名の判定（游明朝）。訳さない）
                        let lead = if mincho { 0.143 * size } else { Self::leading(name) * size };
                        if line + lead > tall.1 + tall.2 {
                            tall = (ascent, line, lead, size);
                        }
                    }
                    let (ascent, line, lead, size) = tall;
                    if line <= 0.0 {
                        continue;
                    }
                    let snapped = (((line + lead) * multiple - 0.01) / pitch).floor() * pitch + pitch;
                    paragraph.line_spacing = Some(ibis_doc_model::LineSpacing::Points((snapped * 100.0).round() as u32));
                    if shift.is_none() {
                        let grid = snapped - line * multiple;
                        let word = (snapped - grid - line) * 0.58 + grid * 0.5 + ascent;
                        let drawn = crate::render::font_metrics::powerpoint_baseline(snapped, crate::render::font_metrics::POWERPOINT_SINGLE * size, ascent, line - ascent);
                        shift = Some(word - drawn);
                    }
                }
                if let Some(shift) = shift {
                    body.insets.top += (shift * 12_700.0) as i64;
                }
            }
        }
    }

    fn resolve_line_height(&self, p: &FlowParagraph, base: f32, lead: f32, in_table: bool) -> f32 {
        self.line_height_parts(p, base, lead, in_table).0
    }

    /// A line's height, and the room of it the line grid adds (`w:docGrid`
    /// `type="lines"`: the height rounded up to whole `linePitch`es).
    fn line_height_parts(&self, p: &FlowParagraph, base: f32, lead: f32, in_table: bool) -> (f32, f32) {
        let snapping = !in_table && p.snap_to_grid && self.line_pitch.get().is_some_and(|pitch| pitch > 0) && !matches!(p.line, LineHeight::Exact(_));
        // `lead`: the room below the text off a grid (see `leading`).
        let base = if snapping { base } else { base + lead };
        let h = match p.line {
            LineHeight::Exact(exact) => return (pt(exact), 0.0),
            LineHeight::AtLeast(min) => pt(min).max(base),
            LineHeight::Auto(m) => base.max(base * m as f32 / 240.0),
        };
        if !in_table && p.snap_to_grid && let Some(pitch) = self.line_pitch.get() {
            let grid_pitch = pt(pitch);
            if grid_pitch > 0.0 {
                // How many grids the line takes counts the room Word leaves below the text
                // (游ゴシック・游明朝: 1.43 em, so a 28 pt line is 3 grids of 20 pt, not 2:
                // measured on Word's PDF of the title of the editing report), but where the
                // text sits in them does not (see `baseline_in_line`).
                let needed = match p.line {
                    LineHeight::Exact(_) => h,
                    LineHeight::AtLeast(min) => pt(min).max(base + lead),
                    LineHeight::Auto(m) => (base + lead).max((base + lead) * m as f32 / 240.0),
                };
                let snapped = ((needed - 0.01) / grid_pitch).floor() * grid_pitch + grid_pitch;
                return (snapped, snapped - h);
            }
        }
        (h, 0.0)
    }

    /// Where the baseline is in a line `height` high whose text is
    /// `text_line` high with `text_ascent`: 58 % of the room the line
    /// spacing gives beyond the text above it, and 0.87 em below that
    /// (measured on Word's PDF: 8 pt text in an exact 8.5 pt line, 12 pt in
    /// 18 pt); the room the line grid adds is split evenly above and below
    /// (Word's PDF of DBDY.p01: a label of 9 pt Arial and 游ゴシック, 11.58 pt
    /// high, in an 18 pt grid line has its baseline 12.0 pt down).
    fn baseline_in_line(&self, p: &FlowParagraph, natural: f32, lead: f32, text_line: f32, text_ascent: f32, in_table: bool) -> f32 {
        let (height, grid) = self.line_height_parts(p, natural, lead, in_table);
        // An exact line spacing leaves no room for the leading (DSEC-00011:
        // 10 pt 游ゴシック in an exact 15 pt line).
        let text_line = if grid > 0.0 || matches!(p.line, LineHeight::Exact(_)) { text_line } else { text_line + lead };
        // A multiple (`w:lineRule="auto"`) puts all of its extra room below the text
        // (DRUX-00730: 8 pt text in a double line sits at the top of it).
        let above = if matches!(p.line, LineHeight::Auto(_)) { 0.0 } else { 0.58 };
        (height - grid - text_line).max(0.0) * above + grid * 0.5 + text_ascent
    }

    fn font_for_size(&self, style: &FlowRunStyle, name: Option<&str>, size: u32) -> Font {
        self.res.font(&TextStyle {
            font_size: size,
            bold: style.bold,
            italic: style.italic,
            font: name.map(str::to_string),
            ..TextStyle::default()
        })
    }

    fn font(&self, style: &FlowRunStyle, name: Option<&str>) -> Font {
        self.font_for_size(style, name, style.size)
    }

    /// The ascent and the line height (points) of typeface `name` at `size`:
    /// Word's, from the font's Windows ascent and descent (`font_metrics`):
    /// the font drawn's when it is the typeface, else those Office gives the
    /// typeface (another font stands in for it), else the font drawn's.
    fn line_metrics(&self, name: Option<&str>, font: &Font, size: f32) -> (f32, f32) {
        let drawn = || {
            let (a, d, _) = self.res.line_metrics(font);
            (a, d)
        };
        let (a, d) = match name.map(str::trim) {
            // Word puts ＭＳ 明朝's and ＭＳ ゴシック's baseline 0.87 of the
            // line down, not at their Windows ascent (0.859): measured on
            // Word's PDF. Whether PowerPoint differs (an app's difference)
            // is being checked by the owner of `font_metrics`.
            Some(n) if fixed_width(n) || n.starts_with("ＭＳ") || n.starts_with("MS ") => (0.87, 0.13), // i18n-ok（フォント名の判定（ＭＳ 系）。訳さない）

            Some(n) => crate::render::font_metrics::office_metrics(n)
                .or_else(|| crate::render::font_metrics::office_metrics(&font.family))
                .unwrap_or_else(drawn),
            None => crate::render::font_metrics::office_metrics(&font.family).unwrap_or_else(drawn),
        };
        (a * size, (a + d) * size)
    }

    /// The room Word leaves below a line of typeface `name` beyond its
    /// ascent and descent when the line is not on a line grid, in ems.
    /// 游ゴシック: its lines are 1.43 em apart, not 1.287 em (its Windows
    /// ascent and descent), with the baseline at its ascent (measured on
    /// Word's PDFs: lines of 10 to 20 pt of a section without a grid in
    /// DSEC, of DBDY with snapToGrid=0 and of DPAR in table cells). On a
    /// grid the line is centred without it (DBDY-00007).
    fn leading(name: Option<&str>) -> f32 {
        match name.map(str::trim) {
            Some(n) if (n.starts_with("游ゴシック") || n.starts_with("Yu Gothic") || n.starts_with("YuGothic")) && !n.contains("UI") => 0.143, // i18n-ok（フォント名の判定（游ゴシック）。訳さない）
            _ => 0.0,
        }
    }

    /// The page counts and section number for fields where laying out is
    /// (a header's or footer's page, or the body's section).
    fn counts(&self) -> Option<(u32, u32, u32)> {
        self.page_counts.get().or_else(|| {
            let s = self.body_section.get();
            self.known_pages.as_ref().map(|(total, per)| (*total, per.get(s).copied().unwrap_or(0), s as u32 + 1))
        })
    }

    /// 囲い文字 (an `EQ \o` field): its character, smaller, in the middle
    /// of its enclosure drawn from `x0` (`width` wide) on baseline `y`.
    #[allow(clippy::too_many_arguments)]
    fn enclosed_items(&self, s: &FlowRunStyle, run: &ibis_doc_model::FlowRun, x0: f32, y: f32, width: f32, size: f32, color: Hsla) -> Vec<Item> {
        if run.is_field_code_shown(self.doc.show_field_codes) {
            return Vec::new();
        }
        let FlowRunKind::Field { instruction, .. } = &run.kind else { return Vec::new() };
        let Some((_, inner)) = ibis_doc_model::field_format::enclosed(instruction.as_str()) else { return Vec::new() };
        let count = inner.chars().count();
        let small = if count <= 1 { size * 0.7 } else { size * 0.5 };
        let w: f32 = inner.chars().map(|c| self.res.text_width(&c.to_string(), &self.font(s, s.font_for(c)), small)).sum();
        let font = self.font(s, s.font_for(inner.chars().next().unwrap_or('あ')));
        let offsets: Vec<f32> = inner.chars().scan(0.0f32, |at, c| {
            let x = *at;
            *at += self.res.text_width(&c.to_string(), &self.font(s, s.font_for(c)), small);
            Some(x)
        }).collect();
        // Centred on the enclosure: its middle 0.35 em above the baseline.
        vec![Item::Text { x: x0 + (width - w) / 2.0, y: y - size * 0.35 + small * 0.35, text: inner.into(), offsets: offsets.into(), font, size: small, color }]
    }

    /// 組み文字's room: its characters in two lines of half `size` (the
    /// first half, rounded up, on the first), the wider of them, and its
    /// brackets at `size`.
    fn combine_width(&self, s: &FlowRunStyle, chars: &[char], size: f32, c: ibis_doc_model::Combine) -> f32 {
        let measure = |list: &[char], at: f32| list.iter().map(|ch| self.res.text_width(&ch.to_string(), &self.font(s, s.font_for(*ch)), at)).sum::<f32>();
        let first = chars.len().div_ceil(2);
        let block = measure(&chars[..first], size / 2.0).max(measure(&chars[first..], size / 2.0));
        block + c.brackets().map_or(0.0, |(a, b)| measure(&[a, b], size))
    }

    /// 組み文字 drawn from `x0` on baseline `y`: the first line's baseline
    /// 0.517 em above, the second's 0.186 em below (Word's PDF of DW14 at
    /// 14 pt), between its brackets at the text's size.
    #[allow(clippy::too_many_arguments)]
    fn combine_items(&self, s: &FlowRunStyle, c: ibis_doc_model::Combine, text: &str, x0: f32, y: f32, size: f32, color: Hsla) -> Vec<Item> {
        let chars: Vec<char> = text.chars().filter(|ch| *ch != '\u{200b}').collect();
        let half = size / 2.0;
        let first = chars.len().div_ceil(2);
        let width = |ch: char, at: f32| self.res.text_width(&ch.to_string(), &self.font(s, s.font_for(ch)), at);
        let mut out = Vec::new();
        let mut x = x0;
        let line_w = |list: &[char]| list.iter().map(|ch| width(*ch, half)).sum::<f32>();
        let block = line_w(&chars[..first]).max(line_w(&chars[first..]));
        let mut glyph = |ch: char, x: f32, y: f32, at: f32| {
            let font = self.font(s, s.font_for(ch));
            out.push(Item::Text { x, y, text: ch.to_string().into(), offsets: Arc::from([0.0f32]), font, size: at, color });
        };
        if let Some((open, _)) = c.brackets() {
            glyph(open, x, y, size);
            x += width(open, size);
        }
        for (list, dy) in [(&chars[..first], -0.517), (&chars[first..], 0.186)] {
            // Each line centred in the block.
            let mut lx = x + (block - line_w(list)) / 2.0;
            for ch in list {
                glyph(*ch, lx, y + dy * size, half);
                lx += width(*ch, half);
            }
        }
        if let Some((_, close)) = c.brackets() {
            glyph(close, x + block, y, size);
        }
        out
    }

    /// ルビ over its base drawn from `x0` (`width` wide) on baseline `y`:
    /// its baseline `raise` above, centred, from the left or right, or
    /// spread over the base (均等割り付け: the first character at the
    /// base's left and the last at its right; 1-2-1: equal room around
    /// each), as Word's PDF of DW14 places them.
    fn ruby_items(&self, s: &FlowRunStyle, kind: &FlowRunKind, x0: f32, y: f32, width: f32, color: Hsla) -> Vec<Item> {
        use ibis_doc_model::RubyAlign;
        let FlowRunKind::Ruby { text, size, raise, align, .. } = kind else { return Vec::new() };
        if text.is_empty() {
            return Vec::new();
        }
        let style = FlowRunStyle { size: *size, ..s.clone() };
        let rs = *size as f32 / 100.0;
        let font = self.font(&style, style.font_for(text.chars().next().unwrap_or('あ')));
        let widths: Vec<f32> = text.chars().map(|c| self.res.text_width(&c.to_string(), &font, rs)).collect();
        let total: f32 = widths.iter().sum();
        let n = widths.len() as f32;
        let room = width - total;
        let (start, gap) = match align {
            RubyAlign::DistributeLetter if room > 0.0 && n > 1.0 => (0.0, room / (n - 1.0)),
            RubyAlign::DistributeSpace if room > 0.0 => (room / n / 2.0, room / n),
            RubyAlign::Left => (0.0, 0.0),
            RubyAlign::Right => (room, 0.0),
            _ => (room / 2.0, 0.0),
        };
        let mut offsets = Vec::with_capacity(widths.len());
        let mut at = start;
        for w in &widths {
            offsets.push(at);
            at += w + gap;
        }
        vec![Item::Text { x: x0, y: y - *raise as f32 / 100.0, text: text.clone().into(), offsets: offsets.into(), font, size: rs, color }]
    }

    /// 傍点 (`w:em`) over (or under) the characters of a run drawn at `x0`
    /// on baseline `y` (`offsets`: each character's start, then the end), as
    /// Word draws them (its PDF of DRUN, 14 pt): the mark is a character
    /// of the run's size — ． for a dot, 、 for a comma (their baseline
    /// 1.011 em above the text's), ˚ for a circle (0.291 em above), ． for a
    /// dot under (0.377 em below) — placed from each character's middle
    /// (0.166, 0.154 and 0.199 em to the left), none over spaces.
    #[allow(clippy::too_many_arguments)]
    fn emphasis_marks(&self, s: &FlowRunStyle, label: bool, x0: f32, y: f32, size: f32, text: &str, offsets: &[f32], color: Hsla) -> Vec<Item> {
        use ibis_doc_model::EmphasisMark;
        let (mark, up, left) = match s.emphasis {
            _ if label => return Vec::new(),
            EmphasisMark::None => return Vec::new(),
            EmphasisMark::Dot => ('．', 1.0114, 0.166),
            EmphasisMark::Comma => ('、', 1.0114, 0.166),
            EmphasisMark::Circle => ('˚', 0.2914, 0.154),
            EmphasisMark::UnderDot => ('．', -0.377, 0.199),
        };
        let font = self.font(s, s.font_for(if mark == '˚' { 'a' } else { 'あ' }));
        let one: Arc<[f32]> = Arc::from([0.0f32]);
        text.chars()
            .enumerate()
            .filter(|(_, c)| !c.is_whitespace() && *c != '\u{200b}')
            .filter_map(|(i, _)| {
                let (a, b) = (*offsets.get(i)?, *offsets.get(i + 1)?);
                let middle = x0 + (a + b) / 2.0;
                Some(Item::Text { x: middle - left * size, y: y - up * size, text: mark.to_string().into(), offsets: one.clone(), font: font.clone(), size, color })
            })
            .collect()
    }

    /// Look up a table by ID in the document's body, headers and footers.
    fn find_table_by_id<'a>(doc: &'a FlowDocument, table_id: u32) -> Option<&'a FlowTable> {
        fn search<'a>(blocks: &'a [Block], id: u32) -> Option<&'a FlowTable> {
            for b in blocks {
                match b {
                    Block::Table(t) => {
                        if t.id == id {
                            return Some(t);
                        }
                        for row in &t.rows {
                            for cell in &row.cells {
                                if let Some(found) = search(&cell.blocks, id) {
                                    return Some(found);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        search(&doc.body, table_id)
            .or_else(|| search(&doc.header, table_id))
            .or_else(|| search(&doc.footer, table_id))
            .or_else(|| search(&doc.header_first, table_id))
            .or_else(|| search(&doc.footer_first, table_id))
            .or_else(|| search(&doc.header_even, table_id))
            .or_else(|| search(&doc.footer_even, table_id))
    }

    /// What a field the layout computes shows on page `page_number` (`None`:
    /// its saved result): `PAGE` with its switches, the page counts and the
    /// section once known, a formula over them, `DATE` and `TIME`.
    fn field_text(&self, instruction: &str, page_number: u32) -> Option<String> {
        use ibis_doc_model::field_format::{compare_field_text, format_field_number, formula_text, if_field_text};
        let expanded_storage;
        let t = if instruction.contains('{') && instruction.contains('}') {
            expanded_storage = ibis_doc_model::expand_nested_fields(instruction, |inner| {
                match inner.split_whitespace().next() {
                    Some("PAGE") if !inner.contains('\\') => Some(page_number.to_string()),
                    _ => self.field_text(inner, page_number).or_else(|| {
                        Some(ibis_doc_model::flow_command::evaluate_field_instruction(inner, self.doc, ibis_doc_model::FlowPos::new(0, 0)))
                    }),
                }
            });
            expanded_storage.trim()
        } else {
            instruction.trim()
        };
        if t.starts_with('=') {
            if let Some((table_id, row, col)) = self.current_cell.get() {
                if let Some(table) = Self::find_table_by_id(self.doc, table_id) {
                    let resolver = ibis_doc_model::FlowTableCellResolver { table, row, col };
                    return ibis_doc_model::formula_text_with_resolver(t, Some(&resolver));
                }
            }
            return formula_text(t);
        }
        let t_upper = t.to_ascii_uppercase();
        if t_upper.starts_with("IF ") || t_upper == "IF" {
            return if_field_text(t);
        }
        if t_upper.starts_with("COMPARE ") || t_upper == "COMPARE" {
            return compare_field_text(t);
        }
        let counts = self.counts();
        let n = match t.split_whitespace().next()? {
            "PAGE" => {
                let text = format_field_number(page_number.into(), t).unwrap_or_else(|| match &*self.page_format.borrow() {
                    Some(f) => ibis_doc_model::format_page_number(page_number, f),
                    None => page_number.to_string(),
                });
                return Some(format!("{}{text}", self.chapter_prefix.borrow()));
            }
            "NUMPAGES" => counts?.0,
            "SECTIONPAGES" => counts?.1,
            "SECTION" => counts.map_or(self.body_section.get() as u32 + 1, |c| c.2),
            // DATE and TIME show the time now, as Word updates them when it
            // shows or prints the document.
            "DATE" | "TIME" => return ibis_doc_model::date_field::date_field_text(t, &crate::clock::local_now()),
            "CREATEDATE" => return self.doc.created_date_text(t),
            "SAVEDATE" => return self.doc.saved_date_text(t),
            "PRINTDATE" => return self.doc.print_date_text(t),
            "AUTHOR" => return Some(ibis_doc_model::apply_instruction_text_switches(self.doc.author_text(t), t)),
            "TITLE" => return Some(ibis_doc_model::apply_instruction_text_switches(self.doc.title_text(t), t)),
            "SUBJECT" => return Some(ibis_doc_model::apply_instruction_text_switches(self.doc.subject_text(t), t)),
            "COMMENTS" => return Some(ibis_doc_model::apply_instruction_text_switches(self.doc.comments_text(t), t)),
            "DOCPROPERTY" => return self.doc.doc_property_text(t).map(|v| ibis_doc_model::apply_instruction_text_switches(v, t)),
            "NOTEREF" => return self.doc.noteref_text(t).map(|v| ibis_doc_model::apply_instruction_text_switches(v, t)),
            "STYLEREF" => return self.doc.styleref_text_at(t, self.field_paragraph.get()).map(|v| ibis_doc_model::apply_instruction_text_switches(v, t)),
            "FILESIZE" => return Some(self.doc.file_size_text(t)),
            "DOCVARIABLE" => return Some(self.doc.doc_variable_text(t)),
            "USERNAME" => {
                let name = self.doc.properties.creator.clone().unwrap_or_else(crate::shell::user_name::user_name);
                return Some(ibis_doc_model::format_user_name(&name, t));
            }
            "USERINITIALS" => {
                let inits = self.doc.properties.creator.as_deref().map(ibis_doc_model::field_format::initials_from_name).unwrap_or_else(crate::shell::user_name::user_initials);
                return Some(ibis_doc_model::format_user_initials(&inits, t));
            }
            "USERADDRESS" => return Some(self.doc.user_address_text(t)),
            "QUOTE" => return Some(self.doc.quote_text(t)),
            "SYMBOL" => return Some(self.doc.symbol_text(t)),
            "SET" => return Some(String::new()),
            "IF" => return if_field_text(t),
            "COMPARE" => return compare_field_text(t),
            "HYPERLINK" => return Some(self.doc.hyperlink_text(t, "")),
            "TEMPLATE" => return Some(self.doc.template_text(t)),
            "ADVANCE" => return Some(String::new()),
            "INFO" => return self.doc.info_text(t),
            "ASK" => return Some(String::new()),
            "FILLIN" | "FILL-IN" => return Some(self.doc.fillin_text(t)),
            "AUTOTEXT" => return Some(ibis_doc_model::field_format::format_autotext(t)),
            "AUTOTEXTLIST" => return Some(ibis_doc_model::field_format::format_autotextlist(t)),
            "CITATION" => return Some(ibis_doc_model::field_format::format_citation(t)),
            "BIBLIOGRAPHY" => return Some(ibis_doc_model::field_format::format_bibliography(t)),
            _ => return None,
        };
        Some(format_field_number(n.into(), t).unwrap_or_else(|| n.to_string()))
    }

    /// A text box in the text that does not wrap its text (`wrap="none"`) is as wide as its longest line (and its
    /// margins) when that is less than its width, as Word draws it (DTXB: a 2-character box is 33 pt wide
    /// whatever the extent says); a longer line wraps at the extent.
    fn fit_unwrapped_text_box(&self, shape: &mut ibis_doc_model::Shape) {
        let frame_width = shape.frame.width;
        let body = match &mut shape.kind {
            ibis_doc_model::ShapeKind::Auto(a) => a.text.as_mut(),
            ibis_doc_model::ShapeKind::TextBox(body) => Some(body),
            _ => None,
        };
        let Some(body) = body else { return };
        if body.wrap || body.vertical || body.sideways != 0 {
            return;
        }
        let mut widest = 0.0f32;
        for paragraph in &body.paragraphs {
            let mut width = 0.0f32;
            for r in paragraph.runs.iter().filter(|r| !r.text.is_empty()) {
                let size = r.style.font_size.unwrap_or(body.style.font_size) as f32 / 100.0;
                let cjk = r.text.chars().any(crate::render::rich_text::is_cjk);
                let name = if cjk { r.style.font.as_deref() } else { r.style.fallback_font.as_deref().or(r.style.font.as_deref()) };
                let font = self.res.font(&TextStyle { font_size: (size * 100.0) as u32, bold: r.style.bold.unwrap_or(false), italic: r.style.italic.unwrap_or(false), font: name.map(str::to_string), ..TextStyle::default() });
                width += self.res.text_width(&r.text, &font, size);
            }
            widest = widest.max(width);
        }
        let natural = ((widest * 12_700.0).round() as i64) + body.insets.left + body.insets.right;
        if natural > 0 && natural < frame_width {
            shape.frame.width = natural;
        } else {
            // Too long for the box: it grows to the line it is on (the cell's width: DTXB, a 78.7 pt box in an
            // 89.4 pt cell is 89.4 pt wide) and wraps there.
            let room = self.line_room.get();
            if room < f32::MAX && pt(frame_width) < room {
                shape.frame.width = ((room * 12_700.0) as i64).min(natural);
            }
            body.wrap = true;
        }
    }

    /// A formula as a unit: typeset at the run's size in the run's typeface, as wide and as high as it comes. Word's math typeface
    /// is Cambria Math (the shared alias table finds a substitute for it).
    fn math_unit(&self, math: &ibis_doc_model::math::Math, display: bool, style: &ibis_doc_model::FlowRunStyle, size: f32) -> Unit {
        let font = self.font(style, Some("Cambria Math"));
        let mut measure = |text: &str, size: f32, italic: bool| {
            let mut f = font.clone();
            if italic {
                f.style = FontStyle::Italic;
            }
            let (ascent, descent, _) = self.res.line_metrics(&f);
            (self.res.text_width(text, &f, size), ascent * size, descent * size)
        };
        let boxed = crate::render::math_view::typeset(math, size, &mut measure);
        let (w, h, descent) = (boxed.width, boxed.ascent + boxed.descent, boxed.descent);
        let color = hsla(ibis_doc_model::Color::opaque(style.color));
        Unit::Math { boxed: Arc::new(boxed), w, h, descent, font, color, display }
    }

    /// A typeset formula as page items: its text where its pieces are (from `x` on the baseline `baseline`), its rules and lines.
    fn math_items(&self, boxed: &crate::render::math_view::MathBox, x: f32, baseline: f32, font: &Font, color: Hsla) -> Vec<Item> {
        use crate::render::math_view::Piece;
        boxed
            .pieces
            .iter()
            .map(|piece| match piece {
                Piece::Text { x: dx, y, text, size, italic } => {
                    let mut f = font.clone();
                    if *italic {
                        f.style = FontStyle::Italic;
                    }
                    let mut offsets = Vec::with_capacity(text.chars().count());
                    let mut at = 0.0;
                    for c in text.chars() {
                        offsets.push(at);
                        at += self.res.text_width(&c.to_string(), &f, *size);
                    }
                    Item::Text { x: x + dx, y: baseline + y, text: SharedString::from(text.clone()), offsets: offsets.into(), font: f, size: *size, color }
                }
                Piece::Rule { x: dx, y, w, h } => Item::Rect { x: x + dx, y: baseline + y, w: *w, h: *h, color },
                Piece::Line { x0, y0, x1, y1, t } => Item::Stroke { x: x + x0, y: baseline + y0, x2: x + x1, y2: baseline + y1, w: *t, color },
            })
            .collect()
    }

    /// The paragraph's text as units, with the page number for `PAGE`.
    fn units(&self, p: &FlowParagraph, page_number: u32) -> (Vec<Unit>, Vec<(usize, bool)>) {
        self.field_paragraph.set(Some(p.id));
        let mut out = Vec::new();
        // The position of each unit in the paragraph, and whether it is the
        // first unit of its position (a page number's digits are one).
        let mut positions: Vec<(usize, bool)> = Vec::new();
        let mut position = 0;
        // A list item's number or bullet, and what follows it: no positions.
        if let (Some(label), Some(list)) = (self.labels.get(&p.id), p.list) {
            let style = &p.list_style;
            let size = style.size as f32 / 100.0;
            // A picture bullet (`w:lvlPicBulletId`): the picture, as high as the text
            // (or as its shape says), not the bullet's character.
            let numbering = &self.doc.styles.numbering;
            let picture = numbering.level(list.num_id, list.level).and_then(|l| l.pic_bullet).and_then(|id| numbering.pic_bullets.get(&id));
            if let Some(&(media, shape)) = picture {
                let (w, h) = shape.map_or((size, size), |s| (pt(s.width), pt(s.height)));
                out.push(Unit::Picture { media, w, h });
                positions.push((0, false));
            }
            for c in label.chars().filter(|_| picture.is_none()) {
                // A bullet in the geometric shapes (●, ■, ◆…) defaults to Arial in
                // Word's PDF of DWID (small) if no bullet font was specified, not the Japanese font's.
                let name = style.font.as_deref().or_else(|| {
                    if matches!(c as u32, 0x2022 | 0x25A0..=0x25FF) { Some("Arial") } else { style.font_for(c) }
                });
                let font = self.font(style, name);
                let width = self.res.text_width(&c.to_string(), &font, size);
                let (mut ascent, mut line) = self.line_metrics(name, &font, size);
                if is_hangul(c) && line < size * HANGUL_LINE {
                    let extra = size * HANGUL_LINE - line;
                    ascent += extra * HANGUL_ASCENT_SHARE;
                    line += extra;
                }
                out.push(Unit::Char { c, width, inset: 0.0, font, size, style: LABEL, line, ascent, lead: Self::leading(name) * size, auto_space: 0.0 });
                positions.push((0, false));
            }
            let suffix = self.doc.styles.numbering.level(list.num_id, list.level).map(|l| l.suffix).unwrap_or_default();
            match suffix {
                ListSuffix::Tab => {
                    out.push(Unit::ListTab);
                    positions.push((0, false));
                }
                ListSuffix::Space => {
                    let font = self.font(style, style.font_for(' '));
                    let width = self.res.text_width(" ", &font, size);
                    let (ascent, line) = self.line_metrics(style.font_for(' '), &font, size);
                    out.push(Unit::Char { c: ' ', width, inset: 0.0, font, size, style: LABEL, line, ascent, lead: Self::leading(style.font_for(' ')) * size, auto_space: 0.0 });
                    positions.push((0, false));
                }
                ListSuffix::Nothing => {}
            }
        }
        if let Some(sub) = &p.subdoc {
            let style = &p.mark;
            let size = (style.size as f32 / 100.0).max(9.0);
            let lock_str = if sub.locked { " [ロック]" } else { "" }; // i18n-ok
            let expand_str = if sub.expanded && !sub.content_paragraphs.is_empty() {
                format!(" ({})", sub.content_paragraphs.join(" / "))
            } else {
                String::new()
            };
            let badge = if p.runs.is_empty() {
                format!("[サブ文書: {}{}{}]", sub.target, lock_str, expand_str) // i18n-ok
            } else {
                format!("[サブ文書: {}{}] ", sub.target, lock_str) // i18n-ok
            };
            for c in badge.chars() {
                let name = style.font_for(c);
                let font = self.font(style, name);
                let width = self.res.text_width(&c.to_string(), &font, size);
                let (ascent, line) = self.line_metrics(name, &font, size);
                out.push(Unit::Char { c, width, inset: 0.0, font, size, style: LABEL, line, ascent, lead: Self::leading(name) * size, auto_space: 0.0 });
                positions.push((0, false));
            }
        }
        for (index, run) in p.runs.iter().enumerate() {
            let before = out.len();
            let style = &run.style;
            let size = style.size as f32 / 100.0 * if style.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
            let half_points = (style.size + 25) / 50;
            let small_half_points = (half_points * 8 + 5) / 10;
            let _small_size = (small_half_points * 50) as f32 / 100.0 * if style.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
            let push_text = |text: &str, out: &mut Vec<Unit>| {
                for c in text.chars() {
                    let char_style_size = style.size_for(c);
                    let (base_size, char_half_points) = if char_style_size == style.size {
                        (size, half_points)
                    } else {
                        let s = char_style_size as f32 / 100.0 * if style.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
                        let hp = (char_style_size + 25) / 50;
                        (s, hp)
                    };
                    let (c, char_size) = if style.caps {
                        (c.to_uppercase().next().unwrap_or(c), base_size)
                    } else if style.small_caps && c.is_lowercase() {
                        let small_hp = (char_half_points * 8 + 5) / 10;
                        let small_sz = (small_hp * 50) as f32 / 100.0 * if style.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
                        (c.to_uppercase().next().unwrap_or(c), small_sz)
                    } else {
                        (c, base_size)
                    };
                    let name = style.font_for(c);
                    // ＭＳ 明朝's half-width letters are a light serif; the
                    // substitute's are heavier, so they are drawn in one.
                    let mincho_latin = name.is_some_and(|n| fixed_width(n) && n.contains("明朝") || n == "MS Mincho") && c.is_ascii() && !c.is_whitespace(); // i18n-ok（フォント名の判定（明朝体の名前）。訳さない）
                    let font = if mincho_latin { self.font_for_size(style, Some(MINCHO_LATIN), char_style_size) } else { self.font_for_size(style, name, char_style_size) };
                    let actual = self.res.text_width(&c.to_string(), &font, char_size);
                    let (cell, inset) = match name {
                        Some(n) if fixed_width(n) => {
                            let cell = if full_width(c) { char_size } else { char_size / 2.0 };
                            (cell, ((cell - actual) / 2.0).max(0.0))
                        }
                        _ => (actual, 0.0),
                    };
                    // A comment's mark (U+200B) takes no room.
                    let width = if c == '\u{200b}' { 0.0 } else { cell * style.scale as f32 / 100.0 + pt(style.spacing) };
                    let (mut ascent, mut line) = self.line_metrics(name, &font, base_size);
                    // A line with Hangul is 1.72 em high in Word (18.1 pt of 10.5 pt text, DNUM: its fallback font for
                    // Hangul), the text sitting low in it.
                    if is_hangul(c) && line < base_size * HANGUL_LINE {
                        let extra = base_size * HANGUL_LINE - line;
                        ascent += extra * HANGUL_ASCENT_SHARE;
                        line += extra;
                    }
                    if let Some(b) = &style.border {
                        let pad_v = pt(b.space);
                        let extra_bw = (pt(b.total()) - pt(b.width)).max(0.0);
                        ascent += 1.2 + pad_v;
                        line += 1.2 + 2.8 + 2.0 * pad_v + 2.0 * extra_bw;
                    }
                    out.push(Unit::Char { c, width, inset, font, size: char_size, style: index, line, ascent, lead: Self::leading(name) * base_size, auto_space: 0.0 });
                }
            };
            // Tracked changes visibility: All shows both; Simple and None hide deleted text; Original hides inserted text.
            let show_hidden = if self.res.print {
                self.res.print_hidden_text
            } else {
                showing_hidden_text()
            };
            let rev_hidden = match run.revision.as_ref() {
                Some(r) if is_reviewer_shown(&r.author) => match self.markup_mode() {
                    MarkupDisplayMode::All => false,
                    MarkupDisplayMode::Simple | MarkupDisplayMode::None => r.kind.is_delete(),
                    MarkupDisplayMode::Original => r.kind.is_insert(),
                },
                Some(r) => {
                    // 非表示校閲者の変更は確定テキスト（挿入は表示、削除は非表示）として扱う
                    r.kind.is_delete()
                }
                None => false,
            };
            let hidden = (run.style.hidden && !show_hidden) || rev_hidden;
            let show_code = run.is_field_code_shown(self.doc.show_field_codes);
            match &run.kind {
                // A picture or a shape of deleted (or hidden) text is not drawn,
                // nor one put off to the next page.
                FlowRunKind::Picture { .. } | FlowRunKind::PictureControl { .. } | FlowRunKind::Shape(_) if hidden => {}
                FlowRunKind::Shape(shape) if self.deferred_shapes.borrow().contains(&shape.id.0) => {}
                FlowRunKind::Text(t) if hidden => push_text(&"\u{200b}".repeat(t.chars().count()), &mut out),
                FlowRunKind::Text(t) => push_text(t, &mut out),
                FlowRunKind::CheckBox { checked, checked_char, unchecked_char } => {
                    let ch = if *checked { *checked_char } else { *unchecked_char };
                    push_text(&ch.to_string(), &mut out);
                }
                FlowRunKind::DropDownList { items, selected, .. } => {
                    let text = items.get(*selected).map(|(d, _)| d.as_str()).unwrap_or("");
                    if hidden {
                        push_text(&"\u{200b}".repeat(text.chars().count()), &mut out);
                    } else {
                        push_text(text, &mut out);
                    }
                }
                FlowRunKind::DatePicker { text, .. }
                | FlowRunKind::RichText { text, .. }
                | FlowRunKind::PlainText { text, .. }
                | FlowRunKind::BuildingBlockGallery { text, .. }
                | FlowRunKind::Group { text, .. } => {
                    if hidden {
                        push_text(&"\u{200b}".repeat(text.chars().count()), &mut out);
                    } else {
                        push_text(text, &mut out);
                    }
                }
                FlowRunKind::RepeatingSection { items, selected, .. } => {
                    let text = items.get(*selected).map(|s| s.as_str()).unwrap_or("");
                    if hidden {
                        push_text(&"\u{200b}".repeat(text.chars().count()), &mut out);
                    } else {
                        push_text(text, &mut out);
                    }
                }
                FlowRunKind::FormField(field) => match &field.kind {
                    ibis_doc_model::FormFieldKind::TextInput { value, default, .. } => {
                        let text = if value.is_empty() {
                            if !default.is_empty() {
                                default.as_str()
                            } else {
                                "\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}"
                            }
                        } else {
                            value.as_str()
                        };
                        if hidden {
                            push_text(&"\u{200b}".repeat(text.chars().count()), &mut out);
                        } else {
                            push_text(text, &mut out);
                        }
                    }
                    ibis_doc_model::FormFieldKind::CheckBox { checked, .. } => {
                        let ch = if *checked { '☒' } else { '☐' };
                        push_text(&ch.to_string(), &mut out);
                    }
                    ibis_doc_model::FormFieldKind::DropDown { items, selected, .. } => {
                        let text = items.get(*selected).map(|s| s.as_str()).unwrap_or("");
                        if hidden {
                            push_text(&"\u{200b}".repeat(text.chars().count()), &mut out);
                        } else {
                            push_text(text, &mut out);
                        }
                    }
                },
                FlowRunKind::PageNumber if show_code => push_text("{ PAGE }", &mut out),
                FlowRunKind::PageNumber => {
                    let text = match &*self.page_format.borrow() {
                        Some(f) => ibis_doc_model::format_page_number(page_number, f),
                        None => page_number.to_string(),
                    };
                    push_text(&format!("{}{text}", self.chapter_prefix.borrow()), &mut out)
                }
                FlowRunKind::FileName(_) if show_code => push_text("{ FILENAME }", &mut out),
                FlowRunKind::FileName(saved) => push_text(self.doc.file_name.as_deref().unwrap_or(saved), &mut out),
                FlowRunKind::Field { instruction, .. } if show_code => {
                    let code = format!("{{ {} }}", instruction.trim());
                    push_text(&code, &mut out);
                }
                FlowRunKind::Field { instruction, shown } => match self.field_text(instruction, page_number) {
                    Some(text) => push_text(&text, &mut out),
                    // 囲い文字: its enclosure takes the room (the character
                    // is drawn over it).
                    None => match ibis_doc_model::field_format::enclosed(instruction) {
                        Some((enclosure, _)) => push_text(enclosure.chars().next().map(String::from).as_deref().unwrap_or("○"), &mut out),
                        // An EQ field of fractions, roots, scripts…: typeset as a formula.
                        None => match ibis_doc_model::eq_field::eq_math(instruction) {
                            Some(math) => out.push(self.math_unit(&math, false, style, size)),
                            None => push_text(shown, &mut out),
                        },
                    },
                },
                FlowRunKind::PageRef { bookmark, .. } if show_code => {
                    let code = format!("{{ PAGEREF {} }}", bookmark);
                    push_text(&code, &mut out);
                }
                FlowRunKind::PageRef { shown, .. } => push_text(shown, &mut out),
                FlowRunKind::Ruby { base, .. } => push_text(base, &mut out),
                FlowRunKind::NoteRef { endnote, id } => {
                    let n = self.note_numbers.get(&(*endnote, *id)).cloned().unwrap_or_default();
                    push_text(&n, &mut out)
                }
                FlowRunKind::NoteMark => {
                    let n = self.current_note.borrow().clone();
                    push_text(&n, &mut out)
                }
                // A comment's marks take a position but no room.
                FlowRunKind::CommentStart(_) | FlowRunKind::CommentEnd(_) | FlowRunKind::CommentRef(_) => push_text("\u{200b}", &mut out),
                FlowRunKind::Tab => out.push(Unit::Tab),
                FlowRunKind::LineBreak => out.push(Unit::LineBreak),
                FlowRunKind::PageBreak => out.push(Unit::PageBreak),
                FlowRunKind::ColumnBreak => out.push(Unit::ColumnBreak),
                FlowRunKind::Picture { media, size, look } | FlowRunKind::PictureControl { media, size, look, .. } if look.is_visual_default() => out.push(Unit::Picture { media: *media, w: pt(size.width), h: pt(size.height) }),
                // A picture turned, flipped, cut or outlined: a shape (drawn as the pictures of a
                // presentation are) whose room in the line is the box around it turned.
                FlowRunKind::Picture { media, size, look } | FlowRunKind::PictureControl { media, size, look, .. } => {
                    let angle = look.rotation as f32 / 60_000.0 * std::f32::consts::PI / 180.0;
                    let (sin, cos) = (angle.sin().abs(), angle.cos().abs());
                    let (w, h) = (pt(size.width), pt(size.height));
                    let picture = ibis_doc_model::Picture { media: *media, crop: look.crop, stroke: look.stroke.clone(), corner: 0, duotone: look.duotone, clear: None, luminance: look.luminance, clip: None, alt_text: look.alt_text.clone(), decorative: look.decorative, name: look.name.clone(), shadow: None, reflection: None, soft_edge: look.soft_edge, geometry: look.geometry.clone(), effects: look.effects.clone(), tile: None, fill_rect: None, artistic_effect: None, ole: look.ole };
                    let shape = ibis_doc_model::Shape {
                        id: ibis_doc_model::ShapeId(0),
                        frame: ibis_doc_model::EmuRect::new(0, 0, size.width, size.height),
                        rotation: look.rotation,
                        flip_h: look.flip_h,
                        flip_v: look.flip_v,
                        kind: ibis_doc_model::ShapeKind::Picture(picture),
                        floating: None,
                    };
                    out.push(Unit::Shape { shape: Box::new(shape), w: w * cos + h * sin, h: w * sin + h * cos });
                }
                FlowRunKind::Math { math, display } => out.push(self.math_unit(math, *display, style, size)),
                FlowRunKind::Shape(shape) => {
                    let is_floating = shape.floating.is_some();
                    let mut shape = shape.clone();
                    if !is_floating {
                        self.fit_unwrapped_text_box(&mut shape);
                    }
                    let (w, h) = if is_floating { (0.0, 0.0) } else { (pt(shape.frame.width), pt(shape.frame.height)) };
                    out.push(Unit::Shape { shape, w, h });
                }
            }
            // 組み文字: the run's room is its two half-size lines' (and
            // its brackets'), on its first character.
            if let Some(c) = style.combine
                && out.len() > before
            {
                let chars: Vec<char> = out[before..].iter().filter_map(unit_char).collect();
                let total = self.combine_width(style, &chars, size, c);
                for (k, u) in out[before..].iter_mut().enumerate() {
                    if let Unit::Char { width, inset, .. } = u {
                        *width = if k == 0 { total } else { 0.0 };
                        *inset = 0.0;
                    }
                }
            }
            // 縦中横・文字の回転 (`w:eastAsianLayout w:vert`).
            if style.tate_chu_yoko && out.len() > before {
                let total = size;
                for (k, u) in out[before..].iter_mut().enumerate() {
                    if let Unit::Char { width, inset, .. } = u {
                        *width = if k == 0 { total } else { 0.0 };
                        *inset = 0.0;
                    }
                }
            }
            let text = matches!(run.kind, FlowRunKind::Text(_));
            for k in 0..out.len() - before {
                if text {
                    positions.push((position + k, true));
                } else {
                    positions.push((position, k == 0));
                }
            }
            position += run.len();
        }
        // Pair kerning (`w:kern`): from that size up (half-points), between
        // two characters of a run in the same font, the first one's width
        // takes the pair's kerning (Word's line breaks in DBDY-00007 follow
        // the kerned widths).
        for i in 1..out.len() {
            let (left, right) = out.split_at_mut(i);
            let (Unit::Char { c: a, width, font, size, style, .. }, Unit::Char { c: b, font: next_font, size: next_size, style: next_style, .. }) = (&mut left[i - 1], &right[0]) else { continue };
            if style != next_style || font != next_font || size != next_size {
                continue;
            }
            let Some(run) = p.runs.get(*style) else { continue };
            if run.style.kern.is_some_and(|k| *size * 2.0 >= k as f32) {
                *width += self.res.pair_kerning(font, *size, *a, *b) * run.style.scale as f32 / 100.0;
            }
        }
        // A quarter em between Japanese and Latin letters or digits.
        if p.auto_space_letters || p.auto_space_digits {
            let spaced = |c: char| (p.auto_space_letters && c.is_ascii_alphabetic()) || (p.auto_space_digits && c.is_ascii_digit());
            // Not the middle dot ・ (Word's PDFs of DTBA and DFLD: "auto・セル"
            // and "2000・tcW" without the space).
            let japanese = |c: char| c != '・' && matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x9fff | 0xf900..=0xfaff);
            for i in 1..out.len() {
                let (Some(a), Some(b)) = (unit_char(&out[i - 1]), unit_char(&out[i])) else { continue };
                if ((japanese(a) && spaced(b)) || (spaced(a) && japanese(b)))
                    && let Unit::Char { width, size, auto_space, .. } = &mut out[i - 1]
                {
                    let space = *size / 4.0;
                    *width += space;
                    *auto_space = space;
                }
            }
        }
        // A closing bracket or a stop followed by an opening bracket (、「 ）（ 。『) is squeezed: the first one takes half an em
        // (Word does so in DIDX-05 and -07 even with `doNotCompress`).
        for i in 1..out.len() {
            let (Some(a), Some(b)) = (unit_char(&out[i - 1]), unit_char(&out[i])) else { continue };
            if "、。，．）」』】〕〉》］｝".contains(a) // i18n-ok（行頭禁則の文字の集合。文字の並び）
                && "「『（【〔〈《［｛".contains(b) // i18n-ok（行末禁則の文字の集合。文字の並び）
                && let Unit::Char { width, size, .. } = &mut out[i - 1]
                && *width >= *size * 0.99
            {
                *width -= *size / 2.0;
            }
        }
        (out, positions)
    }

    /// Break a paragraph `width` points wide into lines.
    fn paragraph(&self, p: &FlowParagraph, width: f32, page_number: u32, obstacle: Option<&WrapObstacleInPara>, in_table: bool) -> Vec<LineBox> {
        self.line_room.set(width - pt(p.left) - pt(p.right));
        let (mut units, mut positions) = self.units(p, page_number);
        self.line_room.set(f32::MAX);
        let left = pt(p.left);
        let right = pt(p.right);
        // A list item's number centred on, or ending at, its position.
        let label: f32 = units.iter().take_while(|u| matches!(u, Unit::Char { style: LABEL, .. })).map(unit_width).sum();
        let label_align = p.list.and_then(|l| self.doc.styles.numbering.level(l.num_id, l.level)).map(|l| l.align);
        let shift = match label_align {
            Some(FlowAlign::Right) => label,
            Some(FlowAlign::Center) => label / 2.0,
            _ => 0.0,
        };
        let first = pt(p.first_line) - shift;
        let tab_default = self.default_tab_pt();

        let line_start_x = |first_line: bool| left + if first_line { first } else { 0.0 };

        let mark_size = p.mark.size as f32 / 100.0;
        let mark_metrics = {
            self.mark_metrics(p, mark_size)
        };
        let default_line_h = self.resolve_line_height(p, mark_metrics.0, mark_metrics.2, in_table);

        let default_obs = if obstacle.is_none() {
            square_shape(p, self.hides_deleted()).filter(|s| !self.deferred_shapes.borrow().contains(&s.id.0)).map(|shape| {
                let f = shape.floating.as_ref().unwrap();
                let sw = pt(shape.frame.width);
                let sh = pt(shape.frame.height);
                let dl = pt(f.dist_l);
                let dr = pt(f.dist_r);
                let dt = pt(f.dist_t);
                let db = pt(f.dist_b);
                let wrap_text = f.wrap_text.clone().unwrap_or_else(|| "bothSides".to_string());
                let page_w = pt(self.doc.page.size.width);
                let margin_l = pt(self.doc.page.left);
                let margin_r = pt(self.doc.page.right);
                let is_even = page_number % 2 == 0;

                let (base_x0, base_w) = match f.relative_h.as_str() {
                    "page" => (0.0, page_w),
                    "leftMargin" => (0.0, margin_l),
                    "rightMargin" => (page_w - margin_r, margin_r),
                    "insideMargin" => if is_even { (page_w - margin_r, margin_r) } else { (0.0, margin_l) },
                    "outsideMargin" => if is_even { (0.0, margin_l) } else { (page_w - margin_r, margin_r) },
                    _ /* "column" | "margin" */ => (margin_l, width),
                };

                let abs_x = if f.relative_h == "character" {
                    match f.align_h.as_deref() {
                        Some("center") => margin_l - sw / 2.0,
                        Some("right") => margin_l - sw,
                        Some("left") => margin_l,
                        _ => margin_l + pt(f.offset_x),
                    }
                } else {
                    match f.align_h.as_deref() {
                        Some("left") => base_x0,
                        Some("center") => base_x0 + (base_w - sw) / 2.0,
                        Some("right") => base_x0 + base_w - sw,
                        Some("inside") => if is_even { base_x0 + base_w - sw } else { base_x0 },
                        Some("outside") => if is_even { base_x0 } else { base_x0 + base_w - sw },
                        _ => base_x0 + pt(f.offset_x),
                    }
                };
                let abs_x = abs_x.clamp(0.0, (page_w - sw).max(0.0));
                let sx = abs_x - margin_l;
                let page_h = pt(self.doc.page.size.height);
                let margin_t = pt(self.doc.page.top);
                let margin_b = pt(self.doc.page.bottom);
                let (base_y0, base_h) = match f.relative_v.as_str() {
                    "page" => (0.0, page_h),
                    "margin" => (margin_t, (page_h - margin_t - margin_b).max(0.0)),
                    "topMargin" => (0.0, margin_t),
                    "bottomMargin" => (page_h - margin_b, margin_b),
                    "insideMargin" => if is_even { (page_h - margin_b, margin_b) } else { (0.0, margin_t) },
                    "outsideMargin" => if is_even { (0.0, margin_t) } else { (page_h - margin_b, margin_b) },
                    "line" => (0.0, default_line_h),
                    _ /* "paragraph" */ => (0.0, 0.0),
                };
                let sy = if f.relative_v == "line" {
                    match f.align_v.as_deref() {
                        Some("top") => 0.0,
                        Some("center") => if base_h > 0.0 { (base_h - sh) / 2.0 } else { 0.0 },
                        Some("bottom") => if base_h > 0.0 { base_h - sh } else { 0.0 },
                        _ => pt(f.offset_y),
                    }
                } else if f.relative_v == "paragraph" {
                    match f.align_v.as_deref() {
                        Some("top") | Some("center") | Some("bottom") => 0.0,
                        _ => pt(f.offset_y),
                    }
                } else {
                    let abs_y = match f.align_v.as_deref() {
                        Some("top") => base_y0,
                        Some("center") => if base_h > 0.0 { base_y0 + (base_h - sh) / 2.0 } else { base_y0 },
                        Some("bottom") => if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 },
                        Some("inside") => if is_even { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } } else { base_y0 },
                        Some("outside") => if is_even { base_y0 } else { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } },
                        _ => base_y0 + pt(f.offset_y),
                    };
                    abs_y - margin_t
                };
                let wrap_polygon = matches!(f.wrap.as_str(), "tight" | "through")
                    .then(|| f.wrap_polygon.clone())
                    .flatten()
                    .map(|poly| (poly, sx, sy, sw, sh, dl, dr));
                WrapObstacleInPara {
                    shape_left: sx - dl,
                    shape_right: sx + sw + dr,
                    rel_top: sy - dt,
                    rel_bottom: sy + sh + db,
                    wrap_text,
                    wrap_polygon,
                    shape_to_emit: Some((shape.clone(), sx, sy, sw, sh, f.behind_doc)),
                    drop_cap_to_emit: None,
                    extra: Vec::new(),
                }
            })
        } else {
            None
        };
        let drop_cap_obs = if obstacle.is_none() && default_obs.is_none() {
            if let Some(dc) = p.drop_cap.filter(|d| d.location != ibis_doc_model::DropCapLocation::None) {
                let first_char_idx = units.iter().position(|u| matches!(u, Unit::Char { c, .. } if !c.is_whitespace() && *c != '\u{200b}'));
                if let Some(idx) = first_char_idx {
                    let (c, font, style) = match &units[idx] {
                        Unit::Char { c, font, style, .. } => (*c, font.clone(), *style),
                        _ => unreachable!(),
                    };
                    units.remove(idx);
                    if idx < positions.len() {
                        positions.remove(idx);
                    }
                    let lines_count = (dc.lines as f32).clamp(1.0, 10.0);
                    let cap_h = default_line_h * lines_count;
                    let cap_font_size = default_line_h * lines_count * 0.78;
                    let cap_w = self.res.text_width(&c.to_string(), &font, cap_font_size);
                    let space_pt = pt(dc.space);
                    let cap_baseline = (lines_count - 1.0) * default_line_h + mark_metrics.1;
                    let cap_color = p.runs.get(style).map_or_else(
                        || hsla(ibis_doc_model::Color::opaque(p.list_style.color)),
                        |r| hsla(ibis_doc_model::Color::opaque(r.style.color)),
                    );
                    let start_x = line_start_x(true);
                    let (cap_x, obs_right) = if dc.location == ibis_doc_model::DropCapLocation::Margin {
                        (start_x - cap_w - space_pt, start_x)
                    } else {
                        (start_x, start_x + cap_w + space_pt)
                    };
                    let item = Item::Text {
                        x: cap_x,
                        y: cap_baseline,
                        text: c.to_string().into(),
                        offsets: Arc::from([0.0]),
                        font,
                        size: cap_font_size,
                        color: cap_color,
                    };
                    Some(WrapObstacleInPara {
                        shape_left: cap_x,
                        shape_right: obs_right,
                        rel_top: 0.0,
                        rel_bottom: cap_h,
                        wrap_text: "right".to_string(),
                        wrap_polygon: None,
                        shape_to_emit: None,
                        drop_cap_to_emit: Some(item),
                        extra: Vec::new(),
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        let effective_obs = obstacle.or(default_obs.as_ref()).or(drop_cap_obs.as_ref());

        if let Some(obs) = effective_obs {
            let shape_left = obs.shape_left;
            let shape_right = obs.shape_right;
            let shape_top = obs.rel_top;
            let shape_bottom = obs.rel_bottom;
            let wrap_text = obs.wrap_text.as_str();

            #[derive(Clone, Copy)]
            struct Segment {
                s: usize,
                e: usize,
                slot_x: f32,
                slot_w: f32,
                forced: bool,
            }

            let mut physical_lines: Vec<Vec<Segment>> = Vec::new();
            let mut i = 0;
            let mut cur_y = 0.0;

            while i < units.len() || physical_lines.is_empty() {
                let first_line = physical_lines.is_empty();
                let line_start = line_start_x(first_line);
                let line_end = width - right;
                let full_room = (line_end - line_start).max(0.0);

                let (line_shape_left, line_shape_right, is_intersecting) = if (cur_y < shape_bottom - 0.01) && (cur_y + default_line_h > shape_top + 0.01) {
                    if let Some((poly, sx, sy, sw, sh, dl, dr)) = &obs.wrap_polygon {
                        if let Some((min_x, max_x)) = poly.x_span_for_y_band(*sx, *sy, *sw, *sh, cur_y, cur_y + default_line_h) {
                            (min_x - dl, max_x + dr, true)
                        } else {
                            (shape_left, shape_right, false)
                        }
                    } else {
                        (shape_left, shape_right, true)
                    }
                } else {
                    (shape_left, shape_right, false)
                };
                // Only the part of the shape over this line's own range cuts it: a shape
                // past the column's edge leaves the line its full width, never more.
                let line_shape_left = line_shape_left.clamp(line_start, line_end.max(line_start));
                let line_shape_right = line_shape_right.clamp(line_start, line_end.max(line_start));
                let is_intersecting = is_intersecting && line_shape_right > line_shape_left;
                let mut slots = if is_intersecting {
                    let left_w = (line_shape_left - line_start).max(0.0);
                    let right_w = (line_end - line_shape_right).max(0.0);
                    let min_slot_w = MIN_WRAP_ROOM;
                    match wrap_text {
                        "left" => {
                            if left_w >= min_slot_w { vec![(line_start, left_w)] } else { vec![] }
                        }
                        "right" => {
                            if right_w >= min_slot_w { vec![(line_shape_right, right_w)] } else { vec![] }
                        }
                        "largest" => {
                            if right_w >= left_w && right_w >= min_slot_w {
                                vec![(line_shape_right, right_w)]
                            } else if left_w >= min_slot_w {
                                vec![(line_start, left_w)]
                            } else {
                                vec![]
                            }
                        }
                        _ /* "bothSides" */ => {
                            let mut s = Vec::new();
                            if left_w >= min_slot_w { s.push((line_start, left_w)); }
                            if right_w >= min_slot_w { s.push((line_shape_right, right_w)); }
                            s
                        }
                    }
                } else {
                    vec![(line_start, full_room)]
                };
                // The other obstacles over this line cut its room too.
                let mut cut = false;
                for &(l, r, t, b) in &obs.extra {
                    if cur_y < b - 0.01 && cur_y + default_line_h > t + 0.01 {
                        let (l, r) = (l.clamp(line_start, line_end.max(line_start)), r.clamp(line_start, line_end.max(line_start)));
                        if r <= l {
                            continue;
                        }
                        cut = true;
                        slots = slots
                            .into_iter()
                            .flat_map(|(x, w)| {
                                let mut parts = Vec::new();
                                if l > x {
                                    parts.push((x, (l - x).min(w)));
                                }
                                if r < x + w {
                                    parts.push((r.max(x), x + w - r.max(x)));
                                }
                                parts
                            })
                            .filter(|&(_, w)| w >= 15.0)
                            .collect();
                    }
                }
                let is_intersecting = is_intersecting || cut;

                let mut phys_line: Vec<Segment> = Vec::new();
                let mut line_forced_break = false;

                for &(slot_x, slot_w) in &slots {
                    if i >= units.len() {
                        break;
                    }
                    // A gap beside the shape that cannot hold even the next character is left
                    // empty (as Word does): the text goes to the other side, or below the shape.
                    if is_intersecting
                        && !matches!(units[i], Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak | Unit::Tab | Unit::ListTab)
                        && unit_width(&units[i]) > slot_w + 0.01
                    {
                        continue;
                    }
                    let start = i;
                    let mut x = 0.0;
                    let mut last_break: Option<usize> = None;
                    let mut slot_ended = false;
                    // What the spaces so far may give (justified text: `SPACE_GIVE`).
                    let mut give = 0.0f32;

                    while i < units.len() {
                        match &units[i] {
                            Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {
                                phys_line.push(Segment { s: start, e: i + 1, slot_x, slot_w, forced: true });
                                if i > 0 && let Unit::Char { width, auto_space, .. } = &mut units[i - 1] {
                                    *width -= *auto_space;
                                    *auto_space = 0.0;
                                }
                                i += 1;
                                slot_ended = true;
                                line_forced_break = true;
                                break;
                            }
                            Unit::ListTab if left > slot_x + x + 0.01 => {
                                x = left - slot_x;
                                i += 1;
                                continue;
                            }
                            Unit::Tab | Unit::ListTab => {
                                let at = slot_x + x;
                                let tab = p.tabs.iter().find(|t| pt(t.position) > at + 0.01);
                                let stop = tab.map(|t| pt(t.position)).unwrap_or(((at / tab_default).floor() + 1.0) * tab_default);
                                let rest: f32 = units[i + 1..].iter().take_while(|u| !matches!(u, Unit::Tab | Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak)).map(unit_width).sum();
                                let stop = match tab.map(|t| t.align) {
                                    Some(TabAlign::Right) => (stop - rest).max(at),
                                    Some(TabAlign::Center) => (stop - rest / 2.0).max(at),
                                    _ => stop,
                                };
                                x = stop - slot_x;
                                i += 1;
                                continue;
                            }
                            _ => {}
                        }
                        let w = unit_width(&units[i]);
                        let c = unit_char(&units[i]);
                        if i > start {
                            let prev = unit_char(&units[i - 1]);
                            if may_break(prev, c, p.kinsoku, p.word_wrap, self.doc) {
                                last_break = Some(i);
                            }
                        }
                        if x + w > slot_w + give + 0.01 && i > start {
                            if c == Some(' ') {
                                x += w;
                                i += 1;
                                continue;
                            }
                            if p.overflow_punct && c.is_some_and(|ch| "、。，．,".contains(ch)) && x <= slot_w + 0.01 { // i18n-ok（オーバーフロー句読点の文字の集合。文字の並び）
                                x += w;
                                i += 1;
                                continue;
                            }
                            let at = match last_break {
                                Some(b) if b > start => b,
                                _ => i,
                            };
                            phys_line.push(Segment { s: start, e: at, slot_x, slot_w, forced: false });
                            if at > 0 && let Unit::Char { width, auto_space, .. } = &mut units[at - 1] {
                                *width -= *auto_space;
                                *auto_space = 0.0;
                            }
                            i = at;
                            slot_ended = true;
                            break;
                        }
                        if c == Some(' ') && p.align == FlowAlign::Justify {
                            give += w * SPACE_GIVE;
                        }
                        x += w;
                        i += 1;
                    }

                    if !slot_ended && start < units.len() {
                        phys_line.push(Segment { s: start, e: units.len(), slot_x, slot_w, forced: true });
                        if let Some(Unit::Char { width, auto_space, .. }) = units.last_mut() {
                            *width -= *auto_space;
                            *auto_space = 0.0;
                        }
                    }

                    if line_forced_break {
                        break;
                    }
                }

                if phys_line.is_empty() && units.is_empty() {
                    phys_line.push(Segment { s: 0, e: units.len(), slot_x: line_start, slot_w: full_room, forced: true });
                }

                let (text_line, lead) = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { line, lead, .. } = u { Some((*line, *lead)) } else { None }).fold((0.0f32, 0.0f32), |(l, d), (l2, d2)| (l.max(l2), d.max(d2)));
                let (text_line, lead) = if text_line > 0.0 { (text_line, lead) } else { (mark_metrics.0, mark_metrics.2) };
                let picture = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| match u { Unit::Picture { h, .. } | Unit::Shape { h, .. } | Unit::Math { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
                let natural = text_line.max(picture);
                let lead = if picture > text_line { 0.0 } else { lead };
                let height = self.resolve_line_height(p, natural, lead, in_table);
                cur_y += height;

                physical_lines.push(phys_line);
                if i >= units.len() {
                    break;
                }
            }

            let mut out = Vec::new();
            for (n, phys_line) in physical_lines.iter().enumerate() {
                let largest = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { size, .. } = u { Some(*size) } else { None }).fold(0.0, f32::max);
                let largest = if largest > 0.0 { largest } else { mark_size };
                let (text_line, text_ascent, lead) = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { c, line, ascent, lead, style, size, .. } = u { let (line, ascent) = mark_room(p, *c, *style, *size, *line, *ascent); Some((line, ascent, *lead)) } else { None }).fold((0.0f32, 0.0f32, 0.0f32), |(l, a, d), (l2, a2, d2)| (l.max(l2), a.max(a2), d.max(d2)));
                let (text_line, text_ascent, lead) = if text_line > 0.0 { (text_line, text_ascent, lead) } else { mark_metrics };
                let picture = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| match u { Unit::Picture { h, .. } | Unit::Shape { h, .. } | Unit::Math { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
                let natural = text_line.max(picture);
                let lead = if picture > text_line { 0.0 } else { lead };
                let height = self.resolve_line_height(p, natural, lead, in_table);
                let height = if physical_lines.len() <= 1 && obs.drop_cap_to_emit.is_some() {
                    height.max(obs.rel_bottom)
                } else {
                    height
                };
                let math_below = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Math { descent, .. } = u { Some(*descent) } else { None }).fold(0.0f32, f32::max);
                let baseline = if picture > text_line { height - math_below } else { self.baseline_in_line(p, natural, lead, text_line, text_ascent, in_table) };
                let page_break = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).any(|u| matches!(u, Unit::PageBreak));
                let column_break = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).any(|u| matches!(u, Unit::ColumnBreak));
                let has_revision = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).any(|u| {
                    if let Unit::Char { style, .. } = u {
                        if *style != LABEL {
                            p.runs.get(*style).is_some_and(|r| r.revision.as_ref().is_some_and(|rev| is_reviewer_shown(&rev.author)))
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                });
                let mut comment_marks = Vec::new();
                for u in phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]) {
                    if let Unit::Char { style, .. } = u {
                        if *style != LABEL {
                            if let Some(c_ids) = self.comment_runs.get(&(p.id, *style)) {
                                for &id in c_ids {
                                    if !comment_marks.contains(&id) {
                                        comment_marks.push(id);
                                    }
                                }
                            }
                            if let Some(r) = p.runs.get(*style) {
                                match r.kind {
                                    FlowRunKind::CommentStart(id) | FlowRunKind::CommentRef(id) => {
                                        if !comment_marks.contains(&id) {
                                            comment_marks.push(id);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                let mut line = LineBox { height, baseline, items: Vec::new(), page_break, column_break, has_revision, comment_marks };

                if n == 0 {
                    if let Some((shape, sx, sy, sw, sh, behind_doc)) = &obs.shape_to_emit {
                        let item = Item::Shape {
                            shape: Box::new((*shape).clone()),
                            x: *sx,
                            y: *sy,
                            w: *sw,
                            h: *sh,
                        };
                        if *behind_doc {
                            line.items.insert(0, item);
                        } else {
                            line.items.push(item);
                        }
                    }
                    if let Some(item) = &obs.drop_cap_to_emit {
                        line.items.push(item.clone());
                    }
                    // The paragraph's other floating shapes than the one the text
                    // wraps round (pictures over a text box), from its top: with
                    // its first line, wherever the units fall.
                    for u in &units {
                        if let Unit::Shape { shape, .. } = u
                            && let Some(f) = &shape.floating
                            && f.wrap != "topAndBottom"
                            && !obs.shape_to_emit.as_ref().is_some_and(|(s, ..)| s.id == shape.id)
                            && !square_shape(p, self.hides_deleted()).is_some_and(|s| s.id == shape.id)
                        {
                            let item = self.floating_item(shape, 0.0, width, line.height, page_number);
                            if f.behind_doc {
                                line.items.insert(0, item);
                            } else {
                                line.items.push(item);
                            }
                        }
                    }
                }

                for (seg_idx, seg) in phys_line.iter().enumerate() {
                    let slice = &units[seg.s..seg.e];
                    let visible_end = slice.iter().rposition(|u| unit_char(u).is_none_or(|c| c != ' ')).map_or(0, |p| p + 1);
                    let used: f32 = slice[..visible_end].iter().map(unit_width).sum();
                    let last = (n + 1 == physical_lines.len() && seg_idx + 1 == phys_line.len()) || seg.forced;
                    let room = seg.slot_w;
                    let (extra, expands) = justify_extra(slice, visible_end, p.align, last, room, used);
                    // A displayed formula alone on its line is centred.
                    let align = if slice.iter().any(|u| matches!(u, Unit::Math { display: true, .. })) && slice.iter().all(|u| !matches!(u, Unit::Char { c, .. } if !c.is_whitespace())) { FlowAlign::Center } else { p.align };
                    let shift = match align {
                        FlowAlign::Center => ((room - used) / 2.0).max(0.0),
                        FlowAlign::Right => (room - used).max(0.0),
                        FlowAlign::Left if p.bidi => (room - used).max(0.0),
                        _ => 0.0,
                    };
                    let mut x = seg.slot_x + shift;
                    let line_x = x;
                    let mut stops: Vec<(usize, f32)> = Vec::new();
                    let mut run: Option<OpenRun> = None;
                    let mut dec = LineDecorations::default();
                    let flush = |run: &mut Option<OpenRun>, line: &mut LineBox, dec: &mut LineDecorations| {
                        if let Some((x0, text, offsets, font, size, style)) = run.take() {
                            let s = if style == LABEL { &p.list_style } else { &p.runs[style].style };
                            let revised;
                            let is_all_markup = self.markup_mode() == MarkupDisplayMode::All;
                            let is_hidden_dotted = p.runs.get(style).is_some_and(|r| r.style.hidden && !self.res.print && showing_hidden_text() && style != LABEL);
                            let active_rev = p.runs.get(style).and_then(|r| r.revision.as_ref()).filter(|r| is_reviewer_shown(&r.author));
                            let s = match active_rev.map(|r| &r.kind).filter(|_| is_all_markup) {
                                Some(ibis_doc_model::RevisionKind::Insert) if style != LABEL => {
                                    revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Single, color: REVISION_COLOR, ..s.clone() };
                                    &revised
                                }
                                Some(ibis_doc_model::RevisionKind::Delete) if style != LABEL => {
                                    revised = ibis_doc_model::FlowRunStyle { strike: true, color: REVISION_COLOR, ..s.clone() };
                                    &revised
                                }
                                Some(ibis_doc_model::RevisionKind::MoveTo) if style != LABEL => {
                                    revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Single, color: MOVE_REVISION_COLOR, ..s.clone() };
                                    &revised
                                }
                                Some(ibis_doc_model::RevisionKind::MoveFrom) if style != LABEL => {
                                    revised = ibis_doc_model::FlowRunStyle { strike: true, color: MOVE_REVISION_COLOR, ..s.clone() };
                                    &revised
                                }
                                _ if is_hidden_dotted => {
                                    revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Dotted, ..s.clone() };
                                    &revised
                                }
                                _ => s,
                            };
                            let s_size = s.size as f32 / 100.0 * if s.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
                            let raise = match s.position {
                                VerticalPosition::Superscript => -largest * 0.35,
                                VerticalPosition::Subscript => largest * 0.1,
                                VerticalPosition::Baseline => 0.0,
                            };
                            let font_name = text.chars().next().and_then(|c| s.font_for(c)).or_else(|| s.font_for('あ'));
                            let (run_ascent, run_line) = self.line_metrics(font_name, &font, s_size);
                            let run_descent = run_line - run_ascent;
                            let align_shift = match p.text_alignment {
                                ibis_doc_model::FontAlign::Top => run_ascent - text_ascent,
                                ibis_doc_model::FontAlign::Center => ((run_ascent - run_descent) - (text_ascent - (text_line - text_ascent))) / 2.0,
                                ibis_doc_model::FontAlign::Bottom => (text_line - text_ascent) - run_descent,
                                ibis_doc_model::FontAlign::Baseline | ibis_doc_model::FontAlign::Auto => 0.0,
                            };
                            let y_base = line.baseline + raise + align_shift;
                            let end = offsets.last().copied().unwrap_or(0.0);
                            let show_comments = self.markup_mode() != MarkupDisplayMode::None && self.markup_mode() != MarkupDisplayMode::Original;
                            if show_comments && style != LABEL && end > 0.0 {
                                if let Some(c_ids) = self.comment_runs.get(&(p.id, style)) && !c_ids.is_empty() {
                                    let color = hsla(COMMENT_HIGHLIGHT_COLOR);
                                    if let Some(Item::Rect { x: last_x, w: last_w, color: last_c, .. }) = dec.highlights.last_mut()
                                        && *last_c == color
                                        && (*last_x + *last_w - x0).abs() < 0.02
                                    {
                                        *last_w = (x0 + end) - *last_x;
                                    } else {
                                        dec.highlights.push(Item::Rect { x: x0, y: 0.0, w: end, h: line.height, color });
                                    }
                                    let comment_ucolor = hsla(ibis_doc_model::Color::opaque(COMMENT_BUBBLE_ICON));
                                    if let Some(last_u) = dec.underlines.last_mut()
                                        && last_u.style == UnderlineStyle::Dash
                                        && last_u.color == comment_ucolor
                                        && (last_u.baseline - y_base).abs() < 0.01
                                        && (last_u.size - s_size).abs() < 0.01
                                        && (last_u.x + last_u.w - x0).abs() < 0.02
                                    {
                                        last_u.w = (x0 + end) - last_u.x;
                                    } else {
                                        dec.underlines.push(UnderlineBox {
                                            style: UnderlineStyle::Dash,
                                            size: s_size,
                                            x: x0,
                                            w: end,
                                            baseline: y_base,
                                            color: comment_ucolor,
                                        });
                                    }
                                }
                            }
                            if let Some(h) = s.highlight && end > 0.0 {
                                let color = hsla(ibis_doc_model::Color::opaque(h));
                                if let Some(Item::Rect { x: last_x, w: last_w, color: last_c, .. }) = dec.highlights.last_mut()
                                    && *last_c == color
                                    && (*last_x + *last_w - x0).abs() < 0.02
                                {
                                    *last_w = (x0 + end) - *last_x;
                                } else {
                                    dec.highlights.push(Item::Rect { x: x0, y: 0.0, w: end, h: line.height, color });
                                }
                                // A stripe or a grid over the fill.
                                if let Some((pattern, pattern_color)) = s.shd_pattern {
                                    dec.highlights.push(Item::Shading { pattern, color: hsla(ibis_doc_model::Color::opaque(pattern_color)), x: x0, y: 0.0, w: end, h: line.height });
                                }
                            }
                            if let Some(ref b) = s.border && end > 0.0 {
                                let run_start = x0;
                                let run_end = x0 + end;
                                let pad_v = pt(b.space);
                                let extra_bw = (pt(b.total()) - pt(b.width)).max(0.0);
                                let y_top = (y_base - run_ascent - 1.2 - pad_v).max(0.0);
                                let y_bottom = y_base + (run_line - run_ascent) + 2.8 + pad_v + 2.0 * extra_bw;
                                if let Some(last) = dec.borders.last_mut()
                                    && last.border == *b
                                    && (run_start - last.x_end).abs() < 2.0
                                {
                                    last.x_end = run_end;
                                    last.y_top = last.y_top.min(y_top);
                                    last.y_bottom = last.y_bottom.max(y_bottom);
                                } else {
                                    dec.borders.push(BorderBox {
                                        border: *b,
                                        x_start: run_start,
                                        x_end: run_end,
                                        y_top,
                                        y_bottom,
                                    });
                                }
                            }
                            let color = hsla(ibis_doc_model::Color::opaque(s.color));
                            if s.underline && end > 0.0 {
                                let ucolor = s.underline_color.map_or(color, |c| hsla(ibis_doc_model::Color::opaque(c)));
                                let mut spans = Vec::new();
                                if s.underline_style == UnderlineStyle::Words {
                                    let mut start: Option<f32> = None;
                                    for (i, c) in text.chars().enumerate() {
                                        let (a, z) = (offsets[i], offsets[i + 1]);
                                        match (c.is_whitespace(), start) {
                                            (false, None) => start = Some(a),
                                            (true, Some(st)) => {
                                                spans.push((st, a));
                                                start = None;
                                            }
                                            _ => {}
                                        }
                                        if i + 2 == offsets.len() && let Some(st) = start {
                                            spans.push((st, z));
                                        }
                                    }
                                } else {
                                    spans.push((0.0, end));
                                }
                                let u_size = s_size;
                                for (a, z) in spans {
                                    let ux = x0 + a;
                                    let uw = z - a;
                                    if let Some(last_u) = dec.underlines.last_mut()
                                        && last_u.style == s.underline_style
                                        && last_u.color == ucolor
                                        && (last_u.baseline - y_base).abs() < 0.01
                                        && (last_u.size - u_size).abs() < 0.01
                                        && (last_u.x + last_u.w - ux).abs() < 0.02
                                    {
                                        last_u.w = (ux + uw) - last_u.x;
                                    } else {
                                        dec.underlines.push(UnderlineBox {
                                            style: s.underline_style,
                                            size: u_size,
                                            x: ux,
                                            w: uw,
                                            baseline: y_base,
                                            color: ucolor,
                                        });
                                    }
                                }
                            }
                            if (s.strike || s.double_strike) && end > 0.0 {
                                let strike_size = s_size;
                                // A double one: two thinner lines about the single one.
                                let lines: Vec<(f32, f32)> = if s.double_strike {
                                    let sh = (strike_size / 13.0).max(0.4);
                                    vec![(y_base - strike_size * 0.3 - strike_size * 0.08, sh), (y_base - strike_size * 0.3 + strike_size * 0.08, sh)]
                                } else {
                                    vec![(y_base - strike_size * 0.3, (strike_size / 18.0).max(0.4))]
                                };
                                for (sy, sh) in lines {
                                    if let Some(last_s) = dec.strikes.iter_mut().rev().find(|l| (l.y - sy).abs() < 0.01)
                                        && last_s.color == color
                                        && (last_s.h - sh).abs() < 0.01
                                        && (last_s.x + last_s.w - x0).abs() < 0.02
                                    {
                                        last_s.w = (x0 + end) - last_s.x;
                                    } else {
                                        dec.strikes.push(StrikeBox { x: x0, y: sy, w: end, h: sh, color });
                                    }
                                }
                            }
                            line.items.extend(self.emphasis_marks(s, style == LABEL, x0, y_base, s_size, &text, &offsets, color));
                            if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                                line.items.extend(self.enclosed_items(s, run, x0, y_base, end, s_size, color));
                            }
                            if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                                line.items.extend(self.ruby_items(s, &run.kind, x0, y_base, end, color));
                            }
                            let combined = p.runs.get(style).filter(|_| style != LABEL).and_then(|r| r.style.combine);
                            if let Some(c) = combined {
                                line.items.extend(self.combine_items(s, c, &text, x0, y_base, s_size, color));
                            }
                            let is_tate = p.runs.get(style).filter(|_| style != LABEL).is_some_and(|r| r.style.tate_chu_yoko);
                            if is_tate {
                                line.items.push(Item::Turned { x: x0 + s_size * 0.88, y: y_base, text: text.clone().into(), font: font.clone(), size: s_size, color, turns: -1 });
                            }
                            let mut offsets = offsets;
                            offsets.pop();
                            if combined.is_none() && !is_tate {
                                push_styled_text_items(&mut line.items, s, x0, y_base, text, offsets, font, size, color);
                            }
                        }
                    };
                    for (k, u) in slice.iter().enumerate() {
                        if let Some(&(position, true)) = positions.get(seg.s + k) {
                            stops.push((position, x - line_x));
                        }
                        match u {
                            Unit::Char { c, width, inset, font, size, style, .. } => {
                                let same = run.as_ref().is_some_and(|r| r.5 == *style && r.3 == *font && r.4 == *size);
                                if !same {
                                    flush(&mut run, &mut line, &mut dec);
                                    run = Some((x, String::new(), vec![0.0], font.clone(), *size, *style));
                                }
                                let r = run.as_mut().expect("a run");
                                r.1.push(*c);
                                let at = x - r.0 + inset;
                                r.2.pop();
                                r.2.push(at);
                                x += width + if expands.get(k).copied().unwrap_or(false) { extra } else { 0.0 };
                                r.2.push(x - r.0);
                            }
                            Unit::ListTab if left > x + 0.01 => {
                                flush(&mut run, &mut line, &mut dec);
                                x = left;
                            }
                            Unit::Tab | Unit::ListTab => {
                                flush(&mut run, &mut line, &mut dec);
                                let stop_list: Vec<_> = p.tabs.iter().filter(|t| pt(t.position) > x + 0.01).collect();
                                let from = x;
                                match stop_list.first() {
                                    Some(t) if matches!(t.align, TabAlign::Right | TabAlign::Center) => {
                                        let rest: f32 = slice[k + 1..].iter().take_while(|u| !matches!(u, Unit::Tab)).map(unit_width).sum();
                                        let stop = pt(t.position);
                                        x = if t.align == TabAlign::Right { stop - rest } else { stop - rest / 2.0 }.max(x);
                                    }
                                    Some(t) => x = pt(t.position),
                                    None => x = ((x / tab_default).floor() + 1.0) * tab_default,
                                }
                                if stop_list.first().is_some_and(|t| t.leader) {
                                    let size = p.runs.first().map_or(10.5, |r| r.style.size as f32 / 100.0);
                                    let (step, dot) = (size * 0.3, (size * 0.08).max(0.5));
                                    let color = hsla(ibis_doc_model::Color::opaque(p.runs.first().map_or(ibis_doc_model::Rgb::BLACK, |r| r.style.color)));
                                    let mut d = (from / step).ceil() * step + step / 2.0;
                                    while d + dot < x - step / 2.0 {
                                        line.items.push(Item::Rect { x: d, y: line.baseline - dot, w: dot, h: dot, color });
                                        d += step;
                                    }
                                }
                            }
                            Unit::Picture { media, w, h } => {
                                flush(&mut run, &mut line, &mut dec);
                                line.items.push(Item::Picture { media: *media, x, y: line.baseline - h, w: *w, h: *h });
                                x += w;
                            }
                            Unit::Math { boxed, w, font, color, .. } => {
                                flush(&mut run, &mut line, &mut dec);
                                let items = self.math_items(boxed, x, line.baseline, font, *color);
                                line.items.extend(items);
                                x += w;
                            }
                            Unit::Shape { .. } => {
                                flush(&mut run, &mut line, &mut dec);
                            }
                            Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {}
                        }
                    }
                    flush(&mut run, &mut line, &mut dec);
                    dec.finish(&mut line);
                    let end = positions.get(seg.e).map_or(p.len(), |&(position, _)| position);
                    stops.push((end, x - line_x));
                    line.items.push(Item::Line { para: p.id, x: line_x, y: 0.0, h: line.height, stops: stops.into(), vertical: false });
                }
                out.push(line);
            }
            return out;
        }

        // Split into lines: (start, end, forced end).
        let mut lines: Vec<(usize, usize, bool)> = Vec::new();
        let mut start = 0;
        let mut x = 0.0;
        let mut i = 0;
        let mut last_break: Option<usize> = None;
        // How much the spaces so far on the line may give (justified text).
        let mut give = 0.0f32;
        while i < units.len() {
            let first_line = lines.is_empty();
            let room = width - right - line_start_x(first_line);
            if i == start {
                give = 0.0;
            }
            match &units[i] {
                Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {
                    lines.push((start, i + 1, true));
                    if i > 0 && let Unit::Char { width, auto_space, .. } = &mut units[i - 1] {
                        *width -= *auto_space;
                        *auto_space = 0.0;
                    }
                    start = i + 1;
                    x = 0.0;
                    last_break = None;
                    i += 1;
                    continue;
                }
                Unit::ListTab if left > line_start_x(first_line) + x + 0.01 => {
                    x = left - line_start_x(first_line);
                    i += 1;
                    continue;
                }
                Unit::Tab | Unit::ListTab => {
                    let at = line_start_x(first_line) + x;
                    let tab = p.tabs.iter().find(|t| pt(t.position) > at + 0.01);
                    let stop = tab.map(|t| pt(t.position)).unwrap_or(((at / tab_default).floor() + 1.0) * tab_default);
                    // Text after a right or centre stop ends at (or centres on) it.
                    let rest: f32 = units[i + 1..].iter().take_while(|u| !matches!(u, Unit::Tab | Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak)).map(unit_width).sum();
                    let stop = match tab.map(|t| t.align) {
                        Some(TabAlign::Right) => (stop - rest).max(at),
                        Some(TabAlign::Center) => (stop - rest / 2.0).max(at),
                        _ => stop,
                    };
                    x = stop - line_start_x(first_line);
                    i += 1;
                    continue;
                }
                _ => {}
            }
            let w = unit_width(&units[i]);
            let c = unit_char(&units[i]);
            // A break is allowed before this unit?
            if i > start {
                let prev = unit_char(&units[i - 1]);
                if may_break(prev, c, p.kinsoku, p.word_wrap, self.doc) {
                    last_break = Some(i);
                }
            }
            if x + w > room + give + 0.01 && i > start {
                // Spaces at the end of a line hang past the edge.
                if c == Some(' ') {
                    x += w;
                    i += 1;
                    continue;
                }
                // Punctuation hangs past the edge if overflow_punct is enabled.
                if p.overflow_punct && c.is_some_and(|ch| "、。，．,".contains(ch)) && x <= room + 0.01 { // i18n-ok（オーバーフロー句読点の文字の集合。文字の並び）
                    x += w;
                    i += 1;
                    continue;
                }
                let at = match last_break {
                    Some(b) if b > start => b,
                    _ => i,
                };
                lines.push((start, at, false));
                if at > 0 && let Unit::Char { width, auto_space, .. } = &mut units[at - 1] {
                    *width -= *auto_space;
                    *auto_space = 0.0;
                }
                start = at;
                x = 0.0;
                last_break = None;
                i = at;
                continue;
            }
            if c == Some(' ') && p.align == FlowAlign::Justify {
                give += w * SPACE_GIVE;
            }
            x += w;
            i += 1;
        }
        if start < units.len() || lines.is_empty() {
            lines.push((start, units.len(), true));
            if let Some(Unit::Char { width, auto_space, .. }) = units.last_mut() {
                *width -= *auto_space;
                *auto_space = 0.0;
            }
        }

        let mark_size = p.mark.size as f32 / 100.0;
        let mark_metrics = {
            self.mark_metrics(p, mark_size)
        };
        let mut out = Vec::new();
        for (n, &(s, e, forced)) in lines.iter().enumerate() {
            let slice = &units[s..e];
            let largest = slice.iter().filter_map(|u| if let Unit::Char { size, .. } = u { Some(*size) } else { None }).fold(0.0, f32::max);
            let largest = if largest > 0.0 { largest } else { mark_size };
            // The line's text height and ascent: the tallest typeface's (the
            // paragraph mark's on an empty line).
            let (text_line, text_ascent, lead) = slice.iter().filter_map(|u| if let Unit::Char { c, line, ascent, lead, style, size, .. } = u { let (line, ascent) = mark_room(p, *c, *style, *size, *line, *ascent); Some((line, ascent, *lead)) } else { None }).fold((0.0f32, 0.0f32, 0.0f32), |(l, a, d), (l2, a2, d2)| (l.max(l2), a.max(a2), d.max(d2)));
            let (text_line, text_ascent, lead) = if text_line > 0.0 { (text_line, text_ascent, lead) } else { mark_metrics };
            let picture = slice.iter().filter_map(|u| match u { Unit::Picture { h, .. } | Unit::Shape { h, .. } | Unit::Math { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
            let natural = text_line.max(picture);
            let lead = if picture > text_line { 0.0 } else { lead };
                let height = self.resolve_line_height(p, natural, lead, in_table);
            // Where Word puts the baseline in a line: see `baseline_in_line`.
            let math_below = slice.iter().filter_map(|u| if let Unit::Math { descent, .. } = u { Some(*descent) } else { None }).fold(0.0f32, f32::max);
            let baseline = if picture > text_line { height - math_below } else { self.baseline_in_line(p, natural, lead, text_line, text_ascent, in_table) };
            let start_x = line_start_x(n == 0);
            let room = width - right - start_x;
            // Width used (without trailing spaces), and the gaps to spread
            // justification over.
            let visible_end = slice.iter().rposition(|u| unit_char(u).is_none_or(|c| c != ' ')).map_or(0, |p| p + 1);
            let used: f32 = slice[..visible_end].iter().map(unit_width).sum();
            let last = n + 1 == lines.len() || forced;
            let (extra, expands) = justify_extra(slice, visible_end, p.align, last, room, used);
            // A displayed formula alone on its line is centred.
            let align = if slice.iter().any(|u| matches!(u, Unit::Math { display: true, .. })) && slice.iter().all(|u| !matches!(u, Unit::Char { c, .. } if !c.is_whitespace())) { FlowAlign::Center } else { p.align };
            let shift = match align {
                FlowAlign::Center => ((room - used) / 2.0).max(0.0),
                FlowAlign::Right => (room - used).max(0.0),
                FlowAlign::Left if p.bidi => (room - used).max(0.0),
                _ => 0.0,
            };
            let has_revision = slice.iter().any(|u| {
                if let Unit::Char { style, .. } = u {
                    if *style != LABEL {
                        p.runs.get(*style).is_some_and(|r| r.revision.as_ref().is_some_and(|rev| is_reviewer_shown(&rev.author)))
                    } else {
                        false
                    }
                } else {
                    false
                }
            });
            let mut comment_marks = Vec::new();
            for u in slice {
                if let Unit::Char { style, .. } = u {
                    if *style != LABEL {
                        if let Some(c_ids) = self.comment_runs.get(&(p.id, *style)) {
                            for &id in c_ids {
                                if !comment_marks.contains(&id) {
                                    comment_marks.push(id);
                                }
                            }
                        }
                        if let Some(r) = p.runs.get(*style) {
                            match r.kind {
                                FlowRunKind::CommentStart(id) | FlowRunKind::CommentRef(id) => {
                                    if !comment_marks.contains(&id) {
                                        comment_marks.push(id);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            let mut line = LineBox { height, baseline, items: Vec::new(), page_break: slice.iter().any(|u| matches!(u, Unit::PageBreak)), column_break: slice.iter().any(|u| matches!(u, Unit::ColumnBreak)), has_revision, comment_marks };
            let mut x = start_x + shift;
            let line_x = x;
            let mut stops: Vec<(usize, f32)> = Vec::new();
            let mut run: Option<OpenRun> = None;
            let mut dec = LineDecorations::default();
            let is_all_markup = self.markup_mode() == MarkupDisplayMode::All;
            let flush = |run: &mut Option<OpenRun>, line: &mut LineBox, dec: &mut LineDecorations| {
                if let Some((x0, text, offsets, font, size, style)) = run.take() {
                    let s = if style == LABEL { &p.list_style } else { &p.runs[style].style };
                    // Tracked changes are shown: insertions underlined and
                    // deletions struck through, in the changes' colour.
                    let revised;
                    let is_hidden_dotted = p.runs.get(style).is_some_and(|r| r.style.hidden && !self.res.print && showing_hidden_text() && style != LABEL);
                    let active_rev = p.runs.get(style).and_then(|r| r.revision.as_ref()).filter(|r| is_reviewer_shown(&r.author));
                    let s = match active_rev.map(|r| &r.kind).filter(|_| is_all_markup) {
                        Some(ibis_doc_model::RevisionKind::Insert) if style != LABEL => {
                            revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Single, color: REVISION_COLOR, ..s.clone() };
                            &revised
                        }
                        Some(ibis_doc_model::RevisionKind::Delete) if style != LABEL => {
                            revised = ibis_doc_model::FlowRunStyle { strike: true, color: REVISION_COLOR, ..s.clone() };
                            &revised
                        }
                        Some(ibis_doc_model::RevisionKind::MoveTo) if style != LABEL => {
                            revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Single, color: MOVE_REVISION_COLOR, ..s.clone() };
                            &revised
                        }
                        Some(ibis_doc_model::RevisionKind::MoveFrom) if style != LABEL => {
                            revised = ibis_doc_model::FlowRunStyle { strike: true, color: MOVE_REVISION_COLOR, ..s.clone() };
                            &revised
                        }
                        _ if is_hidden_dotted => {
                            revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Dotted, ..s.clone() };
                            &revised
                        }
                        _ => s,
                    };
                    let s_size = s.size as f32 / 100.0 * if s.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
                    let raise = match s.position {
                        VerticalPosition::Superscript => -largest * 0.35,
                        VerticalPosition::Subscript => largest * 0.1,
                        VerticalPosition::Baseline => 0.0,
                    };
                    let font_name = text.chars().next().and_then(|c| s.font_for(c)).or_else(|| s.font_for('あ'));
                    let (run_ascent, run_line) = self.line_metrics(font_name, &font, s_size);
                    let run_descent = run_line - run_ascent;
                    let align_shift = match p.text_alignment {
                        ibis_doc_model::FontAlign::Top => run_ascent - text_ascent,
                        ibis_doc_model::FontAlign::Center => ((run_ascent - run_descent) - (text_ascent - (text_line - text_ascent))) / 2.0,
                        ibis_doc_model::FontAlign::Bottom => (text_line - text_ascent) - run_descent,
                        ibis_doc_model::FontAlign::Baseline | ibis_doc_model::FontAlign::Auto => 0.0,
                    };
                    let y_base = line.baseline + raise + align_shift;
                    let end = offsets.last().copied().unwrap_or(0.0);
                    let show_comments = self.markup_mode() != MarkupDisplayMode::None && self.markup_mode() != MarkupDisplayMode::Original;
                    if show_comments && style != LABEL && end > 0.0 {
                        if let Some(c_ids) = self.comment_runs.get(&(p.id, style)) && !c_ids.is_empty() {
                            let color = hsla(COMMENT_HIGHLIGHT_COLOR);
                            if let Some(Item::Rect { x: last_x, w: last_w, color: last_c, .. }) = dec.highlights.last_mut()
                                && *last_c == color
                                && (*last_x + *last_w - x0).abs() < 0.02
                            {
                                *last_w = (x0 + end) - *last_x;
                            } else {
                                dec.highlights.push(Item::Rect { x: x0, y: 0.0, w: end, h: line.height, color });
                            }
                            let comment_ucolor = hsla(ibis_doc_model::Color::opaque(COMMENT_BUBBLE_ICON));
                            if let Some(last_u) = dec.underlines.last_mut()
                                && last_u.style == UnderlineStyle::Dash
                                && last_u.color == comment_ucolor
                                && (last_u.baseline - y_base).abs() < 0.01
                                && (last_u.size - s_size).abs() < 0.01
                                && (last_u.x + last_u.w - x0).abs() < 0.02
                            {
                                last_u.w = (x0 + end) - last_u.x;
                            } else {
                                dec.underlines.push(UnderlineBox {
                                    style: UnderlineStyle::Dash,
                                    size: s_size,
                                    x: x0,
                                    w: end,
                                    baseline: y_base,
                                    color: comment_ucolor,
                                });
                            }
                        }
                    }
                    if let Some(h) = s.highlight && end > 0.0 {
                        let color = hsla(ibis_doc_model::Color::opaque(h));
                        if let Some(Item::Rect { x: last_x, w: last_w, color: last_c, .. }) = dec.highlights.last_mut()
                            && *last_c == color
                            && (*last_x + *last_w - x0).abs() < 0.02
                        {
                            *last_w = (x0 + end) - *last_x;
                        } else {
                            dec.highlights.push(Item::Rect { x: x0, y: 0.0, w: end, h: line.height, color });
                        }
                        // A stripe or a grid over the fill.
                        if let Some((pattern, pattern_color)) = s.shd_pattern {
                            dec.highlights.push(Item::Shading { pattern, color: hsla(ibis_doc_model::Color::opaque(pattern_color)), x: x0, y: 0.0, w: end, h: line.height });
                        }
                    }
                    if let Some(ref b) = s.border && end > 0.0 {
                        let run_start = x0;
                        let run_end = x0 + end;
                        let pad_v = pt(b.space);
                        let extra_bw = (pt(b.total()) - pt(b.width)).max(0.0);
                        let y_top = (y_base - run_ascent - 1.2 - pad_v).max(0.0);
                        let y_bottom = y_base + (run_line - run_ascent) + 2.8 + pad_v + 2.0 * extra_bw;
                        if let Some(last) = dec.borders.last_mut()
                            && last.border == *b
                            && (run_start - last.x_end).abs() < 2.0
                        {
                            last.x_end = run_end;
                            last.y_top = last.y_top.min(y_top);
                            last.y_bottom = last.y_bottom.max(y_bottom);
                        } else {
                            dec.borders.push(BorderBox {
                                border: *b,
                                x_start: run_start,
                                x_end: run_end,
                                y_top,
                                y_bottom,
                            });
                        }
                    }
                    let color = hsla(ibis_doc_model::Color::opaque(s.color));
                    if s.underline && end > 0.0 {
                        let ucolor = s.underline_color.map_or(color, |c| hsla(ibis_doc_model::Color::opaque(c)));
                        // The spans underlined: all of it, or the words.
                        let mut spans = Vec::new();
                        if s.underline_style == UnderlineStyle::Words {
                            let mut start: Option<f32> = None;
                            for (i, c) in text.chars().enumerate() {
                                let (a, z) = (offsets[i], offsets[i + 1]);
                                match (c.is_whitespace(), start) {
                                    (false, None) => start = Some(a),
                                    (true, Some(st)) => {
                                        spans.push((st, a));
                                        start = None;
                                    }
                                    _ => {}
                                }
                                if i + 2 == offsets.len() && let Some(st) = start {
                                    spans.push((st, z));
                                }
                            }
                        } else {
                            spans.push((0.0, end));
                        }
                        let u_size = s_size;
                        for (a, z) in spans {
                            let ux = x0 + a;
                            let uw = z - a;
                            if let Some(last_u) = dec.underlines.last_mut()
                                && last_u.style == s.underline_style
                                && last_u.color == ucolor
                                && (last_u.baseline - y_base).abs() < 0.01
                                && (last_u.size - u_size).abs() < 0.01
                                && (last_u.x + last_u.w - ux).abs() < 0.02
                            {
                                last_u.w = (ux + uw) - last_u.x;
                            } else {
                                dec.underlines.push(UnderlineBox {
                                    style: s.underline_style,
                                    size: u_size,
                                    x: ux,
                                    w: uw,
                                    baseline: y_base,
                                    color: ucolor,
                                });
                            }
                        }
                    }
                    if (s.strike || s.double_strike) && end > 0.0 {
                        let strike_size = s_size;
                        let lines: Vec<(f32, f32)> = if s.double_strike {
                            let sh = (strike_size / 13.0).max(0.4);
                            vec![(y_base - strike_size * 0.3 - strike_size * 0.08, sh), (y_base - strike_size * 0.3 + strike_size * 0.08, sh)]
                        } else {
                            vec![(y_base - strike_size * 0.3, (strike_size / 18.0).max(0.4))]
                        };
                        for (sy, sh) in lines {
                            if let Some(last_s) = dec.strikes.iter_mut().rev().find(|l| (l.y - sy).abs() < 0.01)
                                && last_s.color == color
                                && (last_s.h - sh).abs() < 0.01
                                && (last_s.x + last_s.w - x0).abs() < 0.02
                            {
                                last_s.w = (x0 + end) - last_s.x;
                            } else {
                                dec.strikes.push(StrikeBox { x: x0, y: sy, w: end, h: sh, color });
                            }
                        }
                    }
                    line.items.extend(self.emphasis_marks(s, style == LABEL, x0, y_base, s_size, &text, &offsets, color));
                    if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                        line.items.extend(self.enclosed_items(s, run, x0, y_base, end, s_size, color));
                    }
                    if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                        line.items.extend(self.ruby_items(s, &run.kind, x0, y_base, end, color));
                    }
                    let combined = p.runs.get(style).filter(|_| style != LABEL).and_then(|r| r.style.combine);
                    if let Some(c) = combined {
                        line.items.extend(self.combine_items(s, c, &text, x0, y_base, s_size, color));
                    }
                    let is_tate = p.runs.get(style).filter(|_| style != LABEL).is_some_and(|r| r.style.tate_chu_yoko);
                    if is_tate {
                        line.items.push(Item::Turned { x: x0 + s_size * 0.88, y: y_base, text: text.clone().into(), font: font.clone(), size: s_size, color, turns: -1 });
                    }
                    let mut offsets = offsets;
                    offsets.pop();
                    if combined.is_none() && !is_tate {
                        push_styled_text_items(&mut line.items, s, x0, y_base, text, offsets, font, size, color);
                    }
                }
            };
            for (k, u) in slice.iter().enumerate() {
                if let Some(&(position, true)) = positions.get(s + k) {
                    stops.push((position, x - line_x));
                }
                match u {
                    Unit::Char { c, width, inset, font, size, style, .. } => {
                        let same = run.as_ref().is_some_and(|r| r.5 == *style && r.3 == *font && r.4 == *size);
                        if !same {
                            flush(&mut run, &mut line, &mut dec);
                            run = Some((x, String::new(), vec![0.0], font.clone(), *size, *style));
                        }
                        let r = run.as_mut().expect("a run");
                        // A glyph wider than its fixed-width cell is centred
                        // in it (the substitute's Latin letters).
                        r.1.push(*c);
                        let at = x - r.0 + inset;
                        r.2.pop();
                        r.2.push(at);
                        x += width + if expands.get(k).copied().unwrap_or(false) { extra } else { 0.0 };
                        r.2.push(x - r.0);
                    }
                    Unit::ListTab if left > x + 0.01 => {
                        flush(&mut run, &mut line, &mut dec);
                        x = left;
                    }
                    Unit::Tab | Unit::ListTab => {
                        flush(&mut run, &mut line, &mut dec);
                        let stop_list: Vec<_> = p.tabs.iter().filter(|t| pt(t.position) > x + 0.01).collect();
                        let from = x;
                        match stop_list.first() {
                            Some(t) if matches!(t.align, TabAlign::Right | TabAlign::Center) => {
                                // The text after the tab ends at (or centres on) the stop.
                                let rest: f32 = slice[k + 1..].iter().take_while(|u| !matches!(u, Unit::Tab)).map(unit_width).sum();
                                let stop = pt(t.position);
                                x = if t.align == TabAlign::Right { stop - rest } else { stop - rest / 2.0 }.max(x);
                            }
                            Some(t) => x = pt(t.position),
                            None => x = ((x / tab_default).floor() + 1.0) * tab_default,
                        }
                        // A leader: dots from the text before to the text after.
                        if stop_list.first().is_some_and(|t| t.leader) {
                            let size = p.runs.first().map_or(10.5, |r| r.style.size as f32 / 100.0);
                            let (step, dot) = (size * 0.3, (size * 0.08).max(0.5));
                            let color = hsla(ibis_doc_model::Color::opaque(p.runs.first().map_or(ibis_doc_model::Rgb::BLACK, |r| r.style.color)));
                            let mut d = (from / step).ceil() * step + step / 2.0;
                            while d + dot < x - step / 2.0 {
                                line.items.push(Item::Rect { x: d, y: line.baseline - dot, w: dot, h: dot, color });
                                d += step;
                            }
                        }
                    }
                    Unit::Picture { media, w, h } => {
                        flush(&mut run, &mut line, &mut dec);
                        line.items.push(Item::Picture { media: *media, x, y: line.baseline - h, w: *w, h: *h });
                        x += w;
                    }
                    Unit::Math { boxed, w, font, color, .. } => {
                        flush(&mut run, &mut line, &mut dec);
                        let items = self.math_items(boxed, x, line.baseline, font, *color);
                        line.items.extend(items);
                        x += w;
                    }
                    Unit::Shape { shape, w, h } => {
                        flush(&mut run, &mut line, &mut dec);
                        if let Some(f) = &shape.floating {
                            if f.wrap == "topAndBottom" {
                                continue;
                            }
                            let item = self.floating_item(shape, x, width, line.height, page_number);
                            if f.behind_doc {
                                line.items.insert(0, item);
                            } else {
                                line.items.push(item);
                            }
                        } else {
                            // A turned picture's own frame is centred in the room its turn takes.
                            let (fw, fh) = (pt(shape.frame.width), pt(shape.frame.height));
                            let turned = matches!(shape.kind, ibis_doc_model::ShapeKind::Picture(_)) && shape.rotation != 0;
                            let (fx, fy, fw, fh) = if turned { (x + (*w - fw) / 2.0, line.baseline - h + (*h - fh) / 2.0, fw, fh) } else { (x, line.baseline - h, *w, *h) };
                            line.items.push(Item::Shape { shape: shape.clone(), x: fx, y: fy, w: fw, h: fh });
                            x += w;
                        }
                    }
                    Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {}
                }
            }
            flush(&mut run, &mut line, &mut dec);
            dec.finish(&mut line);
            // Where the line's text ends: the next line's first position.
            let end = positions.get(e).map_or(p.len(), |&(position, _)| position);
            stops.push((end, x - line_x));
            line.items.push(Item::Line { para: p.id, x: line_x, y: 0.0, h: line.height, stops: stops.into(), vertical: false });
            out.push(line);
        }
        out
    }
}

/// A block's pieces as last laid out, and what they were laid out from.
struct CachedPieces {
    block: Block,
    labels: Vec<(u32, String)>,
    page: u32,
    counts: Option<(u32, u32, u32)>,
    pieces: Vec<Piece>,
}

thread_local! {
    /// Laid-out blocks, by (id, width, styles, in_table, line_pitch, print, print_hidden, print_markup): see `Layouter::pieces`.
    static PIECES: std::cell::RefCell<std::collections::HashMap<(u32, u32, usize, bool, Option<Emu>, bool, bool, bool), CachedPieces>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Whether field `instruction` is one whose text depends on the page
/// (`PAGE` with a switch, `NUMPAGES`, `SECTIONPAGES`, `SECTION`, a formula
/// over them).
fn page_field(instruction: &str) -> bool {
    let t = instruction.trim_start();
    t.starts_with('=') || matches!(t.split_whitespace().next(), Some("PAGE" | "NUMPAGES" | "SECTIONPAGES" | "SECTION"))
}

/// Whether a block has fields whose text depends on the page or the file.
fn has_fields(block: &Block) -> bool {
    match block {
        Block::Paragraph(p) => p.runs.iter().any(|r| match &r.kind {
            FlowRunKind::PageNumber | FlowRunKind::FileName(_) | FlowRunKind::NoteRef { .. } | FlowRunKind::NoteMark => true,
            FlowRunKind::Field { instruction, .. } => page_field(instruction),
            _ => false,
        }),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(has_fields),
    }
}

/// Characters of one run being gathered on a line: where it starts, its
/// text, each character's offset (and the end), font, size and run index.
type OpenRun = (f32, String, Vec<f32>, Font, f32, usize);

fn unit_width(u: &Unit) -> f32 {
    match u {
        Unit::Char { width, .. } => *width,
        Unit::Picture { w, .. } | Unit::Shape { w, .. } | Unit::Math { w, .. } => *w,
        _ => 0.0,
    }
}

impl Layouter<'_> {
    /// The line an empty paragraph (or its mark) takes, its ascent and its
    /// leading: the taller of its mark's East Asian and Latin typefaces (Word's
    /// PDF of a paper: an empty footer line of Times New Roman with ＭＳ 明朝
    /// for East Asian text is Times' height, not Mincho's).
    fn mark_metrics(&self, p: &FlowParagraph, size: f32) -> (f32, f32, f32) {
        let metrics = |c: char| {
            let name = p.mark.font_for(c);
            let font = self.font(&p.mark, name);
            let (ascent, line) = self.line_metrics(name, &font, size);
            (line, ascent, Self::leading(name) * size)
        };
        let (east_asian, latin) = (metrics('あ'), metrics('a'));
        if latin.0 + latin.2 > east_asian.0 + east_asian.2 + 0.01 { latin } else { east_asian }
    }

    /// A floating shape of a paragraph placed as its anchor says, from the
    /// paragraph's line (`x` along it, `width` of text, the line `line_h` tall):
    /// x from the left margin, y from the line's top (or the top margin).
    fn floating_item(&self, shape: &ibis_doc_model::Shape, x: f32, width: f32, line_h: f32, page_number: u32) -> Item {
        let f = shape.floating.as_ref().expect("a floating shape");
        let sw = pt(shape.frame.width);
        let sh = pt(shape.frame.height);
        let page_w = pt(self.doc.page.size.width);
        let margin_l = pt(self.doc.page.left);
        let margin_r = pt(self.doc.page.right);
        let is_even = page_number % 2 == 0;
        // The column's left on the page, from the left margin.
        let col = self.column_dx.get();

        let (base_x0, base_w) = match f.relative_h.as_str() {
            "page" => (0.0, page_w),
            "leftMargin" => (0.0, margin_l),
            "rightMargin" => (page_w - margin_r, margin_r),
            "insideMargin" => if is_even { (page_w - margin_r, margin_r) } else { (0.0, margin_l) },
            "outsideMargin" => if is_even { (0.0, margin_l) } else { (page_w - margin_r, margin_r) },
            "character" => (margin_l + col + x, (width - x).max(0.0)),
            "margin" => (margin_l, (page_w - margin_l - margin_r).max(0.0)),
            _ /* "column" */ => (margin_l + col, width),
        };

        let abs_x = if f.relative_h == "character" {
            match f.align_h.as_deref() {
                Some("center") => margin_l + col + x - sw / 2.0,
                Some("right") => margin_l + col + x - sw,
                Some("left") => margin_l + col + x,
                _ => margin_l + col + x + pt(f.offset_x),
            }
        } else {
            match f.align_h.as_deref() {
                Some("left") => base_x0,
                Some("center") => base_x0 + (base_w - sw) / 2.0,
                Some("right") => base_x0 + base_w - sw,
                Some("inside") => if is_even { base_x0 + base_w - sw } else { base_x0 },
                Some("outside") => if is_even { base_x0 } else { base_x0 + base_w - sw },
                _ => base_x0 + pt(f.offset_x),
            }
        };
        let abs_x = abs_x.clamp(0.0, (page_w - sw).max(0.0));
        let sx = abs_x - margin_l - col;
        let page_h = pt(self.doc.page.size.height);
        let margin_t = pt(self.doc.page.top);
        let margin_b = pt(self.doc.page.bottom);
        let sy = if f.relative_v == "line" || f.relative_v == "paragraph" {
            match f.align_v.as_deref() {
                Some("center") => (line_h - sh) / 2.0,
                Some("bottom") => line_h - sh,
                Some("top") => 0.0,
                _ => pt(f.offset_y),
            }
        } else {
            let (base_y0, base_h) = match f.relative_v.as_str() {
                "page" => (0.0, page_h),
                "margin" => (margin_t, (page_h - margin_t - margin_b).max(0.0)),
                "topMargin" => (0.0, margin_t),
                "bottomMargin" => (page_h - margin_b, margin_b),
                "insideMargin" => if is_even { (page_h - margin_b, margin_b) } else { (0.0, margin_t) },
                "outsideMargin" => if is_even { (0.0, margin_t) } else { (page_h - margin_b, margin_b) },
                _ => (margin_t, page_h),
            };
            let abs_y = match f.align_v.as_deref() {
                Some("top") => base_y0,
                Some("center") => if base_h > 0.0 { base_y0 + (base_h - sh) / 2.0 } else { base_y0 },
                Some("bottom") => if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 },
                Some("inside") => if is_even { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } } else { base_y0 },
                Some("outside") => if is_even { base_y0 } else { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } },
                _ => base_y0 + pt(f.offset_y),
            };
            abs_y - margin_t
        };
        Item::Shape { shape: Box::new(shape.clone()), x: sx, y: sy, w: sw, h: sh }
    }
}

fn unit_char(u: &Unit) -> Option<char> {
    match u {
        Unit::Char { c, .. } => Some(*c),
        _ => None,
    }
}

/// The narrowest room beside a shape lines go into (points): narrower, the
/// text goes on past the shape (and a shape leaving no wider room on either
/// side of a column is across it).
const MIN_WRAP_ROOM: f32 = 15.0;

/// How much of its width a space of a justified line may give to fit one
/// more word: Word's PDF of a paper has lines whose spaces are all down to
/// 72 % of the font's (1.70 pt for Arial's 2.36 pt at 8.5 pt), none below.
const SPACE_GIVE: f32 = 0.28;

/// The extra width at each gap of a justified line (negative where the line
/// is over its room: then only spaces give, see `SPACE_GIVE`), and where.
fn justify_extra(slice: &[Unit], visible_end: usize, align: FlowAlign, last: bool, room: f32, used: f32) -> (f32, Vec<bool>) {
    let has_tab = slice.iter().any(|u| matches!(u, Unit::Tab));
    if has_tab || !matches!(align, FlowAlign::Justify | FlowAlign::Distribute) {
        return (0.0, Vec::new());
    }
    if used > room + 0.01 && align == FlowAlign::Justify {
        let spaces: Vec<bool> = (0..visible_end).map(|k| k + 1 < visible_end && slice.get(k).and_then(unit_char) == Some(' ')).collect();
        let n = spaces.iter().filter(|s| **s).count();
        return if n > 0 { ((room - used) / n as f32, spaces) } else { (0.0, Vec::new()) };
    }
    if last && align == FlowAlign::Justify {
        return (0.0, Vec::new());
    }
    let expands = justify_gaps(slice, visible_end, align == FlowAlign::Distribute);
    let gaps = expands.iter().filter(|e| **e).count();
    if used < room && gaps > 0 { ((room - used) / gaps as f32, expands) } else { (0.0, Vec::new()) }
}

/// Which characters of a justified line take extra width after them, as
/// Word: spaces, and characters with a CJK one on either side (Latin words
/// keep their letters together); every character for 均等割り付け.
fn justify_gaps(slice: &[Unit], visible_end: usize, distribute: bool) -> Vec<bool> {
    (0..visible_end)
        .map(|k| {
            let Some(c) = slice.get(k).and_then(unit_char) else { return false };
            if c == '\u{200b}' || k + 1 >= visible_end {
                return false;
            }
            let next = slice.get(k + 1).and_then(unit_char);
            distribute || c == ' ' || is_cjk(c) || next.is_some_and(is_cjk)
        })
        .collect()
}

/// A character's line height and ascent with room for its 傍点 above
/// (`w:em` dot, comma, circle): Word's line grows by 0.36 em over Japanese
/// and 0.309 em over Latin letters (its PDF of DRUN at 14 pt: 5.04 pt for
/// 傍点の試験, 4.32 pt for Emphasis).
fn mark_room(p: &FlowParagraph, c: char, style: usize, size: f32, line: f32, ascent: f32) -> (f32, f32) {
    use ibis_doc_model::EmphasisMark;
    // ルビ: room for it over its base (its ascent, as the base's in its
    // size, `raise` above the baseline; Word's PDF of DW14).
    if let Some(FlowRunKind::Ruby { size: rs, raise, .. }) = p.runs.get(style).map(|r| &r.kind)
        && size > 0.0
    {
        let raised = ascent.max(*raise as f32 / 100.0 + ascent * (*rs as f32 / 100.0) / size);
        return (line + raised - ascent, raised);
    }
    let above = p.runs.get(style).is_some_and(|r| matches!(r.style.emphasis, EmphasisMark::Dot | EmphasisMark::Comma | EmphasisMark::Circle));
    if !above {
        return (line, ascent);
    }
    let extra = if is_cjk(c) { 0.36 } else { 0.3086 } * size;
    (line + extra, ascent + extra)
}

/// Whether a line may break between `before` and `after`.
pub(crate) fn may_break(before: Option<char>, after: Option<char>, kinsoku: bool, word_wrap: bool, doc: &FlowDocument) -> bool {
    let (Some(b), Some(a)) = (before, after) else { return true };
    // The lists the document sets (禁則文字の設定) replace the standard/strict ones.
    let cannot_start = match &doc.no_line_start { Some(list) => list.contains(a), None => no_line_start(a, doc.strict_first_and_last_chars) };
    let cannot_end = match &doc.no_line_end { Some(list) => list.contains(b), None => no_line_end(b) };
    if kinsoku && (cannot_start || cannot_end) {
        return false;
    }
    if b == ' ' {
        return true;
    }
    if !word_wrap {
        return true;
    }
    // After a hyphen within a word ("well-|known"), but not a minus before
    // a number (UAX #14's HY and BA classes).
    if (b == '-' && a.is_alphabetic()) || (matches!(b, '\u{2010}' | '\u{2012}' | '\u{2013}') && a.is_alphanumeric()) {
        return true;
    }
    // Latin words hold together; anything next to CJK may break.
    is_cjk(a) || is_cjk(b)
}

/// Where on the page the body is.
struct Frame {
    left: f32,
    top: f32,
    width: f32,
    bottom: f32,
}

/// A block laid out: its lines (paragraph) or rows (table), each a unit
/// that goes on one page, `x`/`y` relative to the block.
#[derive(Clone, Default)]
struct Piece {
    height: f32,
    /// The space before (of a paragraph's first line) and after (of its
    /// last) in `height`: next to each other the larger one is kept
    /// (Word's PDF of DWID).
    space_before: f32,
    space_after: f32,
    items: Vec<Item>,
    page_break: bool,
    /// Continue in the next column (the next page without columns).
    column_break: bool,
    keep_with_next: bool,
    /// A table's heading row, repeated at the top of each page the table
    /// continues on.
    header: bool,
    /// A table row that may be divided where a page ends (its cells and
    /// their content, to draw each part).
    split: Option<Arc<RowSplit>>,
}

/// A row that may go on to the next page, as its cells are drawn.
#[derive(Clone, Debug)]
struct RowSplit {
    cells: Vec<SplitCell>,
}

#[derive(Clone, Debug)]
struct SplitCell {
    left: f32,
    right: f32,
    fill: Option<Hsla>,
    /// The content, placed in the row (inside the borders and margins).
    content: Vec<Item>,
    /// From the row's top to the content (top border and margin), and from
    /// the content to the row's bottom (margin).
    top: f32,
    bottom: f32,
    /// Top, right, bottom and left, as drawn (after the neighbours').
    borders: [Option<FlowBorder>; 4],
}

/// A vertical cell's text laid out across (`items`, lines `across` thick
/// in all) turned into the cell's box (`x`, `y`, `w`, `h`): down with the
/// lines right to left (`TbRl`; Japanese upright, the rest turned
/// clockwise), down with the lines left to right (`TbLrV`), or up with
/// them left to right (`BtLr`, all turned anticlockwise); its vertical
/// alignment is across the lines (Word's PDF of DCTD: 上 is right for
/// `TbRl`, left for the others).
fn turn_cell_text(items: &[Item], direction: ibis_doc_model::CellDirection, anchor: CellAnchor, (bx, by, bw, bh): (f32, f32, f32, f32), across: f32) -> Vec<Item> {
    use ibis_doc_model::CellDirection;
    let room = (bw - across).max(0.0);
    let shift = match anchor {
        CellAnchor::Top => 0.0,
        CellAnchor::Center => room / 2.0,
        CellAnchor::Bottom => room,
    };
    let mut out = Vec::new();
    for it in items {
        let Item::Text { x, y, text, offsets, font, size, color } = it else { continue };
        for ((_, c), dx) in text.char_indices().zip(offsets.iter()) {
            if c.is_whitespace() || c == '\u{200b}' {
                continue;
            }
            let (u, v) = (x + dx, *y);
            let s: SharedString = c.to_string().into();
            match direction {
                CellDirection::TbRl | CellDirection::TbLrV => {
                    // Down: along the cell from its top; across from its
                    // right (lines right to left) or left.
                    let line_x = match direction {
                        CellDirection::TbRl => bx + bw - shift - v,
                        _ => bx + shift + (across - v),
                    };
                    if is_cjk(c) {
                        // Upright, centred on the line.
                        let mid = line_x + size * 0.35;
                        // 、。 take their vertical forms: the upper right of the box.
                        let (dx, dy) = if matches!(c, '、' | '。' | '，' | '．') { (size * 0.5, -size * 0.6) } else { (0.0, 0.0) };
                        out.push(Item::Text { x: mid - size / 2.0 + dx, y: by + u + size * 0.88 + dy, text: s, offsets: Arc::from([0.0f32]), font: font.clone(), size: *size, color: *color });
                    } else {
                        out.push(Item::Turned { x: line_x, y: by + u, text: s, font: font.clone(), size: *size, color: *color, turns: 1 });
                    }
                }
                CellDirection::BtLr => {
                    out.push(Item::Turned { x: bx + shift + v, y: by + bh - u, text: s, font: font.clone(), size: *size, color: *color, turns: -1 });
                }
                CellDirection::LrTb | CellDirection::LrTbV => {}
            }
        }
    }
    out
}

/// `lrTbV`'s text: across, its Japanese characters turned anticlockwise
/// in their places (Word's PDF of DCTD).
fn turn_japanese(items: impl Iterator<Item = Item>) -> Vec<Item> {
    let mut out = Vec::new();
    for it in items {
        let Item::Text { x, y, text, offsets, font, size, color } = &it else {
            out.push(it);
            continue;
        };
        let mut rest = String::new();
        let mut rest_offsets = Vec::new();
        for ((_, c), dx) in text.char_indices().zip(offsets.iter()) {
            if is_cjk(c) {
                // Its em box where the upright one's was.
                out.push(Item::Turned { x: x + dx + size * 0.88, y: y + size * 0.12, text: c.to_string().into(), font: font.clone(), size: *size, color: *color, turns: -1 });
            } else {
                rest.push(c);
                rest_offsets.push(*dx);
            }
        }
        if !rest.is_empty() {
            out.push(Item::Text { x: *x, y: *y, text: rest.into(), offsets: rest_offsets.into(), font: font.clone(), size: *size, color: *color });
        }
    }
    out
}

/// Where an item starts, downward (a character's top, from its baseline).
fn item_top(item: &Item) -> f32 {
    match item {
        Item::Text { y, size, .. } | Item::Turned { y, size, .. } => y - size * 0.8,
        Item::Rect { y, .. } | Item::Art { y, .. } | Item::Picture { y, .. } | Item::Shape { y, .. } | Item::Line { y, .. } | Item::Shading { y, .. } | Item::CommentBubble { y, .. } => *y,
        Item::Stroke { y, y2, .. } => y.min(*y2),
    }
}

impl RowSplit {
    /// The row's cells drawn `height` high with `contents`.
    fn draw(&self, contents: &[Vec<Item>], height: f32) -> Vec<Item> {
        let mut items = Vec::new();
        for (cell, content) in self.cells.iter().zip(contents) {
            if let Some(fill) = cell.fill {
                items.push(Item::Rect { x: cell.left, y: 0.0, w: cell.right - cell.left, h: height, color: fill });
            }
            items.extend(content.iter().cloned());
        }
        for cell in &self.cells {
            let [top, rgt, bottom, lft] = cell.borders;
            for (b, horizontal, outer_first, at, from, to) in [(top, true, true, 0.0, cell.left, cell.right), (bottom, true, false, height, cell.left, cell.right), (lft, false, true, cell.left, 0.0, height), (rgt, false, false, cell.right, 0.0, height)] {
                if let Some(b) = b {
                    items.extend(border_items(&b, horizontal, outer_first, at, from, to, false, false));
                }
            }
        }
        items
    }

    /// The row divided so that its first part fits in `room`: no line is
    /// cut, each cell's text goes on at its top on the next page. `None`
    /// when not even a line of each cell fits.
    fn split(&self, room: f32) -> Option<(Piece, Piece)> {
        let lines = |c: &SplitCell| -> Vec<(f32, f32)> { c.content.iter().filter_map(|it| if let Item::Line { y, h, .. } = it { Some((*y, y + h)) } else { None }).collect() };
        let mut cut = f32::INFINITY;
        for cell in &self.cells {
            let ls = lines(cell);
            if let Some(i) = ls.iter().position(|(_, b)| *b > room - cell.bottom + 0.01) {
                if i == 0 {
                    return None;
                }
                cut = cut.min(ls[i].0);
            }
        }
        if !cut.is_finite() {
            return None;
        }
        let mut first = Vec::new();
        let mut rest = Vec::new();
        let mut first_h = 0.0f32;
        let mut rest_h = 0.0f32;
        for cell in &self.cells {
            let (a, b): (Vec<Item>, Vec<Item>) = cell.content.iter().cloned().partition(|it| item_top(it) < cut - 0.01);
            let bottom_of = |items: &[Item]| items.iter().filter_map(|it| if let Item::Line { y, h, .. } = it { Some(y + h) } else { None }).fold(0.0f32, f32::max);
            first_h = first_h.max(bottom_of(&a).max(cell.top) + cell.bottom);
            let shift = cell.top - cut;
            let b: Vec<Item> = b.into_iter().map(|it| offset(it, 0.0, shift)).collect();
            rest_h = rest_h.max(bottom_of(&b).max(cell.top) + cell.bottom);
            first.push(a);
            rest.push(b);
        }
        let first_h = first_h.min(room);
        let piece = |items: Vec<Item>, height: f32, split: Option<Arc<RowSplit>>| Piece { height, items, page_break: false, column_break: false, keep_with_next: false, header: false, split, ..Default::default() };
        let rest_split = RowSplit { cells: self.cells.iter().zip(&rest).map(|(c, r)| SplitCell { content: r.clone(), ..c.clone() }).collect() };
        Some((piece(self.draw(&first, first_h), first_h, None), piece(self.draw(&rest, rest_h), rest_h, Some(Arc::new(rest_split)))))
    }
}

impl<'r> Layouter<'r> {
    /// The pieces of a block `width` wide: a paragraph's lines (the space
    /// before goes with the first, after with the last) or a table's rows.
    /// The pieces of a block, laid out again only when the block (or what
    /// it depends on: the width, the list numbers, the page number for a
    /// page-number field) changed since it was laid out last: an edit lays
    /// out only the blocks it changed.
    fn pieces(&self, block: &Block, width: f32, page_number: u32, obstacle: Option<&WrapObstacleInPara>, in_table: bool) -> Vec<Piece> {
        if obstacle.is_some() {
            return self.lay_out_block(block, width, page_number, obstacle, in_table);
        }
        let id = match block {
            Block::Paragraph(p) => p.id,
            Block::Table(t) => t.id,
        };
        let mut label = self.block_labels(block);
        // Its notes' numbers too (they change with the notes' settings).
        label.extend(self.block_note_numbers(block));
        let fields = has_fields(block);
        let styles = Arc::as_ptr(&self.doc.styles) as usize;
        let key = (id, width.to_bits(), styles, in_table, self.line_pitch.get(), self.res.print, self.res.print_hidden_text, self.res.print_markup);
        let hit = PIECES.with(|cache| {
            let cache = cache.borrow();
            cache.get(&key).filter(|c| c.block == *block && c.labels == label && (!fields || (c.page == page_number && c.counts == self.counts()))).map(|c| c.pieces.clone())
        });
        if let Some(pieces) = hit {
            return pieces;
        }
        let pieces = self.lay_out_block(block, width, page_number, None, in_table);
        PIECES.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() > 200_000 {
                cache.clear();
            }
            cache.insert(key, CachedPieces { block: block.clone(), labels: label, page: page_number, counts: self.counts(), pieces: pieces.clone() });
        });
        pieces
    }

    /// The numbers of the notes marked in a block, as (a key apart from the
    /// paragraphs' ids, the number shown).
    fn block_note_numbers(&self, block: &Block) -> Vec<(u32, String)> {
        fn walk(b: &Block, numbers: &std::collections::HashMap<(bool, u32), String>, out: &mut Vec<(u32, String)>) {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        if let FlowRunKind::NoteRef { endnote, id } = r.kind {
                            let key = u32::MAX - (id << 1) - u32::from(endnote);
                            out.push((key, numbers.get(&(endnote, id)).cloned().unwrap_or_default()));
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).for_each(|b| walk(b, numbers, out)),
            }
        }
        let mut out = Vec::new();
        walk(block, &self.note_numbers, &mut out);
        out
    }

    /// The list labels of a block's paragraphs (a table's too).
    fn block_labels(&self, block: &Block) -> Vec<(u32, String)> {
        fn walk(b: &Block, labels: &std::collections::HashMap<u32, String>, out: &mut Vec<(u32, String)>) {
            match b {
                Block::Paragraph(p) => {
                    if let Some(l) = labels.get(&p.id) {
                        out.push((p.id, l.clone()));
                    }
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            for b in &cell.blocks {
                                walk(b, labels, out);
                            }
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        if !self.labels.is_empty() {
            walk(block, &self.labels, &mut out);
        }
        out
    }

    fn lay_out_block(&self, block: &Block, width: f32, page_number: u32, obstacle: Option<&WrapObstacleInPara>, in_table: bool) -> Vec<Piece> {
        match block {
            Block::Paragraph(p) => {
                let lines = self.paragraph(p, width, page_number, obstacle, in_table);
                let count = lines.len();
                // Borders and shading: the box around the text, its borders
                // `space` away from it (outside the indents, and inside the
                // spacing before and after).
                let [top, right, bottom, left] = p.borders;
                let pad = |b: Option<FlowBorder>| b.map_or(0.0, |b| pt(b.space) + pt(b.total()));
                let (pad_top, pad_bottom) = (pad(top), pad(bottom));
                let boxed = p.shading.is_some() || p.shading_pattern.is_some() || p.borders.iter().any(Option::is_some);
                let text_left = pt(p.left.min(p.left + p.first_line));
                let text_right = width - pt(p.right);
                let inner_left = text_left - left.map_or(0.0, |b| pt(b.space));
                let inner_right = text_right + right.map_or(0.0, |b| pt(b.space));
                let hide_deleted = self.hides_deleted();
                let top_shapes: Vec<&Box<ibis_doc_model::Shape>> = p.runs.iter().filter(|r| !run_deleted(r, hide_deleted)).filter_map(|r| match &r.kind {
                    FlowRunKind::Shape(shape) if shape.floating.as_ref().is_some_and(|f| f.wrap == "topAndBottom") => Some(shape),
                    _ => None,
                }).collect();
                if top_shapes.is_empty() {
                    return lines
                        .into_iter()
                        .enumerate()
                        .map(|(i, l)| {
                            let before = if i == 0 { self.space_before(p) } else { 0.0 };
                            let after = if i + 1 == count { self.space_after(p) } else { 0.0 };
                            let above = if i == 0 { pad_top } else { 0.0 };
                            let below = if i + 1 == count { pad_bottom } else { 0.0 };
                            let mut items: Vec<Item> = Vec::new();
                            if boxed {
                                // This line's band of the box, from `y0` to `y1`.
                                let y0 = before + if i == 0 { top.map_or(0.0, |b| pt(b.total())) } else { 0.0 };
                                let y1 = before + above + l.height + below - if i + 1 == count { bottom.map_or(0.0, |b| pt(b.total())) } else { 0.0 };
                                if p.shading.is_some() || p.shading_pattern.is_some() {
                                    let bg = if p.shading_pattern.is_some() { p.shading_fill } else { p.shading };
                                    items.extend(shading_items(p.shading_pattern, bg, p.shading_color, inner_left, y0, inner_right - inner_left, y1 - y0));
                                }
                                let lx = inner_left - left.map_or(0.0, |b| pt(b.total()) / 2.0);
                                let rx = inner_right + right.map_or(0.0, |b| pt(b.total()) / 2.0);
                                if let (Some(b), 0) = (top, i) {
                                    items.extend(border_items(&b, true, true, before + pt(b.total()) / 2.0, lx, rx, left.is_some(), right.is_some()));
                                }
                                if let Some(b) = bottom.filter(|_| i + 1 == count) {
                                    items.extend(border_items(&b, true, false, y1 + pt(b.total()) / 2.0, lx, rx, left.is_some(), right.is_some()));
                                }
                                // The side borders go over the space before and after
                                // when no top or bottom border bounds it (Word's PDF of
                                // DWID: one rectangle per paragraph, end to end).
                                let ty = if i == 0 { if top.is_none() { 0.0 } else { before } } else { 0.0 };
                                let by = if i + 1 == count { y1 + bottom.map_or(after, |b| pt(b.total())) } else { before + above + l.height };
                                if let Some(b) = left {
                                    items.extend(border_items(&b, false, true, lx, ty, by, false, false));
                                }
                                if let Some(b) = right {
                                    items.extend(border_items(&b, false, false, rx, ty, by, false, false));
                                }
                            }
                            items.extend(l.items.into_iter().map(|it| offset(it, 0.0, before + above)));
                            let mode = self.markup_mode();
                            if (mode == MarkupDisplayMode::All || mode == MarkupDisplayMode::Simple) && l.has_revision && !in_table {
                                let margin_l = pt(self.doc.page.left);
                                let bar_x = (inner_left - 10.0).max(-margin_l + 8.0);
                                items.push(Item::Rect {
                                    x: bar_x,
                                    y: before + above,
                                    w: 1.5,
                                    h: l.height,
                                    color: hsla(ibis_doc_model::Color::opaque(REVISION_COLOR)),
                                });
                            }
                            if (mode == MarkupDisplayMode::All || mode == MarkupDisplayMode::Simple) && !l.comment_marks.is_empty() && !in_table {
                                let margin_l = pt(self.doc.page.left);
                                let bubble_size = 16.0;
                                let bubble_x = pt(self.doc.page.size.width) - margin_l + 8.0;
                                let bubble_y = before + above + (l.height - bubble_size).max(0.0) / 2.0;
                                items.push(Item::CommentBubble {
                                    x: bubble_x,
                                    y: bubble_y,
                                    size: bubble_size,
                                });
                            }
                            let page_break = l.page_break || (matches!(p.section_break, SectionBreak::NextPage | SectionBreak::OddPage | SectionBreak::EvenPage) && i + 1 == count);
                            let column_break = l.column_break || (p.section_break == SectionBreak::NextColumn && i + 1 == count);
                            Piece { height: before + above + l.height + below + after, items, page_break, column_break, keep_with_next: p.keep_with_next && i + 1 == count, header: false, split: None, space_before: before, space_after: after }
                        })
                        .collect();
                }

                let mut shape_pieces = Vec::new();
                let has_lines = count > 0;
                let page_w = pt(self.doc.page.size.width);
                let margin_l = pt(self.doc.page.left);
                let margin_r = pt(self.doc.page.right);
                let is_even = page_number % 2 == 0;
                for (si, shape) in top_shapes.iter().enumerate() {
                    let sw = pt(shape.frame.width);
                    let sh = pt(shape.frame.height);
                    let f = shape.floating.as_ref().unwrap();
                    let (base_x0, base_w) = match f.relative_h.as_str() {
                        "page" => (0.0, page_w),
                        "leftMargin" => (0.0, margin_l),
                        "rightMargin" => (page_w - margin_r, margin_r),
                        "insideMargin" => if is_even { (page_w - margin_r, margin_r) } else { (0.0, margin_l) },
                        "outsideMargin" => if is_even { (0.0, margin_l) } else { (page_w - margin_r, margin_r) },
                        "character" => (margin_l, width),
                        _ /* "column" | "margin" */ => (margin_l, width),
                    };

                    let abs_x = match f.align_h.as_deref() {
                        Some("left") => base_x0,
                        Some("center") => base_x0 + (base_w - sw) / 2.0,
                        Some("right") => base_x0 + base_w - sw,
                        Some("inside") => if is_even { base_x0 + base_w - sw } else { base_x0 },
                        Some("outside") => if is_even { base_x0 } else { base_x0 + base_w - sw },
                        _ => base_x0 + pt(f.offset_x),
                    };
                    let abs_x = abs_x.clamp(0.0, (page_w - sw).max(0.0));
                    let sx = abs_x - margin_l;
                    let before = if si == 0 { self.space_before(p) } else { 0.0 };
                    let above = if si == 0 { pad_top } else { 0.0 };
                    let after = if !has_lines && si + 1 == top_shapes.len() { self.space_after(p) } else { 0.0 };
                    let below = if !has_lines && si + 1 == top_shapes.len() { pad_bottom } else { 0.0 };
                    let dt = pt(f.dist_t);
                    let db = pt(f.dist_b);
                    // From the paragraph's top by its offset (Word: a logo raised a
                    // little over its header line), the text going on below it.
                    let (top, room) = if matches!(f.relative_v.as_str(), "paragraph" | "line") && f.align_v.is_none() {
                        let oy = pt(f.offset_y);
                        (oy, (oy + sh + db).max(0.0))
                    } else {
                        (dt, dt + sh + db)
                    };
                    let items = vec![Item::Shape {
                        shape: (*shape).clone(),
                        x: sx,
                        y: before + above + top,
                        w: sw,
                        h: sh,
                    }];
                    shape_pieces.push(Piece {
                        height: before + above + room + below + after,
                        items,
                        page_break: !has_lines && si + 1 == top_shapes.len() && matches!(p.section_break, SectionBreak::NextPage | SectionBreak::OddPage | SectionBreak::EvenPage),
                        column_break: !has_lines && si + 1 == top_shapes.len() && p.section_break == SectionBreak::NextColumn,
                        keep_with_next: !has_lines && si + 1 == top_shapes.len() && p.keep_with_next,
                        header: false,
                        split: None,
                        ..Default::default()
                    });
                }

                let text_pieces: Vec<Piece> = lines
                    .into_iter()
                    .enumerate()
                    .map(|(i, l)| {
                        let after = if i + 1 == count { self.space_after(p) } else { 0.0 };
                        let below = if i + 1 == count { pad_bottom } else { 0.0 };
                        let mut items: Vec<Item> = Vec::new();
                        if boxed {
                            let y0 = 0.0;
                            let y1 = l.height + below - if i + 1 == count { bottom.map_or(0.0, |b| pt(b.total())) } else { 0.0 };
                            if p.shading.is_some() || p.shading_pattern.is_some() {
                                let bg = if p.shading_pattern.is_some() { p.shading_fill } else { p.shading };
                                items.extend(shading_items(p.shading_pattern, bg, p.shading_color, inner_left, y0, inner_right - inner_left, y1 - y0));
                            }
                            let lx = inner_left - left.map_or(0.0, |b| pt(b.total()) / 2.0);
                            let rx = inner_right + right.map_or(0.0, |b| pt(b.total()) / 2.0);
                            if let Some(b) = bottom.filter(|_| i + 1 == count) {
                                items.extend(border_items(&b, true, false, y1 + pt(b.total()) / 2.0, lx, rx, left.is_some(), right.is_some()));
                            }
                            let (ty, by) = (0.0, if i + 1 == count { y1 + bottom.map_or(0.0, |b| pt(b.total())) } else { l.height });
                            if let Some(b) = left {
                                items.extend(border_items(&b, false, true, lx, ty, by, false, false));
                            }
                            if let Some(b) = right {
                                items.extend(border_items(&b, false, false, rx, ty, by, false, false));
                            }
                        }
                        items.extend(l.items);
                        let mode = self.markup_mode();
                        if (mode == MarkupDisplayMode::All || mode == MarkupDisplayMode::Simple) && l.has_revision && !in_table {
                            let bar_x = (inner_left - 10.0).max(-margin_l + 8.0);
                            items.push(Item::Rect {
                                x: bar_x,
                                y: 0.0,
                                w: 1.5,
                                h: l.height,
                                color: hsla(ibis_doc_model::Color::opaque(REVISION_COLOR)),
                            });
                        }
                        if (mode == MarkupDisplayMode::All || mode == MarkupDisplayMode::Simple) && !l.comment_marks.is_empty() && !in_table {
                            let bubble_size = 16.0;
                            let bubble_x = pt(self.doc.page.size.width) - margin_l + 8.0;
                            let bubble_y = (l.height - bubble_size).max(0.0) / 2.0;
                            items.push(Item::CommentBubble {
                                x: bubble_x,
                                y: bubble_y,
                                size: bubble_size,
                            });
                        }
                        let page_break = l.page_break || (matches!(p.section_break, SectionBreak::NextPage | SectionBreak::OddPage | SectionBreak::EvenPage) && i + 1 == count);
                        let column_break = l.column_break || (p.section_break == SectionBreak::NextColumn && i + 1 == count);
                        Piece { height: l.height + below + after, items, page_break, column_break, keep_with_next: p.keep_with_next && i + 1 == count, header: false, split: None, space_before: 0.0, space_after: after }
                    })
                    .collect();

                shape_pieces.extend(text_pieces);
                shape_pieces
            }
            Block::Table(t) => self.table(t, width, page_number),
        }
    }

    /// The narrowest and widest a cell's content can be laid out (points,
    /// without the cell's margins): its longest word (or character, in
    /// Japanese), and its longest line unbroken.
    fn content_widths(&self, blocks: &[Block], page_number: u32) -> (f32, f32) {
        let (mut least, mut most) = (0.0f32, 0.0f32);
        for block in blocks {
            match block {
                Block::Paragraph(p) => {
                    let (units, _) = self.units(p, page_number);
                    let indent = pt(p.left) + pt(p.right);
                    let first = pt(p.first_line).max(0.0);
                    let (mut line, mut word, mut prev) = (first, 0.0f32, None);
                    let mut trailing = 0.0f32;
                    for u in &units {
                        match u {
                            Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {
                                most = most.max(indent + line - trailing);
                                least = least.max(indent + word);
                                (line, word, prev, trailing) = (0.0, 0.0, None, 0.0);
                            }
                            Unit::Tab | Unit::ListTab => {
                                // A tab to the next default stop.
                                let stop = self.default_tab_pt();
                                line = ((line / stop).floor() + 1.0) * stop;
                                least = least.max(indent + word);
                                (word, prev, trailing) = (0.0, None, 0.0);
                            }
                            _ => {
                                let c = unit_char(u);
                                let w = unit_width(u);
                                if may_break(prev, c, p.kinsoku, p.word_wrap, self.doc) {
                                    least = least.max(indent + word);
                                    word = 0.0;
                                }
                                if c == Some(' ') {
                                    trailing += w;
                                } else {
                                    trailing = 0.0;
                                    word += w;
                                }
                                line += w;
                                prev = c;
                            }
                        }
                    }
                    most = most.max(indent + line - trailing);
                    least = least.max(indent + word);
                }
                Block::Table(t) => {
                    let w: f32 = match self.fitted_columns(t, f32::MAX / 4.0, page_number) {
                        Some(c) => c.iter().sum(),
                        None => t.columns.iter().map(|c| pt(*c)).sum(),
                    };
                    least = least.max(w);
                    most = most.max(w);
                }
            }
        }
        (least, most.max(least))
    }

    /// An automatically fitted table's columns (points) in `available`
    /// points, as Word's 自動調整 lays them out: each column the width its
    /// cells prefer, else its content's unbroken width (at least its
    /// longest word); then the table's preferred width shared out (in
    /// proportion), or the columns narrowed towards their longest words
    /// to fit. `None` to use the grid.
    fn fitted_columns(&self, t: &FlowTable, available: f32, page_number: u32) -> Option<Vec<f32>> {
        let fit = t.autofit.as_ref()?;
        if fit.cells.len() != t.rows.len() || fit.cells.iter().zip(&t.rows).any(|(w, r)| w.len() != r.cells.len()) {
            return None;
        }
        let n = t.columns.len();
        let target = match fit.width {
            PreferredWidth::Fixed(w) => Some(pt(w)),
            PreferredWidth::Percent(p) if available < f32::MAX / 8.0 => Some(available * p as f32 / 5000.0),
            _ => None,
        };
        // A cell's preferred width in points (a percent of the table's).
        let base = target.unwrap_or(available);
        let prefer = |w: PreferredWidth| match w {
            PreferredWidth::Fixed(w) => Some(pt(w)),
            PreferredWidth::Percent(p) if base < f32::MAX / 8.0 => Some(base * p as f32 / 5000.0),
            _ => None,
        };
        let (mut least, mut most) = (vec![0.0f32; n], vec![0.0f32; n]);
        let mut preferred: Vec<Option<f32>> = vec![None; n];
        let mut spanning = Vec::new();
        for (r_idx, (row, widths)) in t.rows.iter().zip(&fit.cells).enumerate() {
            let mut col = 0;
            for (cell, w) in row.cells.iter().zip(widths) {
                let span = (cell.span.max(1) as usize).min(n.saturating_sub(col)).max(1);
                if col >= n {
                    break;
                }
                let margins = pt(cell.margins[1]) + pt(cell.margins[3]);
                self.current_cell.set(Some((t.id, r_idx, col)));
                let (lo, hi) = if cell.merge == VerticalMerge::Continue { (0.0, 0.0) } else { self.content_widths(&cell.blocks, page_number) };
                self.current_cell.set(None);
                let lo = if cell.no_wrap { hi } else { lo };
                let (lo, hi) = (lo + margins, hi + margins);
                if span == 1 {
                    least[col] = least[col].max(lo);
                    most[col] = most[col].max(hi);
                    if let Some(p) = prefer(*w) {
                        preferred[col] = Some(preferred[col].map_or(p, |q: f32| q.max(p)));
                    }
                } else {
                    spanning.push((col, span, lo, prefer(*w).unwrap_or(hi).max(lo)));
                }
                col += span;
            }
        }
        let mut widths: Vec<f32> = (0..n).map(|c| preferred[c].unwrap_or(most[c]).max(least[c])).collect();
        // Columns only spanned: their share of the spanning cells' widths.
        for &(col, span, lo, want) in &spanning {
            let have: f32 = widths[col..col + span].iter().sum();
            let need = want.max(lo);
            if need > have {
                let empty: Vec<usize> = (col..col + span).filter(|&c| widths[c] == 0.0).collect();
                let share = if empty.is_empty() { (col..col + span).collect() } else { empty };
                for &c in &share {
                    widths[c] += (need - have) / share.len() as f32;
                }
            }
            for c in col..col + span {
                least[c] = least[c].max(lo / span as f32);
            }
        }
        let total: f32 = widths.iter().sum();
        let goal = match target {
            Some(goal) => goal,
            None if total > available => available,
            None => return Some(widths.into_iter().map(|w| (w / 0.24 - 0.01).ceil() * 0.24).collect()),
        };
        if total < goal {
            // Widened: the columns without a preferred width take the room
            // (in proportion to their widths), else all of them.
            let free: Vec<usize> = (0..n).filter(|&c| preferred[c].is_none()).collect();
            let grow = if free.is_empty() { (0..n).collect() } else { free };
            let sum: f32 = grow.iter().map(|&c| widths[c]).sum();
            for &c in &grow {
                widths[c] += (goal - total) * if sum > 0.0 { widths[c] / sum } else { 1.0 / grow.len() as f32 };
            }
        } else if total > goal {
            // Narrowed towards the longest words, in proportion to how much
            // narrower each can be.
            let floor: f32 = least.iter().sum();
            let k = if total > floor { ((goal - floor) / (total - floor)).max(0.0) } else { 0.0 };
            for c in 0..n {
                widths[c] = least[c] + (widths[c] - least[c]).max(0.0) * k;
            }
        }
        // Up to Word's 1/300 inch (the edges are rounded to it), so that
        // what fits is not broken by the rounding.
        Some(widths.into_iter().map(|w| (w / 0.24 - 0.01).ceil() * 0.24).collect())
    }

    /// A table's rows as pieces. Vertically merged cells are drawn in the
    /// row they start, as high as the rows they cover.
    fn table(&self, t: &FlowTable, width: f32, page_number: u32) -> Vec<Piece> {
        // The columns' edges from the table's left edge, rounded to Word's
        // 1/300 inch (measured on its PDF of DTBB: edges at 160, 280 and
        // 1731 twips are at 7.92, 13.92 and 86.64 pt).
        let mut xs = vec![0.0f32];
        let mut edge = 0;
        let columns: Vec<Emu> = match self.fitted_columns(t, width, page_number) {
            Some(fitted) => fitted.iter().map(|w| (*w as f64 * EMU_PER_POINT as f64) as Emu).collect(),
            None => t.columns.clone(),
        };
        for c in &columns {
            edge += *c;
            // 3 048 EMU: 1/300 inch.
            xs.push((edge as f64 / 3048.0).round() as f32 * 0.24);
        }
        let total = *xs.last().unwrap_or(&0.0);
        // The indent is to the table's left edge.
        let x0 = match t.align {
            FlowAlign::Center => (width - total) / 2.0,
            FlowAlign::Right => width - total,
            _ => pt(t.indent),
        };
        // Lay out each cell: its column, span, content and content height.
        struct Laid {
            col: usize,
            span: usize,
            items: Vec<Item>,
            content: f32,
            /// Vertical text: its lines' total thickness (across the cell).
            across: f32,
        }
        impl HasCol for Laid {
            fn col(&self) -> usize {
                self.col
            }
        }
        impl HasSpan for Laid {
            fn col_end(&self) -> usize {
                self.col + self.span
            }
        }
        let mut grid: Vec<Vec<Laid>> = Vec::new();
        for (r_idx, row) in t.rows.iter().enumerate() {
            let mut col = 0;
            let mut cells = Vec::new();
            for cell in &row.cells {
                let span = cell.span.max(1) as usize;
                let (l, r) = (xs[col.min(xs.len() - 1)], xs[(col + span).min(xs.len() - 1)]);
                let inner = (r - l - pt(cell.margins[1]) - pt(cell.margins[3])).max(1.0);
                // Vertical text's lines run the row's height (as long as
                // they need in a row of no set height).
                let vertical = cell.direction.vertical();
                let inner = if vertical {
                    match row.height {
                        RowHeight::Exact(h) | RowHeight::AtLeast(h) if h > 0 => (pt(h) - pt(cell.margins[0]) - pt(cell.margins[2])).max(1.0),
                        _ => 10_000.0,
                    }
                } else if cell.no_wrap {
                    10_000.0
                } else {
                    inner
                };
                let mut y = 0.0;
                let mut items = Vec::new();
                if cell.merge != VerticalMerge::Continue {
                    self.current_cell.set(Some((t.id, r_idx, col)));
                    for (b, block) in cell.blocks.iter().enumerate() {
                        // The empty paragraph that ends a cell after a
                        // nested table takes no room (as Word prints it).
                        let after_table = b > 0 && b + 1 == cell.blocks.len() && matches!(cell.blocks[b - 1], Block::Table(_));
                        if after_table && matches!(block, Block::Paragraph(p) if p.runs.is_empty()) {
                            continue;
                        }
                        for piece in self.pieces(block, inner, page_number, None, true) {
                            items.extend(piece.items.into_iter().map(|it| offset(it, 0.0, y)));
                            y += piece.height;
                        }
                    }
                    self.current_cell.set(None);
                }
                // Vertical text: its length is its height in the row.
                let (content, across) = if vertical {
                    let length = items.iter().filter_map(|it| match it {
                        Item::Text { x, offsets, size, .. } => Some(x + offsets.last().copied().unwrap_or(0.0) + size),
                        _ => None,
                    }).fold(0.0f32, f32::max);
                    (length, y)
                } else {
                    (y, 0.0)
                };
                cells.push(Laid { col, span, items, content, across });
                col += span;
            }
            grid.push(cells);
        }
        let spacing = t.cell_spacing.map_or(0.0, pt);
        let half_sp = spacing / 2.0;
        // Row heights: the content of cells that are not merged down, the
        // rule's height; then merged cells stretch the last row they cover.
        let mut heights: Vec<f32> = t
            .rows
            .iter()
            .zip(&grid)
            .map(|(row, cells)| {
                // Word counts the width of a cell's top border into its row.
                let content = row
                    .cells
                    .iter()
                    .zip(cells)
                    .filter(|(c, _)| c.merge == VerticalMerge::None)
                    .map(|(c, l)| l.content + pt(c.margins[0]) + pt(c.margins[2]) + c.borders[0].map_or(0.0, |b| pt(b.width)) + spacing)
                    .fold(0.0, f32::max);
                // A minimum height does not count the top border either.
                let border = row.cells.iter().filter_map(|c| c.borders[0]).map(|b| pt(b.width)).fold(0.0, f32::max);
                match row.height {
                    RowHeight::Exact(h) => pt(h),
                    RowHeight::AtLeast(h) => (pt(h) + border + spacing).max(content),
                    RowHeight::Auto => content,
                }
            })
            .collect();
        // The last row's bottom border takes room below the table too.
        if let (Some(last), Some(h)) = (t.rows.last(), heights.last_mut()) {
            *h += last.cells.iter().filter_map(|c| c.borders[2]).map(|b| pt(b.width)).fold(0.0, f32::max);
        }
        // How many rows each merged cell covers.
        let spans_down = |r: usize, col: usize| -> usize {
            let mut n = 1;
            while r + n < t.rows.len() {
                let below = grid[r + n].iter().zip(&t.rows[r + n].cells).find(|(l, _)| l.col == col);
                match below {
                    Some((_, c)) if c.merge == VerticalMerge::Continue => n += 1,
                    _ => break,
                }
            }
            n
        };
        for r in 0..t.rows.len() {
            for (laid, cell) in grid[r].iter().zip(&t.rows[r].cells) {
                if cell.merge != VerticalMerge::Start {
                    continue;
                }
                let n = spans_down(r, laid.col);
                let need = laid.content + pt(cell.margins[0]) + pt(cell.margins[2]);
                let have: f32 = heights[r..r + n].iter().sum();
                if need > have {
                    heights[r + n - 1] += need - have;
                }
            }
        }

        let mut pieces = Vec::new();
        for (r, row) in t.rows.iter().enumerate() {
            let mut items = Vec::new();
            let mut borders = Vec::new();
            // A row Word may divide at a page's end: not kept whole, not a
            // heading, not of an exact height, without merged cells or
            // diagonals.
            let divisible = !row.cant_split
                && !row.header
                && !matches!(row.height, RowHeight::Exact(_))
                && row.cells.iter().all(|c| c.merge == VerticalMerge::None && c.diagonals.iter().all(Option::is_none) && c.anchor == CellAnchor::Top);
            let mut split_cells = Vec::new();
            for (laid, cell) in grid[r].iter().zip(&row.cells) {
                let n = if cell.merge == VerticalMerge::Start { spans_down(r, laid.col) } else { 1 };
                let h: f32 = heights[r..(r + n).min(heights.len())].iter().sum();
                let raw_left = x0 + xs[laid.col.min(xs.len() - 1)];
                let raw_right = x0 + xs[(laid.col + laid.span).min(xs.len() - 1)];
                let (left, right) = if spacing > 0.0 {
                    (raw_left + half_sp, raw_right - half_sp)
                } else {
                    (raw_left, raw_right)
                };
                let cell_top = if spacing > 0.0 { half_sp } else { 0.0 };
                let cell_h = if spacing > 0.0 { (h - spacing).max(0.0) } else { h };
                if cell.merge != VerticalMerge::Continue {
                    // Its own fill, else its table style's.
                    let fill = cell.shading.or(cell.style_fill);
                    if fill.is_some() || cell.shading_pattern.is_some() {
                        let bg = if cell.shading_pattern.is_some() { cell.shading_fill } else { fill };
                        items.extend(shading_items(cell.shading_pattern, bg, cell.shading_color, left, cell_top, right - left, cell_h));
                    }
                    // The top border's width is taken off the room first.
                    // An art border takes as much room as its `w:sz` in points (a 1.5 pt line, 12 pt of room:
                    // Word's text is 10 pt lower, DTBB).
                    let border = cell.borders[0].map_or(0.0, |b| if b.art.is_some() { pt(b.width) * 8.0 } else { pt(b.width) });
                    let room = cell_h - pt(cell.margins[0]) - pt(cell.margins[2]) - border;
                    let dy = cell_top
                        + border
                        + pt(cell.margins[0])
                        + match cell.anchor {
                            CellAnchor::Top => 0.0,
                            CellAnchor::Center => ((room - laid.content) / 2.0).max(0.0),
                            CellAnchor::Bottom => (room - laid.content).max(0.0),
                        };
                    // The text starts inside the cell's left border.
                    let dx = left + pt(cell.margins[3]) + cell.borders[3].map_or(0.0, |b| pt(b.width));
                    // A row of an exact height shows only the lines that
                    // begin in it (Word cuts the rest off; DTST's wrapped
                    // cells).
                    let exact = matches!(row.height, RowHeight::Exact(_)) && n == 1;
                    if cell.direction.vertical() {
                        // The cell's box inside its margins.
                        let bx = left + pt(cell.margins[3]) + cell.borders[3].map_or(0.0, |b| pt(b.width));
                        let by = cell_top + border + pt(cell.margins[0]);
                        let bw = (right - left - pt(cell.margins[1]) - pt(cell.margins[3])).max(0.0);
                        let bh = (cell_h - pt(cell.margins[0]) - pt(cell.margins[2]) - border).max(0.0);
                        items.extend(turn_cell_text(&laid.items, cell.direction, cell.anchor, (bx, by, bw, bh), laid.across));
                    } else if cell.direction == ibis_doc_model::CellDirection::LrTbV {
                        items.extend(turn_japanese(laid.items.iter().cloned().map(|it| offset(it, dx, dy))));
                    } else {
                    items.extend(laid.items.iter().cloned().map(|it| offset(it, dx, dy)).filter(|it| {
                        !exact
                            || match it {
                                Item::Text { y, size, .. } => *y - size * 0.8 < h,
                                Item::Line { y, .. } => *y < h,
                                _ => true,
                            }
                    }));
                    if divisible {
                        split_cells.push(SplitCell {
                            left,
                            right,
                            fill: cell.shading.or(cell.style_fill).map(|f| hsla(ibis_doc_model::Color::opaque(f))),
                            content: laid.items.iter().cloned().map(|it| offset(it, dx, dy)).collect(),
                            top: dy,
                            bottom: pt(cell.margins[2]),
                            borders: [None; 4],
                        });
                    }
                    }
                }
                // Borders: a continued cell draws only its sides and bottom.
                let row_h = if spacing > 0.0 { heights[r] - half_sp } else { heights[r] };
                let row_top = if spacing > 0.0 { half_sp } else { 0.0 };
                let [top, rgt, bottom, lft] = cell.borders;
                // Where two cells' borders meet, the heavier one is drawn
                // (Word's rule: the width in eighths of a point times the
                // style's number; the one above or left on a tie).
                let (top, bottom, lft, rgt) = if spacing > 0.0 {
                    (top, bottom, lft, rgt)
                } else {
                    let neighbour = |row: usize, col: usize| grid.get(row).and_then(|cells| cells.iter().position(|l| l.col == col)).map(|i| t.rows[row].cells[i].borders);
                    let beats = |a: Option<FlowBorder>, b: Option<FlowBorder>| weight(a) > weight(b);
                    let top = if r > 0 && neighbour(r - 1, laid.col).is_some_and(|n| !beats(top, n[2])) { None } else { top };
                    let bottom = if neighbour(r + 1, laid.col).is_some_and(|n| beats(n[0], bottom)) { None } else { bottom };
                    let lft = if laid.col > 0 && cells_left_of(&grid[r], laid.col).is_some_and(|i| !beats(lft, t.rows[r].cells[i].borders[1])) { None } else { lft };
                    let rgt = if grid[r].iter().position(|l| l.col == laid.col + laid.span).is_some_and(|i| beats(t.rows[r].cells[i].borders[3], rgt)) { None } else { rgt };
                    (top, bottom, lft, rgt)
                };
                let is_table_top = r == 0;
                let is_table_bottom = r + 1 == t.rows.len();
                let is_table_left = laid.col == 0;
                let is_table_right = laid.col + laid.span == t.columns.len();
                if cell.merge != VerticalMerge::Continue {
                    // The table's top border starts at its top edge and goes down into the row (Word counts it in the
                    // row and does not draw half of it above the table: DTBB).
                    let top_at = if is_table_top { row_top + top.map_or(0.0, |b| pt(b.total()) / 2.0) } else { row_top };
                    borders.push((top, true, true, top_at, left, right, is_table_top && is_table_left, is_table_top && is_table_right));
                    // Diagonal lines across the cell (as high as it spans).
                    let n = if cell.merge == VerticalMerge::Start { spans_down(r, laid.col) } else { 1 };
                    let h: f32 = heights[r..(r + n).min(heights.len())].iter().sum();
                    let diag_bottom = if spacing > 0.0 { h - half_sp } else { h };
                    for (i, d) in cell.diagonals.iter().enumerate() {
                        if let Some(d) = d {
                            let color = hsla(ibis_doc_model::Color::opaque(d.color));
                            let (x, x2) = if i == 0 { (left, right) } else { (right, left) };
                            items.push(Item::Stroke { x, y: row_top, x2, y2: diag_bottom, w: pt(d.width).max(0.25), color });
                        }
                    }
                }
                if divisible && let Some(c) = split_cells.last_mut() {
                    c.borders = [top, rgt, bottom, lft];
                }
                if cell.merge == VerticalMerge::None || cell.merge == VerticalMerge::Continue && !continues_below(t, &grid, r, laid.col) {
                    // At the table's bottom edge the border ends where the row does (it is inside the room the row
                    // reserves for it), as the top one starts at the top edge.
                    let bottom_at = if is_table_bottom { row_h - bottom.map_or(0.0, |b| pt(b.total()) / 2.0) } else { row_h };
                    borders.push((bottom, true, false, bottom_at, left, right, is_table_bottom && is_table_left, is_table_bottom && is_table_right));
                }
                // The sides meet the top border where it starts (at the table's top edge, see above).
                let side_top = if is_table_top { row_top + top.map_or(0.0, |b| pt(b.total()) / 2.0) } else { row_top };
                let side_bottom = if is_table_bottom { row_h - bottom.map_or(0.0, |b| pt(b.total()) / 2.0) } else { row_h };
                borders.push((lft, false, true, left, side_top, side_bottom, is_table_left && is_table_top, is_table_left && is_table_bottom));
                borders.push((rgt, false, false, right, side_top, side_bottom, is_table_right && is_table_top, is_table_right && is_table_bottom));
            }
            for (b, horizontal, outer_first, at, from, to, extend_start, extend_end) in borders {
                if let Some(b) = b {
                    items.extend(border_items(&b, horizontal, outer_first, at, from, to, extend_start, extend_end));
                }
            }
            let split = (divisible && split_cells.len() == row.cells.len()).then(|| Arc::new(RowSplit { cells: split_cells }));
            pieces.push(Piece { height: heights[r], items, page_break: false, column_break: false, keep_with_next: false, header: row.header && t.rows[..r].iter().all(|r| r.header), split, ..Default::default() });
        }
        pieces
    }

    /// A paragraph's space before and after, less what contextual spacing
    /// takes away.
    fn space_before(&self, p: &FlowParagraph) -> f32 {
        if self.contextual.get(&p.id).is_some_and(|c| c.0) { 0.0 } else { pt(p.space_before) }
    }

    fn space_after(&self, p: &FlowParagraph) -> f32 {
        if self.contextual.get(&p.id).is_some_and(|c| c.1) { 0.0 } else { pt(p.space_after) }
    }

    fn lay_out(&self) -> Pagination {
        use ibis_doc_model::flow_command;
        // The sections: their setup and what their headers and footers show.
        let ids = flow_command::sections(self.doc);
        let mut setups: Vec<ibis_doc_model::SectionSetup> = ids.iter().map(|id| flow_command::section_setup(self.doc, *id).unwrap_or_default()).collect();
        // 縦書き: a section of vertical writing is laid out as across text on
        // a page turned a quarter (see `turn_page_items`).
        let directions: Vec<ibis_doc_model::CellDirection> = setups.iter().map(|s| s.page.direction).collect();
        for setup in &mut setups {
            use ibis_doc_model::CellDirection;
            let page = &mut setup.page;
            let (t, r, b, l) = (page.top, page.right, page.bottom, page.left);
            match page.direction {
                CellDirection::TbRl => (page.top, page.right, page.bottom, page.left) = (r, b, l, t),
                CellDirection::BtLr => (page.top, page.right, page.bottom, page.left) = (l, t, r, b),
                _ => continue,
            }
            page.size = ibis_doc_model::EmuSize { width: page.size.height, height: page.size.width };
            page.borders = [None; 4];
            page.gutter = 0;
        }
        let parts: Vec<[Vec<Block>; 6]> = ids.iter().map(|id| std::array::from_fn(|k| flow_command::resolved_part(self.doc, *id, ibis_doc_model::PartKind::ALL[k]))).collect();
        // The text area of a section's pages, and its columns (段組み).
        // とじしろ (gutter): on the left (the inside with 見開きページ), or
        // at the top.
        let gutters = |s: usize| -> (i64, i64) {
            let page = &setups[s].page;
            if page.gutter_at_top { (0, page.gutter) } else { (page.gutter, 0) }
        };
        let margin_frame = |s: usize| {
            let page = &setups[s].page;
            let (w, h) = (pt(page.size.width), pt(page.size.height));
            let (gl, gt) = gutters(s);
            Frame { left: pt(page.left + gl), top: pt(page.top + gt), width: w - pt(page.left + gl) - pt(page.right), bottom: h - pt(page.bottom) }
        };
        // The header and footer a page has: its section's first-page ones
        // (表紙), even-page ones, or the default ones.
        const DEFAULT_PAGE: usize = 0;
        let page_kind = |s: usize, number: u32, first_in_section: bool| -> usize {
            if setups[s].title_page && first_in_section {
                1
            } else if self.doc.even_and_odd && number % 2 == 0 {
                2
            } else {
                DEFAULT_PAGE
            }
        };
        let header_footer = |s: usize, kind: usize| -> (&Vec<Block>, &Vec<Block>) {
            match kind {
                1 => (&parts[s][2], &parts[s][3]),
                2 => (&parts[s][4], &parts[s][5]),
                _ => (&parts[s][0], &parts[s][1]),
            }
        };
        // As Word: a header taller than the top margin pushes the body down,
        // a footer taller than the bottom one pushes its end up.
        let body_limits: Vec<[(f32, f32); 3]> = (0..setups.len())
            .map(|s| {
                let margins = margin_frame(s);
                let page = &setups[s].page;
                self.line_pitch.set(page.line_pitch);
                let height = |blocks: &Vec<Block>| -> f32 { blocks.iter().flat_map(|b| self.pieces(b, margins.width, 1, None, true)).map(|p| p.height).sum() };
                std::array::from_fn(|kind| {
                    let (header, footer) = header_footer(s, kind);
                    let top = if header.is_empty() { margins.top } else { margins.top.max(pt(page.header) + height(header)) };
                    let bottom = if footer.is_empty() { margins.bottom } else { margins.bottom.min(pt(page.size.height) - pt(page.footer) - height(footer)) };
                    (top, bottom)
                })
            })
            .collect();
        // The page being laid out: its section (the one it starts in, whose
        // header and footer it has) and their kind.
        let kind_now = std::cell::Cell::new((0usize, DEFAULT_PAGE));
        let text_frame = |s: usize| {
            let (page_s, kind) = kind_now.get();
            let (top, bottom) = body_limits[page_s][kind];
            Frame { top, bottom, ..margin_frame(s) }
        };
        let columns_of = |s: usize| -> Vec<(f32, f32)> {
            let page = &setups[s].page;
            page.columns.layout(page.size.width - page.left - gutters(s).0 - page.right).into_iter().map(|(x, w)| (pt(x), pt(w))).collect()
        };
        // 見開きページ・袋とじ: an even page's margins swapped (the inside on the
        // right): how far its text moves.
        let mirror_dx = |s: usize, number: u32| -> f32 {
            let page = &setups[s].page;
            if page.is_mirrored() && number % 2 == 0 { pt(page.right) - pt(page.left + gutters(s).0) } else { 0.0 }
        };
        let column_frame = |s: usize, c: usize| {
            let text = text_frame(s);
            let cols = columns_of(s);
            let (x, w) = cols.get(c).copied().unwrap_or((0.0, text.width));
            Frame { left: text.left + x, width: w, ..text }
        };
        let frame_of = |s: usize| column_frame(s, 0);
        let page_of = |s: usize, number: u32, first: bool| Page {
            items: Vec::new(),
            body_items_count: 0,
            width: pt(setups[s].page.size.width),
            height: pt(setups[s].page.size.height),
            section: s,
            number,
            first_in_section: first,
            notes: Vec::new(),
        };
        // The section each body block is in.
        let mut section = 0usize;
        let mut sections_of = Vec::with_capacity(self.doc.body.len());
        for block in &self.doc.body {
            sections_of.push(section);
            if matches!(block, Block::Paragraph(p) if p.section.is_some()) {
                section = (section + 1).min(setups.len() - 1);
            }
        }
        // The footnotes' marks, by paragraph (their positions), and the
        // notes laid out (each as wide as the text of the first section).
        let mut marks: std::collections::HashMap<u32, Vec<(usize, u32)>> = std::collections::HashMap::new();
        fn collect(blocks: &[Block], marks: &mut std::collections::HashMap<u32, Vec<(usize, u32)>>) {
            for b in blocks {
                match b {
                    Block::Paragraph(p) => {
                        let mut at = 0;
                        for r in &p.runs {
                            if let FlowRunKind::NoteRef { endnote: false, id } = r.kind {
                                marks.entry(p.id).or_default().push((at, id));
                            }
                            at += r.len();
                        }
                    }
                    Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| collect(&c.blocks, marks)),
                }
            }
        }
        collect(&self.doc.body, &mut marks);
        let note_width = frame_of(0).width;
        let note_pieces = |endnote: bool, id: u32| -> Vec<Piece> {
            let notes = if endnote { &self.doc.endnotes } else { &self.doc.footnotes };
            let Some(note) = notes.iter().find(|n| n.id == id) else { return Vec::new() };
            *self.current_note.borrow_mut() = self.note_numbers.get(&(endnote, id)).cloned().unwrap_or_default();
            note.blocks.iter().flat_map(|b| self.pieces(b, note_width, 1, None, true)).collect()
        };
        let mut note_heights: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();
        for ids in marks.values() {
            for (_, id) in ids {
                if !note_heights.contains_key(id) {
                    let h = note_pieces(false, *id).iter().map(|p| p.height).sum();
                    note_heights.insert(*id, h);
                }
            }
        }
        // The separator above the notes: a line of the normal style.
        let separator_h = {
            let mut p = FlowParagraph { direct: ibis_doc_model::ParaFormat { before: Some(0), after: Some(0), line: Some(ibis_doc_model::LineHeight::Auto(240)), ..Default::default() }, ..FlowParagraph::default() };
            p.resolve(&self.doc.styles);
            self.pieces(&Block::Paragraph(p), note_width, 1, None, true).iter().map(|p| p.height).sum::<f32>()
        };
        // The notes a piece has marks of.
        let notes_of = |piece: &Piece| -> Vec<u32> {
            let mut out = Vec::new();
            for it in &piece.items {
                if let Item::Line { para, stops, .. } = it
                    && let Some(list) = marks.get(para)
                {
                    let (from, to) = (stops.first().map_or(0, |s| s.0), stops.last().map_or(0, |s| s.0));
                    for (at, id) in list {
                        if *at >= from && (*at < to || to == from) && !out.contains(id) {
                            out.push(*id);
                        }
                    }
                }
            }
            out
        };
        let notes_room = |page: &Page| -> f32 { if page.notes.is_empty() { 0.0 } else { separator_h + page.notes.iter().map(|id| note_heights.get(id).copied().unwrap_or(0.0)).sum::<f32>() } };
        let mut pages: Vec<Page> = vec![page_of(0, setups[0].page.first_page, true)];
        kind_now.set((0, page_kind(0, setups[0].page.first_page, true)));
        let mut frame = frame_of(0);
        let mut y = frame.top;
        // Where the last piece placed ended (pages so far, column, y) and the
        // space after it that is in it.
        let mut prev_end: Option<(usize, usize, f32, f32)> = None;
        let mut number = setups[0].page.first_page;
        // A new page, in section `s`: numbered on, or again from the
        // section's start when it is the section's first page and restarts.
        let next_page = |pages: &mut Vec<Page>, frame: &mut Frame, y: &mut f32, number: &mut u32, s: usize| {
            let first = pages.last().is_some_and(|p| p.section != s);
            *number = if first && setups[s].restart_numbering { setups[s].page.first_page } else { *number + 1 };
            pages.push(page_of(s, *number, first));
            kind_now.set((s, page_kind(s, *number, first)));
            *frame = frame_of(s);
            *y = frame.top;
        };
        let find_page_obstacle = |s: usize, number: u32, frame: &Frame| -> Option<WrapObstacle> {
            for (b_idx, b) in self.doc.body.iter().enumerate() {
                if sections_of[b_idx] != s {
                    continue;
                }
                if let Block::Paragraph(p) = b {
                    if let Some(shape) = square_shape(p, self.hides_deleted()).filter(|s| !self.deferred_shapes.borrow().contains(&s.id.0)) {
                        let f = shape.floating.as_ref().unwrap();
                        if f.relative_v != "paragraph" && f.relative_v != "line" {
                            let sw = pt(shape.frame.width);
                            let sh = pt(shape.frame.height);
                            let dl = pt(f.dist_l);
                            let dr = pt(f.dist_r);
                            let dt = pt(f.dist_t);
                            let db = pt(f.dist_b);
                            let wrap_text = f.wrap_text.clone().unwrap_or_else(|| "bothSides".to_string());

                            let page_w = pt(setups[s].page.size.width);
                            let page_h = pt(setups[s].page.size.height);
                            let margin_l = pt(setups[s].page.left);
                            let margin_r = pt(setups[s].page.right);
                            let margin_t = pt(setups[s].page.top);
                            let margin_b = pt(setups[s].page.bottom);
                            let content_w = frame.width;
                            let content_h = frame.bottom - frame.top;
                            let is_even = number % 2 == 0;

                            let (base_x0, base_w) = match f.relative_h.as_str() {
                                "page" => (0.0, page_w),
                                "leftMargin" => (0.0, margin_l),
                                "rightMargin" => (page_w - margin_r, margin_r),
                                "insideMargin" => if is_even { (page_w - margin_r, margin_r) } else { (0.0, margin_l) },
                                "outsideMargin" => if is_even { (0.0, margin_l) } else { (page_w - margin_r, margin_r) },
                                _ /* "column" | "margin" */ => (margin_l, content_w),
                            };

                            let abs_x = if f.relative_h == "character" {
                                match f.align_h.as_deref() {
                                    Some("center") => margin_l - sw / 2.0,
                                    Some("right") => margin_l - sw,
                                    Some("left") => margin_l,
                                    _ => margin_l + pt(f.offset_x),
                                }
                            } else {
                                match f.align_h.as_deref() {
                                    Some("left") => base_x0,
                                    Some("center") => base_x0 + (base_w - sw) / 2.0,
                                    Some("right") => base_x0 + base_w - sw,
                                    Some("inside") => if is_even { base_x0 + base_w - sw } else { base_x0 },
                                    Some("outside") => if is_even { base_x0 } else { base_x0 + base_w - sw },
                                    _ => base_x0 + pt(f.offset_x),
                                }
                            };
                            let abs_x = abs_x.clamp(0.0, (page_w - sw).max(0.0));

                            let (base_y0, base_h) = match f.relative_v.as_str() {
                                "page" => (0.0, page_h),
                                "topMargin" => (0.0, margin_t),
                                "bottomMargin" => (page_h - margin_b, margin_b),
                                "insideMargin" => if is_even { (page_h - margin_b, margin_b) } else { (0.0, margin_t) },
                                "outsideMargin" => if is_even { (0.0, margin_t) } else { (page_h - margin_b, margin_b) },
                                "margin" => (margin_t, (page_h - margin_t - margin_b).max(0.0)),
                                _ => (margin_t, content_h),
                            };

                            let abs_y = match f.align_v.as_deref() {
                                Some("top") => base_y0,
                                Some("center") => if base_h > 0.0 { base_y0 + (base_h - sh) / 2.0 } else { base_y0 },
                                Some("bottom") => if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 },
                                Some("inside") => if is_even { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } } else { base_y0 },
                                Some("outside") => if is_even { base_y0 } else { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } },
                                _ => base_y0 + pt(f.offset_y),
                            };
                            let sx = abs_x - margin_l;
                            let wrap_polygon = matches!(f.wrap.as_str(), "tight" | "through")
                                .then(|| f.wrap_polygon.clone())
                                .flatten()
                                .map(|poly| (poly, sx, abs_y, sw, sh, dl, dr));
                            return Some(WrapObstacle {
                                shape_left: sx - dl,
                                shape_right: sx + sw + dr,
                                y_top: abs_y - dt,
                                y_bottom: abs_y + sh + db,
                                wrap_text,
                                wrap_polygon,
                            });
                        }
                    }
                }
            }
            None
        };
        let place_item = |it: Item, y_cursor: f32, s_idx: usize, pg_num: u32, frame_left: f32| -> Item {
            if let Item::Shape { shape, x, y: _, w, h } = &it
                && let Some(f) = &shape.floating
                && matches!(f.relative_v.as_str(), "page" | "margin" | "topMargin" | "bottomMargin" | "insideMargin" | "outsideMargin")
            {
                let page_h = pt(setups[s_idx].page.size.height);
                let margin_t = pt(setups[s_idx].page.top);
                let margin_b = pt(setups[s_idx].page.bottom);
                let is_even = pg_num % 2 == 0;
                let sh = *h;
                let (base_y0, base_h) = match f.relative_v.as_str() {
                    "page" => (0.0, page_h),
                    "margin" => (margin_t, (page_h - margin_t - margin_b).max(0.0)),
                    "topMargin" => (0.0, margin_t),
                    "bottomMargin" => (page_h - margin_b, margin_b),
                    "insideMargin" => if is_even { (page_h - margin_b, margin_b) } else { (0.0, margin_t) },
                    "outsideMargin" => if is_even { (0.0, margin_t) } else { (page_h - margin_b, margin_b) },
                    _ => (margin_t, page_h),
                };
                let abs_y = match f.align_v.as_deref() {
                    Some("top") => base_y0,
                    Some("center") => if base_h > 0.0 { base_y0 + (base_h - sh) / 2.0 } else { base_y0 },
                    Some("bottom") => if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 },
                    Some("inside") => if is_even { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } } else { base_y0 },
                    Some("outside") => if is_even { base_y0 } else { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } },
                    _ => base_y0 + pt(f.offset_y),
                };
                Item::Shape {
                    shape: shape.clone(),
                    x: *x + frame_left,
                    y: abs_y,
                    w: *w,
                    h: *h,
                }
            } else {
                offset(it, frame_left, y_cursor)
            }
        };
        let mut active_obstacle = find_page_obstacle(0, number, &frame);
        // The obstacles placed on the page so far: one wider than its column
        // (a figure across both) keeps the text of the next column off it too.
        let mut page_obstacles: Vec<WrapObstacle> = Vec::new();
        // Floating shapes put off to the next page's top: the shape, x from
        // the left margin, y from the first of them, width and height.
        let mut deferred: Vec<(Box<ibis_doc_model::Shape>, f32, f32, f32, f32)> = Vec::new();
        // The shapes a first laying out placed above their paragraphs, on the
        // page just begun: there from its start, the text round them.
        macro_rules! seed_page {
            () => {{
                let index = pages.len() - 1;
                for (_, obs, item, _) in self.seeds.iter().filter(|s| s.0 == index) {
                    page_obstacles.push(obs.clone());
                    if active_obstacle.is_none() {
                        active_obstacle = Some(obs.clone());
                    }
                    pages.last_mut().expect("a page").items.push(item.clone());
                }
            }};
        }
        seed_page!();
        // The columns being filled on the page (段組み): the section's, the
        // column, where they start, how far each is filled, and the pieces
        // put in them (their items on the page, top, height and column; to
        // balance them where a section ends on the page).
        let mut region_s = 0usize;
        let mut col = 0usize;
        let mut col_top = frame.top;
        let mut col_fill: Vec<f32> = Vec::new();
        let mut placed: Vec<(std::ops::Range<usize>, f32, f32, usize)> = Vec::new();
        // The lines between the columns filled (w:sep).
        macro_rules! close_region {
            () => {{
                if setups[region_s].page.columns.separator {
                    let cols = columns_of(region_s);
                    let left = text_frame(region_s).left;
                    let bottom = col_fill.iter().copied().fold(col_top, f32::max);
                    let page = pages.last_mut().expect("a page");
                    for c in 1..cols.len() {
                        if col_fill.get(c).is_some_and(|&f| f > col_top + 0.01) {
                            let x = left + (cols[c - 1].0 + cols[c - 1].1 + cols[c].0) / 2.0;
                            page.items.push(Item::Rect { x: x - 0.25, y: col_top, w: 0.5, h: bottom - col_top, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) });
                        }
                    }
                }
            }};
        }
        macro_rules! new_page {
            ($s:expr) => {{
                close_region!();
                next_page(&mut pages, &mut frame, &mut y, &mut number, $s);
                region_s = $s;
                // A new page starts again in its first column.
                #[allow(unused_assignments)]
                {
                    col = 0;
                }
                col_top = y;
                col_fill.clear();
                placed.clear();
                page_obstacles.clear();
                active_obstacle = None;
                // The shapes put off to this page: at its top, the text round them.
                let margin_l = pt(setups[$s].page.left);
                for (shape, x, dy, w, h) in deferred.drain(..) {
                    let (sx, sy) = (margin_l + x, frame.top + dy);
                    if let Some(f) = shape.floating.as_ref()
                        && matches!(f.wrap.as_str(), "square" | "tight" | "through")
                    {
                        let obs = WrapObstacle {
                            shape_left: sx - margin_l - pt(f.dist_l),
                            shape_right: sx + w - margin_l + pt(f.dist_r),
                            y_top: sy - pt(f.dist_t),
                            y_bottom: sy + h + pt(f.dist_b),
                            wrap_text: f.wrap_text.clone().unwrap_or_else(|| "bothSides".to_string()),
                            wrap_polygon: None,
                        };
                        if active_obstacle.is_none() {
                            active_obstacle = Some(obs.clone());
                        }
                        page_obstacles.push(obs);
                    }
                    pages.last_mut().expect("a page").items.push(Item::Shape { shape, x: sx, y: sy, w, h });
                }
                seed_page!();
            }};
        }
        // On in the next column, or the next page after the last.
        macro_rules! advance {
            ($s:expr) => {{
                if region_s == $s && col + 1 < columns_of($s).len() {
                    col += 1;
                    frame = column_frame($s, col);
                    y = col_top;
                    // An obstacle of the page reaching into this column.
                    let (left, right) = (frame.left - pt(setups[$s].page.left), frame.left - pt(setups[$s].page.left) + frame.width);
                    active_obstacle = page_obstacles
                        .iter()
                        .filter(|o| o.shape_right > left && o.shape_left < right && o.y_bottom > y)
                        .min_by(|a, b| a.y_top.total_cmp(&b.y_top))
                        .cloned();
                } else {
                    new_page!($s);
                }
            }};
        }
        // The block before had its mark deleted (joined with this one).
        let mut joins_next = false;
        // Pieces are laid out with the number of the page they start on.
        for (b, block) in self.doc.body.iter().enumerate() {
            let s = sections_of[b];
            self.line_pitch.set(setups[s].page.line_pitch);
            // A section that starts a page (after a page-breaking one) takes
            // over the page it starts on.
            if pages.last().is_some_and(|pg| pg.section != s && pg.items.iter().all(|it| matches!(it, Item::Line { .. }))) && y <= frame.top + 0.01 {
                let first = true;
                number = if setups[s].restart_numbering { setups[s].page.first_page } else { number };
                *pages.last_mut().expect("a page") = page_of(s, number, first);
                kind_now.set((s, page_kind(s, number, first)));
                frame = frame_of(s);
                y = frame.top;
                active_obstacle = find_page_obstacle(s, number, &frame).or(active_obstacle.take());
                (region_s, col, col_top) = (s, 0, y);
                col_fill.clear();
                placed.clear();
                page_obstacles.clear();
                seed_page!();
            }
            // A section that starts on the page (continuous): the columns
            // before are balanced (as Word does where a section ends), and
            // its own start below them.
            if s != region_s {
                let cols = columns_of(region_s);
                if cols.len() > 1 && !placed.is_empty() {
                    let heights: Vec<f32> = placed.iter().map(|p| p.2).collect();
                    let total: f32 = heights.iter().sum();
                    let fits = |limit: f32| {
                        let (mut c, mut h) = (0usize, 0.0f32);
                        for &x in &heights {
                            if h + x > limit + 0.01 && h > 0.0 {
                                c += 1;
                                h = 0.0;
                            }
                            h += x;
                        }
                        c < cols.len()
                    };
                    let (mut lo, mut hi) = (heights.iter().copied().fold(0.0, f32::max), total);
                    for _ in 0..40 {
                        let mid = (lo + hi) / 2.0;
                        if fits(mid) { hi = mid } else { lo = mid }
                    }
                    let page = pages.last_mut().expect("a page");
                    let (mut c, mut h) = (0usize, 0.0f32);
                    col_fill = vec![col_top; cols.len()];
                    for (range, top, height, old) in placed.drain(..) {
                        if h + height > hi + 0.01 && h > 0.0 && c + 1 < cols.len() {
                            c += 1;
                            h = 0.0;
                        }
                        let (dx, dy) = (cols[c].0 - cols[old].0, col_top + h - top);
                        for it in &mut page.items[range] {
                            *it = offset(it.clone(), dx, dy);
                        }
                        h += height;
                        col_fill[c] = col_top + h;
                    }
                }
                close_region!();
                if cols.len() > 1 {
                    y = col_fill.iter().copied().fold(col_top, f32::max);
                }
                region_s = s;
                col = 0;
                col_top = y;
                col_fill.clear();
                placed.clear();
                frame = Frame { top: frame.top, ..column_frame(s, 0) };
            }
            *self.page_format.borrow_mut() = setups[s].page_number_format.clone();
            self.body_section.set(s);
            if let Block::Paragraph(p) = block
                && p.page_break_before
                && !pages.last().is_some_and(|pg| pg.items.iter().all(|it| matches!(it, Item::Line { .. })))
            {
                new_page!(s);
                active_obstacle = find_page_obstacle(s, number, &frame).or(active_obstacle.take());
            }
            // No block starts inside an obstacle across its column down to the
            // column's end (a figure over the page): in the next column. (One
            // ending in the column, the block's lines go below it, a line at a
            // time as Word's do.)
            loop {
                let (left, right) = (frame.left - pt(setups[s].page.left), frame.left - pt(setups[s].page.left) + frame.width);
                let Some(bottom) = page_obstacles
                    .iter()
                    .filter(|o| o.shape_left - left < MIN_WRAP_ROOM && right - o.shape_right < MIN_WRAP_ROOM && o.y_top <= y + 0.5 && o.y_bottom > y + 0.01)
                    .map(|o| o.y_bottom)
                    .reduce(f32::max)
                else {
                    break;
                };
                if bottom >= frame.bottom - 0.01 {
                    advance!(s);
                } else {
                    break;
                }
            }
            self.column_dx.set(frame.left - pt(setups[s].page.left));
            let obstacle_for_block = if let Block::Paragraph(p) = block {
                let pad = |b: Option<FlowBorder>| b.map_or(0.0, |b| pt(b.space) + pt(b.total()));
                let before_pad = self.space_before(p) + pad(p.borders[0]);
                // Obstacles are kept from the left margin; the paragraph's
                // own x is from its column's left.
                let col_dx = frame.left - pt(setups[s].page.left);
                // The page's obstacles still below the paragraph's top (but
                // `main`), from the paragraph's place.
                let top = y + before_pad;
                let extra_obstacles = |all: &[WrapObstacle], main: Option<&WrapObstacle>| -> Vec<(f32, f32, f32, f32)> {
                    all.iter()
                        .filter(|o| o.y_bottom > top + 0.01 && main.is_none_or(|m| (m.y_top, m.shape_left, m.shape_right) != (o.y_top, o.shape_left, o.shape_right)))
                        .map(|o| (o.shape_left - col_dx, o.shape_right - col_dx, o.y_top - top, o.y_bottom - top))
                        .collect()
                };
                if let Some(shape) = square_shape(p, self.hides_deleted()).filter(|s| !self.deferred_shapes.borrow().contains(&s.id.0)) {
                    let f = shape.floating.as_ref().unwrap();
                    let sw = pt(shape.frame.width);
                    let sh = pt(shape.frame.height);
                    let dl = pt(f.dist_l);
                    let dr = pt(f.dist_r);
                    let dt = pt(f.dist_t);
                    let db = pt(f.dist_b);
                    let wrap_text = f.wrap_text.clone().unwrap_or_else(|| "bothSides".to_string());

                    let page_w = pt(setups[s].page.size.width);
                    let page_h = pt(setups[s].page.size.height);
                    let margin_l = pt(setups[s].page.left);
                    let margin_r = pt(setups[s].page.right);
                    let margin_t = pt(setups[s].page.top);
                    let margin_b = pt(setups[s].page.bottom);
                    let content_w = text_frame(s).width;
                    let _content_h = frame.bottom - frame.top;
                    let is_even = number % 2 == 0;

                    let (base_x0, base_w) = match f.relative_h.as_str() {
                        "page" => (0.0, page_w),
                        "leftMargin" => (0.0, margin_l),
                        "rightMargin" => (page_w - margin_r, margin_r),
                        "insideMargin" => if is_even { (page_w - margin_r, margin_r) } else { (0.0, margin_l) },
                        "outsideMargin" => if is_even { (0.0, margin_l) } else { (page_w - margin_r, margin_r) },
                        "column" => (frame.left, frame.width),
                        _ /* "margin" */ => (margin_l, content_w),
                    };

                    let abs_x = if f.relative_h == "character" {
                        match f.align_h.as_deref() {
                            Some("center") => margin_l - sw / 2.0,
                            Some("right") => margin_l - sw,
                            Some("left") => margin_l,
                            _ => margin_l + pt(f.offset_x),
                        }
                    } else {
                        match f.align_h.as_deref() {
                            Some("left") => base_x0,
                            Some("center") => base_x0 + (base_w - sw) / 2.0,
                            Some("right") => base_x0 + base_w - sw,
                            Some("inside") => if is_even { base_x0 + base_w - sw } else { base_x0 },
                            Some("outside") => if is_even { base_x0 } else { base_x0 + base_w - sw },
                            _ => base_x0 + pt(f.offset_x),
                        }
                    };
                    let abs_x = abs_x.clamp(0.0, (page_w - sw).max(0.0));

                    let para_top = y + before_pad;
                    let mark_size = p.mark.size as f32 / 100.0;
                    let mark_metrics = {
                        self.mark_metrics(p, mark_size)
                    };
                    let line_h = self.resolve_line_height(p, mark_metrics.0, mark_metrics.2, false);
                    let (base_y0, base_h) = match f.relative_v.as_str() {
                        "page" => (0.0, page_h),
                        "topMargin" => (0.0, margin_t),
                        "bottomMargin" => (page_h - margin_b, margin_b),
                        "insideMargin" => if is_even { (page_h - margin_b, margin_b) } else { (0.0, margin_t) },
                        "outsideMargin" => if is_even { (0.0, margin_t) } else { (page_h - margin_b, margin_b) },
                        "margin" => (margin_t, (page_h - margin_t - margin_b).max(0.0)),
                        "line" => (para_top, line_h),
                        "paragraph" => (para_top, 0.0),
                        _ => (para_top, 0.0),
                    };

                    let abs_y = match f.align_v.as_deref() {
                        Some("top") => base_y0,
                        Some("center") => if base_h > 0.0 { base_y0 + (base_h - sh) / 2.0 } else { base_y0 },
                        Some("bottom") => if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 },
                        Some("inside") => if is_even { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } } else { base_y0 },
                        Some("outside") => if is_even { base_y0 } else { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } },
                        _ => base_y0 + pt(f.offset_y),
                    };
                    let sx = abs_x - margin_l;
                    let sy = abs_y - para_top;
                    // Too tall for what is left of the page below its paragraph
                    // (Word's PDF of a paper: a figure box of a page's height goes
                    // to the top of the next page, the paragraph's text staying):
                    // its paragraph's floating shapes, it and those over it, are
                    // put off to the next page's top, the paragraph laid out
                    // without them.
                    let put_off = matches!(f.relative_v.as_str(), "paragraph" | "line")
                        && f.align_v.is_none()
                        && abs_y + sh + db > frame.bottom + 0.5
                        && y > col_top + 0.5;
                    if put_off {
                        // Placed from the next page's first column.
                        self.column_dx.set(0.0);
                        for r in p.runs.iter().filter(|r| !run_deleted(r, self.hides_deleted())) {
                            if let FlowRunKind::Shape(other) = &r.kind
                                && other.floating.as_ref().is_some_and(|g| matches!(g.relative_v.as_str(), "paragraph" | "line"))
                                && let Item::Shape { shape: placed, x, y: oy, w, h } = self.floating_item(other, 0.0, frame.width, 0.0, number)
                            {
                                deferred.push((placed, x, oy - sy, w, h));
                                self.deferred_shapes.borrow_mut().insert(other.id.0);
                            }
                        }
                        self.column_dx.set(col_dx);
                        None
                    } else {
                    let wrap_poly_abs = matches!(f.wrap.as_str(), "tight" | "through")
                        .then(|| f.wrap_polygon.clone())
                        .flatten()
                        .map(|poly| (poly, sx, abs_y, sw, sh, dl, dr));
                    let wrap_poly_rel = matches!(f.wrap.as_str(), "tight" | "through")
                        .then(|| f.wrap_polygon.clone())
                        .flatten()
                        .map(|poly| (poly, sx - col_dx, sy, sw, sh, dl, dr));

                    let obs = WrapObstacle {
                        shape_left: sx - dl,
                        shape_right: sx + sw + dr,
                        y_top: abs_y - dt,
                        y_bottom: abs_y + sh + db,
                        wrap_text: wrap_text.clone(),
                        wrap_polygon: wrap_poly_abs,
                    };
                    // Above its paragraph, over text laid out before it.
                    if abs_y + sh + db > col_top + 0.5 && abs_y - dt < para_top - 1.0 {
                        let item = Item::Shape { shape: Box::new(shape.clone()), x: abs_x, y: abs_y, w: sw, h: sh };
                        self.raised.borrow_mut().push((pages.len() - 1, obs.clone(), item, shape.id.0));
                    }
                    let extra = extra_obstacles(&page_obstacles, None);
                    active_obstacle = Some(obs.clone());
                    page_obstacles.push(obs.clone());
                    Some(WrapObstacleInPara {
                        shape_left: obs.shape_left - col_dx,
                        shape_right: obs.shape_right - col_dx,
                        rel_top: sy - dt,
                        rel_bottom: sy + sh + db,
                        wrap_text,
                        wrap_polygon: wrap_poly_rel,
                        shape_to_emit: Some((shape.clone(), sx - col_dx, sy, sw, sh, f.behind_doc)),
                        drop_cap_to_emit: None,
                        extra,
                    })
                    }
                } else if let Some(ref obs) = active_obstacle {
                    let rel_top = obs.y_top - (y + before_pad);
                    let rel_bottom = obs.y_bottom - (y + before_pad);
                    if rel_bottom > 0.01 {
                        let wrap_polygon = obs.wrap_polygon.as_ref().map(|(poly, sx, abs_y, sw, sh, dl, dr)| {
                            (poly.clone(), *sx - col_dx, *abs_y - (y + before_pad), *sw, *sh, *dl, *dr)
                        });
                        Some(WrapObstacleInPara {
                            shape_left: obs.shape_left - col_dx,
                            shape_right: obs.shape_right - col_dx,
                            rel_top,
                            rel_bottom,
                            wrap_text: obs.wrap_text.clone(),
                            wrap_polygon,
                            shape_to_emit: None,
                            drop_cap_to_emit: None,
                            extra: extra_obstacles(&page_obstacles, Some(obs)),
                        })
                    } else {
                        active_obstacle = None;
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            // 浮動の表: placed on the page from its anchor (not in the
            // text's flow), the text after it round it.
            if let Block::Table(t) = block
                && let Some(f) = t.floating
            {
                use ibis_doc_model::{TableAlign, TableAnchor};
                let pieces = self.pieces(block, frame.width, number, None, false);
                let h: f32 = pieces.iter().map(|p| p.height).sum();
                let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                for it in pieces.iter().flat_map(|p| &p.items) {
                    let (a, b) = match it {
                        Item::Rect { x, w, .. } | Item::Shading { x, w, .. } => (*x, x + w),
                        Item::Stroke { x, x2, w, .. } => (x.min(*x2) - w / 2.0, x.max(*x2) + w / 2.0),
                        _ => continue,
                    };
                    lo = lo.min(a);
                    hi = hi.max(b);
                }
                if lo > hi {
                    (lo, hi) = (0.0, 0.0);
                }
                let w = hi - lo;
                let page_w = pt(setups[s].page.size.width);
                let page_h = pt(setups[s].page.size.height);
                let text = text_frame(s);
                let (base_x, base_w) = match f.horz_anchor {
                    TableAnchor::Page => (0.0, page_w),
                    TableAnchor::Margin => (text.left, text.width),
                    TableAnchor::Text => (frame.left, frame.width),
                };
                let odd = number % 2 == 1;
                // Measured on Word's PDF of DTFL: an offset puts the table's
                // edge 0.24 pt before it.
                let left = match f.x_align {
                    Some(TableAlign::Start) => base_x,
                    Some(TableAlign::Center) => base_x + (base_w - w) / 2.0,
                    Some(TableAlign::End) => base_x + base_w - w,
                    Some(TableAlign::Inside) => if odd { base_x } else { base_x + base_w - w },
                    Some(TableAlign::Outside) => if odd { base_x + base_w - w } else { base_x },
                    None => base_x + pt(f.x) - 0.24,
                };
                let (base_y, base_h) = match f.vert_anchor {
                    TableAnchor::Page => (0.0, page_h),
                    TableAnchor::Margin => (text.top, text.bottom - text.top),
                    TableAnchor::Text => (y, 0.0),
                };
                let top = match f.y_align {
                    Some(TableAlign::Start) => base_y,
                    Some(TableAlign::Center) => base_y + (base_h - h) / 2.0,
                    Some(TableAlign::End) => base_y + base_h - h,
                    _ => base_y + pt(f.y),
                };
                let mut at = top;
                let page = pages.last_mut().expect("a page");
                // The cells' text 0.9 pt further left than the borders' outer
                // edge would put it (DTFL: each placement's text 0.9 pt left
                // of where this layout's cell text falls).
                let dx = left - lo - 0.9;
                for piece in pieces {
                    page.items.extend(piece.items.into_iter().map(|it| offset(it, dx, at)));
                    at += piece.height;
                }
                let [dl, dr, dt, db] = f.distance.map(pt);
                active_obstacle = Some(WrapObstacle { shape_left: left - dl - text.left, shape_right: left + w + dr - text.left, y_top: top - dt, y_bottom: top + h + db, wrap_text: "bothSides".into(), wrap_polygon: None });
                continue;
            }
            let mut pieces = self.pieces(block, frame.width, number, obstacle_for_block.as_ref(), false);
            self.column_dx.set(0.0);
            // Where the pieces were laid out for (their obstacles are that column's).
            let laid_for = (pages.len(), col);
            // A section ending with the document's last paragraph starts no page.
            if b + 1 == self.doc.body.len()
                && matches!(block, Block::Paragraph(p) if p.section_break != SectionBreak::None)
                && let Some(last) = pieces.last_mut()
            {
                last.page_break = false;
            }
            // A paragraph kept whole (段落を分割しない) starts the next page
            // when its lines do not all fit (and they fit on a page).
            if let Block::Paragraph(p) = block
                && p.keep_lines
            {
                let total: f32 = pieces.iter().map(|p| p.height).sum();
                if y + total > frame.bottom + 0.01 && (y > col_top + 0.01 || col_top > frame.top + 0.01 || col + 1 < columns_of(s).len()) && total <= frame.bottom - frame.top {
                    advance!(s);
                    if obstacle_for_block.is_some() {
                        self.column_dx.set(frame.left - pt(setups[s].page.left));
                        pieces = self.pieces(block, frame.width, number, None, false);
                        self.column_dx.set(0.0);
                    }
                }
            }
            // Widow/orphan control: of a paragraph's lines, the page's end
            // does not leave the first alone (all of them go on) nor the last
            // alone (the one before goes on too).
            let mut break_before: Option<usize> = None;
            if let Block::Paragraph(p) = block
                && p.widow_control
                && !p.keep_lines
                && pieces.len() >= 2
                && pieces.iter().all(|q| q.split.is_none() && !q.header && !q.page_break && !q.column_break)
            {
                let page = pages.last().expect("a page");
                let limit = frame.bottom - notes_room(page) + 0.01;
                let mut used = y;
                let mut fit = 0usize;
                for q in &pieces {
                    if used + q.height > limit {
                        break;
                    }
                    used += q.height;
                    fit += 1;
                }
                let n = pieces.len();
                let can_move = y > col_top + 0.01 || col_top > frame.top + 0.01 || col + 1 < columns_of(s).len();
                if fit > 0 && fit < n && can_move {
                    if fit == 1 || (fit == n - 1 && n == 3) {
                        advance!(s);
                        if obstacle_for_block.is_some() {
                            self.column_dx.set(frame.left - pt(setups[s].page.left));
                        pieces = self.pieces(block, frame.width, number, None, false);
                        self.column_dx.set(0.0);
                        }
                    } else if fit == n - 1 {
                        break_before = Some(n - 2);
                    }
                }
            }
            let headers: Vec<Piece> = pieces.iter().take_while(|p| p.header).cloned().collect();
            // An empty paragraph that only ends a section, the next going on
            // on the page in other columns: Word gives it no height (its PDF of
            // a two-column paper: the columns start right under the space after
            // the paragraph before).
            let no_height = matches!(block, Block::Paragraph(p) if p.runs.is_empty() && p.section.is_some() && p.section_break == SectionBreak::Continuous)
                && columns_of(s).len() != columns_of((s + 1).min(setups.len() - 1)).len();
            // A paragraph whose mark is deleted, where the view leaves deleted
            // text out, is one with the next: nothing of it when its text is all
            // deleted too, else no space between the two (their lines are not
            // joined).
            let hide_deleted = self.hides_deleted();
            let mark_gone = matches!(block, Block::Paragraph(p) if p.mark_deleted.as_ref().is_some_and(|a| hide_deleted || !is_reviewer_shown(a)));
            let all_gone = matches!(block, Block::Paragraph(p) if p.runs.iter().all(|r| run_deleted(r, hide_deleted)));
            let no_height = no_height || (mark_gone && all_gone);
            let joined = joins_next;
            if !no_height {
                joins_next = mark_gone;
            }
            let mut queue: std::collections::VecDeque<Piece> = pieces.into();
            let mut popped = 0usize;
            while let Some(mut piece) = queue.pop_front() {
                if break_before == Some(popped) {
                    advance!(s);
                    break_before = None;
                }
                popped += 1;
                if joined && popped == 1 && !no_height {
                    let above = piece.space_before;
                    piece.items = piece.items.into_iter().map(|it| offset(it, 0.0, -above)).collect();
                    piece.height -= above;
                    piece.space_before = 0.0;
                }
                if mark_gone && queue.is_empty() && !no_height {
                    piece.height -= piece.space_after;
                    piece.space_after = 0.0;
                }
                if no_height {
                    // Its line stays (for the caret), at the top of its place.
                    let above = piece.space_before;
                    piece.items = piece.items.into_iter().map(|it| offset(it, 0.0, -above)).collect();
                    piece.height = 0.0;
                    piece.space_before = 0.0;
                    piece.space_after = 0.0;
                }
                // Paragraphs side by side: the larger of the space after one
                // and before the next, not both.
                if let Some((pc, c, ye, after)) = prev_end
                    && pc == pages.len()
                    && c == col
                    && (y - ye).abs() < 0.01
                    && after > 0.0
                    && piece.space_before > 0.0
                {
                    y -= after.min(piece.space_before);
                }
                // A piece laid out before an obstacle of the page was there (a
                // figure put off to this page, or the piece gone on to another
                // column) goes below one across its column.
                while obstacle_for_block.is_none() || (pages.len(), col) != laid_for {
                    let (left, right) = (frame.left - pt(setups[s].page.left), frame.left - pt(setups[s].page.left) + frame.width);
                    let Some(bottom) = page_obstacles
                        .iter()
                        .filter(|o| o.shape_left - left < MIN_WRAP_ROOM && right - o.shape_right < MIN_WRAP_ROOM && o.y_top < y + piece.height - 0.01 && o.y_bottom > y + 0.01)
                        .map(|o| o.y_bottom)
                        .reduce(f32::max)
                    else {
                        break;
                    };
                    if bottom + piece.height > frame.bottom + 0.01 {
                        advance!(s);
                    } else {
                        y = bottom;
                        break;
                    }
                }
                // A table row that does not fit is divided where the page
                // ends (when a line of each cell fits there), its rest
                // going on at the top of the next page.
                if piece.height > frame.bottom - y + 0.01
                    && let Some(split) = piece.split.clone()
                    && let Some((first, rest)) = split.split(frame.bottom - y)
                {
                    queue.push_front(rest);
                    piece = first;
                    piece.page_break = false;
                    let items = piece.items.into_iter().map(|it| place_item(it, y, s, number, frame.left));
                    let page = pages.last_mut().expect("a page");
                    let start = page.items.len();
                    page.items.extend(items);
                    let end = page.items.len();
                    placed.push((start..end, y, frame.bottom - y, col));
                    if col_fill.len() <= col {
                        col_fill.resize(col + 1, col_top);
                    }
                    col_fill[col] = frame.bottom;
                    advance!(s);
                    for h in &headers {
                        pages.last_mut().expect("a page").items.extend(h.items.iter().cloned().map(|it| place_item(it, y, s, number, frame.left)));
                        y += h.height;
                    }
                    continue;
                }
                // A paragraph kept with the next goes to the next page with
                // the next block's first line when that does not fit.
                let kept = if piece.keep_with_next {
                    if let Some(next) = self.doc.body.get(b + 1) {
                        let next_s = sections_of[b + 1];
                        let saved_pitch = self.line_pitch.get();
                        self.line_pitch.set(setups[next_s].page.line_pitch);
                        let h = self.pieces(next, frame.width, number, None, false).into_iter().next().map_or(0.0, |n| n.height);
                        self.line_pitch.set(saved_pitch);
                        h
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                // Room for the footnotes of the marks on it, at the page's
                // bottom.
                let page = pages.last().expect("a page");
                let new_notes: Vec<u32> = notes_of(&piece).into_iter().filter(|id| !page.notes.contains(id)).collect();
                let room = notes_room(page);
                let more = new_notes.iter().map(|id| note_heights.get(id).copied().unwrap_or(0.0)).sum::<f32>() + if page.notes.is_empty() && !new_notes.is_empty() { separator_h } else { 0.0 };
                if y + piece.height + kept > frame.bottom - room - more + 0.01 && (y > col_top + 0.01 || col_top > frame.top + 0.01 || col + 1 < columns_of(s).len()) {
                    advance!(s);
                    // A table's heading rows start the new page.
                    if !piece.header {
                        for h in &headers {
                            pages.last_mut().expect("a page").items.extend(h.items.iter().cloned().map(|it| place_item(it, y, s, number, frame.left)));
                            y += h.height;
                        }
                    }
                }
                let page = pages.last_mut().expect("a page");
                for id in new_notes {
                    if !page.notes.contains(&id) {
                        page.notes.push(id);
                    }
                }
                let items = piece.items.into_iter().map(|it| place_item(it, y, s, number, frame.left));
                let start = page.items.len();
                page.items.extend(items);
                // A shape the text wraps round, where it landed (its paragraph may
                // have gone on to another page or column than it was laid out
                // for): an obstacle of this page there.
                let margin_l = pt(setups[s].page.left);
                let landed: Vec<WrapObstacle> = page.items[start..]
                    .iter()
                    .filter_map(|it| match it {
                        Item::Shape { shape, x, y, w, h } => {
                            let f = shape.floating.as_ref()?;
                            matches!(f.wrap.as_str(), "square" | "tight" | "through").then(|| WrapObstacle {
                                shape_left: x - margin_l - pt(f.dist_l),
                                shape_right: x + w - margin_l + pt(f.dist_r),
                                y_top: y - pt(f.dist_t),
                                y_bottom: y + h + pt(f.dist_b),
                                wrap_text: f.wrap_text.clone().unwrap_or_else(|| "bothSides".to_string()),
                                wrap_polygon: None,
                            })
                        }
                        _ => None,
                    })
                    .collect();
                for obs in landed {
                    let same = |o: &WrapObstacle| (o.y_top - obs.y_top).abs() < 0.5 && (o.shape_left - obs.shape_left).abs() < 0.5;
                    if !page_obstacles.iter().any(same) {
                        if active_obstacle.as_ref().is_none_or(|a| !same(a)) {
                            active_obstacle = Some(obs.clone());
                        }
                        page_obstacles.push(obs);
                    }
                }
                let page = pages.last_mut().expect("a page");
                placed.push((start..page.items.len(), y, piece.height, col));
                y += piece.height;
                prev_end = Some((pages.len(), col, y, piece.space_after));
                if col_fill.len() <= col {
                    col_fill.resize(col + 1, col_top);
                }
                col_fill[col] = col_fill[col].max(y);
                // 段区切り (and a section starting in the next column).
                if piece.column_break && !piece.page_break {
                    advance!(s);
                }
                // A page break right before an empty paragraph that ends a
                // section on a new page: that paragraph stays on this page
                // (Word's PDF of DSEC makes one page of them, not two).
                let ends_section = matches!(block, Block::Paragraph(p) if p.section_break != SectionBreak::None) && queue.is_empty();
                let breaks_again = !ends_section
                    && queue.is_empty()
                    && matches!(self.doc.body.get(b + 1), Some(Block::Paragraph(next)) if next.runs.is_empty() && next.section.is_some() && matches!(next.section_break, SectionBreak::NextPage | SectionBreak::OddPage | SectionBreak::EvenPage));
                if piece.page_break && !breaks_again {
                    // The page after a section's end is the next section's.
                    let s_next = if ends_section { (s + 1).min(setups.len() - 1) } else { s };
                    new_page!(s_next);
                    active_obstacle = find_page_obstacle(s_next, number, &frame).or(active_obstacle.take());
                    // An odd- or even-page section break: a blank page
                    // between when the next page has the other parity.
                    let wanted = match block {
                        Block::Paragraph(p) if p.section_break == SectionBreak::OddPage => Some(1),
                        Block::Paragraph(p) if p.section_break == SectionBreak::EvenPage => Some(0),
                        _ => None,
                    };
                    if wanted.is_some_and(|w| number % 2 != w) {
                        // The page just begun stays blank, the section's
                        // before; the next section starts on the one after.
                        let last = pages.last_mut().expect("a page");
                        last.section = s;
                        last.first_in_section = false;
                        new_page!(s_next);
                    }
                }
            }
            if let Some(ref obs) = active_obstacle {
                if y >= obs.y_bottom - 0.01 {
                    active_obstacle = None;
                }
            }
        }
        close_region!();
        // The endnotes: either at each section's end or at the document's end.
        let endnotes_with_section: Vec<(usize, u32)> = {
            let mut all: Vec<(usize, u32)> = Vec::new();
            for (b, block) in self.doc.body.iter().enumerate() {
                let s = sections_of[b];
                fn walk(b: &Block, s: usize, out: &mut Vec<(usize, u32)>) {
                    match b {
                        Block::Paragraph(p) => {
                            for r in &p.runs {
                                if let FlowRunKind::NoteRef { endnote: true, id } = r.kind
                                    && !out.iter().any(|(_, i)| *i == id)
                                {
                                    out.push((s, id));
                                }
                            }
                        }
                        Block::Table(t) => {
                            for row in &t.rows {
                                for cell in &row.cells {
                                    for cb in &cell.blocks {
                                        walk(cb, s, out);
                                    }
                                }
                            }
                        }
                    }
                }
                walk(block, s, &mut all);
            }
            all
        };
        let mut sect_endnotes: std::collections::HashMap<usize, Vec<u32>> = std::collections::HashMap::new();
        let mut doc_endnotes: Vec<u32> = Vec::new();
        for (s, id) in endnotes_with_section {
            let pos = setups.get(s).map_or(self.doc.page.endnotes.position, |st| st.page.endnotes.position);
            if pos == ibis_doc_model::NotePosition::SectionEnd {
                sect_endnotes.entry(s).or_default().push(id);
            } else {
                doc_endnotes.push(id);
            }
        }
        for (s, ids) in sect_endnotes {
            if ids.is_empty() {
                continue;
            }
            if let Some(pg_idx) = pages.iter().rposition(|p| p.section == s) {
                kind_now.set((pages[pg_idx].section, page_kind(pages[pg_idx].section, pages[pg_idx].number, pages[pg_idx].first_in_section)));
                let frame = frame_of(s);
                let pg = &mut pages[pg_idx];
                let body_bottom = pg
                    .items
                    .iter()
                    .filter_map(|it| match it {
                        Item::Line { y, h, .. } => Some(y + h),
                        Item::Picture { y, h, .. } | Item::Shape { y, h, .. } | Item::Rect { y, h, .. } => Some(y + h),
                        _ => None,
                    })
                    .fold(frame.top, f32::max);
                let mut note_y = (body_bottom + 12.0).min(frame.bottom);
                if let Some(w) = separator_width(self.doc.note_separators[0], frame.width) {
                    pg.items.push(Item::Rect { x: frame.left, y: note_y + separator_h / 2.0, w, h: 0.5, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) });
                }
                note_y += separator_h;
                for id in ids {
                    for piece in note_pieces(true, id) {
                        pg.items.extend(piece.items.into_iter().map(|it| offset(it, frame.left, note_y)));
                        note_y += piece.height;
                    }
                }
            }
        }
        if !doc_endnotes.is_empty() {
            let s = setups.len() - 1;
            let mut queue: Vec<Piece> = Vec::new();
            let end_separator: Vec<Item> = separator_width(self.doc.note_separators[1], frame.width).map(|w| Item::Rect { x: 0.0, y: separator_h / 2.0, w, h: 0.5, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) }).into_iter().collect();
            queue.push(Piece { height: separator_h, items: end_separator, page_break: false, column_break: false, keep_with_next: true, header: false, split: None, ..Default::default() });
            for id in &doc_endnotes {
                queue.extend(note_pieces(true, *id));
            }
            for piece in queue {
                if y + piece.height > frame.bottom - notes_room(pages.last().expect("a page")) + 0.01 && y > frame.top + 0.01 {
                    new_page!(s);
                }
                let items = piece.items.into_iter().map(|it| offset(it, frame.left, y));
                pages.last_mut().expect("a page").items.extend(items);
                y += piece.height;
            }
        }
        // The footnotes under a separator: either at page bottom or beneath text.
        for pg in pages.iter_mut() {
            if pg.notes.is_empty() {
                continue;
            }
            kind_now.set((pg.section, page_kind(pg.section, pg.number, pg.first_in_section)));
            let frame = frame_of(pg.section);
            let pos = setups.get(pg.section).map_or(self.doc.page.footnotes.position, |s| s.page.footnotes.position);
            let mut y = match pos {
                ibis_doc_model::NotePosition::BeneathText => {
                    let body_bottom = pg
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            Item::Line { y, h, .. } => Some(y + h),
                            Item::Picture { y, h, .. } | Item::Shape { y, h, .. } | Item::Rect { y, h, .. } => Some(y + h),
                            _ => None,
                        })
                        .fold(frame.top, f32::max);
                    let spacing = 12.0;
                    (body_bottom + spacing).min(frame.bottom - notes_room(pg))
                }
                _ => frame.bottom - notes_room(pg),
            };
            if let Some(w) = separator_width(self.doc.note_separators[0], frame.width) {
                pg.items.push(Item::Rect { x: frame.left, y: y + separator_h / 2.0, w, h: 0.5, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) });
            }
            y += separator_h;
            for id in pg.notes.clone() {
                for piece in note_pieces(false, id) {
                    pg.items.extend(piece.items.into_iter().map(|it| offset(it, frame.left, y)));
                    y += piece.height;
                }
            }
        }
        if std::env::var_os("IBIS_DEBUG_PAGES").is_some() {
            for (n, pg) in pages.iter().enumerate() {
                let first: String = pg.items.iter().filter_map(|it| if let Item::Text { text, .. } = it { Some(text.to_string()) } else { None }).collect::<String>().chars().take(16).collect();
                eprintln!("{}\t{}\t{first}", n + 1, pg.number);
            }
        }
        // 垂直方向の配置 (vAlign): the body moved down to the middle or the
        // bottom of the text area; 見開きページ: an even page's body moved
        // to its swapped margins.
        for pg in &mut pages {
            let page = &setups[pg.section].page;
            kind_now.set((pg.section, page_kind(pg.section, pg.number, pg.first_in_section)));
            let frame = text_frame(pg.section);
            let used = pg
                .items
                .iter()
                .filter_map(|it| match it {
                    Item::Line { y, h, .. } => Some(y + h),
                    Item::Picture { y, h, .. } | Item::Shape { y, h, .. } | Item::Rect { y, h, .. } => Some(y + h),
                    _ => None,
                })
                .fold(f32::MIN, f32::max);
            let room = frame.bottom - used;
            let dy = match page.v_align {
                ibis_doc_model::PageVAlign::Center if used > f32::MIN && room > 0.0 => room / 2.0,
                ibis_doc_model::PageVAlign::Bottom if used > f32::MIN && room > 0.0 => room,
                _ => 0.0,
            };
            let dx = mirror_dx(pg.section, pg.number);
            if dx != 0.0 || dy != 0.0 {
                pg.items = std::mem::take(&mut pg.items).into_iter().map(|it| offset(it, dx, dy)).collect();
            }
        }
        // 行番号: the body's lines (not tables', not suppressed, not an
        // empty paragraph that only ends a section) counted by their
        // section's numbering, every count_by'th numbered left of the text
        // (Word's PDF of DSEC: right-aligned a quarter inch, or the
        // distance, from the text; "continuous" goes on from the section
        // before when that had numbers, whatever its restarting).
        {
            use ibis_doc_model::LineNumberRestart;
            // Each numbered paragraph's section.
            let mut numbered: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
            for (b, block) in self.doc.body.iter().enumerate() {
                if let Block::Paragraph(p) = block
                    && !p.suppress_line_numbers
                    && !(p.runs.is_empty() && p.section.is_some())
                {
                    numbered.insert(p.id, sections_of[b]);
                }
            }
            let (_, base) = self.doc.styles.paragraph_base(None, None);
            let style = base.style();
            let size = style.size as f32 / 100.0;
            let font = self.font(&style, style.font_for('1'));
            let mut count = 0u32;
            // The section of the last line counted, and whether it was numbered.
            let mut last: Option<(usize, bool)> = None;
            for (pg_index, pg) in pages.iter_mut().enumerate() {
                let lines: Vec<(f32, f32, usize)> = pg
                    .items
                    .iter()
                    .filter_map(|it| match it {
                        Item::Line { para, y, h, .. } => numbered.get(para).map(|s| (*y, *h, *s)),
                        _ => None,
                    })
                    .collect();
                let mut seen: Vec<f32> = Vec::new();
                let mut new_page = true;
                let mut labels = Vec::new();
                for (y, h, s) in lines {
                    // A line split round an obstacle counts once.
                    if seen.iter().any(|t| (t - y).abs() < 0.01) {
                        continue;
                    }
                    seen.push(y);
                    let Some(n) = setups[s].page.line_numbers else {
                        last = Some((s, false));
                        continue;
                    };
                    let new_section = last.is_none_or(|(ls, _)| ls != s);
                    let continues = n.restart == LineNumberRestart::Continuous && last.is_some_and(|(_, numbered)| numbered);
                    if (new_section && !continues) || (new_page && n.restart == LineNumberRestart::NewPage) {
                        count = n.start - 1;
                    }
                    new_page = false;
                    last = Some((s, true));
                    count += 1;
                    if count % n.count_by != 0 {
                        continue;
                    }
                    let distance = if n.distance > 0 { pt(n.distance) } else { 18.0 };
                    labels.push((y, h, count, text_frame(s).left - distance));
                }
                let _ = pg_index;
                for (y, h, n, right) in labels {
                    // On the line's baseline (its text's).
                    let baseline = pg.items.iter().find_map(|it| match it {
                        Item::Text { y: by, .. } if *by > y && *by <= y + h + 0.01 => Some(*by),
                        _ => None,
                    }).unwrap_or(y + h * 0.8);
                    let label = n.to_string();
                    let w = self.res.text_width(&label, &font, size);
                    let mut offsets = Vec::new();
                    let mut at = 0.0;
                    for c in label.chars() {
                        offsets.push(at);
                        at += self.res.text_width(&c.to_string(), &font, size);
                    }
                    pg.items.push(Item::Text { x: right - w, y: baseline, text: label.into(), offsets: offsets.into(), font: font.clone(), size, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) });
                }
            }
        }
        // Headers and footers, with each page's number: the first page's
        // own, the even pages' own, or the default, of the page's section.
        let total = pages.len() as u32;
        let section_pages: Vec<u32> = pages.iter().map(|pg| pages.iter().filter(|q| q.section == pg.section).count() as u32).collect();
        // 章番号つきのページ番号: each page's chapter, the number of the first heading of the chapter
        // style that starts on it, else the one before (`w:pgNumType/@w:chapStyle`).
        let heading_levels: std::collections::HashMap<u32, u32> = self.doc.body.iter().filter_map(|b| match b {
            Block::Paragraph(p) => ibis_doc_model::flow_command::heading_level(&self.doc.styles, p).map(|l| (p.id, l)),
            _ => None,
        }).collect();
        let mut chapter_now: Option<String> = None;
        let chapter_prefixes: Vec<String> = pages.iter().map(|pg| {
            let Some((level, sep)) = &setups[pg.section].page_chapter else { return String::new() };
            let starts = pg.items.iter().find_map(|item| match item {
                Item::Line { para, stops, .. } if stops.first().is_some_and(|s| s.0 == 0) && heading_levels.get(para) == Some(level) => Some(*para),
                _ => None,
            });
            if let Some(id) = starts {
                chapter_now = self.labels.get(&id).cloned();
            }
            let separator = match sep.as_str() {
                "period" => '.',
                "colon" => ':',
                "emDash" => '\u{ff0d}',
                _ => '-',
            };
            chapter_now.as_ref().map_or(String::new(), |l| format!("{l}{separator}"))
        }).collect();
        for ((pg, &in_section), chapter) in pages.iter_mut().zip(&section_pages).zip(&chapter_prefixes) {
            pg.body_items_count = pg.items.len();
            *self.chapter_prefix.borrow_mut() = chapter.clone();
            self.page_counts.set(Some((total, in_section, pg.section as u32 + 1)));
            let s = pg.section;
            let setup = &setups[s];
            let mut frame = margin_frame(s);
            let n = pg.number;
            frame.left += mirror_dx(s, n);
            *self.page_format.borrow_mut() = setup.page_number_format.clone();
            self.line_pitch.set(setup.page.line_pitch);
            let (header, footer) = header_footer(s, page_kind(s, n, pg.first_in_section));
            let mut y = pt(setup.page.header);
            for block in header {
                for piece in self.pieces(block, frame.width, n, None, true) {
                    pg.items.extend(piece.items.into_iter().map(|it| offset(it, frame.left, y)));
                    y += piece.height;
                }
            }
            let footer: Vec<Piece> = footer.iter().flat_map(|b| self.pieces(b, frame.width, n, None, true)).collect();
            let height: f32 = footer.iter().map(|p| p.height).sum();
            let mut y = pg.height - pt(setup.page.footer) - height;
            for piece in footer {
                pg.items.extend(piece.items.into_iter().map(|it| offset(it, frame.left, y)));
                y += piece.height;
            }
        }
        self.chapter_prefix.borrow_mut().clear();
        for pg in &mut pages {
            pg.items = std::mem::take(&mut pg.items).into_iter().map(|it| offset(it, TEXT_ORIGIN_X, 0.0)).collect();
        }
        // Page borders: `space` from the page's edges to their outer side,
        // or from the text area (the margins) to their inner side.
        for pg in &mut pages {
            let page = &setups[pg.section].page;
            if !page.borders.iter().any(Option::is_some) {
                continue;
            }
            let frame = margin_frame(pg.section);
            let (page_w, page_h) = (pg.width, pg.height);
            let [top, right, bottom, left] = page.borders;
            let total = |b: Option<FlowBorder>| b.map_or(0.0, |b| pt(b.total()));
            let space = |b: Option<FlowBorder>| b.map_or(0.0, |b| pt(b.space));
            let (t, r, b, l) = if page.borders_from_text {
                (
                    frame.top - space(top) - total(top) / 2.0,
                    page_w - pt(page.right) + space(right) + total(right) / 2.0,
                    frame.bottom + space(bottom) + total(bottom) / 2.0,
                    frame.left - space(left) - total(left) / 2.0,
                )
            } else {
                (space(top) + total(top) / 2.0, page_w - space(right) - total(right) / 2.0, page_h - space(bottom) - total(bottom) / 2.0, space(left) + total(left) / 2.0)
            };
            // An art border's corner: the top or bottom border's picture
            // (else the left or right one's), sz × sz about the corner.
            let corner = |cx: f32, cy: f32, b1: Option<&FlowBorder>, b2: Option<&FlowBorder>, part: ArtPart, items: &mut Vec<Item>| {
                let art_border = b1.and_then(|b| b.art.map(|a| (b, a))).or_else(|| b2.and_then(|b| b.art.map(|a| (b, a))));
                if let Some((fb, art)) = art_border
                    && art_border_drawn(art)
                    && art_border::picture(art, part).is_some()
                {
                    let sz = pt(fb.width);
                    let tint = art_border::tint(art, fb.color);
                    // Fitted in sz × sz, keeping its proportions (heartBalloon's
                    // corner is 330 × 660: half as wide as it is high).
                    let (pw, ph) = art_border::picture(art, part).map_or((1.0, 1.0), |p| (p.width as f32, p.height as f32));
                    let k = sz / pw.max(ph) * art_border::scale(art, sz);
                    let (iw, ih) = (pw * k, ph * k);
                    // Against the outer corner for the kinds made smaller there.
                    let (x, y) = if art_border::outer(art) {
                        let left = matches!(part, ArtPart::TopLeft | ArtPart::BottomLeft);
                        let top = matches!(part, ArtPart::TopLeft | ArtPart::TopRight);
                        (if left { cx - sz / 2.0 } else { cx + sz / 2.0 - iw }, if top { cy - sz / 2.0 } else { cy + sz / 2.0 - ih })
                    } else {
                        (cx - iw / 2.0, cy - ih / 2.0)
                    };
                    art_border::warm(art, part, tint, iw.min(ih));
                    items.push(Item::Art { art, part, x, y, w: iw, h: ih, image_w: iw, image_h: ih, tint });
                }
            };
            corner(l, t, top.as_ref(), left.as_ref(), ArtPart::TopLeft, &mut pg.items);
            corner(r, t, top.as_ref(), right.as_ref(), ArtPart::TopRight, &mut pg.items);
            corner(l, b, bottom.as_ref(), left.as_ref(), ArtPart::BottomLeft, &mut pg.items);
            corner(r, b, bottom.as_ref(), right.as_ref(), ArtPart::BottomRight, &mut pg.items);
            if let Some(x) = top {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Top, l, r, t, true, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, true, true, t, l, r, true, true));
                }
            }
            if let Some(x) = bottom {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Bottom, l, r, b, true, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, true, false, b, l, r, true, true));
                }
            }
            if let Some(x) = left {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Left, t, b, l, false, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, false, true, l, t, b, true, true));
                }
            }
            if let Some(x) = right {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Right, t, b, r, false, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, false, false, r, t, b, true, true));
                }
            }
        }
        for pg in &mut pages {
            let mut expanded = Vec::with_capacity(pg.items.len());
            for it in pg.items.drain(..) {
                match it {
                    Item::Shading { pattern, color, x, y, w, h } => {
                        expanded.extend(pattern_items(pattern, color, x, y, w, h));
                    }
                    other => expanded.push(other),
                }
            }
            pg.items = expanded;
        }
        for pg in &mut pages {
            if matches!(directions[pg.section], ibis_doc_model::CellDirection::TbRl | ibis_doc_model::CellDirection::BtLr) {
                let direction = directions[pg.section];
                // The paper: the across page's height is its width.
                let (w, h) = (pg.height, pg.width);
                pg.items = turn_page_items(std::mem::take(&mut pg.items), direction, w, h);
                (pg.width, pg.height) = (w, h);
            }
        }
        Pagination { pages }
    }
}

/// Whether the cell at `col` in the row below `r` continues a vertical merge.
fn continues_below(t: &FlowTable, grid: &[Vec<impl HasCol>], r: usize, col: usize) -> bool {
    let Some(row) = t.rows.get(r + 1) else { return false };
    grid[r + 1].iter().zip(&row.cells).any(|(l, c)| l.col() == col && c.merge == VerticalMerge::Continue)
}

trait HasCol {
    fn col(&self) -> usize;
}

/// Whether Word draws this art border at all. The model keeps every `w:val`
/// (and saving writes it back); only drawing stops.
///
/// candyCorn・checkedBarColor・confettiOutline・cornerTriangles・decoArchColor・
/// pushPinNote1 は、Mac の Word（2026-10-05 ユーザーが印刷レイアウトの画面で確認）が
/// 描かない。PDF の書き出しにも出ない（網羅試験 DPGB の正解 PDF、9/30 に Mac の Word で作成）。
/// Windows の Word では未確認。
fn art_border_drawn(art: ibis_doc_model::ArtBorder) -> bool {
    use ibis_doc_model::ArtBorder::*;
    !matches!(art, CandyCorn | CheckedBarColor | ConfettiOutline | CornerTriangles | DecoArchColor | PushPinNote1)
}

/// The pictures of art border `art` along an edge, `part`, from `from` to
/// `to` (the corners' centres) at `across` (the border's middle line). A
/// picture as long as the page (a strip in the zip) is drawn once, sz thick
/// and cut at the corners; a shorter one is repeated, sz thick and as many as
/// fit at its own proportions, stretched to fill the edge evenly.
fn draw_art_edge(fb: &FlowBorder, art: ibis_doc_model::ArtBorder, part: ArtPart, from: f32, to: f32, across: f32, horizontal: bool, items: &mut Vec<Item>) {
    if !art_border_drawn(art) {
        return;
    }
    let Some(picture) = art_border::picture(art, part) else { return };
    let sz = pt(fb.width);
    if sz <= 0.0 {
        return;
    }
    let tint = art_border::tint(art, fb.color);
    // From corner to corner; the kinds made smaller against the outer side
    // have smaller corners there, which the edge meets.
    let inset = if art_border::outer(art) { sz * art_border::scale(art, sz) - sz / 2.0 } else { sz / 2.0 };
    let (start, end) = (from.min(to) + inset, from.max(to) - inset);
    let span = end - start;
    if span <= 0.0 {
        return;
    }
    // The pictures are drawn to one scale: the corner's longer side is sz
    // (Word's references: moons' edge 469 × 240 with a 240 × 240 corner
    // repeats every 1.9 to 2.0 sz, DPGB-00243/00244; heartBalloon's 330 × 660
    // with a 330 × 660 corner every sz, half as wide, DPGB-00211/00212;
    // confetti's 313 × 400 with a 400 × 400 corner every 0.75 to 0.79 sz,
    // DPGB-00125/00126).
    let (cw, ch) = art_border::picture(art, ArtPart::TopLeft).map_or((picture.width.min(picture.height), picture.width.min(picture.height)), |c| (c.width, c.height));
    let k = sz / cw.max(ch) as f32;
    // Their spacing goes by the corner's width along the top and bottom,
    // its height along the sides: heartBalloon (corner 330 × 660) repeats
    // every sz along the top too, its balloons half as wide (DPGB-00212),
    // paperClips (corner 280 × 560, top 172 × 560) every 0.62 sz (DPGB-00260).
    let cell = if horizontal { picture.width as f32 / cw as f32 } else { picture.height as f32 / ch as f32 } * sz;
    let (along, thick) = if horizontal { (picture.width as f32 * k, picture.height as f32 * k) } else { (picture.height as f32 * k, picture.width as f32 * k) };
    // Drawn smaller than its cell for some kinds, centred in it.
    let s = art_border::scale(art, sz);
    // Where the picture's band starts across the edge: about the middle
    // line, or from the outer side (the top and left edges' outer side is
    // their start).
    let first = matches!(part, ArtPart::Top | ArtPart::Left);
    let band = |t: f32| if !art_border::outer(art) { across - t / 2.0 } else if first { across - thick / 2.0 } else { across + thick / 2.0 - t };
    let place = |a: f32, len: f32, image_len: f32, strip: bool| {
        let (len, image_len, a, t) = if strip { (len, image_len * s, a, thick * s) } else { (len * s, image_len * s, a + len * (1.0 - s) / 2.0, thick * s) };
        let (x, y, w, h, image_w, image_h) =
            if horizontal { (a, band(t), len, t, image_len, t) } else { (band(t), a, t, len, t, image_len) };
        art_border::warm(art, part, tint, image_w.min(image_h));
        Item::Art { art, part, x, y, w, h, image_w, image_h, tint }
    };
    // A strip as long as the page (20 000 pixels in the zip): once, cut.
    if along >= span.max(thick * 4.0) {
        items.push(place(start, span, along, true));
        return;
    }
    // Repeated as many times as fit, stretched to fill the edge evenly,
    // each centred in its cell.
    if art_border::outer(art) {
        // Made smaller, they stay side by side (birds' band is unbroken in
        // Word, DPGB-00090): the cells shrink with them.
        let n = ((span / (cell * s)).round() as usize).max(1);
        let step = span / n as f32;
        let len = along * s * step / (cell * s);
        items.extend((0..n).map(|i| place(start + i as f32 * step + (step - len) / 2.0, len, len / s, true)));
        return;
    }
    // Some kinds Word sets farther apart than their proportions (the
    // picture keeps its length, the cell grows).
    let wide = cell * art_border::pitch_scale(art, sz);
    let n = ((span / wide).round() as usize).max(1);
    let step = span / n as f32;
    let len = along * step / wide;
    items.extend((0..n).map(|i| place(start + i as f32 * step + (step - len) / 2.0, len, len, false)));
}

/// The rectangles of border `b` along a cell's or paragraph's edge: at
/// `at` across, from `from` to `to` along, `horizontal` or not; `outer_first`
/// when the border's outer side is the top or left one. The border is
/// centred on the edge; its lines are extended by half its width at the
/// ends, so the corners meet.
/// The width (points) of the line above the notes: the short line is 2 inches, the full one the text's width; none for no line.
fn separator_width(kind: ibis_doc_model::NoteSeparator, text_width: f32) -> Option<f32> {
    match kind {
        ibis_doc_model::NoteSeparator::Standard => Some(144.0),
        ibis_doc_model::NoteSeparator::Full => Some(text_width),
        ibis_doc_model::NoteSeparator::Hidden => None,
    }
}

fn border_items(
    b: &FlowBorder,
    horizontal: bool,
    outer_first: bool,
    at: f32,
    from: f32,
    to: f32,
    extend_start: bool,
    extend_end: bool,
) -> Vec<Item> {
    let color = hsla(ibis_doc_model::Color::opaque(b.color));
    // Word draws the bands of an uneven double line in the same order on
    // every side, from the top or the left (measured on its PDF of DTBB:
    // thinThickSmallGap has its thick line first, top to bottom and left to
    // right, on all four sides; thickThinSmallGap its thin one), not from
    // the outer side.
    let bands = b.bands();
    let total = pt(bands.iter().map(|b| b.0).sum());
    let dashes = b.dashes().map(|d| d.into_iter().map(pt).collect::<Vec<f32>>());
    let mut out = Vec::new();
    let mut across = at - total / 2.0;
    let mut depth_from_top = 0.0f32;
    let mut drawn_so_far = 0usize;
    for (width, drawn) in bands {
        let t = pt(width).max(0.25);
        // The lines meet at the corners as nested rectangles do (a mitre): a band reaches past the edge's end by
        // half the border's width less its own distance from the border's outer side (an inner line stops at
        // the inner line of the border across).
        let depth = if outer_first { depth_from_top } else { (total - depth_from_top - pt(width)).max(0.0) };
        let reach = total / 2.0 - depth;
        depth_from_top += pt(width);
        let start = if extend_start { from - reach } else { from };
        let end = if extend_end { to + reach } else { to };
        if drawn {
            let mut segments = Vec::new();
            match &dashes {
                Some(pattern) if pattern.iter().sum::<f32>() > 0.1 => {
                    let mut a = start;
                    let mut i = 0;
                    while a < end {
                        let len = pattern[i % pattern.len()];
                        if i % 2 == 0 {
                            segments.push((a, (a + len).min(end)));
                        }
                        a += len;
                        i += 1;
                    }
                }
                _ => segments.push((start, end)),
            }
            // A wave is a sine line (Word's: 2.5 pt high, 5.75 pt long, measured on its PDF of DBRD); a double
            // wave is two of them, a half turn apart, one above the other.
            if matches!(b.style, ibis_doc_model::BorderStyle::Wave | ibis_doc_model::BorderStyle::DoubleWave) {
                let (line, amplitude) = (0.5f32, (t - 0.75) / 2.0);
                let length = 5.75f32;
                let centre = across + t / 2.0;
                let phase = if drawn_so_far % 2 == 1 { std::f32::consts::PI } else { 0.0 };
                drawn_so_far += 1;
                let steps = ((end - start) / length * 8.0).ceil().max(1.0) as usize;
                let point = |i: usize| {
                    let a = start + (end - start) * i as f32 / steps as f32;
                    (a, centre + amplitude * ((a - start) / length * std::f32::consts::TAU + phase).sin())
                };
                for i in 0..steps {
                    let (a0, c0) = point(i);
                    let (a1, c1) = point(i + 1);
                    out.push(if horizontal {
                        Item::Stroke { x: a0, y: c0, x2: a1, y2: c1, w: line, color }
                    } else {
                        Item::Stroke { x: c0, y: a0, x2: c1, y2: a1, w: line, color }
                    });
                }
                across += pt(width);
                continue;
            }
            for (a, z) in segments {
                out.push(if horizontal { Item::Rect { x: a, y: across, w: z - a, h: t, color } } else { Item::Rect { x: across, y: a, w: t, h: z - a, color } });
            }
        }
        across += pt(width);
    }
    out
}

fn offset(item: Item, dx: f32, dy: f32) -> Item {
    match item {
        Item::Text { x, y, text, offsets, font, size, color } => Item::Text { x: x + dx, y: y + dy, text, offsets, font, size, color },
        Item::Turned { x, y, text, font, size, color, turns } => Item::Turned { x: x + dx, y: y + dy, text, font, size, color, turns },
        Item::Rect { x, y, w, h, color } => Item::Rect { x: x + dx, y: y + dy, w, h, color },
        Item::Art { art, part, x, y, w, h, image_w, image_h, tint } => Item::Art { art, part, x: x + dx, y: y + dy, w, h, image_w, image_h, tint },
        Item::Picture { media, x, y, w, h } => Item::Picture { media, x: x + dx, y: y + dy, w, h },
        Item::Shape { shape, x, y, w, h } => Item::Shape { shape, x: x + dx, y: y + dy, w, h },
        Item::Line { para, x, y, h, stops, vertical } => Item::Line { para, x: x + dx, y: y + dy, h, stops, vertical },
        Item::Stroke { x, y, x2, y2, w, color } => Item::Stroke { x: x + dx, y: y + dy, x2: x2 + dx, y2: y2 + dy, w, color },
        Item::Shading { pattern, color, x, y, w, h } => Item::Shading { pattern, color, x: x + dx, y: y + dy, w, h },
        Item::CommentBubble { x, y, size } => Item::CommentBubble { x: x + dx, y: y + dy, size },
    }
}

/// The notes' numbers as shown: footnotes 1, 2, 3 and endnotes i, ii, iii
/// (Word's defaults), in the order their marks come in the body.
fn note_numbers(doc: &FlowDocument) -> std::collections::HashMap<(bool, u32), String> {
    // The notes' marks in body order, with the section each is in.
    fn walk(blocks: &[Block], section: usize, out: &mut Vec<(bool, u32, usize)>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        if let FlowRunKind::NoteRef { endnote, id } = r.kind
                            && !out.iter().any(|(e, i, _)| (*e, *i) == (endnote, id))
                        {
                            out.push((endnote, id, section));
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, section, out)),
            }
        }
    }
    let ids = ibis_doc_model::flow_command::sections(doc);
    let settings: Vec<(ibis_doc_model::NoteSettings, ibis_doc_model::NoteSettings)> = ids
        .iter()
        .map(|id| ibis_doc_model::flow_command::section_setup(doc, *id).map_or((doc.page.footnotes, doc.page.endnotes), |s| (s.page.footnotes, s.page.endnotes)))
        .collect();
    let mut order = Vec::new();
    let mut section = 0usize;
    for b in &doc.body {
        walk(std::slice::from_ref(b), section, &mut order);
        // (A section ends at a paragraph with a setup of its own, as
        // `flow_command::sections` counts them.)
        if let Block::Paragraph(p) = b
            && p.section.is_some()
        {
            section += 1;
        }
    }
    // Numbered from each kind's start, again at each section when asked
    // (again at each page is counted on, as continuous).
    let (mut f, mut e) = (0u32, 0u32);
    let (mut f_section, mut e_section) = (usize::MAX, usize::MAX);
    order
        .into_iter()
        .map(|(endnote, id, s)| {
            let (foot, end) = settings.get(s).or(settings.last()).copied().unwrap_or((doc.page.footnotes, doc.page.endnotes));
            let (set, count, seen) = if endnote { (end, &mut e, &mut e_section) } else { (foot, &mut f, &mut f_section) };
            if *seen == usize::MAX || (set.restart == ibis_doc_model::NoteRestart::EachSection && *seen != s) {
                *count = set.start.max(1) - 1;
            }
            *seen = s;
            *count += 1;
            ((endnote, id), set.format.number(*count))
        })
        .collect()
}

/// Which paragraphs lose their space before and after to
/// `w:contextualSpacing`: a paragraph with it, next to one of the same style
/// (in the same body, header, footer or cell).
fn contextual_spacing(doc: &FlowDocument) -> std::collections::HashMap<u32, (bool, bool)> {
    fn visit(blocks: &[Block], out: &mut std::collections::HashMap<u32, (bool, bool)>) {
        for (i, block) in blocks.iter().enumerate() {
            match block {
                Block::Paragraph(p) => {
                    if !p.contextual_spacing {
                        continue;
                    }
                    let same = |q: Option<&Block>| matches!(q, Some(Block::Paragraph(q)) if q.style == p.style);
                    let before = i > 0 && same(blocks.get(i - 1));
                    let after = same(blocks.get(i + 1));
                    if before || after {
                        out.insert(p.id, (before, after));
                    }
                }
                Block::Table(t) => {
                    for cell in t.rows.iter().flat_map(|r| &r.cells) {
                        visit(&cell.blocks, out);
                    }
                }
            }
        }
    }
    let mut out = std::collections::HashMap::new();
    for blocks in [&doc.body, &doc.header, &doc.footer, &doc.header_first, &doc.footer_first, &doc.header_even, &doc.footer_even] {
        visit(blocks, &mut out);
    }
    out
}

/// Whether any section or the document has footnotes restarting each page.
pub(crate) fn has_each_page_notes(doc: &FlowDocument) -> bool {
    if doc.page.footnotes.restart == ibis_doc_model::NoteRestart::EachPage {
        return true;
    }
    let ids = ibis_doc_model::flow_command::sections(doc);
    for id in ids {
        if let Some(s) = ibis_doc_model::flow_command::section_setup(doc, id) {
            if s.page.footnotes.restart == ibis_doc_model::NoteRestart::EachPage {
                return true;
            }
        }
    }
    false
}

/// The notes' numbers updated with the pages footnotes are on, restarting
/// each page where `EachPage` is set.
pub(crate) fn note_numbers_with_pages(
    doc: &FlowDocument,
    pages: &[Page],
) -> std::collections::HashMap<(bool, u32), String> {
    let ids = ibis_doc_model::flow_command::sections(doc);
    let settings: Vec<(ibis_doc_model::NoteSettings, ibis_doc_model::NoteSettings)> = ids
        .iter()
        .map(|id| ibis_doc_model::flow_command::section_setup(doc, *id).map_or((doc.page.footnotes, doc.page.endnotes), |s| (s.page.footnotes, s.page.endnotes)))
        .collect();

    let mut map = note_numbers(doc);
    for pg in pages {
        let (foot_set, _) = settings.get(pg.section).or(settings.last()).copied().unwrap_or((doc.page.footnotes, doc.page.endnotes));
        if foot_set.restart == ibis_doc_model::NoteRestart::EachPage {
            let mut count = foot_set.start.max(1) - 1;
            for &note_id in &pg.notes {
                count += 1;
                map.insert((false, note_id), foot_set.format.number(count));
            }
        }
    }
    map
}

/// Lay out `doc` into pages.
pub(crate) fn paginate(doc: &FlowDocument, res: &Resources) -> Pagination {
    // 差し込み印刷's 結果のプレビュー: the merge fields show a record's values.
    if let Some(values) = &res.merge_values {
        let merged = ibis_doc_model::merge::merged_view(doc, values);
        return paginate_collapsed(&merged, res);
    }
    paginate_collapsed(doc, res)
}

fn paginate_collapsed(doc: &FlowDocument, res: &Resources) -> Pagination {
    // Headings shown collapsed: what is under them is not laid out.
    let any_collapsed = !res.collapsed_headings.is_empty() || doc.body.iter().any(|b| matches!(b, Block::Paragraph(p) if p.collapsed));
    if any_collapsed
        && let Some(view) = super::outline_ops::collapsed_view(doc, &res.collapsed_headings)
    {
        return paginate_whole(&view, res);
    }
    paginate_whole(doc, res)
}

fn paginate_whole(doc: &FlowDocument, res: &Resources) -> Pagination {
    let ids = ibis_doc_model::flow_command::sections(doc);
    let first_pitch = ids.first().and_then(|id| ibis_doc_model::flow_command::section_setup(doc, *id)).and_then(|s| s.page.line_pitch);
    let lay_out = |known_pages: Option<(u32, Vec<u32>)>, known_notes: Option<std::collections::HashMap<(bool, u32), String>>, seeds: Vec<RaisedShape>| {
        let layouter = Layouter {
            res,
            doc,
            labels: ibis_doc_model::list_labels(doc),
            page_format: std::cell::RefCell::new(doc.page_number_format.clone()),
            chapter_prefix: std::cell::RefCell::new(String::new()),
            line_pitch: std::cell::Cell::new(first_pitch),
            line_room: std::cell::Cell::new(f32::MAX),
            page_counts: std::cell::Cell::new(None),
            known_pages,
            body_section: std::cell::Cell::new(0),
            note_numbers: known_notes.unwrap_or_else(|| note_numbers(doc)),
            current_note: std::cell::RefCell::new(String::new()),
            contextual: contextual_spacing(doc),
            deferred_shapes: std::cell::RefCell::new(seeds.iter().map(|s| s.3).collect()),
            column_dx: std::cell::Cell::new(0.0),
            raised: Default::default(),
            seeds,
            current_cell: std::cell::Cell::new(None),
            field_paragraph: std::cell::Cell::new(None),
            comment_runs: map_comment_runs(doc),
        };
        let mut pagination = layouter.lay_out();
        layouter.grid_text_boxes(&mut pagination);
        (pagination, layouter.raised.take())
    };
    let (first, raised) = lay_out(None, None, Vec::new());
    // Shapes placed above their paragraphs, over text laid out before them:
    // again, with them there from their pages' starts (Word lays such a page
    // out again round them).
    let (first, seeds) = if raised.is_empty() { (first, raised) } else { (lay_out(None, None, raised.clone()).0, raised) };
    let lay_out = |known_pages, known_notes| lay_out(known_pages, known_notes, seeds.clone()).0;
    let need_page_counts = doc.body.iter().any(has_page_counts);
    let need_each_page_notes = has_each_page_notes(doc);
    if !need_page_counts && !need_each_page_notes {
        return first;
    }

    let notes = if need_each_page_notes {
        Some(note_numbers_with_pages(doc, &first.pages))
    } else {
        None
    };

    let counts = if need_page_counts {
        let sections = first.pages.iter().map(|p| p.section + 1).max().unwrap_or(1);
        let per: Vec<u32> = (0..sections).map(|s| first.pages.iter().filter(|p| p.section == s).count() as u32).collect();
        Some((first.pages.len() as u32, per))
    } else {
        None
    };

    lay_out(counts, notes)
}

/// Whether a block has `NUMPAGES` or `SECTIONPAGES` fields (or formulas
/// over them).
fn has_page_counts(block: &Block) -> bool {
    match block {
        Block::Paragraph(p) => p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if instruction.contains("NUMPAGES") || instruction.contains("SECTIONPAGES"))),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(has_page_counts),
    }
}

thread_local! {
    /// Pagination of the documents open, by the document (held so its
    /// address is not reused).
    static PAGINATIONS: std::cell::RefCell<Vec<(Arc<FlowDocument>, Arc<Pagination>, u64, bool, bool, bool, Arc<std::collections::BTreeSet<u32>>, Option<Arc<std::collections::HashMap<String, String>>>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The pages of `doc`, laid out once.
pub(crate) fn pages(doc: &Arc<FlowDocument>, res: &Resources) -> Arc<Pagination> {
    PAGINATIONS.with(|cache| {
        let mut cache = cache.borrow_mut();
        // Laid out again when art borders' pictures have come since (the
        // web version fetches them).
        let generation = crate::docx::art_border::generation();
        cache.retain(|(_, _, g, _, _, _, _, _)| *g == generation);
        if let Some((_, p, _, _, _, _, _, _)) = cache.iter().find(|(d, _, _, pr, ph, pm, co, mv)| Arc::ptr_eq(d, doc) && *pr == res.print && *ph == res.print_hidden_text && *pm == res.print_markup && **co == *res.collapsed_headings && *mv == res.merge_values) {
            return p.clone();
        }
        let started = crate::debug_timing::start();
        let mut p = paginate(doc, res);
        add_watermark(doc, &mut p);
        let p = Arc::new(p);
        crate::debug_timing::report(&format!("paginate ({} pages)", p.pages.len()), started);
        if cache.len() >= 4 {
            cache.remove(0);
        }
        cache.push((doc.clone(), p.clone(), generation, res.print, res.print_hidden_text, res.print_markup, res.collapsed_headings.clone(), res.merge_values.clone()));
        p
    })
}

/// 透かし (fix): the watermark behind every page's text, centred on the
/// margins, as Word draws its WordArt (a semi-transparent one drawn half
/// way to white, as on white paper).
fn add_watermark(doc: &FlowDocument, pagination: &mut Pagination) {
    if doc.watermark.is_none() && doc.section_watermarks.is_empty() {
        return;
    }
    let ids = ibis_doc_model::flow_command::sections(doc);
    let m = &doc.page;
    for page in &mut pagination.pages {
        let own = ids.get(page.section).and_then(|id| doc.section_watermarks.iter().find(|(s, _)| s == id)).map(|(_, w)| w.as_ref());
        let Some(w) = own.unwrap_or(doc.watermark.as_ref()) else { continue };
        if w.text.trim().is_empty() {
            continue;
        }
        let (left, top) = (pt(m.left + m.gutter), pt(m.top));
        let area = (page.width - left - pt(m.right), page.height - top - pt(m.bottom));
        let (sw, sh) = w.size_in(area.0, area.1);
        // Word stretches the text's ink to fill the shape (fitshape). One
        // size here: the largest whose ink fits; wide (CJK) characters each
        // in their own share of the width, as Word's stretched ones sit.
        let chars: Vec<char> = w.text.chars().collect();
        let wide = chars.iter().all(|&c| (c as u32) >= 0x2E80);
        let ink = if wide { 0.88 } else if chars.iter().all(|c| !c.is_lowercase()) { 0.66 } else { 0.75 };
        let ems = chars.iter().map(|&c| if (c as u32) >= 0x2E80 { 1.0 } else if c.is_ascii_uppercase() { 0.56 } else { 0.48 }).sum::<f32>().max(1.0);
        let pieces: Vec<String> = if wide && chars.len() > 1 { chars.iter().map(|c| c.to_string()).collect() } else { vec![w.text.clone()] };
        let slot = sw / pieces.len() as f32;
        let size = if pieces.len() > 1 { (sh / ink).min(slot) } else { (sw / ems).min(sh / ink) };
        let color = if w.semitransparent {
            let half = |c: u32| (c + 255) / 2;
            let c = w.color.0;
            Rgb(half((c >> 16) & 0xFF) << 16 | half((c >> 8) & 0xFF) << 8 | half(c & 0xFF))
        } else {
            w.color
        };
        let turn = w.rotation.rem_euclid(360);
        let (sin, cos) = (turn as f32).to_radians().sin_cos();
        let centre = (left + area.0 / 2.0, top + area.1 / 2.0);
        let mut items = Vec::new();
        for (i, piece) in pieces.iter().enumerate() {
            let style = ibis_doc_model::TextStyle { font_size: (size * 100.0) as u32, color, font: (!w.font.is_empty()).then(|| w.font.clone()), ..Default::default() };
            let mut body = ibis_doc_model::TextBody::plain(piece, style);
            body.paragraphs.iter_mut().for_each(|p| p.align = Some(ibis_doc_model::TextAlign::Center));
            body.anchor = ibis_doc_model::VerticalAnchor::Middle;
            // The ink's middle, not the line's: CJK glyphs sit low in the
            // line (Latin capitals about at its middle).
            let down = if wide { 0.2 } else { 0.0 };
            body.insets = ibis_doc_model::Insets { left: 0, top: (size * down * EMU_PER_POINT as f32) as i64, right: 0, bottom: 0 };
            body.wrap = false;
            let emu = |v: f32| (v * EMU_PER_POINT as f32).round() as i64;
            let mut shape = ibis_doc_model::Shape::new(ibis_doc_model::ShapeId(0), ibis_doc_model::EmuRect::new(0, 0, emu(slot), emu(sh)), ibis_doc_model::ShapeKind::TextBox(body));
            shape.rotation = turn * 60_000;
            shape.floating = Some(ibis_doc_model::FloatingAnchor {
                offset_x: 0,
                offset_y: 0,
                align_h: Some("center".into()),
                align_v: Some("center".into()),
                relative_h: "margin".into(),
                relative_v: "margin".into(),
                wrap: "none".into(),
                behind_doc: true,
                dist_t: 0,
                dist_b: 0,
                dist_l: 0,
                dist_r: 0,
                wrap_text: None,
                locked: false,
                allow_overlap: true,
                layout_in_cell: true,
                relative_height: ibis_doc_model::FloatingAnchor::DEFAULT_RELATIVE_HEIGHT,
                wrap_polygon: None,
                glue: Default::default(),
            });
            // The piece's centre along the turned text.
            let along = (i as f32 + 0.5) * slot - sw / 2.0;
            let (cx, cy) = (centre.0 + along * cos, centre.1 + along * sin);
            items.push(Item::Shape { shape: Box::new(shape), x: cx - slot / 2.0, y: cy - sh / 2.0, w: slot, h: sh });
        }
        page.items.splice(0..0, items);
    }
}

/// A document's captions' numbers, cross-references and the like brought up to date as it is opened or exported (Word
/// does so when it opens the document: a figure added at the front is not 1-1 twice). Not an edit: no undo step.
pub(crate) fn update_fields_on_open(presentation: &mut ibis_doc_model::Presentation, res: &Resources) {
    let Some(doc) = presentation.flow.as_mut() else { return };
    let commands = ibis_doc_model::flow_command::updated_fields(doc);
    if !commands.is_empty() {
        let doc = Arc::make_mut(doc);
        for command in commands {
            let _ = ibis_doc_model::flow_command::apply_to(doc, command);
        }
    }
    // The index (INDEX) of the XE fields: with the pages they are laid out on, twice (the index moves the pages after it).
    for _ in 0..2 {
        let Some((first, instruction)) = super::index_fields::index_field(doc) else { break };
        let pagination = pages(doc, res);
        let mut numbers: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for page in &pagination.pages {
            for item in &page.items {
                if let Item::Line { para, .. } = item {
                    numbers.entry(*para).or_insert(page.number);
                }
            }
        }
        let page_of = |id: u32| numbers.get(&id).copied();
        let commands = super::index_fields::index_commands(doc, first, true, &instruction, &page_of);
        let doc = Arc::make_mut(doc);
        for command in commands {
            let _ = ibis_doc_model::flow_command::apply_to(doc, command);
        }
    }
}

/// Give a presentation read from a flowing document one slide per page.
/// `file_name` is shown by `FILENAME` fields.
pub(crate) fn expand_pages(presentation: &mut ibis_doc_model::Presentation, res: &Resources, file_name: Option<&str>) {
    let Some(mut doc) = presentation.flow.take() else { return };
    if file_name.is_some() {
        Arc::make_mut(&mut doc).file_name = file_name.map(str::to_string);
    }
    presentation.flow = Some(doc.clone());
    let pagination = pages(&doc, res);
    let count = pagination.pages.len().max(1);
    // The pages are the paper's size (after a page setup, too): each its
    // section's.
    let size_of = |i: usize| {
        pagination.pages.get(i).filter(|p| p.width > 0.0).map_or(doc.page.size, |p| ibis_doc_model::EmuSize::new((p.width * EMU_PER_POINT as f32).round() as i64, (p.height * EMU_PER_POINT as f32).round() as i64))
    };
    presentation.slide_size = size_of(0);
    let mut slides = Vec::with_capacity(count);
    for index in 0..count {
        let id = presentation.new_slide_id();
        let mut slide = ibis_doc_model::Slide::new(id);
        let size = size_of(index);
        if size != presentation.slide_size {
            slide.size = Some(size);
        }
        let shape = presentation.new_shape_id();
        slide.shapes.push(ibis_doc_model::Shape {
            id: shape,
            frame: ibis_doc_model::EmuRect::new(0, 0, size.width, size.height),
            rotation: 0,
            flip_h: false,
            flip_v: false,
            kind: ibis_doc_model::ShapeKind::Page { document: doc.clone(), index },
            floating: None,
        });
        slides.push(slide);
    }
    presentation.slides = slides;
}


/// The rectangles of an underline `w` long from `x`, under text of `size`
/// points whose baseline is at `baseline` (measured on Word's PDF: a single
/// line 0.05 em + 0.25 pt thick, its top 0.1125 em + 0.5 pt below the
/// baseline; heavy ones 0.025 em + 1.25 pt from 0.0875 em + 0.25 pt).
fn underline_items(style: UnderlineStyle, size: f32, x: f32, w: f32, baseline: f32, color: Hsla) -> Vec<Item> {
    use UnderlineStyle::*;
    let (top, thick) = if style.heavy() || style == Double || style == WavyDouble { (0.0875 * size + 0.25, 0.025 * size + 1.25) } else { (0.1367 * size, 0.0684 * size) };
    if matches!(style, Wave | WavyHeavy | WavyDouble) {
        // Word's reference PDF uses roughly 0.24 pt strokes (0.96 pt for
        // heavy), with 1.2 pt half-waves. Flatten smooth cubic half-waves
        // to the existing Stroke primitive so screen and export agree.
        let stroke = if style == WavyHeavy { 0.96 } else { 0.24 };
        let amplitude = if style == WavyDouble { 0.24 } else { 0.72 };
        let bands = if style == WavyDouble { 2 } else { 1 };
        let mut out = Vec::new();
        for band in 0..bands {
            let y0 = baseline + top + stroke / 2.0 + band as f32 * (0.024 * size + 0.24);
            let mut a = 0.0;
            let mut half = 0;
            while a < w {
                let (from, to) = if half % 2 == 0 { (0.0, amplitude) } else { (amplitude, 0.0) };
                let mut prev = (a, from);
                for step in 1..=8 {
                    let t = step as f32 / 8.0;
                    let u = 1.0 - t;
                    let dx = 3.0*u*u*t*0.48 + 3.0*u*t*t*0.48 + t*t*t*1.2;
                    let dy = u*u*u*from + 3.0*u*u*t*from + 3.0*u*t*t*to + t*t*t*to;
                    let end = (a + dx).min(w);
                    let fraction = if a + dx > w { (w-prev.0)/(a+dx-prev.0) } else { 1.0 };
                    let y = prev.1 + (dy-prev.1)*fraction;
                    out.push(Item::Stroke { x: x+prev.0, y: y0+prev.1, x2: x+end, y2: y0+y, w: stroke, color });
                    prev = (a+dx, dy);
                    if end >= w { break; }
                }
                a += 1.2;
                half += 1;
            }
        }
        return out;
    }
    // Bands across: (offset from `top`, thickness).
    let bands: Vec<(f32, f32)> = match style {
        Double => {
            let t = 0.025 * size + 0.25;
            vec![(0.0, t), (t + 1.0, t)]
        }
        _ => vec![(0.0, thick)],
    };
    let mut out = Vec::new();
    for (dy, t) in bands {
        let y = baseline + top + dy;
        match style.dashes() {
            Some(pattern) => {
                let (mut a, mut i) = (x, 0);
                while a < x + w {
                    let len = pattern[i % pattern.len()];
                    if i % 2 == 0 {
                        out.push(Item::Rect { x: a, y, w: len.min(x + w - a), h: t, color });
                    }
                    a += len;
                    i += 1;
                }
            }
            None => out.push(Item::Rect { x, y, w, h: t, color }),
        }
    }
    out
}


/// The items drawing a shading: background fill and two-color pattern
/// lines/stripes across rectangle `(x, y, w, h)`.
fn shading_items(
    pattern: Option<ShadingPattern>,
    fill: Option<Rgb>,
    pat_color: Option<Rgb>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Vec<Item> {
    let mut items = Vec::new();
    if fill.is_none() && pattern.is_none() {
        return items;
    }
    // 1. Background fill
    if let Some(f) = fill {
        items.push(Item::Rect { x, y, w, h, color: hsla(ibis_doc_model::Color::opaque(f)) });
    }
    // 2. Pattern placeholder (expanded into absolute page coordinates before rendering)
    if let Some(pat) = pattern {
        let c = pat_color.unwrap_or(Rgb::BLACK);
        items.push(Item::Shading {
            pattern: pat,
            color: hsla(ibis_doc_model::Color::opaque(c)),
            x,
            y,
            w,
            h,
        });
    }
    items
}

/// Expand two-color shading pattern across absolute page rectangle `(x, y, w, h)`.
///
/// Word's own pattern (read from its PDFs of the coverage tests DCEL and
/// DCMB): a tiling pattern of 8 × 8 pixels of 0.24 pt (300 dpi), so it
/// repeats every 1.92 pt, on a grid of 1.92 pt from the page's top left
/// corner (the tile holding the rectangle's top left corner). Drawn here
/// every 2 pt instead: the reference PNGs of the tests are made by pdftoppm
/// at 144 dpi, which draws that tile every 4 px (2 pt), and drawing every
/// 1.92 pt made more of them differ (tried 2026-10-05: 5 newly passing,
/// 33 newly failing). Where pdftoppm starts the stripes does not follow a
/// simple rule from the tile's origin, so some cells still differ in phase
/// (DCMB-01342 and 9 others); that is not copied here.
fn pattern_items(
    pat: ShadingPattern,
    pcolor: Hsla,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Vec<Item> {
    use ShadingPattern::*;
    let mut items = Vec::new();
    let step = 2.0_f32; // 4px at 144 DPI
    match pat {
        ThinHorzStripe => {
            // In Word 144 DPI: thinHorzStripe covers y%4 == 3 (height 0.5pt spanning [-0.5, 0.0] before 2.0k).
            let k_start = ((y + 0.5) / step).floor() as i64;
            let mut k = k_start;
            loop {
                let y_stroke = k as f32 * step - 0.5;
                if y_stroke >= y + h {
                    break;
                }
                let top = y_stroke.max(y);
                let bottom = (y_stroke + 0.5).min(y + h);
                if bottom > top {
                    items.push(Item::Rect { x, y: top, w, h: bottom - top, color: pcolor });
                }
                k += 1;
            }
        }
        HorzStripe => {
            let mut y0 = y;
            while y0 < y + h {
                let lh = 1.0_f32.min(y + h - y0);
                items.push(Item::Rect { x, y: y0, w, h: lh, color: pcolor });
                y0 += step;
            }
        }
        ThinVertStripe => {
            // In Word 144 DPI: thinVertStripe covers x%4 == 0 (width 0.5pt spanning [0.0, 0.5] after 2.0k).
            let k_start = (x / step).floor() as i64;
            let mut k = k_start;
            loop {
                let x_stroke = k as f32 * step;
                if x_stroke >= x + w {
                    break;
                }
                let left = x_stroke.max(x);
                let right = (x_stroke + 0.5).min(x + w);
                if right > left {
                    items.push(Item::Rect { x: left, y, w: right - left, h, color: pcolor });
                }
                k += 1;
            }
        }
        VertStripe => {
            let mut x0 = x;
            while x0 < x + w {
                let lw = 1.0_f32.min(x + w - x0);
                items.push(Item::Rect { x: x0, y, w: lw, h, color: pcolor });
                x0 += step;
            }
        }
        ThinHorzCross => {
            // Horizontal stroke at y%4 == 1 (k*step + 0.5) and vertical stroke at x%4 == 3 (k*step + 1.5).
            let k_y_start = ((y - 0.5) / step).floor() as i64;
            let mut ky = k_y_start;
            loop {
                let y_stroke = ky as f32 * step + 0.5;
                if y_stroke >= y + h { break; }
                let top = y_stroke.max(y);
                let bottom = (y_stroke + 0.5).min(y + h);
                if bottom > top {
                    items.push(Item::Rect { x, y: top, w, h: bottom - top, color: pcolor });
                }
                ky += 1;
            }
            let k_x_start = ((x - 1.5) / step).floor() as i64;
            let mut kx = k_x_start;
            loop {
                let x_stroke = kx as f32 * step + 1.5;
                if x_stroke >= x + w { break; }
                let left = x_stroke.max(x);
                let right = (x_stroke + 0.5).min(x + w);
                if right > left {
                    items.push(Item::Rect { x: left, y, w: right - left, h, color: pcolor });
                }
                kx += 1;
            }
        }
        HorzCross => {
            items.extend(pattern_items(HorzStripe, pcolor, x, y, w, h));
            items.extend(pattern_items(VertStripe, pcolor, x, y, w, h));
        }
        // `\` from top left to bottom right: reverseDiagStripe (Word's PDF:
        // row r of its tile dark at columns r..r+3).
        ReverseDiagStripe | ThinReverseDiagStripe => {
            let thin = matches!(pat, ThinReverseDiagStripe);
            let diag_stroke: f32 = if thin { 0.36 } else { 0.71 };
            let mut d = -h;
            while d <= w {
                let (x1, y1) = if d >= 0.0 { (x + d, y) } else { (x, y - d) };
                let (x2, y2) = if d + h <= w { (x + d + h, y + h) } else { (x + w, y + w - d) };
                if (x2 - x1).abs() > 0.01 || (y2 - y1).abs() > 0.01 {
                    items.push(Item::Stroke { x: x1, y: y1, x2, y2, w: diag_stroke, color: pcolor });
                }
                d += step;
            }
        }
        // `/` from bottom left to top right: diagStripe (Word's PDF: row r of
        // its tile dark at columns 4−r..7−r).
        DiagStripe | ThinDiagStripe => {
            let thin = matches!(pat, ThinDiagStripe);
            let diag_stroke: f32 = if thin { 0.36 } else { 0.71 };
            let mut d = 0.0;
            while d <= w + h {
                let (x1, y1) = if d <= h { (x, y + d) } else { (x + d - h, y + h) };
                let (x2, y2) = if d <= w { (x + d, y) } else { (x + w, y + d - w) };
                if (x2 - x1).abs() > 0.01 || (y2 - y1).abs() > 0.01 {
                    items.push(Item::Stroke { x: x1, y: y1, x2, y2, w: diag_stroke, color: pcolor });
                }
                d += step;
            }
        }
        DiagCross | ThinDiagCross => {
            let thin = matches!(pat, ThinDiagCross);
            let dpat = if thin { ThinDiagStripe } else { DiagStripe };
            let rpat = if thin { ThinReverseDiagStripe } else { ReverseDiagStripe };
            items.extend(pattern_items(dpat, pcolor, x, y, w, h));
            items.extend(pattern_items(rpat, pcolor, x, y, w, h));
        }
    }
    items
}

/// The cell of a row that ends where column `col` starts.
fn cells_left_of(cells: &[impl HasSpan], col: usize) -> Option<usize> {
    cells.iter().position(|l| l.col_end() == col)
}

trait HasSpan {
    fn col_end(&self) -> usize;
}

/// A border's weight where two meet (ECMA-376 §17.4.66): its width in
/// eighths of a point times the style's number in `ST_Border`.
fn weight(b: Option<FlowBorder>) -> i64 {
    use ibis_doc_model::BorderStyle::*;
    let Some(b) = b else { return 0 };
    let number = match b.style {
        Single => 1,
        Thick => 2,
        Double => 3,
        Dotted => 4,
        Dashed => 5,
        DotDash => 6,
        DotDotDash => 7,
        Triple => 8,
        ThinThickSmallGap => 9,
        ThickThinSmallGap => 10,
        ThinThickThinSmallGap => 11,
        ThinThickMediumGap => 12,
        ThickThinMediumGap => 13,
        ThinThickThinMediumGap => 14,
        ThinThickLargeGap => 15,
        ThickThinLargeGap => 16,
        ThinThickThinLargeGap => 17,
        Wave => 18,
        DoubleWave => 19,
        DashSmallGap => 20,
        DashDotStroked => 21,
        ThreeDEmboss => 22,
        ThreeDEngrave => 23,
        Outset => 24,
        Inset => 25,
    };
    (b.width * 8 / 12_700).max(1) * number
}


/// A section of vertical writing (縦書き, `w:textDirection` `tbRl`; or `btLr`)
/// is laid out as across text on a page turned a quarter (its width the
/// paper's height, its margins moved round with it), then each page's items
/// are turned back into the paper `(w, h)` (points): the characters of
/// `tbRl` upright when Japanese and turned clockwise otherwise (as a
/// vertical cell's text, `turn_cell_text`), those of `btLr` all turned
/// anticlockwise.
fn turn_page_items(items: Vec<Item>, direction: ibis_doc_model::CellDirection, w: f32, h: f32) -> Vec<Item> {
    use ibis_doc_model::CellDirection;
    let tb = direction == CellDirection::TbRl;
    // A point of the across page in the paper.
    let at = |x: f32, y: f32| if tb { (w - y, x) } else { (y, h - x) };
    // A rectangle's corner nearest the paper's top-left, and its size.
    let rect = |x: f32, y: f32, rw: f32, rh: f32| {
        let (ax, ay) = at(x, y);
        let (bx, by) = at(x + rw, y + rh);
        (ax.min(bx), ay.min(by), rh, rw)
    };
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        match it {
            Item::Text { .. } => out.extend(turn_cell_text(&[it], direction, CellAnchor::Top, (0.0, 0.0, w, h), w)),
            Item::Rect { x, y, w: rw, h: rh, color } => {
                let (x, y, w, h) = rect(x, y, rw, rh);
                out.push(Item::Rect { x, y, w, h, color });
            }
            Item::Stroke { x, y, x2, y2, w: sw, color } => {
                let ((x, y), (x2, y2)) = (at(x, y), at(x2, y2));
                out.push(Item::Stroke { x, y, x2, y2, w: sw, color });
            }
            Item::Picture { media, x, y, w: pw, h: ph } => {
                let (cx, cy) = at(x + pw / 2.0, y + ph / 2.0);
                out.push(Item::Picture { media, x: cx - pw / 2.0, y: cy - ph / 2.0, w: pw, h: ph });
            }
            Item::Shape { shape, x, y, w: sw, h: sh } => {
                let (cx, cy) = at(x + sw / 2.0, y + sh / 2.0);
                out.push(Item::Shape { shape, x: cx - sw / 2.0, y: cy - sh / 2.0, w: sw, h: sh });
            }
            Item::Shading { pattern, color, x, y, w: sw, h: sh } => {
                let (x, y, w, h) = rect(x, y, sw, sh);
                out.push(Item::Shading { pattern, color, x, y, w, h });
            }
            Item::Turned { x, y, text, font, size, color, turns } => {
                let (ax, ay) = at(x, y);
                let new_turns = if tb { turns + 1 } else { turns - 1 };
                if new_turns == 0 {
                    out.push(Item::Text { x: ax, y: ay, text, offsets: Arc::from([0.0f32]), font, size, color });
                } else {
                    out.push(Item::Turned { x: ax, y: ay, text, font, size, color, turns: new_turns });
                }
            }
            Item::Line { para, x, y, h: lh, stops, .. } => {
                if tb {
                    out.push(Item::Line { para, x: w - y - lh, y: x, h: lh, stops, vertical: true });
                } else {
                    out.push(Item::Line { para, x: y, y: h - x - lh, h: lh, stops, vertical: false });
                }
            }
            other => out.push(other),
        }
    }
    out
}
