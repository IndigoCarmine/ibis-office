// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ワークシート分析: 数式の検証（Evaluate Formula）とウォッチ ウィンドウ（Watch Window）。
//!
//! Excel の「数式」>「ワークシート分析」にある:
//! 1. 「数式の検証」(Evaluate Formula): 選択セルの数式を最も内側の演算から順に
//!    1 ステップずつ評価（簡約）し、下線ハイライト・ステップイン・ステップアウト・やり直しを提供する。
//! 2. 「ウォッチ ウィンドウ」(Watch Window): ワークブック内の重要セル（シート、名前、セル、値、数式）を
//!    一覧登録・常時監視し、クリックでのセルジャンプ、再計算時の自動同期を提供する。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::Disableable as _;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_calc::eval::Book;
use ibis_calc::eval::step::{EvaluateSession, StepInfo};
use ibis_doc_model::{BookView, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 数式の検証パネル（サイドバー表示用）。
pub(crate) struct EvaluateFormulaPanel {
    pub(crate) session: Option<EvaluateSession>,
    pub(crate) info: StepInfo,
    pub(crate) target_cell_label: String,
    pub(crate) message: Option<String>,
}

/// ウォッチ ウィンドウの 1 登録項目。
#[derive(Clone, Debug, PartialEq)]
pub struct WatchEntry {
    pub id: u64,
    pub slide_id: SlideId,
    pub sheet_name: String,
    pub col: u32,
    pub row: u32,
    pub cell_ref: String,
    pub name: Option<String>,
    pub value_text: String,
    pub formula_text: Option<String>,
}

/// ウォッチ ウィンドウ パネル（サイドバー表示用）。
pub(crate) struct WatchWindowPanel {
    pub(crate) selected_id: Option<u64>,
    pub(crate) next_id: u64,
}

impl Default for WatchWindowPanel {
    fn default() -> Self {
        Self {
            selected_id: None,
            next_id: 1,
        }
    }
}

impl SlideEditor {
    // =========================================================================
    // 数式の検証（Evaluate Formula）
    // =========================================================================

    /// 数式の検証パネルを開く。
    pub(crate) fn open_evaluate_formula(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);

        let Some((col, row)) = self.current_cell() else { return };
        let current_slide = self.current_slide;
        let book_view = BookView::new(self.presentation());
        let sheet_idx = book_view.index_of_slide(current_slide).unwrap_or(0);
        let sheet_name = book_view.sheet_name(sheet_idx).to_string();
        let cell_name_str = ibis_doc_model::cell_name(col, row);
        let cell_label = format!("{}!${}${}", sheet_name, cell_name_str.trim_end_matches(|c: char| c.is_ascii_digit()), row + 1);

        let formula_opt = book_view.formula(sheet_idx, row, col).map(|s| s.to_string());

        let panel = match formula_opt {
            Some(formula) => {
                match EvaluateSession::new(&book_view, sheet_idx, row, col) {
                    Some(session) => {
                        let info = session.info(&book_view);
                        EvaluateFormulaPanel {
                            session: Some(session),
                            info,
                            target_cell_label: cell_label,
                            message: None,
                        }
                    }
                    None => {
                        EvaluateFormulaPanel {
                            session: None,
                            info: StepInfo {
                                formula_text: format!("={formula}"),
                                underline_range: None,
                                can_step_in: false,
                                step_in_target: None,
                                is_finished: true,
                                result_value: None,
                                depth: 0,
                            },
                            target_cell_label: cell_label,
                            message: Some(text("_xlsx.failed-to-parse-the-formula").to_string()),
                        }
                    }
                }
            }
            None => {
                let cell_val = book_view.value(sheet_idx, row, col);
                let val_str = match &cell_val {
                    ibis_doc_model::CellValue::Empty => text("_xlsx.blank").to_string(),
                    ibis_doc_model::CellValue::Number(n) => ibis_calc::eval::number_to_text(*n),
                    ibis_doc_model::CellValue::Text(t) => t.clone(),
                    ibis_doc_model::CellValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
                    ibis_doc_model::CellValue::Error(e) => e.to_string(),
                };
                EvaluateFormulaPanel {
                    session: None,
                    info: StepInfo {
                        formula_text: val_str,
                        underline_range: None,
                        can_step_in: false,
                        step_in_target: None,
                        is_finished: true,
                        result_value: Some(cell_val),
                        depth: 0,
                    },
                    target_cell_label: cell_label,
                    message: Some(text("_xlsx.the-cell-currently-being-evaluated-does-not-cont").to_string()),
                }
            }
        };

        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.panels.close();
        s.panels.evaluate_formula = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// 数式の検証: 1 ステップ進める（検証）。
    pub(crate) fn evaluate_step(&mut self) {
        let Some(mut session) = self.wb_mut().panels.evaluate_formula.as_mut().and_then(|p| p.session.take()) else { return };
        let book_view = BookView::new(self.presentation());
        session.step(&book_view);
        let info = session.info(&book_view);
        if let Some(p) = self.wb_mut().panels.evaluate_formula.as_mut() {
            p.info = info;
            p.session = Some(session);
        }
    }

    /// 数式の検証: ステップ イン（参照先セルの数式へ潜る）。
    pub(crate) fn evaluate_step_in(&mut self) {
        let Some(mut session) = self.wb_mut().panels.evaluate_formula.as_mut().and_then(|p| p.session.take()) else { return };
        let book_view = BookView::new(self.presentation());
        let _ = session.step_in(&book_view);
        let info = session.info(&book_view);
        if let Some(p) = self.wb_mut().panels.evaluate_formula.as_mut() {
            p.info = info;
            p.session = Some(session);
        }
    }

    /// 数式の検証: ステップ アウト（親数式へ戻る）。
    pub(crate) fn evaluate_step_out(&mut self) {
        let Some(mut session) = self.wb_mut().panels.evaluate_formula.as_mut().and_then(|p| p.session.take()) else { return };
        let book_view = BookView::new(self.presentation());
        let _ = session.step_out(&book_view);
        let info = session.info(&book_view);
        if let Some(p) = self.wb_mut().panels.evaluate_formula.as_mut() {
            p.info = info;
            p.session = Some(session);
        }
    }

    /// 数式の検証: やり直し（最初に戻る）。
    pub(crate) fn evaluate_restart(&mut self) {
        let Some(mut session) = self.wb_mut().panels.evaluate_formula.as_mut().and_then(|p| p.session.take()) else { return };
        let _ = session.restart();
        let book_view = BookView::new(self.presentation());
        let info = session.info(&book_view);
        if let Some(p) = self.wb_mut().panels.evaluate_formula.as_mut() {
            p.info = info;
            p.session = Some(session);
        }
    }

    /// 数式の検証パネル（サイドバー描画用）。
    pub(crate) fn evaluate_formula_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.evaluate_formula.as_ref()?;
        let cell_label = p.target_cell_label.clone();
        let info = p.info.clone();
        let message = p.message.clone();
        let has_session = p.session.is_some();

        let formula_str = &info.formula_text;
        let span = info.underline_range;

        // 式表示要素の構築（下線対象範囲のハイライト付き）
        let formula_display = if let Some((start, end)) = span {
            if start <= end && end <= formula_str.len() {
                let before = &formula_str[..start];
                let middle = &formula_str[start..end];
                let after = &formula_str[end..];
                h_flex()
                    .flex_wrap()
                    .font_family(".AppleSystemUIFontMonospaced")
                    .text_size(px(13.))
                    .child(SharedString::from(before.to_string()))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .border_b_2()
                            .border_color(rgb(0x1a73e8))
                            .bg(rgba(0x1a73e818))
                            .px(px(2.))
                            .child(SharedString::from(middle.to_string())),
                    )
                    .child(SharedString::from(after.to_string()))
            } else {
                h_flex().font_family(".AppleSystemUIFontMonospaced").text_size(px(13.)).child(SharedString::from(formula_str.clone()))
            }
        } else {
            h_flex().font_family(".AppleSystemUIFontMonospaced").text_size(px(13.)).child(SharedString::from(formula_str.clone()))
        };

        let mut body = v_flex()
            .gap_3()
            .px(px(16.))
            .py(px(12.))
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_size(px(12.)).text_color(rgb(0x666666)).child(text("_xlsx.evaluation-cell")))
                    .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(SharedString::from(cell_label))),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_size(px(12.)).text_color(rgb(0x666666)).child(text("_xlsx.formula-and-step-evaluation")))
                    .child(
                        div()
                            .p(px(8.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(rgb(0xd0d0d0))
                            .bg(rgb(0xf8f9fa))
                            .child(formula_display),
                    ),
            );

        if let Some(target) = &info.step_in_target {
            if info.can_step_in {
                body = body.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(0x1a73e8))
                        .child(format!("{} {} ({})", text("_xlsx.reference"), target, text("_xlsx.step-in-available"))),
                );
            }
        }

        if let Some(msg) = message {
            body = body.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0xd93025))
                    .child(SharedString::from(msg)),
            );
        }

        if has_session {
            let is_finished = info.is_finished;
            let can_step_in = info.can_step_in;
            let depth = info.depth;

            let mut btn_row = h_flex().gap_2();

            // 「検証」ボタン
            let mut eval_btn = Button::new("eval-step").small().label(text("_xlsx.evaluate"));
            if is_finished {
                eval_btn = eval_btn.disabled(true);
            }
            btn_row = btn_row.child(eval_btn.on_click(cx.listener(|this, _, _, cx| {
                this.evaluate_step();
                cx.notify();
            })));

            // 「ステップ イン」ボタン
            let mut step_in_btn = Button::new("eval-step-in").small().label(text("_xlsx.step-in"));
            if !can_step_in {
                step_in_btn = step_in_btn.disabled(true);
            }
            btn_row = btn_row.child(step_in_btn.on_click(cx.listener(|this, _, _, cx| {
                this.evaluate_step_in();
                cx.notify();
            })));

            // 「ステップ アウト」ボタン
            let mut step_out_btn = Button::new("eval-step-out").small().label(text("_xlsx.step-out"));
            if depth == 0 {
                step_out_btn = step_out_btn.disabled(true);
            }
            btn_row = btn_row.child(step_out_btn.on_click(cx.listener(|this, _, _, cx| {
                this.evaluate_step_out();
                cx.notify();
            })));

            // 「やり直し」ボタン
            btn_row = btn_row.child(
                Button::new("eval-restart")
                    .small()
                    .ghost()
                    .label(text("xlsx.eval.restart"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.evaluate_restart();
                        cx.notify();
                    })),
            );

            body = body.child(btn_row);
        }

        body = body.child(
            h_flex().child(
                Button::new("eval-close")
                    .small()
                    .ghost()
                    .label(text("_shell.close"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(s) = this.sheet_state_mut() {
                            s.panels.evaluate_formula = None;
                        }
                        this.sidebar_open = false;
                        cx.notify();
                    })),
            ),
        );

        Some(SidebarPanel {
            title: text("_xlsx.evaluate-formula").to_string().into(),
            body: body.into_any_element(),
        })
    }

    // =========================================================================
    // ウォッチ ウィンドウ（Watch Window）
    // =========================================================================

    /// ウォッチ ウィンドウ パネルを開く。
    pub(crate) fn open_watch_window(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        self.refresh_watches();

        let panel = WatchWindowPanel::default();
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.panels.close();
        s.panels.watch_window = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// 現在選択中のセルをウォッチ式に追加する。
    pub(crate) fn add_watch_cell(&mut self) {
        let Some((col, row)) = self.current_cell() else { return };
        let current_slide = self.current_slide;

        // 1. presentation から必要な情報を抽出
        let (sheet_name, cell_ref, name, value_text, formula_text) = {
            let book_view = BookView::new(self.presentation());
            let sheet_idx = book_view.index_of_slide(current_slide).unwrap_or(0);
            let sheet_name = book_view.sheet_name(sheet_idx).to_string();

            let cell_name_str = ibis_doc_model::cell_name(col, row);
            let col_letters = cell_name_str.trim_end_matches(|c: char| c.is_ascii_digit());
            let cell_ref = format!("${}${}", col_letters, row + 1);

            let name = self.presentation().workbook.as_ref().and_then(|wb| {
                wb.names.iter().find(|n| {
                    if n.hidden || n.name.starts_with("_xlnm.") { return false; }
                    if n.sheet.is_some() && n.sheet != Some(sheet_idx) { return false; }
                    let formula = n.formula.trim().trim_start_matches('=');
                    formula.contains(&format!("${}${}", col_letters, row + 1)) || formula.contains(&cell_name_str)
                }).map(|n| n.name.clone())
            });

            let cell_val = book_view.value(sheet_idx, row, col);
            let value_text = match cell_val {
                ibis_doc_model::CellValue::Empty => String::new(),
                ibis_doc_model::CellValue::Number(n) => ibis_calc::eval::number_to_text(n),
                ibis_doc_model::CellValue::Text(t) => t,
                ibis_doc_model::CellValue::Bool(b) => if b { "TRUE".into() } else { "FALSE".into() },
                ibis_doc_model::CellValue::Error(e) => e.to_string(),
            };
            let formula_text = book_view.formula(sheet_idx, row, col).map(|f| format!("={f}"));
            (sheet_name, cell_ref, name, value_text, formula_text)
        };

        // 2. wb_mut にエントリを追加
        let s = self.wb_mut();
        if s.panels.watches.iter().any(|w| w.slide_id == current_slide && w.col == col && w.row == row) {
            return;
        }

        let next_id = s.panels.watch_window.as_ref().map_or(1, |p| p.next_id);
        if let Some(p) = s.panels.watch_window.as_mut() {
            p.next_id += 1;
        }

        let entry = WatchEntry {
            id: next_id,
            slide_id: current_slide,
            sheet_name,
            col,
            row,
            cell_ref,
            name,
            value_text,
            formula_text,
        };

        s.panels.watches.push(entry);
    }

    /// ウォッチ項目を削除する。
    pub(crate) fn delete_watch(&mut self, id: u64) {
        let s = self.wb_mut();
        s.panels.watches.retain(|w| w.id != id);
        if let Some(p) = s.panels.watch_window.as_mut() {
            if p.selected_id == Some(id) {
                p.selected_id = None;
            }
        }
    }

    /// すべてのウォッチ項目を削除する。
    pub(crate) fn clear_all_watches(&mut self) {
        let s = self.wb_mut();
        s.panels.watches.clear();
        if let Some(p) = s.panels.watch_window.as_mut() {
            p.selected_id = None;
        }
    }

    /// ウォッチ項目のセルへジャンプ（該当シートを選択し、セルを選択）。
    pub(crate) fn jump_to_watch(&mut self, id: u64) {
        let entry_opt = self.sheet_state().and_then(|s| s.panels.watches.iter().find(|w| w.id == id).cloned());
        let Some(entry) = entry_opt else { return };

        // シート切り替え
        self.current_slide = entry.slide_id;
        // セル選択
        self.set_cell(entry.col, entry.row);
    }

    /// ウォッチ一覧の全セルの最新値・数式を更新する。
    pub(crate) fn refresh_watches(&mut self) {
        let updates: Vec<(u64, String, String, Option<String>)> = {
            let book_view = BookView::new(self.presentation());
            let Some(watches) = self.sheet_state().map(|s| &s.panels.watches) else { return };
            watches.iter().map(|entry| {
                let (sheet_name, value_text, formula_text) = if let Some(sheet_idx) = book_view.index_of_slide(entry.slide_id) {
                    let name = book_view.sheet_name(sheet_idx).to_string();
                    let cell_val = book_view.value(sheet_idx, entry.row, entry.col);
                    let val_str = match cell_val {
                        ibis_doc_model::CellValue::Empty => String::new(),
                        ibis_doc_model::CellValue::Number(n) => ibis_calc::eval::number_to_text(n),
                        ibis_doc_model::CellValue::Text(t) => t,
                        ibis_doc_model::CellValue::Bool(b) => if b { "TRUE".into() } else { "FALSE".into() },
                        ibis_doc_model::CellValue::Error(e) => e.to_string(),
                    };
                    let formula_str = book_view.formula(sheet_idx, entry.row, entry.col).map(|f| format!("={f}"));
                    (name, val_str, formula_str)
                } else {
                    (entry.sheet_name.clone(), entry.value_text.clone(), entry.formula_text.clone())
                };
                (entry.id, sheet_name, value_text, formula_text)
            }).collect()
        };

        let s = self.wb_mut();
        for (id, sheet_name, value_text, formula_text) in updates {
            if let Some(entry) = s.panels.watches.iter_mut().find(|w| w.id == id) {
                entry.sheet_name = sheet_name;
                entry.value_text = value_text;
                entry.formula_text = formula_text;
            }
        }
    }

    /// ウォッチ ウィンドウ パネル（サイドバー描画用）。
    pub(crate) fn watch_window_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let selected_id = self.sheet_state()?.panels.watch_window.as_ref()?.selected_id;
        let watches = self.sheet_state()?.panels.watches.clone();

        let mut rows = v_flex().gap_1();

        // テーブルヘッダー
        rows = rows.child(
            h_flex()
                .gap_2()
                .py(px(4.))
                .border_b_1()
                .border_color(rgb(0xe0e0e0))
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(0x666666))
                .child(div().w(px(55.)).child(text("_xlsx.sheet")))
                .child(div().w(px(50.)).child(text("_shell.name")))
                .child(div().w(px(45.)).child(text("fix.phonetic.cell")))
                .child(div().w(px(55.)).child(text("_xlsx.values")))
                .child(div().flex_1().child(text("_xlsx.equation"))),
        );

        // データ行
        for item in &watches {
            let id = item.id;
            let is_sel = selected_id == Some(id);
            let name_str = item.name.as_deref().unwrap_or("-");
            let formula_str = item.formula_text.as_deref().unwrap_or("-");

            let mut row_el = h_flex()
                .gap_2()
                .py(px(4.))
                .px(px(2.))
                .rounded(px(4.))
                .text_size(px(12.))
                .child(div().w(px(55.)).truncate().child(SharedString::from(item.sheet_name.clone())))
                .child(div().w(px(50.)).truncate().child(SharedString::from(name_str.to_string())))
                .child(div().w(px(45.)).font_family(".AppleSystemUIFontMonospaced").child(SharedString::from(item.cell_ref.clone())))
                .child(div().w(px(55.)).truncate().child(SharedString::from(item.value_text.clone())))
                .child(div().flex_1().font_family(".AppleSystemUIFontMonospaced").truncate().child(SharedString::from(formula_str.to_string())));

            if is_sel {
                row_el = row_el.bg(rgba(0x1a73e818));
            } else {
                row_el = row_el.hover(|h| h.bg(rgba(0x00000008)));
            }

            rows = rows.child(
                div()
                    .cursor_pointer()
                    .child(row_el)
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.sheet_state_mut().and_then(|s| s.panels.watch_window.as_mut()) {
                            p.selected_id = Some(id);
                        }
                        this.jump_to_watch(id);
                        cx.notify();
                    })),
            );
        }

        if watches.is_empty() {
            rows = rows.child(
                div()
                    .py(px(16.))
                    .text_size(px(12.))
                    .text_color(rgb(0x999999))
                    .child(text("_xlsx.no-watch-cells-yet-click-add-watch-to-add")),
            );
        }

        let body = v_flex()
            .gap_3()
            .px(px(16.))
            .py(px(12.))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("watch-add")
                            .small()
                            .label(text("_xlsx.add-watch"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.add_watch_cell();
                                cx.notify();
                            })),
                    )
                    .child({
                        let mut del_btn = Button::new("watch-delete").small().label(text("_shell.delete"));
                        if selected_id.is_none() {
                            del_btn = del_btn.disabled(true);
                        }
                        del_btn.on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(id) = selected_id {
                                this.delete_watch(id);
                                cx.notify();
                            }
                        }))
                    })
                    .child(
                        Button::new("watch-clear-all")
                            .small()
                            .ghost()
                            .label(text("_xlsx.delete-all"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.clear_all_watches();
                                cx.notify();
                            })),
                    ),
            )
            .child(rows);

        Some(SidebarPanel {
            title: text("_xlsx.watch-window").to_string().into(),
            body: body.into_any_element(),
        })
    }
}
