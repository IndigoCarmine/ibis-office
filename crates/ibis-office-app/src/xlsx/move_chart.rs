// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! グラフの移動… (Move Chart to Chart Sheet or Existing Sheet):
//! Move chart as an object to another worksheet (`ChartMoveTarget::ExistingSheet`),
//! or to a newly created sheet (`ChartMoveTarget::NewSheet`).
//! Supports Undo/Redo and workbook structure protection checks (XLSX-CHEDIT-020).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{ChartMoveTarget, Command, ShapeId, ShapeKind, SheetCommand, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MoveChartMode {
    NewSheet,
    ExistingSheet,
}

pub(crate) struct MoveChartPanel {
    pub(crate) chart_shape: ShapeId,
    pub(crate) mode: MoveChartMode,
    pub(crate) new_sheet_name: Entity<InputState>,
    pub(crate) target_slide: Option<SlideId>,
    pub(crate) error: Option<SharedString>,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// Opens the "Move Chart" (グラフの移動) sidebar panel for the selected chart.
    pub(crate) fn open_move_chart_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chart_shape = if let Some((id, _)) = self.sheet_chart() {
            id
        } else if let Some(id) = self.slide().shapes.iter().find_map(|s| match &s.kind {
            ShapeKind::Chart(_) => Some(s.id),
            _ => None,
        }) {
            self.selection = Some(id);
            id
        } else {
            self.status = Some(text("_xlsx.select-a-chart").into());
            cx.notify();
            return;
        };

        self.commit_cell_edit(cx);
        self.wb_mut().panels.close();

        // Default new sheet name (e.g. "Chart1", "Chart2", ...)
        let mut n = 1;
        let default_name = loop {
            let candidate = format!("Chart{n}");
            if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &candidate, None) {
                break candidate;
            }
            n += 1;
        };

        let new_sheet_name = cx.new(|cx| InputState::new(window, cx).default_value(default_name));

        // Find default existing sheet target (first other slide, or current slide)
        let other_slide = self
            .presentation()
            .workbook
            .as_deref()
            .and_then(|wb| {
                wb.sheets.iter().find_map(|s| {
                    s.slide.filter(|&id| id != self.current_slide)
                })
            })
            .unwrap_or(self.current_slide);

        self.wb_mut().panels.move_chart = Some(MoveChartPanel {
            chart_shape,
            mode: MoveChartMode::NewSheet,
            new_sheet_name,
            target_slide: Some(other_slide),
            error: None,
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        });

        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn set_move_chart_mode(&mut self, mode: MoveChartMode, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.move_chart.as_mut() {
            p.mode = mode;
            p.error = None;
        }
        cx.notify();
    }

    pub(crate) fn set_move_chart_target_slide(&mut self, slide: SlideId, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.move_chart.as_mut() {
            p.target_slide = Some(slide);
            p.mode = MoveChartMode::ExistingSheet;
            p.error = None;
        }
        cx.notify();
    }

    pub(crate) fn apply_move_chart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb().panels.move_chart.as_ref() else { return };
        let mode = p.mode;
        let chart_shape = p.chart_shape;
        let current_slide = self.current_slide;

        match mode {
            MoveChartMode::NewSheet => {
                let name = p.new_sheet_name.read(cx).text().to_string().trim().to_string();
                if self.presentation().is_workbook_structure_locked() {
                    if let Some(p) = self.wb_mut().panels.move_chart.as_mut() {
                        p.error = Some(text("_xlsx.cannot-create-a-new-sheet-because-the-workbook-s").into());
                    }
                    cx.notify();
                    return;
                }
                if !ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, None) {
                    if let Some(p) = self.wb_mut().panels.move_chart.as_mut() {
                        p.error = Some(text("_xlsx.invalid-sheet-name").into());
                    }
                    cx.notify();
                    return;
                }
                let new_slide_id = self.document.ids().new_slide_id();
                let new_shape_id = self.document.ids().new_shape_id();

                self.run_sheet(
                    vec![Command::Sheet(SheetCommand::MoveChart {
                        from_slide: current_slide,
                        shape: chart_shape,
                        target: ChartMoveTarget::NewSheet {
                            name,
                            slide: new_slide_id,
                            sheet_shape: new_shape_id,
                        },
                    })],
                    cx,
                );

                self.wb_mut().panels.move_chart = None;
                self.sidebar_open = false;
                if self.presentation().slide(new_slide_id).is_some() {
                    self.select_slide(new_slide_id, window, cx);
                    self.selection = Some(chart_shape);
                }
                cx.notify();
            }
            MoveChartMode::ExistingSheet => {
                let Some(dest_slide) = p.target_slide else {
                    if let Some(p) = self.wb_mut().panels.move_chart.as_mut() {
                        p.error = Some(text("_xlsx.please-select-a-destination-sheet").into());
                    }
                    cx.notify();
                    return;
                };

                if dest_slide == current_slide {
                    // Moving within the same sheet is a no-op
                    self.wb_mut().panels.move_chart = None;
                    self.sidebar_open = false;
                    cx.notify();
                    return;
                }

                self.run_sheet(
                    vec![Command::Sheet(SheetCommand::MoveChart {
                        from_slide: current_slide,
                        shape: chart_shape,
                        target: ChartMoveTarget::ExistingSheet { slide: dest_slide },
                    })],
                    cx,
                );

                self.wb_mut().panels.move_chart = None;
                self.sidebar_open = false;
                if self.presentation().slide(dest_slide).is_some() {
                    self.select_slide(dest_slide, window, cx);
                    self.selection = Some(chart_shape);
                }
                cx.notify();
            }
        }
    }

    pub(crate) fn move_chart_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.move_chart.as_ref()?;
        let mode = p.mode;
        let is_locked = self.presentation().is_workbook_structure_locked();
        let target_slide = p.target_slide;
        let error = p.error.clone();
        let ui = p.ui.clone();

        let mark = move |id: &str, el: AnyElement| {
            let id = id.to_string();
            let ui = ui.clone();
            div()
                .relative()
                .child(el)
                .child(
                    canvas(|_, _, _| {}, move |bounds, _, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element()
        };

        let mut body = v_flex().gap_3().px(px(16.)).py(px(8.));

        body = body.child(
            div()
                .text_size(px(12.))
                .text_color(cx.theme().muted)
                .child(text("_xlsx.choose-where-you-want-the-chart-to-be-placed")),
        );

        if let Some(err) = error {
            body = body.child(
                div()
                    .p(px(6.))
                    .rounded_md()
                    .bg(cx.theme().danger.opacity(0.1))
                    .border_1()
                    .border_color(cx.theme().danger)
                    .text_size(px(12.))
                    .text_color(cx.theme().danger)
                    .child(err),
            );
        }

        // Section 1: New Sheet
        let new_sheet_active = mode == MoveChartMode::NewSheet;
        let new_sheet_btn = Button::new("move-chart-mode-new-btn")
            .small()
            .when(new_sheet_active, |b| b.primary())
            .label(text("_xlsx.new-sheet"))
            .disabled(is_locked)
            .on_click(cx.listener(|this, _, _, cx| {
                this.set_move_chart_mode(MoveChartMode::NewSheet, cx);
            }));

        let new_sheet_input = Input::new(&p.new_sheet_name).disabled(is_locked || !new_sheet_active);

        let new_sheet_sec = v_flex()
            .gap_1()
            .p(px(8.))
            .rounded_md()
            .border_1()
            .border_color(if new_sheet_active { cx.theme().accent } else { cx.theme().border })
            .child(h_flex().items_center().gap_2().child(mark("move-chart-mode-new", new_sheet_btn.into_any_element())))
            .child(mark("move-chart-new-name-input", new_sheet_input.into_any_element()));

        body = body.child(new_sheet_sec);

        // Section 2: Object in existing sheet
        let existing_sheet_active = mode == MoveChartMode::ExistingSheet;
        let existing_btn = Button::new("move-chart-mode-existing-btn")
            .small()
            .when(existing_sheet_active, |b| b.primary())
            .label(text("_xlsx.object-in"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.set_move_chart_mode(MoveChartMode::ExistingSheet, cx);
            }));

        let mut sheets_list = v_flex().gap_1();
        if let Some(wb) = self.presentation().workbook.as_deref() {
            for (i, sheet) in wb.sheets.iter().enumerate() {
                let Some(slide_id) = sheet.slide else { continue };
                let is_current = slide_id == self.current_slide;
                let is_selected = target_slide == Some(slide_id);
                let sheet_label = if is_current {
                    crate::shell::i18n::format("_xlsx.sheet-current", &[("sheet", &sheet.name)])
                } else {
                    sheet.name.clone()
                };

                let item_btn = Button::new(("move-chart-sheet-item", i))
                    .small()
                    .when(is_selected && existing_sheet_active, |b| b.primary())
                    .label(SharedString::from(sheet_label))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_move_chart_target_slide(slide_id, cx);
                    }));

                sheets_list = sheets_list.child(mark(&format!("move-chart-sheet-item-{i}"), item_btn.into_any_element()));
            }
        }

        let existing_sec = v_flex()
            .gap_2()
            .p(px(8.))
            .rounded_md()
            .border_1()
            .border_color(if existing_sheet_active { cx.theme().accent } else { cx.theme().border })
            .child(h_flex().items_center().gap_2().child(mark("move-chart-mode-existing", existing_btn.into_any_element())))
            .child(sheets_list);

        body = body.child(existing_sec);

        // Section 3: Action Buttons
        let ok_btn = Button::new("move-chart-ok-btn")
            .primary()
            .small()
            .label(text("OK"))
            .on_click(cx.listener(|this, _, window, cx| {
                this.apply_move_chart(window, cx);
            }));

        let cancel_btn = Button::new("move-chart-cancel-btn")
            .small()
            .label(text("_shell.cancel"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.wb_mut().panels.move_chart = None;
                this.sidebar_open = false;
                cx.notify();
            }));

        let buttons_row = h_flex()
            .justify_end()
            .gap_2()
            .mt(px(4.))
            .child(mark("move-chart-ok", ok_btn.into_any_element()))
            .child(mark("move-chart-cancel", cancel_btn.into_any_element()));

        body = body.child(buttons_row);

        Some(SidebarPanel {
            title: text("_xlsx.move-chart").into(),
            body: body.into_any_element(),
        })
    }
}
