//! THE RECEIPT (§8.4): CLI driver over the shared per-cell receipt in
//! xlc-ingest. Adds corpus parallelism, per-function attribution, failure
//! logging, and the committed artifact gate3 asserts against.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rayon::prelude::*;
use serde::Serialize;
use xlc_ingest::{run_receipt, CellReport, Outcome, ReceiptSummary};

#[derive(Serialize)]
struct FailLine {
    file: String,
    sheet: String,
    cell: String,
    formula: String,
    expected: String,
    got: String,
    class: String,
}

fn fmt_value(v: &xlc_eval::Value) -> String {
    use xlc_eval::Value;
    match v {
        Value::Num(x) => format!("{x:?}"),
        Value::Text(s) => format!("{s:?}"),
        Value::Bool(b) => format!("{b}"),
        Value::Err(e) => e.as_str().into(),
        Value::Blank => "<blank>".into(),
    }
}

fn cell_a1(row: u32, col: u32) -> String {
    format!("{}{}", xlc_parse::ast::col_letters(col), row + 1)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

pub fn receipt_cmd(args: &[String]) -> i32 {
    let targets: Vec<&String> = args.iter().take_while(|a| !a.starts_with("--")).collect();
    if targets.is_empty() {
        eprintln!("usage: xlc receipt <dir-or-file>... [--out artifact.json] [--failures failures.jsonl] [--limit N]");
        return 2;
    }
    let out_path = arg_value(args, "--out");
    let failures_path = arg_value(args, "--failures");
    let limit: usize = arg_value(args, "--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);

    let mut files = Vec::new();
    for target in &targets {
        let target = PathBuf::from(target.as_str());
        if target.is_dir() {
            walk(&target, &mut files);
        } else {
            files.push(target);
        }
    }
    files.sort();
    files.retain(|p| {
        let mut magic = [0u8; 2];
        matches!(
            fs::File::open(p).and_then(|mut f| f.read_exact(&mut magic)),
            Ok(())
        ) && &magic == b"PK"
    });
    files.truncate(limit);
    eprintln!("receipt: {} workbooks", files.len());

    let per_function: Mutex<BTreeMap<String, (usize, usize)>> = Mutex::new(BTreeMap::new());
    let fail_lines: Mutex<Vec<FailLine>> = Mutex::new(Vec::new());
    let panics = AtomicUsize::new(0);
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let done = AtomicUsize::new(0);

    let totals = files
        .par_iter()
        .map(|path| {
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let wb = xlc_ingest::ingest_path(path).ok()?;
                let mut fns = std::collections::BTreeSet::new();
                let sum = run_receipt(&wb, |cell: &CellReport| {
                    let verdict = match cell.outcome {
                        Outcome::Pass(_) => Some(true),
                        Outcome::Mismatch(_) => Some(false),
                        _ => None,
                    };
                    if let Some(ok) = verdict {
                        fns.clear();
                        xlc_parse::scan::extract_functions(cell.formula, &mut fns);
                        let mut pf = per_function.lock().unwrap();
                        for f in fns.iter() {
                            let e = pf.entry(f.clone()).or_insert((0, 0));
                            e.1 += 1;
                            if ok {
                                e.0 += 1;
                            }
                        }
                        drop(pf);
                        if !ok {
                            let mut fl = fail_lines.lock().unwrap();
                            if fl.len() < 50_000 {
                                let class = match cell.outcome {
                                    Outcome::Mismatch(c) => c,
                                    _ => unreachable!(),
                                };
                                fl.push(FailLine {
                                    file: path.display().to_string(),
                                    sheet: wb.sheets[cell.sheet_idx].name.clone(),
                                    cell: cell_a1(cell.row, cell.col),
                                    formula: cell.formula.to_string(),
                                    expected: fmt_value(&cell.expected),
                                    got: cell.got.as_ref().map(fmt_value).unwrap_or_default(),
                                    class: format!("mismatch_{class}"),
                                });
                            }
                        }
                    }
                });
                Some(sum)
            }));
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 500 == 0 {
                eprintln!("  …{n} workbooks");
            }
            match result {
                Ok(Some(sum)) => sum,
                Ok(None) => ReceiptSummary::default(),
                Err(_) => {
                    panics.fetch_add(1, Ordering::Relaxed);
                    ReceiptSummary::default()
                }
            }
        })
        .reduce(ReceiptSummary::default, |mut a, b| {
            a.cells += b.cells;
            a.pass += b.pass;
            a.ulp1 += b.ulp1;
            a.sig15 += b.sig15;
            a.no_cached += b.no_cached;
            for (k, v) in b.excluded {
                *a.excluded.entry(k).or_insert(0) += v;
            }
            for (k, v) in b.mismatches {
                *a.mismatches.entry(k).or_insert(0) += v;
            }
            a
        });
    panic::set_hook(prev_hook);
    let n_panics = panics.load(Ordering::Relaxed);

    let pf = per_function.into_inner().unwrap();
    let mut per_function_json = serde_json::Map::new();
    let mut ranked: Vec<(&String, &(usize, usize))> = pf.iter().collect();
    ranked.sort_by_key(|&(_, &(_, total))| std::cmp::Reverse(total));
    for (name, (pass, total)) in ranked {
        per_function_json.insert(
            name.clone(),
            serde_json::json!({"pass": pass, "total": total,
                "rate": if *total > 0 { *pass as f64 / *total as f64 } else { 0.0 }}),
        );
    }

    let ex = |k: &str| totals.excluded.get(k).copied().unwrap_or(0);
    let mm = |k: &str| totals.mismatches.get(k).copied().unwrap_or(0);
    // "text" folds into "type" for artifact continuity with gate(3).
    let artifact = serde_json::json!({
        "mode": "per-cell (cached-neighbor context)",
        "ulp_policy": "pass iff bit-identical, within 1 ULP (same sign), or equal at 15 significant decimal digits (Excel's stored-literal precision); ulp1/sig15 tracked separately",
        "cells_total": totals.cells - totals.no_cached,
        "cells_pass": totals.pass,
        "no_cached_value": totals.no_cached,
        "pass_ulp1": totals.ulp1,
        "pass_sig15": totals.sig15,
        "excluded": {
            "unimplemented_function": ex("unimplemented"),
            "parse": ex("parse"),
            "external_ref": ex("external_ref"),
            "volatile_nondeterministic": ex("volatile"),
            "array_formula": ex("array_formula"),
        },
        "mismatch_classes": {
            "numeric": mm("numeric"),
            "type": mm("type") + mm("text"),
            "error": mm("error"),
        },
        "panics": n_panics,
        "per_function": serde_json::Value::Object(per_function_json),
    });
    if let Some(p) = out_path {
        if let Some(parent) = Path::new(p).parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(p, serde_json::to_vec_pretty(&artifact).unwrap()).ok();
    }
    if let Some(p) = failures_path {
        let fl = fail_lines.into_inner().unwrap();
        let mut out = String::new();
        for l in &fl {
            out.push_str(&serde_json::to_string(l).unwrap());
            out.push('\n');
        }
        if let Some(parent) = Path::new(p).parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(p, out).ok();
    }

    let denom = totals.cells - totals.no_cached;
    let rate = if denom > 0 {
        totals.pass as f64 / denom as f64
    } else {
        0.0
    };
    println!(
        "receipt: {}/{} pass ({:.2}%) | excluded: {} unimpl, {} parse, {} extref, {} volatile, {} array | {} no-cached | mismatches: {} num, {} type, {} err | {} panics",
        totals.pass,
        denom,
        rate * 100.0,
        ex("unimplemented"),
        ex("parse"),
        ex("external_ref"),
        ex("volatile"),
        ex("array_formula"),
        totals.no_cached,
        mm("numeric"),
        mm("type") + mm("text"),
        mm("error"),
        n_panics
    );
    0
}
