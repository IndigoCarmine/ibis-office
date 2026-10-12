// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Special functions for the statistical and engineering functions: the
//! error function, the gamma and beta functions and their incomplete
//! forms, the normal quantile, and a root finder for the inverses.

use std::f64::consts::{FRAC_2_SQRT_PI, PI};

/// erf(x), to full double precision (series near 0, continued fraction
/// through erfc beyond).
pub fn erf(x: f64) -> f64 {
    if x.abs() == 1.0 {
        return if x < 0.0 { -0.84270079294971489 } else { 0.84270079294971489 };
    }
    if x.abs() < 2.0 {
        // Maclaurin series: 2/√π Σ (-1)^n x^(2n+1) / (n! (2n+1)).
        let mut sum = x;
        let mut term = x;
        let x2 = x * x;
        let mut n = 0.0;
        while n < 200.0 {
            n += 1.0;
            term *= -x2 / n;
            let add = term / (2.0 * n + 1.0);
            sum += add;
            if add.abs() <= 1e-17 * sum.abs() {
                break;
            }
        }
        sum * FRAC_2_SQRT_PI
    } else {
        let s = if x < 0.0 { -1.0 } else { 1.0 };
        s * (1.0 - erfc(x.abs()))
    }
}

/// erfc(x) = 1 - erf(x), accurate for large x.
pub fn erfc(x: f64) -> f64 {
    if x == -1.0 {
        return 1.842_700_792_949_714_8;
    }
    if x < 0.0 {
        return 1.0 + erf(-x);
    }
    if x < 2.0 {
        return 1.0 - erf(x);
    }
    if x > 27.3 {
        return 0.0;
    }
    // Continued fraction (Lentz): erfc(x) = exp(-x²)/√π · 1/(x + 1/2/(x + 1/(x + 3/2/(x + …)))).
    let tiny = 1e-300;
    let mut f = x;
    let mut c = x;
    let mut d = 0.0;
    let mut n = 1.0;
    for _ in 0..500 {
        let a = n / 2.0;
        d = x + a * d;
        d = if d.abs() < tiny { tiny } else { d };
        c = x + a / c;
        c = if c.abs() < tiny { tiny } else { c };
        d = 1.0 / d;
        let delta = c * d;
        f *= delta;
        n += 1.0;
        if (delta - 1.0).abs() < 1e-16 {
            break;
        }
    }
    (-x * x).exp() / (f * PI.sqrt())
}

/// ln Γ(x) for x > 0 (Lanczos, g = 7, n = 9, with the reflection formula
/// below 0.5).
pub fn ln_gamma(x: f64) -> f64 {
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        return (PI / (PI * x).sin().abs()).ln() - ln_gamma(1.0 - x);
    }
    // Exact for small integers.
    if x == x.floor() && x <= 171.0 {
        let mut f = 1.0f64;
        let mut k = 2.0;
        while k < x {
            f *= k;
            k += 1.0;
        }
        return f.ln();
    }
    // Reduce argument for better Lanczos precision near small values: ln(Γ(x)) = ln(Γ(x-1)) + ln(x-1)
    if x > 2.0 && x < 3.0 {
        return ln_gamma(x - 1.0) + (x - 1.0).ln();
    }
    let x = x - 1.0;
    let mut a = C[0];
    let t = x + G + 0.5;
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// Γ(x), including negative non-integers.
pub fn gamma(x: f64) -> f64 {
    if x == x.floor() && x > 0.0 && x <= 171.0 {
        let mut f = 1.0f64;
        let mut k = x - 1.0;
        while k >= 2.0 {
            f *= k;
            k -= 1.0;
        }
        return f;
    }
    if (x - 0.5).fract() == 0.0 && x > 0.0 {
        let mut f = PI.sqrt();
        let mut k = x - 1.0;
        while k >= 0.5 {
            f *= k;
            k -= 1.0;
        }
        return f;
    }
    if x < 0.5 {
        return PI / ((PI * x).sin() * gamma(1.0 - x));
    }
    ln_gamma(x).exp()
}

/// The regularized lower incomplete gamma function P(a, x).
pub fn gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x < a + 1.0 {
        // Series.
        let mut sum = 1.0 / a;
        let mut term = sum;
        let mut n = a;
        for _ in 0..10_000 {
            n += 1.0;
            term *= x / n;
            sum += term;
            if term.abs() <= sum.abs() * 1e-17 {
                break;
            }
        }
        (sum.ln() - x + a * x.ln() - ln_gamma(a)).exp()
    } else {
        1.0 - gamma_q_cf(a, x)
    }
}

/// The regularized upper incomplete gamma function Q(a, x) = 1 - P(a, x).
pub fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if a == 1.0 {
        return (-x).exp();
    }
    if x < a + 1.0 { 1.0 - gamma_p(a, x) } else { gamma_q_cf(a, x) }
}

fn gamma_q_cf(a: f64, x: f64) -> f64 {
    let tiny = 1e-300;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / tiny;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..10_000 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < tiny {
            d = tiny;
        }
        c = b + an / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < 1e-16 {
            break;
        }
    }
    (-x + a * x.ln() - ln_gamma(a)).exp() * h
}

/// ln B(a, b).
pub fn ln_beta(a: f64, b: f64) -> f64 {
    ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b)
}

/// The regularized incomplete beta function I_x(a, b).
pub fn beta_inc(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let front = (a * x.ln() + b * (1.0 - x).ln() - ln_beta(a, b)).exp();
    if x < (a + 1.0) / (a + b + 2.0) { front * beta_cf(a, b, x) / a } else { 1.0 - front * beta_cf(b, a, 1.0 - x) / b }
}

fn beta_cf(a: f64, b: f64, x: f64) -> f64 {
    let tiny = 1e-300;
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..10_000 {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < 1e-16 {
            break;
        }
    }
    h
}

/// The standard normal distribution function Φ(z).
pub fn norm_cdf(z: f64) -> f64 {
    0.5 * erfc(-z / std::f64::consts::SQRT_2)
}

/// Standard normal distribution CDF using Cody's near-minimax rational
/// Chebyshev approximation (ANORM, Algorithm 715 / SPECFUN).
pub fn cody_norm_cdf(z: f64) -> f64 {
    let y = z.abs();
    if y <= 0.67448975 {
        const A: [f64; 5] = [
            2.2352520354606839287,
            161.02823106855587881,
            1067.6894854603709582,
            18154.981253343561249,
            0.065682337918207449113,
        ];
        const B: [f64; 4] = [
            47.20258190468824187,
            976.09855173777669322,
            10260.932208618978205,
            45507.789335026729956,
        ];
        let xsq = z * z;
        let mut xnum = A[4] * xsq;
        let mut xden = xsq;
        for i in 0..3 {
            xnum = (xnum + A[i]) * xsq;
            xden = (xden + B[i]) * xsq;
        }
        let temp = z * (xnum + A[3]) / (xden + B[3]);
        0.5 + temp
    } else if y <= 5.656854249492381 {
        // y <= sqrt(32)
        const C: [f64; 9] = [
            0.39894151208813466764,
            8.8831497943883759412,
            93.506656132177855979,
            597.27027639480026226,
            2494.5375852903726711,
            6848.1904505362823326,
            11602.651437647350124,
            9842.7148383839780218,
            1.0765576773720192317e-8,
        ];
        const D: [f64; 8] = [
            22.266688044328115691,
            235.38790178262499861,
            1519.377599407554805,
            6485.558298266760755,
            18615.571640885098091,
            34900.952721145977266,
            38912.003286093271411,
            19685.429676859990727,
        ];
        let mut xnum = C[8] * y;
        let mut xden = y;
        for i in 0..7 {
            xnum = (xnum + C[i]) * y;
            xden = (xden + D[i]) * y;
        }
        let temp = (xnum + C[7]) / (xden + D[7]);
        let xsq = (y * 16.0).trunc() / 16.0;
        let del = (y - xsq) * (y + xsq);
        let res = (-xsq * xsq / 2.0).exp() * (-del / 2.0).exp() * temp;
        if z < 0.0 { res } else { 1.0 - res }
    } else {
        if y > 38.0 {
            return if z < 0.0 { 0.0 } else { 1.0 };
        }
        const P: [f64; 6] = [
            0.21589853405795699,
            0.1274011611602473639,
            0.022235277870649807,
            0.001421619193227893466,
            2.9112874951168792e-5,
            0.02307344176494017303,
        ];
        const Q: [f64; 5] = [
            1.28426009614491121,
            0.468238212480865118,
            0.0659881378689285515,
            0.00378239633202758244,
            7.29751555083966205e-5,
        ];
        let xsq = 1.0 / (y * y);
        let mut xnum = P[5] * xsq;
        let mut xden = xsq;
        for i in 0..4 {
            xnum = (xnum + P[i]) * xsq;
            xden = (xden + Q[i]) * xsq;
        }
        let temp = xsq * (xnum + P[4]) / (xden + Q[4]);
        const INV_SQRT_2PI: f64 = 0.39894228040143267793994605993438187;
        let temp = (INV_SQRT_2PI - temp) / y;
        let xsq = (y * 16.0).trunc() / 16.0;
        let del = (y - xsq) * (y + xsq);
        let res = (-xsq * xsq / 2.0).exp() * (-del / 2.0).exp() * temp;
        if z < 0.0 { res } else { 1.0 - res }
    }
}

pub fn norm_pdf(z: f64) -> f64 {
    (-0.5 * z * z).exp() / (2.0 * PI).sqrt()
}

/// The standard normal quantile Φ⁻¹(p), 0 < p < 1 (Wichura's AS 241,
/// PPND16, then one Newton step).
pub fn norm_inv(p: f64) -> f64 {
    let q = p - 0.5;
    if q.abs() <= 0.425 {
        let r = 0.180625 - q * q;
        let mut x = q * (((((((2509.080_928_730_122_7 * r + 33430.575_583_588_128) * r + 67265.770_927_008_7) * r + 45921.953_931_549_87) * r + 13731.693_765_509_461) * r
            + 1971.590_950_306_551_3)
            * r
            + 133.141_667_891_784_38)
            * r
            + 3.387_132_872_796_366_5)
            / (((((((5226.495_278_852_545 * r + 28729.085_735_721_943) * r + 39307.895_800_092_71) * r + 21213.794_301_586_597) * r + 5394.196_021_424_751) * r
                + 687.187_007_492_057_9)
                * r
                + 42.313_330_701_600_91)
                * r
                + 1.0);
        let e = norm_cdf(x) - p;
        let d = norm_pdf(x);
        if d > 0.0 && e.abs() > 0.0 {
            x -= e / d;
        }
        x
    } else {
        let mut rr = if q < 0.0 { p } else { 1.0 - p };
        rr = (-rr.ln()).sqrt();
        let v = if rr <= 5.0 {
            let rr = rr - 1.6;
            (((((((7.745_450_142_783_414e-4 * rr + 0.022_723_844_989_269_184) * rr + 0.241_780_725_177_450_6) * rr + 1.270_458_252_452_368_4) * rr
                + 3.647_848_324_763_204_5)
                * rr
                + 5.769_497_221_460_691)
                * rr
                + 4.630_337_846_156_546)
                * rr
                + 1.423_437_110_749_683_5)
                / (((((((1.050_750_071_644_416_9e-9 * rr + 5.475_938_084_995_345e-4) * rr + 0.015_198_666_563_616_457) * rr + 0.148_103_976_427_480_08) * rr
                    + 0.689_767_334_985_1)
                    * rr
                    + 1.676_384_830_183_803_8)
                    * rr
                    + 2.053_191_626_637_759)
                    * rr
                    + 1.0)
        } else {
            let rr = rr - 5.0;
            (((((((2.010_334_399_292_288_1e-7 * rr + 2.711_555_568_743_487_6e-5) * rr + 0.001_242_660_947_388_078_4) * rr + 0.026_532_189_526_576_124) * rr
                + 0.296_560_571_828_504_9)
                * rr
                + 1.784_826_539_917_291_3)
                * rr
                + 5.463_784_911_164_114)
                * rr
                + 6.657_904_643_501_103)
                / (((((((2.044_263_103_389_939_7e-15 * rr + 1.421_511_758_316_446e-7) * rr + 1.846_318_317_510_054_8e-5) * rr + 7.868_691_311_456_133e-4) * rr
                    + 0.014_875_361_290_850_615)
                    * rr
                    + 0.136_929_880_922_735_8)
                    * rr
                    + 0.599_832_206_555_888)
                    * rr
                    + 1.0)
        };
        if q < 0.0 { -v } else { v }
    }
}

/// The value where the increasing function `f` reaches `target`, in
/// `[lo, hi]` (bisection with secant steps). `None` if not bracketed.
pub fn solve(f: impl Fn(f64) -> f64, target: f64, mut lo: f64, mut hi: f64) -> Option<f64> {
    let (mut flo, mut fhi) = (f(lo) - target, f(hi) - target);
    if flo.is_nan() || fhi.is_nan() {
        return None;
    }
    if flo == 0.0 {
        return Some(lo);
    }
    if fhi == 0.0 {
        return Some(hi);
    }
    if flo.signum() == fhi.signum() {
        return None;
    }
    for i in 0..400 {
        // Alternate secant and bisection steps.
        let mid = if i % 2 == 0 && (fhi - flo).abs() > 0.0 {
            let s = lo - flo * (hi - lo) / (fhi - flo);
            if s > lo && s < hi { s } else { 0.5 * (lo + hi) }
        } else {
            0.5 * (lo + hi)
        };
        let fm = f(mid) - target;
        if fm == 0.0 || (hi - lo).abs() <= 1e-15 * mid.abs().max(1e-300) {
            return Some(mid);
        }
        if fm.signum() == flo.signum() {
            lo = mid;
            flo = fm;
        } else {
            hi = mid;
            fhi = fm;
        }
    }
    Some(0.5 * (lo + hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-14 * b.abs().max(1e-300)
    }

    #[test]
    fn values() {
        assert_eq!(erfc(-1.0), 1.842_700_792_949_714_8);
        assert!(close(erf(0.5), 0.520_499_877_813_046_5));
        assert!(close(erfc(3.0), 2.209_049_699_858_544e-5));
        assert!(close(gamma(5.0), 24.0));
        assert!(close(gamma(0.5), PI.sqrt()));
        assert!(close(ln_gamma(10.5), 13.940_625_219_403_763));
        assert!(close(norm_cdf(1.0), 0.841_344_746_068_542_9));
        assert!(close(norm_inv(0.975), 1.959_963_984_540_054));
        assert!(close(gamma_p(2.0, 3.0), 0.800_851_726_528_544_1));
        assert!(close(beta_inc(2.0, 3.0, 0.4), 0.5248));
    }
}
