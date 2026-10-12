// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Open and Repair UI and workflow for damaged Excel workbooks (XLSX-FILE-020).

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use ibis_pptx::repair::{repair_workbook, RepairLogLevel, RepairMode, RepairReport};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use crate::shell::icons::Material;

/// Panel state for opening and repairing a damaged workbook.
pub(crate) struct RepairWorkbookPanel {
    pub(crate) file_path: Option<PathBuf>,
    pub(crate) file_path_input: Entity<InputState>,
    pub(crate) mode: RepairMode,
    pub(crate) status_message: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) report: Option<RepairReport>,
}

impl RepairWorkbookPanel {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(""));
        Self {
            file_path: None,
            file_path_input: input,
            mode: RepairMode::Repair,
            status_message: None,
            error_message: None,
            report: None,
        }
    }

    pub(crate) fn with_path(path: PathBuf, window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        let text_val = path.to_string_lossy().to_string();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(text_val));
        Self {
            file_path: Some(path),
            file_path_input: input,
            mode: RepairMode::Repair,
            status_message: None,
            error_message: None,
            report: None,
        }
    }
}

impl SlideEditor {
    pub(crate) fn open_xlsx_repair_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sheet_state().is_none() {
            return;
        }
        let panel = RepairWorkbookPanel::new(window, cx);
        self.wb_mut().panels.close();
        self.wb_mut().panels.repair = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn open_xlsx_repair_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.sheet_state().is_none() {
            return;
        }
        let panel = RepairWorkbookPanel::with_path(path, window, cx);
        self.wb_mut().panels.close();
        self.wb_mut().panels.repair = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_xlsx_repair_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(wb) = self.sheet_state_mut() {
            wb.panels.repair = None;
        }
        cx.notify();
    }

    pub(crate) fn set_xlsx_repair_mode(&mut self, mode: RepairMode, cx: &mut Context<Self>) {
        if let Some(panel) = self.wb_mut().panels.repair.as_mut() {
            panel.mode = mode;
        }
        cx.notify();
    }

    pub(crate) fn execute_xlsx_repair(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.wb().panels.repair.as_ref() else { return };
        let path = panel.file_path.clone().or_else(|| {
            let val = panel.file_path_input.read(cx).value();
            let s = val.trim();
            (!s.is_empty()).then(|| PathBuf::from(s))
        });
        let mode = panel.mode;

        let Some(path) = path else {
            if let Some(panel) = self.wb_mut().panels.repair.as_mut() {
                panel.error_message = Some(text("xlsx.repair.error-no-file").to_string());
            }
            cx.notify();
            return;
        };

        if !path.exists() {
            if let Some(panel) = self.wb_mut().panels.repair.as_mut() {
                panel.error_message = Some(text("xlsx.repair.error-not-found").to_string());
            }
            cx.notify();
            return;
        }

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                if let Some(panel) = self.wb_mut().panels.repair.as_mut() {
                    panel.error_message = Some(format!("{}: {}", text("xlsx.repair.error-read"), e));
                }
                cx.notify();
                return;
            }
        };

        match repair_workbook(&data, mode) {
            Ok(repaired) => {
                let report = repaired.report.clone();
                let doc_name: SharedString = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "RepairedWorkbook.xlsx".to_string())
                    .into();

                self.load(repaired.presentation, doc_name, window, cx);
                self.source = Some(Arc::new(repaired.source));
                self.set_file_path(Some(path.clone()));
                self.is_repaired = true;
                self.repair_summary = Some(report.summary_text());
                self.repair_report = Some(report.clone());

                let mut new_panel = RepairWorkbookPanel::new(window, cx);
                let path_disp = path.to_string_lossy().to_string();
                new_panel.file_path_input.update(cx, |input, cx| input.set_value(path_disp, window, cx));
                new_panel.file_path = Some(path);
                new_panel.mode = mode;
                new_panel.report = Some(report);
                new_panel.status_message = Some(text("xlsx.repair.success-msg").to_string());
                self.wb_mut().panels.repair = Some(new_panel);
                self.sidebar_open = true;

                self.set_status(text("xlsx.repair.success-msg").to_string(), cx);
            }
            Err(e) => {
                if let Some(panel) = self.wb_mut().panels.repair.as_mut() {
                    panel.error_message = Some(format!("{}: {}", text("xlsx.repair.error-failed"), e));
                    panel.status_message = None;
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn dismiss_repair_banner(&mut self, cx: &mut Context<Self>) {
        self.is_repaired = false;
        cx.notify();
    }

    pub(crate) fn render_repair_banner(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg_color = rgb(0xe8f5e9); // soft light green
        let border_color = rgb(0xa5d6a7);
        let text_color = rgb(0x1b5e20);

        let summary = self.repair_summary.as_deref().unwrap_or("");
        let banner_text = if summary.is_empty() {
            text("xlsx.repair.banner-default").to_string()
        } else {
            format!("［{}］ {} {}", text("xlsx.repair.banner-tag"), summary, text("xlsx.repair.banner-advice")) // i18n-ok（区切りの括弧）
        };

        h_flex()
            .id("repair-banner")
            .w_full()
            .py_1()
            .px_3()
            .bg(bg_color)
            .border_b_1()
            .border_color(border_color)
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(Material("cleaning_services")).size(px(16.)).text_color(text_color))
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(text_color).child(banner_text)),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("repair-banner-details")
                            .small()
                            .ghost()
                            .label(text("xlsx.repair.show-report-btn"))
                            .on_click(cx.listener(|this, _, window, cx| this.open_xlsx_repair_panel(window, cx))),
                    )
                    .child(
                        Button::new("repair-banner-close")
                            .small()
                            .ghost()
                            .label(text("xlsx.repair.banner-close"))
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss_repair_banner(cx))),
                    ),
            )
    }

    pub(crate) fn xlsx_repair_panel(&self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.wb().panels.repair.as_ref()?;
        let theme = cx.theme();

        let is_repair = panel.mode == RepairMode::Repair;
        let is_formulas = panel.mode == RepairMode::ExtractFormulas;
        let is_values = panel.mode == RepairMode::ExtractValues;

        let report_elem = if let Some(rep) = &panel.report {
            let mut logs_col = v_flex().gap_1().w_full();
            for log in &rep.logs {
                let (badge_bg, badge_fg) = match log.level {
                    RepairLogLevel::Info => (rgb(0xe1f5fe), rgb(0x0277bd)),
                    RepairLogLevel::Warning => (rgb(0xfff8e1), rgb(0xf57f17)),
                    RepairLogLevel::Repaired => (rgb(0xe8f5e9), rgb(0x2e7d32)),
                    RepairLogLevel::Removed => (rgb(0xfbe9e7), rgb(0xd84315)),
                };
                logs_col = logs_col.child(
                    h_flex()
                        .gap_1()
                        .items_start()
                        .child(
                            div()
                                .px_1()
                                .rounded_xs()
                                .bg(badge_bg)
                                .text_color(badge_fg)
                                .text_xs()
                                .child(log.level.as_str()),
                        )
                        .child(div().text_xs().text_color(theme.foreground).child(log.message.clone())),
                );
            }

            v_flex()
                .gap_2()
                .p_2()
                .rounded_sm()
                .bg(rgb(0xf9f9f9))
                .border_1()
                .border_color(rgb(0xeeeeee))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_xs()
                        .text_color(rgb(0x2e7d32))
                        .child(rep.summary_text()),
                )
                .child(
                    div()
                        .id("repair-logs-list")
                        .max_h(px(160.0))
                        .overflow_y_scroll()
                        .child(logs_col),
                )
        } else {
            v_flex()
        };

        let file_input = panel.file_path_input.clone();

        let body = v_flex()
            .gap_3()
            .p_3()
            .w_full()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(text("xlsx.repair.desc")),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_xs().child(text("xlsx.repair.file-label")))
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(div().flex_1().child(Input::new(&file_input).small()))
                            .child(
                                Button::new("repair-browse-btn")
                                    .small()
                                    .label(text("xlsx.repair.browse-btn"))
                                    .on_click(cx.listener(|_this, _, window, cx| {
                                        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
                                            files: true,
                                            directories: false,
                                            multiple: false,
                                            prompt: Some(text("xlsx.repair.prompt-file").into()),
                                        });
                                        cx.spawn_in(window, async move |this, cx| {
                                            let Ok(Ok(Some(paths))) = picked.await else { return };
                                            let Some(path) = paths.into_iter().next() else { return };
                                            this.update_in(cx, |this, window, cx| {
                                                if let Some(panel) = this.wb_mut().panels.repair.as_mut() {
                                                    let s = path.to_string_lossy().to_string();
                                                    panel.file_path_input.update(cx, |input, cx| input.set_value(s, window, cx));
                                                    panel.file_path = Some(path);
                                                }
                                                cx.notify();
                                            }).ok();
                                        }).detach();
                                    })),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_xs().child(text("xlsx.repair.mode-label")))
                    .child(
                        Button::new("repair-mode-repair")
                            .small()
                            .outline()
                            .label(format!("{} {}", if is_repair { "●" } else { "○" }, text("xlsx.repair.mode-repair")))
                            .on_click(cx.listener(|this, _, _, cx| this.set_xlsx_repair_mode(RepairMode::Repair, cx))),
                    )
                    .child(
                        div().text_xs().text_color(theme.muted_foreground).child(text("xlsx.repair.mode-repair-desc")),
                    )
                    .child(
                        Button::new("repair-mode-formulas")
                            .small()
                            .outline()
                            .label(format!("{} {}", if is_formulas { "●" } else { "○" }, text("xlsx.repair.mode-formulas")))
                            .on_click(cx.listener(|this, _, _, cx| this.set_xlsx_repair_mode(RepairMode::ExtractFormulas, cx))),
                    )
                    .child(
                        div().text_xs().text_color(theme.muted_foreground).child(text("xlsx.repair.mode-formulas-desc")),
                    )
                    .child(
                        Button::new("repair-mode-values")
                            .small()
                            .outline()
                            .label(format!("{} {}", if is_values { "●" } else { "○" }, text("xlsx.repair.mode-values")))
                            .on_click(cx.listener(|this, _, _, cx| this.set_xlsx_repair_mode(RepairMode::ExtractValues, cx))),
                    )
                    .child(
                        div().text_xs().text_color(theme.muted_foreground).child(text("xlsx.repair.mode-values-desc")),
                    ),
            )
            .child(
                Button::new("repair-execute-btn")
                    .primary()
                    .label(text("xlsx.repair.execute-btn"))
                    .on_click(cx.listener(|this, _, window, cx| this.execute_xlsx_repair(window, cx))),
            )
            .when_some(panel.status_message.as_ref(), |this, msg| {
                this.child(div().p_2().rounded_xs().bg(rgb(0xe8f5e9)).text_color(rgb(0x2e7d32)).text_xs().child(msg.clone()))
            })
            .when_some(panel.error_message.as_ref(), |this, err| {
                this.child(div().p_2().rounded_xs().bg(rgb(0xffebee)).text_color(rgb(0xc62828)).text_xs().child(err.clone()))
            })
            .child(report_elem)
            .child(
                Button::new("repair-close-btn")
                    .ghost()
                    .label(text("xlsx.repair.close-btn"))
                    .on_click(cx.listener(|this, _, _, cx| this.close_xlsx_repair_panel(cx))),
            )
            .into_any_element();

        Some(SidebarPanel {
            title: text("xlsx.repair.title").into(),
            body,
        })
    }
}
