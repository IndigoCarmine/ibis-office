// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 選択ウィンドウ (配置 > 選択ウィンドウ / 表示 > 選択ウィンドウ):
//! ワークシート上の図形・画像・グラフの一覧を表示し、選択、表示/非表示（目のアイコン）、
//! 名前の変更、順序の入れ替え（↑ / ↓）を行うサイドバーパネル。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use ibis_doc_model::{Command, Shape, ShapeId, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;

/// The pane, while open: the shape being renamed and its input field.
#[derive(Default)]
pub(crate) struct SelectionPanePanel {
    pub(crate) renaming: Option<(ShapeId, Entity<InputState>)>,
    _subscription: Option<Subscription>,
}

impl SlideEditor {
    /// 選択ウィンドウの表示 / 非表示トグル.
    pub(crate) fn toggle_xlsx_selection_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_open = self.wb().panels.selection_pane.is_some();
        if is_open {
            self.close_xlsx_selection_pane(window, cx);
        } else {
            self.open_xlsx_selection_pane(window, cx);
        }
    }

    /// 選択ウィンドウを開く.
    pub(crate) fn open_xlsx_selection_pane(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let state = self.wb_mut();
        state.panels.close();
        state.panels.selection_pane = Some(SelectionPanePanel::default());
        self.sidebar_open = true;
        cx.notify();
    }

    /// 選択ウィンドウを閉じる.
    pub(crate) fn close_xlsx_selection_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.wb_mut().panels.selection_pane = None;
        self.sidebar_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// 図形の表示名を取得（設定された名前、または種類に応じた連番名）.
    pub(crate) fn xlsx_shape_label(&self, shape: &Shape, index: usize) -> String {
        let slide = self.slide();
        if let Some(name) = slide.shape_names.get(&shape.id) {
            return name.clone();
        }
        match &shape.kind {
            ShapeKind::TextBox(_) => crate::shell::i18n::format("_xlsx.text-box-n", &[("n", &index)]),
            ShapeKind::Picture(_) => crate::shell::i18n::format("_xlsx.picture-n", &[("n", &index)]),
            ShapeKind::Chart(_) => crate::shell::i18n::format("_xlsx.chart-n", &[("n", &index)]),
            ShapeKind::Line(_) => crate::shell::i18n::format("_xlsx.line-n", &[("n", &index)]),
            ShapeKind::Auto(_) => crate::shell::i18n::format("pptx.selection-pane.shape-n", &[("n", &index)]),
            _ => crate::shell::i18n::format("_xlsx.object-n", &[("n", &index)]),
        }
    }

    /// 図形の表示 / 非表示切り替え（目のアイコン）.
    pub(crate) fn toggle_sheet_shape_hidden(&mut self, shape_id: ShapeId, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let is_hidden = self.slide().hidden_shapes.contains(&shape_id);
        let hidden = !is_hidden;
        if hidden && self.selection == Some(shape_id) {
            self.selection = None;
        }
        self.run_sheet(vec![Command::SetShapeHidden { slide, shape: shape_id, hidden }], cx);
    }

    /// シート上の全図形をすべて表示 / すべて非表示.
    pub(crate) fn set_all_sheet_shapes_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let s = self.slide();
        // shapes[0] is the worksheet itself, skip index 0
        let commands: Vec<Command> = s
            .shapes
            .iter()
            .skip(1)
            .filter(|sh| !s.is_attached(sh.id) && s.hidden_shapes.contains(&sh.id) != hidden)
            .map(|sh| Command::SetShapeHidden { slide, shape: sh.id, hidden })
            .collect();
        if hidden {
            self.selection = None;
        }
        self.run_sheet(commands, cx);
    }

    /// 名前のインライン変更を開始.
    pub(crate) fn start_sheet_shape_renaming(&mut self, shape_id: ShapeId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pos) = self.slide().shapes.iter().skip(1).position(|s| s.id == shape_id) else { return };
        let shape = &self.slide().shapes[pos + 1];
        let name = self.xlsx_shape_label(shape, pos + 1);
        let field = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let subscription = cx.subscribe_in(&field, window, move |e: &mut Self, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                e.finish_sheet_shape_renaming(window, cx);
            }
        });
        field.update(cx, |f, cx| f.focus(window, cx));
        if let Some(p) = self.wb_mut().panels.selection_pane.as_mut() {
            p.renaming = Some((shape_id, field));
            p._subscription = Some(subscription);
        }
        cx.notify();
    }

    /// 名前のインライン変更を確定.
    pub(crate) fn finish_sheet_shape_renaming(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, field)) = self.wb_mut().panels.selection_pane.as_mut().and_then(|p| p.renaming.take()) else { return };
        let name = field.read(cx).value().trim().to_string();
        let slide = self.current_slide;
        if !name.is_empty() && self.slide().shape_names.get(&id) != Some(&name) && self.slide().shape(id).is_some() {
            self.run_sheet(vec![Command::SetShapeName { slide, shape: id, name }], cx);
        }
    }

    /// 選択ウィンドウのサイドバーパネルUIを生成.
    pub(crate) fn xlsx_selection_pane_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let is_open = self.sheet_state().is_some_and(|s| s.panels.selection_pane.is_some());
        if !is_open {
            return None;
        }

        let slide = self.slide();
        // shapes[0] is sheet, get shapes from 1.. in reverse (frontmost at top)
        let floating_shapes: Vec<(usize, Shape)> = slide.shapes.iter().enumerate().skip(1).map(|(i, s)| (i, s.clone())).collect();
        let renaming_id = self.sheet_state().and_then(|s| s.panels.selection_pane.as_ref()).and_then(|p| p.renaming.as_ref()).map(|(id, _)| *id);
        let renaming_field = self.sheet_state().and_then(|s| s.panels.selection_pane.as_ref()).and_then(|p| p.renaming.as_ref()).map(|(_, f)| f.clone());

        let mut body = v_flex().gap_2().px(px(16.)).py(px(12.));

        // 操作ボタン: すべて表示 / すべて非表示
        let actions = h_flex()
            .gap_2()
            .child(
                Button::new("xlsx-selpane-show-all")
                    .small()
                    .ghost()
                    .label(crate::shell::i18n::text("pptx.selection-pane.show-all"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_all_sheet_shapes_hidden(false, cx);
                    })),
            )
            .child(
                Button::new("xlsx-selpane-hide-all")
                    .small()
                    .ghost()
                    .label(crate::shell::i18n::text("pptx.selection-pane.hide-all"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_all_sheet_shapes_hidden(true, cx);
                    })),
            );
        body = body.child(actions);

        if floating_shapes.is_empty() {
            body = body.child(
                div()
                    .py(px(16.))
                    .text_size(px(13.))
                    .text_color(rgb(0x7f7f7f))
                    .child(crate::shell::i18n::text("_xlsx.there-are-no-shapes-or-pictures")),
            );
        } else {
            let mut list = v_flex().gap_1();
            // Frontmost shapes first (reverse order)
            for (index, shape) in floating_shapes.into_iter().rev() {
                let id = shape.id;
                let is_selected = self.selected_ids().contains(&id);
                let is_hidden = slide.hidden_shapes.contains(&id);
                let label = self.xlsx_shape_label(&shape, index);

                let is_renaming = renaming_id == Some(id);

                let mut row = h_flex()
                    .gap_2()
                    .items_center()
                    .px(px(6.))
                    .py(px(4.))
                    .rounded(px(4.))
                    .when(is_selected, |r| r.bg(rgb(0xe0e8f6)));

                // 目のアイコン（表示/非表示）
                let eye_icon = if is_hidden { Material("visibility_off") } else { Material("visibility") };
                let eye_btn = Button::new(format!("xlsx-selpane-eye-{index}"))
                    .small()
                    .ghost()
                    .icon(Icon::new(eye_icon))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_sheet_shape_hidden(id, cx);
                    }));
                row = row.child(eye_btn);

                // 図形種別アイコン
                let kind_icon = match &shape.kind {
                    ShapeKind::TextBox(_) => Material("text_fields"),
                    ShapeKind::Picture(_) => Material("image"),
                    ShapeKind::Chart(_) => Material("bar_chart"),
                    ShapeKind::Line(_) => Material("horizontal_rule"),
                    _ => Material("category"),
                };
                row = row.child(Icon::new(kind_icon).size(px(16.)));

                // 名前またはリネーム入力欄
                if is_renaming && let Some(field) = &renaming_field {
                    let input_field = field.clone();
                    row = row.child(div().flex_1().child(Input::new(&input_field).small()));
                } else {
                    let row_label = label.clone();
                    let name_clickable = div()
                        .flex_1()
                        .text_size(px(13.))
                        .when(is_hidden, |d| d.text_color(rgb(0x9e9e9e)))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                            if e.modifiers.shift || e.modifiers.platform || e.modifiers.control {
                                this.toggle_select_sheet_shape(id);
                            } else {
                                this.select_sheet_shape(id);
                            }
                            cx.notify();
                        }))
                        .child(row_label);
                    row = row.child(name_clickable);

                    // 鉛筆アイコン（名前変更）
                    let edit_btn = Button::new(format!("xlsx-selpane-rename-{index}"))
                        .small()
                        .ghost()
                        .icon(Icon::new(Material("edit")))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.start_sheet_shape_renaming(id, window, cx);
                        }));
                    row = row.child(edit_btn);
                }

                // 順序変更ボタン: ↑ (前面へ) / ↓ (背面へ)
                let up_btn = Button::new(format!("xlsx-selpane-up-{index}"))
                    .small()
                    .ghost()
                    .icon(Icon::new(Material("arrow_upward")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_sheet_shape(id);
                        this.order_sheet_shape(crate::pptx::editing::Order::Forward, cx);
                    }));
                let down_btn = Button::new(format!("xlsx-selpane-down-{index}"))
                    .small()
                    .ghost()
                    .icon(Icon::new(Material("arrow_downward")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_sheet_shape(id);
                        this.order_sheet_shape(crate::pptx::editing::Order::Backward, cx);
                    }));
                row = row.child(up_btn).child(down_btn);

                list = list.child(row);
            }
            body = body.child(list);
        }

        Some(SidebarPanel {
            title: crate::shell::i18n::text("fix.goto-special.select"),
            body: body.into_any_element(),
        })
    }
}
