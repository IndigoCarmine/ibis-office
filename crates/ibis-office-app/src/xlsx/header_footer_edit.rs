// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! カスタム ヘッダー・フッターの編集 UI (XLSX-SEC-016):
//! 左・中・右セクションへの文字列・フィールドコード入力、
//! ページ種別（通常・奇数 / 偶数 / 先頭）切り替え、
//! ページ設定（ヘッダー・フッターのオプション）との連携。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;

use super::header_footer::{join_sections, split_sections};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 編集対象（ヘッダー または フッター）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HfTarget {
    Header,
    Footer,
}

/// 編集対象のページ種別
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum HfPageKind {
    #[default]
    Odd, // 通常・奇数ページ
    Even, // 偶数ページ
    First, // 先頭ページ
}

/// アクティブな入力セクション（フィールドコード挿入先）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum HfActiveSection {
    Left,
    #[default]
    Center,
    Right,
}

/// ヘッダー・フッター編集パネルの状態
pub(crate) struct HeaderFooterEditPanel {
    pub(crate) target: HfTarget,
    pub(crate) page_kind: HfPageKind,
    pub(crate) active_section: HfActiveSection,
    pub(crate) left: Entity<InputState>,
    pub(crate) center: Entity<InputState>,
    pub(crate) right: Entity<InputState>,
    pub(crate) code: Entity<InputState>,
}

const FIELD_BUTTONS: &[(&str, &str, &str)] = &[
    ("hf-field-page", "xlsx.hf.insert-page", "&P"),
    ("hf-field-pages", "xlsx.hf.insert-pages", "&N"),
    ("hf-field-date", "xlsx.hf.insert-date", "&D"),
    ("hf-field-time", "xlsx.hf.insert-time", "&T"),
    ("hf-field-file", "xlsx.hf.insert-file", "&F"),
    ("hf-field-sheet", "xlsx.hf.insert-sheet", "&A"),
];

impl SlideEditor {
    /// カスタム ヘッダー／フッター編集画面を開く
    pub(crate) fn open_header_footer_edit(&mut self, target: HfTarget, window: &mut Window, cx: &mut Context<Self>) {
        let print = self.current_print().unwrap_or_default();
        let hf = &print.header_footer;
        let raw_code = match target {
            HfTarget::Header => hf.odd_header.as_deref().unwrap_or(""),
            HfTarget::Footer => hf.odd_footer.as_deref().unwrap_or(""),
        };
        let (left_str, center_str, right_str) = split_sections(raw_code);
        let left = cx.new(|cx| InputState::new(window, cx).default_value(left_str));
        let center = cx.new(|cx| InputState::new(window, cx).default_value(center_str));
        let right = cx.new(|cx| InputState::new(window, cx).default_value(right_str));
        let code = cx.new(|cx| InputState::new(window, cx).default_value(raw_code.to_string()));

        let panel = HeaderFooterEditPanel {
            target,
            page_kind: HfPageKind::Odd,
            active_section: HfActiveSection::Center,
            left,
            center,
            right,
            code,
        };
        if let Some(s) = self.sheet_state_mut() {
            s.header_footer_edit = Some(panel);
            s.page_setup = true;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 編集中のヘッダー／フッターのコード文字列をモデルに適用する
    pub(crate) fn apply_header_footer_edit(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()) else { return };
        let target = panel.target;
        let kind = panel.page_kind;
        let l = panel.left.read(cx).value().to_string();
        let c = panel.center.read(cx).value().to_string();
        let r = panel.right.read(cx).value().to_string();
        let new_code = join_sections(&l, &c, &r);

        self.set_print(
            move |p| {
                let hf = &mut p.header_footer;
                match (target, kind) {
                    (HfTarget::Header, HfPageKind::Odd) => hf.odd_header = new_code,
                    (HfTarget::Header, HfPageKind::Even) => hf.even_header = new_code,
                    (HfTarget::Header, HfPageKind::First) => hf.first_header = new_code,
                    (HfTarget::Footer, HfPageKind::Odd) => hf.odd_footer = new_code,
                    (HfTarget::Footer, HfPageKind::Even) => hf.even_footer = new_code,
                    (HfTarget::Footer, HfPageKind::First) => hf.first_footer = new_code,
                }
            },
            None,
            cx,
        );
    }

    /// コード表示 input を現在の左右中の内容で同期
    fn sync_hf_code_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()) else { return };
        let l = panel.left.read(cx).value().to_string();
        let c = panel.center.read(cx).value().to_string();
        let r = panel.right.read(cx).value().to_string();
        let new_code = join_sections(&l, &c, &r).unwrap_or_default();
        let code_input = panel.code.clone();
        code_input.update(cx, |i, cx| i.set_value(new_code, window, cx));
    }

    /// アクティブな入力セクションにフィールドコードを挿入する
    pub(crate) fn insert_hf_field(&mut self, field_code: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()) else { return };
        let target_input = match panel.active_section {
            HfActiveSection::Left => panel.left.clone(),
            HfActiveSection::Center => panel.center.clone(),
            HfActiveSection::Right => panel.right.clone(),
        };
        let mut cur = target_input.read(cx).value().to_string();
        cur.push_str(field_code);
        target_input.update(cx, |i, cx| {
            i.set_value(cur, window, cx);
        });
        self.apply_header_footer_edit(cx);
        self.sync_hf_code_input(window, cx);
    }

    /// 直接書かれたコード文字列から左右中セクションを再構成して反映
    pub(crate) fn apply_hf_code_direct(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()) else { return };
        let raw = panel.code.read(cx).value().to_string();
        let (l, c, r) = split_sections(&raw);
        let left_input = panel.left.clone();
        let center_input = panel.center.clone();
        let right_input = panel.right.clone();
        left_input.update(cx, |i, cx| i.set_value(l, window, cx));
        center_input.update(cx, |i, cx| i.set_value(c, window, cx));
        right_input.update(cx, |i, cx| i.set_value(r, window, cx));
        self.apply_header_footer_edit(cx);
    }

    /// ページ種別（通常・偶数・先頭）の切り替え
    pub(crate) fn set_hf_page_kind(&mut self, kind: HfPageKind, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_header_footer_edit(cx);
        let Some(target) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()).map(|p| p.target) else { return };
        let print = self.current_print().unwrap_or_default();
        let hf = &print.header_footer;
        let raw_code = match (target, kind) {
            (HfTarget::Header, HfPageKind::Odd) => hf.odd_header.as_deref().unwrap_or(""),
            (HfTarget::Header, HfPageKind::Even) => hf.even_header.as_deref().unwrap_or(""),
            (HfTarget::Header, HfPageKind::First) => hf.first_header.as_deref().unwrap_or(""),
            (HfTarget::Footer, HfPageKind::Odd) => hf.odd_footer.as_deref().unwrap_or(""),
            (HfTarget::Footer, HfPageKind::Even) => hf.even_footer.as_deref().unwrap_or(""),
            (HfTarget::Footer, HfPageKind::First) => hf.first_footer.as_deref().unwrap_or(""),
        };
        let (l, c, r) = split_sections(raw_code);
        let Some(panel) = self.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) else { return };
        panel.page_kind = kind;
        let left_input = panel.left.clone();
        let center_input = panel.center.clone();
        let right_input = panel.right.clone();
        let code_input = panel.code.clone();
        left_input.update(cx, |i, cx| i.set_value(l, window, cx));
        center_input.update(cx, |i, cx| i.set_value(c, window, cx));
        right_input.update(cx, |i, cx| i.set_value(r, window, cx));
        code_input.update(cx, |i, cx| i.set_value(raw_code.to_string(), window, cx));
        cx.notify();
    }

    /// 全セクションをクリア
    pub(crate) fn clear_header_footer_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()) else { return };
        let left_input = panel.left.clone();
        let center_input = panel.center.clone();
        let right_input = panel.right.clone();
        let code_input = panel.code.clone();
        left_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        center_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        right_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        code_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
        self.apply_header_footer_edit(cx);
    }

    /// ヘッダー／フッター編集パネルを閉じ、通常のページ設定に戻る
    pub(crate) fn close_header_footer_edit(&mut self, cx: &mut Context<Self>) {
        self.apply_header_footer_edit(cx);
        if let Some(s) = self.sheet_state_mut() {
            s.header_footer_edit = None;
        }
        cx.notify();
    }

    /// ヘッダー・フッター編集サイドバーパネル
    pub(crate) fn header_footer_edit_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.header_footer_edit.as_ref()?;
        let target = panel.target;
        let page_kind = panel.page_kind;
        let active_section = panel.active_section;
        let left_state = panel.left.clone();
        let center_state = panel.center.clone();
        let right_state = panel.right.clone();
        let code_state = panel.code.clone();

        let print = self.current_print()?;
        let hf = &print.header_footer;

        let title = match target {
            HfTarget::Header => text("xlsx.header-edit.title"),
            HfTarget::Footer => text("xlsx.footer-edit.title"),
        };

        let choice = |id: SharedString, label: &'static str, on: bool, f: Box<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>)>| {
            let b = Button::new(id).small().label(text(label));
            let b = if on { b.primary() } else { b.ghost() };
            b.on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
        };

        // ページ種別切り替えボタン群
        let page_kinds = h_flex()
            .gap_1()
            .items_center()
            .child(choice(
                "hf-kind-odd".into(),
                "xlsx.hf.odd-page",
                page_kind == HfPageKind::Odd,
                Box::new(|this, cx| {
                    // window が必要な場合はダミーで空呼び出しできるヘルパー
                    this.apply_header_footer_edit(cx);
                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
                        p.page_kind = HfPageKind::Odd;
                    }
                    cx.notify();
                }),
            ))
            .child(choice(
                "hf-kind-even".into(),
                "xlsx.hf.even-page",
                page_kind == HfPageKind::Even,
                Box::new(|this, cx| {
                    this.apply_header_footer_edit(cx);
                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
                        p.page_kind = HfPageKind::Even;
                    }
                    cx.notify();
                }),
            ))
            .child(choice(
                "hf-kind-first".into(),
                "xlsx.hf.first-page",
                page_kind == HfPageKind::First,
                Box::new(|this, cx| {
                    this.apply_header_footer_edit(cx);
                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
                        p.page_kind = HfPageKind::First;
                    }
                    cx.notify();
                }),
            ));

        // オプショントグル
        let options_row = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(choice(
                        "hf-opt-diff-first".into(),
                        "xlsx.hf.diff-first",
                        hf.different_first,
                        Box::new(|this, cx| this.set_print(|p| p.header_footer.different_first = !p.header_footer.different_first, None, cx)),
                    ))
                    .child(choice(
                        "hf-opt-diff-odd-even".into(),
                        "xlsx.hf.diff-odd-even",
                        hf.different_odd_even,
                        Box::new(|this, cx| this.set_print(|p| p.header_footer.different_odd_even = !p.header_footer.different_odd_even, None, cx)),
                    )),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(choice(
                        "hf-opt-scale-doc".into(),
                        "xlsx.hf.scale-with-doc",
                        hf.scale_with_doc,
                        Box::new(|this, cx| this.set_print(|p| p.header_footer.scale_with_doc = !p.header_footer.scale_with_doc, None, cx)),
                    ))
                    .child(choice(
                        "hf-opt-align-margins".into(),
                        "xlsx.hf.align-margins",
                        hf.align_with_margins,
                        Box::new(|this, cx| this.set_print(|p| p.header_footer.align_with_margins = !p.header_footer.align_with_margins, None, cx)),
                    )),
            );

        // フィールド挿入ボタン群
        let field_buttons = h_flex().flex_wrap().gap_1().children(FIELD_BUTTONS.iter().map(|&(id, label, code)| {
            Button::new(id).small().label(text(label)).on_click(cx.listener(move |this, _, window, cx| {
                this.insert_hf_field(code, window, cx);
            }))
        }));

        // セクション選択ボタングループ
        let section_selector = h_flex()
            .gap_1()
            .items_center()
            .child(choice(
                "hf-sec-left".into(),
                "xlsx.hf.left-section",
                active_section == HfActiveSection::Left,
                Box::new(|this, cx| {
                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
                        p.active_section = HfActiveSection::Left;
                    }
                    cx.notify();
                }),
            ))
            .child(choice(
                "hf-sec-center".into(),
                "xlsx.hf.center-section",
                active_section == HfActiveSection::Center,
                Box::new(|this, cx| {
                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
                        p.active_section = HfActiveSection::Center;
                    }
                    cx.notify();
                }),
            ))
            .child(choice(
                "hf-sec-right".into(),
                "xlsx.hf.right-section",
                active_section == HfActiveSection::Right,
                Box::new(|this, cx| {
                    if let Some(p) = this.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
                        p.active_section = HfActiveSection::Right;
                    }
                    cx.notify();
                }),
            ));

        let section_block = |label: &'static str, state: &Entity<InputState>, sec: HfActiveSection| {
            let is_active = active_section == sec;
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(div().text_size(px(12.)).font_weight(if is_active { FontWeight::BOLD } else { FontWeight::NORMAL }).child(text(label)))
                        .child(Button::new(format!("sec-focus-{label}")).small().ghost().label(if is_active { "●" } else { "○" }).on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
                                p.active_section = sec;
                            }
                            cx.notify();
                        }))),
                )
                .child(Input::new(state).small())
        };

        let direct_code_row = v_flex()
            .gap_1()
            .child(div().text_size(px(12.)).child(text("xlsx.hf.code-preview")))
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(div().flex_1().child(Input::new(&code_state).small()))
                    .child(Button::new("hf-apply-code").primary().small().label(text("xlsx.hf.apply-code")).on_click(cx.listener(move |this, _, window, cx| {
                        this.apply_hf_code_direct(window, cx);
                    }))),
            );

        let bottom_actions = h_flex()
            .justify_between()
            .items_center()
            .child(Button::new("hf-back").small().ghost().label(text("xlsx.hf.back")).on_click(cx.listener(|this, _, _, cx| this.close_header_footer_edit(cx))))
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(Button::new("hf-clear").small().ghost().label(text("xlsx.hf.clear")).on_click(cx.listener(|this, _, window, cx| this.clear_header_footer_edit(window, cx))))
                    .child(Button::new("hf-apply").small().primary().label(text("xlsx.hf.apply")).on_click(cx.listener(|this, _, window, cx| {
                        this.apply_header_footer_edit(cx);
                        this.sync_hf_code_input(window, cx);
                    }))),
            );

        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(div().text_size(px(11.)).text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("_xlsx.apply-to-pages")))
            .child(page_kinds)
            .child(div().text_size(px(11.)).text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("_xlsx.options")))
            .child(options_row)
            .child(div().text_size(px(11.)).text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("_xlsx.insert-field-into-section")))
            .child(section_selector)
            .child(field_buttons)
            .child(section_block("xlsx.hf.left-section", &left_state, HfActiveSection::Left))
            .child(section_block("xlsx.hf.center-section", &center_state, HfActiveSection::Center))
            .child(section_block("xlsx.hf.right-section", &right_state, HfActiveSection::Right))
            .child(direct_code_row)
            .child(bottom_actions);

        Some(SidebarPanel { title: title.into(), body: body.into_any_element() })
    }
}
