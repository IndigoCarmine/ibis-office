// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > データ分析 (Excel's Analysis ToolPak): each tool reads numbers
//! from the sheet and gives back the table Excel writes as its output (the
//! cells to put at the output cell, and their number formats).
//!
//! Most tables are values; the moving average and the exponential
//! smoothing write formulas, as Excel does. The p-values and critical
//! values come from the engine's own functions (`T.INV.2T`, `F.DIST.RT`,
//! …), asked through [`Eval`]; the rest is computed here.
//!
//! The captions come through `Labels` (an id of the app's string table to
//! the text of the interface language), so this module has no strings of
//! its own.

use ibis_calc::{CellError, CellValue, number_to_xml};

use crate::model::Presentation;
use crate::sheet::{CellRange, Sheet};
use crate::sheet_calc::BookView;
use crate::sheet_command::CellInput;

/// An id of the string table to the text of the interface language.
pub type Labels<'a> = &'a dyn Fn(&str) -> String;

/// Every caption id the tools use (the app checks that each is in its
/// string tables).
pub const LABEL_IDS: &[&str] = &[
    "xlsx.analysis.out.column", "xlsx.analysis.out.row", "xlsx.analysis.out.x-variable", "xlsx.analysis.out.mean", "xlsx.analysis.out.standard-error", "xlsx.analysis.out.median", "xlsx.analysis.out.mode",
    "xlsx.analysis.out.standard-deviation", "xlsx.analysis.out.sample-variance", "xlsx.analysis.out.kurtosis", "xlsx.analysis.out.skewness", "xlsx.analysis.out.range", "xlsx.analysis.out.minimum",
    "xlsx.analysis.out.maximum", "xlsx.analysis.out.sum", "xlsx.analysis.out.count", "xlsx.analysis.out.largest", "xlsx.analysis.out.smallest", "xlsx.analysis.out.confidence-level", "xlsx.analysis.out.point",
    "xlsx.analysis.out.rank", "xlsx.analysis.out.percent", "xlsx.analysis.out.bin", "xlsx.analysis.out.frequency", "xlsx.analysis.out.more", "xlsx.analysis.out.cumulative", "xlsx.analysis.out.summary-output",
    "xlsx.analysis.out.regression-statistics", "xlsx.analysis.out.multiple-r", "xlsx.analysis.out.r-square", "xlsx.analysis.out.adjusted-r-square", "xlsx.analysis.out.observations",
    "xlsx.analysis.out.anova", "xlsx.analysis.out.df", "xlsx.analysis.out.ss", "xlsx.analysis.out.ms", "xlsx.analysis.out.f", "xlsx.analysis.out.significance-f", "xlsx.analysis.out.regression",
    "xlsx.analysis.out.residual", "xlsx.analysis.out.total", "xlsx.analysis.out.coefficients", "xlsx.analysis.out.t-stat", "xlsx.analysis.out.p-value", "xlsx.analysis.out.lower", "xlsx.analysis.out.upper",
    "xlsx.analysis.out.intercept", "xlsx.analysis.out.residual-output", "xlsx.analysis.out.observation", "xlsx.analysis.out.predicted-y", "xlsx.analysis.out.residuals",
    "xlsx.analysis.out.standard-residuals", "xlsx.analysis.out.t-paired", "xlsx.analysis.out.t-equal", "xlsx.analysis.out.t-unequal", "xlsx.analysis.out.z-test", "xlsx.analysis.out.f-test",
    "xlsx.analysis.out.variable-1", "xlsx.analysis.out.variable-2", "xlsx.analysis.out.variance", "xlsx.analysis.out.pearson", "xlsx.analysis.out.pooled-variance",
    "xlsx.analysis.out.hypothesized-diff", "xlsx.analysis.out.p-t-one", "xlsx.analysis.out.t-crit-one", "xlsx.analysis.out.p-t-two", "xlsx.analysis.out.t-crit-two",
    "xlsx.analysis.out.known-variance", "xlsx.analysis.out.z", "xlsx.analysis.out.p-z-one", "xlsx.analysis.out.z-crit-one", "xlsx.analysis.out.p-z-two", "xlsx.analysis.out.z-crit-two",
    "xlsx.analysis.out.p-f-one", "xlsx.analysis.out.f-crit-one", "xlsx.analysis.out.anova-single", "xlsx.analysis.out.anova-two-without", "xlsx.analysis.out.anova-two-with",
    "xlsx.analysis.out.summary", "xlsx.analysis.out.groups", "xlsx.analysis.out.sample-count", "xlsx.analysis.out.average", "xlsx.analysis.out.source", "xlsx.analysis.out.between",
    "xlsx.analysis.out.within", "xlsx.analysis.out.f-crit", "xlsx.analysis.out.rows", "xlsx.analysis.out.columns", "xlsx.analysis.out.error", "xlsx.analysis.out.sample", "xlsx.analysis.out.interaction",
    "xlsx.analysis.out.within-error",
];

type R = Result<f64, CellError>;

/// Asks the engine for a number (a distribution function with the
/// arguments written in).
pub struct Eval<'a>(BookView<'a>);

impl<'a> Eval<'a> {
    pub fn new(presentation: &'a Presentation) -> Self {
        Self(BookView::new(presentation))
    }

    /// The value of the formula (no `=`), as a number.
    pub fn call(&self, formula: &str) -> R {
        match ibis_calc::recalc::evaluate(&self.0, (0, 0, 0), formula).0 {
            CellValue::Number(n) => Ok(n),
            CellValue::Error(e) => Err(e),
            _ => Err(CellError::Value),
        }
    }

    fn t_inv_2t(&self, alpha: f64, df: f64) -> R {
        self.call(&format!("T.INV.2T({},{})", number_to_xml(alpha), number_to_xml(df)))
    }

    fn t_dist_rt(&self, t: f64, df: f64) -> R {
        self.call(&format!("T.DIST.RT({},{})", number_to_xml(t.abs()), number_to_xml(df)))
    }

    fn f_dist_rt(&self, f: f64, d1: f64, d2: f64) -> R {
        self.call(&format!("F.DIST.RT({},{},{})", number_to_xml(f), number_to_xml(d1), number_to_xml(d2)))
    }

    fn f_inv_rt(&self, p: f64, d1: f64, d2: f64) -> R {
        self.call(&format!("F.INV.RT({},{},{})", number_to_xml(p), number_to_xml(d1), number_to_xml(d2)))
    }

    fn f_dist(&self, f: f64, d1: f64, d2: f64) -> R {
        self.call(&format!("F.DIST({},{},{},TRUE)", number_to_xml(f), number_to_xml(d1), number_to_xml(d2)))
    }

    fn f_inv(&self, p: f64, d1: f64, d2: f64) -> R {
        self.call(&format!("F.INV({},{},{})", number_to_xml(p), number_to_xml(d1), number_to_xml(d2)))
    }

    fn norm_s_dist(&self, z: f64) -> R {
        self.call(&format!("NORM.S.DIST({},TRUE)", number_to_xml(z)))
    }

    fn norm_s_inv(&self, p: f64) -> R {
        self.call(&format!("NORM.S.INV({})", number_to_xml(p)))
    }
}

/// The numbers of the input range, one list per variable (a column of the
/// range, or a row if grouped by rows). Cells that are not numbers are
/// `None` and are skipped by the tools.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    pub names: Vec<String>,
    pub cols: Vec<Vec<Option<f64>>>,
    /// The first row (column) was taken as the names.
    pub labelled: bool,
}

impl Block {
    /// Read `range` of `sheet`. `by_rows`: each row is a variable. With
    /// `labels` the first row (column) holds the names; the others are
    /// named `<column caption> n`-less (`name_of(i)`).
    pub fn read(sheet: &Sheet, range: CellRange, by_rows: bool, labels: bool, name_of: &dyn Fn(usize) -> String) -> Block {
        let number = |r: u32, c: u32| match sheet.cell(r, c).map(|x| &x.value) {
            Some(CellValue::Number(n)) => Some(*n),
            _ => None,
        };
        let text = |r: u32, c: u32| match sheet.cell(r, c).map(|x| &x.value) {
            Some(CellValue::Text(t)) => Some(t.clone()),
            Some(CellValue::Number(n)) => Some(number_to_xml(*n)),
            _ => None,
        };
        let mut block = Block { labelled: labels, ..Block::default() };
        if by_rows {
            for (i, r) in (range.first_row..=range.last_row).enumerate() {
                let start = range.first_column + u32::from(labels);
                block.names.push(if labels { text(r, range.first_column).unwrap_or_else(|| name_of(i)) } else { name_of(i) });
                block.cols.push((start..=range.last_column).map(|c| number(r, c)).collect());
            }
        } else {
            for (i, c) in (range.first_column..=range.last_column).enumerate() {
                let start = range.first_row + u32::from(labels);
                block.names.push(if labels { text(range.first_row, c).unwrap_or_else(|| name_of(i)) } else { name_of(i) });
                block.cols.push((start..=range.last_row).map(|r| number(r, c)).collect());
            }
        }
        block
    }

    fn numbers(&self, i: usize) -> Vec<f64> {
        self.cols.get(i).map(|c| c.iter().flatten().copied().collect()).unwrap_or_default()
    }
}

/// What a tool writes: the cells (row, column relative to the output cell)
/// and the number formats of ranges (relative too).
#[derive(Clone, Debug, Default)]
pub struct Output {
    pub cells: Vec<(u32, u32, CellInput)>,
    pub formats: Vec<(CellRange, String)>,
}

impl Output {
    fn text(&mut self, r: u32, c: u32, s: impl Into<String>) {
        self.cells.push((r, c, CellInput::Value(CellValue::Text(s.into()))));
    }

    fn num(&mut self, r: u32, c: u32, v: f64) {
        self.cells.push((r, c, CellInput::Value(CellValue::Number(v))));
    }

    fn val(&mut self, r: u32, c: u32, v: R) {
        match v {
            Ok(n) => self.num(r, c, n),
            Err(e) => self.cells.push((r, c, CellInput::Value(CellValue::Error(e)))),
        }
    }

    fn err(&mut self, r: u32, c: u32, e: CellError) {
        self.cells.push((r, c, CellInput::Value(CellValue::Error(e))));
    }

    fn formula(&mut self, r: u32, c: u32, f: String) {
        self.cells.push((r, c, CellInput::Formula(f)));
    }

    fn format(&mut self, first_row: u32, first_column: u32, last_row: u32, last_column: u32, code: &str) {
        self.formats.push((CellRange { first_row, first_column, last_row, last_column }, code.to_string()));
    }

    /// Rows and columns the output reaches.
    pub fn size(&self) -> (u32, u32) {
        self.cells.iter().fold((0, 0), |(r, c), (row, column, _)| (r.max(row + 1), c.max(column + 1)))
    }
}

// ---------------------------------------------------------------- statistics

fn mean(v: &[f64]) -> R {
    if v.is_empty() { Err(CellError::Div0) } else { Ok(v.iter().sum::<f64>() / v.len() as f64) }
}

/// Sample variance (n - 1).
fn variance(v: &[f64]) -> R {
    if v.len() < 2 {
        return Err(CellError::Div0);
    }
    let m = mean(v)?;
    Ok(v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (v.len() - 1) as f64)
}

fn skew(v: &[f64]) -> R {
    let n = v.len() as f64;
    if v.len() < 3 {
        return Err(CellError::Div0);
    }
    let (m, s) = (mean(v)?, variance(v)?.sqrt());
    if s == 0.0 {
        return Err(CellError::Div0);
    }
    Ok(n / ((n - 1.0) * (n - 2.0)) * v.iter().map(|x| ((x - m) / s).powi(3)).sum::<f64>())
}

fn kurt(v: &[f64]) -> R {
    let n = v.len() as f64;
    if v.len() < 4 {
        return Err(CellError::Div0);
    }
    let (m, s) = (mean(v)?, variance(v)?.sqrt());
    if s == 0.0 {
        return Err(CellError::Div0);
    }
    let sum = v.iter().map(|x| ((x - m) / s).powi(4)).sum::<f64>();
    Ok(n * (n + 1.0) / ((n - 1.0) * (n - 2.0) * (n - 3.0)) * sum - 3.0 * (n - 1.0) * (n - 1.0) / ((n - 2.0) * (n - 3.0)))
}

fn median(v: &[f64]) -> R {
    if v.is_empty() {
        return Err(CellError::Num);
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    Ok(if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 })
}

/// The value that occurs most often (the first of them in the data), or
/// `#N/A` if none occurs twice.
fn mode(v: &[f64]) -> R {
    let mut best: Option<(usize, f64)> = None;
    for &x in v {
        let count = v.iter().filter(|&&y| y == x).count();
        if count > 1 && best.is_none_or(|(c, _)| count > c) {
            best = Some((count, x));
        }
    }
    best.map(|(_, x)| x).ok_or(CellError::NA)
}

/// The k-th largest (`largest`) or smallest.
fn kth(v: &[f64], k: usize, largest: bool) -> R {
    if k == 0 || k > v.len() {
        return Err(CellError::Num);
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    Ok(if largest { s[s.len() - k] } else { s[k - 1] })
}

fn pairs(a: &[Option<f64>], b: &[Option<f64>]) -> Vec<(f64, f64)> {
    a.iter().zip(b).filter_map(|(x, y)| Some(((*x)?, (*y)?))).collect()
}

fn covariance_of(p: &[(f64, f64)]) -> R {
    if p.is_empty() {
        return Err(CellError::Div0);
    }
    let n = p.len() as f64;
    let (mx, my) = (p.iter().map(|x| x.0).sum::<f64>() / n, p.iter().map(|x| x.1).sum::<f64>() / n);
    Ok(p.iter().map(|(x, y)| (x - mx) * (y - my)).sum::<f64>() / n)
}

fn correlation_of(p: &[(f64, f64)]) -> R {
    let cov = covariance_of(p)?;
    let n = p.len() as f64;
    let (mx, my) = (p.iter().map(|x| x.0).sum::<f64>() / n, p.iter().map(|x| x.1).sum::<f64>() / n);
    let sx = (p.iter().map(|(x, _)| (x - mx) * (x - mx)).sum::<f64>() / n).sqrt();
    let sy = (p.iter().map(|(_, y)| (y - my) * (y - my)).sum::<f64>() / n).sqrt();
    if sx == 0.0 || sy == 0.0 { Err(CellError::Div0) } else { Ok(cov / (sx * sy)) }
}

fn variable_name(l: Labels, caption: &str, i: usize, spaced: bool) -> String {
    format!("{}{}{}", l(caption), if spaced { " " } else { "" }, i + 1)
}

/// `95.0%`.
fn percent_label(percent: f64) -> String {
    format!("{percent:.1}%")
}

// ------------------------------------------------------------------ tools

/// 基本統計量.
#[derive(Clone, Debug, PartialEq)]
pub struct Descriptive {
    /// The confidence level for the mean, in percent (95).
    pub confidence: Option<f64>,
    pub kth_largest: Option<usize>,
    pub kth_smallest: Option<usize>,
}

pub fn descriptive(inp: &Block, o: &Descriptive, l: Labels, ev: &Eval) -> Output {
    let mut out = Output::default();
    for (j, name) in inp.names.iter().enumerate() {
        let c = 2 * j as u32;
        let v = inp.numbers(j);
        out.text(0, c, name.clone());
        let n = v.len() as f64;
        let sd = variance(&v).map(f64::sqrt);
        let mut rows: Vec<(String, R)> = vec![
            (l("xlsx.analysis.out.mean"), mean(&v)),
            (l("xlsx.analysis.out.standard-error"), sd.map(|s| s / n.sqrt())),
            (l("xlsx.analysis.out.median"), median(&v)),
            (l("xlsx.analysis.out.mode"), mode(&v)),
            (l("xlsx.analysis.out.standard-deviation"), sd),
            (l("xlsx.analysis.out.sample-variance"), variance(&v)),
            (l("xlsx.analysis.out.kurtosis"), kurt(&v)),
            (l("xlsx.analysis.out.skewness"), skew(&v)),
            (l("xlsx.analysis.out.range"), kth(&v, 1, true).and_then(|hi| kth(&v, 1, false).map(|lo| hi - lo))),
            (l("xlsx.analysis.out.minimum"), kth(&v, 1, false)),
            (l("xlsx.analysis.out.maximum"), kth(&v, 1, true)),
            (l("xlsx.analysis.out.sum"), Ok(v.iter().sum())),
            (l("xlsx.analysis.out.count"), Ok(n)),
        ];
        if let Some(k) = o.kth_largest {
            rows.push((format!("{}({k})", l("xlsx.analysis.out.largest")), kth(&v, k, true)));
        }
        if let Some(k) = o.kth_smallest {
            rows.push((format!("{}({k})", l("xlsx.analysis.out.smallest")), kth(&v, k, false)));
        }
        if let Some(percent) = o.confidence {
            let alpha = 1.0 - percent / 100.0;
            let half = ev.t_inv_2t(alpha, n - 1.0).and_then(|t| sd.map(|s| t * s / n.sqrt()));
            rows.push((format!("{}({})", l("xlsx.analysis.out.confidence-level"), percent_label(percent)), half));
        }
        for (i, (label, value)) in rows.into_iter().enumerate() {
            out.text(i as u32 + 1, c, label);
            out.val(i as u32 + 1, c + 1, value);
        }
    }
    out
}

/// 相関 (`population` false) and 共分散 (true): the lower triangle of the
/// matrix of the variables.
pub fn correlation_or_covariance(inp: &Block, covariance: bool) -> Output {
    let mut out = Output::default();
    for (i, name) in inp.names.iter().enumerate() {
        out.text(0, i as u32 + 1, name.clone());
        out.text(i as u32 + 1, 0, name.clone());
        for j in 0..=i {
            let p = pairs(&inp.cols[i], &inp.cols[j]);
            let v = if covariance { covariance_of(&p) } else if i == j { Ok(1.0) } else { correlation_of(&p) };
            out.val(i as u32 + 1, j as u32 + 1, v);
        }
    }
    out
}

/// 順位と百分位.
pub fn rank_percentile(inp: &Block, l: Labels) -> Output {
    let mut out = Output::default();
    for (j, name) in inp.names.iter().enumerate() {
        let c = 4 * j as u32;
        out.text(0, c, l("xlsx.analysis.out.point"));
        out.text(0, c + 1, name.clone());
        out.text(0, c + 2, l("xlsx.analysis.out.rank"));
        out.text(0, c + 3, l("xlsx.analysis.out.percent"));
        let mut items: Vec<(usize, f64)> = inp.cols[j].iter().enumerate().filter_map(|(i, v)| Some((i + 1, (*v)?))).collect();
        items.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let n = items.len();
        for (k, (point, value)) in items.iter().enumerate() {
            let r = k as u32 + 1;
            let rank = 1 + items.iter().filter(|x| x.1 > *value).count();
            let below = items.iter().filter(|x| x.1 < *value).count();
            out.num(r, c, *point as f64);
            out.num(r, c + 1, *value);
            out.num(r, c + 2, rank as f64);
            out.val(r, c + 3, if n > 1 { Ok(below as f64 / (n - 1) as f64) } else { Err(CellError::Div0) });
        }
        if n > 0 {
            out.format(1, c + 3, n as u32, c + 3, "0.00%");
        }
    }
    out
}

/// ヒストグラム.
#[derive(Clone, Debug, PartialEq)]
pub struct Histogram {
    /// The bin edges; none: evenly spaced between the least and the greatest.
    pub bins: Option<Vec<f64>>,
    pub pareto: bool,
    pub cumulative: bool,
}

pub fn histogram(data: &[f64], o: &Histogram, l: Labels) -> Output {
    let mut out = Output::default();
    let mut edges = match &o.bins {
        Some(b) => b.clone(),
        None if data.is_empty() => Vec::new(),
        None => {
            let (lo, hi) = (kth(data, 1, false).unwrap_or(0.0), kth(data, 1, true).unwrap_or(0.0));
            let count = (data.len() as f64).sqrt().ceil().max(1.0) as usize;
            let width = (hi - lo) / count as f64;
            (0..=count).map(|i| if i == count { hi } else { lo + width * i as f64 }).collect()
        }
    };
    edges.sort_by(f64::total_cmp);
    let mut counts: Vec<f64> = Vec::new();
    let mut previous = f64::NEG_INFINITY;
    for &edge in &edges {
        counts.push(data.iter().filter(|&&x| x > previous && x <= edge).count() as f64);
        previous = edge;
    }
    counts.push(data.iter().filter(|&&x| x > previous).count() as f64);
    let total = data.len() as f64;
    let table = |out: &mut Output, c0: u32, order: &[usize]| {
        out.text(0, c0, l("xlsx.analysis.out.bin"));
        out.text(0, c0 + 1, l("xlsx.analysis.out.frequency"));
        if o.cumulative || c0 > 0 {
            out.text(0, c0 + 2, l("xlsx.analysis.out.cumulative"));
        }
        let mut running = 0.0;
        for (k, &i) in order.iter().enumerate() {
            let r = k as u32 + 1;
            if i < edges.len() {
                out.num(r, c0, edges[i]);
            } else {
                out.text(r, c0, l("xlsx.analysis.out.more"));
            }
            out.num(r, c0 + 1, counts[i]);
            running += counts[i];
            if o.cumulative || c0 > 0 {
                out.val(r, c0 + 2, if total > 0.0 { Ok(running / total) } else { Err(CellError::Div0) });
            }
        }
        if o.cumulative || c0 > 0 {
            out.format(1, c0 + 2, order.len() as u32, c0 + 2, "0.00%");
        }
    };
    let natural: Vec<usize> = (0..counts.len()).collect();
    table(&mut out, 0, &natural);
    if o.pareto {
        let mut sorted = natural.clone();
        sorted.sort_by(|&a, &b| counts[b].total_cmp(&counts[a]).then(a.cmp(&b)));
        let c0 = if o.cumulative { 4 } else { 3 };
        table(&mut out, c0, &sorted);
    }
    out
}

/// 回帰分析.
#[derive(Clone, Debug, PartialEq)]
pub struct Regression {
    /// Fit a constant (the intercept); false: the line goes through 0.
    pub constant: bool,
    /// The confidence level, in percent (95).
    pub confidence: f64,
    pub residuals: bool,
    pub standard_residuals: bool,
}

/// Solve `a x = b` (and give the inverse of `a`) by Gauss-Jordan; `None`
/// if singular.
fn invert(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    let mut m: Vec<Vec<f64>> = a.iter().enumerate().map(|(i, row)| row.iter().copied().chain((0..n).map(|j| f64::from(i == j))).collect()).collect();
    for col in 0..n {
        let pivot = (col..n).max_by(|&x, &y| m[x][col].abs().total_cmp(&m[y][col].abs()))?;
        if m[pivot][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, pivot);
        let d = m[col][col];
        for v in &mut m[col] {
            *v /= d;
        }
        for r in 0..n {
            if r != col {
                let f = m[r][col];
                if f != 0.0 {
                    let pivot_row = m[col].clone();
                    for (x, p) in m[r].iter_mut().zip(&pivot_row) {
                        *x -= f * p;
                    }
                }
            }
        }
    }
    Some(m.into_iter().map(|row| row[n..].to_vec()).collect())
}

/// `y` against the variables of `xs`; the rows with a missing value are
/// left out. `Err` when the variables are dependent or the data too few.
pub fn regression(y: &[Option<f64>], xs: &Block, o: &Regression, l: Labels, ev: &Eval) -> Result<Output, CellError> {
    let k = xs.cols.len();
    let rows: Vec<(f64, Vec<f64>)> = (0..y.len())
        .filter_map(|i| {
            let mut x = Vec::with_capacity(k);
            for col in &xs.cols {
                x.push((*col.get(i)?)?);
            }
            Some(((y[i])?, x))
        })
        .collect();
    let n = rows.len();
    let p = k + usize::from(o.constant);
    if n <= p || k == 0 {
        return Err(CellError::Num);
    }
    let design = |x: &Vec<f64>| -> Vec<f64> { if o.constant { std::iter::once(1.0).chain(x.iter().copied()).collect() } else { x.clone() } };
    let mut xtx = vec![vec![0.0; p]; p];
    let mut xty = vec![0.0; p];
    for (yv, x) in &rows {
        let d = design(x);
        for i in 0..p {
            xty[i] += d[i] * yv;
            for j in 0..p {
                xtx[i][j] += d[i] * d[j];
            }
        }
    }
    let inverse = invert(&xtx).ok_or(CellError::Num)?;
    let beta: Vec<f64> = (0..p).map(|i| (0..p).map(|j| inverse[i][j] * xty[j]).sum()).collect();
    let fitted: Vec<f64> = rows.iter().map(|(_, x)| design(x).iter().zip(&beta).map(|(a, b)| a * b).sum()).collect();
    let sse: f64 = rows.iter().zip(&fitted).map(|((yv, _), f)| (yv - f) * (yv - f)).sum();
    let ymean = rows.iter().map(|r| r.0).sum::<f64>() / n as f64;
    let sst: f64 = if o.constant { rows.iter().map(|r| (r.0 - ymean) * (r.0 - ymean)).sum() } else { rows.iter().map(|r| r.0 * r.0).sum() };
    let df_res = (n - p) as f64;
    let df_reg = k as f64;
    let r2 = if sst == 0.0 { 1.0 } else { 1.0 - sse / sst };
    let adjusted = 1.0 - (1.0 - r2) * (if o.constant { n as f64 - 1.0 } else { n as f64 }) / df_res;
    let mse = sse / df_res;
    let ssr = sst - sse;
    let f_stat = (ssr / df_reg) / mse;

    let mut out = Output::default();
    out.text(0, 0, l("xlsx.analysis.out.summary-output"));
    out.text(2, 0, l("xlsx.analysis.out.regression-statistics"));
    out.text(3, 0, l("xlsx.analysis.out.multiple-r"));
    out.num(3, 1, r2.max(0.0).sqrt());
    out.text(4, 0, l("xlsx.analysis.out.r-square"));
    out.num(4, 1, r2);
    out.text(5, 0, l("xlsx.analysis.out.adjusted-r-square"));
    out.num(5, 1, adjusted);
    out.text(6, 0, l("xlsx.analysis.out.standard-error"));
    out.num(6, 1, mse.sqrt());
    out.text(7, 0, l("xlsx.analysis.out.observations"));
    out.num(7, 1, n as f64);
    out.text(9, 0, l("xlsx.analysis.out.anova"));
    for (c, id) in ["xlsx.analysis.out.df", "xlsx.analysis.out.ss", "xlsx.analysis.out.ms", "xlsx.analysis.out.f", "xlsx.analysis.out.significance-f"].iter().enumerate() {
        out.text(10, c as u32 + 1, l(id));
    }
    out.text(11, 0, l("xlsx.analysis.out.regression"));
    out.num(11, 1, df_reg);
    out.num(11, 2, ssr);
    out.num(11, 3, ssr / df_reg);
    out.num(11, 4, f_stat);
    out.val(11, 5, ev.f_dist_rt(f_stat, df_reg, df_res));
    out.text(12, 0, l("xlsx.analysis.out.residual"));
    out.num(12, 1, df_res);
    out.num(12, 2, sse);
    out.num(12, 3, mse);
    out.text(13, 0, l("xlsx.analysis.out.total"));
    out.num(13, 1, df_reg + df_res);
    out.num(13, 2, sst);
    out.text(15, 1, l("xlsx.analysis.out.coefficients"));
    out.text(15, 2, l("xlsx.analysis.out.standard-error"));
    out.text(15, 3, l("xlsx.analysis.out.t-stat"));
    out.text(15, 4, l("xlsx.analysis.out.p-value"));
    let ci = percent_label(o.confidence);
    out.text(15, 5, format!("{} 95%", l("xlsx.analysis.out.lower")));
    out.text(15, 6, format!("{} 95%", l("xlsx.analysis.out.upper")));
    out.text(15, 7, format!("{} {ci}", l("xlsx.analysis.out.lower")));
    out.text(15, 8, format!("{} {ci}", l("xlsx.analysis.out.upper")));
    let t95 = ev.t_inv_2t(0.05, df_res);
    let tci = ev.t_inv_2t(1.0 - o.confidence / 100.0, df_res);
    for i in 0..p {
        let r = 16 + i as u32;
        let name = if o.constant && i == 0 {
            l("xlsx.analysis.out.intercept")
        } else {
            let index = i - usize::from(o.constant);
            if xs.labelled { xs.names[index].clone() } else { variable_name(l, "xlsx.analysis.out.x-variable", index, true) }
        };
        out.text(r, 0, name);
        let se = (mse * inverse[i][i]).sqrt();
        let t = beta[i] / se;
        out.num(r, 1, beta[i]);
        out.num(r, 2, se);
        out.num(r, 3, t);
        out.val(r, 4, ev.t_dist_rt(t, df_res).map(|x| x * 2.0));
        out.val(r, 5, t95.map(|c| beta[i] - c * se));
        out.val(r, 6, t95.map(|c| beta[i] + c * se));
        out.val(r, 7, tci.map(|c| beta[i] - c * se));
        out.val(r, 8, tci.map(|c| beta[i] + c * se));
    }
    if o.residuals || o.standard_residuals {
        let top = 16 + p as u32 + 1;
        out.text(top, 0, l("xlsx.analysis.out.residual-output"));
        out.text(top + 2, 0, l("xlsx.analysis.out.observation"));
        out.text(top + 2, 1, l("xlsx.analysis.out.predicted-y"));
        out.text(top + 2, 2, l("xlsx.analysis.out.residuals"));
        if o.standard_residuals {
            out.text(top + 2, 3, l("xlsx.analysis.out.standard-residuals"));
        }
        let sd = (sse / (n as f64 - 1.0)).sqrt();
        for (i, ((yv, _), f)) in rows.iter().zip(&fitted).enumerate() {
            let r = top + 3 + i as u32;
            out.num(r, 0, i as f64 + 1.0);
            out.num(r, 1, *f);
            out.num(r, 2, yv - f);
            if o.standard_residuals {
                out.num(r, 3, (yv - f) / sd);
            }
        }
    }
    Ok(out)
}

/// The kind of two-sample mean test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TTest {
    Paired,
    EqualVariance,
    UnequalVariance,
}

/// t 検定 (three kinds), z 検定 and F 検定 share the layout: a title, the
/// two variables' columns and the rows of statistics.
fn two_sample_head(out: &mut Output, title: String, a: &str, b: &str) {
    out.text(0, 0, title);
    out.text(1, 1, a.to_string());
    out.text(1, 2, b.to_string());
}

pub fn t_test(a: &[Option<f64>], b: &[Option<f64>], names: (&str, &str), kind: TTest, hypothesized: f64, alpha: f64, l: Labels, ev: &Eval) -> Output {
    let mut out = Output::default();
    let title = match kind {
        TTest::Paired => l("xlsx.analysis.out.t-paired"),
        TTest::EqualVariance => l("xlsx.analysis.out.t-equal"),
        TTest::UnequalVariance => l("xlsx.analysis.out.t-unequal"),
    };
    two_sample_head(&mut out, title, names.0, names.1);
    let (x, y): (Vec<f64>, Vec<f64>) = match kind {
        TTest::Paired => pairs(a, b).into_iter().unzip(),
        _ => (a.iter().flatten().copied().collect(), b.iter().flatten().copied().collect()),
    };
    let (n1, n2) = (x.len() as f64, y.len() as f64);
    let (m1, m2) = (mean(&x), mean(&y));
    let (v1, v2) = (variance(&x), variance(&y));
    let mut r = 2;
    let mut row = |out: &mut Output, label: String, v1: Option<R>, v2: Option<R>| {
        out.text(r, 0, label);
        if let Some(v) = v1 {
            out.val(r, 1, v);
        }
        if let Some(v) = v2 {
            out.val(r, 2, v);
        }
        r += 1;
    };
    row(&mut out, l("xlsx.analysis.out.mean"), Some(m1), Some(m2));
    row(&mut out, l("xlsx.analysis.out.variance"), Some(v1), Some(v2));
    row(&mut out, l("xlsx.analysis.out.observations"), Some(Ok(n1)), Some(Ok(n2)));
    let (df, t) = match kind {
        TTest::Paired => {
            let pr = pairs(a, b);
            row(&mut out, l("xlsx.analysis.out.pearson"), Some(correlation_of(&pr)), None);
            row(&mut out, l("xlsx.analysis.out.hypothesized-diff"), Some(Ok(hypothesized)), None);
            let d: Vec<f64> = pr.iter().map(|(p, q)| p - q).collect();
            let df = d.len() as f64 - 1.0;
            let t = mean(&d).and_then(|m| variance(&d).map(|v| (m - hypothesized) / (v / d.len() as f64).sqrt()));
            (Ok(df), t)
        }
        TTest::EqualVariance => {
            let pooled = match (v1, v2) {
                (Ok(a), Ok(b)) => Ok(((n1 - 1.0) * a + (n2 - 1.0) * b) / (n1 + n2 - 2.0)),
                (Err(e), _) | (_, Err(e)) => Err(e),
            };
            row(&mut out, l("xlsx.analysis.out.pooled-variance"), Some(pooled), None);
            row(&mut out, l("xlsx.analysis.out.hypothesized-diff"), Some(Ok(hypothesized)), None);
            let t = match (m1, m2, pooled) {
                (Ok(a), Ok(b), Ok(p)) => Ok((a - b - hypothesized) / (p * (1.0 / n1 + 1.0 / n2)).sqrt()),
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => Err(e),
            };
            (Ok(n1 + n2 - 2.0), t)
        }
        TTest::UnequalVariance => {
            row(&mut out, l("xlsx.analysis.out.hypothesized-diff"), Some(Ok(hypothesized)), None);
            match (m1, m2, v1, v2) {
                (Ok(a), Ok(b), Ok(p), Ok(q)) => {
                    let (s1, s2) = (p / n1, q / n2);
                    let df = ((s1 + s2) * (s1 + s2) / (s1 * s1 / (n1 - 1.0) + s2 * s2 / (n2 - 1.0))).round();
                    (Ok(df), Ok((a - b - hypothesized) / (s1 + s2).sqrt()))
                }
                (Err(e), ..) | (_, Err(e), ..) | (.., Err(e), _) | (.., Err(e)) => (Err(e), Err(e)),
            }
        }
    };
    row(&mut out, l("xlsx.analysis.out.df"), Some(df), None);
    row(&mut out, l("xlsx.analysis.out.t-stat"), Some(t), None);
    let (p1, c1, p2, c2) = match (df, t) {
        (Ok(df), Ok(t)) => (ev.t_dist_rt(t, df), ev.t_inv_2t(2.0 * alpha, df), ev.t_dist_rt(t, df).map(|p| p * 2.0), ev.t_inv_2t(alpha, df)),
        (Err(e), _) | (_, Err(e)) => (Err(e), Err(e), Err(e), Err(e)),
    };
    row(&mut out, l("xlsx.analysis.out.p-t-one"), Some(p1), None);
    row(&mut out, l("xlsx.analysis.out.t-crit-one"), Some(c1), None);
    row(&mut out, l("xlsx.analysis.out.p-t-two"), Some(p2), None);
    row(&mut out, l("xlsx.analysis.out.t-crit-two"), Some(c2), None);
    out
}

/// z 検定: two samples with known variances.
pub fn z_test(a: &[Option<f64>], b: &[Option<f64>], names: (&str, &str), variances: (f64, f64), hypothesized: f64, alpha: f64, l: Labels, ev: &Eval) -> Output {
    let mut out = Output::default();
    two_sample_head(&mut out, l("xlsx.analysis.out.z-test"), names.0, names.1);
    let x: Vec<f64> = a.iter().flatten().copied().collect();
    let y: Vec<f64> = b.iter().flatten().copied().collect();
    let (n1, n2) = (x.len() as f64, y.len() as f64);
    let (m1, m2) = (mean(&x), mean(&y));
    out.text(2, 0, l("xlsx.analysis.out.mean"));
    out.val(2, 1, m1);
    out.val(2, 2, m2);
    out.text(3, 0, l("xlsx.analysis.out.known-variance"));
    out.num(3, 1, variances.0);
    out.num(3, 2, variances.1);
    out.text(4, 0, l("xlsx.analysis.out.observations"));
    out.num(4, 1, n1);
    out.num(4, 2, n2);
    out.text(5, 0, l("xlsx.analysis.out.hypothesized-diff"));
    out.num(5, 1, hypothesized);
    let z = match (m1, m2) {
        (Ok(p), Ok(q)) => Ok((p - q - hypothesized) / (variances.0 / n1 + variances.1 / n2).sqrt()),
        (Err(e), _) | (_, Err(e)) => Err(e),
    };
    out.text(6, 0, l("xlsx.analysis.out.z"));
    out.val(6, 1, z);
    let p_one = z.and_then(|z| ev.norm_s_dist(-z.abs()));
    out.text(7, 0, l("xlsx.analysis.out.p-z-one"));
    out.val(7, 1, p_one);
    out.text(8, 0, l("xlsx.analysis.out.z-crit-one"));
    out.val(8, 1, ev.norm_s_inv(1.0 - alpha));
    out.text(9, 0, l("xlsx.analysis.out.p-z-two"));
    out.val(9, 1, p_one.map(|p| p * 2.0));
    out.text(10, 0, l("xlsx.analysis.out.z-crit-two"));
    out.val(10, 1, ev.norm_s_inv(1.0 - alpha / 2.0));
    out
}

/// F 検定: two samples' variances.
pub fn f_test(a: &[Option<f64>], b: &[Option<f64>], names: (&str, &str), alpha: f64, l: Labels, ev: &Eval) -> Output {
    let mut out = Output::default();
    two_sample_head(&mut out, l("xlsx.analysis.out.f-test"), names.0, names.1);
    let x: Vec<f64> = a.iter().flatten().copied().collect();
    let y: Vec<f64> = b.iter().flatten().copied().collect();
    let (n1, n2) = (x.len() as f64, y.len() as f64);
    let (v1, v2) = (variance(&x), variance(&y));
    out.text(2, 0, l("xlsx.analysis.out.mean"));
    out.val(2, 1, mean(&x));
    out.val(2, 2, mean(&y));
    out.text(3, 0, l("xlsx.analysis.out.variance"));
    out.val(3, 1, v1);
    out.val(3, 2, v2);
    out.text(4, 0, l("xlsx.analysis.out.observations"));
    out.num(4, 1, n1);
    out.num(4, 2, n2);
    out.text(5, 0, l("xlsx.analysis.out.df"));
    out.num(5, 1, n1 - 1.0);
    out.num(5, 2, n2 - 1.0);
    let f = match (v1, v2) {
        (Ok(p), Ok(q)) if q != 0.0 => Ok(p / q),
        (Err(e), _) | (_, Err(e)) => Err(e),
        _ => Err(CellError::Div0),
    };
    out.text(6, 0, l("xlsx.analysis.out.f"));
    out.val(6, 1, f);
    let (d1, d2) = (n1 - 1.0, n2 - 1.0);
    let (p, crit) = match f {
        Ok(f) if f < 1.0 => (ev.f_dist(f, d1, d2), ev.f_inv(alpha, d1, d2)),
        Ok(f) => (ev.f_dist_rt(f, d1, d2), ev.f_inv_rt(alpha, d1, d2)),
        Err(e) => (Err(e), Err(e)),
    };
    out.text(7, 0, l("xlsx.analysis.out.p-f-one"));
    out.val(7, 1, p);
    out.text(8, 0, l("xlsx.analysis.out.f-crit-one"));
    out.val(8, 1, crit);
    out
}

fn summary_header(out: &mut Output, r: u32, first: String, l: Labels, count_id: &str) {
    out.text(r, 0, first);
    out.text(r, 1, l(count_id));
    out.text(r, 2, l("xlsx.analysis.out.sum"));
    out.text(r, 3, l("xlsx.analysis.out.average"));
    out.text(r, 4, l("xlsx.analysis.out.variance"));
}

fn summary_row(out: &mut Output, r: u32, name: String, v: &[f64]) {
    out.text(r, 0, name);
    out.num(r, 1, v.len() as f64);
    out.num(r, 2, v.iter().sum());
    out.val(r, 3, mean(v));
    out.val(r, 4, variance(v));
}

/// 分散分析: 一元配置. Each variable of `inp` is a group; `names` are the
/// group captions.
pub fn anova_single(inp: &Block, alpha: f64, l: Labels, ev: &Eval) -> Output {
    let mut out = Output::default();
    out.text(0, 0, l("xlsx.analysis.out.anova-single"));
    out.text(2, 0, l("xlsx.analysis.out.summary"));
    summary_header(&mut out, 3, l("xlsx.analysis.out.groups"), l, "xlsx.analysis.out.sample-count");
    let groups: Vec<Vec<f64>> = (0..inp.cols.len()).map(|i| inp.numbers(i)).collect();
    for (i, g) in groups.iter().enumerate() {
        summary_row(&mut out, 4 + i as u32, inp.names[i].clone(), g);
    }
    let all: Vec<f64> = groups.iter().flatten().copied().collect();
    let grand = mean(&all).unwrap_or(0.0);
    let k = groups.len() as f64;
    let total_n = all.len() as f64;
    let ssb: f64 = groups.iter().map(|g| g.len() as f64 * (mean(g).unwrap_or(grand) - grand).powi(2)).sum();
    let ssw: f64 = groups.iter().map(|g| g.iter().map(|x| (x - mean(g).unwrap_or(0.0)).powi(2)).sum::<f64>()).sum();
    let (dfb, dfw) = (k - 1.0, total_n - k);
    let top = 4 + groups.len() as u32 + 1;
    out.text(top, 0, l("xlsx.analysis.out.anova"));
    anova_header(&mut out, top + 1, l);
    anova_row(&mut out, top + 2, l("xlsx.analysis.out.between"), ssb, dfb);
    anova_f(&mut out, top + 2, ssb, dfb, ssw / dfw, dfw, alpha, ev);
    anova_row(&mut out, top + 3, l("xlsx.analysis.out.within"), ssw, dfw);
    anova_row(&mut out, top + 5, l("xlsx.analysis.out.total"), ssb + ssw, dfb + dfw);
    out
}

fn anova_header(out: &mut Output, r: u32, l: Labels) {
    for (c, id) in ["xlsx.analysis.out.source", "xlsx.analysis.out.ss", "xlsx.analysis.out.df", "xlsx.analysis.out.ms", "xlsx.analysis.out.f", "xlsx.analysis.out.p-value", "xlsx.analysis.out.f-crit"].iter().enumerate() {
        out.text(r, c as u32, l(id));
    }
}

/// A row of the ANOVA table: the source, SS, df and MS.
fn anova_row(out: &mut Output, r: u32, label: String, ss: f64, df: f64) {
    out.text(r, 0, label);
    out.num(r, 1, ss);
    out.num(r, 2, df);
    out.val(r, 3, if df > 0.0 { Ok(ss / df) } else { Err(CellError::Div0) });
}

/// The F ratio of the row at `r` (SS `ss`, df `df`) against the error's
/// mean square, with its p-value and the critical value.
fn anova_f(out: &mut Output, r: u32, ss: f64, df: f64, error_ms: f64, error_df: f64, alpha: f64, ev: &Eval) {
    let f = (ss / df) / error_ms;
    if !f.is_finite() {
        for c in 4..7 {
            out.err(r, c, CellError::Div0);
        }
        return;
    }
    out.num(r, 4, f);
    out.val(r, 5, ev.f_dist_rt(f, df, error_df));
    out.val(r, 6, ev.f_inv_rt(alpha, df, error_df));
}

/// 分散分析: 繰り返しのない二元配置. `inp` holds the values of the rows
/// as variables (`labelled`: names of the rows are in `names`; the column
/// names in `column_names`).
pub fn anova_two_without(rows: &[Vec<f64>], row_names: &[String], column_names: &[String], alpha: f64, l: Labels, ev: &Eval) -> Output {
    let mut out = Output::default();
    out.text(0, 0, l("xlsx.analysis.out.anova-two-without"));
    out.text(2, 0, l("xlsx.analysis.out.summary"));
    out.text(2, 1, l("xlsx.analysis.out.sample-count"));
    out.text(2, 2, l("xlsx.analysis.out.sum"));
    out.text(2, 3, l("xlsx.analysis.out.average"));
    out.text(2, 4, l("xlsx.analysis.out.variance"));
    let r = rows.len();
    let c = rows.first().map_or(0, Vec::len);
    for (i, row) in rows.iter().enumerate() {
        summary_row(&mut out, 3 + i as u32, row_names[i].clone(), row);
    }
    let columns: Vec<Vec<f64>> = (0..c).map(|j| rows.iter().map(|row| row[j]).collect()).collect();
    for (j, col) in columns.iter().enumerate() {
        summary_row(&mut out, 3 + r as u32 + j as u32, column_names[j].clone(), col);
    }
    let all: Vec<f64> = rows.iter().flatten().copied().collect();
    let grand = mean(&all).unwrap_or(0.0);
    let ssr: f64 = rows.iter().map(|row| c as f64 * (mean(row).unwrap_or(grand) - grand).powi(2)).sum();
    let ssc: f64 = columns.iter().map(|col| r as f64 * (mean(col).unwrap_or(grand) - grand).powi(2)).sum();
    let sst: f64 = all.iter().map(|x| (x - grand).powi(2)).sum();
    let sse = sst - ssr - ssc;
    let (dfr, dfc, dfe) = (r as f64 - 1.0, c as f64 - 1.0, (r as f64 - 1.0) * (c as f64 - 1.0));
    let mse = sse / dfe;
    let top = 3 + r as u32 + c as u32 + 1;
    out.text(top, 0, l("xlsx.analysis.out.anova"));
    anova_header(&mut out, top + 1, l);
    for (k, (label, ss, df)) in [(l("xlsx.analysis.out.rows"), ssr, dfr), (l("xlsx.analysis.out.columns"), ssc, dfc)].into_iter().enumerate() {
        let row = top + 2 + k as u32;
        anova_row(&mut out, row, label, ss, df);
        anova_f(&mut out, row, ss, df, mse, dfe, alpha, ev);
    }
    anova_row(&mut out, top + 4, l("xlsx.analysis.out.error"), sse, dfe);
    anova_row(&mut out, top + 6, l("xlsx.analysis.out.total"), sst, dfr + dfc + dfe);
    out
}

/// 分散分析: 繰り返しのある二元配置. `groups[g][j]` are the `m` values of
/// the sample (row group) `g` in column `j`.
pub fn anova_two_with(groups: &[Vec<Vec<f64>>], group_names: &[String], column_names: &[String], alpha: f64, l: Labels, ev: &Eval) -> Output {
    let mut out = Output::default();
    out.text(0, 0, l("xlsx.analysis.out.anova-two-with"));
    out.text(2, 0, l("xlsx.analysis.out.summary"));
    let c = column_names.len();
    for (j, name) in column_names.iter().enumerate() {
        out.text(2, j as u32 + 1, name.clone());
    }
    out.text(2, c as u32 + 1, l("xlsx.analysis.out.total"));
    let stat_rows = [("xlsx.analysis.out.sample-count", 0), ("xlsx.analysis.out.sum", 1), ("xlsx.analysis.out.average", 2), ("xlsx.analysis.out.variance", 3)];
    let stat = |v: &[f64], which: usize| -> R {
        match which {
            0 => Ok(v.len() as f64),
            1 => Ok(v.iter().sum()),
            2 => mean(v),
            _ => variance(v),
        }
    };
    let mut r = 3u32;
    for (g, group) in groups.iter().enumerate() {
        out.text(r, 0, group_names[g].clone());
        for (k, (id, which)) in stat_rows.iter().enumerate() {
            let row = r + 1 + k as u32;
            out.text(row, 0, l(id));
            for (j, cell) in group.iter().enumerate() {
                out.val(row, j as u32 + 1, stat(cell, *which));
            }
            let pooled: Vec<f64> = group.iter().flatten().copied().collect();
            out.val(row, c as u32 + 1, stat(&pooled, *which));
        }
        r += 6;
    }
    out.text(r, 0, l("xlsx.analysis.out.total"));
    for (k, (id, which)) in stat_rows.iter().enumerate() {
        let row = r + 1 + k as u32;
        out.text(row, 0, l(id));
        for j in 0..c {
            let col: Vec<f64> = groups.iter().flat_map(|g| g[j].iter().copied()).collect();
            out.val(row, j as u32 + 1, stat(&col, *which));
        }
    }
    let r_groups = groups.len() as f64;
    let m = groups.first().and_then(|g| g.first()).map_or(0, Vec::len) as f64;
    let all: Vec<f64> = groups.iter().flatten().flatten().copied().collect();
    let grand = mean(&all).unwrap_or(0.0);
    let ss_sample: f64 = groups
        .iter()
        .map(|g| {
            let pooled: Vec<f64> = g.iter().flatten().copied().collect();
            c as f64 * m * (mean(&pooled).unwrap_or(grand) - grand).powi(2)
        })
        .sum();
    let ss_columns: f64 = (0..c)
        .map(|j| {
            let col: Vec<f64> = groups.iter().flat_map(|g| g[j].iter().copied()).collect();
            r_groups * m * (mean(&col).unwrap_or(grand) - grand).powi(2)
        })
        .sum();
    let sst: f64 = all.iter().map(|x| (x - grand).powi(2)).sum();
    let ss_within: f64 = groups.iter().flatten().map(|cell| cell.iter().map(|x| (x - mean(cell).unwrap_or(0.0)).powi(2)).sum::<f64>()).sum();
    let ss_inter = sst - ss_sample - ss_columns - ss_within;
    let (df_sample, df_columns) = (r_groups - 1.0, c as f64 - 1.0);
    let df_inter = df_sample * df_columns;
    let df_within = r_groups * c as f64 * (m - 1.0);
    let mse = ss_within / df_within;
    let top = r + 6;
    out.text(top, 0, l("xlsx.analysis.out.anova"));
    anova_header(&mut out, top + 1, l);
    for (k, (label, ss, df)) in [(l("xlsx.analysis.out.sample"), ss_sample, df_sample), (l("xlsx.analysis.out.columns"), ss_columns, df_columns), (l("xlsx.analysis.out.interaction"), ss_inter, df_inter)].into_iter().enumerate() {
        let row = top + 2 + k as u32;
        anova_row(&mut out, row, label, ss, df);
        anova_f(&mut out, row, ss, df, mse, df_within, alpha, ev);
    }
    anova_row(&mut out, top + 5, l("xlsx.analysis.out.within-error"), ss_within, df_within);
    anova_row(&mut out, top + 7, l("xlsx.analysis.out.total"), sst, r_groups * c as f64 * m - 1.0);
    out
}

/// Where a series sits in the sheet, for the formulas of the smoothing
/// tools: the cells in order, and the sheet prefix (`Sheet2!`) if it is
/// not the output's sheet.
#[derive(Clone, Debug)]
pub struct Series {
    pub cells: Vec<(u32, u32)>,
    pub prefix: String,
}

fn a1(prefix: &str, (row, column): (u32, u32)) -> String {
    format!("{prefix}{}", crate::sheet::cell_name(column, row))
}

fn a1_range(prefix: &str, first: (u32, u32), last: (u32, u32)) -> String {
    format!("{prefix}{}:{}", crate::sheet::cell_name(first.1, first.0), crate::sheet::cell_name(last.1, last.0))
}

/// 移動平均: the output runs down a column from `origin` (or along a row
/// if the input is a row).
pub fn moving_average(input: &Series, interval: usize, std_errors: bool, origin: (u32, u32)) -> Output {
    let mut out = Output::default();
    let n = input.cells.len();
    let vertical = input.cells.len() < 2 || input.cells[0].1 == input.cells[1].1;
    let at = |i: usize, extra: u32| if vertical { (i as u32, extra) } else { (extra, i as u32) };
    let abs = |i: usize, extra: u32| {
        let (r, c) = at(i, extra);
        (origin.0 + r, origin.1 + c)
    };
    for i in 0..n {
        let (r, c) = at(i, 0);
        if i + 1 < interval {
            out.err(r, c, CellError::NA);
        } else {
            out.formula(r, c, format!("AVERAGE({})", a1_range(&input.prefix, input.cells[i + 1 - interval], input.cells[i])));
        }
        if std_errors {
            let (r, c) = at(i, 1);
            if i + 2 < 2 * interval {
                out.err(r, c, CellError::NA);
            } else {
                out.formula(r, c, format!("SQRT(SUMXMY2({},{})/{interval})", a1_range(&input.prefix, input.cells[i + 1 - interval], input.cells[i]), a1_range("", abs(i + 1 - interval, 0), abs(i, 0))));
            }
        }
    }
    out
}

/// 指数平滑: the damping factor `d` (the weight of the previous
/// smoothed value).
pub fn exponential_smoothing(input: &Series, damping: f64, std_errors: bool, origin: (u32, u32)) -> Output {
    let mut out = Output::default();
    let n = input.cells.len();
    let vertical = input.cells.len() < 2 || input.cells[0].1 == input.cells[1].1;
    let at = |i: usize, extra: u32| if vertical { (i as u32, extra) } else { (extra, i as u32) };
    let abs = |i: usize, extra: u32| {
        let (r, c) = at(i, extra);
        (origin.0 + r, origin.1 + c)
    };
    let (keep, weight) = (number_to_xml(round15(1.0 - damping)), number_to_xml(damping));
    for i in 0..n {
        let (r, c) = at(i, 0);
        match i {
            0 => out.err(r, c, CellError::NA),
            1 => out.formula(r, c, a1(&input.prefix, input.cells[0])),
            _ => out.formula(r, c, format!("{keep}*{}+{weight}*{}", a1(&input.prefix, input.cells[i - 1]), a1("", abs(i - 1, 0)))),
        }
        if std_errors {
            let (r, c) = at(i, 1);
            if i < 3 {
                out.err(r, c, CellError::NA);
            } else {
                out.formula(r, c, format!("SQRT(SUMXMY2({},{})/3)", a1_range(&input.prefix, input.cells[i - 2], input.cells[i]), a1_range("", abs(i - 2, 0), abs(i, 0))));
            }
        }
    }
    out
}

fn round15(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    format!("{v:.14e}").parse().unwrap_or(v)
}

/// サンプリング.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sampling {
    /// Every `n`th number, from the `n`th.
    Periodic(usize),
    /// `n` numbers picked at random (a number can be picked twice).
    Random(usize),
}

pub fn sampling(data: &[f64], how: Sampling, seed: u64) -> Output {
    let mut out = Output::default();
    match how {
        Sampling::Periodic(n) => {
            if n > 0 {
                for (k, v) in data.iter().skip(n - 1).step_by(n).enumerate() {
                    out.num(k as u32, 0, *v);
                }
            }
        }
        Sampling::Random(n) => {
            let mut rng = Rng(seed);
            for k in 0..n {
                if data.is_empty() {
                    break;
                }
                out.num(k as u32, 0, data[(rng.unit() * data.len() as f64) as usize % data.len()]);
            }
        }
    }
    out
}

/// A small generator (SplitMix64) so that a given seed gives the same
/// numbers.
pub struct Rng(pub u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// In [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn normal(&mut self) -> f64 {
        let (u, v) = (1.0 - self.unit(), self.unit());
        (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()
    }
}

/// 乱数発生.
#[derive(Clone, Debug, PartialEq)]
pub enum Distribution {
    Uniform { low: f64, high: f64 },
    Normal { mean: f64, sd: f64 },
    Bernoulli { p: f64 },
    Binomial { p: f64, trials: u32 },
    Poisson { lambda: f64 },
    /// `from`, `to`, step, each number repeated `repeat_each` times, the
    /// whole sequence repeated `repeat_sequence` times.
    Patterned { from: f64, to: f64, step: f64, repeat_each: u32, repeat_sequence: u32 },
    Discrete { values: Vec<f64>, probabilities: Vec<f64> },
}

/// `variables` columns of `count` numbers.
pub fn random_numbers(variables: usize, count: usize, d: &Distribution, seed: u64) -> Result<Output, &'static str> {
    let mut out = Output::default();
    let mut rng = Rng(seed);
    match d {
        Distribution::Uniform { low, high } if high < low => return Err("range"),
        Distribution::Normal { sd, .. } if *sd < 0.0 => return Err("sd"),
        Distribution::Bernoulli { p } | Distribution::Binomial { p, .. } if !(0.0..=1.0).contains(p) => return Err("p"),
        Distribution::Poisson { lambda } if *lambda <= 0.0 => return Err("lambda"),
        Distribution::Binomial { trials, .. } if *trials == 0 || *trials > 1_000_000 => return Err("trials"),
        Distribution::Patterned { step, from, to, .. } if *step == 0.0 || (to - from) * step < 0.0 => return Err("step"),
        Distribution::Discrete { values, probabilities } if values.is_empty() || values.len() != probabilities.len() || (probabilities.iter().sum::<f64>() - 1.0).abs() > 1e-9 || probabilities.iter().any(|p| *p < 0.0) => return Err("probabilities"),
        _ => {}
    }
    if let Distribution::Patterned { from, to, step, repeat_each, repeat_sequence } = d {
        let mut sequence = Vec::new();
        let mut v = *from;
        while (*step > 0.0 && v <= to + 1e-12) || (*step < 0.0 && v >= to - 1e-12) {
            for _ in 0..(*repeat_each).max(1) {
                sequence.push(round15(v));
            }
            v += step;
        }
        let all: Vec<f64> = (0..(*repeat_sequence).max(1)).flat_map(|_| sequence.iter().copied()).collect();
        for col in 0..variables {
            for (row, v) in all.iter().cycle().take(count).enumerate() {
                out.num(row as u32, col as u32, *v);
            }
        }
        return Ok(out);
    }
    for col in 0..variables {
        for row in 0..count {
            let v = match d {
                Distribution::Uniform { low, high } => low + (high - low) * rng.unit(),
                Distribution::Normal { mean, sd } => mean + sd * rng.normal(),
                Distribution::Bernoulli { p } => f64::from(rng.unit() < *p),
                Distribution::Binomial { p, trials } => (0..*trials).filter(|_| rng.unit() < *p).count() as f64,
                Distribution::Poisson { lambda } => {
                    if *lambda > 500.0 {
                        (lambda + lambda.sqrt() * rng.normal()).round().max(0.0)
                    } else {
                        let limit = (-lambda).exp();
                        let (mut k, mut prod) = (0.0, rng.unit());
                        while prod > limit {
                            k += 1.0;
                            prod *= rng.unit();
                        }
                        k
                    }
                }
                Distribution::Discrete { values, probabilities } => {
                    let u = rng.unit();
                    let mut acc = 0.0;
                    let mut pick = *values.last().unwrap_or(&0.0);
                    for (v, p) in values.iter().zip(probabilities) {
                        acc += p;
                        if u < acc {
                            pick = *v;
                            break;
                        }
                    }
                    pick
                }
                Distribution::Patterned { .. } => 0.0,
            };
            out.num(row as u32, col as u32, v);
        }
    }
    Ok(out)
}

/// フーリエ解析: the discrete transform of `data` (a power of 2, at most
/// 4096 numbers) as Excel's complex-number text.
pub fn fourier(data: &[f64], inverse: bool) -> Result<Output, &'static str> {
    let n = data.len();
    if n < 2 || !n.is_power_of_two() || n > 4096 {
        return Err("size");
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut out = Output::default();
    let scale = if inverse { 1.0 / n as f64 } else { 1.0 };
    let results: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let (mut re, mut im) = (0.0, 0.0);
            for (j, x) in data.iter().enumerate() {
                let angle = sign * 2.0 * std::f64::consts::PI * ((j * k) % n) as f64 / n as f64;
                re += x * angle.cos();
                im += x * angle.sin();
            }
            (re * scale, im * scale)
        })
        .collect();
    let magnitude = results.iter().fold(0.0f64, |m, (re, im)| m.max(re.abs()).max(im.abs()));
    let clean = |v: f64| if v.abs() <= magnitude * 1e-12 { 0.0 } else { round15(v) };
    for (k, (re, im)) in results.into_iter().enumerate() {
        out.text(k as u32, 0, complex_text(clean(re), clean(im)));
    }
    Ok(out)
}

/// `3`, `-2+2i`, `4i`, `1-i` (Excel's text for a complex number).
pub fn complex_text(re: f64, im: f64) -> String {
    let imaginary = match im {
        v if v == 1.0 => "i".to_string(),
        v if v == -1.0 => "-i".to_string(),
        v => format!("{}i", number_to_xml(v)),
    };
    match (re == 0.0, im == 0.0) {
        (_, true) => number_to_xml(re),
        (true, false) => imaginary,
        (false, false) => format!("{}{}{}", number_to_xml(re), if im > 0.0 { "+" } else { "" }, imaginary),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{EmuRect, EmuSize};
    use crate::model::{Shape, ShapeKind, Slide};
    use crate::sheet::Workbook;
    use std::sync::Arc;

    fn presentation() -> Presentation {
        let mut p = Presentation::empty(EmuSize::WIDESCREEN);
        let id = p.new_slide_id();
        let mut slide = Slide::new(id);
        slide.name = Some("Sheet1".into());
        let shape = p.new_shape_id();
        let sheet = Sheet { columns: vec![100; 10], rows: vec![100; 20], grid_lines: true, ..Sheet::default() };
        slide.shapes.push(Shape { id: shape, frame: EmuRect::new(0, 0, 1000, 2000), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Sheet(Arc::new(sheet)), floating: None });
        p.slides.push(slide);
        p.workbook = Some(Arc::new(Workbook {
            sheets: vec![crate::sheet::WorkbookSheet { name: "Sheet1".into(), state: Default::default(), slide: Some(id), hidden: None, other: false }],
            ..Default::default()
        }));
        p
    }

    fn l(id: &str) -> String {
        id.trim_start_matches("xlsx.analysis.out.").to_string()
    }

    fn block(cols: &[&[f64]]) -> Block {
        Block { names: (1..=cols.len()).map(|i| format!("C{i}")).collect(), cols: cols.iter().map(|c| c.iter().map(|v| Some(*v)).collect()).collect(), labelled: false }
    }

    fn value(out: &Output, r: u32, c: u32) -> CellValue {
        match out.cells.iter().find(|x| x.0 == r && x.1 == c).map(|x| &x.2) {
            Some(CellInput::Value(v)) => v.clone(),
            Some(CellInput::Formula(f)) => CellValue::Text(format!("={f}")),
            _ => CellValue::Empty,
        }
    }

    fn number(out: &Output, r: u32, c: u32) -> f64 {
        match value(out, r, c) {
            CellValue::Number(n) => n,
            other => panic!("({r},{c}) is {other:?}"),
        }
    }

    #[test]
    fn descriptive_statistics_of_one_to_ten() {
        let p = presentation();
        let ev = Eval::new(&p);
        let b = block(&[&[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]]);
        let out = descriptive(&b, &Descriptive { confidence: Some(95.0), kth_largest: Some(1), kth_smallest: Some(2) }, &l, &ev);
        assert_eq!(value(&out, 0, 0), CellValue::Text("C1".into()));
        assert_eq!(value(&out, 1, 0), CellValue::Text("mean".into()));
        assert_eq!(number(&out, 1, 1), 5.5);
        assert!((number(&out, 2, 1) - 0.957427107756338).abs() < 1e-12);
        assert_eq!(number(&out, 3, 1), 5.5);
        assert_eq!(value(&out, 4, 1), CellValue::Error(CellError::NA));
        assert!((number(&out, 5, 1) - 3.0276503540974917).abs() < 1e-12);
        assert!((number(&out, 6, 1) - 9.166666666666666).abs() < 1e-12);
        assert!((number(&out, 7, 1) + 1.2).abs() < 1e-12);
        assert!(number(&out, 8, 1).abs() < 1e-12);
        assert_eq!(number(&out, 9, 1), 9.0);
        assert_eq!(number(&out, 10, 1), 1.0);
        assert_eq!(number(&out, 11, 1), 10.0);
        assert_eq!(number(&out, 12, 1), 55.0);
        assert_eq!(number(&out, 13, 1), 10.0);
        assert_eq!(value(&out, 14, 0), CellValue::Text("largest(1)".into()));
        assert_eq!(number(&out, 14, 1), 10.0);
        assert_eq!(number(&out, 15, 1), 2.0);
        assert_eq!(value(&out, 16, 0), CellValue::Text("confidence-level(95.0%)".into()));
        // t(0.975, 9) * s / sqrt(n)
        assert!((number(&out, 16, 1) - 2.262157162740992 * 3.0276503540974917 / 10f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn mode_is_the_first_value_that_repeats_most() {
        assert_eq!(mode(&[3., 1., 1., 3., 3., 1.]), Ok(3.0));
        assert_eq!(mode(&[1., 2.]), Err(CellError::NA));
    }

    #[test]
    fn correlation_and_covariance_matrices_are_lower_triangles() {
        let b = block(&[&[1., 2., 3., 4.], &[2., 4., 6., 8.], &[4., 3., 2., 1.]]);
        let out = correlation_or_covariance(&b, false);
        assert_eq!(number(&out, 1, 1), 1.0);
        assert!((number(&out, 2, 1) - 1.0).abs() < 1e-12);
        assert!((number(&out, 3, 1) + 1.0).abs() < 1e-12);
        assert_eq!(value(&out, 1, 2), CellValue::Empty);
        let cov = correlation_or_covariance(&b, true);
        // Population covariance of 1..4 with itself: 1.25.
        assert!((number(&cov, 1, 1) - 1.25).abs() < 1e-12);
        assert!((number(&cov, 2, 1) - 2.5).abs() < 1e-12);
    }

    #[test]
    fn rank_and_percent_are_sorted_from_the_greatest() {
        let b = block(&[&[10., 30., 20., 30.]]);
        let out = rank_percentile(&b, &l);
        // Points 2 and 4 (30) tie for rank 1.
        assert_eq!((number(&out, 1, 0), number(&out, 1, 1), number(&out, 1, 2)), (2.0, 30.0, 1.0));
        assert_eq!((number(&out, 2, 0), number(&out, 2, 2)), (4.0, 1.0));
        assert_eq!((number(&out, 3, 0), number(&out, 3, 2)), (3.0, 3.0));
        assert!((number(&out, 1, 3) - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(number(&out, 4, 3), 0.0);
    }

    #[test]
    fn histogram_counts_each_bin_and_the_rest() {
        let data = [1., 2., 2., 3., 5., 8.];
        let out = histogram(&data, &Histogram { bins: Some(vec![5.0, 2.0]), pareto: false, cumulative: true }, &l);
        // Bins sort to 2, 5: <= 2: 3, (2, 5]: 2, more: 1.
        assert_eq!((number(&out, 1, 0), number(&out, 1, 1)), (2.0, 3.0));
        assert_eq!((number(&out, 2, 0), number(&out, 2, 1)), (5.0, 2.0));
        assert_eq!(value(&out, 3, 0), CellValue::Text("more".into()));
        assert_eq!(number(&out, 3, 1), 1.0);
        assert!((number(&out, 2, 2) - 5.0 / 6.0).abs() < 1e-12);
        assert_eq!(number(&out, 3, 2), 1.0);
        let pareto = histogram(&data, &Histogram { bins: Some(vec![2.0, 5.0]), pareto: true, cumulative: false }, &l);
        assert_eq!(number(&pareto, 1, 4), 3.0);
        assert_eq!(number(&pareto, 2, 4), 2.0);
    }

    #[test]
    fn regression_recovers_a_line_with_noise_free_data() {
        let p = presentation();
        let ev = Eval::new(&p);
        let x = block(&[&[1., 2., 3., 4., 5.]]);
        let y: Vec<Option<f64>> = [3., 5., 7., 9., 11.].iter().map(|v| Some(*v)).collect();
        let out = regression(&y, &x, &Regression { constant: true, confidence: 95.0, residuals: true, standard_residuals: true }, &l, &ev).unwrap();
        assert!((number(&out, 3, 1) - 1.0).abs() < 1e-9);
        assert_eq!(number(&out, 7, 1), 5.0);
        assert!((number(&out, 16, 1) - 1.0).abs() < 1e-9);
        assert!((number(&out, 17, 1) - 2.0).abs() < 1e-9);
        assert_eq!(value(&out, 16, 0), CellValue::Text("intercept".into()));
        assert_eq!(value(&out, 17, 0), CellValue::Text("x-variable 1".into()));
        // Residuals are all about 0.
        let top = 16 + 2 + 1;
        assert!(number(&out, top + 3, 2).abs() < 1e-9);
    }

    #[test]
    fn regression_through_the_origin() {
        let p = presentation();
        let ev = Eval::new(&p);
        let x = block(&[&[1., 2., 3.]]);
        let y: Vec<Option<f64>> = [2., 4., 6.].iter().map(|v| Some(*v)).collect();
        let out = regression(&y, &x, &Regression { constant: false, confidence: 95.0, residuals: false, standard_residuals: false }, &l, &ev).unwrap();
        assert!((number(&out, 16, 1) - 2.0).abs() < 1e-9);
        assert_eq!(value(&out, 16, 0), CellValue::Text("x-variable 1".into()));
    }

    #[test]
    fn single_factor_anova_matches_the_hand_calculation() {
        let p = presentation();
        let ev = Eval::new(&p);
        let b = block(&[&[1., 2., 3.], &[2., 3., 4.], &[5., 6., 7.]]);
        let out = anova_single(&b, 0.05, &l, &ev);
        // Between: SS 26, df 2; within: SS 6, df 6; F = 13.
        let top = 4 + 3 + 1;
        assert_eq!((number(&out, top + 2, 1), number(&out, top + 2, 2)), (26.0, 2.0));
        assert_eq!((number(&out, top + 3, 1), number(&out, top + 3, 2)), (6.0, 6.0));
        assert!((number(&out, top + 2, 4) - 13.0).abs() < 1e-12);
    }

    #[test]
    fn t_tests_have_excels_rows() {
        let p = presentation();
        let ev = Eval::new(&p);
        let a: Vec<Option<f64>> = [1., 2., 3., 4., 5.].iter().map(|v| Some(*v)).collect();
        let b: Vec<Option<f64>> = [2., 4., 5., 7., 9.].iter().map(|v| Some(*v)).collect();
        let eq = t_test(&a, &b, ("a", "b"), TTest::EqualVariance, 0.0, 0.05, &l, &ev);
        // df = 8; pooled variance = (2.5 + 7.3) / 2 = 4.9.
        assert_eq!(number(&eq, 7, 1), 8.0);
        assert!((number(&eq, 5, 1) - 4.9).abs() < 1e-12);
        assert_eq!(number(&eq, 6, 1), 0.0);
        assert!((number(&eq, 3, 1) - 2.5).abs() < 1e-12);
        let t = number(&eq, 8, 1);
        assert!((t - (3.0 - 5.4) / (4.9f64 * 0.4).sqrt()).abs() < 1e-12);
        assert!((number(&eq, 12, 1) - 2.306004135).abs() < 1e-6);
        let paired = t_test(&a, &b, ("a", "b"), TTest::Paired, 0.0, 0.05, &l, &ev);
        assert_eq!(number(&paired, 7, 1), 4.0);
        assert_eq!(number(&paired, 6, 1), 0.0);
        assert!(number(&paired, 5, 1) > 0.99);
        let un = t_test(&a, &b, ("a", "b"), TTest::UnequalVariance, 0.0, 0.05, &l, &ev);
        assert_eq!(number(&un, 6, 1), 6.0);
    }

    #[test]
    fn z_and_f_tests() {
        let p = presentation();
        let ev = Eval::new(&p);
        let a: Vec<Option<f64>> = [1., 2., 3., 4., 5.].iter().map(|v| Some(*v)).collect();
        let b: Vec<Option<f64>> = [2., 4., 5., 7., 9.].iter().map(|v| Some(*v)).collect();
        let z = z_test(&a, &b, ("a", "b"), (2.5, 7.5), 0.0, 0.05, &l, &ev);
        assert!((number(&z, 6, 1) - (-2.4 / 2.0f64.sqrt())).abs() < 1e-12);
        assert!((number(&z, 8, 1) - 1.6448536269514722).abs() < 1e-9);
        assert!((number(&z, 10, 1) - 1.959963984540054).abs() < 1e-9);
        let f = f_test(&a, &b, ("a", "b"), 0.05, &l, &ev);
        assert!((number(&f, 6, 1) - 2.5 / 7.3).abs() < 1e-12);
        assert_eq!(number(&f, 5, 1), 4.0);
    }

    #[test]
    fn two_factor_anova_without_replication() {
        let p = presentation();
        let ev = Eval::new(&p);
        let rows = vec![vec![1., 2., 3.], vec![2., 4., 6.], vec![3., 6., 9.]];
        let names: Vec<String> = (1..=3).map(|i| format!("r{i}")).collect();
        let cols: Vec<String> = (1..=3).map(|i| format!("c{i}")).collect();
        let out = anova_two_without(&rows, &names, &cols, 0.05, &l, &ev);
        // Grand mean 4; row means 2, 4, 6; column means 2, 4, 6.
        let top = 3 + 3 + 3 + 1;
        assert_eq!(number(&out, top + 2, 1), 3.0 * (4.0 + 0.0 + 4.0));
        assert_eq!(number(&out, top + 3, 1), 3.0 * (4.0 + 0.0 + 4.0));
        assert_eq!(number(&out, top + 4, 2), 4.0);
    }

    #[test]
    fn two_factor_anova_with_replication() {
        let p = presentation();
        let ev = Eval::new(&p);
        // Two samples, two columns, two values in each cell.
        let groups = vec![vec![vec![1., 3.], vec![5., 7.]], vec![vec![2., 4.], vec![6., 10.]]];
        let names = vec!["g1".to_string(), "g2".to_string()];
        let cols = vec!["c1".to_string(), "c2".to_string()];
        let out = anova_two_with(&groups, &names, &cols, 0.05, &l, &ev);
        let top = 3 + 6 * 2 + 6;
        // Within: each cell has SS 2, 2, 2, 8 -> 14, df 4.
        assert_eq!((number(&out, top + 5, 1), number(&out, top + 5, 2)), (14.0, 4.0));
        assert_eq!(number(&out, top + 7, 2), 7.0);
    }

    #[test]
    fn moving_average_writes_formulas_after_the_interval() {
        let series = Series { cells: (0..5).map(|r| (r, 0)).collect(), prefix: String::new() };
        let out = moving_average(&series, 3, true, (0, 1));
        assert_eq!(value(&out, 0, 0), CellValue::Error(CellError::NA));
        assert_eq!(value(&out, 1, 0), CellValue::Error(CellError::NA));
        assert_eq!(value(&out, 2, 0), CellValue::Text("=AVERAGE(A1:A3)".into()));
        assert_eq!(value(&out, 4, 0), CellValue::Text("=AVERAGE(A3:A5)".into()));
        assert_eq!(value(&out, 3, 1), CellValue::Error(CellError::NA));
        assert_eq!(value(&out, 4, 1), CellValue::Text("=SQRT(SUMXMY2(A3:A5,B3:B5)/3)".into()));
    }

    #[test]
    fn exponential_smoothing_chains_the_previous_value() {
        let series = Series { cells: (0..4).map(|r| (r, 0)).collect(), prefix: "Data!".into() };
        let out = exponential_smoothing(&series, 0.3, false, (0, 1));
        assert_eq!(value(&out, 0, 0), CellValue::Error(CellError::NA));
        assert_eq!(value(&out, 1, 0), CellValue::Text("=Data!A1".into()));
        assert_eq!(value(&out, 2, 0), CellValue::Text("=0.7*Data!A2+0.3*B2".into()));
    }

    #[test]
    fn sampling_and_random_numbers() {
        let data = [1., 2., 3., 4., 5., 6., 7.];
        let out = sampling(&data, Sampling::Periodic(3), 1);
        assert_eq!(out.cells.len(), 2);
        assert_eq!((number(&out, 0, 0), number(&out, 1, 0)), (3.0, 6.0));
        let random = sampling(&data, Sampling::Random(4), 7);
        assert_eq!(random.cells.len(), 4);
        let again = sampling(&data, Sampling::Random(4), 7);
        assert_eq!(number(&random, 2, 0), number(&again, 2, 0));
        let out = random_numbers(2, 5, &Distribution::Uniform { low: 10.0, high: 20.0 }, 3).unwrap();
        assert_eq!(out.cells.len(), 10);
        assert!(out.cells.iter().all(|c| matches!(&c.2, CellInput::Value(CellValue::Number(n)) if (10.0..20.0).contains(n))));
        let patterned = random_numbers(1, 7, &Distribution::Patterned { from: 1.0, to: 3.0, step: 1.0, repeat_each: 2, repeat_sequence: 1 }, 0).unwrap();
        let values: Vec<f64> = (0..7).map(|r| number(&patterned, r, 0)).collect();
        assert_eq!(values, vec![1., 1., 2., 2., 3., 3., 1.]);
        assert!(random_numbers(1, 3, &Distribution::Bernoulli { p: 2.0 }, 0).is_err());
        let b = random_numbers(1, 50, &Distribution::Binomial { p: 0.5, trials: 10 }, 5).unwrap();
        assert!(b.cells.iter().all(|c| matches!(&c.2, CellInput::Value(CellValue::Number(n)) if (0.0..=10.0).contains(n) && n.fract() == 0.0)));
    }

    #[test]
    fn fourier_transform_of_four_numbers() {
        let out = fourier(&[1., 2., 3., 4.], false).unwrap();
        let texts: Vec<CellValue> = (0..4).map(|r| value(&out, r, 0)).collect();
        assert_eq!(texts, vec![CellValue::Text("10".into()), CellValue::Text("-2+2i".into()), CellValue::Text("-2".into()), CellValue::Text("-2-2i".into())]);
        assert!(fourier(&[1., 2., 3.], false).is_err());
        assert_eq!(complex_text(0.0, 1.0), "i");
        assert_eq!(complex_text(1.0, -1.0), "1-i");
    }
}
