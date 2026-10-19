// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > ソルバー… (Excel's Solver): the objective cell, the variable
//! cells, the constraints and the method; 解決 (Solve) tries the model on a
//! copy of the book (`ibis_doc_model::solver`), and the solution is kept
//! (or not) with the model saved in the sheet's hidden `solver_*` names and,
//! if asked, a 解答レポート (Answer Report) sheet.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::solver::{self, Description, Goal, Method, Rel, Solution, Status};
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, ReportCell, SheetCommand, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GoalKind {
    Max,
    Min,
    Value,
}

const RELATIONS: [Rel; 6] = [Rel::Le, Rel::Eq, Rel::Ge, Rel::Int, Rel::Bin, Rel::Dif];

fn relation_id(r: Rel) -> &'static str {
    match r {
        Rel::Le => "xlsx.solver.rel.le",
        Rel::Eq => "xlsx.solver.rel.eq",
        Rel::Ge => "xlsx.solver.rel.ge",
        Rel::Int => "xlsx.solver.rel.int",
        Rel::Bin => "xlsx.solver.rel.bin",
        Rel::Dif => "xlsx.solver.rel.dif",
    }
}

fn method_id(m: Method) -> &'static str {
    match m {
        Method::Grg => "xlsx.solver.method.grg",
        Method::Simplex => "xlsx.solver.method.simplex",
        Method::Evolutionary => "xlsx.solver.method.evolutionary",
    }
}

fn status_id(s: Status) -> &'static str {
    match s {
        Status::Optimal => "xlsx.solver.status.optimal",
        Status::Converged => "xlsx.solver.status.converged",
        Status::Infeasible => "xlsx.solver.status.infeasible",
        Status::Unbounded => "xlsx.solver.status.unbounded",
        Status::IterationLimit => "xlsx.solver.status.iteration-limit",
        Status::Stopped => "xlsx.solver.status.stopped",
        Status::NotLinear => "xlsx.solver.status.not-linear",
        Status::MissingBounds => "xlsx.solver.status.missing-bounds",
        Status::BadModel => "xlsx.solver.status.bad-model",
    }
}

/// Every caption the panel and the report use.
#[cfg(test)]
pub(crate) const IDS: &[&str] = &[
    "xlsx.solver",
    "xlsx.solver.title",
    "xlsx.solver.objective",
    "xlsx.solver.goal-max",
    "xlsx.solver.goal-min",
    "xlsx.solver.goal-value",
    "xlsx.solver.variables",
    "xlsx.solver.constraints",
    "xlsx.solver.cell-reference",
    "xlsx.solver.constraint-value",
    "xlsx.solver.add",
    "xlsx.solver.change",
    "xlsx.solver.delete",
    "xlsx.solver.non-negative",
    "xlsx.solver.method",
    "xlsx.solver.precision",
    "xlsx.solver.convergence",
    "xlsx.solver.integer-tolerance",
    "xlsx.solver.max-iterations",
    "xlsx.solver.max-time",
    "xlsx.solver.solve",
    "xlsx.solver.reset-all",
    "xlsx.solver.keep",
    "xlsx.solver.restore",
    "xlsx.solver.answer-report",
    "xlsx.solver.sensitivity-report",
    "xlsx.solver.limits-report",
    "xlsx.solver.bad-objective",
    "xlsx.solver.bad-variables",
    "xlsx.solver.bad-constraint",
    "xlsx.solver.bad-number",
    "xlsx.solver.rel.le",
    "xlsx.solver.rel.eq",
    "xlsx.solver.rel.ge",
    "xlsx.solver.rel.int",
    "xlsx.solver.rel.bin",
    "xlsx.solver.rel.dif",
    "xlsx.solver.method.grg",
    "xlsx.solver.method.simplex",
    "xlsx.solver.method.evolutionary",
    "xlsx.solver.status.optimal",
    "xlsx.solver.status.converged",
    "xlsx.solver.status.infeasible",
    "xlsx.solver.status.unbounded",
    "xlsx.solver.status.iteration-limit",
    "xlsx.solver.status.stopped",
    "xlsx.solver.status.not-linear",
    "xlsx.solver.status.missing-bounds",
    "xlsx.solver.status.bad-model",
    "xlsx.solver.report.title",
    "xlsx.solver.report.worksheet",
    "xlsx.solver.report.goal-value",
    "xlsx.solver.report.objective-cell",
    "xlsx.solver.report.variable-cells",
    "xlsx.solver.report.constraints",
    "xlsx.solver.report.cell",
    "xlsx.solver.report.name",
    "xlsx.solver.report.original",
    "xlsx.solver.report.final",
    "xlsx.solver.report.integer",
    "xlsx.solver.report.cell-value",
    "xlsx.solver.report.formula",
    "xlsx.solver.report.status",
    "xlsx.solver.report.slack",
    "xlsx.solver.report.binding",
    "xlsx.solver.report.not-binding",
    "xlsx.solver.report.continuous",
    "xlsx.solver.report.reduced-cost",
    "xlsx.solver.report.objective-coefficient",
    "xlsx.solver.report.allowable-increase",
    "xlsx.solver.report.allowable-decrease",
    "xlsx.solver.report.shadow-price",
    "xlsx.solver.report.constraint-rhs",
    "xlsx.solver.report.value",
    "xlsx.solver.report.lower-limit",
    "xlsx.solver.report.upper-limit",
    "xlsx.solver.report.objective-result",
];

pub(crate) struct SolverPanel {
    pub(crate) objective: Entity<InputState>,
    pub(crate) goal: GoalKind,
    pub(crate) goal_value: Entity<InputState>,
    pub(crate) variables: Entity<InputState>,
    pub(crate) constraints: Vec<(String, Rel, String)>,
    pub(crate) selected: Option<usize>,
    pub(crate) c_lhs: Entity<InputState>,
    pub(crate) c_rhs: Entity<InputState>,
    pub(crate) c_rel: Rel,
    pub(crate) non_negative: bool,
    pub(crate) method: Method,
    pub(crate) precision: Entity<InputState>,
    pub(crate) convergence: Entity<InputState>,
    pub(crate) integer_tolerance: Entity<InputState>,
    pub(crate) max_iterations: Entity<InputState>,
    pub(crate) max_time: Entity<InputState>,
    pub(crate) report: bool,
    pub(crate) sensitivity_report: bool,
    pub(crate) limits_report: bool,
    /// The solution found, until it is kept or dropped.
    pub(crate) found: Option<(Solution, Vec<f64>)>,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
    #[allow(dead_code)]
    slide: SlideId,
}

fn number_text(v: f64) -> String {
    ibis_calc::number_to_xml(v)
}

impl SlideEditor {
    pub(crate) fn normalize_solver_ref(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.parse::<f64>().is_ok() {
            return trimmed.to_string();
        }
        let mut pieces = Vec::new();
        for part in trimmed.split(',') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            if let Some((slide_id, r)) = super::sheet_ui::parse_reference(p).and_then(|ref_| self.resolve_reference(&ref_))
                && slide_id == self.current_slide
            {
                if r.first_row == r.last_row && r.first_column == r.last_column {
                    pieces.push(ibis_doc_model::cell_name(r.first_column, r.first_row));
                } else {
                    pieces.push(format!(
                        "{}:{}",
                        ibis_doc_model::cell_name(r.first_column, r.first_row),
                        ibis_doc_model::cell_name(r.last_column, r.last_row)
                    ));
                }
            } else {
                pieces.push(p.to_string());
            }
        }
        pieces.join(",")
    }

    pub(crate) fn set_solver_ref_from_selection(&mut self, field: &str, window: &mut Window, cx: &mut Context<Self>) {
        let single = self
            .selected_range()
            .map(|r| ibis_doc_model::cell_name(r.first_column, r.first_row))
            .or_else(|| self.current_cell().map(|(c, r)| ibis_doc_model::cell_name(c, r)))
            .unwrap_or_default();
        let range = self.current_selection_a1_string();
        if let Some(p) = self.wb_mut().panels.solver.as_mut() {
            match field {
                "objective" if !single.is_empty() => p.objective.update(cx, |i, cx| i.set_value(single, window, cx)),
                "variables" if !range.is_empty() => p.variables.update(cx, |i, cx| i.set_value(range, window, cx)),
                "c-lhs" if !range.is_empty() => p.c_lhs.update(cx, |i, cx| i.set_value(range, window, cx)),
                "c-rhs" if !range.is_empty() => p.c_rhs.update(cx, |i, cx| i.set_value(range, window, cx)),
                _ => {}
            }
        }
        cx.notify();
    }

    pub(crate) fn solver_reset_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let d = Description::default();
        let limit = |v: u32| if v == 0 { String::new() } else { v.to_string() };
        if let Some(p) = self.wb_mut().panels.solver.as_mut() {
            p.objective.update(cx, |i, cx| i.set_value(String::new(), window, cx));
            p.goal = GoalKind::Max;
            p.goal_value.update(cx, |i, cx| i.set_value(String::new(), window, cx));
            p.variables.update(cx, |i, cx| i.set_value(String::new(), window, cx));
            p.constraints.clear();
            p.selected = None;
            p.c_lhs.update(cx, |i, cx| i.set_value(String::new(), window, cx));
            p.c_rhs.update(cx, |i, cx| i.set_value(String::new(), window, cx));
            p.c_rel = Rel::Le;
            p.non_negative = d.non_negative;
            p.method = d.method;
            p.precision.update(cx, |i, cx| i.set_value(number_text(d.precision), window, cx));
            p.convergence.update(cx, |i, cx| i.set_value(number_text(d.convergence), window, cx));
            p.integer_tolerance.update(cx, |i, cx| i.set_value(number_text(d.integer_tolerance), window, cx));
            p.max_iterations.update(cx, |i, cx| i.set_value(limit(d.max_iterations), window, cx));
            p.max_time.update(cx, |i, cx| i.set_value(limit(d.max_time), window, cx));
            p.found = None;
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn open_solver_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        // The model the sheet keeps, or a new one at the selected cell.
        let kept = self.solver_names_of_sheet();
        let d = kept.as_deref().and_then(Description::from_names).unwrap_or_else(|| Description { objective: self.current_cell().map(|(c, r)| ibis_doc_model::cell_name(c, r)).unwrap_or_default(), ..Description::default() });
        let make = |value: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(value, window, cx));
            state
        };
        let limit = |v: u32| if v == 0 { String::new() } else { v.to_string() };
        let (goal, goal_value) = match d.goal {
            Goal::Max => (GoalKind::Max, String::new()),
            Goal::Min => (GoalKind::Min, String::new()),
            Goal::Value(v) => (GoalKind::Value, number_text(v)),
        };
        let panel = SolverPanel {
            objective: make(d.objective.clone(), window, cx),
            goal,
            goal_value: make(goal_value, window, cx),
            variables: make(d.variables.clone(), window, cx),
            constraints: d.constraints.clone(),
            selected: None,
            c_lhs: make(String::new(), window, cx),
            c_rhs: make(String::new(), window, cx),
            c_rel: Rel::Le,
            non_negative: d.non_negative,
            method: d.method,
            precision: make(number_text(d.precision), window, cx),
            convergence: make(number_text(d.convergence), window, cx),
            integer_tolerance: make(number_text(d.integer_tolerance), window, cx),
            max_iterations: make(limit(d.max_iterations), window, cx),
            max_time: make(limit(d.max_time), window, cx),
            report: false,
            sensitivity_report: false,
            limits_report: false,
            found: None,
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
        state.panels.solver = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// The `solver_*` names of the current sheet: (name, formula).
    fn solver_names_of_sheet(&self) -> Option<Vec<(String, String)>> {
        let wb = self.presentation().workbook.as_ref()?;
        let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide))?;
        let names: Vec<(String, String)> = wb.names.iter().filter(|n| n.sheet == Some(index) && n.name.to_ascii_lowercase().starts_with("solver_")).map(|n| (n.name.clone(), n.formula.clone())).collect();
        (!names.is_empty()).then_some(names)
    }

    fn solver_message(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.solver.as_mut() {
            p.message = message;
        }
        cx.notify();
    }

    /// The model as typed in the panel.
    fn solver_description(&self, cx: &App) -> Result<Description, String> {
        let p = self.sheet_state().and_then(|s| s.panels.solver.as_ref()).ok_or_default()?;
        let read = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let number = |e: &Entity<InputState>, default: f64| -> Result<f64, String> {
            let t = read(e);
            if t.is_empty() { Ok(default) } else { t.parse::<f64>().map_err(|_| text("xlsx.solver.bad-number").to_string()) }
        };
        let goal = match p.goal {
            GoalKind::Max => Goal::Max,
            GoalKind::Min => Goal::Min,
            GoalKind::Value => Goal::Value(read(&p.goal_value).parse::<f64>().map_err(|_| text("xlsx.solver.bad-number").to_string())?),
        };
        let constraints = p
            .constraints
            .iter()
            .map(|(l, rel, r)| (self.normalize_solver_ref(l), *rel, self.normalize_solver_ref(r)))
            .collect();
        Ok(Description {
            objective: self.normalize_solver_ref(&read(&p.objective)),
            goal,
            variables: self.normalize_solver_ref(&read(&p.variables)),
            constraints,
            non_negative: p.non_negative,
            method: p.method,
            precision: number(&p.precision, 1e-6)?,
            convergence: number(&p.convergence, 1e-4)?,
            integer_tolerance: number(&p.integer_tolerance, 1.0)?,
            max_iterations: number(&p.max_iterations, 0.0)?.max(0.0) as u32,
            max_time: number(&p.max_time, 0.0)?.max(0.0) as u32,
        })
    }

    /// 制約条件の追加 / 変更.
    pub(crate) fn solver_put_constraint(&mut self, replace: bool, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.solver.as_ref()) else { return };
        let lhs = self.normalize_solver_ref(&p.c_lhs.read(cx).value());
        let rhs = self.normalize_solver_ref(&p.c_rhs.read(cx).value());
        let rel = p.c_rel;
        let needs_rhs = matches!(rel, Rel::Le | Rel::Eq | Rel::Ge);
        if solver::parse_cells(&lhs).is_none() || (needs_rhs && rhs.parse::<f64>().is_err() && solver::parse_cells(&rhs).is_none()) {
            return self.solver_message(text("xlsx.solver.bad-constraint").to_string(), cx);
        }
        let entry = (lhs, rel, if needs_rhs { rhs } else { String::new() });
        if let Some(p) = self.wb_mut().panels.solver.as_mut() {
            match (replace, p.selected) {
                (true, Some(i)) if i < p.constraints.len() => p.constraints[i] = entry,
                _ => p.constraints.push(entry),
            }
            p.message.clear();
        }
        cx.notify();
    }

    /// 解決: try the model.
    pub(crate) fn solver_solve(&mut self, cx: &mut Context<Self>) {
        let description = match self.solver_description(cx) {
            Ok(d) => d,
            Err(m) => return self.solver_message(m, cx),
        };
        let model = match description.to_model() {
            Ok(m) => m,
            Err("objective") => return self.solver_message(text("xlsx.solver.bad-objective").to_string(), cx),
            Err("variables") => return self.solver_message(text("xlsx.solver.bad-variables").to_string(), cx),
            Err(_) => return self.solver_message(text("xlsx.solver.bad-constraint").to_string(), cx),
        };
        let started = crate::clock::unix_millis();
        let limit = u64::from(description.max_time) * 1000;
        let stop = move || limit > 0 && crate::clock::unix_millis().saturating_sub(started) > limit;
        let original = solver::values_of(self.presentation(), self.current_slide, &model.variables);
        let model = ibis_doc_model::solver::Model { seed: started, ..model };
        let solution = solver::solve(self.presentation(), self.current_slide, &model, &stop);
        let message = text(status_id(solution.status)).to_string();
        let kept = matches!(solution.status, Status::Optimal | Status::Converged | Status::IterationLimit | Status::Stopped);
        if let Some(p) = self.wb_mut().panels.solver.as_mut() {
            p.message = message;
            p.found = kept.then_some((solution, original));
        }
        cx.notify();
    }

    /// ソルバーの解を保持 (`keep`) or 元の値に戻す; the model is saved either way.
    pub(crate) fn solver_finish(&mut self, keep: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let description = match self.solver_description(cx) {
            Ok(d) => d,
            Err(m) => return self.solver_message(m, cx),
        };
        let slide = self.current_slide;
        let sheet_name = self.presentation().workbook.as_ref().and_then(|w| w.sheets.iter().find(|s| s.slide == Some(slide)).map(|s| s.name.clone())).unwrap_or_default();
        let model = description.to_model().ok();
        let (found, report, sensitivity_report, limits_report) = match self.sheet_state().and_then(|s| s.panels.solver.as_ref()) {
            Some(p) => (p.found.clone(), p.report, p.sensitivity_report, p.limits_report),
            None => return,
        };
        let mut commands = Vec::new();
        if let (true, Some((solution, _)), Some(model)) = (keep, found.as_ref(), model.as_ref()) {
            let edits = model.variables.iter().zip(&solution.values).map(|(&(row, column), v)| CellEdit { row, column, input: CellInput::Value(CellValue::Number(*v)) }).collect();
            commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
        }
        commands.push(Command::Sheet(SheetCommand::SetSolverNames { slide, names: description.to_names(&sheet_name) }));
        if let (true, Some((solution, original)), Some(model)) = (keep, found.as_ref(), model.as_ref()) {
            if report {
                commands.push(self.solver_report_command(&sheet_name, model, &description, solution, original));
            }
            if sensitivity_report {
                commands.push(self.solver_sensitivity_report_command(&sheet_name, model, solution));
            }
            if limits_report {
                commands.push(self.solver_limits_report_command(&sheet_name, model, solution));
            }
        }
        if let Err(err) = self.document.execute(commands) {
            return self.solver_message(format("edit.refused", &[("error", &err)]), cx);
        }
        self.wb_mut().panels.solver = None;
        self.sync_shape_formulas(cx);
        cx.notify();
    }

    /// The text beside and above a cell, as Excel names it in a report.
    fn solver_cell_name(&self, (row, column): (u32, u32)) -> String {
        let Some((_, sheet)) = self.current_sheet() else { return String::new() };
        let text_at = |r: u32, c: u32| match sheet.cell(r, c).map(|x| &x.value) {
            Some(CellValue::Text(t)) => Some(t.clone()),
            _ => None,
        };
        let left = (0..column).rev().find_map(|c| text_at(row, c));
        let above = (0..row).rev().find_map(|r| text_at(r, column));
        match (left, above) {
            (Some(a), Some(b)) => format!("{a} {b}"),
            (Some(a), None) | (None, Some(a)) => a,
            _ => String::new(),
        }
    }

    fn solver_report_command(&self, sheet_name: &str, model: &ibis_doc_model::solver::Model, d: &Description, solution: &Solution, original: &[f64]) -> Command {
        let slide = self.current_slide;
        let mut cells: Vec<ReportCell> = Vec::new();
        let mut put = |row: u32, column: u32, value: CellValue, bold: bool| cells.push(ReportCell { row, column, value, bold });
        let t = |s: &str| CellValue::Text(s.to_string());
        let label = |id: &str| t(&text(id));
        let a1 = |(r, c): (u32, u32)| format!("${}${}", ibis_doc_model::cell_name(c, r).trim_end_matches(|ch: char| ch.is_ascii_digit()), r + 1);
        put(0, 0, label("xlsx.solver.report.title"), true);
        put(1, 0, t(&format("xlsx.solver.report.worksheet", &[("sheet", &sheet_name)])), false);
        // The objective.
        let goal = match d.goal {
            Goal::Max => text("xlsx.solver.goal-max"),
            Goal::Min => text("xlsx.solver.goal-min"),
            Goal::Value(v) => format("xlsx.solver.report.goal-value", &[("value", &number_text(v))]).into(),
        };
        put(3, 0, t(&format("xlsx.solver.report.objective-cell", &[("goal", &goal)])), true);
        for (c, id) in ["xlsx.solver.report.cell", "xlsx.solver.report.name", "xlsx.solver.report.original", "xlsx.solver.report.final"].iter().enumerate() {
            put(4, c as u32, label(id), true);
        }
        let before = solver::values_of(self.presentation(), slide, &[model.objective]);
        put(5, 0, t(&a1(model.objective)), false);
        put(5, 1, t(&self.solver_cell_name(model.objective)), false);
        put(5, 2, CellValue::Number(before[0]), false);
        put(5, 3, CellValue::Number(solution.objective), false);
        // The variables.
        put(7, 0, label("xlsx.solver.report.variable-cells"), true);
        for (c, id) in ["xlsx.solver.report.cell", "xlsx.solver.report.name", "xlsx.solver.report.original", "xlsx.solver.report.final", "xlsx.solver.report.integer"].iter().enumerate() {
            put(8, c as u32, label(id), true);
        }
        let integer: Vec<bool> = model.variables.iter().map(|v| model.constraints.iter().any(|c| matches!(c.rel, Rel::Int | Rel::Bin | Rel::Dif) && c.lhs.contains(v))).collect();
        for (i, &cell) in model.variables.iter().enumerate() {
            let r = 9 + i as u32;
            put(r, 0, t(&a1(cell)), false);
            put(r, 1, t(&self.solver_cell_name(cell)), false);
            put(r, 2, CellValue::Number(original[i]), false);
            put(r, 3, CellValue::Number(solution.values[i]), false);
            put(r, 4, label(if integer[i] { "xlsx.solver.report.integer" } else { "xlsx.solver.report.continuous" }), false);
        }
        // The constraints.
        let mut r = 10 + model.variables.len() as u32;
        put(r, 0, label("xlsx.solver.report.constraints"), true);
        r += 1;
        for (c, id) in ["xlsx.solver.report.cell", "xlsx.solver.report.name", "xlsx.solver.report.cell-value", "xlsx.solver.report.formula", "xlsx.solver.report.status", "xlsx.solver.report.slack"].iter().enumerate() {
            put(r, c as u32, label(id), true);
        }
        r += 1;
        for (k, c) in model.constraints.iter().enumerate() {
            let (lhs_text, rel, rhs_text) = &d.constraints[k];
            if !matches!(c.rel, Rel::Le | Rel::Eq | Rel::Ge) {
                continue;
            }
            let rhs_cells: Vec<(u32, u32)> = match &c.rhs {
                ibis_doc_model::solver::Rhs::Cells(cells) => cells.clone(),
                _ => Vec::new(),
            };
            let lhs_values = solver::values_with(self.presentation(), slide, &model.variables, &solution.values, &c.lhs);
            let rhs_values = solver::values_with(self.presentation(), slide, &model.variables, &solution.values, &rhs_cells);
            for (i, &cell) in c.lhs.iter().enumerate() {
                let rhs = match &c.rhs {
                    ibis_doc_model::solver::Rhs::Number(n) => *n,
                    _ => rhs_values.get(if rhs_cells.len() == 1 { 0 } else { i }).copied().unwrap_or(0.0),
                };
                let gap = lhs_values[i] - rhs;
                let slack = match c.rel {
                    ibis_doc_model::solver::Rel::Le => -gap,
                    ibis_doc_model::solver::Rel::Ge => gap,
                    _ => gap.abs(),
                };
                let binding = slack.abs() <= model.precision * (1.0 + rhs.abs());
                let symbol = text(relation_id(*rel));
                let rhs_shown = if rhs_text.trim().parse::<f64>().is_ok() { rhs_text.trim().to_string() } else { a1(rhs_cells.get(if rhs_cells.len() == 1 { 0 } else { i }).copied().unwrap_or((0, 0))) };
                let _ = lhs_text;
                put(r, 0, t(&a1(cell)), false);
                put(r, 1, t(&self.solver_cell_name(cell)), false);
                put(r, 2, CellValue::Number(lhs_values[i]), false);
                put(r, 3, t(&format!("{} {} {}", a1(cell), symbol, rhs_shown)), false);
                put(r, 4, label(if binding { "xlsx.solver.report.binding" } else { "xlsx.solver.report.not-binding" }), false);
                put(r, 5, CellValue::Number(if binding { 0.0 } else { slack }), false);
                r += 1;
            }
        }
        Command::Sheet(SheetCommand::CreateReportSheet { slide, name: text("xlsx.solver.report.title").to_string(), cells, widths: vec![(0, 90), (1, 150), (2, 90), (3, 110), (4, 90), (5, 70)] })
    }

    fn solver_sensitivity_report_command(&self, sheet_name: &str, model: &ibis_doc_model::solver::Model, solution: &Solution) -> Command {
        let slide = self.current_slide;
        let sens = solver::sensitivity(self.presentation(), slide, model, solution);
        let mut cells: Vec<ReportCell> = Vec::new();
        let mut put = |row: u32, column: u32, value: CellValue, bold: bool| cells.push(ReportCell { row, column, value, bold });
        let t = |s: &str| CellValue::Text(s.to_string());
        let label = |id: &str| t(&text(id));
        let num = |v: f64| if v.is_nan() { CellValue::Empty } else { CellValue::Number(v.clamp(-1e30, 1e30)) };
        let a1 = |(r, c): (u32, u32)| format!("${}${}", ibis_doc_model::cell_name(c, r).trim_end_matches(|ch: char| ch.is_ascii_digit()), r + 1);
        put(0, 0, label("xlsx.solver.sensitivity-report"), true);
        put(1, 0, t(&format("xlsx.solver.report.worksheet", &[("sheet", &sheet_name)])), false);
        put(3, 0, label("xlsx.solver.report.variable-cells"), true);
        for (c, id) in [
            "xlsx.solver.report.cell",
            "xlsx.solver.report.name",
            "xlsx.solver.report.final",
            "xlsx.solver.report.reduced-cost",
            "xlsx.solver.report.objective-coefficient",
            "xlsx.solver.report.allowable-increase",
            "xlsx.solver.report.allowable-decrease",
        ]
        .iter()
        .enumerate()
        {
            put(4, c as u32, label(id), true);
        }
        for (i, v) in sens.variables.iter().enumerate() {
            let r = 5 + i as u32;
            put(r, 0, t(&a1(v.cell)), false);
            put(r, 1, t(&self.solver_cell_name(v.cell)), false);
            put(r, 2, num(v.final_value), false);
            put(r, 3, num(v.reduced_cost), false);
            put(r, 4, num(v.objective_coefficient), false);
            put(r, 5, num(v.allowable_increase), false);
            put(r, 6, num(v.allowable_decrease), false);
        }
        let mut r = 6 + sens.variables.len() as u32;
        put(r, 0, label("xlsx.solver.report.constraints"), true);
        r += 1;
        for (c, id) in [
            "xlsx.solver.report.cell",
            "xlsx.solver.report.name",
            "xlsx.solver.report.final",
            "xlsx.solver.report.shadow-price",
            "xlsx.solver.report.constraint-rhs",
            "xlsx.solver.report.allowable-increase",
            "xlsx.solver.report.allowable-decrease",
        ]
        .iter()
        .enumerate()
        {
            put(r, c as u32, label(id), true);
        }
        r += 1;
        for c in &sens.constraints {
            put(r, 0, t(&a1(c.cell)), false);
            put(r, 1, t(&self.solver_cell_name(c.cell)), false);
            put(r, 2, num(c.final_value), false);
            put(r, 3, num(c.shadow_price), false);
            put(r, 4, num(c.constraint_rhs), false);
            put(r, 5, num(c.allowable_increase), false);
            put(r, 6, num(c.allowable_decrease), false);
            r += 1;
        }
        Command::Sheet(SheetCommand::CreateReportSheet {
            slide,
            name: text("xlsx.solver.sensitivity-report").to_string(),
            cells,
            widths: vec![(0, 90), (1, 130), (2, 90), (3, 95), (4, 95), (5, 95), (6, 95)],
        })
    }

    fn solver_limits_report_command(&self, sheet_name: &str, model: &ibis_doc_model::solver::Model, solution: &Solution) -> Command {
        let slide = self.current_slide;
        let lims = solver::limits(self.presentation(), slide, model, solution);
        let mut cells: Vec<ReportCell> = Vec::new();
        let mut put = |row: u32, column: u32, value: CellValue, bold: bool| cells.push(ReportCell { row, column, value, bold });
        let t = |s: &str| CellValue::Text(s.to_string());
        let label = |id: &str| t(&text(id));
        let opt_num = |v: Option<f64>| match v {
            Some(x) if x.is_finite() => CellValue::Number(x),
            _ => t("#N/A"),
        };
        let a1 = |(r, c): (u32, u32)| format!("${}${}", ibis_doc_model::cell_name(c, r).trim_end_matches(|ch: char| ch.is_ascii_digit()), r + 1);
        put(0, 0, label("xlsx.solver.limits-report"), true);
        put(1, 0, t(&format("xlsx.solver.report.worksheet", &[("sheet", &sheet_name)])), false);
        for (c, id) in ["xlsx.solver.report.cell", "xlsx.solver.report.name", "xlsx.solver.report.value"].iter().enumerate() {
            put(3, c as u32, label(id), true);
        }
        put(4, 0, t(&a1(model.objective)), false);
        put(4, 1, t(&self.solver_cell_name(model.objective)), false);
        put(4, 2, CellValue::Number(solution.objective), false);
        for (c, id) in [
            "xlsx.solver.report.cell",
            "xlsx.solver.report.name",
            "xlsx.solver.report.value",
            "xlsx.solver.report.lower-limit",
            "xlsx.solver.report.objective-result",
            "xlsx.solver.report.upper-limit",
            "xlsx.solver.report.objective-result",
        ]
        .iter()
        .enumerate()
        {
            put(6, c as u32, label(id), true);
        }
        for (i, l) in lims.iter().enumerate() {
            let r = 7 + i as u32;
            put(r, 0, t(&a1(l.cell)), false);
            put(r, 1, t(&self.solver_cell_name(l.cell)), false);
            put(r, 2, CellValue::Number(l.value), false);
            put(r, 3, opt_num(l.lower_limit), false);
            put(r, 4, opt_num(l.lower_objective), false);
            put(r, 5, opt_num(l.upper_limit), false);
            put(r, 6, opt_num(l.upper_objective), false);
        }
        Command::Sheet(SheetCommand::CreateReportSheet {
            slide,
            name: text("xlsx.solver.limits-report").to_string(),
            cells,
            widths: vec![(0, 90), (1, 130), (2, 90), (3, 90), (4, 110), (5, 90), (6, 110)],
        })
    }

    pub(crate) fn solver_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.solver.as_ref()?;
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
        let caption = |id: &str| div().text_size(px(13.)).child(text(id));
        let entity = cx.entity().downgrade();
        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.));
        body = body
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(caption("xlsx.solver.objective"))
                    .child(mark(
                        "use-objective",
                        Button::new("solver-use-objective")
                            .xsmall()
                            .ghost()
                            .label(text("xlsx.use-selected-cell"))
                            .on_click(cx.listener(|this, _, window, cx| this.set_solver_ref_from_selection("objective", window, cx)))
                            .into_any_element(),
                    )),
            )
            .child(mark("objective", Input::new(&p.objective).small().into_any_element()));
        let radio = |id: &'static str, on: bool, label: &'static str, kind: GoalKind| {
            mark(
                id,
                Button::new(id)
                    .small()
                    .ghost()
                    .label(format!("{} {}", if on { "◉" } else { "○" }, text(label)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.solver.as_mut() {
                            p.goal = kind;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        body = body.child(
            h_flex()
                .gap_1()
                .flex_wrap()
                .child(radio("goal-max", p.goal == GoalKind::Max, "xlsx.solver.goal-max", GoalKind::Max))
                .child(radio("goal-min", p.goal == GoalKind::Min, "xlsx.solver.goal-min", GoalKind::Min))
                .child(radio("goal-value", p.goal == GoalKind::Value, "xlsx.solver.goal-value", GoalKind::Value)),
        );
        if p.goal == GoalKind::Value {
            body = body.child(mark("goal-number", Input::new(&p.goal_value).small().into_any_element()));
        }
        body = body
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(caption("xlsx.solver.variables"))
                    .child(mark(
                        "use-variables",
                        Button::new("solver-use-variables")
                            .xsmall()
                            .ghost()
                            .label(text("xlsx.subtotal-use-selection"))
                            .on_click(cx.listener(|this, _, window, cx| this.set_solver_ref_from_selection("variables", window, cx)))
                            .into_any_element(),
                    )),
            )
            .child(mark("variables", Input::new(&p.variables).small().into_any_element()));

        // Constraints.
        body = body.child(caption("xlsx.solver.constraints"));
        let mut list = v_flex().gap_1().p(px(4.)).border_1().border_color(cx.theme().border).rounded_md().max_h(px(160.)).overflow_hidden();
        for (i, (lhs, rel, rhs)) in p.constraints.iter().enumerate() {
            let on = Some(i) == p.selected;
            let (l, r, rl) = (lhs.clone(), rhs.clone(), *rel);
            list = list.child(mark(
                &format!("constraint-{i}"),
                Button::new(SharedString::from(format!("solver-constraint-{i}")))
                    .small()
                    .ghost()
                    .child(div().px(px(6.)).text_size(px(12.)).bg(if on { cx.theme().accent.opacity(0.2) } else { hsla(0.0, 0.0, 0.0, 0.0) }).child(SharedString::from(format!("{lhs} {} {rhs}", text(relation_id(*rel))))))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(p) = this.wb_mut().panels.solver.as_mut() {
                            p.selected = Some(i);
                            p.c_rel = rl;
                            p.c_lhs.update(cx, |input, cx| input.set_value(l.clone(), window, cx));
                            p.c_rhs.update(cx, |input, cx| input.set_value(r.clone(), window, cx));
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        }
        body = body.child(list);
        body = body
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(caption("xlsx.solver.cell-reference"))
                    .child(mark(
                        "use-lhs",
                        Button::new("solver-use-lhs")
                            .xsmall()
                            .ghost()
                            .label(text("xlsx.subtotal-use-selection"))
                            .on_click(cx.listener(|this, _, window, cx| this.set_solver_ref_from_selection("c-lhs", window, cx)))
                            .into_any_element(),
                    )),
            )
            .child(mark("c-lhs", Input::new(&p.c_lhs).small().into_any_element()));
        body = body.child(mark(
            "c-rel",
            Button::new("solver-rel")
                .small()
                .label(text(relation_id(p.c_rel)))
                .dropdown_menu({
                    let entity = entity.clone();
                    move |menu, _, _| {
                        RELATIONS.iter().fold(menu, |menu, &r| {
                            let entity = entity.clone();
                            menu.item(PopupMenuItem::new(text(relation_id(r))).on_click(move |_, _, cx| {
                                entity
                                    .update(cx, |this, cx| {
                                        if let Some(p) = this.wb_mut().panels.solver.as_mut() {
                                            p.c_rel = r;
                                        }
                                        cx.notify();
                                    })
                                    .ok();
                            }))
                        })
                    }
                })
                .into_any_element(),
        ));
        body = body.child(caption("xlsx.solver.constraint-value")).child(mark("c-rhs", Input::new(&p.c_rhs).small().into_any_element()));
        body = body.child(
            h_flex()
                .gap_2()
                .child(mark("c-add", Button::new("solver-add").primary().small().label(text("xlsx.solver.add")).on_click(cx.listener(|this, _, _, cx| this.solver_put_constraint(false, cx))).into_any_element()))
                .child(mark("c-change", Button::new("solver-change").small().disabled(p.selected.is_none()).label(text("xlsx.solver.change")).on_click(cx.listener(|this, _, _, cx| this.solver_put_constraint(true, cx))).into_any_element()))
                .child(mark(
                    "c-delete",
                    Button::new("solver-delete")
                        .small()
                        .disabled(p.selected.is_none())
                        .label(text("xlsx.solver.delete"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.solver.as_mut()
                                && let Some(i) = p.selected.take()
                                && i < p.constraints.len()
                            {
                                p.constraints.remove(i);
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                )),
        );

        // Options.
        body = body.child(mark(
            "non-negative",
            super::toggle::toggle_row(
                "solver-non-negative",
                p.non_negative,
                text("xlsx.solver.non-negative"),
                cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.solver.as_mut() {
                        p.non_negative = !p.non_negative;
                    }
                    cx.notify();
                }),
            )
            .into_any_element(),
        ));
        body = body.child(caption("xlsx.solver.method")).child(mark(
            "method",
            Button::new("solver-method")
                .small()
                .label(text(method_id(p.method)))
                .dropdown_menu({
                    let entity = entity.clone();
                    move |menu, _, _| {
                        [Method::Grg, Method::Simplex, Method::Evolutionary].iter().fold(menu, |menu, &m| {
                            let entity = entity.clone();
                            menu.item(PopupMenuItem::new(text(method_id(m))).on_click(move |_, _, cx| {
                                entity
                                    .update(cx, |this, cx| {
                                        if let Some(p) = this.wb_mut().panels.solver.as_mut() {
                                            p.method = m;
                                        }
                                        cx.notify();
                                    })
                                    .ok();
                            }))
                        })
                    }
                })
                .into_any_element(),
        ));
        for (id, caption_id, entity) in [
            ("precision", "xlsx.solver.precision", &p.precision),
            ("convergence", "xlsx.solver.convergence", &p.convergence),
            ("integer-tolerance", "xlsx.solver.integer-tolerance", &p.integer_tolerance),
            ("max-iterations", "xlsx.solver.max-iterations", &p.max_iterations),
            ("max-time", "xlsx.solver.max-time", &p.max_time),
        ] {
            body = body.child(caption(caption_id)).child(mark(id, Input::new(entity).small().into_any_element()));
        }
        if !p.message.is_empty() {
            body = body.child(div().text_size(px(12.)).child(SharedString::from(p.message.clone())));
        }
        if p.found.is_some() {
            body = body.child(mark(
                "report",
                super::toggle::toggle_row(
                    "solver-report",
                    p.report,
                    text("xlsx.solver.answer-report"),
                    cx.listener(|this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.solver.as_mut() {
                            p.report = !p.report;
                        }
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ));
            body = body.child(mark(
                "sensitivity-report",
                super::toggle::toggle_row(
                    "solver-sensitivity-report",
                    p.sensitivity_report,
                    text("xlsx.solver.sensitivity-report"),
                    cx.listener(|this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.solver.as_mut() {
                            p.sensitivity_report = !p.sensitivity_report;
                        }
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ));
            body = body.child(mark(
                "limits-report",
                super::toggle::toggle_row(
                    "solver-limits-report",
                    p.limits_report,
                    text("xlsx.solver.limits-report"),
                    cx.listener(|this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.solver.as_mut() {
                            p.limits_report = !p.limits_report;
                        }
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ));
            body = body.child(
                h_flex()
                    .gap_2()
                    .child(mark("keep", Button::new("solver-keep").primary().small().label(text("xlsx.solver.keep")).on_click(cx.listener(|this, _, _, cx| this.solver_finish(true, cx))).into_any_element()))
                    .child(mark("restore", Button::new("solver-restore").small().label(text("xlsx.solver.restore")).on_click(cx.listener(|this, _, _, cx| this.solver_finish(false, cx))).into_any_element())),
            );
        } else {
            body = body.child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(mark("solve", Button::new("solver-solve").primary().small().label(text("xlsx.solver.solve")).on_click(cx.listener(|this, _, _, cx| this.solver_solve(cx))).into_any_element()))
                    .child(mark("reset", Button::new("solver-reset").small().label(text("xlsx.solver.reset-all")).on_click(cx.listener(|this, _, window, cx| this.solver_reset_all(window, cx))).into_any_element()))
                    .child(mark("close", Button::new("solver-close").small().label(text("_shell.close")).on_click(cx.listener(|this, _, _, cx| this.solver_finish(false, cx))).into_any_element())),
            );
        }
        Some(SidebarPanel { title: text("xlsx.solver.title"), body: body.into_any_element() })
    }
}

trait OrDefault<T> {
    fn ok_or_default(self) -> Result<T, String>;
}

impl<T> OrDefault<T> for Option<T> {
    fn ok_or_default(self) -> Result<T, String> {
        self.ok_or_else(|| text("xlsx.analysis.message.no-data").to_string())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_caption_is_in_both_languages() {
        for table in [include_str!("../../i18n/ja/xlsx.tsv"), include_str!("../../i18n/en/xlsx.tsv")] {
            for id in super::IDS {
                assert!(table.lines().any(|l| l.split('\t').next() == Some(*id)), "{id} is missing");
            }
        }
    }
}
