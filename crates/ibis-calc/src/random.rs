// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The random numbers of `RAND`, `RANDBETWEEN` and `RANDARRAY`: a SplitMix64
//! generator seeded from the clock, so each calculation gives new ones as
//! Excel's volatile functions do. The tools that compare the engine with
//! Excel's saved values (whose random numbers cannot be compared) switch the
//! generator off with [`use_stand_ins`]; the functions then give fixed
//! stand-in values and mark the result as not calculated.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static STATE: AtomicU64 = AtomicU64::new(0);
static STAND_IN: AtomicBool = AtomicBool::new(false);

/// Give fixed stand-in values instead of random numbers (for comparisons with Excel).
pub fn use_stand_ins(on: bool) {
    STAND_IN.store(on, Ordering::Relaxed);
}

fn seed() -> u64 {
    // The clock where there is one; in the browser, the address of a local (the sequence differs from run to run).
    crate::clock::unix_nanos().unwrap_or_else(|| {
        let local = 0u8;
        (std::ptr::addr_of!(local) as usize as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }) | 1
}

/// The next random number in [0, 1), or `None` when stand-ins are asked for.
pub fn next() -> Option<f64> {
    if STAND_IN.load(Ordering::Relaxed) {
        return None;
    }
    let mut s = STATE.load(Ordering::Relaxed);
    if s == 0 {
        s = seed();
    }
    let step = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
    STATE.store(step, Ordering::Relaxed);
    let mut z = step;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    Some((z >> 11) as f64 / (1u64 << 53) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_in_range_and_differ() {
        use_stand_ins(false);
        let v: Vec<f64> = (0..1000).filter_map(|_| next()).collect();
        assert_eq!(v.len(), 1000);
        assert!(v.iter().all(|x| (0.0..1.0).contains(x)));
        let mean = v.iter().sum::<f64>() / 1000.0;
        assert!((0.4..0.6).contains(&mean), "{mean}");
        assert!(v.windows(2).any(|w| w[0] != w[1]));
    }
}
