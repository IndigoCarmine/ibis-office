// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 編集 > フィル > 連続データの作成… (Excel's Series dialog): from the
//! first cell of each row (or column) of the selection, the cells after it
//! filled with a series: 加算 (adding the step), 乗算 (multiplying by
//! it), 日付 (by days, weekdays, months or years), stopping at the stop
//! value if one is given. With one cell selected and a stop value, the
//! series runs on from it until the stop value.
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Linear,
    Growth,
    Date,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DateUnit {
    Day,
    Weekday,
    Month,
    Year,
}

pub(crate) struct SeriesPanel {
    /// Down the columns (列) rather than along the rows (行).
    pub(crate) columns: bool,
    pub(crate) kind: Kind,
    pub(crate) unit: DateUnit,
    pub(crate) trend: bool,
    pub(crate) step: Entity<InputState>,
    pub(crate) stop: Entity<InputState>,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

/// Serial date → (year, month, day), and back (the 1900 system, without
/// its 29 February 1900: dates from March 1900 on).
fn ymd(serial: f64) -> (i64, u32, u32) {
    let days = serial.floor() as i64 - 25569; // days since 1970-01-01
    let (y, m, d) = ibis_calc::eval::convert::civil_from_days(days);
    (y, m, d)
}

fn serial(y: i64, m: u32, d: u32) -> f64 {
    (ibis_calc::eval::convert::days_from_civil(y, m as i64, d as i64) + 25569) as f64
}

/// The `n`th value after `start` (n = 1 the next), or `None` past `stop`.
pub(crate) fn nth(start: f64, step: f64, n: u32, kind: Kind, unit: DateUnit) -> f64 {
    match kind {
        Kind::Linear => start + step * n as f64,
        Kind::Growth => start * step.powi(n as i32),
        Kind::Date => match unit {
            DateUnit::Day => start + step * n as f64,
            DateUnit::Weekday => {
                // Saturday and Sunday skipped (serial 1 is a Sunday).
                let mut d = start;
                let mut left = (step * n as f64).round() as i64;
                let dir = if left < 0 { -1.0 } else { 1.0 };
                while left != 0 {
                    d += dir;
                    let weekday = (d as i64).rem_euclid(7); // 0 Saturday, 1 Sunday
                    if weekday != 0 && weekday != 1 {
                        left -= dir as i64;
                    }
                }
                d
            }
            DateUnit::Month | DateUnit::Year => {
                let (y, m, d) = ymd(start);
                let months = (step * n as f64).round() as i64 * if unit == DateUnit::Year { 12 } else { 1 };
                let total = y * 12 + (m as i64 - 1) + months;
                let (ny, nm) = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
                // The day kept, or the month's last.
                let last = (28..=31).rev().find(|&day| ymd(serial(ny, nm, day)).1 == nm).unwrap_or(28);
                serial(ny, nm, d.min(last)) + start.fract()
            }
        },
    }
}

/// The least-squares line through (0, y0), (1, y1), … at `x`.
pub(crate) fn linear_trend(values: &[f64], x: f64) -> f64 {
    let n = values.len() as f64;
    if n <= 1.0 {
        return values.first().copied().unwrap_or(0.0) + x;
    }
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = values.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (i, v) in values.iter().enumerate() {
        let dx = i as f64 - mean_x;
        sxy += dx * (v - mean_y);
        sxx += dx * dx;
    }
    let slope = if sxx == 0.0 { 0.0 } else { sxy / sxx };
    mean_y + slope * (x - mean_x)
}

/// The exponential growth curve through (0, y0), (1, y1), … at `x`.
/// Computed by linear regression on ln(y), returning exp(trend(ln_y, x)).
pub(crate) fn growth_trend(values: &[f64], x: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return values[0];
    }
    if values.iter().all(|&v| v > 0.0) {
        let log_values: Vec<f64> = values.iter().map(|&v| v.ln()).collect();
        let log_y = linear_trend(&log_values, x);
        log_y.exp()
    } else {
        linear_trend(values, x)
    }
}

/// To 15 significant digits (so 0.1 steps stay 0.3, not 0.30000000000000004).
pub(crate) fn round15(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    format!("{v:.14e}").parse().unwrap_or(v)
}

impl SlideEditor {
    pub(crate) fn open_series_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let input = |text: &str, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            let t = text.to_string();
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        // Excel's guess: down the columns when the selection is taller.
        let columns = range.last_row - range.first_row >= range.last_column - range.first_column;
        let panel = SeriesPanel { columns, kind: Kind::Linear, unit: DateUnit::Day, trend: false, step: input("1", window, cx), stop: input("", window, cx), message: String::new(), ui: Default::default() };
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
        s.panels.series = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn series_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.series.as_mut() {
            p.message = message.to_string();
        }
        cx.notify();
    }

    pub(crate) fn apply_series_panel(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.series.as_ref()) else { return };
        let (columns, kind, unit, trend) = (p.columns, p.kind, p.unit, p.trend);
        if trend {
            let (Some(range), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) else { return };
            let mut edits = Vec::new();
            if columns {
                for c in range.first_column..=range.last_column {
                    let mut known = Vec::new();
                    for r in range.first_row..=range.last_row {
                        match sheet.cell(r, c).map(|cell| &cell.value) {
                            Some(CellValue::Number(n)) => known.push(*n),
                            _ => break,
                        }
                    }
                    if known.is_empty() {
                        continue;
                    }
                    let total_len = range.last_row - range.first_row + 1;
                    for i in 0..total_len {
                        let v = match kind {
                            Kind::Growth => growth_trend(&known, i as f64),
                            _ => linear_trend(&known, i as f64),
                        };
                        edits.push(CellEdit {
                            row: range.first_row + i,
                            column: c,
                            input: CellInput::Value(CellValue::Number(round15(v))),
                        });
                    }
                }
            } else {
                for r in range.first_row..=range.last_row {
                    let mut known = Vec::new();
                    for c in range.first_column..=range.last_column {
                        match sheet.cell(r, c).map(|cell| &cell.value) {
                            Some(CellValue::Number(n)) => known.push(*n),
                            _ => break,
                        }
                    }
                    if known.is_empty() {
                        continue;
                    }
                    let total_len = range.last_column - range.first_column + 1;
                    for i in 0..total_len {
                        let v = match kind {
                            Kind::Growth => growth_trend(&known, i as f64),
                            _ => linear_trend(&known, i as f64),
                        };
                        edits.push(CellEdit {
                            row: r,
                            column: range.first_column + i,
                            input: CellInput::Value(CellValue::Number(round15(v))),
                        });
                    }
                }
            }
            if edits.is_empty() {
                return self.series_message(text("_xlsx.enter-a-number-in-the-first-cell"), cx);
            }
            let slide = self.current_slide;
            let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits })];
            let first = self.current_sheet().and_then(|(_, s)| s.cell(range.first_row, range.first_column).map(|c| s.styles[c.style as usize].number_format.clone()));
            if let Some(code) = first.filter(|c| c != "General") {
                commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: ibis_doc_model::StyleChange::NumberFormat(code) }));
            }
            if let Err(err) = self.document.execute(commands) {
                return self.series_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
            }
            self.wb_mut().panels.series = None;
            return cx.notify();
        }
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let number = |e: &Entity<InputState>| -> Option<f64> {
            match ibis_calc::input::parse_input(e.read(cx).value().trim(), date1904) {
                ibis_calc::input::Typed::Value(CellValue::Number(n), _) => Some(n),
                _ => None,
            }
        };
        let Some(step) = number(&p.step) else { return self.series_message(text("fix.series.enter-step-value"), cx) };
        let stop_text = p.stop.read(cx).value().trim().to_string();
        let stop = number(&p.stop);
        if !stop_text.is_empty() && stop.is_none() {
            return self.series_message(text("fix.series.enter-number-date-stop-value"), cx);
        }
        let (Some(range), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) else { return };
        // One cell with a stop value: the series runs on until it.
        let range = if range.first_row == range.last_row && range.first_column == range.last_column {
            let Some(stop) = stop else { return self.series_message(text("fix.series.enter-stop-value-fill-one"), cx) };
            let start = match sheet.cell(range.first_row, range.first_column).map(|c| &c.value) {
                Some(CellValue::Number(n)) => *n,
                _ => return self.series_message(text("fix.series.enter-number-date-first-cell"), cx),
            };
            let mut count = 0u32;
            while count < 10_000 {
                let v = nth(start, step, count + 1, kind, unit);
                if (step >= 0.0 && kind != Kind::Growth || kind == Kind::Growth && step >= 1.0) && v > stop + 1e-9 || (step < 0.0 && v < stop - 1e-9) {
                    break;
                }
                count += 1;
            }
            if columns {
                CellRange { last_row: range.first_row + count, ..range }
            } else {
                CellRange { last_column: range.first_column + count, ..range }
            }
        } else {
            range
        };
        let mut edits = Vec::new();
        let lines: Vec<(u32, u32)> = if columns { (range.first_column..=range.last_column).map(|c| (range.first_row, c)).collect() } else { (range.first_row..=range.last_row).map(|r| (r, range.first_column)).collect() };
        for (row, column) in lines {
            let Some(CellValue::Number(start)) = sheet.cell(row, column).map(|c| c.value.clone()) else { continue };
            let length = if columns { range.last_row - range.first_row } else { range.last_column - range.first_column };
            for n in 1..=length {
                let v = nth(start, step, n, kind, unit);
                if let Some(stop) = stop
                    && ((step >= 0.0 && v > stop + 1e-9) || (step < 0.0 && v < stop - 1e-9))
                {
                    break;
                }
                let (r, c) = if columns { (row + n, column) } else { (row, column + n) };
                edits.push(CellEdit { row: r, column: c, input: CellInput::Value(CellValue::Number(v)) });
            }
        }
        if edits.is_empty() {
            return self.series_message(text("fix.series.enter-number-date-first-cell"), cx);
        }
        // The series takes the first cell's number format (a date stays a date).
        let slide = self.current_slide;
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits: edits.clone() })];
        let first = self.current_sheet().and_then(|(_, s)| s.cell(range.first_row, range.first_column).map(|c| s.styles[c.style as usize].number_format.clone()));
        if let Some(code) = first.filter(|c| c != "General") {
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: ibis_doc_model::StyleChange::NumberFormat(code) }));
        }
        if let Err(err) = self.document.execute(commands) {
            return self.series_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
        }
        self.wb_mut().panels.series = None;
        cx.notify();
    }

    pub(crate) fn series_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.series.as_ref()?;
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
        type Set = fn(&mut SeriesPanel);
        let radio = |id: &'static str, on: bool, label: &str, set: Set| {
            mark(
                id,
                Button::new(id)
                    .small()
                    .ghost()
                    .label(format!("{} {}", if on { "◉" } else { "○" }, text(label)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.series.as_mut() {
                            set(p);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        let toggle = |id: &'static str, on: bool, label: &str, set: Set| {
            mark(
                id,
                Button::new(id)
                    .small()
                    .ghost()
                    .label(format!("{} {}", if on { "☑" } else { "☐" }, text(label)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.series.as_mut() {
                            set(p);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .child(div().text_size(px(13.)).child(text("_shell.range")))
            .child(h_flex().gap_1().child(radio("series-rows", !p.columns, "_xlsx.row", |p| p.columns = false)).child(radio("series-columns", p.columns, "_xlsx.column", |p| p.columns = true)))
            .child(div().text_size(px(13.)).child(text("fix.series.type")))
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .child(radio("series-linear", p.kind == Kind::Linear, "_xlsx.add", |p| p.kind = Kind::Linear))
                    .child(radio("series-growth", p.kind == Kind::Growth, "_xlsx.multiply", |p| p.kind = Kind::Growth))
                    .child(radio("series-date", p.kind == Kind::Date, "_xlsx.short-date", |p| p.kind = Kind::Date)),
            );
        if !p.trend && p.kind == Kind::Date {
            body = body.child(div().text_size(px(13.)).child(text("fix.series.date-unit"))).child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .child(radio("unit-day", p.unit == DateUnit::Day, "fix.series.day", |p| p.unit = DateUnit::Day))
                    .child(radio("unit-weekday", p.unit == DateUnit::Weekday, "fix.series.weekday", |p| p.unit = DateUnit::Weekday))
                    .child(radio("unit-month", p.unit == DateUnit::Month, "fix.series.month", |p| p.unit = DateUnit::Month))
                    .child(radio("unit-year", p.unit == DateUnit::Year, "fix.series.year", |p| p.unit = DateUnit::Year)),
            );
        }
        body = body.child(toggle("series-trend", p.trend, "_xlsx.trend", |p| p.trend = !p.trend));
        if !p.trend {
            body = body
                .child(div().text_size(px(13.)).child(text("fix.series.step-value")))
                .child(mark("step", Input::new(&p.step).small().into_any_element()))
                .child(div().text_size(px(13.)).child(text("fix.series.stop-value")))
                .child(mark("stop", Input::new(&p.stop).small().into_any_element()));
        }
        body = body
            .child(div().text_size(px(12.)).child(SharedString::from(p.message.clone())))
            .child(mark("apply", Button::new("series-apply").primary().small().label(text("OK")).on_click(cx.listener(|this, _, _, cx| this.apply_series_panel(cx))).into_any_element()))
            .child(mark(
                "close",
                Button::new("series-close")
                    .small()
                    .ghost()
                    .label(text("_shell.close"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.wb_mut().panels.series = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        Some(SidebarPanel { title: "fix.series.series".into(), body: body.into_any_element() })
    }
}

#[cfg(test)]
mod tests {
    use super::{DateUnit, Kind, growth_trend, linear_trend, nth, round15, serial, ymd};

    #[test]
    fn steps() {
        assert_eq!(nth(1.0, 2.0, 3, Kind::Linear, DateUnit::Day), 7.0);
        assert_eq!(nth(3.0, 2.0, 3, Kind::Growth, DateUnit::Day), 24.0);
        // 2026/1/31 + 1 month: 2026/2/28.
        let jan31 = serial(2026, 1, 31);
        assert_eq!(ymd(nth(jan31, 1.0, 1, Kind::Date, DateUnit::Month)), (2026, 2, 28));
        assert_eq!(ymd(nth(jan31, 1.0, 1, Kind::Date, DateUnit::Year)), (2027, 1, 31));
        // Friday 2026/10/9 + 1 weekday: Monday 2026/10/12.
        assert_eq!(ymd(nth(serial(2026, 10, 9), 1.0, 1, Kind::Date, DateUnit::Weekday)), (2026, 10, 12));
        assert_eq!(serial(2026, 10, 7), 46302.0);
    }

    #[test]
    fn linear_and_growth_trends() {
        // Linear trend: (0, 10), (1, 25) -> slope = 15
        assert_eq!(linear_trend(&[10.0, 25.0], 0.0), 10.0);
        assert_eq!(linear_trend(&[10.0, 25.0], 1.0), 25.0);
        assert_eq!(linear_trend(&[10.0, 25.0], 2.0), 40.0);
        assert_eq!(linear_trend(&[10.0, 25.0], 4.0), 70.0);

        // Growth trend: (0, 10), (1, 20) -> ratio = 2
        assert_eq!(round15(growth_trend(&[10.0, 20.0], 0.0)), 10.0);
        assert_eq!(round15(growth_trend(&[10.0, 20.0], 1.0)), 20.0);
        assert_eq!(round15(growth_trend(&[10.0, 20.0], 2.0)), 40.0);
        assert_eq!(round15(growth_trend(&[10.0, 20.0], 3.0)), 80.0);
        assert_eq!(round15(growth_trend(&[10.0, 20.0], 4.0)), 160.0);

        // Growth trend across rows: (0, 3), (1, 9) -> ratio = 3
        assert_eq!(round15(growth_trend(&[3.0, 9.0], 2.0)), 27.0);
        assert_eq!(round15(growth_trend(&[3.0, 9.0], 3.0)), 81.0);
    }
}
