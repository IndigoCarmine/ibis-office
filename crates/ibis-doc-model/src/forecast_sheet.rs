// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > 予測シート: the cells of the sheet Excel makes from a timeline and the values on it. The
//! timeline and the values (copied), the forecast, and the lower and upper bounds of its confidence
//! interval, as formulas, and a table of statistics.
//!
//! The forecast is exponential smoothing (`FORECAST.ETS`, with the season found automatically), its bounds
//! `FORECAST.ETS.CONFINT` of the confidence level, and the statistics `FORECAST.ETS.STAT` (the smoothing
//! parameters and the errors), as Excel's forecast sheet has them. ibisOffice's smoothing is the additive
//! ETS(A,A,A) of the published literature, so its numbers are close to Excel's but not the same.

use crate::sheet::CellRange;
use crate::sheet_command::CellInput;
use ibis_calc::CellValue;

/// What the user chose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// The timeline value the forecast ends at.
    pub end: f64,
    /// The timeline value the forecast starts at (None = default to last historical data point).
    pub start: Option<f64>,
    /// The confidence level of the interval (0.95).
    pub confidence: f64,
    /// Seasonality: None = automatic detection, Some(m) = manual season length (>= 0).
    pub seasonality: Option<i64>,
    /// How missing points are completed: 1 = interpolation (default), 0 = zeros.
    pub data_completion: i64,
    /// How duplicate timestamps are aggregated: 1 = average (default), 2 = count, 3 = counta, 4 = max, 5 = median, 6 = min, 7 = sum.
    pub aggregation: i64,
    /// Whether the statistics table goes beside the data.
    pub statistics: bool,
    /// The workbook counts dates from 1904.
    pub date1904: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            end: 0.0,
            start: None,
            confidence: 0.95,
            seasonality: None,
            data_completion: 1,
            aggregation: 1,
            statistics: true,
            date1904: false,
        }
    }
}

/// Why the sheet cannot be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForecastError {
    /// Fewer than three points.
    TooFew,
    /// The timeline does not rise.
    NotAscending,
    /// The steps of the timeline are not all the same.
    Irregular,
    /// The end is not after the last point.
    EndTooEarly,
    /// The confidence level is not between 0 and 1.
    Confidence,
    /// The end is too far (more than 5000 steps).
    TooLong,
    /// The start is after the end.
    StartAfterEnd,
    /// The start is after the last point.
    StartTooLate,
    /// The start is before the first point.
    StartTooEarly,
}

impl ForecastError {
    /// The id of the message shown.
    pub fn message(self) -> &'static str {
        match self {
            Self::TooFew => "xlsx.forecast.error.too-few",
            Self::NotAscending => "xlsx.forecast.error.not-ascending",
            Self::Irregular => "xlsx.forecast.error.irregular",
            Self::EndTooEarly => "xlsx.forecast.error.end-too-early",
            Self::Confidence => "xlsx.forecast.error.confidence",
            Self::TooLong => "xlsx.forecast.error.too-long",
            Self::StartAfterEnd => "xlsx.forecast.error.start-after-end",
            Self::StartTooLate => "xlsx.forecast.error.start-too-late",
            Self::StartTooEarly => "xlsx.forecast.error.start-too-early",
        }
    }
}

/// The sheet's cells (row, column, input) and the number formats of ranges.
#[derive(Clone, Debug, Default)]
pub struct Built {
    pub cells: Vec<(u32, u32, CellInput)>,
    pub formats: Vec<(CellRange, String)>,
    /// The last row of the timeline, forecast included.
    pub last_row: u32,
}

fn text(s: String) -> CellInput {
    CellInput::Value(CellValue::Text(s))
}

fn number(v: f64) -> CellInput {
    CellInput::Value(CellValue::Number(v))
}

/// How a timeline steps: by the same amount, or by whole months (dates on the same day of each month).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Amount(f64),
    Months(i64),
}

fn days_in_month(y: i64, m: u32) -> u32 {
    let next = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    (ibis_calc::eval::convert::days_from_civil(next.0, next.1 as i64, 1) - ibis_calc::eval::convert::days_from_civil(y, m as i64, 1)) as u32
}

/// The step of a timeline (checked ascending, at least three points).
pub fn step_of(timeline: &[f64], date1904: bool) -> Result<Step, ForecastError> {
    if timeline.len() < 3 {
        return Err(ForecastError::TooFew);
    }
    if timeline.windows(2).any(|w| w[1] <= w[0]) {
        return Err(ForecastError::NotAscending);
    }
    let step = (timeline[timeline.len() - 1] - timeline[0]) / (timeline.len() - 1) as f64;
    if timeline.windows(2).all(|w| ((w[1] - w[0]) - step).abs() <= step.abs() * 1e-9 + 1e-12) {
        return Ok(Step::Amount(step));
    }
    // Dates a month (or a quarter, a year) apart, on the same day of the month.
    if timeline.iter().all(|t| t.fract() == 0.0) {
        let dates: Vec<(i64, u32, u32)> = timeline.iter().map(|&t| ibis_calc::eval::convert::date_from_serial(t as i64, date1904)).collect();
        let months = |a: (i64, u32, u32), b: (i64, u32, u32)| (b.0 - a.0) * 12 + b.1 as i64 - a.1 as i64;
        let k = months(dates[0], dates[1]);
        let same_day = dates.windows(2).all(|w| w[1].2 == w[0].2 && months(w[0], w[1]) == k) || dates.iter().all(|d| d.2 == days_in_month(d.0, d.1)) && dates.windows(2).all(|w| months(w[0], w[1]) == k);
        if k >= 1 && same_day {
            return Ok(Step::Months(k));
        }
    }
    Err(ForecastError::Irregular)
}

/// The timeline value `k` steps after `last` (the date `k` steps on for months).
pub fn after(last: f64, step: Step, k: usize, date1904: bool) -> f64 {
    match step {
        Step::Amount(a) => last + a * k as f64,
        Step::Months(m) => {
            let (y, mo, d) = ibis_calc::eval::convert::date_from_serial(last as i64, date1904);
            let end_of_month = d == days_in_month(y, mo);
            let total = y * 12 + mo as i64 - 1 + m * k as i64;
            let (ny, nm) = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
            let nd = if end_of_month { days_in_month(ny, nm) } else { d.min(days_in_month(ny, nm)) };
            ibis_calc::eval::convert::serial_from_date(ny, nm as i64, nd as i64, date1904) as f64
        }
    }
}

fn ets_extra_args(s: &Settings) -> String {
    if s.aggregation != 1 {
        let season = s.seasonality.unwrap_or(1);
        format!(",{season},{},{}", s.data_completion, s.aggregation)
    } else if s.data_completion != 1 {
        let season = s.seasonality.unwrap_or(1);
        format!(",{season},{}", s.data_completion)
    } else if let Some(m) = s.seasonality {
        format!(",{m}")
    } else {
        String::new()
    }
}

/// The cells of the forecast sheet. `label` gives the text of a label id and the names the labels of the
/// value column (`Forecast(name)`, bounds).
pub fn build(timeline: &[f64], values: &[f64], names: (&str, &str), label: &dyn Fn(&str) -> String, s: &Settings) -> Result<Built, ForecastError> {
    if timeline.len() != values.len() || timeline.len() < 3 {
        return Err(ForecastError::TooFew);
    }
    if !(s.confidence > 0.0 && s.confidence < 1.0) {
        return Err(ForecastError::Confidence);
    }
    let step = step_of(timeline, s.date1904)?;
    let n = timeline.len();
    let last = timeline[n - 1];
    // The forecast's rows: as many steps as reach the end.
    let mut ahead = 0usize;
    while after(last, step, ahead + 1, s.date1904) <= s.end + 1e-9 {
        ahead += 1;
        if ahead > 5000 {
            return Err(ForecastError::TooLong);
        }
    }
    if ahead == 0 {
        return Err(ForecastError::EndTooEarly);
    }
    let start_idx = match s.start {
        Some(st) => {
            if st > s.end + 1e-9 {
                return Err(ForecastError::StartAfterEnd);
            }
            if st > last + 1e-9 {
                return Err(ForecastError::StartTooLate);
            }
            if st < timeline[0] - 1e-9 {
                return Err(ForecastError::StartTooEarly);
            }
            timeline.iter().position(|&t| (t - st).abs() <= 1e-6 || t >= st - 1e-9).unwrap_or(n - 1)
        }
        None => n - 1,
    };
    let extra = ets_extra_args(s);
    let mut out = Built { last_row: (n + ahead) as u32, ..Built::default() };
    let name = names.1;
    for (c, t) in [names.0.to_string(), name.to_string(), format!("{}({name})", label("xlsx.forecast.forecast")), format!("{}({name})", label("xlsx.forecast.lower")), format!("{}({name})", label("xlsx.forecast.upper"))].into_iter().enumerate() {
        out.cells.push((0, c as u32, text(t)));
    }
    let y = format!("$B$2:$B${}", n + 1);
    let x = format!("$A$2:$A${}", n + 1);
    for i in 0..n {
        let r = i as u32 + 1;
        out.cells.push((r, 0, number(timeline[i])));
        out.cells.push((r, 1, number(values[i])));
    }
    if start_idx == n - 1 {
        // The forecast starts from the last point, so its line meets the values there.
        let join = n as u32;
        for c in 2..5 {
            out.cells.push((join, c, CellInput::Formula(format!("B{}", join + 1))));
        }
    } else {
        // Forecast starts earlier: historical rows from start_idx to n - 1 get the forecast formula
        for i in start_idx..n {
            let r = (i + 1) as u32;
            let row = r + 1;
            out.cells.push((r, 2, CellInput::Formula(format!("FORECAST.ETS(A{row},{y},{x}{extra})"))));
        }
    }
    for k in 1..=ahead {
        let r = (n + k) as u32;
        let row = r + 1;
        out.cells.push((r, 0, number(after(last, step, k, s.date1904))));
        out.cells.push((r, 2, CellInput::Formula(format!("FORECAST.ETS(A{row},{y},{x}{extra})"))));
        let width = format!("FORECAST.ETS.CONFINT(A{row},{y},{x},{}{extra})", number_text(s.confidence));
        out.cells.push((r, 3, CellInput::Formula(format!("C{row}-{width}"))));
        out.cells.push((r, 4, CellInput::Formula(format!("C{row}+{width}"))));
    }
    // The first timeline cell's format is the format of the whole column (a date stays a date).
    out.formats.push((CellRange { first_row: 1, first_column: 2, last_row: out.last_row, last_column: 4 }, "General".to_string()));
    if s.statistics {
        let c = 6;
        out.cells.push((0, c, text(label("xlsx.forecast.statistics"))));
        // Excel's list: the smoothing parameters, then the errors of the fit.
        let rows: [(&str, u32); 7] = [("xlsx.forecast.alpha", 1), ("xlsx.forecast.beta", 2), ("xlsx.forecast.gamma", 3), ("xlsx.forecast.mase", 4), ("xlsx.forecast.smape", 5), ("xlsx.forecast.mae", 6), ("xlsx.forecast.rmse", 7)];
        for (i, (id, kind)) in rows.into_iter().enumerate() {
            out.cells.push((i as u32 + 1, c, text(label(id))));
            out.cells.push((i as u32 + 1, c + 1, CellInput::Formula(format!("FORECAST.ETS.STAT({y},{x},{kind}{extra})"))));
        }
    }
    Ok(out)
}

fn number_text(v: f64) -> String {
    ibis_calc::number_to_xml(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(id: &str) -> String {
        id.to_string()
    }

    #[test]
    fn a_regular_timeline_gives_the_forecast_rows() {
        let t = [1.0, 2.0, 3.0, 4.0];
        let v = [2.0, 4.0, 6.0, 8.0];
        let b = build(&t, &v, ("t", "y"), &labels, &Settings { end: 7.0, confidence: 0.95, statistics: true, date1904: false, ..Default::default() }).unwrap();
        assert_eq!(b.last_row, 7);
        // Header, 4 points, 3 forecast rows: the timeline of each row is written.
        let a_cells: Vec<_> = b.cells.iter().filter(|(_, c, _)| *c == 0).collect();
        assert_eq!(a_cells.len(), 1 + 4 + 3);
        assert!(b.cells.iter().any(|(r, c, i)| (*r, *c) == (5, 2) && matches!(i, CellInput::Formula(f) if f.starts_with("FORECAST.ETS(A6,"))));
        assert!(b.cells.iter().any(|(r, c, _)| (*r, *c) == (1, 7)));
    }

    #[test]
    fn dates_a_month_apart_step_by_months() {
        let d = |y, m, day| ibis_calc::eval::convert::serial_from_date(y, m, day, false) as f64;
        let t = [d(2025, 11, 15), d(2025, 12, 15), d(2026, 1, 15), d(2026, 2, 15)];
        let s = Settings { end: d(2026, 5, 20), confidence: 0.95, statistics: false, date1904: false, ..Default::default() };
        let b = build(&t, &[1.0, 2.0, 3.0, 4.0], ("t", "y"), &labels, &s).unwrap();
        assert_eq!(b.last_row, 7);
        let at = |r: u32| match &b.cells.iter().find(|(row, c, _)| (*row, *c) == (r, 0)).unwrap().2 {
            CellInput::Value(CellValue::Number(n)) => *n,
            other => panic!("{other:?}"),
        };
        assert_eq!((at(5), at(6), at(7)), (d(2026, 3, 15), d(2026, 4, 15), d(2026, 5, 15)));
    }

    #[test]
    fn bad_timelines_are_refused() {
        let s = Settings { end: 10.0, confidence: 0.95, statistics: false, date1904: false, ..Default::default() };
        assert_eq!(build(&[1.0, 2.0], &[1.0, 2.0], ("t", "y"), &labels, &s).unwrap_err(), ForecastError::TooFew);
        assert_eq!(build(&[1.0, 3.0, 4.0], &[1.0, 2.0, 3.0], ("t", "y"), &labels, &s).unwrap_err(), ForecastError::Irregular);
        assert_eq!(build(&[3.0, 2.0, 1.0], &[1.0, 2.0, 3.0], ("t", "y"), &labels, &s).unwrap_err(), ForecastError::NotAscending);
        assert_eq!(build(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0], ("t", "y"), &labels, &Settings { end: 3.0, ..s }).unwrap_err(), ForecastError::EndTooEarly);
        assert_eq!(build(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0], ("t", "y"), &labels, &Settings { confidence: 1.5, ..s }).unwrap_err(), ForecastError::Confidence);
        assert_eq!(build(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0], ("t", "y"), &labels, &Settings { start: Some(15.0), ..s }).unwrap_err(), ForecastError::StartAfterEnd);
        assert_eq!(build(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0], ("t", "y"), &labels, &Settings { start: Some(5.0), ..s }).unwrap_err(), ForecastError::StartTooLate);
        assert_eq!(build(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0], ("t", "y"), &labels, &Settings { start: Some(0.0), ..s }).unwrap_err(), ForecastError::StartTooEarly);
    }

    #[test]
    fn custom_options_and_historical_start_are_reflected_in_formulas() {
        let t = [1.0, 2.0, 3.0, 4.0, 5.0];
        let v = [10.0, 20.0, 30.0, 40.0, 50.0];
        let s = Settings {
            end: 8.0,
            start: Some(3.0),
            confidence: 0.95,
            seasonality: Some(4),
            data_completion: 0,
            aggregation: 7,
            statistics: true,
            date1904: false,
        };
        let b = build(&t, &v, ("t", "y"), &labels, &s).unwrap();
        // Row 3 (t=3.0, r=3, row=4): forecast formula exists
        let c4 = b.cells.iter().find(|(r, c, _)| (*r, *c) == (3, 2)).unwrap();
        match &c4.2 {
            CellInput::Formula(f) => assert_eq!(f, "FORECAST.ETS(A4,$B$2:$B$6,$A$2:$A$6,4,0,7)"),
            other => panic!("{other:?}"),
        }
        // Future forecast row (r=6, row=7)
        let c7 = b.cells.iter().find(|(r, c, _)| (*r, *c) == (6, 2)).unwrap();
        match &c7.2 {
            CellInput::Formula(f) => assert_eq!(f, "FORECAST.ETS(A7,$B$2:$B$6,$A$2:$A$6,4,0,7)"),
            other => panic!("{other:?}"),
        }
        // Confint formula
        let lower = b.cells.iter().find(|(r, c, _)| (*r, *c) == (6, 3)).unwrap();
        match &lower.2 {
            CellInput::Formula(f) => assert_eq!(f, "C7-FORECAST.ETS.CONFINT(A7,$B$2:$B$6,$A$2:$A$6,0.95,4,0,7)"),
            other => panic!("{other:?}"),
        }
        // Stat formula
        let stat = b.cells.iter().find(|(r, c, _)| (*r, *c) == (1, 7)).unwrap();
        match &stat.2 {
            CellInput::Formula(f) => assert_eq!(f, "FORECAST.ETS.STAT($B$2:$B$6,$A$2:$A$6,1,4,0,7)"),
            other => panic!("{other:?}"),
        }
    }
}
