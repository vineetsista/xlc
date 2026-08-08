//! The five distributions (§8.6): normal, lognormal, uniform, triangular,
//! PERT — and nothing else. XLC competes on the engine, not the stats
//! menu.
//!
//! Inverse-normal is Wichura's AS241 (double precision, ~1e-15 relative),
//! so normal draws are a pure function of one uniform — no rejection, no
//! state, perfectly counter-addressable. PERT uses the beta-from-two-
//! gammas construction with Marsaglia–Tsang sampling; its rejection loop
//! consumes the `attempt` axis of the draw address.

use crate::rng::{uniform, uniform2, DrawAddr};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dist {
    /// Degenerate point mass (the N=1 oracle uses this).
    Point { value: f64 },
    Normal { mean: f64, sd: f64 },
    LogNormal { mu: f64, sigma: f64 },
    Uniform { a: f64, b: f64 },
    Triangular { a: f64, m: f64, b: f64 },
    Pert { a: f64, m: f64, b: f64 },
}

impl Dist {
    /// Analytic mean (for the moment oracle).
    pub fn mean(&self) -> f64 {
        match *self {
            Dist::Point { value } => value,
            Dist::Normal { mean, .. } => mean,
            Dist::LogNormal { mu, sigma } => (mu + sigma * sigma / 2.0).exp(),
            Dist::Uniform { a, b } => (a + b) / 2.0,
            Dist::Triangular { a, m, b } => (a + m + b) / 3.0,
            Dist::Pert { a, m, b } => (a + 4.0 * m + b) / 6.0,
        }
    }

    /// Analytic variance (for the moment oracle).
    pub fn variance(&self) -> f64 {
        match *self {
            Dist::Point { .. } => 0.0,
            Dist::Normal { sd, .. } => sd * sd,
            Dist::LogNormal { mu, sigma } => {
                let s2 = sigma * sigma;
                (s2.exp() - 1.0) * (2.0 * mu + s2).exp()
            }
            Dist::Uniform { a, b } => (b - a) * (b - a) / 12.0,
            Dist::Triangular { a, m, b } => {
                (a * a + m * m + b * b - a * m - a * b - m * b) / 18.0
            }
            Dist::Pert { a, m, b } => {
                // Beta(alpha, beta) scaled to [a, b] with the PERT
                // parameterization (lambda = 4).
                let (al, be) = pert_shape(a, m, b);
                let n = al + be;
                (b - a) * (b - a) * al * be / (n * n * (n + 1.0))
            }
        }
    }

    /// Draw the value for one (seed, cell, scenario) address.
    pub fn sample(&self, addr: DrawAddr) -> f64 {
        match *self {
            Dist::Point { value } => value,
            Dist::Normal { mean, sd } => mean + sd * inv_norm(uniform(addr)),
            Dist::LogNormal { mu, sigma } => (mu + sigma * inv_norm(uniform(addr))).exp(),
            Dist::Uniform { a, b } => a + (b - a) * uniform(addr),
            Dist::Triangular { a, m, b } => {
                let u = uniform(addr);
                let fc = (m - a) / (b - a);
                if u < fc {
                    a + ((b - a) * (m - a) * u).sqrt()
                } else {
                    b - ((b - a) * (b - m) * (1.0 - u)).sqrt()
                }
            }
            Dist::Pert { a, m, b } => {
                let (al, be) = pert_shape(a, m, b);
                let x = gamma_sample(al, DrawAddr { draw: addr.draw, ..addr });
                let y = gamma_sample(be, DrawAddr { draw: addr.draw + 1, ..addr });
                a + (b - a) * (x / (x + y))
            }
        }
    }
}

fn pert_shape(a: f64, m: f64, b: f64) -> (f64, f64) {
    let alpha = 1.0 + 4.0 * (m - a) / (b - a);
    let beta = 1.0 + 4.0 * (b - m) / (b - a);
    (alpha, beta)
}

/// Marsaglia–Tsang gamma sampler (shape >= 1 after boost), rejection loop
/// walking the `attempt` axis so determinism survives.
fn gamma_sample(shape: f64, addr: DrawAddr) -> f64 {
    let boosted = shape < 1.0;
    let d_shape = if boosted { shape + 1.0 } else { shape };
    let d = d_shape - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    let mut attempt = 0u16;
    loop {
        let (u1, u2) = uniform2(DrawAddr { attempt, ..addr });
        let x = inv_norm(u1);
        let v = 1.0 + c * x;
        if v > 0.0 {
            let v3 = v * v * v;
            if u2.ln() < 0.5 * x * x + d - d * v3 + d * (v3.ln()) {
                let g = d * v3;
                return if boosted {
                    // Boost correction: G(a) = G(a+1) * U^(1/a).
                    let (u3, _) = uniform2(DrawAddr { attempt: attempt | 0x8000, ..addr });
                    g * u3.powf(1.0 / shape)
                } else {
                    g
                };
            }
        }
        attempt += 1;
        assert!(attempt < 0x7fff, "gamma sampler runaway");
    }
}

/// Wichura AS241 (PPND16): inverse of the standard normal CDF, double
/// precision. WASM has no SIMD transcendentals, so this polynomial form
/// is also the vectorization-friendly path.
pub fn inv_norm(p: f64) -> f64 {
    debug_assert!(p > 0.0 && p < 1.0);
    let q = p - 0.5;
    if q.abs() <= 0.425 {
        let r = 0.180625 - q * q;
        return q * poly(&A, r) / poly(&B, r);
    }
    let r = if q < 0.0 { p } else { 1.0 - p };
    let r = (-r.ln()).sqrt();
    let x = if r <= 5.0 {
        let r = r - 1.6;
        poly(&C, r) / poly(&D, r)
    } else {
        let r = r - 5.0;
        poly(&E, r) / poly(&F, r)
    };
    if q < 0.0 {
        -x
    } else {
        x
    }
}

fn poly(c: &[f64], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, &k| acc * x + k)
}

const A: [f64; 8] = [
    3.387_132_872_796_366_5e0,
    1.331_416_678_917_843_8e2,
    1.971_590_950_306_551_3e3,
    1.373_169_376_550_946e4,
    4.592_195_393_154_987e4,
    6.726_577_092_700_87e4,
    3.343_057_558_358_813e4,
    2.509_080_928_730_122_7e3,
];
const B: [f64; 8] = [
    1.0,
    4.231_333_070_160_091e1,
    6.871_870_074_920_579e2,
    5.394_196_021_424_751e3,
    2.121_379_430_415_576e4,
    3.930_789_580_009_271e4,
    2.872_908_573_572_194_3e4,
    5.226_495_278_852_545_5e3,
];
const C: [f64; 8] = [
    1.423_437_110_749_683_5e0,
    4.630_337_846_156_546e0,
    5.769_497_221_460_691e0,
    3.647_848_324_763_204_5e0,
    1.270_458_252_452_368_4e0,
    2.417_807_251_774_506e-1,
    2.272_384_498_926_918_4e-2,
    7.745_450_142_783_414e-4,
];
const D: [f64; 8] = [
    1.0,
    2.053_191_626_637_759e0,
    1.676_384_830_183_803_8e0,
    6.897_673_349_851e-1,
    1.481_039_764_274_800_8e-1,
    1.519_866_656_361_645_7e-2,
    5.475_938_084_995_345e-4,
    1.050_750_071_644_416_9e-9,
];
const E: [f64; 8] = [
    6.657_904_643_501_103e0,
    5.463_784_911_164_114e0,
    1.784_826_539_917_291_3e0,
    2.965_605_718_285_048_7e-1,
    2.653_218_952_657_612_4e-2,
    1.242_660_947_388_078_4e-3,
    2.711_555_568_743_487_6e-5,
    2.010_334_399_292_288_1e-7,
];
const F: [f64; 8] = [
    1.0,
    5.998_322_065_558_88e-1,
    1.369_298_809_227_358e-1,
    1.487_536_129_085_061_5e-2,
    7.868_691_311_456_133e-4,
    1.846_318_317_510_054_8e-5,
    1.421_511_758_316_446e-7,
    2.044_263_103_389_939_7e-15,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv_norm_known_points() {
        assert!((inv_norm(0.5)).abs() < 1e-15);
        assert!((inv_norm(0.975) - 1.959_963_984_540_054).abs() < 1e-12);
        assert!((inv_norm(0.025) + 1.959_963_984_540_054).abs() < 1e-12);
        assert!((inv_norm(0.999_999) - 4.753_424_308_822_899).abs() < 1e-9);
        // Round-trip through the tail.
        assert!(inv_norm(1e-12) < -6.9);
        assert!(inv_norm(1.0 - 1e-12) > 6.9);
    }

    #[test]
    fn moments_all_five() {
        let n = 400_000u32;
        for (name, d) in [
            ("normal", Dist::Normal { mean: 3.0, sd: 2.0 }),
            ("lognormal", Dist::LogNormal { mu: 0.2, sigma: 0.5 }),
            ("uniform", Dist::Uniform { a: -1.0, b: 5.0 }),
            ("triangular", Dist::Triangular { a: 1.0, m: 4.0, b: 11.0 }),
            ("pert", Dist::Pert { a: 1.0, m: 4.0, b: 11.0 }),
        ] {
            let mut sum = 0.0;
            let mut sumsq = 0.0;
            for k in 0..n {
                let x = d.sample(DrawAddr { seed: 7, cell: 1, scenario: k, draw: 0, attempt: 0 });
                sum += x;
                sumsq += x * x;
            }
            let mean = sum / n as f64;
            let var = sumsq / n as f64 - mean * mean;
            let mean_se = (d.variance() / n as f64).sqrt();
            assert!(
                (mean - d.mean()).abs() < 6.0 * mean_se,
                "{name}: mean {mean} vs {} (se {mean_se})",
                d.mean()
            );
            assert!(
                (var - d.variance()).abs() / d.variance() < 0.02,
                "{name}: var {var} vs {}",
                d.variance()
            );
        }
    }

    #[test]
    fn point_is_degenerate() {
        let d = Dist::Point { value: 42.5 };
        for k in 0..10 {
            assert_eq!(
                d.sample(DrawAddr { seed: 1, cell: 0, scenario: k, draw: 0, attempt: 0 }),
                42.5
            );
        }
    }
}
