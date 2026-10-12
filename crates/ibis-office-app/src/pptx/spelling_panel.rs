// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スペルチェック (ツール > スペル > スペルチェック…): the words of the
//! slides' texts (and table cells) the system's spell checker does not
//! know, one at a time in a side panel with its suggestions: 変更, すべて
//! 変更, 無視, すべて無視, 辞書に追加.
use std::collections::HashSet;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, Presentation, ShapeKind, TextBody};

use crate::pptx::find::{replacement, Match};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct SpellPanel {
    pub(crate) issues: Vec<Issue>,
    pub(crate) current: usize,
    /// Words skipped for this check (すべて無視).
    pub(crate) ignored: HashSet<String>,
    /// The language to check in; `None`: the system's choice.
    pub(crate) language: Option<String>,
    pub(crate) suggestions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Issue {
    pub(crate) hit: Match,
    pub(crate) word: String,
}

/// The text of the body `hit` is in.
fn body_of(presentation: &Presentation, hit: Match) -> Option<&TextBody> {
    let shape = presentation.slide(hit.slide)?.shape(hit.shape)?;
    match (hit.cell, &shape.kind) {
        (Some((row, column)), ShapeKind::Table(table)) => table.rows.get(row)?.cells.get(column).map(|c| &c.text),
        _ => shape.text_body(),
    }
}

/// Every word of the presentation's texts not known, in reading order.
pub(crate) fn scan(presentation: &Presentation, ignored: &HashSet<String>, language: Option<&str>) -> Vec<Issue> {
    let mut found = Vec::new();
    let mut body = |slide, shape, cell, body: &TextBody| {
        let text = body.text();
        for m in crate::spelling::check(&text, language) {
            if ignored.contains(&m.word) {
                continue;
            }
            // A generated field (the date, the slide number) is not text to correct.
            let (mut pos, mut field) = (0, false);
            for p in &body.paragraphs {
                for r in &p.runs {
                    let next = pos + r.text.chars().count();
                    field |= r.style.field.is_some() && pos < m.end && next > m.start;
                    pos = next;
                }
                pos += 1;
            }
            if !field {
                found.push(Issue { hit: Match { slide, shape, cell, start: m.start, end: m.end }, word: m.word });
            }
        }
    };
    for slide in &presentation.slides {
        for shape in &slide.shapes {
            if let Some(b) = shape.text_body() {
                body(slide.id, shape.id, None, b);
            }
            if let ShapeKind::Table(table) = &shape.kind {
                for (row, r) in table.rows.iter().enumerate() {
                    for (column, cell) in r.cells.iter().enumerate().filter(|(_, c)| !c.merged) {
                        body(slide.id, shape.id, Some((row, column)), &cell.text);
                    }
                }
            }
        }
    }
    found
}

impl SlideEditor {
    /// ツール > スペル > スペルチェック…: the panel, at the first word.
    pub(crate) fn open_spelling(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let ignored = HashSet::new();
        let issues = scan(self.presentation(), &ignored, None);
        if let Some(s) = self.pptx_mut() {
            s.find = None;
            s.section_panel = None;
            s.layout_panel = None;
            s.print_panel = None;
            s.spelling = Some(SpellPanel { issues, current: 0, ignored, language: None, suggestions: Vec::new() });
        }
        self.sidebar_open = true;
        self.spelling_goto(0, window, cx);
    }

    pub(crate) fn close_spelling(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.spelling = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The word number `index` shown: its slide, the word selected, its suggestions.
    pub(crate) fn spelling_goto(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.spelling.as_ref()) else { return };
        let Some(issue) = p.issues.get(index).cloned() else {
            if let Some(p) = self.pptx_mut().and_then(|s| s.spelling.as_mut()) {
                p.current = 0;
                p.suggestions.clear();
            }
            return cx.notify();
        };
        let hit = issue.hit;
        let suggestions = body_of(self.presentation(), hit).map(|b| crate::spelling::suggestions(&b.text(), hit.start, hit.end, p.language.as_deref())).unwrap_or_default();
        self.select_slide(hit.slide, window, cx);
        match hit.cell {
            Some(cell) => self.begin_table_cell_edit(hit.shape, cell, window, cx),
            None => self.begin_text_edit(hit.shape, false, false, window, cx),
        }
        if let Some(s) = self.pptx_mut() {
            if let Some(edit) = s.edit.as_mut() {
                edit.anchor = hit.start;
                edit.caret = hit.end;
            }
            if let Some(p) = s.spelling.as_mut() {
                p.current = index;
                p.suggestions = suggestions;
            }
        }
        cx.notify();
    }

    pub(crate) fn spelling_next(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.spelling.as_ref()) else { return };
        let n = p.issues.len();
        if n == 0 {
            return;
        }
        let index = if forward { (p.current + 1) % n } else { (p.current + n - 1) % n };
        self.spelling_goto(index, window, cx);
    }

    /// The checks made again (after a change), at the word number `near`.
    fn spelling_rescan(&mut self, near: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((ignored, language)) = self.pptx().and_then(|s| s.spelling.as_ref()).map(|p| (p.ignored.clone(), p.language.clone())) else { return };
        let issues = scan(self.presentation(), &ignored, language.as_deref());
        let index = near.min(issues.len().saturating_sub(1));
        if let Some(p) = self.pptx_mut().and_then(|s| s.spelling.as_mut()) {
            p.issues = issues;
        }
        self.spelling_goto(index, window, cx);
    }

    /// 変更 (or すべて変更: the same word everywhere): the word replaced by `text`.
    pub(crate) fn spelling_change(&mut self, text: &str, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let Some(p) = self.pptx().and_then(|s| s.spelling.as_ref()) else { return };
        let Some(issue) = p.issues.get(p.current).cloned() else { return };
        let mut hits: Vec<Match> = if all { p.issues.iter().filter(|i| i.word == issue.word).map(|i| i.hit).collect() } else { vec![issue.hit] };
        // Back to front within every text: the earlier offsets stay valid.
        hits.reverse();
        let commands: Vec<Command> = hits.into_iter().map(|hit| replacement(hit, text)).collect();
        let at = p.current;
        self.run(commands, cx);
        self.spelling_rescan(at, window, cx);
    }

    /// 無視 (or すべて無視): this word (or this spelling everywhere) is not told again.
    pub(crate) fn spelling_ignore(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx_mut().and_then(|s| s.spelling.as_mut()) else { return };
        let Some(issue) = p.issues.get(p.current).cloned() else { return };
        if all {
            p.ignored.insert(issue.word);
            let at = p.current;
            self.spelling_rescan(at, window, cx);
        } else {
            p.issues.remove(p.current);
            let at = p.current;
            self.spelling_goto(at.min(self.pptx().and_then(|s| s.spelling.as_ref()).map_or(0, |p| p.issues.len().saturating_sub(1))), window, cx);
        }
    }

    /// 辞書に追加: the word learned by the system's dictionary.
    pub(crate) fn spelling_learn(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.spelling.as_ref()) else { return };
        let Some(issue) = p.issues.get(p.current) else { return };
        crate::spelling::learn(&issue.word);
        let at = p.current;
        self.spelling_rescan(at, window, cx);
    }

    pub(crate) fn spelling_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.spelling.as_ref()?;
        let t = crate::shell::i18n::text;
        let heading = |label: &str| div().text_size(px(13.)).font_weight(FontWeight::BOLD).child(t(label));
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.));
        if !crate::spelling::available() {
            body = body.child(div().text_size(px(12.)).child(t("pptx.spelling-panel.spell-checking-not-available")));
        } else if p.issues.is_empty() {
            body = body.child(div().text_size(px(13.)).child(t("pptx.spelling-panel.no-spelling-errors-found")));
        } else {
            let issue = &p.issues[p.current.min(p.issues.len() - 1)];
            body = body
                .child(div().text_size(px(12.)).text_color(rgb(0x888888)).child(crate::shell::i18n::format("_pptx.n-of-total", &[("n", &(p.current + 1)), ("total", &p.issues.len())])))
                .child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).text_color(rgb(0xb3261e)).child(issue.word.clone()))
                .child(heading("pptx.spelling-panel.suggestions"));
            if p.suggestions.is_empty() {
                body = body.child(div().text_size(px(12.)).text_color(rgb(0x888888)).child(t("pptx.spelling-panel.no-suggestions")));
            }
            for (i, s) in p.suggestions.iter().enumerate() {
                let text = s.clone();
                body = body.child(Button::new(ElementId::Name(format!("pptx-spell-suggestion-{i}").into())).small().ghost().label(s.clone()).on_click(cx.listener(move |e, _, w, cx| e.spelling_change(&text, false, w, cx))));
            }
            let first = p.suggestions.first().cloned();
            let (first_one, first_all) = (first.clone(), first);
            body = body
                .child(h_flex().gap_1().flex_wrap()
                    .child(Button::new("pptx-spell-change").small().label(t("_xlsx.change")).on_click(cx.listener(move |e, _, w, cx| if let Some(s) = &first_one { e.spelling_change(s, false, w, cx) })))
                    .child(Button::new("pptx-spell-change-all").small().label(t("pptx.spelling-panel.change-all")).on_click(cx.listener(move |e, _, w, cx| if let Some(s) = &first_all { e.spelling_change(s, true, w, cx) }))))
                .child(h_flex().gap_1().flex_wrap()
                    .child(Button::new("pptx-spell-ignore").small().ghost().label(t("pptx.spelling-panel.ignore")).on_click(cx.listener(|e, _, w, cx| e.spelling_ignore(false, w, cx))))
                    .child(Button::new("pptx-spell-ignore-all").small().ghost().label(t("pptx.spelling-panel.ignore-all")).on_click(cx.listener(|e, _, w, cx| e.spelling_ignore(true, w, cx))))
                    .child(Button::new("pptx-spell-learn").small().ghost().label(t("pptx.spelling-panel.add-dictionary")).on_click(cx.listener(|e, _, w, cx| e.spelling_learn(w, cx)))))
                .child(h_flex().gap_1()
                    .child(Button::new("pptx-spell-previous").small().ghost().label(t("_shell.previous")).on_click(cx.listener(|e, _, w, cx| e.spelling_next(false, w, cx))))
                    .child(Button::new("pptx-spell-next").small().ghost().label(t("_shell.next")).on_click(cx.listener(|e, _, w, cx| e.spelling_next(true, w, cx)))));
        }
        body = body.child(Button::new("pptx-spell-close").small().ghost().label(t("_shell.close")).on_click(cx.listener(|e, _, w, cx| e.close_spelling(w, cx))));
        Some(SidebarPanel { title: t("pptx.spelling-panel.spelling"), body: body.into_any_element() })
    }
}
