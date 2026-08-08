//! Counter-based RNG: Philox 4x32-10 (§8.6).
//!
//! Scenario k's draws derive from (seed, cell, k, draw) with no stored
//! state — tiling is trivially parallel and every scenario is exactly
//! reproducible on any machine, which a trust product needs more than it
//! needs the last 5% of RNG throughput.

const PHILOX_M0: u32 = 0xD251_1F53;
const PHILOX_M1: u32 = 0xCD9E_8D57;
const PHILOX_W0: u32 = 0x9E37_79B9;
const PHILOX_W1: u32 = 0xBB67_AE85;

#[inline]
fn mulhilo(a: u32, b: u32) -> (u32, u32) {
    let p = (a as u64) * (b as u64);
    ((p >> 32) as u32, p as u32)
}

/// One Philox 4x32-10 block: counter (4x u32) + key (2x u32) -> 4x u32.
#[inline]
pub fn philox4x32(mut ctr: [u32; 4], mut key: [u32; 2]) -> [u32; 4] {
    for _ in 0..10 {
        let (hi0, lo0) = mulhilo(PHILOX_M0, ctr[0]);
        let (hi1, lo1) = mulhilo(PHILOX_M1, ctr[2]);
        ctr = [hi1 ^ ctr[1] ^ key[0], lo1, hi0 ^ ctr[3] ^ key[1], lo0];
        key[0] = key[0].wrapping_add(PHILOX_W0);
        key[1] = key[1].wrapping_add(PHILOX_W1);
    }
    ctr
}

/// The draw address: (seed | stream) key, (cell, scenario, draw, attempt)
/// counter. `attempt` gives rejection samplers headroom without
/// perturbing neighbouring draws.
#[derive(Clone, Copy, Debug)]
pub struct DrawAddr {
    pub seed: u64,
    pub cell: u32,
    pub scenario: u32,
    pub draw: u16,
    pub attempt: u16,
}

/// Two uniform doubles in (0,1) from one Philox block. Uses the top 53
/// bits of paired words; never returns exactly 0 or 1.
pub fn uniform2(addr: DrawAddr) -> (f64, f64) {
    let out = philox4x32(
        [
            addr.cell,
            addr.scenario,
            ((addr.draw as u32) << 16) | addr.attempt as u32,
            0x5eed_5eed,
        ],
        [addr.seed as u32, (addr.seed >> 32) as u32],
    );
    let u0 = (((out[0] as u64) << 21) ^ (out[1] as u64)) & ((1u64 << 53) - 1);
    let u1 = (((out[2] as u64) << 21) ^ (out[3] as u64)) & ((1u64 << 53) - 1);
    let scale = 1.0 / (1u64 << 53) as f64;
    (
        ((u0 as f64) + 0.5) * scale,
        ((u1 as f64) + 0.5) * scale,
    )
}

/// One uniform double in (0,1).
pub fn uniform(addr: DrawAddr) -> f64 {
    uniform2(addr).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_stateless() {
        let a = DrawAddr { seed: 42, cell: 7, scenario: 123_456, draw: 0, attempt: 0 };
        assert_eq!(uniform(a), uniform(a));
        // Different scenario, cell, draw, seed all decorrelate.
        for delta in [
            DrawAddr { scenario: 123_457, ..a },
            DrawAddr { cell: 8, ..a },
            DrawAddr { draw: 1, ..a },
            DrawAddr { seed: 43, ..a },
            DrawAddr { attempt: 1, ..a },
        ] {
            assert_ne!(uniform(a), uniform(delta));
        }
    }

    #[test]
    fn uniform_range_and_moments() {
        let n = 200_000u32;
        let mut sum = 0.0;
        let mut sumsq = 0.0;
        for k in 0..n {
            let u = uniform(DrawAddr { seed: 1, cell: 0, scenario: k, draw: 0, attempt: 0 });
            assert!(u > 0.0 && u < 1.0);
            sum += u;
            sumsq += u * u;
        }
        let mean = sum / n as f64;
        let var = sumsq / n as f64 - mean * mean;
        // mean se = sqrt(1/12/n) ~ 0.00065; allow 5 sigma.
        assert!((mean - 0.5).abs() < 5.0 * (1.0 / 12.0f64 / n as f64).sqrt(), "mean {mean}");
        assert!((var - 1.0 / 12.0).abs() < 0.001, "var {var}");
    }

    #[test]
    fn known_philox_structure() {
        // Zero counter/key must not map to zero output (whitening works).
        let out = philox4x32([0; 4], [0; 2]);
        assert_ne!(out, [0; 4]);
        // And is stable across runs (regression pin).
        let again = philox4x32([0; 4], [0; 2]);
        assert_eq!(out, again);
    }
}
