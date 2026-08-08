//! `xlc check` — the analyzer surface (Phase 4), plus the corpus-wide
//! precision-sampling harness that feeds docs/precision/.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;
use serde::Serialize;
use xlc_lint::Finding;

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// Per-feature exclusion counts for the capability report (Law 9).
fn capability_report(wb: &xlc_eval::workbook::Workbook) -> BTreeMap<String, usize> {
    let mut cr: BTreeMap<String, usize> = BTreeMap::new();
    let mut fns = std::collections::BTreeSet::new();
    for sheet in &wb.sheets {
        for (&(_r, _c), src) in &sheet.formulas {
            let Ok(parsed) = xlc_parse::parse_formula(src) else {
                *cr.entry("unparseable".into()).or_insert(0) += 1;
                continue;
            };
            if parsed.expr.has_external_ref() {
                *cr.entry("external_ref".into()).or_insert(0) += 1;
            }
            fns.clear();
            crate::census::extract_functions(src, &mut fns);
            if ["NOW", "TODAY", "RAND", "RANDBETWEEN", "RANDARRAY"]
                .iter()
                .any(|f| fns.contains(*f))
            {
                *cr.entry("volatile_nondeterministic".into()).or_insert(0) += 1;
            }
        }
        if !sheet.array_cells.is_empty() {
            *cr.entry("array_formula".into()).or_insert(0) += sheet.array_cells.len();
        }
    }
    cr
}

pub fn check_cmd(args: &[String]) -> i32 {
    let Some(target) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: xlc check <workbook.xlsx> [--timing-out artifact.json]");
        return 2;
    };
    let timing_out = arg_value(args, "--timing-out");
    let path = PathBuf::from(target);

    let t0 = Instant::now();
    let wb = match crate::receipt::ingest(&path) {
        Ok(wb) => wb,
        Err(e) => {
            eprintln!("xlc check: cannot read {target}: {e}");
            return 1;
        }
    };
    let formula_cells: usize = wb.sheets.iter().map(|s| s.formulas.len()).sum();
    let findings = xlc_lint::analyze(&wb);
    let elapsed = t0.elapsed().as_secs_f64();
    let mut cr = capability_report(&wb);
    let excluded_total: usize = cr.values().sum();
    cr.insert(
        "compilable_cells".into(),
        formula_cells.saturating_sub(excluded_total),
    );

    // Compiler-diagnostic output (§9): file, cell, rule, evidence.
    println!("checking {target}: {formula_cells} formulas across {} sheets", wb.sheets.len());
    for f in &findings {
        println!("warning[{}]: {}!{}", f.detector, f.sheet, f.cell);
        println!("  --> {}", f.formula);
        println!("  = proof: {}", f.proof);
    }
    if excluded_total > 0 {
        println!("note: partial compilation — excluded cells by feature:");
        for (k, v) in &cr {
            println!("  {k}: {v}");
        }
    }
    println!(
        "{} findings | {:.3}s | {} formulas/s",
        findings.len(),
        elapsed,
        (formula_cells as f64 / elapsed) as u64
    );

    if let Some(out) = timing_out {
        let machine = fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("model name"))
                    .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
            })
            .unwrap_or_else(|| "unknown".into());
        let artifact = serde_json::json!({
            "workbook": target,
            "formula_cells": formula_cells,
            "elapsed_s": elapsed,
            "findings": findings.len(),
            "machine": machine,
            "threads": rayon::current_num_threads(),
            "capability_report": cr,
        });
        if let Some(parent) = Path::new(out).parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(out, serde_json::to_vec_pretty(&artifact).unwrap()).ok();
    }
    0
}

#[derive(Serialize)]
struct Sample {
    file: String,
    sheet: String,
    cell: String,
    formula: String,
    proof: String,
    /// Audit verdict: "tp" | "fp" | null (unaudited).
    verdict: Option<String>,
    /// Auditor's note, filled during the audit.
    note: Option<String>,
}

/// Run the detectors over corpus dirs, then emit per-detector sample files
/// (deterministic selection: findings sorted by sha-ish key, first N) for
/// hand auditing into docs/precision/.
pub fn lint_corpus_cmd(args: &[String]) -> i32 {
    let dirs: Vec<&String> = args.iter().take_while(|a| !a.starts_with("--")).collect();
    if dirs.is_empty() {
        eprintln!("usage: xlc lint-corpus <dir>... [--samples-dir docs/precision] [--n 200] [--stats out.json]");
        return 2;
    }
    let samples_dir = arg_value(args, "--samples-dir").unwrap_or("docs/precision");
    let n: usize = arg_value(args, "--n").and_then(|v| v.parse().ok()).unwrap_or(200);
    let stats_out = arg_value(args, "--stats");

    let mut files = Vec::new();
    for d in &dirs {
        walk(Path::new(d.as_str()), &mut files);
    }
    files.sort();
    files.retain(|p| {
        let mut magic = [0u8; 2];
        matches!(fs::File::open(p).and_then(|mut f| f.read_exact(&mut magic)), Ok(()))
            && &magic == b"PK"
    });
    eprintln!("lint-corpus: {} candidate workbooks", files.len());

    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let all: Vec<(String, Finding)> = files
        .par_iter()
        .flat_map_iter(|path| {
            let out: Vec<(String, Finding)> =
                panic::catch_unwind(AssertUnwindSafe(|| match crate::receipt::ingest(path) {
                    Ok(wb) => xlc_lint::analyze(&wb)
                        .into_iter()
                        .map(|f| (path.display().to_string(), f))
                        .collect(),
                    Err(_) => Vec::new(),
                }))
                .unwrap_or_default();
            out
        })
        .collect();
    panic::set_hook(prev_hook);

    let mut per: BTreeMap<String, Vec<&(String, Finding)>> = BTreeMap::new();
    for pair in &all {
        per.entry(pair.1.detector.clone()).or_default().push(pair);
    }
    let _ = fs::create_dir_all(samples_dir);
    let mut stats = serde_json::Map::new();
    for (det, mut findings) in per {
        // Deterministic pseudo-random order: sort by fnv of (file,cell).
        findings.sort_by_key(|(file, f)| fnv(&format!("{file}|{}|{}", f.sheet, f.cell)));
        let total = findings.len();
        let sampled: Vec<Sample> = findings
            .iter()
            .take(n)
            .map(|(file, f)| Sample {
                file: file.clone(),
                sheet: f.sheet.clone(),
                cell: f.cell.clone(),
                formula: f.formula.clone(),
                proof: f.proof.clone(),
                verdict: None,
                note: None,
            })
            .collect();
        let doc = serde_json::json!({
            "detector": det,
            "findings_total": total,
            "sampled": sampled.len(),
            "samples": sampled,
        });
        let path = format!("{samples_dir}/{det}.json");
        fs::write(&path, serde_json::to_vec_pretty(&doc).unwrap()).ok();
        println!("lint-corpus: {det}: {total} findings, {} sampled -> {path}", n.min(total));
        stats.insert(det, serde_json::json!(total));
    }
    if let Some(out) = stats_out {
        fs::write(out, serde_json::to_vec_pretty(&serde_json::Value::Object(stats)).unwrap()).ok();
    }
    0
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}
