// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 数式の検証（Evaluate Formula）のためのステップ評価エンジン。
//!
//! Excel の「数式」>「ワークシート分析」>「数式の検証」(Evaluate Formula) を提供する。
//! 数式内の部分式を最も内側の演算から順に 1 ステップずつ評価（簡約）し、
//! 各ステップの文字列表現、次に評価される下線対象範囲（スパン）、
//! セル参照に対するステップイン（Step In）・ステップアウト（Step Out）、
//! やり直し（Restart）を管理する。

use crate::eval::{Area, Book, Ctx, Stored, Value, to_bool};
use crate::formula::{AreaRef, BinaryOp, Corner, Expr, ParseError, parse, write_reference};
use crate::value::{CellError, CellValue};

/// ステップ評価の現在の表示情報。
#[derive(Clone, Debug, PartialEq)]
pub struct StepInfo {
    /// 現在の数式テキスト（先頭の `=` 付き、最終結果の場合は `=` なし）。
    pub formula_text: String,
    /// 次に評価される部分式の文字範囲（開始オフセット, 終了オフセット）。
    /// 評価完了時は None。
    pub underline_range: Option<(usize, usize)>,
    /// 次の評価対象がセル参照であり、そのセルがさらに数式を持っているか（ステップイン可能）。
    pub can_step_in: bool,
    /// ステップイン対象のセル参照文字列（例: "A1" や "Sheet1!$B$3"）。
    pub step_in_target: Option<String>,
    /// 評価が最終結果に達したか。
    pub is_finished: bool,
    /// 最終評価値（完了時のみ Some）。
    pub result_value: Option<CellValue>,
    /// 現在の呼び出し深度（0 が最上位セル）。
    pub depth: usize,
}

/// ステップイン呼び出しスタックの 1 フレーム。
#[derive(Clone, Debug)]
pub struct StepFrame {
    pub sheet: usize,
    pub row: u32,
    pub column: u32,
    pub cell_name: String,
    pub original_formula: String,
    pub current_expr: Expr,
}

/// 数式のステップ評価セッション。
#[derive(Clone, Debug)]
pub struct EvaluateSession {
    /// ステップインの呼び出し履歴（親フレーム群）。
    pub stack: Vec<StepFrame>,
    /// 現在アクティブな評価フレーム。
    pub current: StepFrame,
}

/// 既約（リテラル値・これ以上簡約できない終端式）か判定する。
pub fn is_reduced(expr: &Expr) -> bool {
    matches!(expr, Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Missing)
}

/// 範囲集計関数か判定する（これらの関数の引数のセル参照は個別値に展開せず関数へ一括で渡す）。
pub fn is_range_aggregate_func(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "SUM" | "AVERAGE" | "COUNT" | "COUNTA" | "COUNTBLANK" | "MAX" | "MIN"
            | "MEDIAN" | "MODE" | "STDEV" | "STDEVP" | "VAR" | "VARP"
            | "PRODUCT" | "SUMPRODUCT" | "LARGE" | "SMALL" | "RANK"
            | "SUBTOTAL" | "AGGREGATE"
    )
}

/// 木構造内のノードパス（子のインデックス列）。
pub type NodePath = Vec<usize>;

impl EvaluateSession {
    /// 新しいステップ評価セッションを開始する。
    pub fn new(book: &dyn Book, sheet: usize, row: u32, column: u32) -> Option<Self> {
        let formula = book.formula(sheet, row, column)?;
        let parsed = parse(formula.trim_start_matches('=')).ok()?;
        let sheet_name = book.sheet_name(sheet);
        let cell_name = format!("{}!{}", sheet_name, cell_ref_name(column, row));
        Some(Self {
            stack: Vec::new(),
            current: StepFrame {
                sheet,
                row,
                column,
                cell_name,
                original_formula: formula.to_string(),
                current_expr: parsed,
            },
        })
    }

    /// 数式文字列を直接指定してセッションを開始する（任意セルの評価用）。
    pub fn with_formula(book: &dyn Book, sheet: usize, row: u32, column: u32, formula: &str) -> Result<Self, ParseError> {
        let parsed = parse(formula.trim_start_matches('='))?;
        let sheet_name = book.sheet_name(sheet);
        let cell_name = format!("{}!{}", sheet_name, cell_ref_name(column, row));
        Ok(Self {
            stack: Vec::new(),
            current: StepFrame {
                sheet,
                row,
                column,
                cell_name,
                original_formula: formula.to_string(),
                current_expr: parsed,
            },
        })
    }

    /// 最初からやり直す（Restart）。
    pub fn restart(&mut self) -> Result<(), ParseError> {
        self.stack.clear();
        let parsed = parse(self.current.original_formula.trim_start_matches('='))?;
        self.current.current_expr = parsed;
        Ok(())
    }

    /// 現在のセッション状態情報を取得する。
    pub fn info(&self, book: &dyn Book) -> StepInfo {
        if is_reduced(&self.current.current_expr) {
            let val = expr_to_cell_value(&self.current.current_expr);
            let text = cell_value_to_display(&val);
            return StepInfo {
                formula_text: text,
                underline_range: None,
                can_step_in: false,
                step_in_target: None,
                is_finished: true,
                result_value: Some(val),
                depth: self.stack.len(),
            };
        }

        let next_path = find_next_redex(&self.current.current_expr, false);
        let mut formatter = ExprFormatter::new(next_path.as_deref());
        formatter.format(&self.current.current_expr);
        let formula_text = format!("={}", formatter.text);
        let underline_range = formatter.target_span.map(|(s, e)| (s + 1, e + 1));

        // 次の簡約対象ノードがセル参照で、数式を持つか判定（ステップイン可否）
        let mut can_step_in = false;
        let mut step_in_target = None;
        if let Some(path) = &next_path {
            if let Some(target_expr) = get_node_by_path(&self.current.current_expr, path) {
                if let Expr::Ref(r) = target_expr {
                    if r.is_cell() {
                        let target_sheet = r.sheet.as_ref()
                            .and_then(|s| book.sheet_index(&s.first))
                            .unwrap_or(self.current.sheet);
                        let target_row = r.start.row;
                        let target_col = r.start.column;
                        let ref_str = write_reference(r);
                        step_in_target = Some(ref_str);
                        if book.formula(target_sheet, target_row, target_col).is_some() {
                            can_step_in = true;
                        }
                    }
                }
            }
        }

        StepInfo {
            formula_text,
            underline_range,
            can_step_in,
            step_in_target,
            is_finished: false,
            result_value: None,
            depth: self.stack.len(),
        }
    }

    /// 1 ステップ評価（簡約）を実行する。
    pub fn step(&mut self, book: &dyn Book) -> bool {
        if is_reduced(&self.current.current_expr) {
            // もし子フレームで完了したなら、親フレームへ自動復帰
            if let Some(parent) = self.stack.pop() {
                let child_val = expr_to_value(&self.current.current_expr);
                self.current = parent;
                // 親フレームの next redex を子フレームの結果で置換
                if let Some(path) = find_next_redex(&self.current.current_expr, false) {
                    replace_node_by_path(&mut self.current.current_expr, &path, value_to_expr(child_val));
                }
                return true;
            }
            return false;
        }

        self.step_current_frame(book)
    }

    /// カレントフレームのみを 1 ステップ簡約する（スタック操作を行わない）。
    pub fn step_current_frame(&mut self, book: &dyn Book) -> bool {
        if is_reduced(&self.current.current_expr) {
            return false;
        }

        let Some(path) = find_next_redex(&self.current.current_expr, false) else {
            return false;
        };

        let reduced = reduce_node(book, self.current.sheet, self.current.row, self.current.column, &self.current.current_expr, &path);
        replace_node_by_path(&mut self.current.current_expr, &path, reduced);
        true
    }

    /// 参照先セルへステップインする。
    pub fn step_in(&mut self, book: &dyn Book) -> bool {
        let Some(path) = find_next_redex(&self.current.current_expr, false) else { return false };
        let Some(target_expr) = get_node_by_path(&self.current.current_expr, &path) else { return false };
        let Expr::Ref(r) = target_expr else { return false };
        if !r.is_cell() { return false; }

        let target_sheet = r.sheet.as_ref()
            .and_then(|s| book.sheet_index(&s.first))
            .unwrap_or(self.current.sheet);
        let target_row = r.start.row;
        let target_col = r.start.column;

        let Some(sub_formula) = book.formula(target_sheet, target_row, target_col) else { return false };
        let Ok(sub_expr) = parse(sub_formula.trim_start_matches('=')) else { return false };

        let sheet_name = book.sheet_name(target_sheet);
        let cell_name = format!("{}!{}", sheet_name, cell_ref_name(target_col, target_row));

        let parent_frame = self.current.clone();
        self.stack.push(parent_frame);
        self.current = StepFrame {
            sheet: target_sheet,
            row: target_row,
            column: target_col,
            cell_name,
            original_formula: sub_formula.to_string(),
            current_expr: sub_expr,
        };
        true
    }

    /// ステップアウト（子セルの評価を最後まで実行して親フレームへ戻る）。
    pub fn step_out(&mut self, book: &dyn Book) -> bool {
        if self.stack.is_empty() { return false; }
        // 現在の子フレームを最後まで評価（安全のため上限 10,000 回）
        let mut count = 0;
        while !is_reduced(&self.current.current_expr) && count < 10000 {
            if !self.step_current_frame(book) { break; }
            count += 1;
        }
        let child_val = expr_to_value(&self.current.current_expr);
        let parent = self.stack.pop().unwrap();
        self.current = parent;
        if let Some(path) = find_next_redex(&self.current.current_expr, false) {
            replace_node_by_path(&mut self.current.current_expr, &path, value_to_expr(child_val));
        }
        true
    }
}

/// 次に評価すべきノードのパス（Next Redex）を見つける。
/// 最も内側の部分式を左から右へ探索する。
fn find_next_redex(expr: &Expr, inside_range_func: bool) -> Option<NodePath> {
    if is_reduced(expr) {
        return None;
    }

    match expr {
        Expr::Ref(_) => {
            if inside_range_func {
                None
            } else {
                Some(Vec::new())
            }
        }
        Expr::Name { .. } | Expr::Structured { .. } => Some(Vec::new()),
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) => {
            if let Some(mut p) = find_next_redex(x, false) {
                p.insert(0, 0);
                Some(p)
            } else {
                Some(Vec::new())
            }
        }
        Expr::Binary(_, a, b) => {
            if let Some(mut p) = find_next_redex(a, false) {
                p.insert(0, 0);
                Some(p)
            } else if let Some(mut p) = find_next_redex(b, false) {
                p.insert(0, 1);
                Some(p)
            } else {
                Some(Vec::new())
            }
        }
        Expr::Call { name, args } => {
            if name.eq_ignore_ascii_case("IF") && !args.is_empty() {
                // IF の場合は第 1 引数（条件）をまず簡約
                if let Some(mut p) = find_next_redex(&args[0], false) {
                    p.insert(0, 0);
                    return Some(p);
                }
                // 条件が簡約済みなら IF 自身を簡約（選択された枝に置換）
                return Some(Vec::new());
            }

            let is_agg = is_range_aggregate_func(name);
            for (i, arg) in args.iter().enumerate() {
                if let Some(mut p) = find_next_redex(arg, is_agg) {
                    p.insert(0, i);
                    return Some(p);
                }
            }
            Some(Vec::new())
        }
        Expr::Range(a, b) | Expr::Intersect(a, b) => {
            if let Some(mut p) = find_next_redex(a, false) {
                p.insert(0, 0);
                Some(p)
            } else if let Some(mut p) = find_next_redex(b, false) {
                p.insert(0, 1);
                Some(p)
            } else {
                Some(Vec::new())
            }
        }
        Expr::Spill(x) | Expr::Implicit(x) => {
            if let Some(mut p) = find_next_redex(x, false) {
                p.insert(0, 0);
                Some(p)
            } else {
                Some(Vec::new())
            }
        }
        Expr::Invoke { callee, args } => {
            if let Some(mut p) = find_next_redex(callee, false) {
                p.insert(0, 0);
                Some(p)
            } else {
                for (i, arg) in args.iter().enumerate() {
                    if let Some(mut p) = find_next_redex(arg, false) {
                        p.insert(0, i + 1);
                        return Some(p);
                    }
                }
                Some(Vec::new())
            }
        }
        Expr::Array(_) | Expr::Union(_) => Some(Vec::new()),
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Missing => None,
    }
}

/// 指定パスのノードを取り出す。
fn get_node_by_path<'a>(expr: &'a Expr, path: &[usize]) -> Option<&'a Expr> {
    if path.is_empty() {
        return Some(expr);
    }
    match expr {
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) | Expr::Spill(x) | Expr::Implicit(x) => {
            if path[0] == 0 { get_node_by_path(x, &path[1..]) } else { None }
        }
        Expr::Binary(_, a, b) | Expr::Range(a, b) | Expr::Intersect(a, b) => {
            if path[0] == 0 {
                get_node_by_path(a, &path[1..])
            } else if path[0] == 1 {
                get_node_by_path(b, &path[1..])
            } else {
                None
            }
        }
        Expr::Call { args, .. } => {
            args.get(path[0]).and_then(|arg| get_node_by_path(arg, &path[1..]))
        }
        Expr::Invoke { callee, args } => {
            if path[0] == 0 {
                get_node_by_path(callee, &path[1..])
            } else {
                args.get(path[0] - 1).and_then(|arg| get_node_by_path(arg, &path[1..]))
            }
        }
        _ => None,
    }
}

/// 指定パスのノードを置換する。
fn replace_node_by_path(expr: &mut Expr, path: &[usize], replacement: Expr) {
    if path.is_empty() {
        *expr = replacement;
        return;
    }
    match expr {
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) | Expr::Spill(x) | Expr::Implicit(x) => {
            if path[0] == 0 { replace_node_by_path(x, &path[1..], replacement); }
        }
        Expr::Binary(_, a, b) | Expr::Range(a, b) | Expr::Intersect(a, b) => {
            if path[0] == 0 {
                replace_node_by_path(a, &path[1..], replacement);
            } else if path[0] == 1 {
                replace_node_by_path(b, &path[1..], replacement);
            }
        }
        Expr::Call { args, .. } => {
            if let Some(arg) = args.get_mut(path[0]) {
                replace_node_by_path(arg, &path[1..], replacement);
            }
        }
        Expr::Invoke { callee, args } => {
            if path[0] == 0 {
                replace_node_by_path(callee, &path[1..], replacement);
            } else if let Some(arg) = args.get_mut(path[0] - 1) {
                replace_node_by_path(arg, &path[1..], replacement);
            }
        }
        _ => {}
    }
}

/// 対象パスのノードを 1 ステップ簡約して新しい Expr を返す。
fn reduce_node(book: &dyn Book, sheet: usize, row: u32, column: u32, root_expr: &Expr, path: &[usize]) -> Expr {
    let Some(target) = get_node_by_path(root_expr, path) else {
        return Expr::Error(CellError::Value);
    };

    // IF 関数の短絡評価の特別処理
    if let Expr::Call { name, args } = target {
        if name.eq_ignore_ascii_case("IF") && !args.is_empty() {
            let cond_val = expr_to_value(&args[0]);
            let cond_bool = to_bool(&cond_val).unwrap_or(false);
            if cond_bool {
                return args.get(1).cloned().unwrap_or(Expr::Bool(true));
            } else {
                return args.get(2).cloned().unwrap_or(Expr::Bool(false));
            }
        }
    }

    let mut ctx = Ctx::new(book, &Stored, sheet, row, column);

    // セル参照の簡約: 単一セル参照はセルの値に展開する
    if let Expr::Ref(r) = target {
        let target_sheet = r.sheet.as_ref()
            .and_then(|s| ctx.resolve_sheet(Some(s)).ok().map(|(sh, _)| sh))
            .unwrap_or(sheet);
        if r.is_cell() {
            let cell_val = ctx.cell_value(target_sheet, r.start.row, r.start.column);
            return value_to_expr(cell_val);
        } else {
            let area = Area {
                sheet: target_sheet,
                last_sheet: target_sheet,
                first_row: r.start.row,
                first_column: r.start.column,
                last_row: r.end.row,
                last_column: r.end.column,
            };
            let deref = ctx.deref_implicit(&[area]);
            if !matches!(deref, Value::Error(CellError::Value)) {
                return value_to_expr(deref);
            }
        }
    }

    // 通常の評価
    let val = ctx.eval(target);
    value_to_expr(val)
}

/// Value を Expr に変換する。
fn value_to_expr(v: Value) -> Expr {
    match v {
        Value::Number(n) => Expr::Number(n),
        Value::Text(t) => Expr::Text(t),
        Value::Bool(b) => Expr::Bool(b),
        Value::Error(e) => Expr::Error(e),
        Value::Empty | Value::Omitted => Expr::Number(0.0),
        Value::Ref(areas) => {
            if let Some(area) = areas.first() {
                Expr::Ref(AreaRef {
                    sheet: None,
                    start: Corner { row: area.first_row, column: area.first_column, row_fixed: false, column_fixed: false },
                    end: Corner { row: area.last_row, column: area.last_column, row_fixed: false, column_fixed: false },
                    whole_columns: false,
                    whole_rows: false,
                })
            } else {
                Expr::Error(CellError::Value)
            }
        }
        Value::Array(a) => {
            if let Some(first) = a.data.first() {
                value_to_expr(first.clone())
            } else {
                Expr::Error(CellError::Value)
            }
        }
        Value::Lambda(_) => Expr::Error(CellError::Calc),
    }
}

/// Expr から Value への変換（リテラルのみ）。
fn expr_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Number(n) => Value::Number(*n),
        Expr::Text(t) => Value::Text(t.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Error(e) => Value::Error(*e),
        _ => Value::Error(CellError::Value),
    }
}

/// Expr から CellValue への変換。
fn expr_to_cell_value(expr: &Expr) -> CellValue {
    match expr {
        Expr::Number(n) => CellValue::Number(*n),
        Expr::Text(t) => CellValue::Text(t.clone()),
        Expr::Bool(b) => CellValue::Bool(*b),
        Expr::Error(e) => CellValue::Error(*e),
        _ => CellValue::Error(CellError::Value),
    }
}

/// CellValue の表示用文字列。
fn cell_value_to_display(v: &CellValue) -> String {
    match v {
        CellValue::Empty => String::new(),
        CellValue::Number(n) => crate::eval::convert::number_to_text(*n),
        CellValue::Text(t) => t.clone(),
        CellValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
        CellValue::Error(e) => e.to_string(),
    }
}

/// セル名（例: $A$1）。
fn cell_ref_name(col: u32, row: u32) -> String {
    let letters = crate::formula::write_reference(&AreaRef {
        sheet: None,
        start: Corner { row, column: col, row_fixed: true, column_fixed: true },
        end: Corner { row, column: col, row_fixed: true, column_fixed: true },
        whole_columns: false,
        whole_rows: false,
    });
    letters
}

// ----- 式フォーマッター & スパン検出器 -----------------------------

struct ExprFormatter<'a> {
    target_path: Option<&'a [usize]>,
    current_path: Vec<usize>,
    pub text: String,
    pub target_span: Option<(usize, usize)>,
}

impl<'a> ExprFormatter<'a> {
    fn new(target_path: Option<&'a [usize]>) -> Self {
        Self {
            target_path,
            current_path: Vec::new(),
            text: String::new(),
            target_span: None,
        }
    }

    fn check_target_start(&mut self) -> usize {
        let start = self.text.len();
        if self.target_path == Some(&self.current_path[..]) {
            self.target_span = Some((start, start));
        }
        start
    }

    fn check_target_end(&mut self, start: usize) {
        if self.target_path == Some(&self.current_path[..]) {
            self.target_span = Some((start, self.text.len()));
        }
    }

    fn format(&mut self, expr: &Expr) {
        let start = self.check_target_start();
        self.format_inner(expr, 0);
        self.check_target_end(start);
    }

    fn format_inner(&mut self, expr: &Expr, parent_prec: u8) {
        match expr {
            Expr::Number(n) => {
                self.text.push_str(&crate::eval::convert::number_to_text(*n));
            }
            Expr::Text(t) => {
                self.text.push('"');
                self.text.push_str(&t.replace('"', "\"\""));
                self.text.push('"');
            }
            Expr::Bool(b) => {
                self.text.push_str(if *b { "TRUE" } else { "FALSE" });
            }
            Expr::Error(e) => {
                self.text.push_str(&e.to_string());
            }
            Expr::Missing => {}
            Expr::Ref(r) => {
                self.text.push_str(&write_reference(r));
            }
            Expr::Name { sheet, name } => {
                if let Some(s) = sheet {
                    self.text.push_str(&s.first);
                    self.text.push('!');
                }
                self.text.push_str(name);
            }
            Expr::Structured { table, spec } => {
                self.text.push_str(table);
                self.text.push_str(spec);
            }
            Expr::Call { name, args } => {
                self.text.push_str(name);
                self.text.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.text.push_str(", ");
                    }
                    self.current_path.push(i);
                    let start = self.check_target_start();
                    self.format_inner(arg, 0);
                    self.check_target_end(start);
                    self.current_path.pop();
                }
                self.text.push(')');
            }
            Expr::Invoke { callee, args } => {
                self.current_path.push(0);
                let start = self.check_target_start();
                self.format_inner(callee, 0);
                self.check_target_end(start);
                self.current_path.pop();

                self.text.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { self.text.push_str(", "); }
                    self.current_path.push(i + 1);
                    let start = self.check_target_start();
                    self.format_inner(arg, 0);
                    self.check_target_end(start);
                    self.current_path.pop();
                }
                self.text.push(')');
            }
            Expr::Neg(x) => {
                self.text.push('-');
                self.current_path.push(0);
                let start = self.check_target_start();
                self.format_inner(x, 7);
                self.check_target_end(start);
                self.current_path.pop();
            }
            Expr::Plus(x) => {
                self.text.push('+');
                self.current_path.push(0);
                let start = self.check_target_start();
                self.format_inner(x, 7);
                self.check_target_end(start);
                self.current_path.pop();
            }
            Expr::Percent(x) => {
                self.current_path.push(0);
                let start = self.check_target_start();
                self.format_inner(x, 7);
                self.check_target_end(start);
                self.current_path.pop();
                self.text.push('%');
            }
            Expr::Binary(op, a, b) => {
                let prec = op_precedence(*op);
                let need_paren = prec < parent_prec;
                if need_paren { self.text.push('('); }

                self.current_path.push(0);
                let start_a = self.check_target_start();
                self.format_inner(a, prec);
                self.check_target_end(start_a);
                self.current_path.pop();

                self.text.push(' ');
                self.text.push_str(op_symbol(*op));
                self.text.push(' ');

                self.current_path.push(1);
                let start_b = self.check_target_start();
                self.format_inner(b, prec + 1); // 右結合性/左優先のため +1
                self.check_target_end(start_b);
                self.current_path.pop();

                if need_paren { self.text.push(')'); }
            }
            Expr::Range(a, b) => {
                self.format_inner(a, 8);
                self.text.push(':');
                self.format_inner(b, 8);
            }
            Expr::Intersect(a, b) => {
                self.format_inner(a, 8);
                self.text.push(' ');
                self.format_inner(b, 8);
            }
            Expr::Union(items) => {
                for (i, it) in items.iter().enumerate() {
                    if i > 0 { self.text.push_str(", "); }
                    self.format_inner(it, 0);
                }
            }
            Expr::Spill(x) => {
                self.format_inner(x, 8);
                self.text.push('#');
            }
            Expr::Implicit(x) => {
                self.text.push('@');
                self.format_inner(x, 8);
            }
            Expr::Array(rows) => {
                self.text.push('{');
                for (r, row) in rows.iter().enumerate() {
                    if r > 0 { self.text.push(';'); }
                    for (c, item) in row.iter().enumerate() {
                        if c > 0 { self.text.push(','); }
                        self.format_inner(item, 0);
                    }
                }
                self.text.push('}');
            }
        }
    }
}

fn op_precedence(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Pow => 6,
        BinaryOp::Mul | BinaryOp::Div => 5,
        BinaryOp::Add | BinaryOp::Sub => 4,
        BinaryOp::Concat => 3,
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 2,
    }
}

fn op_symbol(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Pow => "^",
        BinaryOp::Concat => "&",
        BinaryOp::Eq => "=",
        BinaryOp::Ne => "<>",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::Book;
    use std::collections::HashMap;

    struct MockBook {
        sheets: Vec<String>,
        cells: HashMap<(usize, u32, u32), (CellValue, Option<String>)>,
    }

    impl MockBook {
        fn new() -> Self {
            Self {
                sheets: vec!["Sheet1".into(), "Sheet2".into()],
                cells: HashMap::new(),
            }
        }
        fn set_value(&mut self, sheet: usize, row: u32, col: u32, val: CellValue) {
            self.cells.entry((sheet, row, col)).or_insert((CellValue::Empty, None)).0 = val;
        }
        fn set_formula(&mut self, sheet: usize, row: u32, col: u32, formula: &str) {
            self.cells.entry((sheet, row, col)).or_insert((CellValue::Empty, None)).1 = Some(formula.into());
        }
    }

    impl Book for MockBook {
        fn sheet_count(&self) -> usize { self.sheets.len() }
        fn sheet_name(&self, sheet: usize) -> &str { &self.sheets[sheet] }
        fn value(&self, sheet: usize, row: u32, col: u32) -> CellValue {
            self.cells.get(&(sheet, row, col)).map(|(v, _)| v.clone()).unwrap_or(CellValue::Empty)
        }
        fn formula(&self, sheet: usize, row: u32, col: u32) -> Option<&str> {
            self.cells.get(&(sheet, row, col)).and_then(|(_, f)| f.as_deref())
        }
        fn for_each_cell(&self, sheet: usize, area: (u32, u32, u32, u32), f: &mut dyn FnMut(u32, u32, &CellValue)) {
            let (r1, c1, r2, c2) = area;
            for r in r1..=r2 {
                for c in c1..=c2 {
                    if let Some((v, _)) = self.cells.get(&(sheet, r, c)) {
                        f(r, c, v);
                    }
                }
            }
        }
        fn extent(&self, _sheet: usize) -> (u32, u32) { (100, 100) }
        fn defined_name(&self, _name: &str, _sheet: Option<usize>) -> Option<String> { None }
    }

    #[test]
    fn test_step_arithmetic() {
        let book = MockBook::new();
        let mut session = EvaluateSession::with_formula(&book, 0, 0, 0, "=2 + 3 * 4").unwrap();

        // Step 0: 3 * 4 is target
        let info = session.info(&book);
        assert_eq!(info.formula_text, "=2 + 3 * 4");
        assert_eq!(info.underline_range, Some((5, 10))); // "3 * 4"
        assert!(!info.is_finished);

        // Step 1: 3 * 4 -> 12
        assert!(session.step(&book));
        let info = session.info(&book);
        assert_eq!(info.formula_text, "=2 + 12");
        assert_eq!(info.underline_range, Some((1, 7))); // "2 + 12"
        assert!(!info.is_finished);

        // Step 2: 2 + 12 -> 14
        assert!(session.step(&book));
        let info = session.info(&book);
        assert_eq!(info.formula_text, "14");
        assert_eq!(info.underline_range, None);
        assert!(info.is_finished);
        assert_eq!(info.result_value, Some(CellValue::Number(14.0)));
    }

    #[test]
    fn test_step_cell_references_and_functions() {
        let mut book = MockBook::new();
        book.set_value(0, 0, 0, CellValue::Number(10.0)); // A1 = 10
        book.set_value(0, 1, 0, CellValue::Number(20.0)); // A2 = 20

        let mut session = EvaluateSession::with_formula(&book, 0, 2, 0, "=A1 + A2").unwrap();

        // Step 0: A1
        let info = session.info(&book);
        assert_eq!(info.formula_text, "=A1 + A2");
        assert_eq!(info.underline_range, Some((1, 3))); // "A1"

        // Step 1: A1 -> 10
        assert!(session.step(&book));
        let info = session.info(&book);
        assert_eq!(info.formula_text, "=10 + A2");
        assert_eq!(info.underline_range, Some((6, 8))); // "A2"

        // Step 2: A2 -> 20
        assert!(session.step(&book));
        let info = session.info(&book);
        assert_eq!(info.formula_text, "=10 + 20");
        assert_eq!(info.underline_range, Some((1, 8))); // "10 + 20"

        // Step 3: 30
        assert!(session.step(&book));
        let info = session.info(&book);
        assert_eq!(info.formula_text, "30");
        assert!(info.is_finished);
        assert_eq!(info.result_value, Some(CellValue::Number(30.0)));
    }

    #[test]
    fn test_step_in_and_step_out() {
        let mut book = MockBook::new();
        book.set_value(0, 0, 1, CellValue::Number(5.0)); // B1 = 5
        book.set_formula(0, 0, 0, "=B1 * 2"); // A1 = =B1 * 2
        book.set_value(0, 0, 0, CellValue::Number(10.0));

        let mut session = EvaluateSession::with_formula(&book, 0, 2, 0, "=A1 + 1").unwrap();

        // Initial on A1
        let info = session.info(&book);
        assert_eq!(info.formula_text, "=A1 + 1");
        assert!(info.can_step_in);
        assert_eq!(info.step_in_target, Some("A1".into()));

        // Step In into A1
        assert!(session.step_in(&book));
        let info = session.info(&book);
        assert_eq!(info.depth, 1);
        assert_eq!(info.formula_text, "=B1 * 2");

        // Step Out back to parent
        assert!(session.step_out(&book));
        let info = session.info(&book);
        assert_eq!(info.depth, 0);
        assert_eq!(info.formula_text, "=10 + 1");

        // Step to finish
        assert!(session.step(&book));
        let info = session.info(&book);
        assert_eq!(info.formula_text, "11");
        assert!(info.is_finished);
    }

    #[test]
    fn test_step_if_and_sum() {
        let mut book = MockBook::new();
        book.set_value(0, 0, 0, CellValue::Number(10.0)); // A1 = 10
        book.set_value(0, 1, 0, CellValue::Number(20.0)); // A2 = 20

        // Test SUM(A1:A2) + 5 -> 30 + 5 -> 35
        let mut session = EvaluateSession::with_formula(&book, 0, 2, 0, "=SUM(A1:A2) + 5").unwrap();
        let info = session.info(&book);
        assert_eq!(info.formula_text, "=SUM(A1:A2) + 5");
        assert_eq!(info.underline_range, Some((1, 11))); // "SUM(A1:A2)"

        assert!(session.step(&book));
        let info = session.info(&book);
        assert_eq!(info.formula_text, "=30 + 5");

        assert!(session.step(&book));
        let info = session.info(&book);
        assert_eq!(info.formula_text, "35");
        assert!(info.is_finished);

        // Test IF(A1 > 5, 100, 200)
        let mut session_if = EvaluateSession::with_formula(&book, 0, 3, 0, "=IF(A1 > 5, 100, 200)").unwrap();
        let info = session_if.info(&book);
        assert_eq!(info.formula_text, "=IF(A1 > 5, 100, 200)");
        assert_eq!(info.underline_range, Some((4, 6))); // "A1"

        assert!(session_if.step(&book)); // A1 -> 10
        let info = session_if.info(&book);
        assert_eq!(info.formula_text, "=IF(10 > 5, 100, 200)");
        assert_eq!(info.underline_range, Some((4, 10))); // "10 > 5"

        assert!(session_if.step(&book)); // 10 > 5 -> TRUE
        let info = session_if.info(&book);
        assert_eq!(info.formula_text, "=IF(TRUE, 100, 200)");

        assert!(session_if.step(&book)); // IF(TRUE, 100, 200) -> 100
        let info = session_if.info(&book);
        assert_eq!(info.formula_text, "100");
        assert!(info.is_finished);
    }
}
