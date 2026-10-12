// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > テキスト/CSV から…: a text file read into the sheet as Excel's
//! wizard does: its encoding (自動, UTF-8, Shift_JIS, UTF-16), its delimiter
//! (自動, カンマ, タブ, セミコロン, スペース), the row to start from, each
//! column read as typed (標準), kept as text (文字列: the leading zeros of a
//! code stay), read as a date in a chosen order (日付：YMD・MDY・DMY …) or left out (読み込まない); put from a destination cell, asking
//! before it replaces what is there. (Not a query: the cells do not
//! refresh from the file.)
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_calc::input::{DateOrder, Typed, parse_date_in_order, parse_input};
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, SheetCommand, SlideId, StyleChange};
use ibis_pptx::csv::{ImportOptions, TextEncoding};

/// How a column is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ColumnType {
    /// 標準: numbers, dates and formulas as typed.
    #[default]
    Standard,
    /// 文字列: kept as written.
    Text,
    /// 日付: read in the given order of year, month and day (what is not a
    /// date in that order stays text).
    Date(DateOrder),
    /// 読み込まない.
    Skip,
}

impl ColumnType {
    fn next(self) -> Self {
        match self {
            Self::Standard => Self::Text,
            Self::Text => Self::Date(DateOrder::Ymd),
            Self::Date(o) => match DateOrder::ALL.iter().position(|&x| x == o) {
                Some(i) if i + 1 < DateOrder::ALL.len() => Self::Date(DateOrder::ALL[i + 1]),
                _ => Self::Skip,
            },
            Self::Skip => Self::Standard,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Standard => "_xlsx.general",
            Self::Text => "_xlsx.text",
            Self::Date(DateOrder::Ymd) => "_xlsx.date-ymd",
            Self::Date(DateOrder::Mdy) => "_xlsx.date-mdy",
            Self::Date(DateOrder::Dmy) => "_xlsx.date-dmy",
            Self::Date(DateOrder::Dym) => "_xlsx.date-dym",
            Self::Date(DateOrder::Myd) => "_xlsx.date-myd",
            Self::Date(DateOrder::Ydm) => "_xlsx.date-ydm",
            Self::Skip => "_xlsx.do-not-import-column-skip",
        }
    }
}

const ENCODINGS: [(TextEncoding, &str); 4] = [(TextEncoding::Auto, "_shell.auto"), (TextEncoding::Utf8, "UTF-8"), (TextEncoding::ShiftJis, "Shift_JIS"), (TextEncoding::Utf16, "UTF-16")];
const DELIMITERS: [(Option<char>, &str); 5] = [(None, "_shell.auto"), (Some(','), "fix.text-columns.comma"), (Some('\t'), "fix.text-columns.tab"), (Some(';'), "fix.text-columns.semicolon"), (Some(' '), "fix.text-columns.space")];

pub(crate) struct ImportTextPanel {
    /// The file's name, and its bytes (read once when picked).
    pub(crate) name: Option<String>,
    pub(crate) data: Option<Vec<u8>>,
    pub(crate) encoding: TextEncoding,
    pub(crate) delimiter: Option<char>,
    /// The row to start from (1 for the first).
    pub(crate) start_row: Entity<InputState>,
    /// How each column of the file is read (past the list: 標準).
    pub(crate) columns: Vec<ColumnType>,
    pub(crate) destination: Entity<InputState>,
    /// Asked whether to replace the cells there: the next apply does.
    pub(crate) confirm: bool,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
    slide: SlideId,
}

impl ImportTextPanel {
    fn options(&self, cx: &App) -> ImportOptions {
        let start_row = self.start_row.read(cx).value().trim().parse::<usize>().ok().filter(|&n| n >= 1).unwrap_or(1);
        ImportOptions { encoding: self.encoding, delimiter: self.delimiter, start_row }
    }

    fn column(&self, i: usize) -> ColumnType {
        self.columns.get(i).copied().unwrap_or_default()
    }
}

/// The cells a read file puts from `origin`: each field as its column says
/// (the columns left out leave no gap), and the number formats those take.
/// An empty field leaves its cell as it is (no edit).
pub(crate) fn cells_of(rows: &[Vec<String>], columns: &[ColumnType], origin: (u32, u32), date1904: bool) -> (Vec<CellEdit>, Vec<(CellRange, String)>) {
    let mut edits = Vec::new();
    let mut formats = Vec::new();
    for (r, record) in rows.iter().enumerate() {
        let mut to_column = origin.1;
        for (c, field) in record.iter().enumerate() {
            let kind = columns.get(c).copied().unwrap_or_default();
            if kind == ColumnType::Skip {
                continue;
            }
            let row = origin.0 + r as u32;
            let column = to_column;
            to_column += 1;
            if field.is_empty() {
                continue;
            }
            let one = CellRange { first_row: row, last_row: row, first_column: column, last_column: column };
            let input = match kind {
                ColumnType::Text => {
                    formats.push((one, "@".to_string()));
                    CellInput::Value(CellValue::Text(field.clone()))
                }
                ColumnType::Date(order) if parse_date_in_order(field, order, date1904).is_some() => {
                    formats.push((one, "yyyy/m/d".to_string()));
                    CellInput::Value(CellValue::Number(parse_date_in_order(field, order, date1904).unwrap_or_default()))
                }
                ColumnType::Date(_) => {
                    // Not a date in that order: kept as written, as text.
                    formats.push((one, "@".to_string()));
                    CellInput::Value(CellValue::Text(field.clone()))
                }
                _ => match parse_input(field, date1904) {
                    Typed::Formula(f) => CellInput::Formula(f),
                    Typed::Value(v, format) => {
                        if let Some(code) = format {
                            formats.push((one, code.to_string()));
                        }
                        CellInput::Value(v)
                    }
                },
            };
            edits.push(CellEdit { row, column, input });
        }
    }
    (edits, formats)
}

impl SlideEditor {
    pub(crate) fn open_import_text_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_import_text_panel(window, cx);
        self.pick_import_text_file(window, cx);
    }

    /// The panel, with no file yet.
    pub(crate) fn show_import_text_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let input = |text: String, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text(placeholder)));
            input.update(cx, |i, cx| i.set_value(text, window, cx));
            input
        };
        let panel = ImportTextPanel {
            name: None,
            data: None,
            encoding: TextEncoding::Auto,
            delimiter: None,
            start_row: input("1".into(), "1", window, cx),
            columns: Vec::new(),
            destination: input(ibis_doc_model::cell_name(column, row), "A1", window, cx),
            confirm: false,
            message: String::new(),
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
        state.panels.import_text = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// Ask for the text file, and read it into the panel.
    pub(crate) fn pick_import_text_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_xlsx.load").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                this.update(cx, |this, cx| this.set_import_text_data(name, data, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
                this.update(cx, |this, cx| this.set_import_text_data(name, data, cx)).ok();
            })
            .detach();
        }
    }

    /// The text file the panel reads: its name and bytes.
    pub(crate) fn set_import_text_data(&mut self, name: String, data: Vec<u8>, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.import_text.as_mut() {
            p.name = Some(name);
            p.data = Some(data);
            p.columns.clear();
            p.confirm = false;
            p.message.clear();
        }
        cx.notify();
    }

    fn import_text_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.import_text.as_mut() {
            p.message = text;
        }
        cx.notify();
    }

    pub(crate) fn apply_import_text_panel(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.import_text.as_ref()) else { return };
        if p.slide != self.current_slide {
            return self.import_text_message(crate::shell::i18n::text("_xlsx.the-sheet-has-changed-reopen-it").into(), cx);
        }
        let Some(data) = p.data.as_ref() else {
            return self.import_text_message(crate::shell::i18n::text("_xlsx.choose-a-file-to-load").into(), cx);
        };
        let imported = match ibis_pptx::csv::import(data, &p.options(cx)) {
            Ok(i) => i,
            Err(e) => return self.import_text_message(e.to_string(), cx),
        };
        let Some(destination) = super::dedupe_edit::parse_range(&p.destination.read(cx).value()) else {
            return self.import_text_message(crate::shell::i18n::text("fix.text-columns.enter-destination-cell-e-g").into(), cx);
        };
        let (confirm, slide, columns) = (p.confirm, p.slide, p.columns.clone());
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let (edits, formats) = cells_of(&imported.rows, &columns, (destination.first_row, destination.first_column), date1904);
        if edits.is_empty() {
            return self.import_text_message(crate::shell::i18n::text("_xlsx.there-is-no-data-to-load").into(), cx);
        }
        let Some((_, sheet)) = self.current_sheet() else { return };
        let occupied = edits.iter().any(|e| sheet.cell(e.row, e.column).is_some_and(|c| !c.value.is_empty() || c.formula.is_some()));
        if occupied && !confirm {
            if let Some(p) = self.wb_mut().panels.import_text.as_mut() {
                p.confirm = true;
            }
            return self.import_text_message(crate::shell::i18n::text("_xlsx.there-is-already-data-here-press-load-again-to-r").into(), cx);
        }
        let rows = imported.rows.len();
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits })];
        for (range, code) in formats {
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::NumberFormat(code) }));
        }
        if let Err(error) = self.document.execute(commands) {
            return self.import_text_message(error.to_string(), cx);
        }
        self.wb_mut().last_fill = None;
        if let Some(p) = self.wb_mut().panels.import_text.as_mut() {
            p.confirm = false;
        }
        self.import_text_message(crate::shell::i18n::format("import-text.done", &[("rows", &rows)]), cx);
    }

    pub(crate) fn import_text_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let preview: Vec<Vec<String>> = {
            let p = self.sheet_state()?.panels.import_text.as_ref()?;
            match p.data.as_ref().map(|d| ibis_pptx::csv::import(d, &p.options(cx))) {
                Some(Ok(i)) => i.rows.into_iter().take(6).collect(),
                _ => Vec::new(),
            }
        };
        let p = self.sheet_state()?.panels.import_text.as_ref()?;
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
        type Change = Box<dyn Fn(&mut ImportTextPanel)>;
        let button = |id: String, label: String, change: Change| {
            mark(
                &id.clone(),
                Button::new(SharedString::from(id))
                    .small()
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.import_text.as_mut() {
                            change(p);
                            p.confirm = false;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        let file = p.name.clone().unwrap_or_else(|| crate::shell::i18n::text("_xlsx.no-file-chosen").to_string());
        let encoding = ENCODINGS.iter().find(|(e, _)| *e == p.encoding).map_or("", |(_, n)| *n);
        let delimiter = DELIMITERS.iter().find(|(d, _)| *d == p.delimiter).map_or("", |(_, n)| *n);
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(crate::shell::i18n::format("import-text.file", &[("file", &file)]))
            .child(mark(
                "pick",
                Button::new("import-text-pick").small().label(crate::shell::i18n::text("_xlsx.choose-file")).on_click(cx.listener(|this, _, window, cx| this.pick_import_text_file(window, cx))).into_any_element(),
            ))
            .child(button(
                "import-text-encoding".into(),
                crate::shell::i18n::format("import-text.encoding", &[("encoding", &crate::shell::i18n::text(encoding))]),
                Box::new(|p| {
                    let i = ENCODINGS.iter().position(|(e, _)| *e == p.encoding).unwrap_or(0);
                    p.encoding = ENCODINGS[(i + 1) % ENCODINGS.len()].0;
                }),
            ))
            .child(button(
                "import-text-delimiter".into(),
                crate::shell::i18n::format("import-text.delimiter", &[("delimiter", &crate::shell::i18n::text(delimiter))]),
                Box::new(|p| {
                    let i = DELIMITERS.iter().position(|(d, _)| *d == p.delimiter).unwrap_or(0);
                    p.delimiter = DELIMITERS[(i + 1) % DELIMITERS.len()].0;
                }),
            ))
            .child(crate::shell::i18n::text("_xlsx.start-at-row"))
            .child(mark("start-row", Input::new(&p.start_row).small().into_any_element()))
            .child(crate::shell::i18n::text("_xlsx.column-data-format-click-to-change"));
        let shown = preview.iter().map(Vec::len).max().unwrap_or(0).min(8);
        let mut types = h_flex().gap_1().flex_wrap();
        for i in 0..shown {
            let kind = p.column(i);
            types = types.child(button(
                format!("import-text-column-{i}"),
                crate::shell::i18n::format("import-text.column", &[("n", &(i + 1)), ("format", &crate::shell::i18n::text(kind.label()))]),
                Box::new(move |p| {
                    if p.columns.len() <= i {
                        p.columns.resize(i + 1, ColumnType::Standard);
                    }
                    p.columns[i] = p.columns[i].next();
                }),
            ));
        }
        body = body.child(types);
        if shown > 0 {
            body = body.child(button(
                "import-text-all-text".into(),
                crate::shell::i18n::text("_xlsx.make-all-columns-text").to_string(),
                Box::new(move |p| {
                    let n = p.data.as_ref().and_then(|d| ibis_pptx::csv::import(d, &ImportOptions { start_row: 1, ..ImportOptions { encoding: p.encoding, delimiter: p.delimiter, start_row: 1 } }).ok()).map_or(0, |i| i.rows.iter().map(Vec::len).max().unwrap_or(0));
                    p.columns = vec![ColumnType::Text; n];
                }),
            ));
        }
        body = body.child(crate::shell::i18n::text("fix.text-columns.destination")).child(mark("destination", Input::new(&p.destination).small().into_any_element())).child(crate::shell::i18n::text("fix.text-columns.preview"));
        for record in preview {
            let line = record.iter().take(8).map(String::as_str).collect::<Vec<_>>().join(" │ ");
            body = body.child(div().text_size(px(12.)).child(SharedString::from(line)));
        }
        body = body
            .child(div().text_size(px(12.)).child(SharedString::from(p.message.clone())))
            .child(mark(
                "apply",
                Button::new("import-text-apply").primary().small().label(crate::shell::i18n::text(if p.confirm { "fix.text-columns.replace" } else { "_xlsx.import" })).on_click(cx.listener(|this, _, _, cx| this.apply_import_text_panel(cx))).into_any_element(),
            ))
            .child(mark(
                "close",
                Button::new("import-text-close")
                    .small()
                    .label(crate::shell::i18n::text("_shell.close"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.wb_mut().panels.import_text = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        Some(SidebarPanel { title: "_xlsx.from-text-csv".into(), body: body.into_any_element() })
    }
}
