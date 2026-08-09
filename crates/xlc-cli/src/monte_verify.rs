//! Gate 6 artifact: N=1 oracle, deterministic mean, distribution moments,
//! (seed,k) reproducibility, bytes-moved accounting, and native throughput
//! on a named public workbook.

use std::fs;
use std::path::Path;
use std::time::Instant;

use xlc_eval::Value;
use xlc_scenario::dist::Dist;
use xlc_scenario::engine::{CellKey, Engine, ScenarioSpec};
use xlc_scenario::rng::DrawAddr;

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

pub fn monte_verify_cmd(args: &[String]) -> i32 {
    let out_path = arg_value(args, "--out").unwrap_or("docs/benchmarks/scenario.json");
    let bench_wb = arg_value(args, "--bench-workbook")
        .unwrap_or("corpus/work/fuse-bins/cc-binaries/56cbca12-9c02-49e5-a064-53129669df80");

    // ---- 1. N=1 oracle over fixtures + a deterministic corpus sample ----
    let mut n1_cells = 0usize;
    let mut n1_mism = 0usize;
    let mut sample_paths: Vec<std::path::PathBuf> = fs::read_dir("corpus/subset")
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    sample_paths.sort();
    let sample: Vec<_> = sample_paths.iter().step_by(7).take(40).collect();
    for path in sample {
        let Ok(wb) = xlc_ingest::ingest_path(path) else {
            continue;
        };
        let inputs: Vec<(CellKey, f64)> = xlc_scenario::engine::auto_inputs(&wb, 8).into_iter().map(|(k, x, _)| (k, x)).collect();
        if inputs.is_empty() {
            continue;
        }
        let spec = ScenarioSpec {
            seed: 11,
            inputs: inputs
                .iter()
                .map(|&(k, x)| (k, Dist::Point { value: x }))
                .collect(),
        };
        let engine = Engine::new(&wb, spec);
        let (c, m) = engine.verify_n1();
        n1_cells += c;
        n1_mism += m;
    }
    println!("monte-verify: N=1 oracle {n1_cells} cells, {n1_mism} mismatches");

    // ---- 2. deterministic mean (fixture chain) ----
    let det = {
        let mut wb = xlc_eval::workbook::Workbook::default();
        let id = wb.add_sheet("S");
        for r in 0..10u32 {
            wb.set_value(id, r, 0, Value::Num((r + 1) as f64 * 1.1));
            let f = if r == 0 {
                "A1*2".into()
            } else {
                format!("A{}*2+B{}", r + 1, r)
            };
            wb.set_formula(id, r, 1, f);
        }
        let spec = ScenarioSpec {
            seed: 5,
            inputs: vec![((0, 0, 0), Dist::Point { value: 0.1 })],
        };
        let engine = Engine::new(&wb, spec);
        let res = engine.run(10_000, 1024, &[(0, 9, 1)]);
        let vals = &res.watched[&(0, 9, 1)];
        let first = vals[0].clone();
        vals.iter().all(|v| xlc_ir::bit_equal(v, &first))
    };
    println!("monte-verify: deterministic mean exact: {det}");

    // ---- 3. moments at N=1e6 ----
    let mut moments = serde_json::Map::new();
    for (name, d) in [
        ("normal", Dist::Normal { mean: 3.0, sd: 2.0 }),
        (
            "lognormal",
            Dist::LogNormal {
                mu: 0.2,
                sigma: 0.5,
            },
        ),
        ("uniform", Dist::Uniform { a: -1.0, b: 5.0 }),
        (
            "triangular",
            Dist::Triangular {
                a: 1.0,
                m: 4.0,
                b: 11.0,
            },
        ),
        (
            "pert",
            Dist::Pert {
                a: 1.0,
                m: 4.0,
                b: 11.0,
            },
        ),
    ] {
        let n = 1_000_000u32;
        let mut sum = 0.0;
        let mut sumsq = 0.0;
        for k2 in 0..n {
            let x = d.sample(DrawAddr {
                seed: 21,
                cell: 2,
                scenario: k2,
                draw: 0,
                attempt: 0,
            });
            sum += x;
            sumsq += x * x;
        }
        let mean = sum / n as f64;
        let var = sumsq / n as f64 - mean * mean;
        let mean_se = (d.variance() / n as f64).sqrt();
        // Var of sample variance ~ 2 sigma^4 / n for near-normal; use a
        // conservative kurtosis-free bound of 3 sigma^4.
        let var_se = (3.0 * d.variance() * d.variance() / n as f64).sqrt();
        moments.insert(
            name.into(),
            serde_json::json!({
                "mean": mean, "mean_expected": d.mean(),
                "mean_sigmas": (mean - d.mean()).abs() / mean_se,
                "var": var, "var_expected": d.variance(),
                "var_sigmas": (var - d.variance()).abs() / var_se,
            }),
        );
    }
    println!("monte-verify: moments recorded for 5 distributions");

    // ---- 4 + 5 + 6. reproducibility, bytes, throughput on the named book ----
    let bench = (|| -> Option<serde_json::Value> {
        let wb = xlc_ingest::ingest_path(Path::new(bench_wb)).ok()?;
        let inputs: Vec<(CellKey, f64)> = xlc_scenario::engine::auto_inputs(&wb, 32).into_iter().map(|(k, x, _)| (k, x)).collect();
        let spec = ScenarioSpec {
            seed: 42,
            inputs: inputs
                .iter()
                .map(|&(k, x)| {
                    (
                        k,
                        Dist::Normal {
                            mean: x,
                            sd: x.abs() * 0.1 + 1.0,
                        },
                    )
                })
                .collect(),
        };
        let engine = Engine::new(&wb, spec);
        let cone = engine.cone_cells();
        let keys = engine.cone_keys();
        let watch = [*keys.last()?];
        let n = 10_000u32;
        let t0 = Instant::now();
        let res = engine.run(n, 1024, &watch);
        let secs = t0.elapsed().as_secs_f64();

        // Reproducibility: five scenarios re-derived solo.
        let mut repro = true;
        for k2 in [0u32, 1023, 1024, 5000, 9999] {
            let solo = engine.eval_scenario(k2, &watch);
            if !xlc_ir::bit_equal(&solo[&watch[0]], &res.watched[&watch[0]][k2 as usize]) {
                repro = false;
            }
        }

        let inputs_n = engine_inputs_len(&engine);
        let tile = 1024u64;
        let budget: u64 = 8 * 1024 * 1024;
        let peak_live_bytes = res.peak_live_buffers as u64 * tile * 8;
        let resident = peak_live_bytes <= budget;
        // DRAM model (docs/decisions.md): writes + input reads when the
        // live set is provably cache-resident; otherwise count streams.
        let measured = if resident {
            (res.bytes_written + 8 * n as u64 * inputs_n as u64) as f64 / n as f64
        } else {
            (res.bytes_written + res.bytes_read_streams) as f64 / n as f64
        };
        let theo = 8.0 * (cone + inputs_n) as f64;
        let machine = fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("model name"))
                    .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
            })
            .unwrap_or_default();
        println!(
            "monte-verify: bench cone {cone} cells x {n} scenarios in {secs:.2}s = {:.2e} cell-scenarios/s | fast {} scalar {} | bytes/scen {measured:.0} vs min {theo:.0}",
            cone as f64 * n as f64 / secs,
            res.fast_path_cells,
            res.scalar_path_cells
        );
        Some(serde_json::json!({
            "reproducible": repro,
            "bytes": {
                "measured_per_scenario": measured,
                "theoretical_min_per_scenario": theo,
                "peak_live_bytes": peak_live_bytes,
                "residency_budget_bytes": budget,
                "stream_reads_counted": !resident,
                "stream_reads_per_scenario_8B_units": res.bytes_read_streams as f64 / n as f64 / 8.0,
            },
            "throughput": {
                "workbook": bench_wb,
                "cone_cells": cone,
                "scenarios": n,
                "elapsed_s": secs,
                "native_cells_x_scenarios_per_s": cone as f64 * n as f64 / secs,
                "fast_path_cells": res.fast_path_cells,
                "scalar_path_cells": res.scalar_path_cells,
                "machine": machine,
                "tile": 1024,
            },
        }))
    })();

    let Some(bench) = bench else {
        eprintln!("monte-verify: bench workbook failed to load");
        return 1;
    };

    // Supplementary SYNTHETIC benchmark (labeled as such): a deep pure-
    // arithmetic chain that exercises the vectorized fast path at scale.
    let synthetic = {
        let mut wb = xlc_eval::workbook::Workbook::default();
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
            seed: 42,
            inputs: (0..rows)
                .step_by(40)
                .map(|r| {
                    (
                        (0u32, r, 0u32),
                        Dist::Normal {
                            mean: 50.0,
                            sd: 5.0,
                        },
                    )
                })
                .collect(),
        };
        let engine = Engine::new(&wb, spec);
        let cone = engine.cone_cells();
        let n = 10_000u32;
        let t0 = Instant::now();
        let res = engine.run(n, 1024, &[]);
        let secs = t0.elapsed().as_secs_f64();
        println!(
            "monte-verify: SYNTHETIC cone {cone} x {n} in {secs:.2}s = {:.2e} cell-scenarios/s (fast {} / scalar {})",
            cone as f64 * n as f64 / secs,
            res.fast_path_cells,
            res.scalar_path_cells
        );
        serde_json::json!({
            "label": "synthetic 20-column x 2000-row arithmetic chain (fast-path showcase, NOT the public-workbook claim)",
            "cone_cells": cone,
            "scenarios": n,
            "elapsed_s": secs,
            "cells_x_scenarios_per_s": cone as f64 * n as f64 / secs,
            "fast_path_cells": res.fast_path_cells,
            "scalar_path_cells": res.scalar_path_cells,
        })
    };

    let artifact = serde_json::json!({
        "n1_oracle": { "cells": n1_cells, "mismatches": n1_mism },
        "deterministic_mean": { "exact": det, "scenarios": 10_000 },
        "moments": serde_json::Value::Object(moments),
        "reproducibility": { "exact": bench["reproducible"] },
        "bytes_moved": bench["bytes"],
        "throughput": bench["throughput"],
        "synthetic_supplementary": synthetic,
        "accounting": "DRAM model: measured = 8B x (cone writes + input reads) per scenario when the peak live buffer set provably fits the 8MB residency budget (witness recorded); otherwise full stream reads are counted. Stream-read totals recorded either way.",
    });
    if let Some(parent) = Path::new(out_path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(out_path, serde_json::to_vec_pretty(&artifact).unwrap()).ok();
    println!("monte-verify: artifact -> {out_path}");
    0
}

fn engine_inputs_len(engine: &Engine) -> usize {
    engine.inputs_len()
}
