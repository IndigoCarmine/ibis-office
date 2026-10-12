// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What a cell holds: a number, text, a logical value, an error or nothing.

use std::fmt;

/// An error value, as Excel shows it (`#DIV/0!`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CellError {
    Null,
    Div0,
    Value,
    Ref,
    Name,
    Num,
    NA,
    GettingData,
    Spill,
    Calc,
    Field,
    Blocked,
    Connect,
    Busy,
    Unknown,
    External,
    Python,
}

impl CellError {
    pub const ALL: [CellError; 17] = [
        Self::Null,
        Self::Div0,
        Self::Value,
        Self::Ref,
        Self::Name,
        Self::Num,
        Self::NA,
        Self::GettingData,
        Self::Spill,
        Self::Calc,
        Self::Field,
        Self::Blocked,
        Self::Connect,
        Self::Busy,
        Self::Unknown,
        Self::External,
        Self::Python,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Null => "#NULL!",
            Self::Div0 => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Ref => "#REF!",
            Self::Name => "#NAME?",
            Self::Num => "#NUM!",
            Self::NA => "#N/A",
            Self::GettingData => "#GETTING_DATA",
            Self::Spill => "#SPILL!",
            Self::Calc => "#CALC!",
            Self::Field => "#FIELD!",
            Self::Blocked => "#BLOCKED!",
            Self::Connect => "#CONNECT!",
            Self::Busy => "#BUSY!",
            Self::Unknown => "#UNKNOWN!",
            Self::External => "#EXTERNAL!",
            Self::Python => "#PYTHON!",
        }
    }

    /// The error written as `text` (case-insensitive, as Excel accepts it).
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.as_str().eq_ignore_ascii_case(text))
    }

    /// `ERROR.TYPE`'s number.
    pub fn type_number(self) -> Option<f64> {
        Some(match self {
            Self::Null => 1.0,
            Self::Div0 => 2.0,
            Self::Value => 3.0,
            Self::Ref => 4.0,
            Self::Name => 5.0,
            Self::Num => 6.0,
            Self::NA => 7.0,
            Self::GettingData => 8.0,
            Self::Spill => 9.0,
            Self::Connect => 10.0,
            Self::Blocked => 11.0,
            Self::Unknown => 12.0,
            Self::Field => 13.0,
            Self::Calc => 14.0,
            Self::External => 19.0,
            Self::Busy => 17.0,
            Self::Python => return None,
        })
    }
}

impl fmt::Display for CellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A cell's value. Numbers compare by their bits, so that a model holding
/// them can be compared exactly (undo must give back the very same value).
#[derive(Clone, Debug, Default)]
pub enum CellValue {
    #[default]
    Empty,
    Number(f64),
    Text(String),
    Bool(bool),
    Error(CellError),
}

impl PartialEq for CellValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) => true,
            (Self::Number(a), Self::Number(b)) => a.to_bits() == b.to_bits(),
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Error(a), Self::Error(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for CellValue {}

impl std::hash::Hash for CellValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Empty => {}
            Self::Number(n) => n.to_bits().hash(state),
            Self::Text(t) => t.hash(state),
            Self::Bool(b) => b.hash(state),
            Self::Error(e) => e.hash(state),
        }
    }
}

impl CellValue {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }
}

/// A number as SpreadsheetML writes it in `c/v`: the shortest text that
/// reads back as the same `f64` (Excel writes up to 17 significant digits,
/// which read back the same).
pub fn number_to_xml(n: f64) -> String {
    if n == 0.0 {
        return "0".into();
    }
    let plain = format!("{n}");
    // Very large or small numbers in exponent form, as Excel writes them
    // (`1E+20`, `1.5E-7`), rather than a long run of zeros.
    let abs = n.abs();
    if !(1e-6..1e20).contains(&abs) {
        let e = format!("{n:E}");
        if let Some((mantissa, exp)) = e.split_once('E') {
            let exp: i32 = exp.parse().unwrap_or(0);
            return format!("{mantissa}E{}{}", if exp < 0 { "-" } else { "+" }, exp.abs());
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_round_trip() {
        for e in CellError::ALL {
            assert_eq!(CellError::parse(e.as_str()), Some(e));
        }
        assert_eq!(CellError::parse("#div/0!"), Some(CellError::Div0));
    }

    #[test]
    fn numbers_as_xml() {
        assert_eq!(number_to_xml(3.7), "3.7");
        assert_eq!(number_to_xml(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(number_to_xml(1e20), "1E+20");
        assert_eq!(number_to_xml(-1.5e-7), "-1.5E-7");
        assert_eq!(number_to_xml(123456789.0), "123456789");
        for n in [3.7, 1e20, -1.5e-7, 1.0 / 3.0, 1e-300, 6.02214076e23] {
            assert_eq!(number_to_xml(n).parse::<f64>().unwrap(), n);
        }
    }
}
