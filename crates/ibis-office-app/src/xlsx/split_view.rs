// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示 > 分割 (ウィンドウの分割): the window split at the selected cell
//! into panes that scroll on their own, as Excel's: the top pane up and
//! down by itself (sideways with the bottom one), the left pane sideways
//! by itself (up and down with the right one). Drawn as the frozen panes
//! are (`render_frozen_panes`), each pane with its own scrolling.
//! Saved in OOXML as sheetView/pane state="split", with Undo/Redo support.
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
        let (_, sheet) = self.current_sheet()?;
        let at = sheet.split?;
        let shift = self.sheet_state().and_then(|s| s.panels.splits.get(&self.current_slide).map(|p| p.shift)).unwrap_or_default();
        Some(SplitPanes { at, shift })
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
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Split { slide, split: None })]) {
                log::warn!("un-splitting rejected: {err}");
            }
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
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Split { slide, split: Some(at) })]) {
            log::warn!("splitting rejected: {err}");
        }
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

    /// Press on a split bar: double-click removes the split on that axis (or both);
    /// single click starts dragging the split bar.
    pub(crate) fn on_split_down(&mut self, axis: SplitAxis, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some(panes) = self.split_panes() else { return };
        if event.click_count >= 2 {
            // Excel: double-click on a split bar removes the split.
            let (col, row) = panes.at;
            let new_split = match axis {
                SplitAxis::Horizontal => if col > 0 { Some((col, 0)) } else { None },
                SplitAxis::Vertical => if row > 0 { Some((0, row)) } else { None },
                SplitAxis::Both => None,
            };
            if new_split.is_none() {
                self.wb_mut().panels.splits.remove(&slide);
            } else if let Some(at) = new_split {
                if let Some(p) = self.wb_mut().panels.splits.get_mut(&slide) {
                    p.at = at;
                }
            }
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Split { slide, split: new_split })]) {
                log::warn!("split removal rejected: {err}");
            }
            return cx.notify();
        }

        // Start split drag
        self.wb_mut().split_drag = Some(SplitDrag {
            axis,
            start: event.position,
            current: event.position,
            start_at: panes.at,
        });
        cx.notify();
    }

    /// Moving while dragging a split bar: update current position and notify.
    pub(crate) fn on_split_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(drag) = self.wb_mut().split_drag.as_mut() {
            drag.current = position;
            cx.notify();
        }
    }

    /// Release while dragging a split bar: calculate new row/col split position and apply command.
    pub(crate) fn on_split_up(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.wb_mut().split_drag.take() else { return };
        let (Some(content), Some((shape, sheet))) = (self.workspace_content(), self.current_sheet()) else { return };
        let ws_origin = self.workspace_bounds.map_or(Point::default(), |b| b.origin);
        let local_x = f32::from(position.x - ws_origin.x);
        let local_y = f32::from(position.y - ws_origin.y);

        let emu_x = ((local_x - f32::from(content.slide_origin.x)) / content.scale) as i64 - shape.frame.x;
        let emu_y = ((local_y - f32::from(content.slide_origin.y)) / content.scale) as i64 - shape.frame.y;
        let head_w = if sheet.headings { super::sheet_ui::row_header_width(sheet) } else { 0.0 };
        let head_h = if sheet.headings { super::sheet_ui::COLUMN_HEADER_HEIGHT } else { 0.0 };

        let find_closest = |offsets: &[i64], target_emu: i64, min_head: f32, pos: f32| -> u32 {
            if pos <= min_head + 8.0 {
                return 0;
            }
            let mut best_i = 1;
            let mut min_diff = i64::MAX;
            for (i, &offset) in offsets.iter().enumerate().skip(1) {
                let diff = (offset - target_emu).abs();
                if diff < min_diff {
                    min_diff = diff;
                    best_i = i as u32;
                }
            }
            best_i
        };

        let mut new_at = drag.start_at;
        if drag.axis == SplitAxis::Vertical || drag.axis == SplitAxis::Both {
            new_at.0 = find_closest(&sheet.column_offsets(), emu_x, head_w, local_x);
        }
        if drag.axis == SplitAxis::Horizontal || drag.axis == SplitAxis::Both {
            new_at.1 = find_closest(&sheet.row_offsets(), emu_y, head_h, local_y);
        }

        let new_split = if new_at == (0, 0) {
            None
        } else {
            Some(new_at)
        };

        let slide = self.current_slide;
        if let Some(at) = new_split {
            let shift = self.workspace_scroll.offset();
            self.wb_mut().panels.splits.insert(slide, SplitPanes { at, shift });
        } else {
            self.wb_mut().panels.splits.remove(&slide);
        }

        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Split { slide, split: new_split })]) {
            log::warn!("split move rejected: {err}");
        }
        cx.notify();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SplitAxis {
    Horizontal,
    Vertical,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SplitDrag {
    pub(crate) axis: SplitAxis,
    pub(crate) start: Point<Pixels>,
    pub(crate) current: Point<Pixels>,
    pub(crate) start_at: (u32, u32),
}
