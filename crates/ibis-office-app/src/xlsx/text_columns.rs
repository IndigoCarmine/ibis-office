// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > 区切り位置: the text of a column of cells split into the
//! columns beside it, as Excel's wizard: by delimiters (tab, semicolon,
//! comma, space, another character; runs of them as one; a quote that
//! keeps delimiters inside it) or at fixed character positions; each part
//! read as typed (標準: numbers, dates) or kept as text (文字列); put from a
//! destination cell, asking before it replaces what is there.
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_calc::input::{Typed, parse_input};
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, SheetCommand, SlideId, StyleChange};

/// How the text is split.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Split {
    /// At any of the characters; runs of them as one if `consecutive`;
    /// inside `quote` they are text (the quotes go, a doubled one stays).
    Delimited { delimiters: Vec<char>, consecutive: bool, quote: Option<char> },
    /// Before the characters at these positions (counted from 0, rising).
    Fixed(Vec<usize>),
}

/// `text` split as `split` says.
pub(crate) fn split_text(text: &str, split: &Split) -> Vec<String> {
    match split {
        Split::Fixed(breaks) => {
            // Parts past the text's end are left out, as Excel does.
            let chars: Vec<char> = text.chars().collect();
            let (mut out, mut start) = (Vec::new(), 0);
            for &b in breaks {
                if b <= start || b >= chars.len() {
                    continue;
                }
                out.push(chars[start..b].iter().collect::<String>().trim_end().to_string());
                start = b;
            }
            out.push(chars[start..].iter().collect::<String>().trim_end().to_string());
            out
        }
        Split::Delimited { delimiters, consecutive, quote } => {
            let mut out = Vec::new();
            let mut part = String::new();
            let mut chars = text.chars().peekable();
            let mut quoted = false;
            let mut last_was_delimiter = false;
            while let Some(c) = chars.next() {
                if Some(c) == *quote {
                    if quoted && chars.peek() == Some(&c) {
                        part.push(c);
                        chars.next();
                    } else {
                        quoted = !quoted;
                    }
                    last_was_delimiter = false;
                    continue;
                }
                if !quoted && delimiters.contains(&c) {
                    if *consecutive && last_was_delimiter {
                        continue;
                    }
                    out.push(std::mem::take(&mut part));
                    last_was_delimiter = true;
                    continue;
                }
                part.push(c);
                last_was_delimiter = false;
            }
            out.push(part);
            out
        }
    }
}

pub(crate) struct TextColumnsPanel {
    /// The cells split: one column.
    pub(crate) source: CellRange,
    pub(crate) fixed: bool,
    pub(crate) tab: bool,
    pub(crate) semicolon: bool,
    pub(crate) comma: bool,
    pub(crate) space: bool,
    /// Another delimiter (its first character).
    pub(crate) other: Entity<InputState>,
    pub(crate) consecutive: bool,
    /// The quote: `"`, `'` or none.
    pub(crate) quote: Option<char>,
    /// Fixed width: the positions, as `3, 8`.
    pub(crate) breaks: Entity<InputState>,
    /// Each part kept as text (文字列) rather than read as typed (標準).
    pub(crate) as_text: bool,
    pub(crate) destination: Entity<InputState>,
    /// Asked whether to replace the cells there: the next apply does.
    pub(crate) confirm: bool,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
    slide: SlideId,
}

impl TextColumnsPanel {
    fn split(&self, cx: &App) -> Result<Split, String> {
        if self.fixed {
            let text = self.breaks.read(cx).value().to_string();
            let mut breaks = Vec::new();
            for piece in text.split([',', '、', ' ']).filter(|p| !p.trim().is_empty()) {
                match piece.trim().parse::<usize>() {
                    Ok(n) if n > 0 => breaks.push(n),
                    _ => return Err(crate::shell::i18n::text("fix.text-columns.enter-break-positions-character").into()),
                }
            }
            if breaks.is_empty() {
                return Err(crate::shell::i18n::text("fix.text-columns.enter-break-positions-e-g").into());
            }
            breaks.sort();
            breaks.dedup();
            return Ok(Split::Fixed(breaks));
        }
        let mut delimiters = Vec::new();
        for (on, c) in [(self.tab, '\t'), (self.semicolon, ';'), (self.comma, ','), (self.space, ' ')] {
            if on {
                delimiters.push(c);
            }
        }
        delimiters.extend(self.other.read(cx).value().chars().next());
        if delimiters.is_empty() {
            return Err(crate::shell::i18n::text("fix.text-columns.select-least-one-delimiter").into());
        }
        Ok(Split::Delimited { delimiters, consecutive: self.consecutive, quote: self.quote })
    }
}

impl SlideEditor {
    pub(crate) fn open_text_columns_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let source = self.selected_range().unwrap_or(CellRange { first_row: row, last_row: row, first_column: column, last_column: column });
        let input = |text: String, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text(placeholder)));
            input.update(cx, |i, cx| i.set_value(text, window, cx));
            input
        };
        let panel = TextColumnsPanel {
            source,
            fixed: false,
            tab: true,
            semicolon: false,
            comma: false,
            space: false,
            other: input(String::new(), "fix.text-columns.other-1-character", window, cx),
            consecutive: false,
            quote: Some('"'),
            breaks: input(String::new(), "3, 8", window, cx),
            as_text: false,
            destination: input(ibis_doc_model::cell_name(source.first_column, source.first_row), "A1", window, cx),
            confirm: false,
            message: if source.first_column != source.last_column { crate::shell::i18n::text("fix.text-columns.only-one-column-can-converted").into() } else { String::new() },
            ui: Default::default(),
            slide: self.current_slide,
        };
        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.sort_edit = None;
        state.panels.close();
        state.panels.text_columns = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn text_columns_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.text_columns.as_mut() {
            p.message = text;
        }
        cx.notify();
    }

    /// The texts of the source cells, by row (an empty cell none).
    pub(crate) fn text_columns_source(&self) -> Vec<(u32, String)> {
        let (Some(p), Some((_, sheet))) = (self.sheet_state().and_then(|s| s.panels.text_columns.as_ref()), self.current_sheet()) else { return Vec::new() };
        (p.source.first_row..=p.source.last_row)
            .filter_map(|r| sheet.cell(r, p.source.first_column).filter(|c| !c.text.is_empty() || !c.value.is_empty()).map(|c| (r, if c.text.is_empty() { format!("{:?}", c.value) } else { c.text.clone() })))
            .collect()
    }

    pub(crate) fn apply_text_columns_panel(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.text_columns.as_ref()) else { return };
        if p.slide != self.current_slide {
            self.text_columns_message(crate::shell::i18n::text("_xlsx.the-sheet-has-changed-reopen-it").into(), cx);
            return;
        }
        if p.source.first_column != p.source.last_column {
            self.text_columns_message(crate::shell::i18n::text("fix.text-columns.only-one-column-can-converted").into(), cx);
            return;
        }
        let split = match p.split(cx) {
            Ok(s) => s,
            Err(e) => return self.text_columns_message(e, cx),
        };
        let Some(destination) = super::dedupe_edit::parse_range(&p.destination.read(cx).value()) else {
            return self.text_columns_message(crate::shell::i18n::text("fix.text-columns.enter-destination-cell-e-g").into(), cx);
        };
        let (as_text, confirm, slide, source) = (p.as_text, p.confirm, p.slide, p.source);
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let rows = self.text_columns_source();
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut edits = Vec::new();
        let mut formats: Vec<(CellRange, String)> = Vec::new();
        let mut occupied = false;
        for (row, text) in &rows {
            let to_row = destination.first_row + (row - source.first_row);
            for (i, part) in split_text(text, &split).into_iter().enumerate() {
                let column = destination.first_column + i as u32;
                let at = (to_row, column);
                // A cell written that is not one of the source's and holds something.
                if !(column == source.first_column && (source.first_row..=source.last_row).contains(&to_row))
                    && sheet.cell(at.0, at.1).is_some_and(|c| !c.value.is_empty() || c.formula.is_some())
                {
                    occupied = true;
                }
                let one = CellRange { first_row: to_row, last_row: to_row, first_column: column, last_column: column };
                let input = if part.is_empty() {
                    CellInput::Clear
                } else if as_text {
                    formats.push((one, "@".into()));
                    CellInput::Value(CellValue::Text(part))
                } else {
                    match parse_input(&part, date1904) {
                        Typed::Formula(f) => CellInput::Formula(f),
                        Typed::Value(v, format) => {
                            let general = sheet.cell(to_row, column).is_none_or(|c| sheet.styles[c.style as usize].number_format == "General");
                            if let (Some(code), true) = (format, general) {
                                formats.push((one, code.to_string()));
                            }
                            CellInput::Value(v)
                        }
                    }
                };
                edits.push(CellEdit { row: to_row, column, input });
            }
        }
        if edits.is_empty() {
            return self.text_columns_message(crate::shell::i18n::text("fix.text-columns.there-no-data-split").into(), cx);
        }
        // Excel asks before it replaces the cells there.
        if occupied && !confirm {
            if let Some(p) = self.wb_mut().panels.text_columns.as_mut() {
                p.confirm = true;
            }
            return self.text_columns_message(crate::shell::i18n::text("fix.text-columns.there-already-data-here-replace").into(), cx);
        }
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits })];
        for (range, code) in formats {
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::NumberFormat(code) }));
        }
        if let Err(error) = self.document.execute(commands) {
            return self.text_columns_message(error.to_string(), cx);
        }
        self.wb_mut().last_fill = None;
        if let Some(p) = self.wb_mut().panels.text_columns.as_mut() {
            p.confirm = false;
        }
        self.text_columns_message(crate::shell::i18n::format("text-columns.done", &[("rows", &rows.len())]), cx);
    }

    pub(crate) fn text_columns_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let preview: Vec<String> = {
            let p = self.sheet_state()?.panels.text_columns.as_ref()?;
            match p.split(cx) {
                Ok(split) => self.text_columns_source().iter().take(5).map(|(_, t)| split_text(t, &split).join(" │ ")).collect(),
                Err(_) => Vec::new(),
            }
        };
        let p = self.sheet_state()?.panels.text_columns.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        type Toggle = fn(&mut TextColumnsPanel);
        let toggle = |id: &'static str, label: String, flip: Toggle| {
            mark(
                id,
                Button::new(id)
                    .small()
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.text_columns.as_mut() {
                            flip(p);
                            p.confirm = false;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        let check = |on: bool, name: &str| format!("{} {}", if on { "☑" } else { "☐" }, crate::shell::i18n::text(name));
        let source = format!("{}:{}", ibis_doc_model::cell_name(p.source.first_column, p.source.first_row), ibis_doc_model::cell_name(p.source.last_column, p.source.last_row));
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(crate::shell::i18n::format("text-columns.source", &[("cells", &source)]))
            .child(
                h_flex()
                    .gap_1()
                    .child(toggle("text-columns-delimited", check(!p.fixed, "fix.text-columns.delimiters"), |p| p.fixed = false))
                    .child(toggle("text-columns-fixed", check(p.fixed, "fix.text-columns.fixed-width"), |p| p.fixed = true)),
            );
        if p.fixed {
            body = body.child(crate::shell::i18n::text("fix.text-columns.break-positions-characters-start")).child(mark("breaks", Input::new(&p.breaks).small().into_any_element()));
        } else {
            body = body
                .child(crate::shell::i18n::text("fix.text-columns.delimiters"))
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(toggle("text-columns-tab", check(p.tab, "fix.text-columns.tab"), |p| p.tab = !p.tab))
                        .child(toggle("text-columns-semicolon", check(p.semicolon, "fix.text-columns.semicolon"), |p| p.semicolon = !p.semicolon))
                        .child(toggle("text-columns-comma", check(p.comma, "fix.text-columns.comma"), |p| p.comma = !p.comma))
                        .child(toggle("text-columns-space", check(p.space, "fix.text-columns.space"), |p| p.space = !p.space)),
                )
                .child(mark("other", Input::new(&p.other).small().into_any_element()))
                .child(toggle("text-columns-consecutive", check(p.consecutive, "fix.text-columns.treat-consecutive-delimiters-one"), |p| p.consecutive = !p.consecutive))
                .child(toggle(
                    "text-columns-quote",
                    crate::shell::i18n::format(
                        "text-columns.quote",
                        &[("quote", &match p.quote {
                            Some('"') => "\"".into(),
                            Some(_) => "'".into(),
                            None => crate::shell::i18n::text("_shell.none"),
                        })],
                    ),
                    |p| {
                        p.quote = match p.quote {
                            Some('"') => Some('\''),
                            Some(_) => None,
                            None => Some('"'),
                        }
                    },
                ));
        }
        body = body
            .child(toggle("text-columns-format", crate::shell::i18n::format("text-columns.format", &[("format", &crate::shell::i18n::text(if p.as_text { "_xlsx.text" } else { "_xlsx.general" }))]), |p| p.as_text = !p.as_text))
            .child(crate::shell::i18n::text("fix.text-columns.destination"))
            .child(mark("destination", Input::new(&p.destination).small().into_any_element()))
            .child(crate::shell::i18n::text("fix.text-columns.preview"));
        for line in preview {
            body = body.child(div().text_size(px(12.)).child(SharedString::from(line)));
        }
        body = body
            .child(div().text_size(px(12.)).child(SharedString::from(p.message.clone())))
            .child(mark(
                "apply",
                Button::new("text-columns-apply").primary().small().label(crate::shell::i18n::text(if p.confirm { "fix.text-columns.replace" } else { "fix.text-columns.split" })).on_click(cx.listener(|this, _, _, cx| this.apply_text_columns_panel(cx))).into_any_element(),
            ))
            .child(mark(
                "close",
                Button::new("text-columns-close")
                    .small()
                    .label(crate::shell::i18n::text("_shell.close"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.wb_mut().panels.text_columns = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        Some(SidebarPanel { title: "fix.text-columns.text-columns".into(), body: body.into_any_element() })
    }
}

#[cfg(test)]
mod tests {
    use super::{Split, split_text};

    fn delimited(delimiters: &[char], consecutive: bool, quote: Option<char>) -> Split {
        Split::Delimited { delimiters: delimiters.to_vec(), consecutive, quote }
    }

    #[test]
    fn delimiters_runs_and_quotes() {
        assert_eq!(split_text("a,b,,c", &delimited(&[','], false, None)), ["a", "b", "", "c"]);
        assert_eq!(split_text("a,b,,c", &delimited(&[','], true, None)), ["a", "b", "c"]);
        assert_eq!(split_text("a b;c", &delimited(&[' ', ';'], false, None)), ["a", "b", "c"]);
        assert_eq!(split_text("\"x,y\",z", &delimited(&[','], false, Some('"'))), ["x,y", "z"]);
        assert_eq!(split_text("\"say \"\"hi\"\"\",z", &delimited(&[','], false, Some('"'))), ["say \"hi\"", "z"]);
        assert_eq!(split_text("'x,y',z", &delimited(&[','], false, None)), ["'x", "y'", "z"]);
        assert_eq!(split_text("東京都\t千代田区", &delimited(&['\t'], false, Some('"'))), ["東京都", "千代田区"]);
    }

    #[test]
    fn fixed_positions() {
        assert_eq!(split_text("2026-10-07", &Split::Fixed(vec![4, 5, 7, 8])), ["2026", "-", "10", "-", "07"]);
        assert_eq!(split_text("ab", &Split::Fixed(vec![1, 5])), ["a", "b"]);
        assert_eq!(split_text("東京都千代田区", &Split::Fixed(vec![3])), ["東京都", "千代田区"]);
    }
}
