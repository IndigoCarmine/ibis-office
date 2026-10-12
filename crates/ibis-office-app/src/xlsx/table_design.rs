// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! テーブル デザイン (Excel's Table Design tab), as a panel: the table's
//! name, its header and 集計行 (each column's function), its style and its
//! options (縞模様, 最初の列, 最後の列, フィルター ボタン).

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Border, CellInput, CellRange, CellStyle, CellValue, Command, PutCell, Rgb, Sheet, SheetCommand, SheetTable, TableStyleInfo, TableTotal};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct TableDesignPanel {
    pub(crate) index: usize,
    pub(crate) name: Entity<InputState>,
    /// The column whose 集計行 function is being chosen.
    pub(crate) total_column: Option<usize>,
    pub(crate) message: String,
}

/// The functions of a 集計行 cell: (`totalsRowFunction`, SUBTOTAL's
/// number, the name shown), Excel's list.
pub(crate) const TOTALS: [(&str, u32, &str); 8] = [
    ("average", 101, "_xlsx.average"),
    ("count", 103, "_xlsx.count"),
    ("countNums", 102, "_xlsx.count-numbers"),
    ("max", 104, "_xlsx.max-2"),
    ("min", 105, "_xlsx.min"),
    ("sum", 109, "_xlsx.sum"),
    ("stdDev", 107, "_xlsx.stddev"),
    ("var", 110, "_xlsx.var"),
];

/// The built-in table styles, as Excel lists them.
fn style_names() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (prefix, label, count) in [("TableStyleLight", "_xlsx.light", 21), ("TableStyleMedium", "_xlsx.medium-2", 28), ("TableStyleDark", "_xlsx.dark", 11)] {
        for i in 1..=count {
            out.push((format!("{prefix}{i}"), format!("{} {i}", text(label))));
        }
    }
    out
}

/// A name a table can take: a letter, `_` or `\` first, then letters,
/// digits, `_`, `.` or `\`; not a cell's reference (`A1`, `R1C1`).
pub(crate) fn valid_table_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return false };
    if !(first.is_alphabetic() || first == '_' || first == '\\') || !chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\')) || name.chars().count() > 255 {
        return false;
    }
    let upper = name.to_ascii_uppercase();
    let letters = upper.chars().take_while(|c| c.is_ascii_uppercase()).count();
    let looks_like_cell = letters > 0 && letters <= 3 && upper.len() > letters && upper[letters..].chars().all(|c| c.is_ascii_digit());
    !looks_like_cell && !matches!(upper.as_str(), "R" | "C")
}

/// The formats the old style laid on `sheet`'s cells taken off: each of a
/// cell's fill, borders, bold and colour that is what the style gives it.
fn without_style(sheet: &Sheet, table: &SheetTable, theme: &[(String, Rgb)]) -> Sheet {
    let r = table.range;
    let mut bare = sheet.clone();
    for c in bare.cells.iter_mut().filter(|c| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)) {
        let mut st = sheet.styles[c.style as usize].clone();
        st.fill = None;
        st.left = None;
        st.right = None;
        st.top = None;
        st.bottom = None;
        st.bold = false;
        st.color = Rgb::BLACK;
        bare.styles.push(st);
        c.style = (bare.styles.len() - 1) as u32;
    }
    let overlay = ibis_pptx::styled_with_table(&bare, table, theme);
    let mut out = sheet.clone();
    for row in r.first_row..=r.last_row {
        for column in r.first_column..=r.last_column {
            let (Some(cell), Some(o)) = (sheet.cell(row, column), overlay.cell(row, column)) else { continue };
            let o = &overlay.styles[o.style as usize];
            let mut st = sheet.styles[cell.style as usize].clone();
            let before = st.clone();
            if o.fill.is_some() && st.fill == o.fill {
                st.fill = None;
            }
            let strip = |side: &mut Option<Border>, given: &Option<Border>| {
                if given.is_some() && side == given {
                    *side = None;
                }
            };
            strip(&mut st.left, &o.left);
            strip(&mut st.right, &o.right);
            strip(&mut st.top, &o.top);
            strip(&mut st.bottom, &o.bottom);
            if o.bold && st.bold {
                st.bold = false;
            }
            if o.color != Rgb::BLACK && st.color == o.color {
                st.color = Rgb::BLACK;
            }
            if st != before {
                let id = out.style_index(st);
                if let Some(c) = out.cells.iter_mut().find(|c| (c.row, c.column) == (row, column)) {
                    c.style = id;
                }
            }
        }
    }
    out
}

impl SlideEditor {
    /// The table the selected cell is in.
    pub(crate) fn table_at_selection(&self) -> Option<usize> {
        let (column, row) = self.current_cell()?;
        let (_, sheet) = self.current_sheet()?;
        sheet.tables.iter().position(|t| (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column))
    }

    /// 表示形式 > テーブルのデザイン: the panel of the table the selected cell is in.
    pub(crate) fn open_table_design(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.table_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-table"));
            return cx.notify();
        };
        let name = self.current_sheet().map(|(_, s)| s.tables[index].name.clone()).unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |i, cx| i.set_value(name, window, cx));
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.panels.close();
        s.panels.table_design = Some(TableDesignPanel { index, name: input, total_column: None, message: String::new() });
        self.sidebar_open = true;
        cx.notify();
    }

    fn table_message(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.table_design.as_mut() {
            p.message = message.into();
        }
        cx.notify();
    }

    /// The table `index` changed by `change` (which gives the cells to put
    /// in, or why not), its look laid again; one step to undo.
    pub(crate) fn change_sheet_table(&mut self, index: usize, change: impl FnOnce(&Sheet, &mut SheetTable) -> Result<Vec<(u32, u32, CellInput)>, String>, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        let theme = self.presentation().workbook.as_ref().map(|w| w.theme_colors.clone()).unwrap_or_default();
        let Some((_, sheet)) = self.current_sheet() else { return };
        let sheet: Sheet = (**sheet).clone();
        let Some(old) = sheet.tables.get(index).cloned() else { return };
        let mut table = old.clone();
        let contents = match change(&sheet, &mut table) {
            Ok(c) => c,
            Err(message) => return self.table_message(message, cx),
        };
        let mut base = without_style(&sheet, &old, &theme);
        for (row, column, input) in &contents {
            if matches!(input, CellInput::Clear) {
                // A row the table leaves (its 集計行, its headings) loses its look.
                if let Some(c) = base.cells.iter_mut().find(|c| (c.row, c.column) == (*row, *column)) {
                    c.style = 0;
                }
            }
        }
        let styled = ibis_pptx::styled_with_table(&base, &table, &theme);
        let area = |r: CellRange| (r.first_row..=r.last_row).flat_map(move |row| (r.first_column..=r.last_column).map(move |c| (row, c)));
        let mut cells: Vec<PutCell> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (row, column) in area(old.range).chain(area(table.range)) {
            if !seen.insert((row, column)) {
                continue;
            }
            let inside = (table.range.first_row..=table.range.last_row).contains(&row) && (table.range.first_column..=table.range.last_column).contains(&column);
            let style_before = sheet.cell(row, column).map(|c| sheet.styles[c.style as usize].clone());
            let style_after: Option<CellStyle> = if inside { styled.cell(row, column).map(|c| styled.styles[c.style as usize].clone()) } else { base.cell(row, column).map(|c| base.styles[c.style as usize].clone()) };
            let content = contents.iter().find(|(a, b, _)| (*a, *b) == (row, column)).map(|(_, _, i)| i.clone());
            if style_before == style_after && content.is_none() {
                continue;
            }
            let input = content.unwrap_or_else(|| match sheet.cell(row, column) {
                Some(c) => match &c.formula {
                    Some(f) => CellInput::Formula(f.clone()),
                    None if c.value.is_empty() => CellInput::Clear,
                    None => CellInput::Value(c.value.clone()),
                },
                None => CellInput::Clear,
            });
            cells.push(PutCell { row, column, input, style: Some(style_after.unwrap_or_else(|| sheet.styles.first().cloned().unwrap_or_default())) });
        }
        self.status = match self.document.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index, table: Some(table), cells })]) {
            Ok(_) => None,
            Err(err) => Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into()),
        };
        self.table_message(self.status.clone().map(|s| s.to_string()).unwrap_or_default(), cx);
    }

    /// テーブル名: the name typed, if it can be one (the formulas follow).
    pub(crate) fn rename_table(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.table_design.as_ref()) else { return };
        let (index, name) = (p.index, p.name.read(cx).value().trim().to_string());
        let taken: Vec<String> = self.presentation().slides.iter().filter_map(ibis_doc_model::sheet_calc::sheet_of_slide).flat_map(|s| s.tables.iter().map(|t| t.name.clone())).collect();
        let names: Vec<String> = self.presentation().workbook.as_ref().map(|w| w.names.iter().map(|n| n.name.clone()).collect()).unwrap_or_default();
        let current = self.current_sheet().and_then(|(_, s)| s.tables.get(index).map(|t| t.name.clone())).unwrap_or_default();
        if name == current {
            return;
        }
        if !valid_table_name(&name) {
            return self.table_message(text("_xlsx.this-table-name-is-not-valid"), cx);
        }
        if taken.iter().chain(&names).any(|t| t.eq_ignore_ascii_case(&name) && !t.eq_ignore_ascii_case(&current)) {
            return self.table_message(text("_xlsx.this-name-is-already-in-use"), cx);
        }
        self.change_sheet_table(index, move |_, t| {
            t.name = name;
            Ok(Vec::new())
        }, cx);
    }

    /// 集計行 on (a row under the table: 集計 in its first column, the last
    /// column's SUBTOTAL — 合計 of numbers, else 個数) or off.
    pub(crate) fn toggle_totals_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.change_sheet_table(index, |sheet, t| {
            let r = t.range;
            if t.totals_rows > 0 {
                let row = r.last_row;
                t.range.last_row -= t.totals_rows;
                t.totals_rows = 0;
                t.totals.clear();
                return Ok((r.first_column..=r.last_column).map(|c| (row, c, CellInput::Clear)).collect());
            }
            let row = r.last_row + 1;
            if row as usize >= sheet.rows.len() || (r.first_column..=r.last_column).any(|c| sheet.cell(row, c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some())) {
                return Err(text("_xlsx.the-cells-just-below-the-table-are-not-empty-so").to_string());
            }
            let last = r.last_column;
            let numbers = ((r.first_row + t.header_rows)..=r.last_row).any(|row| matches!(sheet.cell(row, last).map(|c| &c.value), Some(CellValue::Number(_))));
            let (function, code) = if numbers { ("sum", 109) } else { ("count", 103) };
            let column = t.columns.last().cloned().unwrap_or_default();
            t.range.last_row = row;
            t.totals_rows = 1;
            t.totals = (0..t.columns.len()).map(|_| TableTotal::default()).collect();
            let mut contents = vec![(row, last, CellInput::Formula(format!("SUBTOTAL({code},{}[{}])", t.name, escape_column(&column))))];
            if let Some(tt) = t.totals.last_mut() {
                tt.function = Some(function.into());
            }
            if r.first_column != last {
                contents.push((row, r.first_column, CellInput::Value(CellValue::Text(text("_xlsx.summarize").to_string()))));
                if let Some(tt) = t.totals.first_mut() {
                    tt.label = Some(text("_xlsx.summarize").to_string());
                }
            }
            Ok(contents)
        }, cx);
    }

    /// A column's 集計行 cell: `function` (see [`TOTALS`]) or none.
    pub(crate) fn set_table_total(&mut self, index: usize, column: usize, function: Option<&'static str>, cx: &mut Context<Self>) {
        self.change_sheet_table(index, move |_, t| {
            if t.totals_rows == 0 {
                return Ok(Vec::new());
            }
            let row = t.range.last_row;
            let c = t.range.first_column + column as u32;
            if t.totals.len() < t.columns.len() {
                t.totals.resize(t.columns.len(), TableTotal::default());
            }
            let name = t.columns.get(column).cloned().unwrap_or_default();
            let input = match function.and_then(|f| TOTALS.iter().find(|x| x.0 == f)) {
                Some((f, code, _)) => {
                    t.totals[column] = TableTotal { function: Some((*f).into()), label: None };
                    CellInput::Formula(format!("SUBTOTAL({code},{}[{}])", t.name, escape_column(&name)))
                }
                None => {
                    t.totals[column] = TableTotal::default();
                    CellInput::Clear
                }
            };
            Ok(vec![(row, c, input)])
        }, cx);
    }

    /// 見出し行 off (its row emptied, the table starting under it) or on
    /// (the columns' names in the row above, which must be empty).
    pub(crate) fn toggle_header_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.change_sheet_table(index, |sheet, t| {
            let r = t.range;
            if t.header_rows > 0 {
                if r.last_row - r.first_row < 1 + t.totals_rows {
                    return Err(text("_xlsx.the-table-has-only-its-header-row").to_string());
                }
                t.range.first_row += 1;
                t.header_rows = 0;
                t.filter = None;
                return Ok((r.first_column..=r.last_column).map(|c| (r.first_row, c, CellInput::Clear)).collect());
            }
            if r.first_row == 0 || (r.first_column..=r.last_column).any(|c| sheet.cell(r.first_row - 1, c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some())) {
                return Err(text("_xlsx.the-cells-just-above-the-table-are-not-empty-so").to_string());
            }
            let row = r.first_row - 1;
            t.range.first_row = row;
            t.header_rows = 1;
            t.filter = Some(ibis_doc_model::AutoFilter { range: CellRange { last_row: t.range.last_row - t.totals_rows, ..t.range }, columns: Vec::new() });
            Ok(t.columns.iter().enumerate().map(|(i, name)| (row, r.first_column + i as u32, CellInput::Value(CellValue::Text(name.clone())))).collect())
        }, cx);
    }

    /// フィルター ボタン on or off.
    pub(crate) fn toggle_table_filter_button(&mut self, index: usize, cx: &mut Context<Self>) {
        self.change_sheet_table(index, |_, t| {
            if t.header_rows == 0 {
                return Ok(Vec::new());
            }
            t.filter = match t.filter {
                Some(_) => None,
                None => Some(ibis_doc_model::AutoFilter { range: CellRange { last_row: t.range.last_row - t.totals_rows, ..t.range }, columns: Vec::new() }),
            };
            Ok(Vec::new())
        }, cx);
    }

    /// The style's options, or another style.
    pub(crate) fn set_table_style(&mut self, index: usize, change: impl FnOnce(&mut TableStyleInfo) + 'static, cx: &mut Context<Self>) {
        self.change_sheet_table(index, move |_, t| {
            let mut st = t.style.clone().unwrap_or_else(|| TableStyleInfo { name: "TableStyleMedium2".into(), row_stripes: true, ..Default::default() });
            change(&mut st);
            t.style = Some(st);
            Ok(Vec::new())
        }, cx);
    }

    pub(crate) fn table_design_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.table_design.as_ref()?;
        let index = p.index;
        let (_, sheet) = self.current_sheet()?;
        let t = sheet.tables.get(index)?.clone();
        let message = p.message.clone();
        let total_column = p.total_column;
        let st = t.style.clone().unwrap_or_default();
        let check = |on: bool, label: &str| format!("{} {}", if on { "☑" } else { "☐" }, text(label));
        let mut body = v_flex()
            .gap_1()
            .px(px(18.))
            .py(px(8.))
            .child(text("_xlsx.table-name"))
            .child(h_flex().gap_1().child(div().flex_1().child(Input::new(&p.name).small())).child(Button::new("table-rename").small().label(text("_xlsx.change")).on_click(cx.listener(|this, _, _, cx| this.rename_table(cx)))))
            .child(div().pt(px(6.)).child(text("_xlsx.table-style-options")))
            .child(Button::new("table-header").small().label(check(t.header_rows > 0, "docx.table-design.header-row")).on_click(cx.listener(move |this, _, _, cx| this.toggle_header_row(index, cx))))
            .child(Button::new("table-totals").small().label(check(t.totals_rows > 0, "docx.table-design.total-row")).on_click(cx.listener(move |this, _, _, cx| this.toggle_totals_row(index, cx))))
            .child(Button::new("table-row-stripes").small().label(check(st.row_stripes, "_xlsx.banded-rows")).on_click(cx.listener(move |this, _, _, cx| this.set_table_style(index, |s| s.row_stripes = !s.row_stripes, cx))))
            .child(Button::new("table-column-stripes").small().label(check(st.column_stripes, "_xlsx.banded-columns")).on_click(cx.listener(move |this, _, _, cx| this.set_table_style(index, |s| s.column_stripes = !s.column_stripes, cx))))
            .child(Button::new("table-first-column").small().label(check(st.first_column, "docx.table-design.first-column")).on_click(cx.listener(move |this, _, _, cx| this.set_table_style(index, |s| s.first_column = !s.first_column, cx))))
            .child(Button::new("table-last-column").small().label(check(st.last_column, "docx.table-design.last-column")).on_click(cx.listener(move |this, _, _, cx| this.set_table_style(index, |s| s.last_column = !s.last_column, cx))))
            .child(Button::new("table-filter-button").small().label(check(t.filter.is_some(), "_xlsx.filter-button")).disabled(t.header_rows == 0).on_click(cx.listener(move |this, _, _, cx| this.toggle_table_filter_button(index, cx))));
        if t.totals_rows > 0 {
            body = body.child(div().pt(px(6.)).child(text("_xlsx.total-row-functions")));
            for (i, name) in t.columns.iter().enumerate() {
                let current = t.totals.get(i).and_then(|x| x.function.clone());
                let shown = current.as_deref().and_then(|f| TOTALS.iter().find(|x| x.0 == f)).map_or(text("_shell.none").to_string(), |x| text(x.2).to_string());
                body = body.child(Button::new(SharedString::from(format!("table-total-{i}"))).small().label(format!("{name}: {shown}")).on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.table_design.as_mut() {
                        p.total_column = if p.total_column == Some(i) { None } else { Some(i) };
                    }
                    cx.notify();
                })));
                if total_column == Some(i) {
                    let mut choices = h_flex().gap_1().flex_wrap().pl(px(12.)).child(Button::new(SharedString::from(format!("table-total-{i}-none"))).small().label(text("_shell.none")).on_click(cx.listener(move |this, _, _, cx| this.set_table_total(index, i, None, cx))));
                    for (f, _, label) in TOTALS {
                        let on = current.as_deref() == Some(f);
                        choices = choices.child(Button::new(SharedString::from(format!("table-total-{i}-{f}"))).small().label(format!("{}{}", if on { "◉ " } else { "" }, text(label))).on_click(cx.listener(move |this, _, _, cx| this.set_table_total(index, i, Some(f), cx))));
                    }
                    body = body.child(choices);
                }
            }
        }
        body = body.child(div().pt(px(6.)).child(text("_xlsx.table-styles")));
        let mut styles = h_flex().gap_1().flex_wrap();
        for (name, label) in style_names() {
            let on = st.name == name;
            let chosen = name.clone();
            styles = styles.child(Button::new(SharedString::from(format!("table-style-{name}"))).small().label(format!("{}{label}", if on { "◉ " } else { "" })).on_click(cx.listener(move |this, _, _, cx| {
                let chosen = chosen.clone();
                this.set_table_style(index, move |s| s.name = chosen, cx)
            })));
        }
        body = body.child(styles).child(div().text_size(px(12.)).child(SharedString::from(message))).child(Button::new("table-design-close").small().label(text("_shell.close")).on_click(cx.listener(|this, _, _, cx| {
            this.wb_mut().panels.table_design = None;
            cx.notify();
        })));
        Some(SidebarPanel { title: "_xlsx.table-design".into(), body: body.into_any_element() })
    }
}

/// A column's name inside a structured reference (`[` `]` `#` `'` escaped
/// with `'`).
fn escape_column(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if matches!(c, '[' | ']' | '#' | '\'') {
            out.push('\'');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::valid_table_name;

    #[test]
    fn table_names_as_excel_takes_them() {
        assert!(valid_table_name("売上表"));
        assert!(valid_table_name("Sales_2026"));
        assert!(valid_table_name("_t"));
        assert!(!valid_table_name("A1"));
        assert!(!valid_table_name("XFD100"));
        assert!(!valid_table_name("1st"));
        assert!(!valid_table_name("a b"));
        assert!(!valid_table_name("R"));
        assert!(!valid_table_name(""));
    }
}
