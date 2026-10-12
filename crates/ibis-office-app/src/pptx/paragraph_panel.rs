// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Paragraph spacing for a frozen text selection, including a table cell.
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, FontAlign, LineSpacing, ParagraphFormat, ShapeId, ShapeKind, SlideId, TextBody};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode { Default, Multiple, Points }
type Target = (SlideId, ShapeId, Option<(usize, usize)>, usize, usize, TextBody);
pub(crate) struct ParagraphPanel {
    pub(crate) fields: Vec<Entity<InputState>>,
    pub(crate) mode: Mode,
    font_align: Option<FontAlign>,
    targets: Vec<Target>,
    error: Option<SharedString>,
}
fn amount(value: &str, scale: f64, max: f64, positive: bool) -> Option<u32> {
    let value: f64 = value.trim().parse().ok()?;
    let scaled = (value * scale).round();
    (value.is_finite() && value >= 0. && scaled <= max && (!positive || scaled > 0.)).then_some(scaled as u32)
}
fn signed_amount(value: &str, scale: f64, max: f64) -> Option<i64> {
    let value: f64 = value.trim().parse().ok()?;
    let scaled = (value * scale).round();
    (value.is_finite() && scaled.abs() <= max).then_some(scaled as i64)
}
impl SlideEditor {
    pub(crate) fn open_paragraph_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let (targets, paragraph) = if let Some(edit) = self.text_editing() {
            let range = edit.range();
            let Some(body) = self.edited_body() else { return };
            let mut pos = 0;
            let p = body.paragraphs.iter().find(|p| { let end = pos + p.runs.iter().map(|r| r.text.chars().count()).sum::<usize>(); let found = range.start <= end; pos = end + 1; found }).cloned();
            (vec![(self.current_slide, edit.shape, edit.cell, range.start, range.end, body.clone())], p)
        } else {
            let shapes = self.selected_shapes();
            (shapes.iter().filter_map(|s| s.text_body().map(|b| (self.current_slide, s.id, None, 0, b.text().chars().count(), b.clone()))).collect(),
             shapes.iter().find_map(|s| s.text_body().and_then(|b| b.paragraphs.first()).cloned()))
        };
        let Some(p) = paragraph else { return };
        self.end_text_edit(window, cx);
        let (mode, value) = match p.line_spacing { None => (Mode::Default, 1.), Some(LineSpacing::Percent(v)) => (Mode::Multiple, v as f64 / 100_000.), Some(LineSpacing::Points(v)) => (Mode::Points, v as f64 / 100.) };
        let fields: Vec<_> = [
            value,
            p.space_before as f64 / 100.,
            p.space_after as f64 / 100.,
            p.margin_left as f64 / 12_700.,
            p.indent as f64 / 12_700.,
        ].into_iter().map(|v| cx.new(|cx| InputState::new(window, cx).default_value(format!("{v:.2}")))).collect();
        let first = fields[0].clone();
        if let Some(s) = self.pptx_mut() {
            s.section_panel=None;s.gradient_panel = None; s.shadow_panel = None; s.glow_panel = None; s.find = None; s.size_panel = None; s.frame_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None; s.insets_panel = None; s.crop_panel = None; s.timing_panel = None;
            s.paragraph_panel = Some(ParagraphPanel { fields, mode, font_align: None, targets, error: None });
        }
        self.sidebar_open = true; first.update(cx, |f, cx| f.focus(window, cx)); cx.notify();
    }
    pub(crate) fn set_paragraph_panel_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.paragraph_panel.as_mut()) { p.mode = mode; }
        cx.notify();
    }
    pub(crate) fn set_paragraph_panel_font_align(&mut self, value: FontAlign, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.paragraph_panel.as_mut()) { p.font_align = Some(value); }
        cx.notify();
    }
    pub(crate) fn apply_paragraph_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.paragraph_panel.as_ref()) else { return };
        let line = match p.mode {
            Mode::Default => Some(None),
            Mode::Multiple => amount(p.fields[0].read(cx).value().as_ref(), 100_000., 1_000_000., true).map(|v| Some(LineSpacing::Percent(v))),
            Mode::Points => amount(p.fields[0].read(cx).value().as_ref(), 100., 400_000., true).map(|v| Some(LineSpacing::Points(v))),
        };
        let before = amount(p.fields[1].read(cx).value().as_ref(), 100., 400_000., false);
        let after = amount(p.fields[2].read(cx).value().as_ref(), 100., 400_000., false);
        let margin_left = amount(p.fields[3].read(cx).value().as_ref(), 12_700., 51_206_400., false).map(|v| v as i64);
        let indent = signed_amount(p.fields[4].read(cx).value().as_ref(), 12_700., 51_206_400.);
        let (Some(line), Some(before), Some(after), Some(margin_left), Some(indent)) = (line, before, after, margin_left, indent) else {
            if let Some(p) = self.pptx_mut().and_then(|s| s.paragraph_panel.as_mut()) { p.error = Some(crate::shell::i18n::text("_pptx.enter-valid-values-in-points-for-line-spacing-pa").into()); }
            return cx.notify();
        };
        // A saved offset is valid only for the body it was captured from.
        // Validate every target before creating any command, so a multi-selection
        // cannot partially apply when one body has changed or disappeared.
        let conflict = self.text_editing().is_some_and(|edit| edit.marked.is_some()) || p.targets.iter().any(|(slide, shape, cell, _, _, snapshot)| {
            let live = self.presentation().slide(*slide).and_then(|s| s.shape(*shape)).and_then(|target| match (cell, &target.kind) {
                (Some((row, column)), ShapeKind::Table(t)) => t.rows.get(*row).and_then(|r| r.cells.get(*column)).map(|c| &c.text),
                (None, _) => target.text_body(),
                _ => None,
            });
            live != Some(snapshot)
        });
        if conflict {
            if let Some(p) = self.pptx_mut().and_then(|s| s.paragraph_panel.as_mut()) {
                p.error = Some(crate::shell::i18n::text("pptx.paragraph-panel.target-text-has-changed-confirm").into());
            }
            return cx.notify();
        }
        let format = ParagraphFormat {
            font_align: p.font_align,
            line_spacing: Some(line),
            space_before: Some(before),
            space_after: Some(after),
            margin_left: Some(margin_left),
            indent: Some(indent),
            ..Default::default()
        };
        let commands = p.targets.clone().into_iter().filter_map(|(slide, shape, cell, start, end, _)| {
            let target = self.presentation().slide(slide)?.shape(shape)?;
            let body = match (cell, &target.kind) {
                (Some((row, column)), ShapeKind::Table(t)) => &t.rows.get(row)?.cells.get(column)?.text,
                (None, _) => target.text_body()?,
                _ => return None,
            };
            let mut changed = body.clone(); changed.format_paragraphs(start, end, &format);
            if changed == *body { return None; }
            let command = Command::SetParagraphFormat { slide, shape, start, end, format: format.clone() };
            Some(match cell { Some((row, column)) => Command::InCell { row, column, command: Box::new(command) }, None => command })
        }).collect();
        self.run(commands, cx); self.close_paragraph_panel(window, cx);
    }
    pub(crate) fn close_paragraph_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() { s.paragraph_panel = None; }
        self.focus_handle.focus(window, cx); cx.notify();
    }
    pub(crate) fn pptx_paragraph_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.paragraph_panel.as_ref()?;
        let (fields, mode, font_align, error) = (p.fields.clone(), p.mode, p.font_align, p.error.clone());
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.));
        for (value, label, id) in [(Mode::Default, crate::shell::i18n::text("pptx.paragraph-panel.default-line-spacing"), "pptx-paragraph-default"), (Mode::Multiple, crate::shell::i18n::text("_shell.multiple"), "pptx-paragraph-multiple"), (Mode::Points, crate::shell::i18n::text("pptx.paragraph-panel.exactly-pt"), "pptx-paragraph-points")] {
            body = body.child(Button::new(id).small().ghost().label(if mode == value { format!("✓ {label}") } else { label.to_string() }).on_click(cx.listener(move |e, _, _, cx| e.set_paragraph_panel_mode(value, cx))));
        }
        body = body.child(div().text_size(px(13.)).child(crate::shell::i18n::text("pptx.paragraph-panel.alignment-mixed-font-sizes-keep")));
        for (value, label, id) in [(FontAlign::Auto, crate::shell::i18n::text("_shell.auto"), "pptx-font-align-auto"), (FontAlign::Top, crate::shell::i18n::text("_shell.top"), "pptx-font-align-top"), (FontAlign::Center, crate::shell::i18n::text("_shell.center"), "pptx-font-align-center"), (FontAlign::Baseline, crate::shell::i18n::text("pptx.chrome.baseline"), "pptx-font-align-baseline"), (FontAlign::Bottom, crate::shell::i18n::text("_shell.bottom"), "pptx-font-align-bottom")] {
            body = body.child(Button::new(id).small().ghost().label(if font_align == Some(value) { format!("✓ {label}") } else { label.to_string() }).on_click(cx.listener(move |e, _, _, cx| e.set_paragraph_panel_font_align(value, cx))));
        }
        for (label, field) in [
            crate::shell::i18n::text("pptx.paragraph-panel.line-spacing-value"),
            crate::shell::i18n::text("pptx.paragraph-panel.before-pt"),
            crate::shell::i18n::text("pptx.paragraph-panel.after-pt"),
            crate::shell::i18n::text("_pptx.left-indent-pt"),
            crate::shell::i18n::text("_pptx.first-line-indent-pt"),
        ].into_iter().zip(fields) { body = body.child(div().text_size(px(13.)).child(label)).child(Input::new(&field).small()); }
        if let Some(error) = error { body = body.child(div().text_size(px(12.)).text_color(rgb(0xb3261e)).child(error)); }
        body = body.child(h_flex().gap_1()
            .child(Button::new("pptx-paragraph-apply").primary().small().label(crate::shell::i18n::text("_shell.apply")).on_click(cx.listener(|e, _, w, cx| e.apply_paragraph_panel(w, cx))))
            .child(Button::new("pptx-paragraph-cancel").small().ghost().label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(|e, _, w, cx| e.close_paragraph_panel(w, cx)))));
        Some(SidebarPanel { title: crate::shell::i18n::text("_pptx.paragraph-line-spacing-spacing-indentation"), body: body.into_any_element() })
    }
}
