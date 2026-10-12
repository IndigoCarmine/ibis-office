// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A workbook's toolbar and menus (the mock's ⑤-4 「スプレッドシート：セル
//! を選択中」): the buttons show the selected cell's format and change the
//! selected range's; the menus insert and delete rows, columns and sheets.
//! The editing itself is the model's commands (`sheet_command.rs`).

use gpui_kit::*;
use ibis_doc_model::{
    Border, BorderLine, CellAlign, CellRange, CellStyle, CellVerticalAlign, Command, Edge, Rgb, SheetCommand, SlideId, StyleChange, sheet_command::Axis,
};

use crate::shell::chrome::{Handler, MenuEntry, MenuItem, Menus, Submenu, ToolButton, ToolItem};
use crate::shell::editor::SlideEditor;

fn sep() -> ToolItem {
    ToolItem::Separator
}

fn handler(f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Handler {
    std::rc::Rc::new(f)
}

fn fill_submenu(menus: &mut Menus, id: &str, items: Vec<MenuEntry>) {
    fn find<'a>(entries: &'a mut [MenuEntry], id: &str) -> Option<&'a mut Submenu> {
        for entry in entries {
            if let MenuEntry::Submenu(sub) = entry {
                if sub.id == id {
                    return Some(sub);
                }
                if let Some(found) = find(&mut sub.items, id) {
                    return Some(found);
                }
            }
        }
        None
    }
    for (_, entries) in &mut menus.menus {
        if let Some(sub) = find(entries, id) {
            sub.items = items;
            return;
        }
    }
}

pub(super) const FONTS: &[&str] = &["游ゴシック", "游明朝", "ＭＳ Ｐゴシック", "ＭＳ ゴシック", "メイリオ", "BIZ UDPゴシック", "Arial", "Calibri", "Times New Roman"]; // i18n-ok（フォント名。メニューの表示と、保存する値に使う（訳さない））
/// Fill colours (the light ones first, as a highlight).
/// The theme colours in SpreadsheetML's order: Excel's names, the menu ids, and the tints its palette gives each (millionths).
const THEME_NAMES: [&str; 10] = ["_xlsx.background-1", "_xlsx.text-1", "_xlsx.background-2", "_xlsx.text-2", "_xlsx.accent-1", "_xlsx.accent-2", "_xlsx.accent-3", "_xlsx.accent-4", "_xlsx.accent-5", "_xlsx.accent-6"];
const THEME_IDS: [&str; 10] = ["xlsx.theme.0", "xlsx.theme.1", "xlsx.theme.2", "xlsx.theme.3", "xlsx.theme.4", "xlsx.theme.5", "xlsx.theme.6", "xlsx.theme.7", "xlsx.theme.8", "xlsx.theme.9"];
const THEME_TINTS: [[i32; 6]; 10] = [
    [0, -50_000, -150_000, -250_000, -350_000, -500_000],
    [0, 500_000, 350_000, 250_000, 150_000, 50_000],
    [0, -100_000, -250_000, -500_000, -750_000, -900_000],
    [0, 900_000, 750_000, 500_000, 250_000, 100_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
];
pub(super) const FILLS: &[(&str, u32)] = &[("_xlsx.light-yellow", 0xfff2cc), ("_shell.light-green", 0xe2efda), ("_shell.light-blue", 0xddebf7), ("_xlsx.light-orange", 0xfce4d6), ("_xlsx.light-gray", 0xededed), ("_shell.yellow", 0xffff00), ("_shell.green", 0x92d050), ("_shell.blue", 0x9bc2e6), ("_shell.gray", 0xbfbfbf), ("_shell.black", 0x000000)];
pub(super) const COLORS: &[(&str, u32)] = &[("_xlsx.black", 0x000000), ("_shell.dark-red", 0xc00000), ("_shell.red", 0xff0000), ("_shell.orange", 0xffc000), ("_shell.yellow", 0xffff00), ("_shell.light-green", 0x92d050), ("_shell.green", 0x00b050), ("_shell.light-blue", 0x00b0f0), ("_shell.blue", 0x0070c0), ("_shell.dark-blue", 0x002060), ("_shell.purple", 0x7030a0), ("_shell.white", 0xffffff)];
/// Number formats of 表示形式の詳細 (123 ▼).
pub(super) const NUMBER_FORMATS: &[(&str, &str)] = &[
    ("_xlsx.general", "General"),
    ("_xlsx.number", "#,##0"),
    ("_xlsx.number-2-decimal-places-2", "#,##0.00"),
    ("_xlsx.currency", "\"¥\"#,##0;\"¥\"\\-#,##0"),
    ("_xlsx.accounting", "_ \"¥\"* #,##0_ ;_ \"¥\"* \\-#,##0_ ;_ \"¥\"* \"-\"_ ;_ @_ "),
    ("docx.field.date", "yyyy/m/d"),
    ("_xlsx.date-year-month-day", "yyyy\"年\"m\"月\"d\"日\""), // i18n-ok（表示形式の書式コード（ファイルに保存される値））
    ("docx.field.time", "h:mm"),
    ("_xlsx.percentage", "0%"),
    ("_xlsx.fraction", "# ?/?"),
    ("_xlsx.scientific", "0.00E+00"),
    ("_xlsx.text", "@"),
];

impl SlideEditor {
    /// The selected cell's format.
    pub(crate) fn current_style(&self) -> CellStyle {
        let default = || self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
        match (self.current_sheet(), self.current_cell()) {
            (Some((_, sheet)), Some((column, row))) => sheet.cell(row, column).map_or_else(default, |c| sheet.styles[c.style as usize].clone()),
            _ => default(),
        }
    }

    pub(crate) fn current_style_bold(&self) -> bool {
        self.current_style().bold
    }

    pub(crate) fn current_style_italic(&self) -> bool {
        self.current_style().italic
    }

    pub(crate) fn current_style_underline(&self) -> bool {
        self.current_style().underline.enabled()
    }

    /// Change the selected range's format (one undo step).
    pub(crate) fn format_cells(&mut self, change: StyleChange, cx: &mut Context<Self>) {
        // Text selected while typing: the font change is for that part alone.
        if self.format_edit_selection(&change, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        if self.protection_refuses(super::protection::Guarded::FormatCells, cx) {
            return;
        }
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        let wrap_on = matches!(change, StyleChange::Wrap(true) | StyleChange::Rotation(1..=255));
        // With the ranges 条件を選択してジャンプ selected with it.
        let format = Command::Sheet(SheetCommand::SetFormat { slide, ranges: self.selected_ranges(), change });
        self.run_sheet(vec![format.clone()], cx);
        // Wrapping or turning on: the rows grow to the lines, as Excel's do (in the same step).
        if wrap_on {
            let ts = cx.text_system().clone();
            let fit = self.fit_rows_commands(range.first_row, range.last_row, &ts);
            if !fit.is_empty() {
                self.document.undo();
                let mut commands = vec![format];
                commands.extend(fit);
                self.run_sheet(commands, cx);
            }
        }
    }

    pub(crate) fn run_sheet(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.last_fill = None;
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("edit rejected: {err}");
            self.status = Some(crate::shell::i18n::format("_xlsx.this-operation-can-t-be-done-err", &[("err", &err)]).into());
        }
        if self.presentation().slide(self.current_slide).is_none() {
            self.current_slide = self.presentation().slides[0].id;
        }
        self.sync_workbook_peers(cx);
        cx.notify();
    }

    /// Borders on the selected range: every line (格子), the outside, the bottom, or none.
    pub(super) fn set_borders(&mut self, kind: &'static str, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        let slide = self.current_slide;
        let chosen = Border { line: self.wb().border_line, color: self.wb().border_color };
        let line = Some(chosen);
        let edge = |ranges: Vec<CellRange>, e: Edge, b: Option<Border>| Command::Sheet(SheetCommand::SetFormat { slide, ranges, change: StyleChange::Border(e, b) });
        let row = |i: u32| CellRange { first_row: i, last_row: i, ..r };
        let column = |i: u32| CellRange { first_column: i, last_column: i, ..r };
        let commands = match kind {
            "all" => [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom].into_iter().map(|e| edge(vec![r], e, line)).collect(),
            "outside" => vec![edge(vec![row(r.first_row)], Edge::Top, line), edge(vec![row(r.last_row)], Edge::Bottom, line), edge(vec![column(r.first_column)], Edge::Left, line), edge(vec![column(r.last_column)], Edge::Right, line)],
            "bottom" => vec![edge(vec![row(r.last_row)], Edge::Bottom, line)],
            "top" => vec![edge(vec![row(r.first_row)], Edge::Top, line)],
            "left" => vec![edge(vec![column(r.first_column)], Edge::Left, line)],
            "right" => vec![edge(vec![column(r.last_column)], Edge::Right, line)],
            "diagonal-up" => vec![edge(vec![r], Edge::DiagonalUp, line)],
            "diagonal-down" => vec![edge(vec![r], Edge::DiagonalDown, line)],
            "thick-bottom" => vec![edge(vec![row(r.last_row)], Edge::Bottom, Some(Border { line: BorderLine::Thick, ..chosen }))],
            "double-bottom" => vec![edge(vec![row(r.last_row)], Edge::Bottom, Some(Border { line: BorderLine::Double, ..chosen }))],
            _ => [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom, Edge::DiagonalUp, Edge::DiagonalDown].into_iter().map(|e| edge(vec![r], e, None)).collect(),
        };
        self.run_sheet(commands, cx);
    }

    fn merge_cells(&mut self, center: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Merge { slide, range, center })], cx);
    }

    /// 横方向に結合: each row of the selection merged on its own.
    fn merge_across(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        if r.first_column == r.last_column {
            return;
        }
        let slide = self.current_slide;
        let commands = (r.first_row..=r.last_row).map(|row| Command::Sheet(SheetCommand::Merge { slide, range: CellRange { first_row: row, last_row: row, ..r }, center: false })).collect();
        self.run_sheet(commands, cx);
    }

    /// The status bar's 平均・データの個数・合計 of the selected range (more
    /// than one cell; hidden rows and columns left out), in the selected
    /// cell's number format.
    pub(crate) fn range_summary(&self) -> Option<String> {
        let (ranges, (_, sheet)) = (self.selected_ranges(), self.current_sheet()?);
        if ranges.len() < 2 && ranges.iter().all(|r| r.first_row == r.last_row && r.first_column == r.last_column) {
            return None;
        }
        // Over all the ranges selected (⌘-click), each cell once (not
        // measured against Excel for overlapping ranges).
        let inside = |c: &ibis_doc_model::SheetCell| ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column));
        let (mut count, mut numbers, mut sum) = (0usize, 0usize, 0.0f64);
        for c in &sheet.cells {
            if !inside(c) {
                continue;
            }
            if sheet.rows.get(c.row as usize) == Some(&0) || sheet.columns.get(c.column as usize) == Some(&0) || c.value.is_empty() {
                continue;
            }
            count += 1;
            if let ibis_doc_model::CellValue::Number(n) = c.value {
                numbers += 1;
                sum += n;
            }
        }
        // Excel shows them from two cells with something in them.
        if count < 2 {
            return None;
        }
        let code = self.current_style().number_format;
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let show = |v: f64| ibis_calc::numfmt::format(&ibis_doc_model::CellValue::Number(v), &code, date1904).text;
        Some(if numbers == 0 {
            crate::shell::i18n::format("_xlsx.count-count", &[("count", &count)])
        } else {
            crate::shell::i18n::format("_xlsx.average-average-count-count-sum-sum", &[("average", &show(sum / numbers as f64)), ("count", &count), ("sum", &show(sum))])
        })
    }

    /// Select every cell of the sheet (⌘A).
    pub(crate) fn select_all_cells(&mut self, cx: &mut Context<Self>) {
        if self.cell_edit().is_some_and(|e| e.mode == super::cell_edit::EditMode::Edit) && self.cell_select_all_text(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let end = (sheet.columns.len().max(1) as u32 - 1, sheet.rows.len().max(1) as u32 - 1);
        let slide = self.current_slide;
        self.selection = None;
        let state = self.wb_mut();
        state.cells.insert(slide, (0, 0));
        state.cell_ends.insert(slide, end);
        cx.notify();
    }

    /// すべてクリア: the selected cells' values and formats.
    pub(crate) fn clear_all_cells(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let (Some(range), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) else { return };
        let edits: Vec<ibis_doc_model::CellEdit> = sheet
            .cells
            .iter()
            .filter(|c| (range.first_row..=range.last_row).contains(&c.row) && (range.first_column..=range.last_column).contains(&c.column))
            .filter(|c| !c.value.is_empty() || c.formula.is_some())
            .map(|c| ibis_doc_model::CellEdit { row: c.row, column: c.column, input: ibis_doc_model::CellInput::Clear })
            .collect();
        let slide = self.current_slide;
        let mut commands = Vec::new();
        if !edits.is_empty() {
            commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
        }
        commands.push(Command::Sheet(SheetCommand::ClearFormats { slide, ranges: vec![range] }));
        self.run_sheet(commands, cx);
    }

    fn unmerge_cells(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Unmerge { slide, range })], cx);
    }

    /// Insert rows (or columns) at the selection, before it or after it.
    pub(crate) fn insert_lines(&mut self, axis: Axis, after: bool, cx: &mut Context<Self>) {
        let guard = if axis == Axis::Rows { super::protection::Guarded::InsertRows } else { super::protection::Guarded::InsertColumns };
        if self.protection_refuses(guard, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        let (first, last) = match axis {
            Axis::Rows => (r.first_row, r.last_row),
            Axis::Columns => (r.first_column, r.last_column),
        };
        let slide = self.current_slide;
        let at = if after { last + 1 } else { first };
        self.run_sheet(vec![Command::Sheet(SheetCommand::Insert { slide, axis, at, count: last - first + 1 })], cx);
    }

    pub(crate) fn delete_lines(&mut self, axis: Axis, cx: &mut Context<Self>) {
        let guard = if axis == Axis::Rows { super::protection::Guarded::DeleteRows } else { super::protection::Guarded::DeleteColumns };
        if self.protection_refuses(guard, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        let (first, last) = match axis {
            Axis::Rows => (r.first_row, r.last_row),
            Axis::Columns => (r.first_column, r.last_column),
        };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Delete { slide, axis, at: first, count: last - first + 1 })], cx);
    }

    /// Hide the selected rows (or columns), or show the hidden ones among them.
    pub(crate) fn hide_lines(&mut self, axis: Axis, hidden: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        let (first, last) = match axis {
            Axis::Rows => (r.first_row, r.last_row),
            Axis::Columns => (r.first_column, r.last_column),
        };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Hide { slide, axis, first, last, hidden })]) {
            log::debug!("nothing to hide or show: {err}");
        }
        cx.notify();
    }

    /// A new sheet after the current one, named シートN.
    pub(crate) fn insert_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(wb) = self.presentation().workbook.clone() else { return };
        let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide)).map_or(wb.sheets.len(), |i| i + 1);
        let mut n = wb.sheets.len() + 1;
        let name = loop {
            let name = format!("Sheet{n}");
            if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, None) {
                break name;
            }
            n += 1;
        };
        let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
        self.run_sheet(vec![Command::Sheet(SheetCommand::AddSheet { index, name, slide, shape })], cx);
        if self.presentation().slide(slide).is_some() {
            self.select_slide(slide, window, cx);
        }
    }

    fn delete_sheet(&mut self, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::DeleteSheet { slide })], cx);
    }

    /// ウィンドウ枠の固定: the rows above and columns left of the selected
    /// cell stay in view (A1: the top row's… nothing); again, unfrozen.
    pub(crate) fn toggle_freeze(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let slide = self.current_slide;
        let frozen = match sheet.frozen {
            Some(_) => None,
            None => self.current_cell().filter(|&(c, r)| c > 0 || r > 0),
        };
        if frozen.is_none() && sheet.frozen.is_none() {
            return;
        }
        // Frozen panes take the place of split ones (表示 > 分割).
        self.wb_mut().panels.splits.remove(&slide);
        self.run_sheet(vec![Command::Sheet(SheetCommand::Freeze { slide, frozen })], cx);
    }

    /// セルの挿入・削除 at the selected range, the cells beside moving down
    /// or right (up or left).
    pub(crate) fn shift_cells(&mut self, down: bool, insert: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::ShiftCells { slide, range, down, insert })]) {
            log::warn!("shifting cells rejected: {err}");
            self.status = Some(crate::shell::i18n::text("_xlsx.cannot-move-part-of-a-merged-cell"));
        }
        cx.notify();
    }

    /// Ask where, and write the sheet shown there as CSV.
    pub(crate) fn export_csv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap_or_else(|| ".".into());
            let stem = self.path.as_deref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Sheet1".into());
            let name = format!("{stem}.csv");
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    let r = this.write_csv(&path);
                    this.status = Some(match r {
                        Ok(()) => crate::shell::i18n::format("_xlsx.exported-to-path", &[("path", &path.display())]).into(),
                        Err(e) => crate::shell::i18n::format("_xlsx.couldn-t-export-error", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve"));
            cx.notify();
        }
    }

    /// The sheet shown, as CSV, to `path`.
    pub(crate) fn write_csv(&self, path: &std::path::Path) -> std::io::Result<()> {
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        std::fs::write(path, ibis_pptx::csv::sheet_to_csv(self.presentation(), index))
    }

    /// Ask where, and write the sheet shown there as Shift_JIS CSV.
    pub(crate) fn export_csv_sjis(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap_or_else(|| ".".into());
            let stem = self.path.as_deref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Sheet1".into());
            let name = format!("{stem}.csv");
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    let r = this.write_csv_sjis(&path);
                    this.status = Some(match r {
                        Ok(()) => crate::shell::i18n::format("_xlsx.exported-to-path", &[("path", &path.display())]).into(),
                        Err(e) => crate::shell::i18n::format("_xlsx.couldn-t-export-error", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve"));
            cx.notify();
        }
    }

    /// Ask where, and write the sheet shown there as TSV (tab-separated values).
    pub(crate) fn export_tsv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap_or_else(|| ".".into());
            let stem = self.path.as_deref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Sheet1".into());
            let name = format!("{stem}.tsv");
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    let r = this.write_tsv(&path);
                    this.status = Some(match r {
                        Ok(()) => crate::shell::i18n::format("_xlsx.exported-to-path", &[("path", &path.display())]).into(),
                        Err(e) => crate::shell::i18n::format("_xlsx.couldn-t-export-error", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve"));
            cx.notify();
        }
    }

    /// The sheet shown, as Shift_JIS CSV, to `path`.
    pub(crate) fn write_csv_sjis(&self, path: &std::path::Path) -> std::io::Result<()> {
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        std::fs::write(path, ibis_pptx::csv::sheet_to_csv_sjis(self.presentation(), index))
    }

    /// The sheet shown, as TSV, to `path`.
    pub(crate) fn write_tsv(&self, path: &std::path::Path) -> std::io::Result<()> {
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        std::fs::write(path, ibis_pptx::csv::sheet_to_tsv(self.presentation(), index))
    }

    /// The axis an outline acts on: columns when whole columns are selected, else rows.
    fn outline_axis(&self) -> (Axis, u32, u32) {
        let r = self.selected_range();
        match (self.selected_lines(Axis::Columns), r) {
            (Some((a, b)), _) => (Axis::Columns, a, b),
            (None, Some(r)) => (Axis::Rows, r.first_row, r.last_row),
            _ => (Axis::Rows, 0, 0),
        }
    }

    /// グループ化 (`deeper`) or グループ解除 of the selected rows (or columns).
    pub(crate) fn group_lines(&mut self, deeper: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let (axis, first, last) = self.outline_axis();
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Group { slide, axis, first, last, deeper })], cx);
    }

    /// 詳細を表示しない (`collapse`) or 詳細を表示: the group the selected row
    /// (or column) is in hidden or shown.
    pub(crate) fn collapse_group(&mut self, collapse: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let (axis, at, _) = self.outline_axis();
        let levels = if axis == Axis::Rows { &sheet.outline_rows } else { &sheet.outline_columns };
        let Some(&level) = levels.get(&at) else { return };
        let (mut first, mut last) = (at, at);
        while first > 0 && levels.get(&(first - 1)).is_some_and(|&l| l >= level) {
            first -= 1;
        }
        while levels.get(&(last + 1)).is_some_and(|&l| l >= level) {
            last += 1;
        }
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Hide { slide, axis, first, last, hidden: collapse })], cx);
    }

    /// シートを複製: a copy right after it, named as Excel names it (「月次売上 (2)」).
    pub(crate) fn copy_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        let Some(base) = self.slide().name.clone() else { return };
        let mut n = 2;
        let name = loop {
            let name = format!("{base} ({n})");
            if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, None) {
                break name;
            }
            n += 1;
        };
        let (new_slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
        self.run_sheet(vec![Command::Sheet(SheetCommand::CopySheet { slide, name, new_slide, shape })], cx);
        if self.presentation().slide(new_slide).is_some() {
            self.select_slide(new_slide, window, cx);
        }
    }

    /// 並べ替え: the table around the selected cell (or the selected range)
    /// sorted by the selected cell's column; a heading row (text over a
    /// column of numbers, or bold) stays on top, as Excel guesses.
    pub(crate) fn sort_selection(&self) -> Option<(CellRange, u32, bool)> {
        let ((column, row), (_, sheet)) = (self.current_cell()?, self.current_sheet()?);
        let filled = |r: u32, c: u32| sheet.cell(r, c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some());
        let range = match self.selected_range() {
            Some(r) if r.first_row != r.last_row => r,
            _ => {
                // The current region: grow while the rows and columns next to it hold something.
                let mut r = CellRange { first_row: row, first_column: column, last_row: row, last_column: column };
                loop {
                    let mut grown = r;
                    if r.first_row > 0 && (r.first_column..=r.last_column).any(|c| filled(r.first_row - 1, c)) {
                        grown.first_row -= 1;
                    }
                    if (r.first_column..=r.last_column).any(|c| filled(r.last_row + 1, c)) {
                        grown.last_row += 1;
                    }
                    if r.first_column > 0 && (r.first_row..=r.last_row).any(|x| filled(x, r.first_column - 1)) {
                        grown.first_column -= 1;
                    }
                    if (r.first_row..=r.last_row).any(|x| filled(x, r.last_column + 1)) {
                        grown.last_column += 1;
                    }
                    if grown == r {
                        break r;
                    }
                    r = grown;
                }
            }
        };
        let top = |c: u32| sheet.cell(range.first_row, c);
        let below = |c: u32| sheet.cell(range.first_row + 1, c);
        let header = (range.first_column..=range.last_column).any(|c| {
            let t = top(c);
            t.is_some_and(|x| sheet.styles[x.style as usize].bold) || (matches!(t.map(|x| &x.value), Some(ibis_doc_model::CellValue::Text(_))) && matches!(below(c).map(|x| &x.value), Some(ibis_doc_model::CellValue::Number(_))))
        });
        Some((range, column, header))
    }

    pub(crate) fn sort_by_selected_column(&mut self, ascending: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Sort, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((range, column, header)) = self.sort_selection() else { return };
        self.run_sheet(vec![Command::Sheet(SheetCommand::Sort { slide: self.current_slide, range, keys: vec![(column, ascending)], header })], cx);
    }

    /// The current sheet's tab colour.
    pub(crate) fn set_tab_color(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::SetTabColor { slide, color })], cx);
    }

    /// The current sheet one place left (`-1`) or right (`1`).
    pub(crate) fn move_sheet(&mut self, step: i32, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        let Some(wb) = self.presentation().workbook.clone() else { return };
        let Some(at) = wb.sheets.iter().position(|s| s.slide == Some(slide)) else { return };
        let to = at as i64 + step as i64;
        if to < 0 || to as usize >= wb.sheets.len() {
            return;
        }
        self.run_sheet(vec![Command::Sheet(SheetCommand::MoveSheet { slide, to: to as usize })], cx);
    }

    /// Hide the current sheet (シートを非表示).
    pub(crate) fn hide_sheet(&mut self, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        let Some(sheet) = self.presentation().workbook.as_ref().and_then(|w| w.sheets.iter().position(|s| s.slide == Some(slide))) else { return };
        let shape = self.document.ids().new_shape_id();
        self.run_sheet(vec![Command::Sheet(SheetCommand::HideSheet { slide, sheet, hidden: true, shape })], cx);
    }

    /// Show hidden sheet `sheet` again, and go to it.
    pub(crate) fn unhide_sheet(&mut self, sheet: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
        self.run_sheet(vec![Command::Sheet(SheetCommand::HideSheet { slide, sheet, hidden: false, shape })], cx);
        if self.presentation().slide(slide).is_some() {
            self.select_slide(slide, window, cx);
        }
    }

    /// Open the name field over a sheet's tab, its name selected.
    pub(crate) fn begin_sheet_rename(&mut self, slide: SlideId, window: &mut Window, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        if slide != self.current_slide {
            self.select_slide(slide, window, cx);
        }
        let Some(index) = self.presentation().slides.iter().position(|s| s.id == slide) else { return };
        let name = self.presentation().slides[index].name.clone().unwrap_or_else(|| format!("Sheet{}", index + 1));
        let Some(state) = self.sheet_state_mut() else { return };
        state.jump.renaming = Some(slide);
        let input = state.jump.tab_rename.clone();
        input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
            input.select_all(window, cx);
        });
        input.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    /// The name field's Enter (or leaving it): rename the sheet. A name
    /// Excel does not take leaves the field open, to be put right.
    pub(crate) fn confirm_sheet_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.sheet_state_mut() else { return };
        let Some(slide) = state.jump.renaming else { return };
        let name = state.jump.tab_rename.read(cx).value().trim().to_string();
        let old = self.presentation().slide(slide).and_then(|s| s.name.clone());
        if name.is_empty() || old.as_deref() == Some(name.as_str()) {
            return self.end_sheet_rename(window, cx);
        }
        let index = self.presentation().workbook.as_ref().and_then(|w| w.sheets.iter().position(|s| s.slide == Some(slide)));
        if !ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, index) {
            self.status = Some(format!("{}: {name}", crate::shell::i18n::text("_xlsx.this-name-cannot-be-used-as-a-sheet-name")).into());
            cx.notify();
            return;
        }
        self.run_sheet(vec![Command::Sheet(SheetCommand::RenameSheet { slide, name })], cx);
        self.end_sheet_rename(window, cx);
    }

    fn end_sheet_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut() {
            state.jump.renaming = None;
        }
        if self.status.as_ref().is_some_and(|s| s.starts_with(crate::shell::i18n::text("_xlsx.this-name-cannot-be-used-as-a-sheet-name").as_ref())) {
            self.status = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The number format with one decimal more or less.
    fn step_decimals(&mut self, more: bool, cx: &mut Context<Self>) {
        let code = self.current_style().number_format;
        let new = step_decimals(&code, more);
        if new != code {
            self.format_cells(StyleChange::NumberFormat(new), cx);
        }
    }
}

/// `code` with one decimal place more or less in each section.
fn step_decimals(code: &str, more: bool) -> String {
    if code.eq_ignore_ascii_case("General") {
        return if more { "0.0".into() } else { "0".into() };
    }
    code.split(';')
        .map(|section| {
            // The last run of digit placeholders.
            let Some(last) = section.rfind(['0', '#']) else { return section.to_string() };
            let (head, tail) = section.split_at(last + 1);
            match head.rfind('.') {
                Some(dot) if head[dot + 1..].chars().all(|c| c == '0' || c == '#') => {
                    if more {
                        format!("{head}0{tail}")
                    } else if head.len() - dot - 1 <= 1 {
                        format!("{}{tail}", &head[..dot])
                    } else {
                        format!("{}{tail}", &head[..head.len() - 1])
                    }
                }
                _ if more => format!("{head}.0{tail}"),
                _ => section.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

pub(crate) fn toolbar_right(editor: &SlideEditor) -> Vec<ToolItem> {
    let style = editor.current_style();
    let format = |change: StyleChange| move |this: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>| this.format_cells(change.clone(), cx);
    let color_menu = |fill: bool| -> Vec<MenuEntry> {
        let mut items: Vec<MenuEntry> = (if fill { FILLS } else { COLORS })
            .iter()
            .map(|&(name, c)| {
                let change = if fill { StyleChange::Fill(Some(Rgb(c))) } else { StyleChange::Color(Rgb(c)) };
                MenuItem::new(if fill { "xlsx.fill" } else { "xlsx.color" }, crate::shell::i18n::text(name)).on(format(change)).into()
            })
            .collect();
        // テーマの色: the workbook's ten theme colours, each with its five tints.
        let colors = editor.current_theme_palette_colors();
        let theme_items: Vec<MenuEntry> = (0..10)
            .map(|i| {
                let name = crate::shell::i18n::text(THEME_NAMES[i]);
                let items: Vec<MenuEntry> = THEME_TINTS[i]
                    .iter()
                    .map(|&tint| {
                        let theme = ibis_doc_model::ThemeColor { index: i as u8, tint };
                        let rgb = ibis_pptx::tinted(colors[i], tint);
                        let change = if fill { StyleChange::FillTheme(rgb, theme) } else { StyleChange::ColorTheme(rgb, theme) };
                        let label = match tint {
                            0 => name.to_string(),
                            t if t > 0 => crate::shell::i18n::format("xlsx.theme-lighter", &[("name", &name), ("n", &(t / 10_000))]),
                            t => crate::shell::i18n::format("xlsx.theme-darker", &[("name", &name), ("n", &(-t / 10_000))]),
                        };
                        MenuItem::new(if fill { "xlsx.theme-fill" } else { "xlsx.theme-color" }, label).on(format(change)).into()
                    })
                    .collect();
                Submenu::new(THEME_IDS[i], "palette", name, items).into()
            })
            .collect();
        items.insert(0, MenuEntry::Separator);
        items.splice(0..0, theme_items);
        if fill {
            items.insert(0, MenuItem::tr("xlsx.fill-none").on(format(StyleChange::Fill(None))).into());
        }
        items
    };
    let mut number_menu: Vec<MenuEntry> = NUMBER_FORMATS.iter().map(|&(name, code)| MenuItem::new("xlsx.number-format", crate::shell::i18n::text(name)).checked(style.number_format == code).on(format(StyleChange::NumberFormat(code.into()))).into()).collect();
    number_menu.push(MenuEntry::Separator);
    number_menu.push(MenuItem::tr("xlsx.number-format-custom").on(|this, window, cx| this.open_custom_format(window, cx)).into());
    let font = style.font.clone().unwrap_or_else(|| "游ゴシック".into()); // i18n-ok（既定のフォント名（スタイルに保存される値））
    let font_menu: Vec<MenuEntry> = FONTS.iter().map(|&f| MenuItem::new("xlsx.font", f).checked(font == f).on(format(StyleChange::Font(Some(f.into())))).into()).collect();
    let size = style.font_size;
    let halign = |id: &'static str, icon: &'static str, label: &'static str, a: CellAlign| -> MenuEntry { MenuItem::new(id, crate::shell::i18n::text(label)).icon(icon).checked(style.align == a).on(format(StyleChange::Align(a))).into() };
    let valign = |id: &'static str, icon: &'static str, label: &'static str, a: CellVerticalAlign| -> MenuEntry { MenuItem::new(id, crate::shell::i18n::text(label)).icon(icon).checked(style.vertical == a).on(format(StyleChange::Vertical(a))).into() };
    let border = |id: &'static str, icon: &'static str, label: &'static str, kind: &'static str| -> MenuEntry { MenuItem::new(id, label).icon(icon).on(move |this, _, cx| this.set_borders(kind, cx)).into() };
    let border_styles = BorderLine::ALL.into_iter().map(|line| {
        let label = match line {
            BorderLine::Thin => crate::shell::i18n::text("_xlsx.thin"), BorderLine::Hair => crate::shell::i18n::text("_xlsx.hair"), BorderLine::Medium => crate::shell::i18n::text("_xlsx.medium"), BorderLine::Thick => crate::shell::i18n::text("_shell.thick"),
            BorderLine::Double => crate::shell::i18n::text("_shell.double"), BorderLine::Dotted => crate::shell::i18n::text("_shell.dotted"), BorderLine::Dashed => crate::shell::i18n::text("_shell.dashed"),
            BorderLine::DashDot => crate::shell::i18n::text("docx.picture.border.dash.dashdot"), BorderLine::DashDotDot => crate::shell::i18n::text("_xlsx.dash-dot-dot"), BorderLine::MediumDashed => crate::shell::i18n::text("_xlsx.medium-dashed"),
            BorderLine::MediumDashDot => crate::shell::i18n::text("_xlsx.medium-dash-dot-2"), BorderLine::MediumDashDotDot => crate::shell::i18n::text("_xlsx.medium-dash-dot-dot-2"), BorderLine::SlantDashDot => crate::shell::i18n::text("_xlsx.slant-dash-dot-2"),
        };
        let icon: &'static str = match line {
            BorderLine::Thin => "ibis/border-thin", BorderLine::Hair => "ibis/border-hair", BorderLine::Medium => "ibis/border-medium", BorderLine::Thick => "ibis/border-thick",
            BorderLine::Double => "ibis/border-double", BorderLine::Dotted => "ibis/border-dotted", BorderLine::Dashed => "ibis/border-dashed",
            BorderLine::DashDot => "ibis/border-dashdot", BorderLine::DashDotDot => "ibis/border-dashdotdot", BorderLine::MediumDashed => "ibis/border-mediumdashed",
            BorderLine::MediumDashDot => "ibis/border-mediumdashdot", BorderLine::MediumDashDotDot => "ibis/border-mediumdashdotdot", BorderLine::SlantDashDot => "ibis/border-slantdashdot",
        };
        MenuItem::new(line.ooxml(), label).icon(icon).checked(editor.wb().border_line == line).on(move |this, _, cx| { this.wb_mut().border_line = line; cx.notify(); }).into()
    }).collect();
    let border_colors = COLORS.iter().map(|&(label, color)| MenuItem::new(label, crate::shell::i18n::text(label)).checked(editor.wb().border_color == Rgb(color)).on(move |this, _, cx| { this.wb_mut().border_color = Rgb(color); cx.notify(); }).into()).collect();
    let halign_icon = match style.align {
        CellAlign::Center | CellAlign::CenterAcross => "format_align_center",
        CellAlign::Right => "format_align_right",
        CellAlign::Distributed => "format_align_justify",
        _ => "format_align_left",
    };
    vec![
        ToolButton::text("currency", "¥", "_shell.accounting-number-format").click(format(StyleChange::NumberFormat("\"¥\"#,##0;\"¥\"\\-#,##0".into()))).into(),
        ToolButton::new("percent", "percent", "_shell.percent-style").click(format(StyleChange::NumberFormat("0%".into()))).into(),
        ToolButton::new("decimal-less", "decimal_decrease", "_shell.decrease-decimal").click(|this, _, cx| this.step_decimals(false, cx)).into(),
        ToolButton::new("decimal-more", "decimal_increase", "_shell.increase-decimal").click(|this, _, cx| this.step_decimals(true, cx)).into(),
        ToolButton::text("number-format", "123", "_shell.more-number-formats").menu(number_menu).into(),
        sep(),
        ToolItem::Select { id: "font", value: font.into(), width: 128.0, tooltip: crate::shell::i18n::text("_shell.font").into(), menu: font_menu, disabled: false },
        sep(),
        ToolItem::FontSize {
            value: Some(format!("{}", size as f32 / 100.0).into()),
            smaller: Some(handler(move |this, _, cx| this.format_cells(StyleChange::FontSize(ibis_doc_model::step_font_size(size, false)), cx))),
            larger: Some(handler(move |this, _, cx| this.format_cells(StyleChange::FontSize(ibis_doc_model::step_font_size(size, true)), cx))),
        },
        sep(),
        ToolButton::new("bold", "format_bold", "_shell.bold-b").on(style.bold).click(format(StyleChange::Bold(!style.bold))).into(),
        ToolButton::new("italic", "format_italic", "_shell.italic-i").on(style.italic).click(format(StyleChange::Italic(!style.italic))).into(),
        ToolButton::new("underline", "format_underlined", "_shell.underline-u").on(style.underline.enabled()).click(format(StyleChange::Underline(!style.underline.enabled()))).into(),
        ToolButton::new("underline-kind", "arrow_drop_down", "_xlsx.underline-style").menu(ibis_doc_model::CellUnderline::ALL.into_iter().map(|u| {
            let label = match u { ibis_doc_model::CellUnderline::None => crate::shell::i18n::text("_xlsx.no-underline"), ibis_doc_model::CellUnderline::Single => crate::shell::i18n::text("_xlsx.single-underline"), ibis_doc_model::CellUnderline::Double => crate::shell::i18n::text("_xlsx.double-underline"), ibis_doc_model::CellUnderline::SingleAccounting => crate::shell::i18n::text("_xlsx.single-accounting"), ibis_doc_model::CellUnderline::DoubleAccounting => crate::shell::i18n::text("_xlsx.double-accounting") };
            MenuItem::new(u.ooxml(), label).checked(style.underline == u).on(format(StyleChange::UnderlineKind(u))).into()
        }).collect()).into(),
        ToolButton::new("text-color", "format_color_text", "_shell.font-color").color(rgb(style.color.0).into()).menu(color_menu(false)).into(),
        sep(),
        ToolButton::new("fill", "format_color_fill", "_shell.fill-color").color(rgb(style.fill.map_or(0xffffff, |c| c.0)).into()).menu(color_menu(true)).into(),
        ToolButton::new("borders", "border_all", "_shell.line")
            .menu(vec![
                border("xlsx.border-all", "border_all", "_xlsx.all-borders", "all"),
                border("xlsx.border-outside", "border_outer", "_xlsx.outside-borders", "outside"),
                border("xlsx.border-bottom", "border_bottom", "_xlsx.bottom-border", "bottom"),
                border("xlsx.border-top", "border_top", "_xlsx.top-border", "top"),
                border("xlsx.border-left", "border_left", "_xlsx.left-border", "left"),
                border("xlsx.border-right", "border_right", "_xlsx.right-border", "right"),
                border("xlsx.border-diagonal-up", "border_all", "_xlsx.diagonal-up-border", "diagonal-up"),
                border("xlsx.border-diagonal-down", "border_all", "_xlsx.diagonal-down-border", "diagonal-down"),
                border("xlsx.border-thick-bottom", "border_bottom", "_xlsx.thick-bottom-border", "thick-bottom"),
                border("xlsx.border-double-bottom", "border_bottom", "_xlsx.double-bottom-border", "double-bottom"),
                border("xlsx.border-none", "border_clear", "_xlsx.no-border", "none"),
                MenuEntry::Separator,
                Submenu::tr("xlsx.border-styles", "border_all", border_styles).into(),
                Submenu::tr("xlsx.border-colors", "palette", border_colors).into(),
            ])
            .into(),
        ToolButton::new("merge", "cell_merge", "_shell.merge-cells")
            .menu(vec![
                MenuItem::tr("xlsx.merge-center").icon("ibis/merge-center").on(|this, _, cx| this.merge_cells(true, cx)).into(),
                MenuItem::tr("xlsx.merge-across").icon("ibis/merge-across").on(|this, _, cx| this.merge_across(cx)).into(),
                MenuItem::tr("xlsx.merge").icon("cell_merge").on(|this, _, cx| this.merge_cells(false, cx)).into(),
                MenuItem::tr("xlsx.unmerge").icon("ibis/unmerge").on(|this, _, cx| this.unmerge_cells(cx)).into(),
            ])
            .into(),
        sep(),
        ToolButton::new("halign", halign_icon, "_shell.horizontal-alignment")
            .menu(vec![
                halign("xlsx.align-general", "format_align_left", "_xlsx.general", CellAlign::General),
                halign("xlsx.align-left", "format_align_left", "_shell.left", CellAlign::Left),
                halign("xlsx.align-center", "format_align_center", "_shell.center", CellAlign::Center),
                halign("xlsx.align-right", "format_align_right", "_shell.right", CellAlign::Right),
                halign("xlsx.align-center-across", "format_align_center", "_xlsx.center-across-selection", CellAlign::CenterAcross),
                halign("xlsx.align-distributed", "format_align_justify", "_shell.distributed", CellAlign::Distributed),
                halign("xlsx.align-fill", "format_align_left", "_xlsx.fill", CellAlign::Fill),
                halign("xlsx.align-justify", "format_align_justify", "align.justify", CellAlign::Justify),
                MenuEntry::Separator,
                MenuItem::tr("xlsx.shrink").checked(style.shrink).on(move |this, _, cx| this.format_cells(StyleChange::Shrink(!style.shrink), cx)).into(),
            ])
            .into(),
        ToolButton::new("valign", "vertical_align_bottom", "_shell.vertical-alignment")
            .menu(vec![
                valign("xlsx.valign-top", "vertical_align_top", "_shell.top", CellVerticalAlign::Top),
                valign("xlsx.valign-center", "vertical_align_center", "_shell.center", CellVerticalAlign::Center),
                valign("xlsx.valign-bottom", "vertical_align_bottom", "_shell.bottom", CellVerticalAlign::Bottom),
                valign("xlsx.valign-justify", "vertical_align_center", "align.justify", CellVerticalAlign::Justify),
                valign("xlsx.valign-distributed", "vertical_align_center", "_shell.distributed", CellVerticalAlign::Distributed),
            ])
            .into(),
        ToolButton::new("wrap", "wrap_text", "_shell.wrap-text").on(style.wrap).click(format(StyleChange::Wrap(!style.wrap))).into(),
        ToolButton::new("indent-less", "format_indent_decrease", "_xlsx.decrease-indent").click(format(StyleChange::Indent(style.indent.saturating_sub(1)))).into(),
        ToolButton::new("indent-more", "format_indent_increase", "_xlsx.increase-indent").click(format(StyleChange::Indent(style.indent + 1))).into(),
        sep(),
        ToolButton::new("functions", "functions", "_shell.functions")
            .menu(super::autosum::FUNCTIONS.iter().map(|&(f, label)| MenuItem::new("xlsx.function", format!("{f}（{}）", crate::shell::i18n::text(label))).on(move |this, _, cx| this.auto_function(f, cx)).into()).collect()) // i18n-ok（関数名の括弧）
            .into(),
    ]
}

/// Put `new` after the item `id` (in whatever menu or submenu it is).
fn insert_after(entries: &mut Vec<MenuEntry>, id: &str, new: MenuEntry) -> Result<(), MenuEntry> {
    if let Some(i) = entries.iter().position(|e| matches!(e, MenuEntry::Item(item) if item.id == id)) {
        entries.insert(i + 1, new);
        return Ok(());
    }
    let mut new = new;
    for e in entries.iter_mut() {
        if let MenuEntry::Submenu(sub) = e {
            match insert_after(&mut sub.items, id, new) {
                Ok(()) => return Ok(()),
                Err(back) => new = back,
            }
        }
    }
    Err(new)
}

/// ワードアート・文字変形のプリセット (ECMA-376 a:prstTxWarp)
pub(crate) const XLSX_TEXT_WARP_PRESETS: &[(&str, &str, &str)] = &[
    ("textArchUp", "xlsx.text.warp.arch-up", "上アーチ"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textArchDown", "xlsx.text.warp.arch-down", "下アーチ"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textCircle", "xlsx.text.warp.circle", "pptx.common.circle"),
    ("textButton", "xlsx.text.warp.button", "ボタン"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textCurveUp", "xlsx.text.warp.curve-up", "上カーブ"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textCurveDown", "xlsx.text.warp.curve-down", "下カーブ"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textWave1", "xlsx.text.warp.wave1", "波 1"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textWave2", "xlsx.text.warp.wave2", "波 2"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textDoubleWave1", "xlsx.text.warp.double-wave1", "二重波 1"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textInflate", "xlsx.text.warp.inflate", "膨張"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textDeflate", "xlsx.text.warp.deflate", "収縮"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textStop", "xlsx.text.warp.stop", "停止標識"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textTriangle", "xlsx.text.warp.triangle", "三角形"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textTriangleInverted", "xlsx.text.warp.triangle-inverted", "逆三角形"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textChevron", "xlsx.text.warp.chevron", "山形"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textChevronInverted", "xlsx.text.warp.chevron-inverted", "逆山形"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textFadeRight", "xlsx.text.warp.fade-right", "右フェード"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textFadeLeft", "xlsx.text.warp.fade-left", "左フェード"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textFadeUp", "xlsx.text.warp.fade-up", "上フェード"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textFadeDown", "xlsx.text.warp.fade-down", "下フェード"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
];

const XLSX_WORDART_STYLES: &[(&'static str, &'static str, &'static str)] = &[
    ("xlsx.wordart.style1", "xlsx.insert.wordart.style1", "xlsx.text.wordart.style1"),
    ("xlsx.wordart.style2", "xlsx.insert.wordart.style2", "xlsx.text.wordart.style2"),
    ("xlsx.wordart.style3", "xlsx.insert.wordart.style3", "xlsx.text.wordart.style3"),
    ("xlsx.wordart.style4", "xlsx.insert.wordart.style4", "xlsx.text.wordart.style4"),
    ("xlsx.wordart.style5", "xlsx.insert.wordart.style5", "xlsx.text.wordart.style5"),
    ("xlsx.wordart.style6", "xlsx.insert.wordart.style6", "xlsx.text.wordart.style6"),
];

fn wordart_insert_items() -> Vec<MenuEntry> {
    XLSX_WORDART_STYLES.iter().enumerate().map(|(idx, &(name_id, insert_id, _))| {
        MenuItem::new(insert_id, crate::shell::i18n::text(name_id)).on(move |e, _, cx| {
            e.insert_sheet_wordart(idx, None, cx);
        }).into()
    }).collect()
}

fn wordart_style_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let has_text = editor.edited_body().is_some() || editor.selected_sheet_shape().and_then(|id| editor.slide().shape(id)).is_some_and(|s| s.text_body().is_some() || matches!(s.kind, ibis_doc_model::ShapeKind::TextBox(_) | ibis_doc_model::ShapeKind::Auto(_)));
    XLSX_WORDART_STYLES.iter().enumerate().map(|(idx, &(name_id, _, apply_id))| {
        MenuItem::new(apply_id, crate::shell::i18n::text(name_id)).disabled(!has_text).on(move |e, w, cx| {
            e.set_selected_sheet_wordart_style(idx, w, cx);
        }).into()
    }).collect()
}

fn text_warp_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current_warp = editor.selected_sheet_text_warp();
    let has_text = editor.edited_body().is_some() || editor.selected_sheet_shape().and_then(|id| editor.slide().shape(id)).is_some_and(|s| s.text_body().is_some() || matches!(s.kind, ibis_doc_model::ShapeKind::TextBox(_) | ibis_doc_model::ShapeKind::Auto(_)));
    let mut items: Vec<MenuEntry> = Vec::new();
    items.push(MenuItem::tr("xlsx.text.warp.none").disabled(!has_text).checked(current_warp.is_none()).on(|e, _, cx| e.set_sheet_text_warp(None, cx)).into());
    items.push(MenuEntry::Separator);
    for &(prst, id, _name) in XLSX_TEXT_WARP_PRESETS {
        let is_checked = current_warp.as_deref() == Some(prst);
        items.push(MenuItem::tr(id).disabled(!has_text).checked(is_checked).on(move |e, _, cx| e.set_sheet_text_warp(Some(prst), cx)).into());
    }
    items
}

pub(crate) fn menus(editor: &SlideEditor, menus: &mut Menus) {
    // 編集 > 画像としてコピー (after コピー).
    let mut copy_as_pic: MenuEntry = MenuItem::tr("xlsx.copy-as-picture").icon("image").shortcut("⌥⇧⌘C").on(|this, _, cx| this.copy_as_picture(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "edit.copy", copy_as_pic) {
            Ok(()) => break,
            Err(back) => copy_as_pic = back,
        }
    }
    // シートを再表示: the hidden sheets, after シートを非表示.
    let hidden: Vec<(usize, String)> = editor
        .presentation()
        .workbook
        .as_ref()
        .map(|w| w.sheets.iter().enumerate().filter(|(_, s)| s.slide.is_none() && s.hidden.is_some() && !s.other).map(|(i, s)| (i, s.name.clone())).collect())
        .unwrap_or_default();
    let items: Vec<MenuEntry> = hidden.into_iter().map(|(i, name)| MenuItem::new("xlsx.unhide-sheet", name).on(move |this, window, cx| this.unhide_sheet(i, window, cx)).into()).collect();
    let mut entry: MenuEntry = if items.is_empty() {
        MenuItem::tr("xlsx.unhide-sheet").icon("visibility").disabled(true).into()
    } else {
        crate::shell::chrome::Submenu::tr("xlsx.unhide-sheets", "visibility", items).into()
    };
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.hide-sheet", entry) {
            Ok(()) => break,
            Err(back) => entry = back,
        }
    }
    menus.connect("xlsx.hide-sheet", |this, _, cx| this.hide_sheet(cx));
    menus.connect("xlsx.duplicate-sheet", |this, window, cx| this.copy_sheet(window, cx));
    // 見出しの色, after シートの名前を変更.
    let mut colors: Vec<MenuEntry> = vec![MenuItem::tr("xlsx.tab-color-none").on(|this, _, cx| this.set_tab_color(None, cx)).into()];
    colors.extend(COLORS.iter().map(|&(name, c)| -> MenuEntry { MenuItem::new("xlsx.tab-color", crate::shell::i18n::text(name)).on(move |this, _, cx| this.set_tab_color(Some(Rgb(c)), cx)).into() }));
    let mut tab: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.tab-colors", "palette", colors).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.rename-sheet", tab) {
            Ok(()) => break,
            Err(back) => tab = back,
        }
    }
    for (after, entry) in [
        ("xlsx.duplicate-sheet", MenuItem::tr("xlsx.move-sheet-left").on(|this, _, cx| this.move_sheet(-1, cx))),
        ("xlsx.move-sheet-left", MenuItem::tr("xlsx.move-sheet-right").on(|this, _, cx| this.move_sheet(1, cx))),
    ] {
        let mut entry: MenuEntry = entry.into();
        for (_, entries) in menus.menus.iter_mut() {
            match insert_after(entries, after, entry) {
                Ok(()) => break,
                Err(back) => entry = back,
            }
        }
    }
    menus.connect("xlsx.freeze", |this, _, cx| this.toggle_freeze(cx));
    // 表示 > 分割 (after ウィンドウ枠の固定).
    let mut split: MenuEntry = MenuItem::tr("xlsx.split").checked(editor.split_panes().is_some()).on(|this, _, cx| this.toggle_split(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.freeze", split) {
            Ok(()) => break,
            Err(back) => split = back,
        }
    }
    menus.connect("xlsx.sort", |this, _, cx| this.sort_by_selected_column(true, cx));
    let mut sort_manage: MenuEntry = MenuItem::tr("xlsx.sort-manage").icon("sort").on(|this, window, cx| this.open_sort_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.sort", sort_manage) {
            Ok(()) => break,
            Err(back) => sort_manage = back,
        }
    }
    menus.connect("xlsx.filter", |this, _, cx| this.toggle_auto_filter(cx));
    let mut adv_filter: MenuEntry = MenuItem::tr("xlsx.advanced-filter").icon("filter_alt").on(|this, window, cx| this.open_advanced_filter_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.filter", adv_filter) {
            Ok(()) => break,
            Err(back) => adv_filter = back,
        }
    }
    menus.connect("xlsx.advanced-filter", |this, window, cx| this.open_advanced_filter_panel(window, cx));
    // データ > 重複の削除 (after the sorting).
    let mut dedupe: MenuEntry = MenuItem::tr("xlsx.remove-duplicates").icon("sort").on(|this, window, cx| this.open_dedupe_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.sort-manage", dedupe) {
            Ok(()) => break,
            Err(back) => dedupe = back,
        }
    }
    // 編集 > 条件を選択してジャンプ (after 検索と置換).
    let mut goto_special: MenuEntry = MenuItem::tr("xlsx.goto-special").icon("find_replace").on(|this, _, cx| this.open_goto_special_panel(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "edit.find-replace", goto_special) {
            Ok(()) => break,
            Err(back) => goto_special = back,
        }
    }
    // 挿入 > 新しいメモ (編集 for a cell with one), after the link; メモの削除.
    let has_note = editor.current_cell().zip(editor.current_sheet()).is_some_and(|((c, r), (_, sheet))| sheet.comment_at(r, c).is_some());
    let mut note: MenuEntry = MenuItem::tr(if has_note { "xlsx.note-edit" } else { "xlsx.note-new" }).icon("note_add").on(|this, window, cx| this.open_note_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "insert.link", note) {
            Ok(()) => break,
            Err(back) => note = back,
        }
    }
    let mut note_delete: MenuEntry = MenuItem::tr("xlsx.note-delete").icon("delete").disabled(!has_note).on(|this, _, cx| this.delete_notes(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, if has_note { "xlsx.note-edit" } else { "xlsx.note-new" }, note_delete) {
            Ok(()) => break,
            Err(back) => note_delete = back,
        }
    }
    let has_thread = editor.current_cell().zip(editor.current_sheet()).is_some_and(|((c, r), (_, sheet))| sheet.thread_at(r, c).is_some());
    let mut comment_delete: MenuEntry = MenuItem::tr("xlsx.comment-delete").icon("delete").disabled(!has_thread).on(|this, _, cx| {
        if let Some((c, r)) = this.current_cell() {
            this.delete_threaded_comment(r, c, cx);
        }
    }).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.note-delete", comment_delete) {
            Ok(()) => break,
            Err(back) => comment_delete = back,
        }
    }
    let has_any_comments = editor.current_sheet().is_some_and(|(_, s)| !s.comments.is_empty() || !s.threads.is_empty());
    let mut clear_comments: MenuEntry = MenuItem::tr("xlsx.clear-comments").icon("clear_all").disabled(!has_any_comments).on(|this, _, cx| this.delete_threaded_comments_and_notes(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.comment-delete", clear_comments) {
            Ok(()) => break,
            Err(back) => clear_comments = back,
        }
    }
    // ツール > シートの保護… / シート保護の解除…, ブックの保護… / ブック保護の解除…, and 表示形式 > セルのロック・数式を表示しない.
    let protected = editor.sheet_protection().is_some();
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr(if protected { "xlsx.unprotect-sheet" } else { "xlsx.protect-sheet" }).icon("lock").on(|this, window, cx| this.open_protect_panel(window, cx)).into());
    let wb_protected = editor.workbook_protection().is_some();
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr(if wb_protected { "xlsx.unprotect-workbook" } else { "xlsx.protect-workbook" }).icon("lock").on(|this, window, cx| this.open_protect_workbook_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.allow-edit-ranges").icon("lock").on(|this, window, cx| this.open_allow_edit_ranges_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.accessibility-check").icon("accessibility").on(|this, window, cx| this.open_xlsx_accessibility_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.compatibility-check").icon("rule").on(|this, window, cx| this.open_xlsx_compatibility_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.autorecover-settings").icon("settings_backup_restore").on(|this, window, cx| this.open_xlsx_autorecover_settings_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.recover-unsaved").icon("restore_page").on(|this, window, cx| this.open_xlsx_recover_unsaved_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.backup-save-settings").icon("save_as").on(|this, window, cx| this.open_xlsx_backup_save_settings_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.open-and-repair").icon("cleaning_services").on(|this, window, cx| this.open_xlsx_repair_panel(window, cx)).into());
    let style = editor.current_style();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.lock-cells").checked(!style.unlocked).on(|this, _, cx| this.toggle_cell_protection(false, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.hide-formulas").checked(style.formula_hidden).on(|this, _, cx| this.toggle_cell_protection(true, cx)).into());
    let show_phonetic = editor
        .presentation()
        .slide(editor.current_slide)
        .and_then(ibis_doc_model::sheet_calc::sheet_of_slide)
        .map(|s| s.show_phonetic)
        .unwrap_or(false);
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.phonetic-toggle")
            .checked(show_phonetic)
            .on(|this, _, cx| this.toggle_show_phonetic(cx))
            .into(),
    );
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.phonetic-edit")
            .shortcut("⌥⇧↑")
            .on(|this, window, cx| this.open_phonetic_panel(window, cx))
            .into(),
    );
    // データ > フラッシュ フィル (⌃E), after 区切り位置.
    let mut flash: MenuEntry = MenuItem::tr("xlsx.flash-fill").icon("sort").shortcut("⌃E").on(|this, _, cx| this.flash_fill_column(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.remove-duplicates", flash) {
            Ok(()) => break,
            Err(back) => flash = back,
        }
    }
    let mut text_columns: MenuEntry = MenuItem::tr("xlsx.text-to-columns").icon("sort").on(|this, window, cx| this.open_text_columns_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.remove-duplicates", text_columns) {
            Ok(()) => break,
            Err(back) => text_columns = back,
        }
    }
    let mut import_text: MenuEntry = MenuItem::tr("xlsx.import-text").icon("sort").on(|this, window, cx| this.open_import_text_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.text-to-columns", import_text) {
            Ok(()) => break,
            Err(back) => import_text = back,
        }
    }
    let mut external_links: MenuEntry = MenuItem::tr("xlsx.external-links").icon("link").on(|this, window, cx| this.open_external_links_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.import-text", external_links) {
            Ok(()) => break,
            Err(back) => external_links = back,
        }
    }
    let mut consolidate: MenuEntry = MenuItem::tr("xlsx.consolidate").icon("table_chart").on(|this, window, cx| this.open_consolidate_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.text-to-columns", consolidate) {
            Ok(()) => break,
            Err(back) => consolidate = back,
        }
    }
    menus.connect("xlsx.consolidate", |this, window, cx| this.open_consolidate_panel(window, cx));
    let mut subtotal: MenuEntry = MenuItem::tr("xlsx.subtotal").icon("functions").on(|this, window, cx| this.open_subtotal_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.consolidate", subtotal) {
            Ok(()) => break,
            Err(back) => subtotal = back,
        }
    }
    menus.connect("xlsx.subtotal", |this, window, cx| this.open_subtotal_panel(window, cx));
    let mut sort_down: MenuEntry = MenuItem::tr("xlsx.sort-descending").icon("sort").on(|this, _, cx| this.sort_by_selected_column(false, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.sort", sort_down) {
            Ok(()) => break,
            Err(back) => sort_down = back,
        }
    }
    let grid = editor.current_sheet().is_some_and(|(_, s)| s.grid_lines);
    if let Some(item) = menus.find_mut("xlsx.gridlines") {
        item.checked = grid;
    }
    menus.connect("xlsx.gridlines", |this, _, cx| {
        let Some((_, sheet)) = this.current_sheet() else { return };
        let (slide, shown) = (this.current_slide, !sheet.grid_lines);
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetGridLines { slide, shown })], cx);
    });
    let headings = editor.current_sheet().map_or(true, |(_, s)| s.headings);
    if let Some(item) = menus.find_mut("xlsx.headings") {
        item.checked = headings;
    }
    menus.connect("xlsx.headings", |this, _, cx| {
        let Some((_, sheet)) = this.current_sheet() else { return };
        let (slide, shown) = (this.current_slide, !sheet.headings);
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetHeadings { slide, shown })], cx);
    });
    let formula_bar = editor.sheet_state().map_or(true, |s| s.show_formula_bar);
    if let Some(item) = menus.find_mut("xlsx.formula-bar") {
        item.checked = formula_bar;
    }
    menus.connect("xlsx.formula-bar", |this, _, cx| {
        if let Some(state) = this.sheet_state_mut() {
            state.show_formula_bar = !state.show_formula_bar;
            cx.notify();
        }
    });
    let preview = editor.current_sheet().is_some_and(|(_, s)| s.page_break_preview);
    if let Some(item) = menus.find_mut("xlsx.page-break-preview") {
        item.checked = preview;
    }
    menus.connect("xlsx.page-break-preview", |this, _, cx| {
        let Some((_, sheet)) = this.current_sheet() else { return };
        let (slide, shown) = (this.current_slide, !sheet.page_break_preview);
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetPageBreakPreview { slide, shown })], cx);
    });
    let layout = editor.current_sheet().is_some_and(|(_, s)| s.page_layout);
    if let Some(item) = menus.find_mut("xlsx.page-layout") {
        item.checked = layout;
    }
    menus.connect("xlsx.page-layout", |this, _, cx| {
        let Some((_, sheet)) = this.current_sheet() else { return };
        let (slide, shown) = (this.current_slide, !sheet.page_layout);
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetPageLayout { slide, shown })], cx);
    });
    menus.connect("xlsx.zoom-selection", |this, window, cx| {
        this.zoom_to_selection(window, cx);
    });
    let has_tables = editor.presentation().has_tables();
    if let Some(item) = menus.find_mut("xlsx.custom-views") {
        item.disabled = has_tables;
    }
    menus.connect("xlsx.custom-views", |this, window, cx| {
        this.toggle_custom_views_panel(window, cx);
    });
    menus.connect("xlsx.new-window", |this, _window, cx| {
        this.new_window(cx);
    });
    menus.connect("xlsx.arrange-all", |this, window, cx| {
        this.toggle_arrange_windows_panel(window, cx);
    });
    if let Some(item) = menus.find_mut("xlsx.view-side-by-side") {
        item.checked = editor.side_by_side;
    }
    menus.connect("xlsx.view-side-by-side", |this, window, cx| {
        this.toggle_view_side_by_side(window, cx);
    });
    if let Some(item) = menus.find_mut("xlsx.sync-scrolling") {
        item.checked = editor.sync_scrolling;
        item.disabled = !editor.side_by_side;
    }
    menus.connect("xlsx.sync-scrolling", |this, _window, cx| {
        this.toggle_sync_scrolling(cx);
    });
    if let Some(item) = menus.find_mut("xlsx.reset-window-position") {
        item.disabled = !editor.side_by_side;
    }
    menus.connect("xlsx.reset-window-position", |this, window, cx| {
        this.reset_side_by_side_positions(window, cx);
    });
    let is_hidden = editor.presentation().is_workbook_window_hidden();
    let is_locked = editor.presentation().is_workbook_windows_locked();
    if let Some(item) = menus.find_mut("xlsx.hide-window") {
        item.disabled = is_hidden || is_locked;
    }
    menus.connect("xlsx.hide-window", |this, _window, cx| {
        this.hide_workbook_window(cx);
    });
    if let Some(item) = menus.find_mut("xlsx.unhide-window") {
        item.disabled = !is_hidden || is_locked;
    }
    menus.connect("xlsx.unhide-window", |this, window, cx| {
        this.toggle_unhide_window_panel(window, cx);
    });
    // ファイル > ダウンロード > CSV: the sheet shown, as CSV.
    let mut csv: MenuEntry = MenuItem::tr("xlsx.export-csv").icon("description").on(|this, window, cx| this.export_csv(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "file.export.svg", csv) {
            Ok(()) => break,
            Err(back) => csv = back,
        }
    }
    // ファイル > 書き出し > CSV (Shift_JIS)
    let mut csv_sjis: MenuEntry = MenuItem::tr("xlsx.export-csv-sjis").icon("description").on(|this, window, cx| this.export_csv_sjis(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.export-csv", csv_sjis) {
            Ok(()) => break,
            Err(back) => csv_sjis = back,
        }
    }
    menus.connect("xlsx.export-csv-sjis", |this, window, cx| this.export_csv_sjis(window, cx));

    // ファイル > 書き出し > テキスト (タブ区切り)（.tsv）
    let mut tsv: MenuEntry = MenuItem::tr("xlsx.export-tsv").icon("description").on(|this, window, cx| this.export_tsv(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.export-csv-sjis", tsv) {
            Ok(()) => break,
            Err(back) => tsv = back,
        }
    }
    menus.connect("xlsx.export-tsv", |this, window, cx| this.export_tsv(window, cx));

    // ファイル > 書き出し > Excel テンプレート（.xltx）
    let mut xltx: MenuEntry = MenuItem::tr("xlsx.export-xltx").icon("description").on(|this, window, cx| this.export_xltx(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.export-tsv", xltx) {
            Ok(()) => break,
            Err(back) => xltx = back,
        }
    }
    menus.connect("xlsx.export-xltx", |this, window, cx| this.export_xltx(window, cx));

    // ファイル > 新規 > テンプレートから新規作成…
    let mut tmpl_entry: MenuEntry = MenuItem::tr("xlsx.new-from-template")
        .icon("dashboard_customize")
        .on(|this, window, cx| this.new_from_template(window, cx))
        .into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "file.new.workbook", tmpl_entry) {
            Ok(()) => break,
            Err(back) => tmpl_entry = back,
        }
    }
    menus.connect("xlsx.new-from-template", |this, window, cx| this.new_from_template(window, cx));

    // ファイル > 未保存のブックの回復…
    let mut recover_entry: MenuEntry = MenuItem::tr("xlsx.recover-unsaved")
        .icon("restore_page")
        .on(|this, window, cx| this.open_xlsx_recover_unsaved_panel(window, cx))
        .into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "file.open", recover_entry) {
            Ok(()) => break,
            Err(back) => recover_entry = back,
        }
    }
    // ファイル > 開いて修復… (XLSX-FILE-020)
    let mut repair_file_entry: MenuEntry = MenuItem::tr("xlsx.open-and-repair")
        .icon("cleaning_services")
        .on(|this, window, cx| this.open_xlsx_repair_panel(window, cx))
        .into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.recover-unsaved", repair_file_entry) {
            Ok(()) => break,
            Err(back) => repair_file_entry = back,
        }
    }
    // セルの挿入・削除: down (up) from the menu item, and the other way after it.
    menus.connect("xlsx.insert-cells", |this, _, cx| this.shift_cells(true, true, cx));
    menus.connect("xlsx.delete-cells", |this, _, cx| this.shift_cells(true, false, cx));
    for (after, entry) in [
        ("xlsx.insert-cells", MenuItem::tr("xlsx.insert-cells-right").icon("add").on(|this, _, cx| this.shift_cells(false, true, cx))),
        ("xlsx.delete-cells", MenuItem::tr("xlsx.delete-cells-left").icon("delete").on(|this, _, cx| this.shift_cells(false, false, cx))),
    ] {
        let mut entry: MenuEntry = entry.into();
        for (_, entries) in menus.menus.iter_mut() {
            match insert_after(entries, after, entry) {
                Ok(()) => break,
                Err(back) => entry = back,
            }
        }
    }
    menus.connect("edit.find-replace", |this, window, cx| this.open_sheet_find(window, cx));
    // データ > グループ化 and the rest of the outline.
    let data = menus.items_mut(crate::shell::chrome::MenuId::Data);
    data.push(MenuEntry::Separator);
    data.push(MenuItem::tr("xlsx.group").on(|this, _, cx| this.group_lines(true, cx)).into());
    data.push(MenuItem::tr("xlsx.ungroup").on(|this, _, cx| this.group_lines(false, cx)).into());
    data.push(MenuItem::tr("xlsx.collapse").on(|this, _, cx| this.collapse_group(true, cx)).into());
    data.push(MenuItem::tr("xlsx.expand").on(|this, _, cx| this.collapse_group(false, cx)).into());
    data.push(MenuEntry::Separator);
    data.extend(super::pivot::menu_items());
    data.push(MenuEntry::Separator);
    data.push(MenuItem::tr("xlsx.goal-seek").on(|this, window, cx| this.open_goal_seek(window, cx)).into());
    data.push(MenuItem::tr("xlsx.data-table").on(|this, window, cx| this.open_data_table(window, cx)).into());
    data.push(MenuItem::tr("xlsx.scenario-manager").on(|this, window, cx| this.open_scenario_manager(window, cx)).into());
    let selected_version = editor.presentation().workbook.as_deref().and_then(|w| w.compatibility_version).unwrap_or(1);
    let versions = ibis_doc_model::CalculationCompatibilityVersion::ALL.into_iter().map(|version| {
        let (id, label) = match version {
            ibis_doc_model::CalculationCompatibilityVersion::V1 => ("xlsx.compatibility-v1", crate::shell::i18n::text("_xlsx.version-1")),
            ibis_doc_model::CalculationCompatibilityVersion::V2 => ("xlsx.compatibility-v2", crate::shell::i18n::text("_xlsx.version-2")),
        };
        MenuItem::new(id, label).checked(selected_version == version.number()).disabled(selected_version == version.number()).on(move |this, _, cx| {
            this.run_sheet(vec![Command::Sheet(SheetCommand::SetCompatibilityVersion { version })], cx);
        }).into()
    }).collect();
    data.push(crate::shell::chrome::Submenu::tr("xlsx.compatibility-version", "functions", versions).into());
    let calc_mode = editor.presentation().workbook.as_deref().map(|w| w.calc_mode).unwrap_or_default();
    let modes = [
        (ibis_doc_model::CalcMode::Auto, "xlsx.calc-auto"),
        (ibis_doc_model::CalcMode::AutoNoTable, "xlsx.calc-auto-no-table"),
        (ibis_doc_model::CalcMode::Manual, "xlsx.calc-manual"),
    ].into_iter().map(|(mode, key)| {
        MenuItem::tr(key).checked(calc_mode == mode).disabled(calc_mode == mode).on(move |this, _, cx| {
            this.run_sheet(vec![Command::Sheet(SheetCommand::SetCalcMode { mode })], cx);
        }).into()
    }).collect();
    data.push(crate::shell::chrome::Submenu::tr("xlsx.calc-options", "tune", modes).into());
    data.push(MenuItem::tr("xlsx.calculate-now").shortcut("F9").on(|this, _, cx| {
        this.run_sheet(vec![Command::Sheet(SheetCommand::CalculateNow)], cx);
    }).into());
    data.push(MenuItem::tr("xlsx.calculate-sheet").shortcut("⇧F9").on(|this, _, cx| {
        let slide = this.current_slide;
        this.run_sheet(vec![Command::Sheet(SheetCommand::CalculateSheet { slide })], cx);
    }).into());
    let full_precision = editor.presentation().workbook.as_deref().map(|w| w.full_precision).unwrap_or(true);
    let precision_as_displayed = !full_precision;
    data.push(MenuItem::tr("xlsx.precision-as-displayed").checked(precision_as_displayed).on(move |this, _, cx| {
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetFullPrecision { full_precision: precision_as_displayed })], cx);
    }).into());
    let iterate = editor.presentation().workbook.as_deref().map(|w| w.iterate).unwrap_or(false);
    data.push(MenuItem::tr("xlsx.iterative-calculation").checked(iterate).on(move |this, _, cx| {
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetIterativeCalculation {
            iterate: !iterate,
            max_iterations: 100,
            max_change: 0.001.into(),
        })], cx);
    }).into());
    data.push(MenuItem::tr("xlsx.trace-precedents").icon("arrow_forward").on(|this, _, cx| this.trace_precedents(cx)).into());
    data.push(MenuItem::tr("xlsx.trace-dependents").icon("arrow_back").on(|this, _, cx| this.trace_dependents(cx)).into());
    let remove_arrow_items = vec![
        MenuItem::tr("xlsx.remove-all-arrows").on(|this, _, cx| this.remove_all_arrows(cx)).into(),
        MenuItem::tr("xlsx.remove-precedent-arrows").on(|this, _, cx| this.remove_precedent_arrows(cx)).into(),
        MenuItem::tr("xlsx.remove-dependent-arrows").on(|this, _, cx| this.remove_dependent_arrows(cx)).into(),
    ];
    data.push(crate::shell::chrome::Submenu::tr("xlsx.remove-arrows", "clear", remove_arrow_items).into());
    data.push(MenuItem::tr("xlsx.evaluate-formula").icon("functions").on(|this, window, cx| this.open_evaluate_formula(window, cx)).into());
    data.push(MenuItem::tr("xlsx.watch-window").icon("visibility").on(|this, window, cx| this.open_watch_window(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(MenuItem::tr("xlsx.pivot-insert").icon("table_chart").on(|this, window, cx| this.open_create_pivot(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(MenuItem::tr("xlsx.pivot-chart-insert").icon("insert_chart").on(|this, window, cx| this.open_insert_pivot_chart(window, cx)).into());
    let spark_style_items: Vec<MenuEntry> = [
        ("xlsx.sparkline-style-1", 0),
        ("xlsx.sparkline-style-2", 1),
        ("xlsx.sparkline-style-3", 2),
        ("xlsx.sparkline-style-4", 3),
        ("xlsx.sparkline-style-5", 4),
        ("xlsx.sparkline-style-6", 5),
    ]
    .into_iter()
    .map(|(id, idx)| {
        MenuItem::tr(id)
            .on(move |this, _, cx| {
                this.set_selected_sparklines_style(idx, cx);
            })
            .into()
    })
    .collect();
    let spark_color_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.sparkline-color-blue").on(|this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x376092)), cx)).into(),
        MenuItem::tr("xlsx.sparkline-color-red").on(|this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0xC00000)), cx)).into(),
        MenuItem::tr("xlsx.sparkline-color-green").on(|this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x70AD47)), cx)).into(),
        MenuItem::tr("xlsx.sparkline-color-purple").on(|this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x7030A0)), cx)).into(),
        MenuItem::tr("xlsx.sparkline-color-cyan").on(|this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x4BACC6)), cx)).into(),
        MenuItem::tr("xlsx.sparkline-color-orange").on(|this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0xED7D31)), cx)).into(),
        MenuItem::tr("xlsx.sparkline-color-gray").on(|this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x7F7F7F)), cx)).into(),
        MenuItem::tr("xlsx.sparkline-color-black").on(|this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x000000)), cx)).into(),
    ];
    let spark_hidden_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.sparkline-display-hidden-on").on(|this, _, cx| this.set_selected_sparklines_display_hidden(true, cx)).into(),
        MenuItem::tr("xlsx.sparkline-display-hidden-off").on(|this, _, cx| this.set_selected_sparklines_display_hidden(false, cx)).into(),
    ];
    let spark_axis_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.sparkline-date-axis-on").on(|this, _, cx| this.set_selected_sparklines_date_axis(true, cx)).into(),
        MenuItem::tr("xlsx.sparkline-date-axis-off").on(|this, _, cx| this.set_selected_sparklines_date_axis(false, cx)).into(),
    ];
    let spark_items = vec![
        MenuItem::tr("xlsx.sparkline-line").on(|this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::Line), window, cx)).into(),
        MenuItem::tr("xlsx.sparkline-column").on(|this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::Column), window, cx)).into(),
        MenuItem::tr("xlsx.sparkline-winloss").on(|this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::WinLoss), window, cx)).into(),
        MenuEntry::Separator,
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-styles", "palette", spark_style_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-colors", "format_color_text", spark_color_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-display-hidden", "visibility", spark_hidden_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-date-axis-menu", "timeline", spark_axis_items).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(crate::shell::chrome::Submenu::tr("xlsx.sparklines", "show_chart", spark_items).into());
    let shape_entries: Vec<MenuEntry> = super::shape_edit::SHEET_SHAPES
        .iter()
        .map(|&(preset, id, _label, icon)| {
            MenuItem::tr(id)
                .icon(icon)
                .on(move |this, _, cx| {
                    this.insert_sheet_shape(preset, cx);
                })
                .into()
        })
        .chain(std::iter::once(MenuEntry::Separator))
        .chain([
            MenuItem::tr("xlsx.shape.line")
                .icon("horizontal_rule")
                .on(|this, _, cx| {
                    this.insert_sheet_line(false, cx);
                })
                .into(),
            MenuItem::tr("xlsx.shape.line-arrow")
                .icon("arrow_right_alt")
                .on(|this, _, cx| {
                    this.insert_sheet_line(true, cx);
                })
                .into(),
        ])
        .collect();

    let insert = menus.items_mut(crate::shell::chrome::MenuId::Insert);
    insert.push(MenuItem::tr("xlsx.insert-textbox").icon("text_fields").on(|this, window, cx| { this.insert_sheet_textbox(window, cx); }).into());
    insert.push(crate::shell::chrome::Submenu::tr("xlsx.insert-shape", "shapes", shape_entries.clone()).into());
    insert.push(MenuItem::tr("xlsx.insert-picture").icon("image").on(|this, window, cx| { this.pick_sheet_picture(window, cx); }).into());
    insert.push(MenuItem::tr("xlsx.insert.wordart").icon("title").on(|this, _, cx| { this.insert_sheet_wordart(0, None, cx); }).into());
    insert.push(crate::shell::chrome::Submenu::tr("xlsx.insert.wordart-styles", "title", wordart_insert_items()).into());

    fill_submenu(menus, "insert.shape", shape_entries);
    fill_submenu(menus, "insert.chart", super::chart_insert::chart_items());
    fill_submenu(menus, "insert.image", vec![MenuItem::tr("xlsx.insert-picture").icon("upload").on(|this, window, cx| { this.pick_sheet_picture(window, cx); }).into()]);
    menus.connect("insert.symbol", |this, _, cx| this.open_symbols(cx));
    menus.connect("insert.link", |this, window, cx| this.open_link_panel(window, cx));
    menus.connect("insert.comment", |this, window, cx| this.open_threaded_comment_panel(window, cx));
    menus.connect("xlsx.comment-delete", |this, _, cx| {
        if let Some((c, r)) = this.current_cell() {
            this.delete_threaded_comment(r, c, cx);
        }
    });
    menus.connect("xlsx.clear-comments", |this, _, cx| this.delete_threaded_comments_and_notes(cx));
    menus.connect("file.page-setup", |this, _, cx| this.open_page_setup(cx));
    menus.connect("file.info", |this, window, cx| this.open_sheet_doc_properties_panel(window, cx));
    menus.connect("xlsx.properties", |this, window, cx| this.open_sheet_doc_properties_panel(window, cx));
    menus.connect("xlsx.named-ranges", |this, window, cx| {
        if let Some(s) = this.sheet_state_mut() {
            s.validation = None;
            s.find = None;
        }
        this.open_names(window, cx);
    });
    menus.connect("xlsx.validation", |this, window, cx| {
        if let Some(s) = this.sheet_state_mut() {
            s.names = None;
            s.find = None;
        }
        this.open_validation_panel(window, cx);
    });
    menus.connect("xlsx.validation.circle-invalid", |this, _, cx| this.circle_invalid_data(cx));
    menus.connect("xlsx.validation.clear-circles", |this, _, cx| this.clear_validation_circles(cx));
    menus.connect("xlsx.trace-precedents", |this, _, cx| this.trace_precedents(cx));
    menus.connect("xlsx.trace-dependents", |this, _, cx| this.trace_dependents(cx));
    menus.connect("xlsx.remove-arrows", |this, _, cx| this.remove_all_arrows(cx));
    menus.connect("xlsx.remove-all-arrows", |this, _, cx| this.remove_all_arrows(cx));
    menus.connect("xlsx.remove-precedent-arrows", |this, _, cx| this.remove_precedent_arrows(cx));
    menus.connect("xlsx.remove-dependent-arrows", |this, _, cx| this.remove_dependent_arrows(cx));
    menus.connect("xlsx.evaluate-formula", |this, window, cx| this.open_evaluate_formula(window, cx));
    menus.connect("xlsx.watch-window", |this, window, cx| this.open_watch_window(window, cx));
    menus.connect("xlsx.accessibility-check", |this, window, cx| this.open_xlsx_accessibility_panel(window, cx));
    menus.connect("xlsx.compatibility-check", |this, window, cx| this.open_xlsx_compatibility_panel(window, cx));
    menus.connect("xlsx.autorecover-settings", |this, window, cx| this.open_xlsx_autorecover_settings_panel(window, cx));
    menus.connect("xlsx.recover-unsaved", |this, window, cx| this.open_xlsx_recover_unsaved_panel(window, cx));
    menus.connect("xlsx.backup-save-settings", |this, window, cx| this.open_xlsx_backup_save_settings_panel(window, cx));
    menus.connect("xlsx.open-and-repair", |this, window, cx| this.open_xlsx_repair_panel(window, cx));
    menus.connect("xlsx.print-active-sheet", |this, _, cx| this.set_print_what(super::PrintWhat::ActiveSheet, cx));
    menus.connect("xlsx.print-entire-workbook", |this, _, cx| this.set_print_what(super::PrintWhat::EntireWorkbook, cx));
    menus.connect("xlsx.print-selection", |this, _, cx| this.set_print_what(super::PrintWhat::Selection, cx));
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.table-design").icon("table").on(|this, window, cx| this.open_table_design(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(super::conditional_quick::menu());
    menus.items_mut(crate::shell::chrome::MenuId::Format).extend(super::format_more::menus());
    // 表示形式 > セルのスタイル (Excel's ホーム > セルのスタイル).
    if let Some(styles) = editor.cell_style_menu() {
        menus.items_mut(crate::shell::chrome::MenuId::Format).push(styles);
    }
    // 表示形式 > テーマの配色 & テーマの色とフォント…
    let theme_palette_items: Vec<MenuEntry> = crate::pptx::theme_panel::THEME_PALETTES
        .iter()
        .map(|preset| {
            let pid = preset.id;
            let active = editor.is_xlsx_theme_palette_active(pid);
            let menu_id = match pid {
                "office" => "xlsx.theme-palette.office",
                "aspect" => "xlsx.theme-palette.aspect",
                "apex" => "xlsx.theme-palette.apex",
                "slate" => "xlsx.theme-palette.slate",
                "median" => "xlsx.theme-palette.median",
                "orient" => "xlsx.theme-palette.orient",
                "blue" => "xlsx.theme-palette.blue",
                _ => "xlsx.theme-palette",
            };
            MenuItem::new(menu_id, crate::shell::i18n::text(preset.label_key))
                .checked(active)
                .on(move |this, _, cx| this.apply_xlsx_theme_palette(pid, cx))
                .into()
        })
        .collect();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        Submenu::tr("xlsx.theme-palette", "palette", theme_palette_items).into(),
    );
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.theme-colors")
            .icon("tune")
            .on(|this, window, cx| this.open_xlsx_theme_panel(window, cx))
            .into(),
    );
    let has_chart = editor.sheet_chart().is_some();
    let has_data_table = editor.sheet_chart().is_some_and(|(_, c)| c.data_table.is_some());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.chart-data-table")
            .icon("table_chart")
            .disabled(!has_chart)
            .checked(has_data_table)
            .on(|this, _, cx| {
                let shown = this.sheet_chart().map(|(_, c)| c.data_table.is_some()).unwrap_or(false);
                this.set_sheet_chart_data_table_shown(!shown, cx);
            })
            .into(),
    );
    let has_trendline = editor.sheet_chart().is_some_and(|(_, c)| {
        c.groups.iter().flat_map(|g| &g.series).any(|s| s.trendline.is_some())
    });
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.chart-trendline")
            .icon("show_chart")
            .disabled(!has_chart)
            .checked(has_trendline)
            .on(|this, _, cx| {
                this.toggle_sheet_chart_trendline(cx);
            })
            .into(),
    );
    let has_error_bars = editor.sheet_chart().is_some_and(|(_, c)| {
        c.groups.iter().flat_map(|g| &g.series).any(|s| s.error_bars.is_some())
    });
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.chart-error-bars")
            .icon("stacked_bar_chart")
            .disabled(!has_chart)
            .checked(has_error_bars)
            .on(|this, _, cx| {
                this.toggle_sheet_chart_error_bars(cx);
            })
            .into(),
    );
    let style_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.chart-style-none").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_style(None, cx)).into(),
        MenuItem::tr("xlsx.chart-style-1").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_style(Some(1), cx)).into(),
        MenuItem::tr("xlsx.chart-style-2").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_style(Some(2), cx)).into(),
        MenuItem::tr("xlsx.chart-style-3").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_style(Some(3), cx)).into(),
        MenuItem::tr("xlsx.chart-style-4").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_style(Some(4), cx)).into(),
        MenuItem::tr("xlsx.chart-style-5").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_style(Some(5), cx)).into(),
        MenuItem::tr("xlsx.chart-style-6").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_style(Some(6), cx)).into(),
        MenuItem::tr("xlsx.chart-style-7").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_style(Some(7), cx)).into(),
        MenuItem::tr("xlsx.chart-style-8").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_style(Some(8), cx)).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.chart-styles", "palette", style_items).into(),
    );
    let palette_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.chart-palette-1").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_color_palette(0, cx)).into(),
        MenuItem::tr("xlsx.chart-palette-2").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_color_palette(1, cx)).into(),
        MenuItem::tr("xlsx.chart-palette-3").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_color_palette(2, cx)).into(),
        MenuItem::tr("xlsx.chart-palette-4").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_color_palette(3, cx)).into(),
        MenuItem::tr("xlsx.chart-palette-5").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_color_palette(4, cx)).into(),
        MenuItem::tr("xlsx.chart-palette-6").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_color_palette(5, cx)).into(),
        MenuItem::tr("xlsx.chart-palette-7").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_color_palette(6, cx)).into(),
        MenuItem::tr("xlsx.chart-palette-8").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_color_palette(7, cx)).into(),
        MenuItem::tr("xlsx.chart-palette-9").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_color_palette(8, cx)).into(),
        MenuItem::tr("xlsx.chart-palette-10").disabled(!has_chart).on(|this, _, cx| this.set_sheet_chart_color_palette(9, cx)).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.chart-palettes", "color_lens", palette_items).into(),
    );
    menus.connect("xlsx.theme-palette.office", |this, _, cx| this.apply_xlsx_theme_palette("office", cx));
    menus.connect("xlsx.theme-palette.aspect", |this, _, cx| this.apply_xlsx_theme_palette("aspect", cx));
    menus.connect("xlsx.theme-palette.apex", |this, _, cx| this.apply_xlsx_theme_palette("apex", cx));
    menus.connect("xlsx.theme-palette.slate", |this, _, cx| this.apply_xlsx_theme_palette("slate", cx));
    menus.connect("xlsx.theme-palette.median", |this, _, cx| this.apply_xlsx_theme_palette("median", cx));
    menus.connect("xlsx.theme-palette.orient", |this, _, cx| this.apply_xlsx_theme_palette("orient", cx));
    menus.connect("xlsx.theme-palette.blue", |this, _, cx| this.apply_xlsx_theme_palette("blue", cx));
    menus.connect("xlsx.theme-colors", |this, window, cx| this.open_xlsx_theme_panel(window, cx));
    menus.connect("xlsx.theme", |this, window, cx| this.open_xlsx_theme_panel(window, cx));
    menus.connect("xlsx.chart-data-table", |this, _, cx| {
        let shown = this.sheet_chart().map(|(_, c)| c.data_table.is_some()).unwrap_or(false);
        this.set_sheet_chart_data_table_shown(!shown, cx);
    });
    menus.connect("xlsx.chart-trendline", |this, _, cx| {
        this.toggle_sheet_chart_trendline(cx);
    });
    menus.connect("xlsx.chart-error-bars", |this, _, cx| {
        this.toggle_sheet_chart_error_bars(cx);
    });
    menus.connect("xlsx.chart-style-none", |this, _, cx| this.set_sheet_chart_style(None, cx));
    menus.connect("xlsx.chart-style-1", |this, _, cx| this.set_sheet_chart_style(Some(1), cx));
    menus.connect("xlsx.chart-style-2", |this, _, cx| this.set_sheet_chart_style(Some(2), cx));
    menus.connect("xlsx.chart-style-3", |this, _, cx| this.set_sheet_chart_style(Some(3), cx));
    menus.connect("xlsx.chart-style-4", |this, _, cx| this.set_sheet_chart_style(Some(4), cx));
    menus.connect("xlsx.chart-style-5", |this, _, cx| this.set_sheet_chart_style(Some(5), cx));
    menus.connect("xlsx.chart-style-6", |this, _, cx| this.set_sheet_chart_style(Some(6), cx));
    menus.connect("xlsx.chart-style-7", |this, _, cx| this.set_sheet_chart_style(Some(7), cx));
    menus.connect("xlsx.chart-style-8", |this, _, cx| this.set_sheet_chart_style(Some(8), cx));
    menus.connect("xlsx.chart-palette-1", |this, _, cx| this.set_sheet_chart_color_palette(0, cx));
    menus.connect("xlsx.chart-palette-2", |this, _, cx| this.set_sheet_chart_color_palette(1, cx));
    menus.connect("xlsx.chart-palette-3", |this, _, cx| this.set_sheet_chart_color_palette(2, cx));
    menus.connect("xlsx.chart-palette-4", |this, _, cx| this.set_sheet_chart_color_palette(3, cx));
    menus.connect("xlsx.chart-palette-5", |this, _, cx| this.set_sheet_chart_color_palette(4, cx));
    menus.connect("xlsx.chart-palette-6", |this, _, cx| this.set_sheet_chart_color_palette(5, cx));
    menus.connect("xlsx.chart-palette-7", |this, _, cx| this.set_sheet_chart_color_palette(6, cx));
    menus.connect("xlsx.chart-palette-8", |this, _, cx| this.set_sheet_chart_color_palette(7, cx));
    menus.connect("xlsx.chart-palette-9", |this, _, cx| this.set_sheet_chart_color_palette(8, cx));
    menus.connect("xlsx.chart-palette-10", |this, _, cx| this.set_sheet_chart_color_palette(9, cx));
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.alt-text")
            .icon("accessibility")
            .disabled(editor.selected_sheet_shape().is_none())
            .on(|this, window, cx| this.open_sheet_alt_text_panel(window, cx))
            .into(),
    );
    let has_shape = editor.selected_sheet_shape().is_some();
    let order_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.order-front")
            .icon("flip_to_front")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.order_sheet_shape(crate::pptx::editing::Order::Front, cx);
            })
            .into(),
        MenuItem::tr("xlsx.order-forward")
            .icon("arrow_upward")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.order_sheet_shape(crate::pptx::editing::Order::Forward, cx);
            })
            .into(),
        MenuItem::tr("xlsx.order-backward")
            .icon("arrow_downward")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.order_sheet_shape(crate::pptx::editing::Order::Backward, cx);
            })
            .into(),
        MenuItem::tr("xlsx.order-back")
            .icon("flip_to_back")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.order_sheet_shape(crate::pptx::editing::Order::Back, cx);
            })
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.order", "layers", order_items).into(),
    );
    let align_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.align.left")
            .icon("align_horizontal_left")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Left, cx);
            })
            .into(),
        MenuItem::tr("xlsx.align.center")
            .icon("align_horizontal_center")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Center, cx);
            })
            .into(),
        MenuItem::tr("xlsx.align.right")
            .icon("align_horizontal_right")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Right, cx);
            })
            .into(),
        MenuItem::tr("xlsx.align.top")
            .icon("align_vertical_top")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Top, cx);
            })
            .into(),
        MenuItem::tr("xlsx.align.middle")
            .icon("align_vertical_center")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Middle, cx);
            })
            .into(),
        MenuItem::tr("xlsx.align.bottom")
            .icon("align_vertical_bottom")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Bottom, cx);
            })
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.distribute.horizontal")
            .icon("horizontal_distribute")
            .disabled(editor.slide().shapes.len() < 4)
            .on(|this, _, cx| {
                this.distribute_sheet_shapes(true, cx);
            })
            .into(),
        MenuItem::tr("xlsx.distribute.vertical")
            .icon("vertical_distribute")
            .disabled(editor.slide().shapes.len() < 4)
            .on(|this, _, cx| {
                this.distribute_sheet_shapes(false, cx);
            })
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.align", "format_align_left", align_items).into(),
    );

    // 図形のグループ化 (xlsx.group-shapes)
    let can_grp = editor.can_group();
    let can_ungrp = editor.can_ungroup();
    let group_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.group-shapes.group")
            .icon("group_work")
            .disabled(!can_grp)
            .on(|this, _, cx| this.group_selected(cx))
            .into(),
        MenuItem::tr("xlsx.group-shapes.ungroup")
            .icon("ungroup")
            .disabled(!can_ungrp)
            .on(|this, _, cx| this.ungroup_selected(cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.group-shapes", "group_work", group_items).into(),
    );

    // 画像のトリミング (xlsx.crop)
    let is_picture = editor.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_)));
    let crop_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.crop.left")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop { left: 10_000, ..Default::default() }, cx))
            .into(),
        MenuItem::tr("xlsx.crop.right")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop { right: 10_000, ..Default::default() }, cx))
            .into(),
        MenuItem::tr("xlsx.crop.top")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop { top: 10_000, ..Default::default() }, cx))
            .into(),
        MenuItem::tr("xlsx.crop.bottom")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop { bottom: 10_000, ..Default::default() }, cx))
            .into(),
        MenuItem::tr("xlsx.crop.center")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 }, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.crop.reset")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop::default(), cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.crop", "crop", crop_items).into(),
    );

    // 画像の色 (xlsx.picture.color)
    let color_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.picture.color.none")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(None, cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.grayscale")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x000000), ibis_doc_model::Rgb(0xffffff))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.sepia")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x402000), ibis_doc_model::Rgb(0xffe0c0))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.blue")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.green")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x004010), ibis_doc_model::Rgb(0xe2efda))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.red")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x600000), ibis_doc_model::Rgb(0xfce4d6))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.orange")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x803000), ibis_doc_model::Rgb(0xffe6cc))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.purple")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x400060), ibis_doc_model::Rgb(0xf2e6ff))), cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.picture.color", "palette", color_items).into(),
    );

    // 画像の補正 (xlsx.picture.correction)
    let correction_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.picture.lum.normal")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(None, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.picture.lum.bright-p40")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((40_000, 0)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.bright-p20")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((20_000, 0)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.bright-m20")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((-20_000, 0)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.bright-m40")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((-40_000, 0)), cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.picture.lum.contrast-p40")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((0, 40_000)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.contrast-p20")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((0, 20_000)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.contrast-m20")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((0, -20_000)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.contrast-m40")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((0, -40_000)), cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.picture.lum.bright-contrast-high")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((20_000, 20_000)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.bright-contrast-low")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((-20_000, -20_000)), cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.picture.correction", "tune", correction_items).into(),
    );

    // 画像の角丸スタイル (xlsx.picture.corner)
    let corner_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.picture.corner.none")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures_corner(0, cx))
            .into(),
        MenuItem::tr("xlsx.picture.corner.small")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures_corner(5_000, cx))
            .into(),
        MenuItem::tr("xlsx.picture.corner.medium")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures_corner(10_000, cx))
            .into(),
        MenuItem::tr("xlsx.picture.corner.large")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures_corner(20_000, cx))
            .into(),
        MenuItem::tr("xlsx.picture.corner.round")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures_corner(50_000, cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.picture.corner", "rounded_corner", corner_items).into(),
    );

    // 画像の色と効果のリセット (xlsx.picture.reset)
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.picture.reset")
            .icon("restart_alt")
            .disabled(!is_picture)
            .on(|this, _, cx| this.reset_picture_effects(cx))
            .into(),
    );

    // 図形の変更 (xlsx.change-shape)
    let is_auto_or_textbox = editor.selected_sheet_shape().and_then(|id| editor.slide().shape(id)).is_some_and(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Auto(_) | ibis_doc_model::ShapeKind::TextBox(_)));
    let change_shape_items: Vec<MenuEntry> = super::shape_edit::CHANGE_SHAPES
        .iter()
        .map(|&(preset, item_id, _, icon)| {
            MenuItem::tr(item_id)
                .icon(icon)
                .disabled(!is_auto_or_textbox)
                .on(move |this, _, cx| this.change_sheet_shape(preset, cx))
                .into()
        })
        .collect();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.change-shape", "shapes", change_shape_items).into(),
    );

    // セルと連動 (xlsx.shape-textlink)
    let has_shape_formula = editor.selected_sheet_shape().and_then(|id| editor.slide().shape_formulas.get(&id)).is_some();
    let textlink_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.shape-textlink.current-cell")
            .icon("link")
            .disabled(!is_auto_or_textbox)
            .on(|this, _, cx| this.link_selected_shape_to_current_cell(cx))
            .into(),
        MenuItem::tr("xlsx.shape-textlink.clear")
            .icon("link_off")
            .disabled(!has_shape_formula)
            .on(|this, _, cx| this.clear_selected_shape_formula(cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.shape-textlink", "link", textlink_items).into(),
    );

    // 図形の塗りつぶし (xlsx.shape-fill)
    let fill_color_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.shape-fill.none")
            .icon("format_color_reset")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::None, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.shape-fill.black")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::BLACK), cx))
            .into(),
        MenuItem::tr("xlsx.shape-fill.white")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::WHITE), cx))
            .into(),
        MenuItem::tr("xlsx.shape-fill.blue")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-fill.cyan")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x17a2b8))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-fill.green")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x107c41))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-fill.yellow")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffc000))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-fill.orange")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xed7d31))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-fill.red")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-fill.purple")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7030a0))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-fill.gray")
            .disabled(!has_shape)
            .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7f7f7f))), cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.shape-fill", "format_color_fill", fill_color_items).into(),
    );

    // 図形の枠線 (xlsx.shape-stroke)
    let width_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.shape-stroke.width-0-5")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(6_350, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-0-75")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(9_525, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-1-5")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(19_050, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-2-25")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(28_575, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-3")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(38_100, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-4-5")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(57_150, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-6")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(76_200, cx))
            .into(),
    ];
    let dash_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.shape-stroke.dash-solid")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Solid, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.dash-dash")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Dash, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.dash-dot")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Dot, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.dash-dash-dot")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::DashDot, cx))
            .into(),
    ];
    let stroke_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.shape-stroke.none")
            .icon("format_color_reset")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(None, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.shape-stroke.black")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::BLACK), cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.white")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::WHITE), cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.blue")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.cyan")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x17a2b8))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.green")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x107c41))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.yellow")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffc000))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.orange")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xed7d31))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.red")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.purple")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7030a0))), cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.gray")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7f7f7f))), cx))
            .into(),
        MenuEntry::Separator,
        crate::shell::chrome::Submenu::tr("xlsx.shape-stroke.width", "line_weight", width_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.shape-stroke.dash", "border_style", dash_items).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.shape-stroke", "border_color", stroke_items).into(),
    );

    // 図形の効果 (xlsx.shape-effect)
    let effect_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.shape-effect.shadow-none")
            .disabled(!has_shape)
            .on(|this, _, cx| this.shadow_sheet_shape(None, cx))
            .into(),
        MenuItem::tr("xlsx.shape-effect.shadow-bottom-right")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                let shadow = ibis_doc_model::Shadow::simple(
                    ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 },
                    0,
                    50_800,
                    2_700_000,
                );
                this.shadow_sheet_shape(Some(shadow), cx);
            })
            .into(),
        MenuItem::tr("xlsx.shape-effect.shadow-right")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                let shadow = ibis_doc_model::Shadow::simple(
                    ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 },
                    0,
                    50_800,
                    0,
                );
                this.shadow_sheet_shape(Some(shadow), cx);
            })
            .into(),
        MenuItem::tr("xlsx.shape-effect.shadow-soft")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                let shadow = ibis_doc_model::Shadow::simple(
                    ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 },
                    50_800,
                    50_800,
                    2_700_000,
                );
                this.shadow_sheet_shape(Some(shadow), cx);
            })
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.shape-effect", "blur_on", effect_items).into(),
    );

    // テキストボックス・図形内テキストの書式設定 (XLSX-OBJECT-012 & XLSX-OBJECT-013)
    let has_text = editor.edited_body().is_some() || editor.selected_sheet_shape().and_then(|id| editor.slide().shape(id)).is_some_and(|s| s.text_body().is_some() || matches!(s.kind, ibis_doc_model::ShapeKind::TextBox(_) | ibis_doc_model::ShapeKind::Auto(_)));
    let text_color_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.text.color-black").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_color(ibis_doc_model::Rgb(0x000000), w, cx)).into(),
        MenuItem::tr("xlsx.text.color-white").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_color(ibis_doc_model::Rgb(0xffffff), w, cx)).into(),
        MenuItem::tr("xlsx.text.color-blue").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_color(ibis_doc_model::Rgb(0x2b579a), w, cx)).into(),
        MenuItem::tr("xlsx.text.color-cyan").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_color(ibis_doc_model::Rgb(0x17a2b8), w, cx)).into(),
        MenuItem::tr("xlsx.text.color-green").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_color(ibis_doc_model::Rgb(0x107c41), w, cx)).into(),
        MenuItem::tr("xlsx.text.color-yellow").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_color(ibis_doc_model::Rgb(0xffc000), w, cx)).into(),
        MenuItem::tr("xlsx.text.color-orange").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_color(ibis_doc_model::Rgb(0xed7d31), w, cx)).into(),
        MenuItem::tr("xlsx.text.color-red").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_color(ibis_doc_model::Rgb(0xc00000), w, cx)).into(),
        MenuItem::tr("xlsx.text.color-purple").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_color(ibis_doc_model::Rgb(0x7030a0), w, cx)).into(),
        MenuItem::tr("xlsx.text.color-gray").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_color(ibis_doc_model::Rgb(0x7f7f7f), w, cx)).into(),
    ];
    const TEXT_SIZES: [(&'static str, u32); 15] = [
        ("xlsx.text.size-8", 8),
        ("xlsx.text.size-9", 9),
        ("xlsx.text.size-10", 10),
        ("xlsx.text.size-11", 11),
        ("xlsx.text.size-12", 12),
        ("xlsx.text.size-14", 14),
        ("xlsx.text.size-16", 16),
        ("xlsx.text.size-18", 18),
        ("xlsx.text.size-20", 20),
        ("xlsx.text.size-24", 24),
        ("xlsx.text.size-28", 28),
        ("xlsx.text.size-32", 32),
        ("xlsx.text.size-36", 36),
        ("xlsx.text.size-44", 44),
        ("xlsx.text.size-72", 72),
    ];
    let text_size_items: Vec<MenuEntry> = TEXT_SIZES.into_iter().map(|(id, pt)| {
        MenuItem::new(id, format!("{pt} pt")).disabled(!has_text).on(move |this, w, cx| this.format_sheet_text_size(pt, w, cx)).into()
    }).collect();

    let text_format_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.text.bold").icon("format_bold").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_bold(w, cx)).into(),
        MenuItem::tr("xlsx.text.italic").icon("format_italic").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_italic(w, cx)).into(),
        MenuItem::tr("xlsx.text.underline").icon("format_underlined").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_underline(w, cx)).into(),
        MenuItem::tr("xlsx.text.strikethrough").icon("format_strikethrough").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_strikethrough(w, cx)).into(),
        MenuEntry::Separator,
        crate::shell::chrome::Submenu::tr("xlsx.text.color", "format_color_text", text_color_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.text.size", "format_size", text_size_items).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.text-format", "text_format", text_format_items).into(),
    );

    let text_align_items: Vec<MenuEntry> = vec![
        MenuItem::tr("align.left").icon("format_align_left").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_align(ibis_doc_model::TextAlign::Left, w, cx)).into(),
        MenuItem::tr("align.center").icon("format_align_center").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_align(ibis_doc_model::TextAlign::Center, w, cx)).into(),
        MenuItem::tr("align.right").icon("format_align_right").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_align(ibis_doc_model::TextAlign::Right, w, cx)).into(),
        MenuItem::tr("align.justify").icon("format_align_justify").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_align(ibis_doc_model::TextAlign::Justify, w, cx)).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.text-align", "format_align_left", text_align_items).into(),
    );

    let vertical_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.text-vertical.top").icon("vertical_align_top").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Top, cx)).into(),
        MenuItem::tr("xlsx.text-vertical.middle").icon("vertical_align_center").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Middle, cx)).into(),
        MenuItem::tr("xlsx.text-vertical.bottom").icon("vertical_align_bottom").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Bottom, cx)).into(),
    ];
    let direction_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.text-direction.horizontal").icon("format_textdirection_l_to_r").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_direction(false, 0, cx)).into(),
        MenuItem::tr("xlsx.text-direction.vertical").icon("format_textdirection_vertical").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_direction(true, 0, cx)).into(),
        MenuItem::tr("xlsx.text-direction.rotate-90").icon("rotate_90_degrees_cw").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_direction(false, 5_400_000, cx)).into(),
        MenuItem::tr("xlsx.text-direction.rotate-270").icon("rotate_90_degrees_ccw").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_direction(false, 16_200_000, cx)).into(),
    ];
    let insets_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.text-insets.none").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_insets(ibis_doc_model::Insets { left: 0, top: 0, right: 0, bottom: 0 }, cx)).into(),
        MenuItem::tr("xlsx.text-insets.normal").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_insets(ibis_doc_model::Insets::default(), cx)).into(),
        MenuItem::tr("xlsx.text-insets.wide").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_insets(ibis_doc_model::Insets { left: 182_880, top: 91_440, right: 182_880, bottom: 91_440 }, cx)).into(),
    ];

    let text_box_items: Vec<MenuEntry> = vec![
        crate::shell::chrome::Submenu::tr("xlsx.text-vertical", "vertical_align_center", vertical_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.text-direction", "format_textdirection_l_to_r", direction_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.text-insets", "padding", insets_items).into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.text.wrap").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_wrap(cx)).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.text-box", "text_fields", text_box_items).into(),
    );
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.text.wordart-styles", "title", wordart_style_items(editor)).into(),
    );
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.text.warp", "transform", text_warp_items(editor)).into(),
    );

    let shape_placement = editor.selected_sheet_shape().and_then(|id| editor.slide().shape_placements.get(&id).copied()).unwrap_or_default();
    let shape_print = editor.selected_sheet_shape().and_then(|id| editor.slide().shape_prints.get(&id).copied()).unwrap_or(true);

    let placement_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.placement.move-and-size")
            .disabled(!has_shape)
            .checked(has_shape && shape_placement == ibis_doc_model::ObjectPlacement::MoveAndSize)
            .on(|this, _, cx| this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::MoveAndSize, cx))
            .into(),
        MenuItem::tr("xlsx.placement.move-only")
            .disabled(!has_shape)
            .checked(has_shape && shape_placement == ibis_doc_model::ObjectPlacement::MoveOnly)
            .on(|this, _, cx| this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::MoveOnly, cx))
            .into(),
        MenuItem::tr("xlsx.placement.free")
            .disabled(!has_shape)
            .checked(has_shape && shape_placement == ibis_doc_model::ObjectPlacement::Free)
            .on(|this, _, cx| this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::Free, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.placement.print")
            .disabled(!has_shape)
            .checked(has_shape && shape_print)
            .on(|this, _, cx| this.toggle_sheet_shape_print(cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.object-properties", "settings", placement_items).into(),
    );

    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.frame-settings")
            .icon("straighten")
            .disabled(!has_shape)
            .on(|this, window, cx| {
                this.open_sheet_frame_panel(window, cx);
            })
            .into(),
    );
    let sel_pane_open = editor.sheet_state().is_some_and(|s| s.panels.selection_pane.is_some());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.selection-pane")
            .icon("visibility")
            .checked(sel_pane_open)
            .on(|this, window, cx| {
                this.toggle_xlsx_selection_pane(window, cx);
            })
            .into(),
    );
    menus.items_mut(crate::shell::chrome::MenuId::View).push(
        MenuItem::tr("xlsx.selection-pane")
            .icon("visibility")
            .checked(sel_pane_open)
            .on(|this, window, cx| {
                this.toggle_xlsx_selection_pane(window, cx);
            })
            .into(),
    );
    // 編集 > 形式を選択して貼り付け, after 書式なしで貼り付け.
    use super::clipboard::PasteSpecial as P;
    let special = |id: &'static str, label: &'static str, kind: P| -> MenuEntry { MenuItem::new(id, crate::shell::i18n::text(label)).on(move |this, _, cx| this.paste_special(kind, cx)).into() };
    let submenu: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.paste-special", "content_paste",
        vec![
            special("xlsx.paste-all", "_xlsx.all", P::All),
            special("xlsx.paste-values", "_xlsx.values", P::Values),
            special("xlsx.paste-formulas", "_xlsx.equation", P::Formulas),
            special("xlsx.paste-formats", "_xlsx.formats", P::Formats),
            special("xlsx.paste-validation", "xlsx.paste-validation", P::Validation),
            special("xlsx.paste-transpose", "_xlsx.transpose", P::Transpose),
            special("xlsx.paste-link", "_xlsx.paste-link", P::Link),
            MenuEntry::Separator,
            special("xlsx.paste-add", "_xlsx.add", P::Add),
            special("xlsx.paste-subtract", "_xlsx.subtract", P::Subtract),
            special("xlsx.paste-multiply", "_xlsx.multiply", P::Multiply),
            special("xlsx.paste-divide", "_xlsx.divide", P::Divide),
            MenuEntry::Separator,
            // A setting for the items above, as the dialog's check box.
            MenuItem::tr("xlsx.paste-skip-blanks")
                .checked(editor.sheet_state().is_some_and(|s| s.panels.skip_blanks))
                .on(|this, _, cx| {
                    if let Some(s) = this.sheet_state_mut() {
                        s.panels.skip_blanks = !s.panels.skip_blanks;
                    }
                    cx.notify();
                })
                .into(),
        ],
    )
    .into();
    for (_, entries) in menus.menus.iter_mut() {
        if insert_after(entries, "edit.paste-plain", submenu.clone()).is_ok() {
            break;
        }
    }
    // 編集 > すべてクリア (Excel's ホーム > クリア > すべてクリア), after 削除.
    let edit = menus.items_mut(crate::shell::chrome::MenuId::Edit);
    let at = edit.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "edit.delete")).map_or(edit.len(), |i| i + 1);
    edit.insert(at, MenuItem::tr("xlsx.clear-all").icon("delete").on(|this, _, cx| this.clear_all_cells(cx)).into());
    // コメントとメモのクリア (Excel's クリア > コメントとメモのクリア), after it.
    edit.insert(at + 1, MenuItem::tr("xlsx.clear-comments").icon("delete").on(|this, _, cx| this.delete_notes(cx)).into());
    let clear_sparklines_submenu = vec![
        MenuItem::tr("xlsx.sparkline-clear-selected").on(|this, _, cx| this.clear_sparklines_action(false, cx)).into(),
        MenuItem::tr("xlsx.sparkline-clear-group").on(|this, _, cx| this.clear_sparklines_action(true, cx)).into(),
    ];
    edit.insert(at + 2, crate::shell::chrome::Submenu::tr("xlsx.clear-sparklines", "delete", clear_sparklines_submenu).into());
    // 連続データの作成… (Excel's ホーム > フィル > 連続データの作成), after them.
    edit.insert(at + 3, MenuItem::tr("xlsx.series").icon("format_list_numbered").on(|this, window, cx| this.open_series_panel(window, cx)).into());
    // 下方向へコピー・右方向へコピー (Excel's フィル), ⌘D and ⌘R.
    edit.insert(at + 4, MenuItem::tr("xlsx.fill-down").icon("arrow_downward").shortcut("⌘D").on(|this, _, cx| this.fill_selection_edge(true, cx)).into());
    edit.insert(at + 5, MenuItem::tr("xlsx.fill-right").icon("arrow_forward").shortcut("⌘R").on(|this, _, cx| this.fill_selection_edge(false, cx)).into());
    menus.connect("edit.select-all", |this, _, cx| this.select_all_cells(cx));
    menus.connect("xlsx.insert-row-above", |this, _, cx| this.insert_lines(Axis::Rows, false, cx));
    menus.connect("xlsx.insert-row-below", |this, _, cx| this.insert_lines(Axis::Rows, true, cx));
    menus.connect("xlsx.insert-column-left", |this, _, cx| this.insert_lines(Axis::Columns, false, cx));
    menus.connect("xlsx.insert-column-right", |this, _, cx| this.insert_lines(Axis::Columns, true, cx));
    menus.connect("xlsx.delete-row", |this, _, cx| this.delete_lines(Axis::Rows, cx));
    menus.connect("xlsx.delete-column", |this, _, cx| this.delete_lines(Axis::Columns, cx));
    menus.connect("xlsx.insert-sheet", |this, window, cx| this.insert_sheet(window, cx));
    menus.connect("xlsx.insert-table", |this, window, cx| this.open_table_dialog(window, cx));
    menus.connect("xlsx.table-to-range", |this, _, cx| this.table_to_range(cx));
    menus.connect("xlsx.pivot-chart-insert", |this, window, cx| this.open_insert_pivot_chart(window, cx));
    menus.connect("xlsx.chart-format", |this, window, cx| this.open_sheet_chart_panel(window, cx));
    menus.connect("xlsx.move-chart", |this, window, cx| this.open_move_chart_panel(window, cx));
    menus.connect("xlsx.chart.save-template", |this, window, cx| this.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Save, window, cx));
    menus.connect("xlsx.chart-save-template", |this, window, cx| this.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Save, window, cx));
    menus.connect("xlsx.chart.apply-template", |this, window, cx| this.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Apply, window, cx));
    menus.connect("xlsx.chart-apply-template", |this, window, cx| this.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Apply, window, cx));
    menus.connect("xlsx.alt-text", |this, window, cx| this.open_sheet_alt_text_panel(window, cx));
    menus.connect("shape.alt-text", |this, window, cx| this.open_sheet_alt_text_panel(window, cx));
    menus.connect("xlsx.chart-alt-text", |this, window, cx| this.open_sheet_alt_text_panel(window, cx));
    menus.connect("xlsx.delete-sheet", |this, _, cx| this.delete_sheet(cx));
    menus.connect("xlsx.rename-sheet", |this, window, cx| this.begin_sheet_rename(this.current_slide, window, cx));
    menus.connect("xlsx.sparkline-line", |this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::Line), window, cx));
    menus.connect("xlsx.sparkline-column", |this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::Column), window, cx));
    menus.connect("xlsx.sparkline-winloss", |this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::WinLoss), window, cx));
    menus.connect("xlsx.sparkline-clear-selected", |this, _, cx| this.clear_sparklines_action(false, cx));
    menus.connect("xlsx.sparkline-clear-group", |this, _, cx| this.clear_sparklines_action(true, cx));
    menus.connect("xlsx.sparkline-group", |this, _, cx| this.group_selected_sparklines(cx));
    menus.connect("xlsx.sparkline-ungroup", |this, _, cx| this.ungroup_selected_sparklines(cx));
    menus.connect("xlsx.sparkline-style-1", |this, _, cx| this.set_selected_sparklines_style(0, cx));
    menus.connect("xlsx.sparkline-style-2", |this, _, cx| this.set_selected_sparklines_style(1, cx));
    menus.connect("xlsx.sparkline-style-3", |this, _, cx| this.set_selected_sparklines_style(2, cx));
    menus.connect("xlsx.sparkline-style-4", |this, _, cx| this.set_selected_sparklines_style(3, cx));
    menus.connect("xlsx.sparkline-style-5", |this, _, cx| this.set_selected_sparklines_style(4, cx));
    menus.connect("xlsx.sparkline-style-6", |this, _, cx| this.set_selected_sparklines_style(5, cx));
    menus.connect("xlsx.sparkline-color-blue", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x376092)), cx));
    menus.connect("xlsx.sparkline-color-red", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0xC00000)), cx));
    menus.connect("xlsx.sparkline-color-green", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x70AD47)), cx));
    menus.connect("xlsx.sparkline-color-purple", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x7030A0)), cx));
    menus.connect("xlsx.sparkline-color-cyan", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x4BACC6)), cx));
    menus.connect("xlsx.sparkline-color-orange", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0xED7D31)), cx));
    menus.connect("xlsx.sparkline-color-gray", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x7F7F7F)), cx));
    menus.connect("xlsx.sparkline-color-black", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x000000)), cx));
    menus.connect("xlsx.sparkline-display-hidden-on", |this, _, cx| this.set_selected_sparklines_display_hidden(true, cx));
    menus.connect("xlsx.sparkline-display-hidden-off", |this, _, cx| this.set_selected_sparklines_display_hidden(false, cx));
    menus.connect("xlsx.sparkline-date-axis-on", |this, _, cx| this.set_selected_sparklines_date_axis(true, cx));
    menus.connect("xlsx.sparkline-date-axis-off", |this, _, cx| this.set_selected_sparklines_date_axis(false, cx));
    menus.connect("xlsx.sparkline-empty-gap", |this, _, cx| this.set_selected_sparklines_empty(ibis_doc_model::SparklineEmpty::Gap, cx));
    menus.connect("xlsx.sparkline-empty-zero", |this, _, cx| this.set_selected_sparklines_empty(ibis_doc_model::SparklineEmpty::Zero, cx));
    menus.connect("xlsx.sparkline-empty-span", |this, _, cx| this.set_selected_sparklines_empty(ibis_doc_model::SparklineEmpty::Span, cx));
    menus.connect("xlsx.pivot-options", |this, window, cx| this.open_pivot_options(window, cx));
    menus.connect("xlsx.pivot-calculated-field", |this, window, cx| this.open_pivot_calc_field(window, cx));
    menus.connect("xlsx.pivot-calculated-item", |this, window, cx| this.open_pivot_calc_item(window, cx));
    menus.connect("xlsx.pivot-group", |this, window, cx| this.open_pivot_group(window, cx));
    menus.connect("xlsx.pivot-ungroup", |this, _, cx| this.pivot_ungroup(cx));
    menus.connect("edit.copy", |this, _, cx| this.copy_cells(false, cx));
    menus.connect("xlsx.copy-as-picture", |this, _, cx| this.copy_as_picture(cx));
    menus.connect("edit.cut", |this, _, cx| this.copy_cells(true, cx));
    menus.connect("edit.paste", |this, _, cx| this.paste_cells(false, cx));
    menus.connect("edit.paste-plain", |this, _, cx| this.paste_cells(true, cx));
    menus.connect("format.clear", |this, _, cx| this.clear_cell_formats(cx));
    menus.connect("xlsx.protect-workbook", |this, window, cx| this.open_protect_workbook_panel(window, cx));
    menus.connect("xlsx.unprotect-workbook", |this, window, cx| this.open_protect_workbook_panel(window, cx));
    menus.connect("xlsx.allow-edit-ranges", |this, window, cx| this.open_allow_edit_ranges_panel(window, cx));
    menus.connect("xlsx.paste-validation", |this, _, cx| this.paste_special(super::clipboard::PasteSpecial::Validation, cx));
    menus.connect("insert.text-box", |this, window, cx| { this.insert_sheet_textbox(window, cx); });
    menus.connect("xlsx.insert-textbox", |this, window, cx| { this.insert_sheet_textbox(window, cx); });
    menus.connect("xlsx.shape.rect", |this, _, cx| { this.insert_sheet_shape("rect", cx); });
    menus.connect("xlsx.shape.round-rect", |this, _, cx| { this.insert_sheet_shape("roundRect", cx); });
    menus.connect("xlsx.shape.ellipse", |this, _, cx| { this.insert_sheet_shape("ellipse", cx); });
    menus.connect("xlsx.shape.triangle", |this, _, cx| { this.insert_sheet_shape("triangle", cx); });
    menus.connect("xlsx.shape.right-arrow", |this, _, cx| { this.insert_sheet_shape("rightArrow", cx); });
    menus.connect("xlsx.shape.star5", |this, _, cx| { this.insert_sheet_shape("star5", cx); });
    menus.connect("xlsx.shape.callout", |this, _, cx| { this.insert_sheet_shape("wedgeRectCallout", cx); });
    menus.connect("xlsx.shape.heart", |this, _, cx| { this.insert_sheet_shape("heart", cx); });
    menus.connect("xlsx.shape.line", |this, _, cx| { this.insert_sheet_line(false, cx); });
    menus.connect("xlsx.shape.line-arrow", |this, _, cx| { this.insert_sheet_line(true, cx); });
    menus.connect("shape.rect", |this, _, cx| { this.insert_sheet_shape("rect", cx); });
    menus.connect("shape.roundRect", |this, _, cx| { this.insert_sheet_shape("roundRect", cx); });
    menus.connect("shape.ellipse", |this, _, cx| { this.insert_sheet_shape("ellipse", cx); });
    menus.connect("shape.triangle", |this, _, cx| { this.insert_sheet_shape("triangle", cx); });
    menus.connect("shape.rightArrow", |this, _, cx| { this.insert_sheet_shape("rightArrow", cx); });
    menus.connect("shape.star5", |this, _, cx| { this.insert_sheet_shape("star5", cx); });
    menus.connect("shape.wedgeRectCallout", |this, _, cx| { this.insert_sheet_shape("wedgeRectCallout", cx); });
    menus.connect("shape.heart", |this, _, cx| { this.insert_sheet_shape("heart", cx); });
    menus.connect("xlsx.insert-picture", |this, window, cx| { this.pick_sheet_picture(window, cx); });
    menus.connect("xlsx.image-file", |this, window, cx| { this.pick_sheet_picture(window, cx); });
    menus.connect("image.file", |this, window, cx| { this.pick_sheet_picture(window, cx); });
    menus.connect("xlsx.order-front", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Front, cx); });
    menus.connect("xlsx.order-back", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Back, cx); });
    menus.connect("xlsx.order-forward", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Forward, cx); });
    menus.connect("xlsx.order-backward", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Backward, cx); });
    menus.connect("order.front", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Front, cx); });
    menus.connect("order.back", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Back, cx); });
    menus.connect("order.forward", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Forward, cx); });
    menus.connect("order.backward", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Backward, cx); });
    menus.connect("xlsx.align.left", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Left, cx); });
    menus.connect("xlsx.align.center", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Center, cx); });
    menus.connect("xlsx.align.right", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Right, cx); });
    menus.connect("xlsx.align.top", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Top, cx); });
    menus.connect("xlsx.align.middle", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Middle, cx); });
    menus.connect("xlsx.align.bottom", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Bottom, cx); });
    menus.connect("xlsx.distribute.horizontal", |this, _, cx| { this.distribute_sheet_shapes(true, cx); });
    menus.connect("xlsx.distribute.vertical", |this, _, cx| { this.distribute_sheet_shapes(false, cx); });
    menus.connect("xlsx.frame-settings", |this, window, cx| { this.open_sheet_frame_panel(window, cx); });
    menus.connect("xlsx.selection-pane", |this, window, cx| { this.toggle_xlsx_selection_pane(window, cx); });

    menus.connect("xlsx.shape-fill.none", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::None, cx); });
    menus.connect("xlsx.shape-fill.black", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::BLACK), cx); });
    menus.connect("xlsx.shape-fill.white", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::WHITE), cx); });
    menus.connect("xlsx.shape-fill.blue", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a))), cx); });
    menus.connect("xlsx.shape-fill.cyan", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x17a2b8))), cx); });
    menus.connect("xlsx.shape-fill.green", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x107c41))), cx); });
    menus.connect("xlsx.shape-fill.yellow", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffc000))), cx); });
    menus.connect("xlsx.shape-fill.orange", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xed7d31))), cx); });
    menus.connect("xlsx.shape-fill.red", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000))), cx); });
    menus.connect("xlsx.shape-fill.purple", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7030a0))), cx); });
    menus.connect("xlsx.shape-fill.gray", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7f7f7f))), cx); });

    menus.connect("xlsx.shape-stroke.none", |this, _, cx| { this.stroke_sheet_shape_color(None, cx); });
    menus.connect("xlsx.shape-stroke.black", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::BLACK), cx); });
    menus.connect("xlsx.shape-stroke.white", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::WHITE), cx); });
    menus.connect("xlsx.shape-stroke.blue", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a))), cx); });
    menus.connect("xlsx.shape-stroke.cyan", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x17a2b8))), cx); });
    menus.connect("xlsx.shape-stroke.green", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x107c41))), cx); });
    menus.connect("xlsx.shape-stroke.yellow", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffc000))), cx); });
    menus.connect("xlsx.shape-stroke.orange", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xed7d31))), cx); });
    menus.connect("xlsx.shape-stroke.red", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000))), cx); });
    menus.connect("xlsx.shape-stroke.purple", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7030a0))), cx); });
    menus.connect("xlsx.shape-stroke.gray", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7f7f7f))), cx); });

    menus.connect("xlsx.shape-stroke.width-0-5", |this, _, cx| { this.stroke_sheet_shape_width(6_350, cx); });
    menus.connect("xlsx.shape-stroke.width-0-75", |this, _, cx| { this.stroke_sheet_shape_width(9_525, cx); });
    menus.connect("xlsx.shape-stroke.width-1-5", |this, _, cx| { this.stroke_sheet_shape_width(19_050, cx); });
    menus.connect("xlsx.shape-stroke.width-2-25", |this, _, cx| { this.stroke_sheet_shape_width(28_575, cx); });
    menus.connect("xlsx.shape-stroke.width-3", |this, _, cx| { this.stroke_sheet_shape_width(38_100, cx); });
    menus.connect("xlsx.shape-stroke.width-4-5", |this, _, cx| { this.stroke_sheet_shape_width(57_150, cx); });
    menus.connect("xlsx.shape-stroke.width-6", |this, _, cx| { this.stroke_sheet_shape_width(76_200, cx); });

    menus.connect("xlsx.shape-stroke.dash-solid", |this, _, cx| { this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Solid, cx); });
    menus.connect("xlsx.shape-stroke.dash-dash", |this, _, cx| { this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Dash, cx); });
    menus.connect("xlsx.shape-stroke.dash-dot", |this, _, cx| { this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Dot, cx); });
    menus.connect("xlsx.shape-stroke.dash-dash-dot", |this, _, cx| { this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::DashDot, cx); });

    menus.connect("xlsx.shape-effect.shadow-none", |this, _, cx| { this.shadow_sheet_shape(None, cx); });
    menus.connect("xlsx.shape-effect.shadow-bottom-right", |this, _, cx| {
        let shadow = ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 }, 0, 50_800, 2_700_000);
        this.shadow_sheet_shape(Some(shadow), cx);
    });
    menus.connect("xlsx.shape-effect.shadow-right", |this, _, cx| {
        let shadow = ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 }, 0, 50_800, 0);
        this.shadow_sheet_shape(Some(shadow), cx);
    });
    menus.connect("xlsx.shape-effect.shadow-soft", |this, _, cx| {
        let shadow = ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 }, 50_800, 50_800, 2_700_000);
        this.shadow_sheet_shape(Some(shadow), cx);
    });

    menus.connect("xlsx.group-shapes", |this, _, cx| { this.group_selected(cx); });
    menus.connect("xlsx.group-shapes.group", |this, _, cx| { this.group_selected(cx); });
    menus.connect("xlsx.group-shapes.ungroup", |this, _, cx| { this.ungroup_selected(cx); });

    menus.connect("xlsx.crop.left", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop { left: 10_000, ..Default::default() }, cx); });
    menus.connect("xlsx.crop.right", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop { right: 10_000, ..Default::default() }, cx); });
    menus.connect("xlsx.crop.top", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop { top: 10_000, ..Default::default() }, cx); });
    menus.connect("xlsx.crop.bottom", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop { bottom: 10_000, ..Default::default() }, cx); });
    menus.connect("xlsx.crop.center", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 }, cx); });
    menus.connect("xlsx.crop.reset", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop::default(), cx); });

    menus.connect("xlsx.picture.color.none", |this, _, cx| { this.set_selected_pictures_duotone(None, cx); });
    menus.connect("xlsx.picture.color.grayscale", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x000000), ibis_doc_model::Rgb(0xffffff))), cx); });
    menus.connect("xlsx.picture.color.sepia", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x402000), ibis_doc_model::Rgb(0xffe0c0))), cx); });
    menus.connect("xlsx.picture.color.blue", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))), cx); });
    menus.connect("xlsx.picture.color.green", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x004010), ibis_doc_model::Rgb(0xe2efda))), cx); });
    menus.connect("xlsx.picture.color.red", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x600000), ibis_doc_model::Rgb(0xfce4d6))), cx); });
    menus.connect("xlsx.picture.color.orange", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x803000), ibis_doc_model::Rgb(0xffe6cc))), cx); });
    menus.connect("xlsx.picture.color.purple", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x400060), ibis_doc_model::Rgb(0xf2e6ff))), cx); });

    menus.connect("xlsx.picture.lum.normal", |this, _, cx| { this.set_selected_pictures_luminance(None, cx); });
    menus.connect("xlsx.picture.lum.bright-p40", |this, _, cx| { this.set_selected_pictures_luminance(Some((40_000, 0)), cx); });
    menus.connect("xlsx.picture.lum.bright-p20", |this, _, cx| { this.set_selected_pictures_luminance(Some((20_000, 0)), cx); });
    menus.connect("xlsx.picture.lum.bright-m20", |this, _, cx| { this.set_selected_pictures_luminance(Some((-20_000, 0)), cx); });
    menus.connect("xlsx.picture.lum.bright-m40", |this, _, cx| { this.set_selected_pictures_luminance(Some((-40_000, 0)), cx); });
    menus.connect("xlsx.picture.lum.contrast-p40", |this, _, cx| { this.set_selected_pictures_luminance(Some((0, 40_000)), cx); });
    menus.connect("xlsx.picture.lum.contrast-p20", |this, _, cx| { this.set_selected_pictures_luminance(Some((0, 20_000)), cx); });
    menus.connect("xlsx.picture.lum.contrast-m20", |this, _, cx| { this.set_selected_pictures_luminance(Some((0, -20_000)), cx); });
    menus.connect("xlsx.picture.lum.contrast-m40", |this, _, cx| { this.set_selected_pictures_luminance(Some((0, -40_000)), cx); });
    menus.connect("xlsx.picture.lum.bright-contrast-high", |this, _, cx| { this.set_selected_pictures_luminance(Some((20_000, 20_000)), cx); });
    menus.connect("xlsx.picture.lum.bright-contrast-low", |this, _, cx| { this.set_selected_pictures_luminance(Some((-20_000, -20_000)), cx); });

    menus.connect("xlsx.picture.corner.none", |this, _, cx| { this.crop_selected_pictures_corner(0, cx); });
    menus.connect("xlsx.picture.corner.small", |this, _, cx| { this.crop_selected_pictures_corner(5_000, cx); });
    menus.connect("xlsx.picture.corner.medium", |this, _, cx| { this.crop_selected_pictures_corner(10_000, cx); });
    menus.connect("xlsx.picture.corner.large", |this, _, cx| { this.crop_selected_pictures_corner(20_000, cx); });
    menus.connect("xlsx.picture.corner.round", |this, _, cx| { this.crop_selected_pictures_corner(50_000, cx); });

    menus.connect("xlsx.picture.reset", |this, _, cx| { this.reset_picture_effects(cx); });

    menus.connect("xlsx.text.bold", |this, window, cx| { this.format_sheet_text_bold(window, cx); });
    menus.connect("xlsx.text.italic", |this, window, cx| { this.format_sheet_text_italic(window, cx); });
    menus.connect("xlsx.text.underline", |this, window, cx| { this.format_sheet_text_underline(window, cx); });
    menus.connect("xlsx.text.strikethrough", |this, window, cx| { this.format_sheet_text_strikethrough(window, cx); });

    menus.connect("xlsx.text.color-black", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x000000), w, cx); });
    menus.connect("xlsx.text.color-white", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0xffffff), w, cx); });
    menus.connect("xlsx.text.color-blue", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x2b579a), w, cx); });
    menus.connect("xlsx.text.color-cyan", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x17a2b8), w, cx); });
    menus.connect("xlsx.text.color-green", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x107c41), w, cx); });
    menus.connect("xlsx.text.color-yellow", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0xffc000), w, cx); });
    menus.connect("xlsx.text.color-orange", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0xed7d31), w, cx); });
    menus.connect("xlsx.text.color-red", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0xc00000), w, cx); });
    menus.connect("xlsx.text.color-purple", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x7030a0), w, cx); });
    menus.connect("xlsx.text.color-gray", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x7f7f7f), w, cx); });

    menus.connect("xlsx.text-vertical.top", |this, _, cx| { this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Top, cx); });
    menus.connect("xlsx.text-vertical.middle", |this, _, cx| { this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Middle, cx); });
    menus.connect("xlsx.text-vertical.bottom", |this, _, cx| { this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Bottom, cx); });

    menus.connect("xlsx.text-direction.horizontal", |this, _, cx| { this.format_sheet_text_direction(false, 0, cx); });
    menus.connect("xlsx.text-direction.vertical", |this, _, cx| { this.format_sheet_text_direction(true, 0, cx); });
    menus.connect("xlsx.text-direction.rotate-90", |this, _, cx| { this.format_sheet_text_direction(false, 5_400_000, cx); });
    menus.connect("xlsx.text-direction.rotate-270", |this, _, cx| { this.format_sheet_text_direction(false, 16_200_000, cx); });

    menus.connect("xlsx.text-insets.none", |this, _, cx| { this.format_sheet_text_insets(ibis_doc_model::Insets { left: 0, top: 0, right: 0, bottom: 0 }, cx); });
    menus.connect("xlsx.text-insets.normal", |this, _, cx| { this.format_sheet_text_insets(ibis_doc_model::Insets::default(), cx); });
    menus.connect("xlsx.text-insets.wide", |this, _, cx| { this.format_sheet_text_insets(ibis_doc_model::Insets { left: 182_880, top: 91_440, right: 182_880, bottom: 91_440 }, cx); });

    for &(id, pt) in &TEXT_SIZES {
        menus.connect(id, move |this, w, cx| { this.format_sheet_text_size(pt, w, cx); });
    }

    menus.connect("align.left", |this, w, cx| { this.format_sheet_text_align(ibis_doc_model::TextAlign::Left, w, cx); });
    menus.connect("align.center", |this, w, cx| { this.format_sheet_text_align(ibis_doc_model::TextAlign::Center, w, cx); });
    menus.connect("align.right", |this, w, cx| { this.format_sheet_text_align(ibis_doc_model::TextAlign::Right, w, cx); });
    menus.connect("align.justify", |this, w, cx| { this.format_sheet_text_align(ibis_doc_model::TextAlign::Justify, w, cx); });

    menus.connect("xlsx.placement.move-and-size", |this, _, cx| { this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::MoveAndSize, cx); });
    menus.connect("xlsx.placement.move-only", |this, _, cx| { this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::MoveOnly, cx); });
    menus.connect("xlsx.placement.free", |this, _, cx| { this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::Free, cx); });
    menus.connect("xlsx.placement.print", |this, _, cx| { this.toggle_sheet_shape_print(cx); });
    menus.connect("xlsx.placement.print-on", |this, _, cx| { this.set_sheet_shape_print(true, cx); });
    menus.connect("xlsx.placement.print-off", |this, _, cx| { this.set_sheet_shape_print(false, cx); });

    menus.connect("xlsx.text.wrap", |this, _, cx| { this.format_sheet_text_wrap(cx); });

    for &(preset, item_id, _, _) in super::shape_edit::CHANGE_SHAPES {
        menus.connect(item_id, move |this, _, cx| { this.change_sheet_shape(preset, cx); });
    }

    menus.connect("xlsx.shape-textlink.current-cell", |this, _, cx| { this.link_selected_shape_to_current_cell(cx); });
    menus.connect("xlsx.shape-textlink.clear", |this, _, cx| { this.clear_selected_shape_formula(cx); });

    menus.connect("xlsx.insert.wordart", |this, _, cx| {
        this.insert_sheet_wordart(0, None, cx);
    });
    menus.connect("insert.wordart", |this, _, cx| {
        this.insert_sheet_wordart(0, None, cx);
    });
    for (idx, &(_, insert_id, apply_id)) in XLSX_WORDART_STYLES.iter().enumerate() {
        menus.connect(insert_id, move |this, _, cx| {
            this.insert_sheet_wordart(idx, None, cx);
        });
        menus.connect(apply_id, move |this, window, cx| {
            this.set_selected_sheet_wordart_style(idx, window, cx);
        });
    }
    menus.connect("xlsx.text.warp.none", |this, _, cx| {
        this.set_sheet_text_warp(None, cx);
    });
    for &(prst, id, _) in XLSX_TEXT_WARP_PRESETS {
        menus.connect(id, move |this, _, cx| {
            this.set_sheet_text_warp(Some(prst), cx);
        });
    }
    menus.connect("xlsx.phonetic-toggle", |this, _, cx| {
        this.toggle_show_phonetic(cx);
    });
    menus.connect("xlsx.phonetic-edit", |this, window, cx| {
        this.open_phonetic_panel(window, cx);
    });

    if editor.workbook_structure_locked() {
        fn disable_ids(entries: &mut [MenuEntry], ids: &[&str]) {
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) => {
                        if ids.contains(&item.id) {
                            item.disabled = true;
                        }
                    }
                    MenuEntry::Submenu(sub) => {
                        if ids.contains(&sub.id) {
                            for item in &mut sub.items {
                                if let MenuEntry::Item(i) = item {
                                    i.disabled = true;
                                }
                            }
                        }
                        disable_ids(&mut sub.items, ids);
                    }
                    _ => {}
                }
            }
        }
        for (_, entries) in menus.menus.iter_mut() {
            disable_ids(entries, &[
                "xlsx.insert-sheet",
                "xlsx.delete-sheet",
                "xlsx.rename-sheet",
                "xlsx.hide-sheet",
                "xlsx.unhide-sheet",
                "xlsx.unhide-sheets",
                "xlsx.duplicate-sheet",
                "xlsx.move-sheet-left",
                "xlsx.move-sheet-right",
                "xlsx.tab-colors",
                "xlsx.tab-color",
                "xlsx.tab-color-none",
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::step_decimals;

    #[test]
    fn decimals() {
        assert_eq!(step_decimals("General", true), "0.0");
        assert_eq!(step_decimals("#,##0", true), "#,##0.0");
        assert_eq!(step_decimals("#,##0.00", false), "#,##0.0");
        assert_eq!(step_decimals("0.0%", false), "0%");
        assert_eq!(step_decimals("\"¥\"#,##0;\"¥\"\\-#,##0", true), "\"¥\"#,##0.0;\"¥\"\\-#,##0.0");
    }
}
