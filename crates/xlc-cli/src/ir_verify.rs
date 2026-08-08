//! Gate 5 oracle: lower every subset workbook to the IR and compare every
//! lane's IR evaluation against the scalar interpreter, bit-for-bit.

use std::fs;
use std::io::Read;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rayon::prelude::*;
use xlc_eval::interp::{Interp, Origin};

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

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
}

#[derive(Default)]
struct Tally {
    cells: usize,
    mismatches: usize,
    parse_skipped: usize,
    families: usize,
    family_cells: usize,
    insts_before: usize,
    insts_after: usize,
}

pub fn ir_verify_cmd(args: &[String]) -> i32 {
    let Some(target) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: xlc ir-verify <dir> [--out docs/benchmarks/ir.json]");
        return 2;
    };
    let out_path = arg_value(args, "--out").unwrap_or("docs/benchmarks/ir.json");

    let mut files = Vec::new();
    walk(Path::new(target.as_str()), &mut files);
    files.sort();
    files.retain(|p| {
        let mut magic = [0u8; 2];
        matches!(fs::File::open(p).and_then(|mut f| f.read_exact(&mut magic)), Ok(()))
            && &magic == b"PK"
    });
    eprintln!("ir-verify: {} workbooks", files.len());

    let ratios: Mutex<Vec<f64>> = Mutex::new(Vec::new());
    let drift_samples: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let panics = AtomicUsize::new(0);

    let t = files
        .par_iter()
        .map(|path| {
            let mut t = Tally::default();
            let r = panic::catch_unwind(AssertUnwindSafe(|| {
                let wb = xlc_ingest::ingest_path(path).ok()?;
                let m = xlc_ir::lower(&wb);
                t.families = m.families.len();
                t.insts_before = m.insts_before();
                t.insts_after = m.insts_after();
                for fam in &m.families {
                    t.family_cells += fam.lanes.len();
                    for (lane, &(row, col)) in fam.lanes.iter().enumerate() {
                        let src = &wb.sheets[fam.sheet as usize].formulas[&(row, col)];
                        let Ok(parsed) = xlc_parse::parse_formula(src) else {
                            t.parse_skipped += 1;
                            continue;
                        };
                        let scalar = Interp::new(&wb, Origin { sheet: fam.sheet, row, col })
                            .eval_formula(&parsed.expr);
                        let ir = fam.eval_lane(&wb, lane);
                        t.cells += 1;
                        if !xlc_ir::bit_equal(&scalar, &ir) {
                            t.mismatches += 1;
                            let mut ds = drift_samples.lock().unwrap();
                            if ds.len() < 50 {
                                ds.push(format!(
                                    "{}: {}!{}{} = {} | scalar {:?} ir {:?}",
                                    path.display(),
                                    wb.sheets[fam.sheet as usize].name,
                                    xlc_parse::ast::col_letters(col),
                                    row + 1,
                                    src,
                                    scalar,
                                    ir
                                ));
                            }
                        }
                    }
                }
                if t.families > 0 {
                    ratios
                        .lock()
                        .unwrap()
                        .push(t.family_cells as f64 / t.families as f64);
                }
                Some(t)
            }));
            match r {
                Ok(Some(t)) => t,
                Ok(None) => Tally::default(),
                Err(_) => {
                    panics.fetch_add(1, Ordering::Relaxed);
                    Tally::default()
                }
            }
        })
        .reduce(Tally::default, |mut a, b| {
            a.cells += b.cells;
            a.mismatches += b.mismatches;
            a.parse_skipped += b.parse_skipped;
            a.families += b.families;
            a.family_cells += b.family_cells;
            a.insts_before += b.insts_before;
            a.insts_after += b.insts_after;
            a
        });
    panic::set_hook(prev_hook);

    let ratios = ratios.into_inner().unwrap();
    let artifact = serde_json::json!({
        "cells_compared": t.cells,
        "mismatches": t.mismatches,
        "parse_skipped": t.parse_skipped,
        "panics": panics.load(Ordering::Relaxed),
        "coarsening": { "cells": t.family_cells, "nodes": t.families },
        "cse": { "insts_before": t.insts_before, "insts_after": t.insts_after },
        "per_workbook_ratios_recorded": !ratios.is_empty(),
        "per_workbook_ratio_mean": ratios.iter().sum::<f64>() / ratios.len().max(1) as f64,
    });
    if let Some(parent) = Path::new(out_path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(out_path, serde_json::to_vec_pretty(&artifact).unwrap()).ok();
    for d in drift_samples.into_inner().unwrap().iter().take(10) {
        eprintln!("drift: {d}");
    }
    println!(
        "ir-verify: {}/{} bit-identical | coarsening {} cells -> {} nodes ({:.2}x) | CSE {} -> {} insts | {} panics",
        t.cells - t.mismatches,
        t.cells,
        t.family_cells,
        t.families,
        t.family_cells as f64 / t.families.max(1) as f64,
        t.insts_before,
        t.insts_after,
        panics.load(Ordering::Relaxed)
    );
    if t.mismatches == 0 {
        0
    } else {
        1
    }
}
