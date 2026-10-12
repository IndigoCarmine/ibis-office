// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ハイパーリンク: ⌘K (or 挿入 > リンク) opens the sidebar's panel to link
//! the selected cells to a web address or a place in the workbook
//! (`#Sheet1!A1`), shown as Excel's 「ハイパーリンク」 style; ⌘-click a
//! linked cell follows it; the panel takes a link off.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Hyperlink, Rgb, SheetCommand, StyleChange};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// Excel's 「ハイパーリンク」 style colour (the Office theme's hyperlink).
const LINK_COLOR: u32 = 0x0563C1;

pub(crate) struct LinkPanel {
    pub(crate) address: Entity<InputState>,
}

impl SlideEditor {
    /// The link of the selected cell.
    pub(crate) fn current_link(&self) -> Option<Hyperlink> {
        let (column, row) = self.current_cell()?;
        let (_, sheet) = self.current_sheet()?;
        sheet.hyperlinks.iter().find(|h| (h.range.first_row..=h.range.last_row).contains(&row) && (h.range.first_column..=h.range.last_column).contains(&column)).cloned()
    }

    pub(crate) fn open_link_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let address = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.https-or-sheet1-a1")));
        if let Some(link) = self.current_link() {
            let text = link.target.clone().unwrap_or_else(|| format!("#{}", link.location.clone().unwrap_or_default()));
            address.update(cx, |a, cx| a.set_value(text, window, cx));
        }
        address.update(cx, |a, cx| a.focus(window, cx));
        if let Some(s) = self.sheet_state_mut() {
            s.names = None;
            s.validation = None;
            s.find = None;
            s.link = Some(LinkPanel { address });
        }
        if let Some(s) = self.sheet_state_mut() {
            s.page_setup = false;
        }
        if let Some(s) = self.sheet_state_mut() {
            s.custom_format = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Link the selected cells to `address` (`#…` a place in the workbook):
    /// one step with the link's style, and the address as the text of an
    /// empty cell.
    pub(crate) fn insert_cell_link(&mut self, address: &str, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let address = address.trim();
        let Some(range) = self.selected_range() else { return };
        if address.is_empty() {
            return;
        }
        let link = match address.strip_prefix('#') {
            Some(place) => Hyperlink { range, target: None, location: Some(place.to_string()), tooltip: None },
            None => Hyperlink { range, target: Some(address.to_string()), location: None, tooltip: None },
        };
        let slide = self.current_slide;
        let mut commands = vec![
            Command::Sheet(SheetCommand::SetHyperlink { slide, range, link: Some(link) }),
            Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::Color(Rgb(LINK_COLOR)) }),
            Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::Underline(true) }),
        ];
        if self.current_sheet().is_some_and(|(_, s)| s.cell(range.first_row, range.first_column).is_none_or(|c| c.value.is_empty() && c.formula.is_none())) {
            let edits = vec![CellEdit { row: range.first_row, column: range.first_column, input: CellInput::Value(CellValue::Text(address.trim_start_matches('#').to_string())) }];
            commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("link rejected: {err}");
        }
        cx.notify();
    }

    /// Take the links off the selected cells (Excel leaves the text and its format).
    pub(crate) fn remove_cell_link(&mut self, cx: &mut Context<Self>) {
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetHyperlink { slide, range, link: None })]) {
            log::warn!("removing a link rejected: {err}");
        }
        cx.notify();
    }

    /// ⌘-click on a linked cell: open its address, or go to its place.
    pub(crate) fn follow_cell_link(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(link) = self.current_link() else { return false };
        match (&link.target, &link.location) {
            (Some(url), _) => {
                cx.open_url(url);
                self.status = Some(crate::shell::i18n::format("_xlsx.opening-url", &[("url", url)]).into());
                cx.notify();
            }
            (None, Some(place)) => {
                if !self.go_to_reference(place, window, cx) {
                    self.status = Some(crate::shell::i18n::format("_xlsx.invalid-reference-place", &[("place", place)]).into());
                    cx.notify();
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn cell_link_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let address = self.sheet_state()?.link.as_ref()?.address.clone();
        let a2 = address.clone();
        let current: SharedString = self.current_link().map_or(crate::shell::i18n::text("_xlsx.no-link").into(), |l| l.target.unwrap_or_else(|| format!("#{}", l.location.unwrap_or_default())).into());
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(div().text_size(px(12.)).child(current))
            .child(Input::new(&address).small())
            .child(Button::new("link-insert").primary().small().label(crate::shell::i18n::text("_shell.insert-link")).on_click(cx.listener(move |this, _, _, cx| {
                let text = a2.read(cx).value().to_string();
                this.insert_cell_link(&text, cx);
            })))
            .child(Button::new("link-remove").small().ghost().label(crate::shell::i18n::text("_shell.remove-link")).on_click(cx.listener(|this, _, _, cx| this.remove_cell_link(cx))));
        Some(SidebarPanel { title: crate::shell::i18n::text("insert.link").into(), body: body.into_any_element() })
    }
}
