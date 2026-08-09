//! Product verbs (Phase 8): `xlc monte` and `xlc diff`.

use std::path::Path;

use xlc_eval::workbook::Workbook;
use xlc_eval::Value;
use xlc_scenario::dist::Dist;
use xlc_scenario::engine::{CellKey, Engine, ScenarioSpec};

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn arg_values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    args.iter()
        .enumerate()
        .filter(|(_, a)| *a == flag)
        .filter_map(|(i, _)| args.get(i + 1))
        .map(String::as_str)
        .collect()
}

/// "Sheet!A1" -> CellKey.
fn parse_cell(wb: &Workbook, s: &str) -> Option<CellKey> {
    let (sheet_name, cell) = s.rsplit_once('!')?;
    let sheet = wb.resolve_sheet_pub(sheet_name.trim_matches('\''))?;
    let letters_end = cell.bytes().take_while(|b| b.is_ascii_alphabetic()).count();
    let col = xlc_parse::ast::letters_col(&cell[..letters_end].to_uppercase())?;
    let row: u32 = cell[letters_end..].parse().ok()?;
    Some((sheet, row.checked_sub(1)?, col))
}

/// "normal(50,5)" | "lognormal(0.2,0.5)" | "uniform(1,9)" |
/// "triangular(1,4,9)" | "pert(1,4,9)" | "point(3.2)".
fn parse_dist(s: &str) -> Option<Dist> {
    let (name, rest) = s.split_once('(')?;
    let nums: Vec<f64> = rest
        .trim_end_matches(')')
        .split(',')
        .map(|t| t.trim().parse().ok())
        .collect::<Option<_>>()?;
    Some(
        match (name.trim().to_ascii_lowercase().as_str(), nums.as_slice()) {
            ("normal", [m, s]) => Dist::Normal { mean: *m, sd: *s },
            ("lognormal", [mu, sg]) => Dist::LogNormal {
                mu: *mu,
                sigma: *sg,
            },
            ("uniform", [a, b]) => Dist::Uniform { a: *a, b: *b },
            ("triangular", [a, m, b]) => Dist::Triangular {
                a: *a,
                m: *m,
                b: *b,
            },
            ("pert", [a, m, b]) => Dist::Pert {
                a: *a,
                m: *m,
                b: *b,
            },
            ("point", [v]) => Dist::Point { value: *v },
            _ => return None,
        },
    )
}

fn cell_name(wb: &Workbook, k: CellKey) -> String {
    format!(
        "{}!{}{}",
        wb.sheets[k.0 as usize].name,
        xlc_parse::ast::col_letters(k.2),
        k.1 + 1
    )
}

pub fn monte_cmd(args: &[String]) -> i32 {
    let Some(target) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: xlc monte <file> --scenarios N [--seed S] --input \"Sheet!A1=normal(50,5)\" [--watch \"Sheet!B10\"]");
        return 2;
    };
    let n: u32 = arg_value(args, "--scenarios")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let seed: u64 = arg_value(args, "--seed")
        .and_then(|v| v.parse().ok())
        .unwrap_or(42);

    let wb = match xlc_ingest::ingest_path(Path::new(target.as_str())) {
        Ok(wb) => wb,
        Err(e) => {
            eprintln!("xlc monte: {e}");
            return 1;
        }
    };
    let mut inputs = Vec::new();
    for spec in arg_values(args, "--input") {
        let Some((cell, dist)) = spec.split_once('=') else {
            eprintln!("xlc monte: bad --input {spec}");
            return 2;
        };
        let (Some(key), Some(d)) = (parse_cell(&wb, cell), parse_dist(dist)) else {
            eprintln!("xlc monte: cannot parse --input {spec}");
            return 2;
        };
        inputs.push((key, d));
    }
    if inputs.is_empty() {
        eprintln!("xlc monte: at least one --input is required");
        return 2;
    }
    let engine = Engine::new(&wb, ScenarioSpec { seed, inputs });
    if engine.cone_cells() == 0 {
        eprintln!("xlc monte: no formula cell depends on the given inputs");
        return 1;
    }
    let watch: Vec<CellKey> = {
        let named: Vec<CellKey> = arg_values(args, "--watch")
            .iter()
            .filter_map(|w| parse_cell(&wb, w))
            .collect();
        if named.is_empty() {
            vec![*engine.cone_keys().last().unwrap()]
        } else {
            named
        }
    };
    let t0 = std::time::Instant::now();
    let res = engine.run(n, 1024, &watch);
    let secs = t0.elapsed().as_secs_f64();

    println!(
        "monte: {} scenarios over a {}-cell cone in {:.2}s (seed {seed})",
        n,
        engine.cone_cells(),
        secs
    );
    for key in &watch {
        let vals = &res.watched[key];
        let mut xs: Vec<f64> = vals
            .iter()
            .filter_map(|v| match v {
                Value::Num(x) => Some(*x),
                _ => None,
            })
            .collect();
        if xs.is_empty() {
            println!("  {}: non-numeric across scenarios", cell_name(&wb, *key));
            continue;
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        let var = xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / xs.len() as f64;
        let q = |p: f64| xs[((xs.len() - 1) as f64 * p) as usize];
        println!(
            "  {}: mean {mean:.6} sd {:.6} | p5 {:.6} p50 {:.6} p95 {:.6}",
            cell_name(&wb, *key),
            var.sqrt(),
            q(0.05),
            q(0.50),
            q(0.95)
        );
    }
    0
}

pub fn diff_cmd(args: &[String]) -> i32 {
    let files: Vec<&String> = args.iter().take_while(|a| !a.starts_with("--")).collect();
    if files.len() != 2 {
        eprintln!("usage: xlc diff <a.xlsx> <b.xlsx> --output \"Sheet!Cell\" [--input \"Sheet!A1=normal(..)\"] [--scenarios N]");
        return 2;
    }
    let n: u32 = arg_value(args, "--scenarios")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let (wa, wbk) = match (
        xlc_ingest::ingest_path(Path::new(files[0].as_str())),
        xlc_ingest::ingest_path(Path::new(files[1].as_str())),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("xlc diff: {e}");
            return 1;
        }
    };
    let Some(output) = arg_value(args, "--output").and_then(|o| parse_cell(&wa, o)) else {
        eprintln!("xlc diff: --output Sheet!Cell is required");
        return 2;
    };
    let mut inputs = Vec::new();
    for spec in arg_values(args, "--input") {
        let Some((cell, dist)) = spec.split_once('=') else {
            continue;
        };
        if let (Some(key), Some(d)) = (parse_cell(&wa, cell), parse_dist(dist)) {
            inputs.push((key, d));
        }
    }
    if inputs.is_empty() {
        // Default: perturb every common static numeric cell that feeds
        // the output's cone in version A, ±10% normal.
        for (sid, sheet) in wa.sheets.iter().enumerate() {
            for (&(r, c), v) in &sheet.values {
                if sheet.formulas.contains_key(&(r, c)) {
                    continue;
                }
                if let Value::Num(x) = v {
                    let same_in_b = wbk
                        .sheets
                        .get(sid)
                        .and_then(|s2| s2.values.get(&(r, c)))
                        .is_some_and(|v2| matches!(v2, Value::Num(y) if y == x));
                    if same_in_b {
                        inputs.push((
                            (sid as u32, r, c),
                            Dist::Normal {
                                mean: *x,
                                sd: x.abs() * 0.1 + 1e-9,
                            },
                        ));
                    }
                }
            }
        }
        inputs.sort_by_key(|(k, _)| *k);
        inputs.truncate(64);
    }
    let spec = ScenarioSpec {
        seed: 20_26,
        inputs,
    };
    let rep = xlc_diff::diff_output(&wa, &wbk, &spec, output, n);
    println!(
        "diff: {}/{} sampled scenarios diverge on {}",
        rep.divergent,
        rep.scenarios,
        cell_name(&wa, output)
    );
    match rep.witness {
        Some(w) => {
            println!("  witness scenario {}:", w.scenario);
            for (k, v) in &w.inputs {
                println!("    {} = {v:.6}", cell_name(&wa, *k));
            }
            println!(
                "    {}: v1 = {:?}   v2 = {:?}",
                cell_name(&wa, output),
                w.v1_output,
                w.v2_output
            );
            1 // divergence found: non-zero for CI use
        }
        None => {
            println!("  no divergence on the sampled input space");
            0
        }
    }
}
