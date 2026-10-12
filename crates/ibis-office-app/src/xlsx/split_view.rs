// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示 > 分割 (ウィンドウの分割): the window split at the selected cell
//! into panes that scroll on their own, as Excel's: the top pane up and
//! down by itself (sideways with the bottom one), the left pane sideways
//! by itself (up and down with the right one). Drawn as the frozen panes
//! are (`render_frozen_panes`), each pane with its own scrolling. A view
//! of the window only: not saved (frozen panes are).
use crate::shell::editor::SlideEditor;
use gpui_kit::*;
use ibis_doc_model::{Command, SheetCommand};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SplitPanes {
    /// Where it splits: the columns left of it and the rows above it.
    pub(crate) at: (u32, u32),
    /// How far the top and left panes are scrolled (as a scroll offset:
    /// zero or less).
    pub(crate) shift: Point<Pixels>,
}

impl SlideEditor {
    /// The current sheet's split panes, if it is split.
    pub(crate) fn split_panes(&self) -> Option<SplitPanes> {
        self.sheet_state()?.panels.splits.get(&self.current_slide).copied()
    }

    /// How far the split panes are scrolled (nothing for frozen ones).
    pub(crate) fn pane_shift(&self) -> Point<Pixels> {
        self.split_panes().map_or(Point::default(), |p| p.shift)
    }

    /// Split the window at the selected cell (in the middle of the view if
    /// it is A1), or take the split away. Frozen panes are unfrozen first,
    /// as Excel does.
    pub(crate) fn toggle_split(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        if self.split_panes().is_some() {
            self.wb_mut().panels.splits.remove(&slide);
            return cx.notify();
        }
        let (Some((_, sheet)), Some((column, row))) = (self.current_sheet(), self.current_cell()) else { return };
        let at = if (column, row) == (0, 0) {
            // The middle of the view, in whole rows and columns.
            let (Some(ws), Some(content)) = (self.workspace_bounds, self.workspace_content()) else { return };
            let scale = content.scale;
            let offset = self.workspace_scroll.offset();
            let middle = |offsets: Vec<i64>, view: Pixels, scroll: Pixels| {
                let target = (f32::from(view) / 2.0 - f32::from(scroll)) / scale;
                offsets.iter().position(|&o| o as f32 > target).map_or(1, |i| i.saturating_sub(1).max(1)) as u32
            };
            (middle(sheet.column_offsets(), ws.size.width, offset.x), middle(sheet.row_offsets(), ws.size.height, offset.y))
        } else {
            (column, row)
        };
        if sheet.frozen.is_some() {
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Freeze { slide, frozen: None })]) {
                log::warn!("unfreezing rejected: {err}");
            }
        }
        // The panes start where the view is scrolled to.
        let shift = self.workspace_scroll.offset();
        self.wb_mut().panels.splits.insert(slide, SplitPanes { at, shift });
        cx.notify();
    }

    /// The wheel over a split pane: its own scrolling, on the axes it has
    /// (`across` sideways, `down` up and down). False when the wheel goes
    /// on to the sheet (the axis the pane shares with it).
    pub(crate) fn scroll_split_pane(&mut self, event: &ScrollWheelEvent, across: bool, down: bool, cx: &mut Context<Self>) -> bool {
        let (Some(panes), Some(content)) = (self.split_panes(), self.workspace_content()) else { return false };
        let delta = event.delta.pixel_delta(px(20.));
        let mut shift = panes.shift;
        let moved = if down && !delta.y.is_zero() && delta.y.abs() >= delta.x.abs() {
            shift.y = (shift.y + delta.y).clamp(-content.size.height, px(0.));
            true
        } else if across && !delta.x.is_zero() {
            shift.x = (shift.x + delta.x).clamp(-content.size.width, px(0.));
            true
        } else {
            false
        };
        if moved {
            let slide = self.current_slide;
            if let Some(p) = self.wb_mut().panels.splits.get_mut(&slide) {
                p.shift = shift;
            }
            cx.notify();
        }
        moved
    }
}
