// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Tables: a grid of cells with text, fills and borders.

use crate::drawing::{Fill, Stroke};
use crate::geometry::Emu;
use crate::text::TextBody;

/// The four edges and diagonal borders of a cell. `None` draws no line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellBorders {
    pub left: Option<Stroke>,
    pub right: Option<Stroke>,
    pub top: Option<Stroke>,
    pub bottom: Option<Stroke>,
    pub tl_to_br: Option<Stroke>,
    pub bl_to_tr: Option<Stroke>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableCell {
    /// Text, with the cell's margins as insets.
    pub text: TextBody,
    pub fill: Fill,
    pub borders: CellBorders,
    /// Columns and rows this cell spans (1 for an ordinary cell).
    pub column_span: u32,
    pub row_span: u32,
    /// Covered by a neighbour spanning over it; not drawn.
    pub merged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableRow {
    /// Height as stated in the file. PowerPoint grows rows to fit their
    /// text; the shape's frame holds the grown size.
    pub height: Emu,
    pub cells: Vec<TableCell>,
    /// The row of the table as read this one is (`None` for a new row):
    /// for writing rows inserted, removed and moved into the file.
    pub source: Option<usize>,
}

/// Table style options (`a:tblPr/@firstRow`, `@bandRow`, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableStyleOptions {
    pub first_row: bool,
    pub last_row: bool,
    pub first_col: bool,
    pub last_col: bool,
    pub band_row: bool,
    pub band_col: bool,
}

impl Default for TableStyleOptions {
    fn default() -> Self {
        Self {
            first_row: true,
            last_row: false,
            first_col: false,
            last_col: false,
            band_row: true,
            band_col: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// Column widths.
    pub columns: Vec<Emu>,
    pub rows: Vec<TableRow>,
    /// For each column, the column of the table as read it is (`None` for
    /// a new one; empty for a table not read from a file).
    pub column_sources: Vec<Option<usize>>,
    /// Table style options (`firstRow`, `bandRow`, etc.).
    pub style_options: TableStyleOptions,
    /// Style ID (`a:tableStyleId`), if specified.
    pub style_id: Option<String>,
}

impl Table {
    /// Row heights scaled so that together they fill `height` (the frame),
    /// which is how tall PowerPoint drew the rows after growing them.
    pub fn row_heights(&self, height: Emu) -> Vec<Emu> {
        let stated: Emu = self.rows.iter().map(|r| r.height).sum();
        if stated <= 0 {
            let n = self.rows.len().max(1) as Emu;
            return self.rows.iter().map(|_| height / n).collect();
        }
        if stated >= height {
            return self.rows.iter().map(|r| r.height).collect();
        }
        let mut out: Vec<Emu> = self.rows.iter().map(|r| r.height * height / stated).collect();
        // Give rounding leftovers to the last row so the total is exact.
        let total: Emu = out.iter().sum();
        if let Some(last) = out.last_mut() {
            *last += height - total;
        }
        out
    }
}

impl Table {
    /// A new table of `rows` × `columns` filling `width` × `height`, as
    /// PowerPoint's 表の挿入 makes one in its default style (中間スタイル 2 -
    /// アクセント 1): a dark header row with white bold text, light banded
    /// rows and white lines between the cells, written as the cells' own
    /// formats.
    pub fn new_grid(rows: usize, columns: usize, width: Emu, height: Emu) -> Table {
        use crate::drawing::Color;
        use crate::{Insets, Rgb};
        use crate::text::{TextStyle, VerticalAnchor};
        let (rows, columns) = (rows.max(1), columns.max(1));
        let line = Some(Stroke::solid(Color::WHITE, 12_700));
        let cell = |row: usize| {
            let header = row == 0;
            let style = TextStyle { color: if header { Rgb::WHITE } else { Rgb(0x000000) }, bold: header, ..TextStyle::default() };
            let mut text = TextBody::plain("", style);
            text.insets = Insets { left: 91_440, right: 91_440, top: 45_720, bottom: 45_720 };
            text.anchor = VerticalAnchor::Top;
            let fill = crate::drawing::Fill::Solid(Color::opaque(Rgb(match row {
                0 => 0x4472c4,
                r if r % 2 == 1 => 0xcfd5ea,
                _ => 0xe9ebf5,
            })));
            TableCell {
                text,
                fill,
                borders: CellBorders { left: line.clone(), right: line.clone(), top: line.clone(), bottom: line.clone(), tl_to_br: None, bl_to_tr: None },
                column_span: 1,
                row_span: 1,
                merged: false,
            }
        };
        let column_width = width / columns as Emu;
        let row_height = height / rows as Emu;
        Table {
            columns: vec![column_width; columns],
            rows: (0..rows).map(|r| TableRow { height: row_height, cells: (0..columns).map(|_| cell(r)).collect(), source: None }).collect(),
            column_sources: Vec::new(),
            style_options: TableStyleOptions::default(),
            style_id: None,
        }
    }

    /// The table with the cells from (`r0`, `c0`) to (`r1`, `c1`) merged into
    /// the first (セルの結合): it spans them, the others are covered, and
    /// their text follows its own as more paragraphs (as PowerPoint merges).
    /// `None` if the range is one cell or cuts a merged cell.
    pub fn with_merged(&self, (r0, c0): (usize, usize), (r1, c1): (usize, usize)) -> Option<Table> {
        let (r0, r1, c0, c1) = (r0.min(r1), r0.max(r1), c0.min(c1), c0.max(c1));
        if (r0, c0) == (r1, c1) || r1 >= self.rows.len() || c1 >= self.columns.len() {
            return None;
        }
        // No merged cell may stick out of the range.
        for (r, row) in self.rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                if cell.merged || (cell.row_span <= 1 && cell.column_span <= 1) {
                    continue;
                }
                let (re, ce) = (r + cell.row_span.max(1) as usize - 1, c + cell.column_span.max(1) as usize - 1);
                let inside = r >= r0 && re <= r1 && c >= c0 && ce <= c1;
                let apart = re < r0 || r > r1 || ce < c0 || c > c1;
                if !inside && !apart {
                    return None;
                }
            }
        }
        let mut t = self.clone();
        let mut extra: Vec<crate::text::Paragraph> = Vec::new();
        for r in r0..=r1 {
            for c in c0..=c1 {
                if (r, c) == (r0, c0) {
                    continue;
                }
                let cell = &mut t.rows[r].cells[c];
                if !cell.text.is_empty() {
                    extra.extend(cell.text.paragraphs.iter().cloned());
                }
                // The covered cell keeps its paragraph's format, emptied.
                let emptied = Self::empty_like(cell);
                cell.text = emptied.text;
                cell.merged = true;
                cell.row_span = 1;
                cell.column_span = 1;
            }
        }
        let first = &mut t.rows[r0].cells[c0];
        first.row_span = (r1 - r0 + 1) as u32;
        first.column_span = (c1 - c0 + 1) as u32;
        first.merged = false;
        if !extra.is_empty() {
            // The text moved in takes the first cell's paragraph format.
            let like = first.text.paragraphs.first().cloned().unwrap_or_default();
            if first.text.is_empty() {
                first.text.paragraphs.clear();
            }
            first.text.paragraphs.extend(extra.into_iter().map(|p| crate::text::Paragraph { runs: p.runs, end_style: p.end_style, line_break: false, ..like.clone() }));
        }
        Some(t)
    }

    /// The table with merged cell (`r`, `c`) split back into the cells it
    /// covers (セルの分割), which come back empty; `None` if it spans none.
    pub fn with_split(&self, (r, c): (usize, usize)) -> Option<Table> {
        let cell = self.rows.get(r)?.cells.get(c)?;
        if cell.merged || (cell.row_span <= 1 && cell.column_span <= 1) {
            return None;
        }
        let (rs, cs) = (cell.row_span.max(1) as usize, cell.column_span.max(1) as usize);
        let mut t = self.clone();
        for rr in r..(r + rs).min(t.rows.len()) {
            for cc in c..(c + cs).min(t.columns.len()) {
                let x = &mut t.rows[rr].cells[cc];
                x.merged = false;
                x.row_span = 1;
                x.column_span = 1;
            }
        }
        Some(t)
    }

    /// Whether a cell spans or is covered (rows and columns are inserted and
    /// removed only in tables without).
    pub fn has_merges(&self) -> bool {
        self.rows.iter().flat_map(|r| &r.cells).any(|c| c.merged || c.column_span > 1 || c.row_span > 1)
    }

    /// A cell like `cell` (its fill, borders and text format) without text.
    fn empty_like(cell: &TableCell) -> TableCell {
        let mut text = cell.text.clone();
        let end = text.paragraphs.first().map(|p| p.runs.first().map(|r| r.style.clone()).unwrap_or_else(|| p.end_style.clone())).unwrap_or_default();
        text.paragraphs.truncate(1);
        if let Some(p) = text.paragraphs.first_mut() {
            p.runs.clear();
            p.end_style = end;
        }
        TableCell { text, ..cell.clone() }
    }

    /// The table with a new row before row `at` (`at` = the number of rows:
    /// after the last), like the row above it (the first row for a new first
    /// row); `None` if it has merged cells.
    pub fn with_row_inserted(&self, at: usize) -> Option<Table> {
        if self.has_merges() || self.rows.is_empty() || at > self.rows.len() {
            return None;
        }
        let like = &self.rows[at.saturating_sub(1).min(self.rows.len() - 1)];
        let row = TableRow { height: like.height, cells: like.cells.iter().map(Self::empty_like).collect(), source: None };
        let mut t = self.clone();
        t.rows.insert(at, row);
        Some(t)
    }

    /// The table without row `at`; `None` for its only row or merged cells.
    pub fn with_row_removed(&self, at: usize) -> Option<Table> {
        if self.has_merges() || self.rows.len() <= 1 || at >= self.rows.len() {
            return None;
        }
        let mut t = self.clone();
        t.rows.remove(at);
        Some(t)
    }

    /// The table with a new column before column `at`, like the column left
    /// of it (the first column for a new first one), as wide as it (the
    /// table grows); `None` with merged cells.
    pub fn with_column_inserted(&self, at: usize) -> Option<Table> {
        if self.has_merges() || self.columns.is_empty() || at > self.columns.len() {
            return None;
        }
        let like = at.saturating_sub(1).min(self.columns.len() - 1);
        let mut t = self.clone();
        t.columns.insert(at, self.columns[like]);
        if !t.column_sources.is_empty() {
            t.column_sources.insert(at, None);
        }
        for row in &mut t.rows {
            let cell = Self::empty_like(&row.cells[like.min(row.cells.len().saturating_sub(1))]);
            row.cells.insert(at.min(row.cells.len()), cell);
        }
        Some(t)
    }

    /// The table without column `at`; `None` for its only column or merged cells.
    pub fn with_column_removed(&self, at: usize) -> Option<Table> {
        if self.has_merges() || self.columns.len() <= 1 || at >= self.columns.len() {
            return None;
        }
        let mut t = self.clone();
        t.columns.remove(at);
        if !t.column_sources.is_empty() {
            t.column_sources.remove(at);
        }
        for row in &mut t.rows {
            if at < row.cells.len() {
                row.cells.remove(at);
            }
        }
        Some(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(heights: &[Emu]) -> Table {
        Table {
            columns: vec![100],
            rows: heights.iter().map(|&height| TableRow { height, cells: Vec::new(), source: None }).collect(),
            column_sources: Vec::new(),
            style_options: TableStyleOptions::default(),
            style_id: None,
        }
    }

    #[test]
    fn rows_grow_to_the_frame() {
        assert_eq!(table(&[100, 300]).row_heights(800), vec![200, 600]);
        assert_eq!(table(&[100, 100, 100]).row_heights(1000), vec![333, 333, 334]);
    }

    #[test]
    fn rows_taller_than_the_frame_keep_their_height() {
        assert_eq!(table(&[500, 500]).row_heights(600), vec![500, 500]);
    }
}
