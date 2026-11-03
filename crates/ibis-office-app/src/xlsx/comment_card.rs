// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A cell's comment shown while the pointer is on the cell, as in Excel:
//! a threaded comment's posts (who, when, what; replies under the first),
//! or a note's author and text.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;

use crate::shell::editor::SlideEditor;

/// `2026-10-07T09:30:00.00` as `2026/10/07 9:30`.
pub(crate) fn post_time(stamp: &str) -> String {
    let (date, time) = stamp.split_once('T').unwrap_or((stamp, ""));
    let date = date.replace('-', "/");
    let mut hm = time.split(':');
    match (hm.next().and_then(|h| h.parse::<u32>().ok()), hm.next()) {
        (Some(h), Some(m)) => format!("{date} {h}:{}", &m[..m.len().min(2)]),
        _ => date,
    }
}

impl SlideEditor {
    /// Show the comment of `cell` (none: hide), if it has one.
    pub(crate) fn show_comment_card(&mut self, cell: Option<(u32, u32)>, cx: &mut Context<Self>) {
        let has = |(c, r): (u32, u32)| {
            self.current_sheet().is_some_and(|(_, s)| s.threads.iter().any(|t| (t.column, t.row) == (c, r)) || s.comments.iter().any(|n| (n.column, n.row) == (c, r)))
        };
        let shown = cell.filter(|&c| has(c));
        if let Some(s) = self.sheet_state_mut()
            && s.comment_hover != shown
        {
            s.comment_hover = shown;
            cx.notify();
        }
    }

    /// The window position of a cell's top-right corner.
    fn cell_top_right(&self, column: u32, row: u32) -> Option<Point<Pixels>> {
        let layout = self.slide_layout()?;
        let (shape, sheet) = self.current_sheet()?;
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let x = shape.frame.x + xs.get(column as usize + 1)?;
        let y = shape.frame.y + ys.get(row as usize)?;
        Some(point(layout.origin.x + layout.to_px(x), layout.origin.y + layout.to_px(y)))
    }

    /// The card of the comment under the pointer.
    pub(crate) fn render_comment_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (column, row) = self.sheet_state()?.comment_hover?;
        if self.wb().filter_panel.is_some() || self.cell_edit().is_some() {
            return None;
        }
        let (_, sheet) = self.current_sheet()?;
        let theme = cx.theme().clone();
        let mut card = v_flex().w(px(260.)).p_2().gap_2().bg(theme.background).border_1().border_color(theme.border).rounded_md().shadow_lg().text_size(px(12.));
        if let Some(t) = sheet.threads.iter().find(|t| (t.column, t.row) == (column, row)) {
            for (i, post) in t.posts.iter().enumerate() {
                card = card.child(
                    v_flex()
                        .gap(px(2.))
                        .when(i > 0, |d| d.pl_3().border_t_1().border_color(theme.border).pt_1())
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(post.author.clone()))
                        .child(div().text_color(theme.muted_foreground).child(post_time(&post.date)))
                        .child(div().child(post.text.clone())),
                );
            }
            if t.done {
                card = card.child(div().text_color(theme.muted_foreground).child(crate::shell::i18n::text("_xlsx.resolved")));
            }
        } else if let Some(n) = sheet.comments.iter().find(|n| (n.column, n.row) == (column, row)) {
            card = card.when(!n.author.is_empty(), |c| c.child(div().font_weight(FontWeight::SEMIBOLD).child(n.author.clone()))).child(div().child(n.text.clone()));
        } else {
            return None;
        }
        let at = self.cell_top_right(column, row)?;
        Some(deferred(anchored().position(point(at.x + px(6.), at.y)).snap_to_window_with_margin(px(8.)).child(card.id("comment-card").occlude())).with_priority(1).into_any_element())
    }

    /// Move to the next or previous comment/note on the active sheet (wrapping around).
    pub(crate) fn select_next_comment(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut cells: Vec<(u32, u32)> = sheet
            .threads
            .iter()
            .map(|t| (t.column, t.row))
            .chain(sheet.comments.iter().map(|c| (c.column, c.row)))
            .collect();
        if cells.is_empty() {
            return;
        }
        cells.sort_by_key(|&(c, r)| (r, c));
        cells.dedup();

        let cur = self.current_cell();
        let target = if forward {
            if let Some((cc, cr)) = cur {
                cells.iter().find(|&&(c, r)| (r, c) > (cr, cc)).copied().unwrap_or(cells[0])
            } else {
                cells[0]
            }
        } else {
            if let Some((cc, cr)) = cur {
                cells.iter().rfind(|&&(c, r)| (r, c) < (cr, cc)).copied().unwrap_or(*cells.last().unwrap())
            } else {
                *cells.last().unwrap()
            }
        };

        self.set_cell(target.0, target.1);
        self.scroll_range_into_view(true);
        self.show_comment_card(Some(target), cx);
    }

    /// Toggle displaying all note cards simultaneously on the sheet.
    pub(crate) fn toggle_show_all_notes(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.show_all_notes = !s.show_all_notes;
            cx.notify();
        }
    }

    /// Render cards for all notes when `show_all_notes` is active.
    pub(crate) fn render_all_notes(&self, _cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(s) = self.sheet_state() else { return Vec::new() };
        if !s.show_all_notes || self.wb().filter_panel.is_some() || self.cell_edit().is_some() {
            return Vec::new();
        }
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let hover = s.comment_hover;
        let mut cards = Vec::new();
        for n in &sheet.comments {
            if hover == Some((n.column, n.row)) {
                continue;
            }
            let Some(at) = self.cell_top_right(n.column, n.row) else { continue };
            let mut card = v_flex()
                .w(px(220.))
                .p_2()
                .gap_1()
                .bg(rgb(0xfff9c4))
                .text_color(rgb(0x202124))
                .border_1()
                .border_color(rgb(0xd4a017))
                .rounded_sm()
                .shadow_md()
                .text_size(px(12.));
            if !n.author.is_empty() {
                card = card.child(div().font_weight(FontWeight::SEMIBOLD).child(n.author.clone()));
            }
            card = card.child(div().child(n.text.clone()));
            cards.push(
                deferred(
                    anchored()
                        .position(point(at.x + px(6.), at.y))
                        .snap_to_window_with_margin(px(8.))
                        .child(card.id(ElementId::Name(format!("all-note-{}-{}", n.row, n.column).into())).occlude()),
                )
                .with_priority(1)
                .into_any_element(),
            );
        }
        cards
    }
}

#[cfg(test)]
mod tests {
    use super::post_time;

    #[test]
    fn post_times_as_shown() {
        assert_eq!(post_time("2026-10-07T09:30:00.00"), "2026/10/07 9:30");
        assert_eq!(post_time("2026-10-07"), "2026/10/07");
    }
}
