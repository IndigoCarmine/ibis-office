// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A cell's comment shown while the pointer is on the cell, as in Excel:
//! a threaded comment's posts (who, when, what; replies under the first),
//! or a note's author and text.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

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
