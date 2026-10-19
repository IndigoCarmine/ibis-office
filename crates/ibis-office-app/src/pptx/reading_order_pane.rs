// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 読み上げ順序ペイン (校閲 > 読み上げ順序 / アクセシビリティ チェック: PPTX-ACCESS-004):
//! スライド上のオブジェクトがスクリーンリーダーによって読み上げられる順序（上から下へ）を
//! 表示・確認・並べ替え可能にする。装飾用オブジェクトのスキップ表示や、
//! 視覚的配置（タイトル最優先、上から下・左から右）に基づく自動整列もサポート。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Command, ShapeId, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;

/// The Reading Order pane state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReadingOrderPane;

impl SlideEditor {
    /// 読み上げ順序ペインの開閉トグル
    pub(crate) fn toggle_reading_order_pane(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.reading_order_pane = match s.reading_order_pane.take() {
                Some(_) => None,
                None => Some(ReadingOrderPane),
            };
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 読み上げ順序ペインを開く
    pub(crate) fn open_reading_order_pane(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.reading_order_pane = Some(ReadingOrderPane);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 読み上げ順序（インデックス）を 1 つ移動する
    /// `earlier == true`: より早く読まれる（上へ移動、インデックス - 1）
    /// `earlier == false`: より遅く読まれる（下へ移動、インデックス + 1）
    pub(crate) fn move_reading_order(&mut self, shape: ShapeId, earlier: bool, cx: &mut Context<Self>) {
        let slide_id = self.current_slide;
        let slide = self.slide();
        let Some(curr_idx) = slide.shape_index(shape) else { return };
        let new_idx = if earlier {
            if curr_idx == 0 { return; }
            curr_idx - 1
        } else {
            if curr_idx + 1 >= slide.shapes.len() { return; }
            curr_idx + 1
        };
        self.set_selection(vec![shape]);
        self.run(vec![Command::MoveShape { slide: slide_id, shape, index: new_idx }], cx);
    }

    /// 視覚的配置に基づいて論理的な読み上げ順序に自動整列する
    /// 1. タイトルプレースホルダーが最初
    /// 2. サブタイトル / プレースホルダー
    /// 3. 一般図形・コンテンツ: Y座標昇順（上から下）、おおむね同一行ならX座標昇順（左から右）
    /// 4. 装飾用オブジェクト（読み上げ不要）は末尾へ
    pub(crate) fn auto_reading_order(&mut self, cx: &mut Context<Self>) {
        let slide_id = self.current_slide;
        let slide = self.slide().clone();
        if slide.shapes.len() <= 1 { return; }

        let mut ranked: Vec<(usize, ShapeId)> = slide
            .shapes
            .iter()
            .enumerate()
            .map(|(original_idx, sh)| {
                let is_decorative = slide.decorative_shapes.contains(&sh.id);
                let ph = slide.placeholders.get(&sh.id).map(|p| p.placeholder.normalized());
                let is_title = ph == Some("title") || ph == Some("ctrTitle");
                let is_sub_title = ph == Some("subTitle");

                // スコア計算:
                // 装飾用: 1_000_000_000 + original_idx
                // タイトル: 0
                // サブタイトル: 10_000
                // 通常: 100_000 + (Y / 200_000) * 10_000 + (X / 200_000)
                let score: i64 = if is_decorative {
                    1_000_000_000 + original_idx as i64
                } else if is_title {
                    0
                } else if is_sub_title {
                    10_000
                } else {
                    let y = (sh.frame.y / 200_000).max(0);
                    let x = (sh.frame.x / 200_000).max(0);
                    100_000 + y * 10_000 + x
                };
                (score as usize, sh.id)
            })
            .collect();

        // 安定ソート
        ranked.sort_by_key(|&(score, _)| score);

        let target_order: Vec<ShapeId> = ranked.into_iter().map(|(_, id)| id).collect();
        let current_order: Vec<ShapeId> = slide.shapes.iter().map(|s| s.id).collect();
        if target_order == current_order { return; }

        // 現在の shapes から target_order の順序へ並べ替える MoveShape の列を生成
        let mut sim = current_order;
        let mut commands = Vec::new();
        for (target_idx, &id) in target_order.iter().enumerate() {
            let current_pos = sim.iter().position(|&x| x == id).unwrap();
            if current_pos != target_idx {
                sim.remove(current_pos);
                sim.insert(target_idx, id);
                commands.push(Command::MoveShape { slide: slide_id, shape: id, index: target_idx });
            }
        }

        if !commands.is_empty() {
            self.run(commands, cx);
        }
    }

    /// 図形の「装飾としてマーク（読み上げスキップ）」フラグをトグル切り替えする
    pub(crate) fn toggle_shape_decorative(&mut self, shape: ShapeId, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let is_decorative = self.slide().decorative_shapes.contains(&shape);
        self.set_selection(vec![shape]);
        self.run(vec![Command::SetShapeDecorative { slide, shape, decorative: !is_decorative }], cx);
    }

    /// Context menu entries for a shape row in the Reading Order Pane.
    pub(crate) fn reading_order_context_menu_entries(&self, id: ShapeId) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let slide = self.slide();
        let idx = slide.shape_index(id).unwrap_or(0);
        let count = slide.shapes.len();
        let is_decorative = slide.decorative_shapes.contains(&id);
        vec![
            MenuItem::tr("pptx.reading-order-pane.move-up-read-earlier")
                .icon("arrow_upward")
                .disabled(idx == 0)
                .on(move |e, _, cx| e.move_reading_order(id, true, cx))
                .into(),
            MenuItem::tr("pptx.reading-order-pane.move-down-read-later")
                .icon("arrow_downward")
                .disabled(idx + 1 >= count)
                .on(move |e, _, cx| e.move_reading_order(id, false, cx))
                .into(),
            MenuItem::tr("pptx.reading-order-pane.auto-order-top-bottom")
                .icon("format_list_numbered")
                .disabled(count <= 1)
                .on(|e, _, cx| e.auto_reading_order(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.alt-text.decorative")
                .icon("visibility_off")
                .checked(is_decorative)
                .on(move |e, _, cx| e.toggle_shape_decorative(id, cx))
                .into(),
            MenuItem::tr("pptx.alt-text")
                .icon("accessibility")
                .on(move |e, w, cx| {
                    e.set_selection(vec![id]);
                    e.open_alt_text_panel(w, cx);
                })
                .into(),
        ]
    }

    /// Open context menu for shape `id` in the Reading Order Pane at pointer `position`.
    pub(crate) fn open_reading_order_context_menu(&mut self, id: ShapeId, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_ids().contains(&id) {
            self.set_selection(vec![id]);
            cx.notify();
        }
        let entries = self.reading_order_context_menu_entries(id);
        self.open_popup_entries(entries, position, window, cx);
    }

    /// 読み上げ順序サイドバーパネルの描画
    pub(crate) fn reading_order_pane(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.pptx()?.reading_order_pane?;
        let t = crate::shell::i18n::text;
        let slide = self.slide().clone();
        let selected = self.selected_ids();
        let shapes = &slide.shapes;
        let count = shapes.len();

        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.));

        // 説明ヘッダー
        body = body.child(
            div()
                .text_sm()
                .text_color(rgb(0x5f6368))
                .child(t("pptx.reading-order-pane.order-which-objects-will-read")),
        );

        if shapes.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.selection-pane.no-shapes-slide")));
        } else {
            let mut list = v_flex().gap_1();
            for (i, shape) in shapes.iter().enumerate() {
                let id = shape.id;
                let is_decorative = slide.decorative_shapes.contains(&id);
                let is_selected = selected.contains(&id);

                // アイコンの選定
                let ph = slide.placeholders.get(&id).map(|p| p.placeholder.normalized());
                let icon_name = if ph == Some("title") || ph == Some("ctrTitle") {
                    "title"
                } else {
                    match &shape.kind {
                        ShapeKind::Auto(a) if a.text.is_some() => "text_fields",
                        ShapeKind::TextBox(_) => "text_fields",
                        ShapeKind::Picture(_) => "image",
                        ShapeKind::Table(_) => "table_chart",
                        ShapeKind::Chart(_) => "bar_chart",
                        _ => "shapes",
                    }
                };

                let label = self.shape_label(id);
                let n = id.0 as usize;

                let row = h_flex()
                    .id(("reading-order-row", n))
                    .gap_1p5()
                    .p_1()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .when(is_selected, |d| d.bg(rgb(0xd3e3fd)))
                    .on_click(cx.listener(move |e, _, _, cx| {
                        e.set_selection(vec![id]);
                        cx.notify();
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |e, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        e.open_reading_order_context_menu(id, event.position, window, cx);
                    }))
                    // 順番番号バッジ
                    .child(
                        div()
                            .w(px(20.))
                            .h(px(20.))
                            .rounded_full()
                            .bg(if is_selected { rgb(0x1a73e8) } else { rgb(0xe8eaed) })
                            .text_color(if is_selected { rgb(0xffffff) } else { rgb(0x3c4043) })
                            .text_xs()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(format!("{}", i + 1)),
                    )
                    // アイコン
                    .child(Icon::new(Material(icon_name)).size(px(16.)).text_color(if is_decorative { rgb(0x9aa0a6) } else { rgb(0x5f6368) }))
                    // ラベル & 装飾用バッジ
                    .child(
                        h_flex()
                            .flex_1()
                            .gap_1()
                            .items_center()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_sm()
                                    .truncate()
                                    .when(is_decorative, |d| d.text_color(rgb(0x9aa0a6)).line_through())
                                    .child(label),
                            )
                            .when(is_decorative, |d| {
                                d.child(
                                    div()
                                        .px_1()
                                        .rounded(px(2.))
                                        .bg(rgb(0xf1f3f4))
                                        .text_color(rgb(0x70757a))
                                        .text_xs()
                                        .child(t("pptx.reading-order-pane.decorative-skipped")),
                                )
                            }),
                    )
                    // 装飾マーク切り替え・↑ / ↓ ボタン
                    .child(
                        Button::new(("ro-decorative", n))
                            .small()
                            .ghost()
                            .icon(Icon::new(Material(if is_decorative { "visibility_off" } else { "visibility" })))
                            .tooltip(t("pptx.alt-text.decorative"))
                            .on_click(cx.listener(move |e, _, _, cx| e.toggle_shape_decorative(id, cx))),
                    )
                    .child(
                        Button::new(("ro-up", n))
                            .small()
                            .ghost()
                            .label("↑")
                            .tooltip(t("pptx.reading-order-pane.move-up-read-earlier"))
                            .disabled(i == 0)
                            .on_click(cx.listener(move |e, _, _, cx| e.move_reading_order(id, true, cx))),
                    )
                    .child(
                        Button::new(("ro-down", n))
                            .small()
                            .ghost()
                            .label("↓")
                            .tooltip(t("pptx.reading-order-pane.move-down-read-later"))
                            .disabled(i + 1 >= count)
                            .on_click(cx.listener(move |e, _, _, cx| e.move_reading_order(id, false, cx))),
                    );

                list = list.child(row);
            }
            body = body.child(list);
        }

        // アクションバー
        body = body.child(
            h_flex()
                .gap_1()
                .pt_2()
                .child(
                    Button::new("ro-auto-order")
                        .small()
                        .label(t("pptx.reading-order-pane.auto-order-top-bottom"))
                        .tooltip(t("pptx.reading-order-pane.automatically-orders-reading-order"))
                        .on_click(cx.listener(|e, _, _, cx| e.auto_reading_order(cx))),
                )
                .child(
                    Button::new("ro-to-selection-pane")
                        .small()
                        .ghost()
                        .label(t("pptx.reading-order-pane.selection-pane"))
                        .tooltip(t("pptx.reading-order-pane.open-selection-pane-manage-z"))
                        .on_click(cx.listener(|e, _, _, cx| {
                            e.toggle_selection_pane(cx);
                        })),
                ),
        );

        Some(SidebarPanel { title: t("pptx.reading-order-pane.reading-order"), body: body.into_any_element() })
    }
}
