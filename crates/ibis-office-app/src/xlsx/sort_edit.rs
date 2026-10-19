// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Value sorting with ordered column keys on a captured worksheet range.
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, SheetCommand, SlideId};

/// What a level sorts by (Excel's 並べ替えのキー): the cells' values, or
/// a fill, text colour, or conditional formatting icon put on top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LevelKey {
    Value,
    CellColor(Option<ibis_doc_model::Rgb>),
    FontColor(Option<ibis_doc_model::Rgb>),
    Icon(String, u32),
}

pub(crate) struct SortPanel {
    pub(crate) range: Entity<InputState>,
    pub(crate) levels: Vec<(Entity<InputState>, bool)>,
    /// Each level's key, beside `levels`.
    pub(crate) keys: Vec<LevelKey>,
    /// Each level's custom list input, beside `levels`.
    pub(crate) custom_lists: Vec<Entity<InputState>>,
    /// Which level has the custom list settings open.
    pub(crate) custom_open: Option<usize>,
    /// The level whose column list (by the headings) is open.
    pub(crate) picking: Option<usize>,
    /// The level whose key list is open.
    pub(crate) keying: Option<usize>,
    pub(crate) ui:
        std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
    slide: SlideId,
    pub(crate) header: bool,
    pub(crate) match_case: bool,
    pub(crate) error: String,
}
fn range(text: &str) -> Option<CellRange> {
    let mut ends = text.trim().split(':');
    let a = ibis_calc::formula::parser::parse_cell(ends.next()?)?;
    let b = match ends.next() {
        Some(s) => ibis_calc::formula::parser::parse_cell(s)?,
        None => a.clone(),
    };
    if ends.next().is_some() || a.row > b.row || a.column > b.column {
        return None;
    }
    Some(CellRange {
        first_row: a.row,
        last_row: b.row,
        first_column: a.column,
        last_column: b.column,
    })
}
fn column_input(
    column: u32,
    window: &mut Window,
    cx: &mut Context<SlideEditor>,
) -> Entity<InputState> {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.column-a-b")));
    let name = ibis_doc_model::cell_name(column, 0);
    input.update(cx, |i, cx| {
        i.set_value(name.strip_suffix('1').unwrap(), window, cx)
    });
    input
}
fn custom_list_input(
    window: &mut Window,
    cx: &mut Context<SlideEditor>,
) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.e-g-sun-mon-tue-wed-thu-fri-sat")))
}
impl SlideEditor {
    pub(crate) fn sort_by_selected_cell_color(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Sort, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((range, column, header)) = self.sort_selection() else { return };
        let color = self.current_style().fill;
        self.run_sheet(
            vec![Command::Sheet(SheetCommand::SortBy {
                slide: self.current_slide,
                range,
                keys: vec![(
                    column,
                    ibis_doc_model::sheet_command::SortBy::CellColor { color, top: true },
                )],
                header,
                match_case: false,
            })],
            cx,
        );
    }

    pub(crate) fn sort_by_selected_font_color(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Sort, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((range, column, header)) = self.sort_selection() else { return };
        let style = self.current_style();
        let color = (style.color.0 != 0).then_some(style.color);
        self.run_sheet(
            vec![Command::Sheet(SheetCommand::SortBy {
                slide: self.current_slide,
                range,
                keys: vec![(
                    column,
                    ibis_doc_model::sheet_command::SortBy::FontColor { color, top: true },
                )],
                header,
                match_case: false,
            })],
            cx,
        );
    }

    pub(crate) fn sort_by_selected_icon(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Sort, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let Some((range, _, header)) = self.sort_selection() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some((icon_set, icon_id)) = ibis_doc_model::sheet_command::cell_icon(sheet, row, column) else {
            return;
        };
        self.run_sheet(
            vec![Command::Sheet(SheetCommand::SortBy {
                slide: self.current_slide,
                range,
                keys: vec![(
                    column,
                    ibis_doc_model::sheet_command::SortBy::Icon {
                        icon_set,
                        icon_id,
                        top: true,
                    },
                )],
                header,
                match_case: false,
            })],
            cx,
        );
    }

    pub(crate) fn open_sort_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((r, column, header)) = self.sort_selection() else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("A1:C10"));
        input.update(cx, |i, cx| {
            i.set_value(
                format!(
                    "{}:{}",
                    ibis_doc_model::cell_name(r.first_column, r.first_row),
                    ibis_doc_model::cell_name(r.last_column, r.last_row)
                ),
                window,
                cx,
            );
            i.focus(window, cx);
        });
        let custom_inp = custom_list_input(window, cx);
        let panel = SortPanel {
            range: input,
            levels: vec![(column_input(column, window, cx), true)],
            keys: vec![LevelKey::Value],
            custom_lists: vec![custom_inp],
            custom_open: None,
            picking: None,
            keying: None,
            ui: Default::default(),
            slide: self.current_slide,
            header,
            match_case: false,
            error: String::new(),
        };
        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.panels.close();
        state.sort_edit = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }
    pub(crate) fn resolve_sort_range(&self, raw: &str) -> Option<CellRange> {
        self.resolve_advanced_filter_range(raw).or_else(|| range(raw))
    }

    pub(crate) fn set_sort_range_from_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((r, col, _)) = self.sort_selection().or_else(|| {
            self.selected_range().map(|r| (r, r.first_column, true))
        }) else {
            return;
        };
        let range_str = format!(
            "{}:{}",
            ibis_doc_model::cell_name(r.first_column, r.first_row),
            ibis_doc_model::cell_name(r.last_column, r.last_row)
        );
        let Some(p) = self.wb().sort_edit.as_ref() else { return };
        let range_inp = p.range.clone();
        let first_col_inp = p.levels.first().map(|(inp, _)| inp.clone());
        range_inp.update(cx, |i, cx| i.set_value(range_str, window, cx));
        if let Some(col_inp) = first_col_inp {
            let cur_text = col_inp.read(cx).value().trim().to_uppercase();
            let cur_col = ibis_calc::formula::parser::parse_cell(&format!("{cur_text}1")).map(|a| a.column);
            if !cur_col.is_some_and(|c| (r.first_column..=r.last_column).contains(&c)) {
                let target_col = col.clamp(r.first_column, r.last_column);
                let col_letter = ibis_doc_model::cell_name(target_col, 0).trim_end_matches('1').to_string();
                col_inp.update(cx, |i, cx| i.set_value(col_letter, window, cx));
            }
        }
        if let Some(p) = self.wb_mut().sort_edit.as_mut() {
            p.error.clear();
        }
        cx.notify();
    }

    fn sort_error(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().sort_edit.as_mut() {
            p.error = text.into();
        }
        cx.notify();
    }
    pub(crate) fn apply_sort_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.sort_edit.as_ref()) else {
            return;
        };
        if p.slide != self.current_slide {
            self.sort_error(&crate::shell::i18n::text("_xlsx.the-sheet-has-changed-reopen-it"), cx);
            return;
        }
        let Some(r) = self.resolve_sort_range(&p.range.read(cx).value()) else {
            self.sort_error(&crate::shell::i18n::text("_xlsx.enter-a-valid-cell-range"), cx);
            return;
        };
        let Some((_, sheet)) = self.current_sheet() else {
            return;
        };
        if r.last_row as usize >= sheet.rows.len()
            || r.last_column as usize >= sheet.columns.len()
            || (p.header && r.first_row == r.last_row)
        {
            self.sort_error(&crate::shell::i18n::text("_xlsx.check-the-range-and-headers"), cx);
            return;
        }
        let mut keys = Vec::new();
        for (level, (input, ascending)) in p.levels.iter().enumerate() {
            let key = p.keys.get(level).cloned().unwrap_or(LevelKey::Value);
            let raw_text = input.read(cx).value().trim().to_string();
            let text = raw_text.to_uppercase();
            let by_letter = if !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphabetic()) {
                ibis_calc::formula::parser::parse_cell(&format!("{text}1"))
                    .map(|a| a.column)
                    .filter(|c| (r.first_column..=r.last_column).contains(c))
            } else {
                None
            };
            let column = by_letter.or_else(|| {
                if p.header && !raw_text.is_empty() {
                    (r.first_column..=r.last_column).find(|&c| {
                        sheet
                            .cell(r.first_row, c)
                            .is_some_and(|cell| cell.text.trim().eq_ignore_ascii_case(&raw_text) || cell.text.trim() == raw_text)
                    })
                } else {
                    None
                }
            });
            let Some(column) = column.filter(|c| {
                (r.first_column..=r.last_column).contains(c)
                    && !keys.iter().enumerate().any(|(j, (old, _))| old == c && p.keys.get(j).cloned().unwrap_or(LevelKey::Value) == key)
            }) else {
                self.sort_error(
                    &crate::shell::i18n::text("_xlsx.specify-column-names-within-the-range-each-colum"),
                    cx,
                );
                return;
            };
            keys.push((column, *ascending));
        }
        let by: Vec<(u32, ibis_doc_model::sheet_command::SortBy)> = keys
            .iter()
            .enumerate()
            .map(|(level, &(column, ascending))| {
                use ibis_doc_model::sheet_command::SortBy;
                let key = p.keys.get(level).cloned().unwrap_or(LevelKey::Value);
                let custom_text = p.custom_lists.get(level).map(|c| c.read(cx).value().trim().to_string()).unwrap_or_default();
                (column, match key {
                    LevelKey::Value => {
                        let items: Vec<String> = custom_text
                            .split(|c| c == ',' || c == '、' || c == '\n')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if !items.is_empty() {
                            SortBy::CustomList { list: items, ascending }
                        } else {
                            SortBy::Value(ascending)
                        }
                    }
                    LevelKey::CellColor(color) => SortBy::CellColor { color, top: ascending },
                    LevelKey::FontColor(color) => SortBy::FontColor { color, top: ascending },
                    LevelKey::Icon(icon_set, icon_id) => SortBy::Icon { icon_set, icon_id, top: ascending },
                })
            })
            .collect();
        let (slide, header, match_case) = (p.slide, p.header, p.match_case);
        if let Err(error) = self
            .document
            .execute(vec![Command::Sheet(SheetCommand::SortBy {
                slide,
                range: r,
                keys: by,
                header,
                match_case,
            })])
        {
            self.sort_error(&error.to_string(), cx);
            return;
        }
        self.wb_mut().last_fill = None;
        if let Some(p) = self.wb_mut().sort_edit.as_mut() {
            p.error.clear();
        }
        cx.notify();
    }
    /// The range's columns (letter, heading or 列 X), the fills and text
    /// colours in its rows (when more than one), and the conditional
    /// formatting icons, for the level lists.
    fn sort_choices(&self, p: &SortPanel, cx: &App) -> (Vec<(String, String)>, Vec<Option<ibis_doc_model::Rgb>>, Vec<Option<ibis_doc_model::Rgb>>, Vec<(String, u32)>) {
        let (Some(r), Some((_, sheet))) = (self.resolve_sort_range(&p.range.read(cx).value()), self.current_sheet()) else { return Default::default() };
        let letter = |c: u32| ibis_doc_model::cell_name(c, 0).trim_end_matches('1').to_string();
        let columns = (r.first_column..=r.last_column.min(r.first_column + 200))
            .map(|c| {
                let heading = sheet.cell(r.first_row, c).map(|x| x.text.clone()).filter(|t| p.header && !t.is_empty());
                (letter(c), heading.unwrap_or_else(|| crate::shell::i18n::format("_xlsx.column-name", &[("name", &letter(c))])))
            })
            .collect();
        let (mut fills, mut fonts, mut icons) = (Vec::new(), Vec::new(), Vec::new());
        let first = r.first_row + u32::from(p.header);
        for c in sheet.cells.iter().filter(|c| (first..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)) {
            let style = sheet.styles.get(c.style as usize);
            let fill = style.and_then(|s| s.fill);
            if !fills.contains(&fill) {
                fills.push(fill);
            }
            let font = style.map(|s| s.color).filter(|c| c.0 != 0);
            if !fonts.contains(&font) {
                fonts.push(font);
            }
            if let Some(ic) = ibis_doc_model::sheet_command::cell_icon(sheet, c.row, c.column)
                && !icons.contains(&ic)
            {
                icons.push(ic);
            }
        }
        if fills.len() < 2 {
            fills.clear();
        }
        if fonts.len() < 2 {
            fonts.clear();
        }
        (columns, fills, fonts, icons)
    }

    pub(crate) fn sort_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.sort_edit.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, _, _| {
                            ui.borrow_mut().insert(id.clone(), bounds);
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element()
        };
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(crate::shell::i18n::text("_xlsx.range-to-sort"))
                    .child(mark(
                        "use-selection",
                        Button::new("sort-use-selection")
                            .xsmall()
                            .ghost()
                            .label(crate::shell::i18n::text("xlsx.subtotal-use-selection"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_sort_range_from_selection(window, cx);
                            }))
                            .into_any_element(),
                    )),
            )
            .child(mark(
                "range",
                Input::new(&p.range).small().into_any_element(),
            ))
            .child(
                h_flex()
                    .gap_2()
                    .child(mark(
                        "header",
                        Button::new("sort-header")
                            .small()
                            .label(if p.header {
                                crate::shell::i18n::text("_xlsx.my-data-has-headers-on")
                            } else {
                                crate::shell::i18n::text("_xlsx.my-data-has-headers-off")
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                    p.header = !p.header;
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    ))
                    .child(mark(
                        "match-case",
                        Button::new("sort-match-case")
                            .small()
                            .label(if p.match_case {
                                crate::shell::i18n::text("_xlsx.case-sensitive-on")
                            } else {
                                crate::shell::i18n::text("_xlsx.case-sensitive-off")
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                    p.match_case = !p.match_case;
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    )),
            );
        // The headings (or column letters) of the range, and the colours and icons in it.
        let choices = self.sort_choices(p, cx);
        for (i, (column, ascending)) in p.levels.iter().enumerate() {
            let key = p.keys.get(i).cloned().unwrap_or(LevelKey::Value);
            let typed = column.read(cx).value().trim().to_uppercase();
            let heading = choices.0.iter().find(|(letter, _)| *letter == typed).map_or_else(|| crate::shell::i18n::text("_xlsx.select-column").to_string(), |(_, h)| h.clone());
            let key_label = match &key {
                LevelKey::Value => crate::shell::i18n::text("_xlsx.cell-values").to_string(),
                LevelKey::CellColor(c) => crate::shell::i18n::format("_xlsx.cell-color-color", &[("color", &colour_name(*c, true))]),
                LevelKey::FontColor(c) => crate::shell::i18n::format("_xlsx.font-color-color", &[("color", &colour_name(*c, false))]),
                LevelKey::Icon(set, id) => crate::shell::i18n::format("xlsx.sort-icon-key", &[("set", set), ("id", &id.to_string())]),
            };
            let custom_text = p.custom_lists.get(i).map(|c| c.read(cx).value().trim().to_string()).unwrap_or_default();
            let has_custom = key == LevelKey::Value && !custom_text.is_empty();
            let order_label = match (&key, *ascending, has_custom) {
                (LevelKey::Value, true, false) => crate::shell::i18n::text("_xlsx.ascending"),
                (LevelKey::Value, false, false) => crate::shell::i18n::text("_xlsx.descending"),
                (LevelKey::Value, true, true) => crate::shell::i18n::text("_xlsx.list-order"),
                (LevelKey::Value, false, true) => crate::shell::i18n::text("_xlsx.reverse-list-order"),
                (_, true, _) => crate::shell::i18n::text("_shell.top"),
                (_, false, _) => crate::shell::i18n::text("_shell.bottom"),
            };
            let row = h_flex()
                .gap_1()
                .child(mark(
                    &format!("column-{i}"),
                    Input::new(column).small().w(px(56.)).into_any_element(),
                ))
                .child(mark(
                    &format!("pick-{i}"),
                    Button::new(("sort-pick", i))
                        .small()
                        .label(format!("{heading} ▾"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                p.picking = if p.picking == Some(i) { None } else { Some(i) };
                                p.keying = None;
                                p.custom_open = None;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    &format!("key-{i}"),
                    Button::new(("sort-key", i))
                        .small()
                        .label(format!("{key_label} ▾"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                p.keying = if p.keying == Some(i) { None } else { Some(i) };
                                p.picking = None;
                                p.custom_open = None;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    &format!("order-{i}"),
                    Button::new(("sort-order", i))
                        .small()
                        .label(order_label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                if let Some((_, ascending)) = p.levels.get_mut(i) {
                                    *ascending = !*ascending;
                                }
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ));
            body = body.child(crate::shell::i18n::format("_xlsx.level-n", &[("n", &(i + 1))])).child(row);
            if p.custom_open == Some(i) {
                if let Some(custom_input) = p.custom_lists.get(i) {
                    let mut custom_box = v_flex()
                        .gap_1()
                        .pl_2()
                        .pr_2()
                        .py_1()
                        .child(
                            div()
                                .text_size(px(12.))
                                .child(crate::shell::i18n::text("_xlsx.custom-list-comma-separated")),
                        )
                        .child(mark(
                            &format!("custom-input-{i}"),
                            Input::new(custom_input).small().into_any_element(),
                        ));

                    let presets: [(&str, &str); 7] = [
                        ("_xlsx.sun-sat", "日, 月, 火, 水, 木, 金, 土"), // i18n-ok（行 411 のプリセットの値（カスタムリストの中身。照合に使う））
                        ("_xlsx.sunday-saturday", "日曜日, 月曜日, 火曜日, 水曜日, 木曜日, 金曜日, 土曜日"), // i18n-ok（行 412 のプリセットの値（カスタムリストの中身））
                        ("_xlsx.jan-dec", "1月, 2月, 3月, 4月, 5月, 6月, 7月, 8月, 9月, 10月, 11月, 12月"), // i18n-ok（行 413 のプリセットの値（カスタムリストの中身））
                        ("_xlsx.q1-q4", "第1四半期, 第2四半期, 第3四半期, 第4四半期"), // i18n-ok（行 414 のプリセットの値（カスタムリストの中身））
                        ("_xlsx.sun-sat-2", "Sun, Mon, Tue, Wed, Thu, Fri, Sat"),
                        ("_xlsx.sunday-saturday-2", "Sunday, Monday, Tuesday, Wednesday, Thursday, Friday, Saturday"),
                        ("_xlsx.jan-dec-2", "Jan, Feb, Mar, Apr, May, Jun, Jul, Aug, Sep, Oct, Nov, Dec"),
                    ];
                    let mut preset_row1 = h_flex().gap_1();
                    for (p_idx, (label, val)) in presets[..4].iter().enumerate() {
                        let val = (*val).to_string();
                        preset_row1 = preset_row1.child(mark(
                            &format!("preset-{i}-{p_idx}"),
                            Button::new(SharedString::from(format!("sort-preset-{i}-{p_idx}")))
                                .small()
                                .label(crate::shell::i18n::text(label))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                        if let Some(inp) = p.custom_lists.get(i) {
                                            let val = val.clone();
                                            inp.update(cx, |s, cx| s.set_value(val, window, cx));
                                        }
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                        ));
                    }
                    let mut preset_row2 = h_flex().gap_1();
                    for (p_idx, (label, val)) in presets[4..].iter().enumerate() {
                        let p_idx = p_idx + 4;
                        let val = (*val).to_string();
                        preset_row2 = preset_row2.child(mark(
                            &format!("preset-{i}-{p_idx}"),
                            Button::new(SharedString::from(format!("sort-preset-{i}-{p_idx}")))
                                .small()
                                .label(crate::shell::i18n::text(label))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                        if let Some(inp) = p.custom_lists.get(i) {
                                            let val = val.clone();
                                            inp.update(cx, |s, cx| s.set_value(val, window, cx));
                                        }
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                        ));
                    }
                    let clear_btn = h_flex().gap_1().child(mark(
                        &format!("custom-clear-{i}"),
                        Button::new(SharedString::from(format!("sort-custom-clear-{i}")))
                            .small()
                            .label(crate::shell::i18n::text("_xlsx.clear-list-normal-order"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                    if let Some(inp) = p.custom_lists.get(i) {
                                        inp.update(cx, |s, cx| s.set_value(String::new(), window, cx));
                                    }
                                    p.custom_open = None;
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    ));
                    custom_box = custom_box.child(preset_row1).child(preset_row2).child(clear_btn);
                    body = body.child(custom_box);
                }
            }
            if p.picking == Some(i) {
                let mut list = v_flex().pl_2();
                for (letter, heading) in &choices.0 {
                    let letter = letter.clone();
                    list = list.child(mark(
                        &format!("pick-{i}-{letter}"),
                        Button::new(SharedString::from(format!("sort-pick-{i}-{letter}")))
                            .small()
                            .label(heading.clone())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let Some(p) = this.wb_mut().sort_edit.as_mut() else { return };
                                p.picking = None;
                                if let Some((input, _)) = p.levels.get(i) {
                                    let input = input.clone();
                                    let letter = letter.clone();
                                    input.update(cx, |s, cx| s.set_value(letter, window, cx));
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    ));
                }
                body = body.child(list);
            }
            if p.keying == Some(i) {
                let mut list = v_flex().pl_2();
                let mut options = vec![LevelKey::Value];
                options.extend(choices.1.iter().map(|&c| LevelKey::CellColor(c)));
                options.extend(choices.2.iter().map(|&c| LevelKey::FontColor(c)));
                options.extend(choices.3.iter().map(|(set, id)| LevelKey::Icon(set.clone(), *id)));
                for (k, option) in options.into_iter().enumerate() {
                    let text = match &option {
                        LevelKey::Value => crate::shell::i18n::text("_xlsx.cell-values").to_string(),
                        LevelKey::CellColor(c) => crate::shell::i18n::format("_xlsx.cell-color-color", &[("color", &colour_name(*c, true))]),
                        LevelKey::FontColor(c) => crate::shell::i18n::format("_xlsx.font-color-color", &[("color", &colour_name(*c, false))]),
                        LevelKey::Icon(set, id) => crate::shell::i18n::format("xlsx.sort-icon-key", &[("set", set), ("id", &id.to_string())]),
                    };
                    let option_clone = option.clone();
                    list = list.child(mark(
                        &format!("key-{i}-{k}"),
                        Button::new(SharedString::from(format!("sort-key-{i}-{k}")))
                            .small()
                            .label(text)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                    if let Some(key) = p.keys.get_mut(i) {
                                        *key = option_clone.clone();
                                    }
                                    p.keying = None;
                                    if option_clone != LevelKey::Value {
                                        p.custom_open = None;
                                    }
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    ));
                }
                body = body.child(list);
            }
            let mut actions = h_flex()
                .gap_1()
                .child(mark(
                    &format!("up-{i}"),
                    Button::new(("sort-up", i))
                        .small()
                        .label("↑")
                        .disabled(i == 0)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                if i > 0 && i < p.levels.len() {
                                    p.levels.swap(i, i - 1);
                                    p.keys.swap(i, i - 1);
                                    p.custom_lists.swap(i, i - 1);
                                    if p.custom_open == Some(i) {
                                        p.custom_open = Some(i - 1);
                                    } else if p.custom_open == Some(i - 1) {
                                        p.custom_open = Some(i);
                                    }
                                }
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    &format!("down-{i}"),
                    Button::new(("sort-down", i))
                        .small()
                        .label("↓")
                        .disabled(i + 1 == p.levels.len())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                if i + 1 < p.levels.len() {
                                    p.levels.swap(i, i + 1);
                                    p.keys.swap(i, i + 1);
                                    p.custom_lists.swap(i, i + 1);
                                    if p.custom_open == Some(i) {
                                        p.custom_open = Some(i + 1);
                                    } else if p.custom_open == Some(i + 1) {
                                        p.custom_open = Some(i);
                                    }
                                }
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    &format!("copy-{i}"),
                    Button::new(("sort-copy", i))
                        .small()
                        .label(crate::shell::i18n::text("xlsx.sort-copy-level"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let Some(p) = this.wb().sort_edit.as_ref() else { return };
                            let Some((col_inp, asc)) = p.levels.get(i) else { return };
                            let col_val = col_inp.read(cx).value().to_string();
                            let asc_val = *asc;
                            let key_val = p.keys.get(i).cloned().unwrap_or(LevelKey::Value);
                            let custom_val = p.custom_lists.get(i).map(|c| c.read(cx).value().to_string()).unwrap_or_default();
                            let new_col = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.column-a-b")));
                            new_col.update(cx, |inp, cx| inp.set_value(col_val, window, cx));
                            let new_custom = custom_list_input(window, cx);
                            if !custom_val.is_empty() {
                                new_custom.update(cx, |inp, cx| inp.set_value(custom_val, window, cx));
                            }
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                let insert_at = (i + 1).min(p.levels.len());
                                p.levels.insert(insert_at, (new_col, asc_val));
                                p.keys.insert(insert_at, key_val);
                                p.custom_lists.insert(insert_at, new_custom);
                                if let Some(open) = p.custom_open {
                                    if open > i {
                                        p.custom_open = Some(open + 1);
                                    }
                                }
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    &format!("remove-{i}"),
                    Button::new(("sort-remove", i))
                        .small()
                        .label(crate::shell::i18n::text("_shell.delete"))
                        .disabled(p.levels.len() == 1)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                if p.levels.len() > 1 && i < p.levels.len() {
                                    p.levels.remove(i);
                                    p.keys.remove(i);
                                    p.custom_lists.remove(i);
                                    if p.custom_open == Some(i) {
                                        p.custom_open = None;
                                    } else if let Some(open) = p.custom_open {
                                        if open > i {
                                            p.custom_open = Some(open - 1);
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ));
            if key == LevelKey::Value {
                let custom_btn_label = if has_custom {
                    crate::shell::i18n::text("_xlsx.list-configured")
                } else {
                    crate::shell::i18n::text("_xlsx.list")
                };
                actions = actions.child(mark(
                    &format!("custom-btn-{i}"),
                    Button::new(("sort-custom-btn", i))
                        .small()
                        .label(custom_btn_label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                p.custom_open = if p.custom_open == Some(i) { None } else { Some(i) };
                                p.picking = None;
                                p.keying = None;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ));
            }
            body = body.child(actions);
        }
        body = body
            .child(mark(
                "add",
                Button::new("sort-add")
                    .small()
                    .label(crate::shell::i18n::text("_xlsx.add-column"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let Some(p) = this.wb().sort_edit.as_ref() else {
                            return;
                        };
                        let range_val = p.range.read(cx).value().to_string();
                        let levels_len = p.levels.len() as u32;
                        let column = this.resolve_sort_range(&range_val).map_or(0, |r| {
                            (r.first_column + levels_len).min(r.last_column)
                        });
                        let input = column_input(column, window, cx);
                        let custom_inp = custom_list_input(window, cx);
                        let p = this.wb_mut().sort_edit.as_mut().unwrap();
                        p.levels.push((input, true));
                        p.keys.push(LevelKey::Value);
                        p.custom_lists.push(custom_inp);
                        cx.notify();
                    }))
                    .into_any_element(),
            ))
            .child(
                div()
                    .text_size(px(12.))
                    .child(SharedString::from(p.error.clone())),
            )
            .child(mark(
                "apply",
                Button::new("sort-apply").primary()
                    .small()
                    .label(crate::shell::i18n::text("_xlsx.sort"))
                    .on_click(cx.listener(|this, _, _, cx| this.apply_sort_panel(cx)))
                    .into_any_element(),
            ))
            .child(mark(
                "close",
                Button::new("sort-close")
                    .small()
                    .label(crate::shell::i18n::text("_shell.close"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.wb_mut().sort_edit = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        Some(SidebarPanel {
            title: crate::shell::i18n::text("_xlsx.sort-2").into(),
            body: body.into_any_element(),
        })
    }
}

/// A colour in the lists: its code, or none / automatic.
fn colour_name(c: Option<ibis_doc_model::Rgb>, cell: bool) -> String {
    match c {
        Some(c) => format!("#{:06X}", c.0 & 0xFF_FFFF),
        None if cell => crate::shell::i18n::text("_xlsx.no-fill").into(),
        None => crate::shell::i18n::text("_xlsx.automatic").into(),
    }
}
