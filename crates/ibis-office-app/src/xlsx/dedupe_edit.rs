// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > 重複の削除: the rows of a range repeating an earlier row in
//! the columns chosen go (`SheetCommand::RemoveDuplicates`), as Excel's
//! dialog: the range (the current region), whether its first row is a
//! heading, the columns to compare (all at first), and a message of how
//! many rows went and how many are left.
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, SheetCommand, SlideId};

pub(crate) struct DedupePanel {
    pub(crate) range: Entity<InputState>,
    /// The columns compared, by their index in the sheet.
    pub(crate) columns: Vec<u32>,
    pub(crate) header: bool,
    /// What happened, or what is wrong.
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
    slide: SlideId,
}

/// A range typed as `A1:C10` (or one cell).
pub(crate) fn parse_range(text: &str) -> Option<CellRange> {
    let mut ends = text.trim().split(':');
    let a = ibis_calc::formula::parser::parse_cell(ends.next()?)?;
    let b = match ends.next() {
        Some(s) => ibis_calc::formula::parser::parse_cell(s)?,
        None => a.clone(),
    };
    if ends.next().is_some() || a.row > b.row || a.column > b.column {
        return None;
    }
    Some(CellRange { first_row: a.row, last_row: b.row, first_column: a.column, last_column: b.column })
}

/// A column's letters (`A`, `AB`).
pub(crate) fn column_name(column: u32) -> String {
    let name = ibis_doc_model::cell_name(column, 0);
    name.strip_suffix('1').unwrap_or(&name).to_string()
}

impl SlideEditor {
    pub(crate) fn open_dedupe_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((r, _, header)) = self.sort_selection() else { return };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("A1:C10"));
        input.update(cx, |i, cx| {
            i.set_value(format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row)), window, cx);
        });
        let panel = DedupePanel {
            range: input,
            columns: (r.first_column..=r.last_column).collect(),
            header,
            message: String::new(),
            ui: Default::default(),
            slide: self.current_slide,
        };
        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.sort_edit = None;
        state.panels.close();
        state.panels.dedupe = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn dedupe_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.dedupe.as_mut() {
            p.message = text;
        }
        cx.notify();
    }

    /// The range typed, its columns in the sheet: the columns chosen made
    /// to fit a range changed since (all of a new one's).
    fn dedupe_range(&self, cx: &App) -> Option<CellRange> {
        let p = self.sheet_state()?.panels.dedupe.as_ref()?;
        let val = p.range.read(cx).value().to_string();
        if let Some((slide_id, r)) = super::sheet_ui::parse_reference(val.trim()).and_then(|ref_| self.resolve_reference(&ref_))
            && slide_id == self.current_slide
        {
            return Some(r);
        }
        parse_range(&val)
    }

    pub(crate) fn refresh_dedupe_range_from_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((r, _, header)) = self.sort_selection() else { return };
        let range_str = format!(
            "{}:{}",
            ibis_doc_model::cell_name(r.first_column, r.first_row),
            ibis_doc_model::cell_name(r.last_column, r.last_row)
        );
        let cur_slide = self.current_slide;
        if let Some(p) = self.wb_mut().panels.dedupe.as_mut() {
            p.range.update(cx, |i, cx| i.set_value(range_str, window, cx));
            p.columns = (r.first_column..=r.last_column).collect();
            p.header = header;
            p.slide = cur_slide;
        }
        cx.notify();
    }

    pub(crate) fn apply_dedupe_panel(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.dedupe.as_ref()) else { return };
        if p.slide != self.current_slide {
            self.dedupe_message(crate::shell::i18n::text("_xlsx.the-sheet-has-changed-reopen-it").into(), cx);
            return;
        }
        let Some(r) = self.dedupe_range(cx) else {
            self.dedupe_message(crate::shell::i18n::text("_xlsx.enter-a-valid-cell-range").into(), cx);
            return;
        };
        let (header, slide) = (p.header, p.slide);
        let columns: Vec<u32> = p.columns.iter().copied().filter(|c| (r.first_column..=r.last_column).contains(c)).collect();
        if columns.is_empty() {
            self.dedupe_message(crate::shell::i18n::text("_xlsx.select-at-least-one-column").into(), cx);
            return;
        }
        let Some((_, sheet)) = self.current_sheet() else { return };
        if header && r.first_row == r.last_row {
            self.dedupe_message(crate::shell::i18n::text("_xlsx.check-the-range-and-headers").into(), cx);
            return;
        }
        let gone = ibis_doc_model::sheet_command::duplicate_rows(sheet, r, &columns, header).len();
        let rows = (r.last_row - r.first_row + 1 - u32::from(header)) as usize;
        if gone > 0
            && let Err(error) = self.document.execute(vec![Command::Sheet(SheetCommand::RemoveDuplicates { slide, range: r, columns, header })])
        {
            self.dedupe_message(error.to_string(), cx);
            return;
        }
        self.wb_mut().last_fill = None;
        // Excel's message.
        let text = if gone == 0 {
            crate::shell::i18n::text("_xlsx.no-duplicate-values-were-found").to_string()
        } else {
            crate::shell::i18n::format("dedupe.done", &[("gone", &gone), ("left", &(rows - gone))])
        };
        self.dedupe_message(text, cx);
    }

    pub(crate) fn dedupe_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let r = self.dedupe_range(cx);
        let p = self.sheet_state()?.panels.dedupe.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(crate::shell::i18n::text("_shell.range"))
                    .child(mark(
                        "use-selection",
                        Button::new("dedupe-use-selection")
                            .xsmall()
                            .ghost()
                            .label(crate::shell::i18n::text("xlsx.subtotal-use-selection"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.refresh_dedupe_range_from_selection(window, cx);
                            }))
                            .into_any_element(),
                    )),
            )
            .child(mark("range", Input::new(&p.range).small().into_any_element()))
            .child(mark(
                "header",
                Button::new("dedupe-header")
                    .small()
                    .label(crate::shell::i18n::text(if p.header { "fix.dedupe-edit.my-data-has-headers" } else { "fix.dedupe-edit.my-data-has-headers-off" }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.dedupe.as_mut() {
                            p.header = !p.header;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            ))
            .child(crate::shell::i18n::text("fix.dedupe-edit.columns-compare"))
            .child(
                h_flex()
                    .gap_1()
                    .child(mark(
                        "all",
                        Button::new("dedupe-all")
                            .small()
                            .label(crate::shell::i18n::text("fix.dedupe-edit.select-all"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let r = this.dedupe_range(cx);
                                if let (Some(r), Some(p)) = (r, this.wb_mut().panels.dedupe.as_mut()) {
                                    p.columns = (r.first_column..=r.last_column).collect();
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    ))
                    .child(mark(
                        "none",
                        Button::new("dedupe-none")
                            .small()
                            .label(crate::shell::i18n::text("fix.dedupe-edit.unselect-all"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.wb_mut().panels.dedupe.as_mut() {
                                    p.columns.clear();
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    )),
            );
        if let Some(r) = r {
            let sheet = self.current_sheet().map(|(_, s)| s);
            for column in r.first_column..=r.last_column.min(r.first_column + 63) {
                let on = p.columns.contains(&column);
                // The heading's text, as Excel lists the columns with one.
                let heading = sheet
                    .filter(|_| p.header)
                    .and_then(|s| s.cell(r.first_row, column))
                    .map(|c| c.text.clone())
                    .filter(|t| !t.is_empty());
                let name = match heading {
                    Some(h) => crate::shell::i18n::format("dedupe.column-heading", &[("column", &column_name(column)), ("heading", &h)]),
                    None => crate::shell::i18n::format("dedupe.column", &[("column", &column_name(column))]),
                };
                body = body.child(mark(
                    &format!("column-{}", column_name(column)),
                    Button::new(("dedupe-column", column as usize))
                        .small()
                        .label(format!("{} {name}", if on { "☑" } else { "☐" }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.dedupe.as_mut() {
                                match p.columns.iter().position(|&c| c == column) {
                                    Some(i) => {
                                        p.columns.remove(i);
                                    }
                                    None => p.columns.push(column),
                                }
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ));
            }
        }
        body = body
            .child(div().text_size(px(12.)).child(SharedString::from(p.message.clone())))
            .child(mark(
                "apply",
                Button::new("dedupe-apply").primary().small().label(crate::shell::i18n::text("fix.dedupe-edit.remove-duplicates")).on_click(cx.listener(|this, _, _, cx| this.apply_dedupe_panel(cx))).into_any_element(),
            ))
            .child(mark(
                "close",
                Button::new("dedupe-close")
                    .small()
                    .label(crate::shell::i18n::text("_shell.close"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.wb_mut().panels.dedupe = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        Some(SidebarPanel { title: "fix.dedupe-edit.remove-duplicates".into(), body: body.into_any_element() })
    }
}
