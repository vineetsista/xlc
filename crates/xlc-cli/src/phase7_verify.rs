//! Gate 7 artifact: incremental latency, AD vs central differences over
//! random smooth models, and the semantic-diff witness.

use std::fs;
use std::path::Path;
use std::time::Instant;

use xlc_eval::workbook::Workbook;
use xlc_eval::Value;
use xlc_scenario::dist::Dist;
use xlc_scenario::engine::{CellKey, Engine, ScenarioSpec};

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// Deterministic tiny PRNG for model generation (not the engine RNG).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 11
    }
    fn f64(&mut self) -> f64 {
        (self.next() % 10_000) as f64 / 10_000.0
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo)
    }
}

/// Random smooth model: input row A, then chains of +,-,*,/ (denominators
/// offset away from zero) and SUM ranges over prior cells.
fn random_smooth_model(seed: u64) -> (Workbook, Vec<CellKey>, CellKey) {
    let mut rng = Lcg(seed.wrapping_mul(2654435761).wrapping_add(1));
    let n_inputs = rng.range(3, 8) as u32;
    let n_cells = rng.range(30, 120) as u32;
    let mut wb = Workbook::default();
    let id = wb.add_sheet("S");
    let mut inputs = Vec::new();
    for i in 0..n_inputs {
        let v = 1.0 + rng.f64() * 4.0;
        wb.set_value(id, i, 0, Value::Num(v));
        inputs.push((0u32, i, 0u32));
    }
    // Column B: the computation chain.
    for r in 0..n_cells {
        let pick = |rng: &mut Lcg, r: u32| -> String {
            if r == 0 || rng.range(0, 3) == 0 {
                // an input ref
                format!("A{}", rng.range(1, n_inputs as u64 + 1))
            } else {
                format!("B{}", rng.range(1, r as u64 + 1))
            }
        };
        let a = pick(&mut rng, r);
        let b = pick(&mut rng, r);
        let f = match rng.range(0, 5) {
            0 => format!("{a}+{b}"),
            1 => format!("{a}-{b}*0.25"),
            2 => format!("{a}*{b}"),
            // Division with the denominator pushed away from zero.
            3 => format!("{a}/({b}+7.5)"),
            _ if r >= 4 => format!("SUM(B{}:B{})", r.saturating_sub(3).max(1), r),
            _ => format!("{a}+{b}*2"),
        };
        wb.set_formula(id, r, 1, f);
    }
    let out = (0u32, n_cells - 1, 1u32);
    (wb, inputs, out)
}

pub fn phase7_verify_cmd(args: &[String]) -> i32 {
    let out_path = arg_value(args, "--out").unwrap_or("docs/benchmarks/phase7.json");

    // ---- 1. incremental recompute latency ----
    // 40k-formula model (20 cols x 2000 rows of row-independent chains);
    // the changed input's cone is one row's 20-cell chain — §8.10's
    // "re-run only that sub-DAG" made literal. Schedule prebuilt (that is
    // the compile); latency = the re-run at N=1e5.
    let incremental = {
        let mut wb = Workbook::default();
        let id = wb.add_sheet("S");
        let rows = 2000u32;
        let cols = 20u32;
        for r in 0..rows {
            wb.set_value(id, r, 0, Value::Num((r % 97) as f64 + 1.0));
        }
        for c in 1..=cols {
            for r in 0..rows {
                let prev = xlc_parse::ast::col_letters(c - 1);
                wb.set_formula(id, r, c, format!("{prev}{}*1.01+{}", r + 1, c));
            }
        }
        let spec = ScenarioSpec {
            seed: 4242,
            inputs: vec![((0, 1000, 0), Dist::Normal { mean: 50.0, sd: 5.0 })],
        };
        let build0 = Instant::now();
        let engine = Engine::new(&wb, spec);
        let build_ms = build0.elapsed().as_secs_f64() * 1e3;
        let cone = engine.cone_cells();
        let n = 100_000u32;
        // Warm once, then measure the slider stroke.
        let _ = engine.run(n, 8192, &[]);
        let t0 = Instant::now();
        let res = engine.run(n, 8192, &[(0, 1000, 20)]);
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        println!(
            "phase7: incremental {ms:.1} ms | 40k-formula model, cone {cone}, N={n} (build {build_ms:.0} ms, fast {} / scalar {})",
            res.fast_path_cells, res.scalar_path_cells
        );
        serde_json::json!({
            "latency_ms": ms,
            "model_formulas": rows * cols,
            "cone_cells": cone,
            "scenarios": n,
            "schedule_build_ms": build_ms,
            "model": "synthetic 20x2000 row-independent chains, single input changed",
        })
    };

    // ---- 2. AD vs central finite differences on 50 random smooth models ----
    let ad = {
        let mut max_rel = 0.0f64;
        let mut checked = 0usize;
        let mut models = 0usize;
        let mut seed = 1u64;
        while models < 50 {
            seed += 1;
            let (wb, inputs, out) = random_smooth_model(seed);
            let point: Vec<f64> = inputs
                .iter()
                .map(|&(s, r, c)| match wb.sheets[s as usize].values.get(&(r, c)) {
                    Some(Value::Num(x)) => *x,
                    _ => 1.0,
                })
                .collect();
            let spec = ScenarioSpec {
                seed: 7,
                inputs: inputs
                    .iter()
                    .zip(&point)
                    .map(|(&k, &x)| (k, Dist::Point { value: x }))
                    .collect(),
            };
            let engine = Engine::new(&wb, spec);
            let Some(g) = engine.gradient(out, 0) else { continue };
            if !g.structural_cells.is_empty() {
                continue; // only smooth models count toward the oracle
            }
            // Central differences via engine evaluation at shifted points.
            let mut ok_model = true;
            for (i, &x) in point.iter().enumerate() {
                let h = (x.abs() * 1e-6).max(1e-8);
                let eval_at = |xi: f64| -> Option<f64> {
                    let mut pt = point.clone();
                    pt[i] = xi;
                    let spec = ScenarioSpec {
                        seed: 7,
                        inputs: inputs
                            .iter()
                            .zip(&pt)
                            .map(|(&k, &x2)| (k, Dist::Point { value: x2 }))
                            .collect(),
                    };
                    let e = Engine::new(&wb, spec);
                    match e.eval_scenario(0, &[out]).remove(&out) {
                        Some(Value::Num(v)) => Some(v),
                        _ => None,
                    }
                };
                let (Some(fp), Some(fm)) = (eval_at(x + h), eval_at(x - h)) else {
                    ok_model = false;
                    break;
                };
                let fd = (fp - fm) / (2.0 * h);
                let scale = fd.abs().max(g.d_inputs[i].abs()).max(1.0);
                let rel = (fd - g.d_inputs[i]).abs() / scale;
                max_rel = max_rel.max(rel);
                checked += 1;
            }
            if ok_model {
                models += 1;
            }
        }
        println!("phase7: AD {models} models, {checked} gradients, max rel err {max_rel:.2e}");
        serde_json::json!({
            "models": models,
            "gradients_checked": checked,
            "max_rel_err": max_rel,
        })
    };

    // ---- 3. semantic diff with a planted change ----
    let diff = {
        let build = |coef: f64| {
            let mut wb = Workbook::default();
            let id = wb.add_sheet("S");
            for r in 0..40u32 {
                wb.set_value(id, r, 0, Value::Num(r as f64 + 1.0));
                let f = if r == 0 {
                    format!("A1*{coef}")
                } else if r == 25 {
                    // The planted change sits mid-chain.
                    format!("A{}*{coef}+B{}", r + 1, r)
                } else {
                    format!("A{}*1.10+B{}", r + 1, r)
                };
                wb.set_formula(id, r, 1, f);
            }
            wb
        };
        let wb1 = build(1.10);
        let wb2 = build(1.17);
        let spec = ScenarioSpec {
            seed: 314,
            inputs: vec![((0, 25, 0), Dist::Normal { mean: 26.0, sd: 4.0 })],
        };
        let out = (0u32, 39u32, 1u32);
        let rep = xlc_diff::diff_output(&wb1, &wb2, &spec, out, 10_000);
        let w = rep.witness.as_ref();
        println!(
            "phase7: diff {} / {} scenarios diverge; witness {:?}",
            rep.divergent,
            rep.scenarios,
            w.map(|w| w.scenario)
        );
        serde_json::json!({
            "divergence_detected": rep.divergent > 0,
            "divergent_pct": 100.0 * rep.divergent as f64 / rep.scenarios as f64,
            "witness_scenario": w.map(|w| w.scenario),
            "witness_inputs": w.map(|w| w.inputs.iter().map(|(k, v)| serde_json::json!({"cell": format!("{k:?}"), "value": v})).collect::<Vec<_>>()),
            "v1_output": w.map(|w| format!("{:?}", w.v1_output)),
            "v2_output": w.map(|w| format!("{:?}", w.v2_output)),
        })
    };

    let artifact = serde_json::json!({
        "incremental": incremental,
        "ad": ad,
        "diff": diff,
    });
    if let Some(parent) = Path::new(out_path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(out_path, serde_json::to_vec_pretty(&artifact).unwrap()).ok();
    println!("phase7-verify: artifact -> {out_path}");
    0
}
