// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The workbook's right pane (#11): 書式設定・グラフ エディター・データの
//! 入力規則 as tabs across its top (the shell draws them, `SidebarTab`),
//! replacing the column of buttons that used to stand at the window's right.
//!
//! 書式設定 shows the 表示形式 panel for now (Excel's セルの書式設定 will
//! widen it: 配置・フォント・罫線・塗りつぶし・保護); グラフ エディター the
//! selected chart's panel; データの入力規則 the validation panel.
use std::rc::Rc;

use gpui_kit::*;

use crate::shell::chrome::{SidebarPanel, SidebarTab};
use crate::shell::editor::SlideEditor;

const FORMAT: &str = "fix.right-tabs.format";
const CHART: &str = "fix.right-tabs.chart-editor";
const VALIDATION: &str = "_shell.data-validation";

pub(super) fn tabs() -> Vec<SidebarTab> {
    vec![
        SidebarTab {
            label: FORMAT,
            panel_title: "fix.protection.format-cells",
            open: Rc::new(|this: &mut SlideEditor, window, cx| {
                this.close_tab_panels(FORMAT);
                this.open_custom_format(window, cx);
            }),
        },
        SidebarTab {
            label: CHART,
            panel_title: "_xlsx.chart-format",
            open: Rc::new(|this: &mut SlideEditor, window, cx| {
                this.close_tab_panels(CHART);
                if this.sheet_chart().is_some() {
                    this.open_sheet_chart_panel(window, cx);
                }
                cx.notify();
            }),
        },
        SidebarTab {
            label: VALIDATION,
            panel_title: "_shell.data-validation",
            open: Rc::new(|this: &mut SlideEditor, window, cx| {
                this.close_tab_panels(VALIDATION);
                this.open_validation_panel(window, cx);
            }),
        },
    ]
}

impl SlideEditor {
    /// Close the three tabs' panels (before one of them opens) and remember
    /// the tab chosen.
    fn close_tab_panels(&mut self, chosen: &'static str) {
        if let Some(s) = self.sheet_state_mut() {
            s.custom_format = None;
            s.validation = None;
            s.chart_edit = None;
            s.right_tab = Some(chosen);
        }
        self.sidebar_open = true;
    }

    /// The グラフ エディター tab with no chart to show: what to do.
    pub(crate) fn right_tab_hint(&mut self, _cx: &mut Context<Self>) -> Option<SidebarPanel> {
        if self.sheet_state()?.right_tab != Some(CHART) {
            return None;
        }
        Some(SidebarPanel {
            title: crate::shell::i18n::text("_xlsx.format-chart").into(),
            body: div().px(px(18.)).py(px(8.)).child(crate::shell::i18n::text("_xlsx.select-a-chart")).into_any_element(),
        })
    }
}
