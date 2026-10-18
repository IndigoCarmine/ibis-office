// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! タイムライン (timelines): a bar of periods (years, quarters, months or days) over a date field of pivot tables;
//! the periods selected keep only the dates within them (MS-XLSX `x15:timeline`, `x15:timelineCacheDefinition`, and
//! MS-ODRAWXML `tsle:timeslicer` for the drawing).
//!
//! A timeline is a floating shape on a sheet ([`crate::ShapeKind::Timeline`]); it keeps its selection, and the
//! pivot tables it is on show the items within it.

/// A timeline (`x15:timeline`) and its cache's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timeline {
    /// Its name (`name`), the one the drawing names too.
    pub name: String,
    /// The heading (`caption`).
    pub caption: String,
    /// Its cache's name (`cache`).
    pub cache: String,
    /// The date field's (column's) name (`sourceName` of the cache).
    pub source: String,
    /// The pivot tables it filters (by name).
    pub pivots: Vec<String>,
    /// The periods shown: 0 years, 1 quarters, 2 months, 3 days (`level`).
    pub level: u8,
    /// The periods a selection is made of (`selectionLevel`).
    pub selection_level: u8,
    /// The first date shown (`scrollPosition`), as a serial number (whole days).
    pub scroll: i64,
    /// The first and the last date of the data (`bounds`), as serial numbers; the last is the start of the day after.
    pub bounds: (i64, i64),
    /// The dates kept: the first, and the day after the last; `None` keeps all.
    pub selection: Option<(i64, i64)>,
    pub show_header: bool,
    pub show_selection_label: bool,
    pub show_time_level: bool,
    pub show_scroll_bar: bool,
    /// Timeline style name (e.g. `"TimeSlicerStyleLight1"`).
    pub style: String,
    /// The parts of the file (the timeline's and its cache's), for saving what changed.
    pub part: String,
    pub cache_part: String,
}

/// A period of the bar: a year, a quarter, a month or a day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Period {
    pub year: i64,
    /// The quarter (1 to 4), the month (1 to 12) or the day of the month; 1 for a year.
    pub index: u32,
    /// The first day, and the day after the last, as serial numbers.
    pub start: i64,
    pub end: i64,
}

pub const LEVELS: u8 = 4;

fn civil(serial: i64) -> (i64, u32, u32) {
    ibis_calc::eval::convert::civil_from_days(serial - 25_569)
}

fn serial_of(y: i64, m: i64, d: i64) -> i64 {
    ibis_calc::eval::convert::days_from_civil(y, m, d) + 25_569
}

impl Timeline {
    /// The periods from the one holding `from` to the one holding the last of the bounds.
    pub fn periods(&self, level: u8) -> Vec<Period> {
        let (first, last) = (self.bounds.0, (self.bounds.1 - 1).max(self.bounds.0));
        let (y0, m0, d0) = civil(first);
        let (y1, m1, _) = civil(last);
        let mut out = Vec::new();
        match level {
            0 => {
                for y in y0..=y1 {
                    out.push(Period { year: y, index: 1, start: serial_of(y, 1, 1), end: serial_of(y + 1, 1, 1) });
                }
            }
            1 | 2 => {
                let step = if level == 1 { 3 } else { 1 };
                let (mut y, mut m) = (y0, ((m0 as i64 - 1) / step) * step + 1);
                while (y, m) <= (y1, m1 as i64) {
                    let (ny, nm) = if m + step > 12 { (y + 1, m + step - 12) } else { (y, m + step) };
                    out.push(Period { year: y, index: if level == 1 { (m as u32 - 1) / 3 + 1 } else { m as u32 }, start: serial_of(y, m, 1), end: serial_of(ny, nm, 1) });
                    (y, m) = (ny, nm);
                }
            }
            _ => {
                let _ = d0;
                for day in first..=last {
                    let (y, _, d) = civil(day);
                    out.push(Period { year: y, index: d, start: day, end: day + 1 });
                }
            }
        }
        out
    }

    /// Whether the dates of `p` are all within the selection (all are, with none).
    pub fn selected(&self, p: &Period) -> bool {
        match self.selection {
            None => true,
            Some((a, b)) => p.start >= a && p.end <= b,
        }
    }

    /// Whether the date `serial` is kept.
    pub fn keeps(&self, serial: f64) -> bool {
        match self.selection {
            None => true,
            Some((a, b)) => serial >= a as f64 && serial < b as f64,
        }
    }

    /// The selection after a press on period `index` (of the periods at the selection's level): that period alone, or from
    /// the first selected one to it with `extend`; a press on the only period selected shows all again.
    pub fn select_after_click(&self, periods: &[Period], index: usize, extend: bool) -> Option<(i64, i64)> {
        let p = periods.get(index)?;
        let now: Vec<usize> = periods.iter().enumerate().filter(|(_, q)| self.selected(q) && self.selection.is_some()).map(|(i, _)| i).collect();
        if extend && !now.is_empty() {
            let (a, b) = (now[0].min(index), now[now.len() - 1].max(index));
            return Some((periods[a].start, periods[b].end));
        }
        if now.len() == 1 && now[0] == index {
            return None;
        }
        Some((p.start, p.end))
    }
}

// The layout, in EMU (points ×12700) from the shape's top left.
const PT: i64 = 12_700;
pub const CAPTION: i64 = 24 * PT;
pub const SUB: i64 = 22 * PT;
pub const GROUP: i64 = 18 * PT;
pub const CELL_WIDTH: i64 = 46 * PT;
pub const SCROLL: i64 = 14 * PT;

/// What a press on a timeline is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineHit {
    /// The selection label's clear icon.
    Clear,
    /// The level choice (the next level of the periods).
    Level,
    /// A period of the bar (an index of the periods at the selection level).
    Period(usize),
    /// Left grip of the selected period range.
    SelectionStart,
    /// Right grip of the selected period range.
    SelectionEnd,
    /// The scroll arrows: one period earlier (`-1`) or later (`1`).
    Scroll(i64),
    /// The scrollbar thumb.
    ScrollThumb,
}

/// The index of the first period shown (the one holding `scroll`).
pub fn first_shown(periods: &[Period], scroll: i64) -> usize {
    periods.iter().position(|p| p.end > scroll).unwrap_or(periods.len().saturating_sub(1))
}

/// How many periods are shown across a timeline `width` wide.
pub fn shown_count(width: i64) -> usize {
    ((width - 2 * SCROLL).max(CELL_WIDTH) / CELL_WIDTH).max(1) as usize
}

/// Horizontal bounds `(x0, x1)` of the scrollbar thumb in EMU relative to the timeline left edge.
pub fn thumb_rect(t: &Timeline, periods: &[Period], width: i64) -> (i64, i64) {
    let track = (width - 2 * SCROLL).max(16 * PT);
    let shown = shown_count(width);
    let total = periods.len().max(1);
    let first = first_shown(periods, t.scroll);
    let thumb_w = ((track * shown as i64) / total as i64).clamp(16 * PT, track);
    let max_first = total.saturating_sub(shown).max(1) as i64;
    let thumb_x = SCROLL + ((track - thumb_w) * (first as i64).min(max_first)) / max_first;
    (thumb_x, thumb_x + thumb_w)
}

/// Period index at horizontal offset `x` (clamped to `0..periods.len()`).
pub fn period_at_x(t: &Timeline, periods: &[Period], width: i64, x: i64) -> Option<usize> {
    if periods.is_empty() {
        return None;
    }
    let first = first_shown(periods, t.scroll);
    let shown = shown_count(width);
    let col = ((x - SCROLL).max(0) / CELL_WIDTH) as usize;
    let idx = (first + col.min(shown.saturating_sub(1))).min(periods.len() - 1);
    Some(idx)
}

/// Scroll serial position when dragging the scrollbar thumb to horizontal offset `x`.
pub fn scroll_at_thumb_x(_t: &Timeline, periods: &[Period], width: i64, x: i64) -> i64 {
    if periods.is_empty() {
        return 0;
    }
    let track = (width - 2 * SCROLL).max(16 * PT);
    let shown = shown_count(width);
    let total = periods.len().max(1);
    let max_first = total.saturating_sub(shown);
    if max_first == 0 {
        return periods[0].start;
    }
    let thumb_w = ((track * shown as i64) / total as i64).clamp(16 * PT, track);
    let avail = (track - thumb_w).max(1);
    let pos = (x - SCROLL - thumb_w / 2).clamp(0, avail);
    let idx = ((pos * max_first as i64 + avail / 2) / avail) as usize;
    periods[idx.min(periods.len() - 1)].start
}

pub fn hit(t: &Timeline, width: i64, height: i64, x: i64, y: i64) -> Option<TimelineHit> {
    if x < 0 || y < 0 || x > width || y > height {
        return None;
    }
    let header = if t.show_header { CAPTION } else { 0 };
    let sub = if t.show_selection_label || t.show_time_level { SUB } else { 0 };
    if y >= header && y < header + sub {
        if x > width - 70 * PT && t.show_time_level {
            return Some(TimelineHit::Level);
        }
        if t.selection.is_some() && x > width - 90 * PT {
            return Some(TimelineHit::Clear);
        }
        return None;
    }
    let body_top = header + sub + GROUP;
    let periods = t.periods(t.selection_level.min(t.level.max(t.selection_level)));
    if y >= body_top && y < height - SCROLL {
        let first = first_shown(&periods, t.scroll);
        let shown = shown_count(width);
        if t.selection.is_some() {
            let sel_indices: Vec<usize> = periods
                .iter()
                .enumerate()
                .filter(|(_, p)| t.selected(p))
                .map(|(i, _)| i)
                .collect();
            if let (Some(&fs), Some(&ls)) = (sel_indices.first(), sel_indices.last()) {
                let grip = 5 * PT;
                if fs >= first && fs < first + shown {
                    let x0 = SCROLL + (fs - first) as i64 * CELL_WIDTH;
                    if x >= x0 - grip && x <= x0 + grip {
                        return Some(TimelineHit::SelectionStart);
                    }
                }
                if ls >= first && ls < first + shown {
                    let x1 = SCROLL + (ls - first + 1) as i64 * CELL_WIDTH;
                    if x >= x1 - grip && x <= x1 + grip {
                        return Some(TimelineHit::SelectionEnd);
                    }
                }
            }
        }
        let col = ((x - SCROLL).max(0) / CELL_WIDTH) as usize;
        if x >= SCROLL && first + col < periods.len() && col < shown {
            return Some(TimelineHit::Period(first + col));
        }
        return None;
    }
    if y >= height - SCROLL && t.show_scroll_bar {
        if x < SCROLL {
            return Some(TimelineHit::Scroll(-1));
        }
        if x > width - SCROLL {
            return Some(TimelineHit::Scroll(1));
        }
        let (tx0, tx1) = thumb_rect(t, &periods, width);
        if x >= tx0 && x <= tx1 {
            return Some(TimelineHit::ScrollThumb);
        }
        if x < tx0 {
            return Some(TimelineHit::Scroll(-1));
        } else {
            return Some(TimelineHit::Scroll(1));
        }
    }
    None
}
