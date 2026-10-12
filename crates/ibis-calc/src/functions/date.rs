// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Date and time functions. Dates are serial numbers in the workbook's
//! date system (see `eval/convert.rs`).

use super::{Func, arg_array, err, number, numeric, numeric_defaults};
use crate::eval::convert::{date_from_serial, days_in_month, is_leap, serial_from_date};
use crate::eval::{Ctx, Value, to_number};
use crate::formula::Expr;
use crate::value::CellError;

/// The last serial Excel accepts (9999-12-31).
const MAX_SERIAL: f64 = 2_958_465.0;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "DATE" => |c, a| {
            let d1904 = c.date1904();
            numeric(c, a, move |n| make_date(n[0].trunc(), n[1].trunc(), n[2].trunc(), d1904))
        },
        "TIME" => |c, a| {
            numeric(c, a, |n| {
                let (h, m, s) = (n[0].trunc(), n[1].trunc(), n[2].trunc());
                if h > 32767.0 || m > 32767.0 || s > 32767.0 {
                    return err(CellError::Num);
                }
                let total = h * 3600.0 + m * 60.0 + s;
                if total < 0.0 {
                    return err(CellError::Num);
                }
                let days = total / 86400.0;
                number(days - days.floor())
            })
        },
        "YEAR" => |c, a| date_part(c, a, |y, _, _| y as f64),
        "MONTH" => |c, a| date_part(c, a, |_, m, _| m as f64),
        "DAY" => |c, a| date_part(c, a, |_, _, d| d as f64),
        "HOUR" => |c, a| time_part(c, a, |s| (s / 3600) as f64),
        "MINUTE" => |c, a| time_part(c, a, |s| (s / 60 % 60) as f64),
        "SECOND" => |c, a| time_part(c, a, |s| (s % 60) as f64),
        "DATESTRING" => |c, a| {
            let d1904 = c.date1904();
            numeric(c, a, move |n| {
                let serial = n[0].trunc();
                if serial < 0.0 || serial > MAX_SERIAL {
                    return err(CellError::Num);
                }
                let formatted = crate::numfmt::format(&crate::CellValue::Number(serial), "[$-411]ggge\"年\"m\"月\"d\"日\"", d1904);
                if formatted.overflow || formatted.text.is_empty() {
                    err(CellError::Num)
                } else {
                    Value::Text(formatted.text)
                }
            })
        },
        "DATEVALUE" => |c, a| {
            let d1904 = c.date1904();
            let v = c.eval_scalar_or_array(&a[0]);
            c.map1(v, move |_, v| match v {
                Value::Text(t) => match crate::eval::convert::parse_date_time(&t, d1904) {
                    Some(n) if n >= 0.0 => number(n.floor()),
                    _ => err(CellError::Value),
                },
                Value::Error(e) => err(e),
                _ => err(CellError::Value),
            })
        },
        "TIMEVALUE" => |c, a| {
            let d1904 = c.date1904();
            let v = c.eval_scalar_or_array(&a[0]);
            c.map1(v, move |_, v| match v {
                Value::Text(t) => match crate::eval::convert::parse_date_time(&t, d1904) {
                    Some(n) => number(n - n.floor()),
                    None => err(CellError::Value),
                },
                Value::Error(e) => err(e),
                _ => err(CellError::Value),
            })
        },
        "WEEKDAY" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 1.0], move |n| {
                let s = match serial(n[0], d1904) {
                    Ok(s) => s,
                    Err(e) => return err(e),
                };
                let w = day_of_week(s, d1904) as i64; // 0 = Sunday
                let t = n[1].trunc() as i64;
                let start = match t {
                    1 | 17 => 0,
                    2 | 11 => 1,
                    3 => return number(((w + 6) % 7) as f64),
                    12..=16 => t - 10,
                    _ => return err(CellError::Num),
                };
                number(((w - start).rem_euclid(7) + 1) as f64)
            })
        },
        "WEEKNUM" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 1.0], move |n| {
                let s = match serial(n[0], d1904) {
                    Ok(s) => s,
                    Err(e) => return err(e),
                };
                let t = n[1].trunc() as i64;
                if t == 21 {
                    return number(iso_week(s, d1904) as f64);
                }
                let start = match t {
                    1 | 17 => 0,
                    2 | 11 => 1,
                    12..=16 => t - 10,
                    _ => return err(CellError::Num),
                };
                let (y, _, _) = date_from_serial(s, d1904);
                let jan1 = serial_from_date(y, 1, 1, d1904);
                let offset = (day_of_week(jan1, d1904) as i64 - start).rem_euclid(7);
                number(((s - jan1 + offset).div_euclid(7) + 1) as f64)
            })
        },
        "ISOWEEKNUM" => |c, a| {
            let d1904 = c.date1904();
            numeric(c, a, move |n| match serial(n[0], d1904) {
                Ok(s) => number(iso_week(s, d1904) as f64),
                Err(e) => err(e),
            })
        },
        "EDATE" => |c, a| {
            if let Some(e) = no_logical(c, a) {
                return e;
            }
            let d1904 = c.date1904();
            numeric(c, a, move |n| {
                let s = match serial(n[0], d1904) {
                    Ok(s) => s,
                    Err(e) => return err(e),
                };
                // Serial 0 (1900-01-00) is 1899-12-31.
                let (y, m, d) = if s == 0 && !d1904 { (1899, 12, 31) } else { date_from_serial(s, d1904) };
                let total = y * 12 + (m as i64 - 1) + n[1].trunc() as i64;
                let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
                let nd = (d as u32).min(days_in_month(ny, nm));
                let r = serial_from_date(ny, nm as i64, nd.max(1) as i64, d1904);
                if (!d1904 && ny < 1900) || (d1904 && r < 0) || r as f64 > MAX_SERIAL { err(CellError::Num) } else { number(r as f64) }
            })
        },
        "EOMONTH" => |c, a| {
            if let Some(e) = no_logical(c, a) {
                return e;
            }
            let d1904 = c.date1904();
            numeric(c, a, move |n| {
                let s = match serial(n[0], d1904) {
                    Ok(s) => s,
                    Err(e) => return err(e),
                };
                let (y, m, _) = date_from_serial(s, d1904);
                let total = y * 12 + (m as i64 - 1) + n[1].trunc() as i64;
                let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
                let r = serial_from_date(ny, nm as i64, days_in_month(ny, nm) as i64, d1904);
                if (!d1904 && ny < 1900) || (d1904 && r < 0) || r as f64 > MAX_SERIAL {
                    return err(CellError::Num);
                }
                number(r as f64)
            })
        },
        "DAYS" => |c, a| {
            let d1904 = c.date1904();
            numeric(c, a, move |n| match (serial(n[0], d1904), serial(n[1], d1904)) {
                (Ok(e), Ok(s)) => number((e - s) as f64),
                (Err(x), _) | (_, Err(x)) => err(x),
            })
        },
        "DAYS360" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0], move |n| match (serial(n[0], d1904), serial(n[1], d1904)) {
                (Ok(s), Ok(e)) => number(days360(s, e, n[2] != 0.0, d1904) as f64),
                (Err(x), _) | (_, Err(x)) => err(x),
            })
        },
        "DATEDIF" => datedif,
        "YEARFRAC" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0], move |n| {
                let (s, e) = match (serial(n[0], d1904), serial(n[1], d1904)) {
                    (Ok(s), Ok(e)) => (s, e),
                    (Err(x), _) | (_, Err(x)) => return err(x),
                };
                let basis = n[2].trunc();
                if !(0.0..=4.0).contains(&basis) {
                    return err(CellError::Num);
                }
                number(yearfrac(s, e, basis as u32, d1904))
            })
        },
        "NETWORKDAYS" => |c, a| networkdays(c, a, false),
        "NETWORKDAYS.INTL" => |c, a| networkdays(c, a, true),
        "WORKDAY" => |c, a| workday(c, a, false),
        "WORKDAY.INTL" => |c, a| workday(c, a, true),
        "NOW" => |c, _| match c.book.now() {
            Some(n) => number(n),
            None => {
                c.unsupported = true;
                err(CellError::NA)
            }
        },
        "TODAY" => |c, _| match c.book.now() {
            Some(n) => number(n.floor()),
            None => {
                c.unsupported = true;
                err(CellError::NA)
            }
        },
        _ => return None,
    })
}

/// DATE: years 0–1899 are after 1900; months and days carry over.
fn make_date(y: f64, m: f64, d: f64, d1904: bool) -> Value {
    if !(0.0..10000.0).contains(&y) {
        return err(CellError::Num);
    }
    let y = if y < 1900.0 { y + 1900.0 } else { y } as i64;
    let total = y * 12 + (m as i64 - 1);
    let (yy, mm) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    if yy < 1900 && !(yy == 1899 && mm == 12 && !d1904) {
        return err(CellError::Num);
    }
    // Serials count days, the false 1900-02-29 included, so the day
    // simply adds on to the month's first.
    let first = serial_from_date(yy, mm, 1, d1904);
    let s = first as f64 + d - 1.0;
    if s < 0.0 || s > MAX_SERIAL {
        return err(CellError::Num);
    }
    number(s)
}

/// A serial number as a whole day, checked.
fn serial(n: f64, _d1904: bool) -> Result<i64, CellError> {
    if n < 0.0 || n > MAX_SERIAL + 0.999_999 {
        return Err(CellError::Num);
    }
    Ok(n.floor() as i64)
}

/// 0 = Sunday. Serial 0 of the 1900 system counts as Saturday (the day
/// before 1900-01-01, taken as a Sunday by Excel).
fn day_of_week(s: i64, d1904: bool) -> u32 {
    let s = if d1904 { s + 1462 } else { s };
    // Excel counts serial 1 as Sunday, through its false 1900-02-29.
    ((s + 6).rem_euclid(7)) as u32
}

fn iso_week(s: i64, d1904: bool) -> i64 {
    let (y, _, _) = date_from_serial(s, d1904);
    // Monday-based day of week, 1..7.
    let dow = |x: i64| ((day_of_week(x, d1904) as i64 + 6) % 7) + 1;
    let week1_start = |year: i64| {
        let jan4 = serial_from_date(year, 1, 4, d1904);
        jan4 - (dow(jan4) - 1)
    };
    let mut start = week1_start(y + 1);
    if s >= start {
        return 1;
    }
    start = week1_start(y);
    if s < start {
        start = week1_start(y - 1);
    }
    (s - start) / 7 + 1
}

fn date_part(c: &mut Ctx, a: &[Expr], f: fn(i64, u32, u32) -> f64) -> Value {
    let d1904 = c.date1904();
    let v = c.eval_scalar_or_array(&a[0]);
    c.map1(v, move |_, v| {
        let n = match to_number(&v, d1904) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        match serial(n, d1904) {
            Ok(s) => {
                let (y, m, d) = date_from_serial(s, d1904);
                number(f(y, m, d))
            }
            Err(e) => err(e),
        }
    })
}

fn time_part(c: &mut Ctx, a: &[Expr], f: fn(i64) -> f64) -> Value {
    let d1904 = c.date1904();
    let v = c.eval_scalar_or_array(&a[0]);
    c.map1(v, move |_, v| {
        let n = match to_number(&v, d1904) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        if n < 0.0 {
            return err(CellError::Num);
        }
        // To the nearest second.
        let secs = ((n - n.floor()) * 86400.0).round() as i64 % 86400;
        number(f(secs))
    })
}

fn days360(s: i64, e: i64, european: bool, d1904: bool) -> i64 {
    let (y1, m1, mut d1) = date_from_serial(s, d1904);
    let (y2, m2, mut d2) = date_from_serial(e, d1904);
    // (1900-02-29, serial 60, counts as the end of February.)
    let last_feb = |y: i64, m: u32, d: u32| m == 2 && d >= days_in_month(y, 2);
    if european {
        d1 = d1.min(30);
        d2 = d2.min(30);
    } else {
        let start_last_feb = last_feb(y1, m1, d1);
        if start_last_feb && last_feb(y2, m2, d2) {
            d2 = 30;
        }
        if start_last_feb || d1 == 31 {
            d1 = 30;
        }
        if d2 == 31 && d1 >= 30 {
            d2 = 30;
        }
    }
    (y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (d2 as i64 - d1 as i64)
}

pub(crate) fn yearfrac(mut s: i64, mut e: i64, basis: u32, d1904: bool) -> f64 {
    if s > e {
        std::mem::swap(&mut s, &mut e);
    }
    match basis {
        0 => {
            let (y1, m1, mut d1) = date_from_serial(s, d1904);
            let (y2, m2, mut d2) = date_from_serial(e, d1904);
            // US 30/360 (NASD).
            if d1 == 31 && d2 == 31 {
                d1 = 30;
                d2 = 30;
            } else if d1 == 31 {
                d1 = 30;
            } else if d1 == 30 && d2 == 31 {
                d2 = 30;
            } else if m1 == 2 && m2 == 2 && d1 == days_in_month(y1, 2) && d2 == days_in_month(y2, 2) {
                d1 = 30;
                d2 = 30;
            } else if m1 == 2 && d1 == days_in_month(y1, 2) {
                d1 = 30;
            }
            ((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (d2 as i64 - d1 as i64)) as f64 / 360.0
        }
        1 => {
            let (y1, m1, d1) = date_from_serial(s, d1904);
            let (y2, m2, d2) = date_from_serial(e, d1904);
            let days = (e - s) as f64;
            // Within a year: the year's length (366 if it holds a 29 February).
            let within_year = y1 == y2 || (y2 == y1 + 1 && (m1 > m2 || (m1 == m2 && d1 >= d2)));
            if within_year {
                let leap = if y1 == y2 {
                    is_leap(y1)
                } else {
                    (is_leap(y1) && (m1 < 2 || (m1 == 2 && d1 <= 29))) || (is_leap(y2) && (m2 > 2 || (m2 == 2 && d2 == 29)))
                };
                days / if leap { 366.0 } else { 365.0 }
            } else {
                let years = (y2 - y1 + 1) as f64;
                let total: i64 = (y1..=y2).map(|y| if is_leap(y) { 366 } else { 365 }).sum();
                days / (total as f64 / years)
            }
        }
        2 => (e - s) as f64 / 360.0,
        3 => (e - s) as f64 / 365.0,
        _ => {
            let (y1, m1, d1) = date_from_serial(s, d1904);
            let (y2, m2, d2) = date_from_serial(e, d1904);
            let (d1, d2) = (d1.min(30), d2.min(30));
            ((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (d2 as i64 - d1 as i64)) as f64 / 360.0
        }
    }
}

fn datedif(c: &mut Ctx, a: &[Expr]) -> Value {
    let d1904 = c.date1904();
    let (s, e) = match (super::arg_num(c, a, 0), super::arg_num(c, a, 1)) {
        (Ok(s), Ok(e)) => (s, e),
        (Err(x), _) | (_, Err(x)) => return err(x),
    };
    let unit = match super::arg_text(c, a, 2) {
        Ok(u) => u.to_uppercase(),
        Err(x) => return err(x),
    };
    let (s, e) = match (serial(s, d1904), serial(e, d1904)) {
        (Ok(s), Ok(e)) => (s, e),
        (Err(x), _) | (_, Err(x)) => return err(x),
    };
    if s > e {
        return err(CellError::Num);
    }
    let (y1, m1, d1) = date_from_serial(s, d1904);
    let (y2, m2, d2) = date_from_serial(e, d1904);
    let months = (y2 - y1) * 12 + m2 as i64 - m1 as i64 - i64::from(d2 < d1);
    match unit.as_str() {
        "Y" => number((months / 12) as f64),
        "M" => number(months as f64),
        "D" => number((e - s) as f64),
        "YM" => number((months % 12) as f64),
        "MD" => {
            if d2 >= d1 {
                number((d2 - d1) as f64)
            } else {
                // Days from the same day of the month before. Excel allows negative values when d1 > dim.
                let (py, pm) = if m2 == 1 { (y2 - 1, 12) } else { (y2, m2 - 1) };
                let dim = days_in_month(py, pm) as i64;
                number((dim - d1 as i64 + d2 as i64) as f64)
            }
        }
        "YD" => {
            let mut anniversary_year = y2;
            if (m1, d1) > (m2, d2) {
                anniversary_year -= 1;
            }
            let day = (d1).min(days_in_month(anniversary_year, m1));
            let start = serial_from_date(anniversary_year, m1 as i64, day as i64, d1904);
            number((e - start) as f64)
        }
        _ => err(CellError::Num),
    }
}

/// Which days of the week (0 = Monday) are weekend days.
fn weekend_of(v: &Value, allow_all_weekend: bool) -> Result<[bool; 7], CellError> {
    match v {
        Value::Text(t) => {
            if t.len() != 7 || !t.bytes().all(|b| b == b'0' || b == b'1') || (!allow_all_weekend && t == "1111111") {
                return Err(CellError::Value);
            }
            let mut w = [false; 7];
            for (i, b) in t.bytes().enumerate() {
                w[i] = b == b'1';
            }
            Ok(w)
        }
        Value::Empty => Ok([false, false, false, false, false, true, true]),
        v => {
            let n = to_number(v, false)?.trunc() as i64;
            let mut w = [false; 7];
            match n {
                1..=7 => {
                    // 1: Sat+Sun, 2: Sun+Mon, … 7: Fri+Sat.
                    let first = (n + 4) % 7; // Monday-based index of the first weekend day
                    w[first as usize] = true;
                    w[((first + 1) % 7) as usize] = true;
                }
                11..=17 => w[((n - 11 + 6) % 7) as usize] = true,
                _ => return Err(CellError::Num),
            }
            Ok(w)
        }
    }
}

fn holidays(c: &mut Ctx, e: Option<&Expr>, d1904: bool) -> Result<Vec<i64>, CellError> {
    let Some(e) = e.filter(|e| !matches!(e, Expr::Missing)) else { return Ok(Vec::new()) };
    let arr = arg_array(c, e)?;
    let mut out = Vec::new();
    for v in arr.data.iter() {
        match v {
            Value::Empty => {}
            Value::Error(e) => return Err(*e),
            v => out.push(serial(to_number(v, d1904)?, d1904)?),
        }
    }
    Ok(out)
}

fn networkdays(c: &mut Ctx, a: &[Expr], intl: bool) -> Value {
    let d1904 = c.date1904();
    let (s, e) = match (super::arg_num(c, a, 0), super::arg_num(c, a, 1)) {
        (Ok(s), Ok(e)) => (s, e),
        (Err(x), _) | (_, Err(x)) => return err(x),
    };
    let (s, e) = match (serial(s, d1904), serial(e, d1904)) {
        (Ok(s), Ok(e)) => (s, e),
        (Err(x), _) | (_, Err(x)) => return err(x),
    };
    let weekend = if intl {
        let v = match a.get(2) {
            None | Some(Expr::Missing) => Value::Empty,
            Some(x) => c.eval_scalar(x),
        };
        match weekend_of(&v, true) {
            Ok(w) => w,
            Err(x) => return err(x),
        }
    } else {
        [false, false, false, false, false, true, true]
    };
    let hol = match holidays(c, a.get(if intl { 3 } else { 2 }), d1904) {
        Ok(h) => h,
        Err(x) => return err(x),
    };
    let (lo, hi, sign) = if s <= e { (s, e, 1.0) } else { (e, s, -1.0) };
    let mut count = 0i64;
    for d in lo..=hi {
        let monday = ((day_of_week(d, d1904) as i64) + 6) % 7;
        if !weekend[monday as usize] && !hol.contains(&d) {
            count += 1;
        }
    }
    number(sign * count as f64)
}

fn workday(c: &mut Ctx, a: &[Expr], intl: bool) -> Value {
    if let Some(e) = no_logical(c, &a[..2]) {
        return e;
    }
    let d1904 = c.date1904();
    let (s, days) = match (super::arg_num(c, a, 0), super::arg_num(c, a, 1)) {
        (Ok(s), Ok(d)) => (s, d.trunc() as i64),
        (Err(x), _) | (_, Err(x)) => return err(x),
    };
    let s = match serial(s, d1904) {
        Ok(s) => s,
        Err(x) => return err(x),
    };
    let weekend = if intl {
        let v = match a.get(2) {
            None | Some(Expr::Missing) => Value::Empty,
            Some(x) => c.eval_scalar(x),
        };
        match weekend_of(&v, false) {
            Ok(w) => w,
            Err(x) => return err(x),
        }
    } else {
        [false, false, false, false, false, true, true]
    };
    let hol = match holidays(c, a.get(if intl { 3 } else { 2 }), d1904) {
        Ok(h) => h,
        Err(x) => return err(x),
    };
    let step = if days >= 0 { 1 } else { -1 };
    let mut left = days.abs();
    let mut d = s;
    while left > 0 {
        d += step;
        if d < 0 || d as f64 > MAX_SERIAL {
            return err(CellError::Num);
        }
        let monday = ((day_of_week(d, d1904) as i64) + 6) % 7;
        if !weekend[monday as usize] && !hol.contains(&d) {
            left -= 1;
        }
    }
    number(d as f64)
}

/// The Analysis ToolPak's date functions take no logical values:
/// `#VALUE!` if an argument is one.
fn no_logical(c: &mut Ctx, a: &[Expr]) -> Option<Value> {
    for e in a {
        if let Value::Bool(_) = c.eval_scalar(e) {
            return Some(err(CellError::Value));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_1904_system() {
        assert_eq!(make_date(1904.0, 1.0, 1.0, true), Value::Number(0.0));
        assert_eq!(make_date(1900.0, 1.0, 1.0, true), Value::Error(CellError::Num));
        assert_eq!(make_date(2026.0, 10.0, 3.0, true), Value::Number(44836.0));
        assert_eq!(date_from_serial(0, true), (1904, 1, 1));
        assert_eq!(serial_from_date(1904, 1, 1, true), 0);
        assert_eq!(serial_from_date(2026, 10, 3, true), 44836);
        // Friday (0 = Sunday, 5 = Friday in internal day_of_week)
        assert_eq!(day_of_week(0, true), 5);
    }
}
