// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! フォントの置換 (書式 > フォントの置換…): replace a font across the entire
//! presentation (all slides, text boxes, shapes and table cells) with another font.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, Presentation, ShapeKind, TextBody};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// 標準フォントの候補一覧（置換先のプリセット候補）
pub(crate) const STANDARD_FONTS: &[(&'static str, &'static str)] = &[
    ("pptx.font.arial", "Arial"),
    ("pptx.font.calibri", "Calibri"),
    ("pptx.font.carlito", "Carlito"),
    ("pptx.font.century-gothic", "Century Gothic"),
    ("pptx.font.courier-new", "Courier New"),
    ("pptx.font.georgia", "Georgia"),
    ("pptx.font.helvetica", "Helvetica"),
    ("pptx.font.hiragino-sans", "Hiragino Sans"),
    ("pptx.font.ibm-plex-sans", "IBM Plex Sans"),
    ("pptx.font.meiryo", "メイリオ"), // i18n-ok（STANDARD_FONTS の値（フォント名。文書に保存される値。pptx/c）
    ("pptx.font.ms-gothic", "ＭＳ ゴシック"), // i18n-ok（STANDARD_FONTS の値（フォント名。文書に保存される値。pptx/c）
    ("pptx.font.ms-mincho", "ＭＳ 明朝"), // i18n-ok（STANDARD_FONTS の値（フォント名。文書に保存される値。pptx/c）
    ("pptx.font.times", "Times New Roman"),
    ("pptx.font.yu-gothic", "游ゴシック"), // i18n-ok（STANDARD_FONTS の値（フォント名。文書に保存される値。pptx/c）
    ("pptx.font.yu-mincho", "游明朝"), // i18n-ok（STANDARD_FONTS の値（フォント名。文書に保存される値。pptx/c）
];

pub(crate) struct ReplaceFontsState {
    pub(crate) target: Entity<InputState>,
    pub(crate) replacement: Entity<InputState>,
    pub(crate) used_fonts: Vec<String>,
    pub(crate) message: Option<SharedString>,
}

/// プレゼンテーション全スライドで使用されているフォントファミリー名を重複なく昇順で収集する。
pub(crate) fn presentation_used_fonts(presentation: &Presentation) -> Vec<String> {
    let mut fonts = std::collections::BTreeSet::new();
    let mut collect_from_body = |body: &TextBody| {
        if let Some(font) = &body.style.font {
            let trimmed = font.trim();
            if !trimmed.is_empty() {
                fonts.insert(trimmed.to_string());
            }
        }
        if let Some(font) = &body.style.fallback_font {
            let trimmed = font.trim();
            if !trimmed.is_empty() {
                fonts.insert(trimmed.to_string());
            }
        }
        for p in &body.paragraphs {
            if let Some(font) = &p.end_style.font {
                let trimmed = font.trim();
                if !trimmed.is_empty() {
                    fonts.insert(trimmed.to_string());
                }
            }
            if let Some(font) = &p.end_style.fallback_font {
                let trimmed = font.trim();
                if !trimmed.is_empty() {
                    fonts.insert(trimmed.to_string());
                }
            }
            for r in &p.runs {
                if let Some(font) = &r.style.font {
                    let trimmed = font.trim();
                    if !trimmed.is_empty() {
                        fonts.insert(trimmed.to_string());
                    }
                }
                if let Some(font) = &r.style.fallback_font {
                    let trimmed = font.trim();
                    if !trimmed.is_empty() {
                        fonts.insert(trimmed.to_string());
                    }
                }
            }
        }
    };

    for slide in &presentation.slides {
        for shape in &slide.shapes {
            if let Some(body) = shape.text_body() {
                collect_from_body(body);
            }
            if let ShapeKind::Table(table) = &shape.kind {
                for r in &table.rows {
                    for cell in &r.cells {
                        collect_from_body(&cell.text);
                    }
                }
            }
        }
    }

    fonts.into_iter().collect()
}

/// 単一の `TextBody` 内で対象フォントを置換し、変更があれば新しい `TextBody` を返す。
pub(crate) fn replace_fonts_in_body(body: &TextBody, target: &str, replacement: &str) -> Option<TextBody> {
    let target = target.trim();
    let replacement = replacement.trim();
    if target.is_empty() || replacement.is_empty() || target.eq_ignore_ascii_case(replacement) {
        return None;
    }

    let mut changed = false;
    let mut new_body = body.clone();
    let font_matches = |f: &str| f.trim().eq_ignore_ascii_case(target);

    if let Some(font) = &new_body.style.font {
        if font_matches(font) {
            new_body.style.font = Some(replacement.to_string());
            changed = true;
        }
    }
    if let Some(font) = &new_body.style.fallback_font {
        if font_matches(font) {
            new_body.style.fallback_font = Some(replacement.to_string());
            changed = true;
        }
    }

    for p in &mut new_body.paragraphs {
        if let Some(font) = &p.end_style.font {
            if font_matches(font) {
                p.end_style.font = Some(replacement.to_string());
                changed = true;
            }
        }
        if let Some(font) = &p.end_style.fallback_font {
            if font_matches(font) {
                p.end_style.fallback_font = Some(replacement.to_string());
                changed = true;
            }
        }
        for r in &mut p.runs {
            if let Some(font) = &r.style.font {
                if font_matches(font) {
                    r.style.font = Some(replacement.to_string());
                    changed = true;
                }
            }
            if let Some(font) = &r.style.fallback_font {
                if font_matches(font) {
                    r.style.fallback_font = Some(replacement.to_string());
                    changed = true;
                }
            }
        }
    }

    if changed {
        Some(new_body)
    } else {
        None
    }
}

/// プレゼンテーション全体で指定したフォントを置換する `Command` 一覧を生成する。
pub(crate) fn replace_presentation_fonts_commands(presentation: &Presentation, target: &str, replacement: &str) -> Vec<Command> {
    let mut commands = Vec::new();
    let target = target.trim();
    let replacement = replacement.trim();
    if target.is_empty() || replacement.is_empty() || target.eq_ignore_ascii_case(replacement) {
        return commands;
    }

    for slide in &presentation.slides {
        for shape in &slide.shapes {
            if let Some(body) = shape.text_body() {
                if let Some(new_body) = replace_fonts_in_body(body, target, replacement) {
                    commands.push(Command::SetTextBody {
                        slide: slide.id,
                        shape: shape.id,
                        body: new_body,
                    });
                }
            }
            if let ShapeKind::Table(table) = &shape.kind {
                for (row, r) in table.rows.iter().enumerate() {
                    for (column, cell) in r.cells.iter().enumerate().filter(|(_, c)| !c.merged) {
                        if let Some(new_body) = replace_fonts_in_body(&cell.text, target, replacement) {
                            commands.push(Command::InCell {
                                row,
                                column,
                                command: Box::new(Command::SetTextBody {
                                    slide: slide.id,
                                    shape: shape.id,
                                    body: new_body,
                                }),
                            });
                        }
                    }
                }
            }
        }
    }

    commands
}

impl SlideEditor {
    pub(crate) fn open_replace_fonts_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if let Some(state) = self.pptx_mut() {
            state.section_panel = None;
            state.gradient_panel = None;
            state.shadow_panel = None;
            state.glow_panel = None;
            state.paragraph_panel = None;
            state.timing_panel = None;
            state.crop_panel = None;
            state.size_panel = None;
            state.frame_panel = None;
            state.comments = None;
            state.chart_panel = None;
            state.header_footer = None;
            state.insets_panel = None;
            state.find = None;
            state.theme_panel = None;
        }

        let used_fonts = presentation_used_fonts(self.presentation());
        let default_target = used_fonts.first().cloned().unwrap_or_default();
        let default_replacement = if default_target == "Arial" { "Calibri".to_string() } else { "Arial".to_string() };

        let target = cx.new(|cx| InputState::new(window, cx).default_value(default_target));
        let replacement = cx.new(|cx| InputState::new(window, cx).default_value(default_replacement));

        if let Some(s) = self.pptx_mut() {
            s.replace_fonts = Some(ReplaceFontsState {
                target,
                replacement,
                used_fonts,
                message: None,
            });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_replace_fonts_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.replace_fonts = None;
        }
        cx.notify();
    }

    /// プレゼンテーション内のフォントを一括置換する（直接呼び出し用）
    pub(crate) fn replace_fonts_in_presentation(&mut self, target: &str, replacement: &str, cx: &mut Context<Self>) -> usize {
        let commands = replace_presentation_fonts_commands(self.presentation(), target, replacement);
        let count = commands.len();
        if !commands.is_empty() {
            self.run(commands, cx);
        }
        count
    }

    /// パネル上の「置換」ボタンから実行
    pub(crate) fn replace_presentation_fonts(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.pptx().and_then(|s| s.replace_fonts.as_ref()) else { return };
        let target = state.target.read(cx).value().to_string();
        let replacement = state.replacement.read(cx).value().to_string();

        let count = self.replace_fonts_in_presentation(&target, &replacement, cx);

        let new_used = presentation_used_fonts(self.presentation());
        if let Some(s) = self.pptx_mut().and_then(|s| s.replace_fonts.as_mut()) {
            s.used_fonts = new_used;
            s.message = Some(if count > 0 {
                crate::shell::i18n::format("pptx.replace-fonts.replaced-fonts-n-location-s", &[("n", &count)]).into()
            } else {
                crate::shell::i18n::text("pptx.replace-fonts.font-replace-was-not-found").into()
            });
        }
        cx.notify();
    }

    pub(crate) fn replace_fonts_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.pptx()?.replace_fonts.as_ref()?;
        let target = state.target.clone();
        let replacement = state.replacement.clone();
        let used_fonts = state.used_fonts.clone();
        let message = state.message.clone();

        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.));

        // 置換前のフォント
        body = body.child(div().text_size(px(12.)).text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("pptx.replace-fonts.with")));
        body = body.child(Input::new(&target).small());

        // 現在使用されているフォント候補の表示（クリックで選択可能）
        if !used_fonts.is_empty() {
            body = body.child(div().text_size(px(11.)).text_color(rgb(0x70757a)).child(crate::shell::i18n::text("pptx.replace-fonts.fonts-used-presentation")));
            let mut font_chips = h_flex().gap_1().flex_wrap();
            for f in &used_fonts {
                let f_clone = f.clone();
                let target_clone = target.clone();
                font_chips = font_chips.child(
                    Button::new(ElementId::Name(format!("pptx-used-font-{f}").into()))
                        .small()
                        .ghost()
                        .label(f.clone())
                        .on_click(cx.listener(move |_, _, window, cx| {
                            target_clone.update(cx, |t, cx| t.set_value(&f_clone, window, cx));
                        })),
                );
            }
            body = body.child(font_chips);
        }

        // 置換後のフォント
        body = body.child(div().text_size(px(12.)).text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("pptx.replace-fonts.replace")));
        body = body.child(Input::new(&replacement).small());

        // 実行メッセージ
        if let Some(msg) = message {
            body = body.child(div().text_size(px(12.)).text_color(rgb(0x1a73e8)).child(msg));
        }

        // アクションボタン
        body = body.child(
            h_flex()
                .gap_1()
                .child(Button::new("pptx-replace-fonts-apply").primary().small().label(crate::shell::i18n::text("_shell.replace")).on_click(cx.listener(|e, _, w, cx| e.replace_presentation_fonts(w, cx))))
                .child(Button::new("pptx-replace-fonts-close").small().ghost().label(crate::shell::i18n::text("_shell.close")).on_click(cx.listener(|e, _, _, cx| e.close_replace_fonts_panel(cx)))),
        );

        Some(SidebarPanel {
            title: crate::shell::i18n::text("_pptx.replace-fonts"),
            body: body.into_any_element(),
        })
    }
}
