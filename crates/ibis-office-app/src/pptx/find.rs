// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pptx text search and replacement, including table cells and run boundaries.
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, Presentation, ShapeId, ShapeKind, SlideId, TextBody};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) slide: SlideId,
    pub(crate) shape: ShapeId,
    pub(crate) cell: Option<(usize, usize)>,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

pub(crate) struct FindState {
    pub(crate) query: Entity<InputState>,
    pub(crate) replacement: Entity<InputState>,
    pub(crate) current: Option<usize>,
    pub(crate) match_case: bool,
    pub(crate) whole_word: bool,
    _subscription: Subscription,
}

fn text_ranges(text: &str, query: &str, match_case: bool, whole_word: bool) -> Vec<(usize, usize)> {
    let chars: Vec<_> = text.chars().collect();
    let needle: Vec<_> = query.chars().collect();
    if needle.is_empty() || needle.len() > chars.len() { return Vec::new(); }
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut ranges = Vec::new();
    let mut at = 0;
    while at + needle.len() <= chars.len() {
        let end = at + needle.len();
        let equal = chars[at..end].iter().zip(&needle).all(|(&a, &b)| if match_case { a == b } else { a.to_lowercase().eq(b.to_lowercase()) });
        let boundary = !whole_word || ((!at.checked_sub(1).is_some_and(|i| word(chars[i]))) && (end == chars.len() || !word(chars[end])));
        if equal && boundary { ranges.push((at, end)); at = end; } else { at += 1; }
    }
    ranges
}

fn matches(presentation: &Presentation, query: &str, match_case: bool, whole_word: bool) -> Vec<Match> {
    if query.is_empty() { return Vec::new(); }
    let mut found = Vec::new();
    let mut body_matches = |slide, shape, cell, body: &TextBody| {
        let text = body.text();
        for (start, end) in text_ranges(&text, query, match_case, whole_word) {
            // Generated fields (date/slide number) are not replaceable plain text.
            let mut pos = 0;
            let mut field = false;
            for p in &body.paragraphs {
                for r in &p.runs {
                    let next = pos + r.text.chars().count();
                    field |= r.style.field.is_some() && pos < end && next > start;
                    pos = next;
                }
                pos += 1;
            }
            if !field { found.push(Match { slide, shape, cell, start, end }); }
        }
    };
    for slide in &presentation.slides {
        for shape in &slide.shapes {
            if let Some(body) = shape.text_body() { body_matches(slide.id, shape.id, None, body); }
            if let ShapeKind::Table(table) = &shape.kind {
                for (row, r) in table.rows.iter().enumerate() {
                    for (column, cell) in r.cells.iter().enumerate().filter(|(_, c)| !c.merged) {
                        body_matches(slide.id, shape.id, Some((row, column)), &cell.text);
                    }
                }
            }
        }
    }
    found
}

pub(crate) fn replacement(hit: Match, text: &str) -> Command {
    let command = Command::ReplaceText { slide: hit.slide, shape: hit.shape, at: hit.start, remove: hit.end - hit.start, insert: text.into() };
    match hit.cell { Some((row, column)) => Command::InCell { row, column, command: Box::new(command) }, None => command }
}

impl SlideEditor {
    pub(crate) fn open_pptx_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if let Some(state) = self.pptx_mut() { state.section_panel=None;state.gradient_panel = None; state.shadow_panel = None; state.glow_panel = None; state.paragraph_panel = None; state.timing_panel = None; state.crop_panel = None; state.size_panel = None; state.frame_panel = None; state.comments = None; state.chart_panel = None; state.header_footer = None; state.insets_panel = None; state.replace_fonts = None; }
        if self.pptx().is_some_and(|s| s.find.is_none()) {
            let query = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_shell.find-what")));
            let replacement = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_shell.replace-with")));
            let subscription = cx.subscribe_in(&query, window, |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => { if let Some(f) = this.pptx_mut().and_then(|s| s.find.as_mut()) { f.current = None; } cx.notify(); }
                InputEvent::PressEnter { .. } => this.pptx_find_next(true, window, cx),
                _ => {}
            });
            if let Some(s) = self.pptx_mut() { s.find = Some(FindState { query, replacement, current: None, match_case: true, whole_word: false, _subscription: subscription }); }
        }
        self.sidebar_open = true;
        if let Some(query) = self.pptx().and_then(|s| s.find.as_ref()).map(|f| f.query.clone()) {
            query.update(cx, |q, cx| q.focus(window, cx));
        }
        cx.notify();
    }

    pub(crate) fn toggle_pptx_find_option(&mut self, case: bool, cx: &mut Context<Self>) {
        if let Some(f) = self.pptx_mut().and_then(|s| s.find.as_mut()) {
            if case { f.match_case = !f.match_case; } else { f.whole_word = !f.whole_word; }
            f.current = None;
        }
        cx.notify();
    }

    pub(crate) fn pptx_find_matches(&self, cx: &App) -> Vec<Match> {
        self.pptx().and_then(|s| s.find.as_ref()).map(|f| matches(self.presentation(), f.query.read(cx).value().as_ref(), f.match_case, f.whole_word)).unwrap_or_default()
    }

    pub(crate) fn pptx_find_next(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let hits = self.pptx_find_matches(cx);
        if hits.is_empty() { return; }
        let current = self.pptx().and_then(|s| s.find.as_ref()).and_then(|f| f.current);
        let index = match current { Some(i) if forward => (i + 1) % hits.len(), Some(i) => (i + hits.len() - 1) % hits.len(), None if forward => 0, None => hits.len() - 1 };
        let hit = hits[index];
        self.select_slide(hit.slide, window, cx);
        match hit.cell { Some(cell) => self.begin_table_cell_edit(hit.shape, cell, window, cx), None => self.begin_text_edit(hit.shape, false, false, window, cx) }
        if let Some(s) = self.pptx_mut() {
            if let Some(edit) = s.edit.as_mut() { edit.anchor = hit.start; edit.caret = hit.end; }
            if let Some(f) = s.find.as_mut() { f.current = Some(index); }
        }
        cx.notify();
    }

    pub(crate) fn pptx_replace(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let hits = self.pptx_find_matches(cx);
        let Some(f) = self.pptx().and_then(|s| s.find.as_ref()) else { return };
        let text = f.replacement.read(cx).value().to_string();
        let commands = if all {
            // Work backwards within every body: earlier offsets stay valid.
            hits.into_iter().rev().map(|hit| replacement(hit, &text)).collect()
        } else {
            hits.get(f.current.unwrap_or(0)).copied().map(|hit| vec![replacement(hit, &text)]).unwrap_or_default()
        };
        self.run(commands, cx);
        if let Some(f) = self.pptx_mut().and_then(|s| s.find.as_mut()) { f.current = None; }
        cx.notify();
    }

    pub(crate) fn pptx_find_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let f = self.pptx()?.find.as_ref()?;
        let (query, text, current) = (f.query.clone(), f.replacement.clone(), f.current);
        let (match_case, whole_word) = (f.match_case, f.whole_word);
        let n = self.pptx_find_matches(cx).len();
        let status = current.filter(|&i| i < n).map_or_else(|| crate::shell::i18n::format("docx.find.n-results", &[("n", &n)]), |i| crate::shell::i18n::format("_pptx.n-of-total", &[("n", &(i + 1)), ("total", &n)]));
        let body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .child(Input::new(&query).small())
            .child(Button::new("pptx-find-case").small().ghost().label(if match_case { crate::shell::i18n::text("pptx.find.match-case") } else { crate::shell::i18n::text("_shell.match-case") }).on_click(cx.listener(|e, _, _, cx| e.toggle_pptx_find_option(true, cx))))
            .child(Button::new("pptx-find-word").small().ghost().label(if whole_word { crate::shell::i18n::text("pptx.find.whole-words") } else { crate::shell::i18n::text("pptx.chrome.find-whole-words-only") }).on_click(cx.listener(|e, _, _, cx| e.toggle_pptx_find_option(false, cx))))
            .child(h_flex().gap_1()
                .child(Button::new("pptx-find-previous").small().ghost().label(crate::shell::i18n::text("_shell.previous")).on_click(cx.listener(|e, _, w, cx| e.pptx_find_next(false, w, cx))))
                .child(Button::new("pptx-find-next").small().ghost().label(crate::shell::i18n::text("_shell.next")).on_click(cx.listener(|e, _, w, cx| e.pptx_find_next(true, w, cx))))
                .child(div().text_size(px(12.)).child(status)))
            .child(Input::new(&text).small())
            .child(h_flex().gap_1()
                .child(Button::new("pptx-replace-one").small().label(crate::shell::i18n::text("_shell.replace")).on_click(cx.listener(|e, _, w, cx| e.pptx_replace(false, w, cx))))
                .child(Button::new("pptx-replace-all").small().label(crate::shell::i18n::text("_shell.replace-all")).on_click(cx.listener(|e, _, w, cx| e.pptx_replace(true, w, cx)))))
            .child(Button::new("pptx-find-close").small().ghost().label(crate::shell::i18n::text("_shell.close")).on_click(cx.listener(|e, _, _, cx| { if let Some(s) = e.pptx_mut() { s.find = None; } cx.notify(); })));
        Some(SidebarPanel { title: crate::shell::i18n::text("_shell.find-and-replace"), body: body.into_any_element() })
    }
}
