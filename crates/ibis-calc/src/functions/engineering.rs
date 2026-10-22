// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Engineering functions: number bases, bits, complex numbers, Bessel
//! functions, the error function, units.

use super::{Func, arg_num, arg_text, err, number, numeric, numeric_defaults};
use crate::eval::{Ctx, Value, to_number, to_text};
use crate::formula::Expr;
use crate::special::{erf, erfc, ln_gamma};
use crate::value::CellError;
use std::f64::consts::{FRAC_PI_4, LN_10, PI};

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "BIN2DEC" => |c, a| from_base(c, a, 2, None),
        "OCT2DEC" => |c, a| from_base(c, a, 8, None),
        "HEX2DEC" => |c, a| from_base(c, a, 16, None),
        "BIN2OCT" => |c, a| from_base(c, a, 2, Some(8)),
        "BIN2HEX" => |c, a| from_base(c, a, 2, Some(16)),
        "OCT2BIN" => |c, a| from_base(c, a, 8, Some(2)),
        "OCT2HEX" => |c, a| from_base(c, a, 8, Some(16)),
        "HEX2BIN" => |c, a| from_base(c, a, 16, Some(2)),
        "HEX2OCT" => |c, a| from_base(c, a, 16, Some(8)),
        "DEC2BIN" => |c, a| dec_to(c, a, 2),
        "DEC2OCT" => |c, a| dec_to(c, a, 8),
        "DEC2HEX" => |c, a| dec_to(c, a, 16),
        "BITAND" => |c, a| bits(c, a, |x, y| x & y),
        "BITOR" => |c, a| bits(c, a, |x, y| x | y),
        "BITXOR" => |c, a| bits(c, a, |x, y| x ^ y),
        "BITLSHIFT" => |c, a| shift(c, a, true),
        "BITRSHIFT" => |c, a| shift(c, a, false),
        "DELTA" => |c, a| numeric_defaults(c, a, &[0.0, 0.0], |n| number(if n[0] == n[1] { 1.0 } else { 0.0 })),
        "GESTEP" => |c, a| numeric_defaults(c, a, &[0.0, 0.0], |n| number(if n[0] >= n[1] { 1.0 } else { 0.0 })),
        "ERF" => |c, a| {
            let given = a.len();
            numeric(c, a, move |n| if given > 1 { number(erf(n[1]) - erf(n[0])) } else { number(erf(n[0])) })
        },
        "ERF.PRECISE" => |c, a| numeric(c, a, |n| number(erf(n[0]))),
        "ERFC" | "ERFC.PRECISE" => |c, a| numeric(c, a, |n| number(erfc(n[0]))),
        "BESSELJ" => |c, a| bessel(c, a, besselj),
        "BESSELY" => |c, a| bessel(c, a, bessely),
        "BESSELI" => |c, a| bessel(c, a, besseli),
        "BESSELK" => |c, a| bessel(c, a, besselk),
        "COMPLEX" => complex_fn,
        "IMREAL" => |c, a| im1(c, a, |z| number(z.re)),
        "IMAGINARY" => |c, a| im1(c, a, |z| number(z.im)),
        "IMABS" => |c, a| im1(c, a, |z| number(z.re.hypot(z.im))),
        "IMARGUMENT" => |c, a| im1(c, a, |z| if z.re == 0.0 && z.im == 0.0 { err(CellError::Div0) } else { number(z.im.atan2(z.re)) }),
        "IMCONJUGATE" => |c, a| im1z(c, a, |z| Some(C { re: z.re, im: -z.im })),
        "IMEXP" => |c, a| im1z(c, a, |z| Some(z.exp())),
        "IMLN" => |c, a| im1z(c, a, |z| z.ln()),
        "IMLOG10" => |c, a| im1z(c, a, |z| z.ln().map(|l| l.scale(1.0 / std::f64::consts::LN_10))),
        // Excel's IMLOG2(z) evaluates as IMDIV(IMLN(z), IMLN(2)); IMLN(2) text formats as "0.693147180559945".
        "IMLOG2" => |c, a| im1z(c, a, |z| z.ln().map(|l| l.scale(1.0 / 0.693147180559945))),
        "IMSQRT" => |c, a| im1z(c, a, |z| Some(z.sqrt())),
        "IMSIN" => |c, a| im1z(c, a, |z| Some(z.sin())),
        "IMCOS" => |c, a| im1z(c, a, |z| Some(z.cos())),
        "IMTAN" => |c, a| im1z(c, a, |z| z.sin().div(z.cos())),
        "IMCOT" => |c, a| im1z(c, a, |z| z.cos().div(z.sin())),
        "IMSEC" => |c, a| im1z(c, a, |z| C::ONE.div(z.cos())),
        "IMCSC" => |c, a| im1z(c, a, |z| C::ONE.div(z.sin())),
        "IMSINH" => |c, a| im1z(c, a, |z| Some(z.sinh())),
        "IMCOSH" => |c, a| im1z(c, a, |z| Some(z.cosh())),
        "IMSECH" => |c, a| im1z(c, a, |z| C::ONE.div(z.cosh())),
        "IMCSCH" => |c, a| im1z(c, a, |z| C::ONE.div(z.sinh())),
        "IMPOWER" => impower,
        "IMDIV" => |c, a| im2(c, a, |x, y| x.div(y)),
        "IMSUB" => |c, a| im2(c, a, |x, y| Some(C { re: x.re - y.re, im: x.im - y.im })),
        "IMSUM" => |c, a| im_fold(c, a, C::ZERO, |x, y| C { re: x.re + y.re, im: x.im + y.im }),
        "IMPRODUCT" => |c, a| im_fold(c, a, C::ONE, |x, y| x.mul(y)),
        "CONVERT" => convert,
        _ => return None,
    })
}

// ----- Number bases --------------------------------------------------------

/// Text in `base` (10 digits at most, the top bit negative) as a number.
fn parse_base(t: &str, base: u32) -> Result<i64, CellError> {
    let t = t.trim();
    if t.len() > 10 || t.is_empty() && false {
        return Err(CellError::Num);
    }
    if t.is_empty() {
        return Ok(0);
    }
    let mut v: i64 = 0;
    for ch in t.chars() {
        let d = ch.to_digit(base).ok_or(CellError::Num)?;
        v = v * base as i64 + d as i64;
    }
    // Ten digits: two's complement over 10 digits.
    let full = (base as i64).pow(10);
    if t.len() == 10 && v >= full / 2 {
        v -= full;
    }
    Ok(v)
}

fn base_text(v: Value) -> Result<String, CellError> {
    match v {
        Value::Bool(_) => Err(CellError::Value),
        Value::Number(n) => {
            if n.fract() != 0.0 {
                return Err(CellError::Num);
            }
            Ok(format!("{}", n as i64))
        }
        v => to_text(&v),
    }
}

fn from_base(c: &mut Ctx, a: &[Expr], base: u32, to: Option<u32>) -> Value {
    // The places first: their error comes before the number's.
    let places = match places_arg(c, a, 1) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let text = match base_text(c.eval_scalar(&a[0])) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let v = match parse_base(&text, base) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    match to {
        None => number(v as f64),
        Some(to) => to_base(v, to, places),
    }
}

fn places_arg(c: &mut Ctx, a: &[Expr], i: usize) -> Result<Option<usize>, CellError> {
    match a.get(i) {
        None | Some(Expr::Missing) => Ok(None),
        Some(e) => {
            let v = c.eval_scalar(e);
            if let Value::Bool(_) = v {
                return Err(CellError::Value);
            }
            let p = to_number(&v, false)?.trunc();
            if !(0.0..=10.0).contains(&p) {
                return Err(CellError::Num);
            }
            Ok(Some(p as usize))
        }
    }
}

/// A number in `base`: two's complement over 10 digits when negative.
fn to_base(v: i64, base: u32, places: Option<usize>) -> Value {
    let full = (base as i64).pow(10);
    if v < -full / 2 || v >= full / 2 {
        return err(CellError::Num);
    }
    let u = if v < 0 { v + full } else { v };
    let mut digits = Vec::new();
    let mut x = u;
    loop {
        digits.push(std::char::from_digit((x % base as i64) as u32, base).unwrap().to_ascii_uppercase());
        x /= base as i64;
        if x == 0 {
            break;
        }
    }
    let mut s: String = digits.into_iter().rev().collect();
    if v >= 0
        && let Some(p) = places
    {
        if s.len() > p {
            return err(CellError::Num);
        }
        s = format!("{}{s}", "0".repeat(p - s.len()));
    }
    Value::Text(s)
}

fn dec_to(c: &mut Ctx, a: &[Expr], base: u32) -> Value {
    let v = c.eval_scalar(&a[0]);
    if let Value::Bool(_) = v {
        return err(CellError::Value);
    }
    let n = match to_number(&v, false) {
        Ok(n) => n.trunc(),
        Err(e) => return err(e),
    };
    let places = match places_arg(c, a, 1) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    to_base(n as i64, base, places)
}

const BIT_LIMIT: f64 = 281_474_976_710_655.0; // 2^48 - 1

fn bits(c: &mut Ctx, a: &[Expr], f: fn(u64, u64) -> u64) -> Value {
    numeric(c, a, move |n| {
        let (x, y) = (n[0], n[1]);
        if x < 0.0 || y < 0.0 || x > BIT_LIMIT || y > BIT_LIMIT || x.fract() != 0.0 || y.fract() != 0.0 {
            return err(CellError::Num);
        }
        number(f(x as u64, y as u64) as f64)
    })
}

fn shift(c: &mut Ctx, a: &[Expr], left: bool) -> Value {
    numeric(c, a, move |n| {
        let (x, s) = (n[0], n[1].trunc());
        if x < 0.0 || x > BIT_LIMIT || x.fract() != 0.0 {
            return err(CellError::Num);
        }
        if x == 0.0 {
            return number(0.0);
        }
        if s.abs() > 53.0 {
            return err(CellError::Num);
        }
        let s = if left { s } else { -s };
        let r = if s >= 0.0 { x * 2f64.powi(s as i32) } else { (x as u64 >> (-s) as u32) as f64 };
        if r > BIT_LIMIT { err(CellError::Num) } else { number(r) }
    })
}

// ----- Bessel --------------------------------------------------------------

fn bessel(c: &mut Ctx, a: &[Expr], f: fn(f64, i32) -> Result<f64, CellError>) -> Value {
    numeric(c, a, move |n| {
        let (x, order) = (n[0], n[1].trunc());
        if order < 0.0 || order > 1e9 {
            return err(CellError::Num);
        }
        f(x, order as i32).map_or_else(err, number)
    })
}

/// ln of `f64::MAX` and of the smallest positive (subnormal) f64.
const LN_F64_MAX: f64 = 709.782_712_893_384;
const LN_F64_MIN: f64 = -744.440_071_921_381_2;

/// ln((x/2)^n / n!), the leading term of the J_n and I_n power series.
fn ln_lead_term(half: f64, n: i32) -> f64 {
    n as f64 * half.ln() - ln_gamma(n as f64 + 1.0)
}

/// J_n(x): the power series where it has no cancellation, Hankel's expansion
/// (or forward recurrence from J0 and J1) for large x, and Miller's backward
/// recurrence in between.
fn besselj(x: f64, n: i32) -> Result<f64, CellError> {
    if n < 0 || x.abs() >= 1e14 {
        return Err(CellError::Num);
    }
    let ax = x.abs();
    if ax == 0.0 {
        return Ok(if n == 0 { 1.0 } else { 0.0 });
    }
    let half = ax / 2.0;
    let ln_lead = ln_lead_term(half, n);
    // |J_n(x)| <= (|x|/2)^n / n! (DLMF 10.14.4), so below the smallest f64 it is 0.
    if ln_lead < LN_F64_MIN {
        return Ok(0.0);
    }
    let ans = if half * half <= 0.5 * (n as f64 + 1.0) {
        // Power series Σ (-1)^k (x/2)^(2k+n) / (k! (n+k)!): each term is at
        // most half the one before, so the sum has no cancellation.
        let mut term = ln_lead.exp();
        let mut sum = term;
        let mut k = 1.0;
        loop {
            term *= -half * half / (k * (k + n as f64));
            sum += term;
            if term.abs() <= 1e-17 * sum.abs() {
                break;
            }
            k += 1.0;
        }
        sum
    } else if ax >= 25.0 && n as f64 <= ax {
        match hankel_j(ax, n) {
            Some(v) => v,
            None => {
                // The expansion does not reach double precision at this order:
                // recur upward from J0 and J1 (stable while the order stays below x).
                // At x >= 25 the expansion does converge for orders 0 and 1.
                let mut a = hankel_j(ax, 0).ok_or(CellError::Num)?;
                let mut b = hankel_j(ax, 1).ok_or(CellError::Num)?;
                for k in 1..n {
                    let next = 2.0 * k as f64 / ax * b - a;
                    a = b;
                    b = next;
                }
                b
            }
        }
    } else {
        miller_j(ax, n)
    };
    Ok(if x < 0.0 && n % 2 == 1 { -ans } else { ans })
}

/// J_n(x) for large x > 0 by Hankel's expansion
/// sqrt(2/(πx)) (P cos χ - Q sin χ), χ = x - (n/2 + 1/4)π (DLMF 10.17.3),
/// summed while its terms shrink, until they fall below double precision.
/// None when the terms start growing first (x is not large enough for n).
fn hankel_j(x: f64, n: i32) -> Option<f64> {
    let mu = 4.0 * (n as f64).powi(2);
    let (mut p, mut q, mut t) = (1.0f64, 0.0f64, 1.0f64);
    let mut k: i64 = 1;
    loop {
        // t_k = a_k(n) / x^k, a_k = (μ - 1²)(μ - 3²)…(μ - (2k-1)²) / (k! 8^k).
        let next = t * (mu - ((2 * k - 1) as f64).powi(2)) / (8.0 * k as f64 * x);
        if next.abs() >= t.abs() {
            return None;
        }
        t = next;
        // P = t0 - t2 + t4 - …, Q = t1 - t3 + t5 - ….
        match k % 4 {
            1 => q += t,
            2 => p -= t,
            3 => q -= t,
            _ => p += t,
        }
        if t.abs() <= 1e-17 * p.abs().max(q.abs()) {
            break;
        }
        k += 1;
    }
    // cos χ and sin χ from those of x, with the phase (n/2 + 1/4)π reduced
    // mod 2π exactly, so that large x loses no accuracy in χ.
    let phase = ((2 * n as i64 + 1) % 8) as f64 * FRAC_PI_4;
    let (s, c) = x.sin_cos();
    let (sp, cp) = phase.sin_cos();
    let (cos_chi, sin_chi) = (c * cp + s * sp, s * cp - c * sp);
    Some((2.0 / (PI * x)).sqrt() * (p * cos_chi - q * sin_chi))
}

/// J_n(x) for x > 0 by Miller's backward recurrence, started at an even
/// order far enough above both n and x that J is negligible there, and
/// normalised by J0 + 2 Σ J_2k = 1 (DLMF 10.12.4).
fn miller_j(x: f64, n: i32) -> f64 {
    let top = (n as f64).max(x.ceil());
    let m = 2 * ((top + (160.0 * top).sqrt() + 20.0) as i64 / 2);
    // Unnormalised J_{k+1} and J_k, starting from J_{m+1} = 0, J_m = 1.
    let (mut above, mut cur) = (0.0f64, 1.0f64);
    let mut ans = 0.0;
    let mut sum = cur; // 2 Σ J_2k is summed over k >= 1; m is even.
    for k in (1..=m).rev() {
        let below = 2.0 * k as f64 / x * cur - above;
        above = cur;
        cur = below;
        if cur.abs() > 1e100 {
            above *= 1e-100;
            cur *= 1e-100;
            ans *= 1e-100;
            sum *= 1e-100;
        }
        let order = k - 1;
        if order == n as i64 {
            ans = cur;
        }
        if order > 0 && order % 2 == 0 {
            sum += cur;
        }
    }
    ans / (2.0 * sum + cur)
}

/// I_n(x) by its power series Σ (x/2)^(2k+n) / (k! (n+k)!), whose terms are
/// all positive, so it has no cancellation at any x. The sum is carried as
/// `sum · e^scale`, so that neither a tiny leading term nor a large result
/// overflows or underflows on the way.
fn besseli(x: f64, n: i32) -> Result<f64, CellError> {
    if n < 0 {
        return Err(CellError::Num);
    }
    if x != 0.0 && x.abs() < 1e-10 && n >= 10 {
        return Err(CellError::Num);
    }
    let ax = x.abs();
    if ax == 0.0 {
        return Ok(if n == 0 { 1.0 } else { 0.0 });
    }
    let half = ax / 2.0;
    let q = half * half;
    let mut scale = ln_lead_term(half, n);
    // With t0 the leading term, t0 <= I_n(x) <= t0 e^(q/(n+1)), since the
    // k-th term is t0 q^k / (k! (n+1)…(n+k)) <= t0 (q/(n+1))^k / k!.
    if scale > LN_F64_MAX {
        return Err(CellError::Num);
    }
    if scale + q / (n as f64 + 1.0) < LN_F64_MIN {
        return Ok(0.0);
    }
    let (mut term, mut sum) = (1.0f64, 1.0f64);
    let mut k = 1.0;
    loop {
        term *= q / (k * (k + n as f64));
        sum += term;
        if sum > 1e280 {
            term *= 1e-280;
            sum *= 1e-280;
            scale += 280.0 * LN_10;
            // The result is at least e^scale.
            if scale > LN_F64_MAX {
                return Err(CellError::Num);
            }
        }
        if term <= 1e-17 * sum {
            break;
        }
        k += 1.0;
    }
    let lead = scale.exp();
    let ans = if lead >= f64::MIN_POSITIVE { sum * lead } else { (scale + sum.ln()).exp() };
    Ok(if x < 0.0 && n % 2 == 1 { -ans } else { ans })
}

fn bessely(x: f64, n: i32) -> Result<f64, CellError> {
    if x <= 0.0 || n < 0 || x >= 1e14 {
        return Err(CellError::Num);
    }
    if x >= 1e7 {
        let chi = x - (n as f64 / 2.0 + 0.25) * std::f64::consts::PI;
        let v = (2.0 / (std::f64::consts::PI * x)).sqrt() * chi.sin();
        return Ok(v);
    }
    let (y0, y1) = if x >= 8.0 {
        // Hankel asymptotic expansion for Y0 and Y1.
        let chi0 = x - 0.25 * std::f64::consts::PI;
        let chi1 = x - 0.75 * std::f64::consts::PI;
        let amp = (2.0 / (std::f64::consts::PI * x)).sqrt();
        let p0 = 1.0 - 9.0 / (128.0 * x * x) + 3675.0 / (32768.0 * x.powi(4));
        let q0 = -1.0 / (8.0 * x) + 75.0 / (1024.0 * x.powi(3));
        let y0 = amp * (p0 * chi0.sin() + q0 * chi0.cos());
        let p1 = 1.0 + 15.0 / (128.0 * x * x) - 3465.0 / (32768.0 * x.powi(4));
        let q1 = 3.0 / (8.0 * x) - 105.0 / (1024.0 * x.powi(3));
        let y1 = amp * (p1 * chi1.sin() + q1 * chi1.cos());
        (y0, y1)
    } else {
        let j0 = besselj(x, 0)?;
        let half = x / 2.0;
        let mut sum = 0.0;
        let mut term = 1.0;
        let mut h = 0.0;
        for k in 1..300 {
            term *= -half * half / (k as f64 * k as f64);
            h += 1.0 / k as f64;
            let add = term * h;
            sum += add;
            if add.abs() < 1e-17 * sum.abs().max(1e-300) {
                break;
            }
        }
        let y0 = 2.0 / std::f64::consts::PI * ((half.ln() + 0.577_215_664_901_532_9) * j0 - sum);
        let (j0, j1) = (besselj(x, 0)?, besselj(x, 1)?);
        let y1 = (j1 * y0 - 2.0 / (std::f64::consts::PI * x)) / j0;
        (y0, y1)
    };
    if n == 0 {
        return Ok(y0);
    }
    let (mut a, mut b) = (y0, y1);
    for k in 1..n {
        let next = 2.0 * k as f64 / x * b - a;
        a = b;
        b = next;
    }
    Ok(b)
}

fn besselk(x: f64, n: i32) -> Result<f64, CellError> {
    if x <= 0.0 || n < 0 {
        return Err(CellError::Num);
    }
    if x >= 700.0 {
        return Ok(0.0);
    }
    if x >= 8.0 {
        // Asymptotic form for large x.
        let mu = 4.0 * (n as f64).powi(2);
        let amp = (std::f64::consts::PI / (2.0 * x)).sqrt() * (-x).exp();
        let mut s = 1.0;
        let mut t = 1.0;
        for k in 1..25 {
            t *= (mu - ((2 * k - 1) as f64).powi(2)) / (k as f64 * 8.0 * x);
            s += t;
            if t.abs() < 1e-17 * s.abs() {
                break;
            }
        }
        return Ok(amp * s);
    }
    let k0 = {
        let i0 = besseli(x, 0)?;
        let half = x / 2.0;
        let mut sum = 0.0;
        let mut term = 1.0;
        let mut h = 0.0;
        for k in 1..300 {
            term *= half * half / (k as f64 * k as f64);
            h += 1.0 / k as f64;
            let add = term * h;
            sum += add;
            if add.abs() < 1e-17 * sum.abs().max(1e-300) {
                break;
            }
        }
        -(half.ln() + 0.577_215_664_901_532_9) * i0 + sum
    };
    if n == 0 {
        return Ok(k0);
    }
    // Wronskian: I0 K1 + I1 K0 = 1/x.
    let (i0, i1) = (besseli(x, 0)?, besseli(x, 1)?);
    let k1 = (1.0 / x - i1 * k0) / i0;
    let (mut a, mut b) = (k0, k1);
    for k in 1..n {
        let next = 2.0 * k as f64 / x * b + a;
        a = b;
        b = next;
    }
    Ok(b)
}

// ----- Complex numbers -----------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    const ZERO: C = C { re: 0.0, im: 0.0 };
    const ONE: C = C { re: 1.0, im: 0.0 };

    fn scale(self, k: f64) -> C {
        C { re: self.re * k, im: self.im * k }
    }
    fn mul(self, o: C) -> C {
        C { re: self.re * o.re - self.im * o.im, im: self.re * o.im + self.im * o.re }
    }
    fn div(self, o: C) -> Option<C> {
        // Over an infinitely large number: zero.
        if !o.re.is_finite() || !o.im.is_finite() {
            return Some(C::ZERO);
        }
        let d = o.re * o.re + o.im * o.im;
        if d == 0.0 {
            return None;
        }
        Some(C { re: (self.re * o.re + self.im * o.im) / d, im: (self.im * o.re - self.re * o.im) / d })
    }
    fn exp(self) -> C {
        let e = self.re.exp();
        C { re: e * self.im.cos(), im: e * self.im.sin() }
    }
    fn ln(self) -> Option<C> {
        if self.re == 0.0 && self.im == 0.0 {
            return None;
        }
        Some(C { re: self.re.hypot(self.im).ln(), im: self.im.atan2(self.re) })
    }
    fn sqrt(self) -> C {
        let r = self.re.hypot(self.im).sqrt();
        let t = self.im.atan2(self.re) / 2.0;
        C { re: r * t.cos(), im: r * t.sin() }
    }
    fn sin(self) -> C {
        C { re: self.re.sin() * self.im.cosh(), im: self.re.cos() * self.im.sinh() }
    }
    fn cos(self) -> C {
        C { re: self.re.cos() * self.im.cosh(), im: -self.re.sin() * self.im.sinh() }
    }
    fn sinh(self) -> C {
        C { re: self.re.sinh() * self.im.cos(), im: self.re.cosh() * self.im.sin() }
    }
    fn cosh(self) -> C {
        C { re: self.re.cosh() * self.im.cos(), im: self.re.sinh() * self.im.sin() }
    }
}

/// A complex number as text (`3+4i`, `-2.5j`, `i`, `5`), and its suffix.
fn parse_complex(t: &str) -> Result<(C, Option<char>), CellError> {
    let t = t.trim();
    if t.is_empty() {
        return Err(CellError::Num);
    }
    // A date or time typed as text is its number.
    if let Some(n) = crate::eval::parse_number_text(t, false) {
        return Ok((C { re: n, im: 0.0 }, None));
    }
    let (body, suffix) = match t.chars().last() {
        Some(ch @ ('i' | 'j')) => (&t[..t.len() - 1], Some(ch)),
        _ => (t, None),
    };
    let num = |s: &str| -> Result<f64, CellError> {
        if s.is_empty() || s == "+" {
            return Ok(1.0);
        }
        if s == "-" {
            return Ok(-1.0);
        }
        if s.contains(char::is_whitespace) || s.starts_with("++") {
            return Err(CellError::Num);
        }
        s.parse::<f64>().map_err(|_| CellError::Num)
    };
    match suffix {
        None => Ok((C { re: num(body)?, im: 0.0 }, None)),
        Some(sfx) => {
            // Split the real part off at the last sign that is not an exponent's.
            let bytes = body.as_bytes();
            let mut split = None;
            for i in (1..bytes.len()).rev() {
                if (bytes[i] == b'+' || bytes[i] == b'-') && !matches!(bytes[i - 1], b'e' | b'E') {
                    split = Some(i);
                    break;
                }
            }
            match split {
                Some(i) => Ok((C { re: body[..i].parse::<f64>().map_err(|_| CellError::Num)?, im: num(&body[i..])? }, Some(sfx))),
                None => Ok((C { re: 0.0, im: num(body)? }, Some(sfx))),
            }
        }
    }
}

/// A number as IM* functions write it (up to 15 significant digits).
fn im_number(v: f64) -> String {
    crate::eval::number_to_text(v)
}

fn format_complex(z: C, suffix: char) -> String {
    let (re, im) = (clean_zero(z.re), clean_zero(z.im));
    if im == 0.0 {
        return im_number(re);
    }
    let imag = match im {
        1.0 => String::new(),
        -1.0 => "-".into(),
        v => im_number(v),
    };
    if re == 0.0 {
        return format!("{imag}{suffix}");
    }
    let sign = if im > 0.0 { "+" } else { "" };
    format!("{}{sign}{imag}{suffix}", im_number(re))
}

fn clean_zero(v: f64) -> f64 {
    if v.abs() < 1e-300 { 0.0 } else { v }
}

fn complex_arg(c: &mut Ctx, e: &Expr) -> Result<(C, Option<char>), CellError> {
    match c.eval_scalar(e) {
        Value::Number(n) => Ok((C { re: n, im: 0.0 }, None)),
        Value::Bool(_) => Err(CellError::Value),
        Value::Empty => Ok((C::ZERO, None)),
        v => parse_complex(&to_text(&v)?),
    }
}

fn im1(c: &mut Ctx, a: &[Expr], f: fn(C) -> Value) -> Value {
    match complex_arg(c, &a[0]) {
        Ok((z, _)) => f(z),
        Err(e) => err(e),
    }
}

fn im1z(c: &mut Ctx, a: &[Expr], f: fn(C) -> Option<C>) -> Value {
    match complex_arg(c, &a[0]) {
        Ok((z, s)) => match f(z) {
            Some(r) if r.re.is_finite() && r.im.is_finite() => Value::Text(format_complex(r, s.unwrap_or('i'))),
            _ => err(CellError::Num),
        },
        Err(e) => err(e),
    }
}

fn im2(c: &mut Ctx, a: &[Expr], f: fn(C, C) -> Option<C>) -> Value {
    let (x, y) = match (complex_arg(c, &a[0]), complex_arg(c, &a[1])) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    if let (Some(p), Some(q)) = (x.1, y.1)
        && p != q
    {
        return err(CellError::Value);
    }
    match f(x.0, y.0) {
        Some(r) => Value::Text(format_complex(r, x.1.or(y.1).unwrap_or('i'))),
        None => err(CellError::Num),
    }
}

fn im_fold(c: &mut Ctx, a: &[Expr], start: C, f: fn(C, C) -> C) -> Value {
    let mut acc = start;
    let mut suffix: Option<char> = None;
    for (v, _) in super::collect_values(c, a, true) {
        let (z, s) = match v {
            Value::Empty => (C::ZERO, None),
            Value::Number(n) => (C { re: n, im: 0.0 }, None),
            Value::Error(e) => return err(e),
            Value::Bool(_) => return err(CellError::Value),
            v => match to_text(&v).map_err(|e| e).and_then(|t| parse_complex(&t)) {
                Ok(x) => x,
                Err(e) => return err(e),
            },
        };
        if let (Some(p), Some(q)) = (suffix, s)
            && p != q
        {
            return err(CellError::Value);
        }
        suffix = suffix.or(s);
        acc = f(acc, z);
    }
    Value::Text(format_complex(acc, suffix.unwrap_or('i')))
}

fn complex_fn(c: &mut Ctx, a: &[Expr]) -> Value {
    // The suffix first: a bad one wins over the numbers' errors.
    let suffix = match a.get(2) {
        None | Some(Expr::Missing) => 'i',
        Some(_) => match arg_text(c, a, 2) {
            Ok(s) if s == "i" || s == "j" || s.is_empty() => s.chars().next().unwrap_or('i'),
            Ok(_) => return err(CellError::Value),
            Err(e) => return err(e),
        },
    };
    let (re, im) = match (arg_num(c, a, 0), arg_num(c, a, 1)) {
        (Ok(r), Ok(i)) => (r, i),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    Value::Text(format_complex(C { re, im }, suffix))
}

fn impower(c: &mut Ctx, a: &[Expr]) -> Value {
    let (z, s) = match complex_arg(c, &a[0]) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    let p = match arg_num(c, a, 1) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    if z == C::ZERO {
        return if p > 0.0 { Value::Text("0".into()) } else { err(CellError::Num) };
    }
    let r = if z.im == 0.0 && z.re > 0.0 && p.fract() != 0.0 {
        (p * z.re.ln()).exp()
    } else {
        z.re.hypot(z.im).powf(p)
    };
    let t = z.im.atan2(z.re) * p;
    let result = C { re: r * t.cos(), im: r * t.sin() };
    if !result.re.is_finite() || !result.im.is_finite() {
        return err(CellError::Num);
    }
    Value::Text(format_complex(result, s.unwrap_or('i')))
}

// ----- Units ----------------------------------------------------------------

/// A unit: its category, its size in the category's base unit, and
/// whether it takes metric (and binary) prefixes.
struct Unit {
    name: &'static str,
    category: &'static str,
    factor: f64,
    prefixes: bool,
}

const UNITS: &[Unit] = &[
    // Weight and mass (gram).
    Unit { name: "g", category: "mass", factor: 1.0, prefixes: true },
    Unit { name: "sg", category: "mass", factor: 14_593.902_937_206_4, prefixes: false },
    Unit { name: "lbm", category: "mass", factor: 453.592_37, prefixes: false },
    Unit { name: "u", category: "mass", factor: 1.660_539_040e-24, prefixes: true },
    Unit { name: "ozm", category: "mass", factor: 28.349_523_125, prefixes: false },
    Unit { name: "grain", category: "mass", factor: 0.064_798_91, prefixes: false },
    Unit { name: "cwt", category: "mass", factor: 45_359.237, prefixes: false },
    Unit { name: "shweight", category: "mass", factor: 45_359.237, prefixes: false },
    Unit { name: "uk_cwt", category: "mass", factor: 50_802.345_44, prefixes: false },
    Unit { name: "lcwt", category: "mass", factor: 50_802.345_44, prefixes: false },
    Unit { name: "hweight", category: "mass", factor: 50_802.345_44, prefixes: false },
    Unit { name: "stone", category: "mass", factor: 6_350.293_18, prefixes: false },
    Unit { name: "ton", category: "mass", factor: 907_184.74, prefixes: false },
    Unit { name: "uk_ton", category: "mass", factor: 1_016_046.908_8, prefixes: false },
    Unit { name: "LTON", category: "mass", factor: 1_016_046.908_8, prefixes: false },
    Unit { name: "brton", category: "mass", factor: 1_016_046.908_8, prefixes: false },
    // Distance (metre).
    Unit { name: "m", category: "distance", factor: 1.0, prefixes: true },
    Unit { name: "mi", category: "distance", factor: 1_609.344, prefixes: false },
    Unit { name: "Nmi", category: "distance", factor: 1_852.0, prefixes: false },
    Unit { name: "in", category: "distance", factor: 0.0254, prefixes: false },
    Unit { name: "ft", category: "distance", factor: 0.3048, prefixes: false },
    Unit { name: "yd", category: "distance", factor: 0.9144, prefixes: false },
    Unit { name: "ang", category: "distance", factor: 1e-10, prefixes: true },
    Unit { name: "ell", category: "distance", factor: 1.143, prefixes: false },
    Unit { name: "ly", category: "distance", factor: 9_460_730_472_580_800.0, prefixes: true },
    Unit { name: "parsec", category: "distance", factor: 30_856_775_812_815_500.0, prefixes: true },
    Unit { name: "pc", category: "distance", factor: 30_856_775_812_815_500.0, prefixes: true },
    Unit { name: "Pica", category: "distance", factor: 0.0254 / 72.0, prefixes: false },
    Unit { name: "Picapt", category: "distance", factor: 0.0254 / 72.0, prefixes: false },
    Unit { name: "pica", category: "distance", factor: 0.0254 / 6.0, prefixes: false },
    Unit { name: "survey_mi", category: "distance", factor: 1_609.347_218_694_437, prefixes: false },
    // Time (second).
    Unit { name: "yr", category: "time", factor: 31_557_600.0, prefixes: false },
    Unit { name: "day", category: "time", factor: 86_400.0, prefixes: false },
    Unit { name: "d", category: "time", factor: 86_400.0, prefixes: false },
    Unit { name: "hr", category: "time", factor: 3_600.0, prefixes: false },
    Unit { name: "mn", category: "time", factor: 60.0, prefixes: false },
    Unit { name: "min", category: "time", factor: 60.0, prefixes: false },
    Unit { name: "sec", category: "time", factor: 1.0, prefixes: true },
    Unit { name: "s", category: "time", factor: 1.0, prefixes: true },
    // Pressure (pascal).
    Unit { name: "Pa", category: "pressure", factor: 1.0, prefixes: true },
    Unit { name: "p", category: "pressure", factor: 1.0, prefixes: true },
    Unit { name: "atm", category: "pressure", factor: 101_325.0, prefixes: true },
    Unit { name: "at", category: "pressure", factor: 101_325.0, prefixes: true },
    Unit { name: "mmHg", category: "pressure", factor: 133.322_368_421_052_6, prefixes: true },
    Unit { name: "psi", category: "pressure", factor: 6_894.757_293_168_36, prefixes: false },
    Unit { name: "Torr", category: "pressure", factor: 133.322_368_421_052_6, prefixes: false },
    // Force (newton).
    Unit { name: "N", category: "force", factor: 1.0, prefixes: true },
    Unit { name: "dyn", category: "force", factor: 1e-5, prefixes: true },
    Unit { name: "dy", category: "force", factor: 1e-5, prefixes: true },
    Unit { name: "lbf", category: "force", factor: 4.448_221_615_260_5, prefixes: false },
    Unit { name: "pond", category: "force", factor: 0.009_806_65, prefixes: true },
    // Energy (joule).
    Unit { name: "J", category: "energy", factor: 1.0, prefixes: true },
    Unit { name: "e", category: "energy", factor: 1e-7, prefixes: true },
    Unit { name: "c", category: "energy", factor: 4.184, prefixes: true },
    Unit { name: "cal", category: "energy", factor: 4.1868, prefixes: true },
    Unit { name: "eV", category: "energy", factor: 1.602_176_487e-19, prefixes: true },
    Unit { name: "ev", category: "energy", factor: 1.602_176_487e-19, prefixes: true },
    Unit { name: "HPh", category: "energy", factor: 2_684_519.537_696_17, prefixes: false },
    Unit { name: "hh", category: "energy", factor: 2_684_519.537_696_17, prefixes: false },
    Unit { name: "Wh", category: "energy", factor: 3_600.0, prefixes: true },
    Unit { name: "wh", category: "energy", factor: 3_600.0, prefixes: true },
    Unit { name: "flb", category: "energy", factor: 0.042_140_110_093_805_4, prefixes: false },
    Unit { name: "BTU", category: "energy", factor: 1_055.055_852_62, prefixes: false },
    Unit { name: "btu", category: "energy", factor: 1_055.055_852_62, prefixes: false },
    // Power (watt).
    Unit { name: "HP", category: "power", factor: 745.699_871_582_270_2, prefixes: false },
    Unit { name: "h", category: "power", factor: 745.699_871_582_270_2, prefixes: false },
    Unit { name: "PS", category: "power", factor: 735.498_75, prefixes: false },
    Unit { name: "W", category: "power", factor: 1.0, prefixes: true },
    Unit { name: "w", category: "power", factor: 1.0, prefixes: true },
    // Magnetism (tesla).
    Unit { name: "T", category: "magnetism", factor: 1.0, prefixes: true },
    Unit { name: "ga", category: "magnetism", factor: 1e-4, prefixes: true },
    // Volume (cubic metre).
    Unit { name: "l", category: "volume", factor: 1e-3, prefixes: true },
    Unit { name: "L", category: "volume", factor: 1e-3, prefixes: true },
    Unit { name: "lt", category: "volume", factor: 1e-3, prefixes: true },
    Unit { name: "tsp", category: "volume", factor: 4.928_921_593_75e-6, prefixes: false },
    Unit { name: "tspm", category: "volume", factor: 5e-6, prefixes: false },
    Unit { name: "tbs", category: "volume", factor: 1.478_676_478_125e-5, prefixes: false },
    Unit { name: "oz", category: "volume", factor: 2.957_352_956_25e-5, prefixes: false },
    Unit { name: "cup", category: "volume", factor: 2.365_882_365e-4, prefixes: false },
    Unit { name: "pt", category: "volume", factor: 4.731_764_73e-4, prefixes: false },
    Unit { name: "us_pt", category: "volume", factor: 4.731_764_73e-4, prefixes: false },
    Unit { name: "uk_pt", category: "volume", factor: 5.682_612_5e-4, prefixes: false },
    Unit { name: "qt", category: "volume", factor: 9.463_529_46e-4, prefixes: false },
    Unit { name: "uk_qt", category: "volume", factor: 1.136_522_5e-3, prefixes: false },
    Unit { name: "gal", category: "volume", factor: 3.785_411_784e-3, prefixes: false },
    Unit { name: "uk_gal", category: "volume", factor: 4.546_09e-3, prefixes: false },
    Unit { name: "m3", category: "volume", factor: 1.0, prefixes: true },
    Unit { name: "ang3", category: "volume", factor: 1e-30, prefixes: true },
    Unit { name: "barrel", category: "volume", factor: 0.158_987_294_928, prefixes: false },
    Unit { name: "bushel", category: "volume", factor: 0.035_239_070_166_88, prefixes: false },
    Unit { name: "ft3", category: "volume", factor: 0.028_316_846_592, prefixes: false },
    Unit { name: "in3", category: "volume", factor: 1.638_706_4e-5, prefixes: false },
    Unit { name: "ly3", category: "volume", factor: 8.467_866_646_237_2e47, prefixes: false },
    Unit { name: "mi3", category: "volume", factor: 4_168_181_825.440_58, prefixes: false },
    Unit { name: "yd3", category: "volume", factor: 0.764_554_857_984, prefixes: false },
    Unit { name: "Nmi3", category: "volume", factor: 6_352_182_208.0, prefixes: false },
    Unit { name: "MTON", category: "volume", factor: 1.132_673_863_68, prefixes: false },
    Unit { name: "GRT", category: "volume", factor: 2.831_684_659_2, prefixes: false },
    Unit { name: "regton", category: "volume", factor: 2.831_684_659_2, prefixes: false },
    // Area (square metre).
    Unit { name: "m2", category: "area", factor: 1.0, prefixes: true },
    Unit { name: "ha", category: "area", factor: 10_000.0, prefixes: false },
    Unit { name: "uk_acre", category: "area", factor: 4_046.856_422_4, prefixes: false },
    Unit { name: "us_acre", category: "area", factor: 4_046.872_609_874_252, prefixes: false },
    Unit { name: "ang2", category: "area", factor: 1e-20, prefixes: true },
    Unit { name: "ar", category: "area", factor: 100.0, prefixes: true },
    Unit { name: "ft2", category: "area", factor: 0.092_903_04, prefixes: false },
    Unit { name: "in2", category: "area", factor: 6.4516e-4, prefixes: false },
    Unit { name: "ly2", category: "area", factor: 8.950_542_107_488_33e31, prefixes: false },
    Unit { name: "mi2", category: "area", factor: 2_589_988.110_336, prefixes: false },
    Unit { name: "Nmi2", category: "area", factor: 3_429_904.0, prefixes: false },
    Unit { name: "yd2", category: "area", factor: 0.836_127_36, prefixes: false },
    Unit { name: "Morgen", category: "area", factor: 2_500.0, prefixes: false },
    // Information (bit).
    Unit { name: "bit", category: "information", factor: 1.0, prefixes: true },
    Unit { name: "byte", category: "information", factor: 8.0, prefixes: true },
    // Speed (m/s).
    Unit { name: "m/s", category: "speed", factor: 1.0, prefixes: true },
    Unit { name: "m/sec", category: "speed", factor: 1.0, prefixes: true },
    Unit { name: "m/h", category: "speed", factor: 1.0 / 3600.0, prefixes: true },
    Unit { name: "m/hr", category: "speed", factor: 1.0 / 3600.0, prefixes: true },
    Unit { name: "mph", category: "speed", factor: 0.447_04, prefixes: false },
    Unit { name: "kn", category: "speed", factor: 1_852.0 / 3600.0, prefixes: false },
    Unit { name: "admkn", category: "speed", factor: 0.514_773_333_333_333_3, prefixes: false },
];

const PREFIXES: &[(&str, f64)] = &[
    ("Y", 1e24), ("Z", 1e21), ("E", 1e18), ("P", 1e15), ("T", 1e12), ("G", 1e9), ("M", 1e6), ("k", 1e3), ("h", 1e2), ("da", 1e1), ("e", 1e1),
    ("d", 1e-1), ("c", 1e-2), ("m", 1e-3), ("u", 1e-6), ("n", 1e-9), ("p", 1e-12), ("f", 1e-15), ("a", 1e-18), ("z", 1e-21), ("y", 1e-24),
];

const BINARY_PREFIXES: &[(&str, f64)] = &[
    ("Yi", 1_208_925_819_614_629_174_706_176.0), ("Zi", 1_180_591_620_717_411_303_424.0), ("Ei", 1_152_921_504_606_846_976.0), ("Pi", 1_125_899_906_842_624.0),
    ("Ti", 1_099_511_627_776.0), ("Gi", 1_073_741_824.0), ("Mi", 1_048_576.0), ("ki", 1_024.0),
];

/// A unit name (with a prefix, perhaps) as its category and size.
fn find_unit(name: &str) -> Option<(&'static str, f64, bool)> {
    if let Some(u) = UNITS.iter().find(|u| u.name == name) {
        return Some((u.category, u.factor, false));
    }
    if name == "C" || name == "cel" {
        return Some(("temperature", 0.0, true));
    }
    for t in ["F", "fah", "K", "kel", "Rank", "Reau"] {
        if name == t {
            return Some(("temperature", 0.0, true));
        }
    }
    for (p, f) in BINARY_PREFIXES.iter().chain(PREFIXES.iter()) {
        if let Some(rest) = name.strip_prefix(p)
            && let Some(u) = UNITS.iter().find(|u| u.name == rest && u.prefixes)
        {
            if BINARY_PREFIXES.iter().any(|(bp, _)| bp == p) && u.category != "information" {
                continue;
            }
            // Squared and cubed units take the prefix squared or cubed.
            let power = if rest.ends_with('2') { 2 } else if rest.ends_with('3') { 3 } else { 1 };
            return Some((u.category, u.factor * f.powi(power), false));
        }
        if let Some(rest) = name.strip_prefix(p)
            && ["K", "kel"].contains(&rest)
        {
            return Some(("temperature-k", *f, true));
        }
    }
    None
}

fn to_kelvin(v: f64, unit: &str) -> f64 {
    match unit {
        "C" | "cel" => v + 273.15,
        "F" | "fah" => (v - 32.0) * 5.0 / 9.0 + 273.15,
        "Rank" => v * 5.0 / 9.0,
        "Reau" => v * 1.25 + 273.15,
        _ => v,
    }
}

fn from_kelvin(k: f64, unit: &str) -> f64 {
    match unit {
        "C" | "cel" => k - 273.15,
        "F" | "fah" => (k - 273.15) * 9.0 / 5.0 + 32.0,
        "Rank" => k * 9.0 / 5.0,
        "Reau" => (k - 273.15) * 0.8,
        _ => k,
    }
}

fn convert(c: &mut Ctx, a: &[Expr]) -> Value {
    let v = match arg_num(c, a, 0) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    let (from, to) = match (arg_text(c, a, 1), arg_text(c, a, 2)) {
        (Ok(f), Ok(t)) => (f, t),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    let (Some((cf, ff, tf)), Some((ct, ft, tt))) = (find_unit(&from), find_unit(&to)) else {
        return err(CellError::NA);
    };
    if tf || tt {
        if !(tf && tt) {
            return err(CellError::NA);
        }
        // Temperatures (a prefixed kelvin scales).
        let kf = if cf == "temperature-k" { v * ff } else { to_kelvin(v, &from) };
        let r = if ct == "temperature-k" { kf / ft } else { from_kelvin(kf, &to) };
        return number(r);
    }
    if cf != ct {
        return err(CellError::NA);
    }
    number(v * ff / ft)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(f: fn(f64, i32) -> Result<f64, CellError>, name: &str, cases: &[(f64, i32, f64)]) {
        for &(x, n, want) in cases {
            let got = f(x, n).unwrap_or_else(|e| panic!("{name}({x}, {n}) = {e:?}"));
            let rel = ((got - want) / want).abs();
            assert!(rel < 1e-9, "{name}({x}, {n}) = {got}, want {want} (relative error {rel:e})");
        }
    }

    // Reference values: the defining power series
    // Σ (∓1)^k (x/2)^(2k+n) / (k! (n+k)!) summed in exact rational arithmetic
    // (Python `fractions`) until the remaining terms are below 1e-60 relative.

    #[test]
    fn besselj_is_accurate_when_order_and_x_are_comparable() {
        assert_close(
            besselj,
            "J",
            &[
                (25.0, 20, 0.051_994_049_228_303_232),
                (30.0, 25, 0.084_292_740_643_031_729),
                (100.0, 50, -0.038_698_339_728_525_383),
                (50.0, 49, 0.151_195_142_521_472_24),
                (200.0, 150, -0.031_593_559_273_458_418),
                (-25.0, 21, -0.164_623_803_684_942_98),
                (26.0, 100, 4.915_267_118_592_869_7e-48),
            ],
        );
    }

    #[test]
    fn besselj_is_accurate_for_low_orders_at_moderate_x() {
        assert_close(
            besselj,
            "J",
            &[
                (30.0, 0, -0.086_367_983_581_040_211),
                (24.0, 0, -0.056_230_274_166_859_267),
                (25.0, 1, -0.125_350_249_580_289_9),
                (1000.0, 2, -0.024_777_229_528_605_996),
                (3.0, 2, 0.486_091_260_585_891_08),
                (7.0, 30, 5.317_260_794_010_017_7e-17),
            ],
        );
    }

    #[test]
    fn besseli_is_accurate_at_large_x_and_order() {
        assert_close(
            besseli,
            "I",
            &[
                (25.0, 30, 337.207_268_604_940_65),
                (40.0, 50, 5_726.865_663_128_989_7),
                (30.0, 0, 781_672_297_823.977_49),
                (100.0, 10, 6.498_975_524_720_147_8e41),
                (-30.0, 5, -512_151_465_476.934_97),
                (3.0, 1, 3.953_370_217_402_609_4),
                (700.0, 5, 1.502_502_532_192_868_8e302),
                (2.0, 100, 1.082_171_474_549_860_6e-158),
            ],
        );
    }
}
