//! THE RECEIPT (§8.4): recompute every formula cell and bit-diff against
//! Excel's cached value.
//!
//! This first receipt is the *per-cell semantic check*: each formula is
//! evaluated against its neighbors' CACHED values, so every cell is an
//! independent test of function semantics. The full-recompute receipt
//! (schedule-driven, derived values only) layers on top once ingest and
//! scheduling are wired together — same comparison machinery.
//!
//! ULP policy (documented in the artifact): a numeric result passes if it
//! is bit-identical OR within 1 ULP of the cached value. Everything else
//! is classified, counted, and logged.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use calamine::{open_workbook, Data, Reader, Xlsx};
use rayon::prelude::*;
use serde::Serialize;
use xlc_eval::interp::{Interp, Origin};
use xlc_eval::workbook::Workbook;
use xlc_eval::{ExcelError, Value};

fn lower_cached(d: &Data) -> Value {
    match d {
        Data::Int(i) => Value::Num(*i as f64),
        Data::Float(x) => Value::Num(*x),
        Data::String(s) => Value::Text(s.clone()),
        Data::Bool(b) => Value::Bool(*b),
        Data::DateTime(dt) => Value::Num(dt.as_f64()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => Value::Text(s.clone()),
        Data::Error(e) => Value::Err(lower_cell_error(e)),
        Data::Empty => Value::Blank,
    }
}

fn lower_cell_error(e: &calamine::CellErrorType) -> ExcelError {
    use calamine::CellErrorType as C;
    match e {
        C::Div0 => ExcelError::Div0,
        C::NA => ExcelError::NA,
        C::Value => ExcelError::Value,
        C::Ref => ExcelError::Ref,
        C::Name => ExcelError::Name,
        C::Num => ExcelError::Num,
        C::Null => ExcelError::Null,
        C::GettingData => ExcelError::GettingData,
    }
}

/// Load one workbook into the model via the STREAMING cells reader.
///
/// Never use `worksheet_range` here: it materializes a dense
/// width x height grid from the sheet's declared dimension, and real
/// corpus files lie about their dimension — one subset workbook declared
/// A1:XFD1048576 and the dense path attempted a 512 GiB allocation.
/// The cell reader streams each present cell with absolute coordinates
/// and hands us cached value + expanded formula in one pass.
/// (Shared-formula cells whose anchor was not yet seen in stream order
/// come back with formula=None; counted as value-only cells.)
pub fn ingest(path: &Path) -> Result<Workbook, String> {
    let mut xl = open_workbook::<Xlsx<_>, _>(path).map_err(|e| e.to_string())?;
    let mut wb = Workbook::default();
    let sheet_names = xl.sheet_names();
    for name in &sheet_names {
        let id = wb.add_sheet(name);
        let mut reader = match xl.worksheet_cells_reader(name) {
            Ok(r) => r,
            Err(_) => continue,
        };
        while let Some(cell) = reader.next_cell_with_formula().map_err(|e| e.to_string())? {
            let (row, col) = cell.pos;
            let v = lower_cached(&Data::from(cell.value));
            if !matches!(v, Value::Blank) {
                wb.set_value(id, row, col, v);
            }
            if let Some(f) = cell.formula {
                if !f.is_empty() {
                    wb.set_formula(id, row, col, f);
                }
            }
        }
    }
    Ok(wb)
}

#[derive(Default)]
struct Counts {
    cells: usize,
    pass: usize,
    ulp1: usize, // passes under policy, tracked separately
    excluded_unimplemented: usize,
    excluded_parse: usize,
    excluded_external: usize,
    no_cached_value: usize,
    mismatch_numeric: usize,
    mismatch_type: usize,
    mismatch_error: usize,
    panics: usize,
}

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

fn ulps_apart(a: f64, b: f64) -> u64 {
    if a == b {
        return 0;
    }
    if a.is_nan() || b.is_nan() || a.is_sign_positive() != b.is_sign_positive() {
        return u64::MAX;
    }
    let ia = a.abs().to_bits();
    let ib = b.abs().to_bits();
    ia.abs_diff(ib)
}

fn classify(expected: &Value, got: &Value) -> (&'static str, bool) {
    match (expected, got) {
        (Value::Num(e), Value::Num(g)) => {
            let u = ulps_apart(*e, *g);
            if u == 0 {
                ("exact", true)
            } else if u <= 1 {
                ("ulp1", true)
            } else {
                ("mismatch_numeric", false)
            }
        }
        (Value::Text(e), Value::Text(g)) => {
            if e == g {
                ("exact", true)
            } else {
                ("mismatch_text", false)
            }
        }
        (Value::Bool(e), Value::Bool(g)) => {
            if e == g {
                ("exact", true)
            } else {
                ("mismatch_type", false)
            }
        }
        (Value::Err(e), Value::Err(g)) => {
            if e == g {
                ("exact", true)
            } else {
                ("mismatch_error", false)
            }
        }
        // A formula cell with no cached value (Excel stores none for some
        // blanks) — treat cached Blank vs computed 0/"" as pass? NO: count
        // as type mismatch; the corpus will tell us how Excel behaves.
        _ => ("mismatch_type", false),
    }
}

fn fmt_value(v: &Value) -> String {
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

pub fn receipt_cmd(args: &[String]) -> i32 {
    let Some(target) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: xlc receipt <dir-or-file> [--out artifact.json] [--failures failures.jsonl] [--limit N]");
        return 2;
    };
    let out_path = arg_value(args, "--out");
    let failures_path = arg_value(args, "--failures");
    let limit: usize = arg_value(args, "--limit").and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);

    let target = PathBuf::from(target);
    let mut files = Vec::new();
    if target.is_dir() {
        walk(&target, &mut files);
        files.sort();
    } else {
        files.push(target);
    }
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
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let done = AtomicUsize::new(0);

    let totals = files
        .par_iter()
        .map(|path| {
            let mut t = Counts::default();
            let wb = match ingest(path) {
                Ok(wb) => wb,
                Err(_) => return t,
            };
            for (sid, sheet) in wb.sheets.iter().enumerate() {
                for (&(row, col), src) in &sheet.formulas {
                    t.cells += 1;
                    let parsed = match xlc_parse::parse_formula(src) {
                        Ok(e) => e,
                        Err(_) => {
                            t.excluded_parse += 1;
                            continue;
                        }
                    };
                    if parsed.expr.has_external_ref() {
                        t.excluded_external += 1;
                        continue;
                    }
                    let expected = sheet.values.get(&(row, col)).cloned().unwrap_or(Value::Blank);
                    if matches!(expected, Value::Blank) {
                        // Excel stored no cached value — nothing to verify.
                        t.no_cached_value += 1;
                        continue;
                    }
                    let origin = Origin { sheet: sid as u32, row, col };
                    let got = match panic::catch_unwind(AssertUnwindSafe(|| {
                        Interp::new(&wb, origin).eval_formula(&parsed.expr)
                    })) {
                        Ok(v) => v,
                        Err(_) => {
                            t.panics += 1;
                            continue;
                        }
                    };
                    // Unimplemented functions surface as #NAME? where Excel
                    // cached something else: that's an exclusion (Law 9),
                    // not a semantic failure.
                    if got == Value::Err(ExcelError::Name) && expected != Value::Err(ExcelError::Name)
                    {
                        t.excluded_unimplemented += 1;
                        continue;
                    }
                    let (class, ok) = classify(&expected, &got);
                    let mut funcs = std::collections::BTreeSet::new();
                    crate::census::extract_functions(src, &mut funcs);
                    {
                        let mut pf = per_function.lock().unwrap();
                        for f in &funcs {
                            let e = pf.entry(f.clone()).or_insert((0, 0));
                            e.1 += 1;
                            if ok {
                                e.0 += 1;
                            }
                        }
                    }
                    if ok {
                        t.pass += 1;
                        if class == "ulp1" {
                            t.ulp1 += 1;
                        }
                    } else {
                        match class {
                            "mismatch_numeric" => t.mismatch_numeric += 1,
                            "mismatch_error" => t.mismatch_error += 1,
                            _ => t.mismatch_type += 1,
                        }
                        let mut fl = fail_lines.lock().unwrap();
                        if fl.len() < 50_000 {
                            fl.push(FailLine {
                                file: path.display().to_string(),
                                sheet: sheet.name.clone(),
                                cell: cell_a1(row, col),
                                formula: src.clone(),
                                expected: fmt_value(&expected),
                                got: fmt_value(&got),
                                class: class.into(),
                            });
                        }
                    }
                }
            }
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 100 == 0 {
                eprintln!("  …{n} workbooks");
            }
            t
        })
        .reduce(Counts::default, |a, b| Counts {
            cells: a.cells + b.cells,
            pass: a.pass + b.pass,
            ulp1: a.ulp1 + b.ulp1,
            excluded_unimplemented: a.excluded_unimplemented + b.excluded_unimplemented,
            excluded_parse: a.excluded_parse + b.excluded_parse,
            excluded_external: a.excluded_external + b.excluded_external,
            no_cached_value: a.no_cached_value + b.no_cached_value,
            mismatch_numeric: a.mismatch_numeric + b.mismatch_numeric,
            mismatch_type: a.mismatch_type + b.mismatch_type,
            mismatch_error: a.mismatch_error + b.mismatch_error,
            panics: a.panics + b.panics,
        });
    panic::set_hook(prev_hook);

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

    let artifact = serde_json::json!({
        "mode": "per-cell (cached-neighbor context)",
        "ulp_policy": "pass iff bit-identical or within 1 ULP (same sign); ulp1 tracked separately",
        "cells_total": totals.cells - totals.no_cached_value,
        "cells_pass": totals.pass,
        "no_cached_value": totals.no_cached_value,
        "pass_ulp1": totals.ulp1,
        "excluded": {
            "unimplemented_function": totals.excluded_unimplemented,
            "parse": totals.excluded_parse,
            "external_ref": totals.excluded_external,
        },
        "mismatch_classes": {
            "numeric": totals.mismatch_numeric,
            "type": totals.mismatch_type,
            "error": totals.mismatch_error,
        },
        "panics": totals.panics,
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

    let denom = totals.cells - totals.no_cached_value;
    let rate = if denom > 0 { totals.pass as f64 / denom as f64 } else { 0.0 };
    println!(
        "receipt: {}/{} pass ({:.2}%) | excluded: {} unimpl, {} parse, {} extref | {} no-cached | mismatches: {} num, {} type, {} err | {} panics",
        totals.pass,
        denom,
        rate * 100.0,
        totals.excluded_unimplemented,
        totals.excluded_parse,
        totals.excluded_external,
        totals.no_cached_value,
        totals.mismatch_numeric,
        totals.mismatch_type,
        totals.mismatch_error,
        totals.panics
    );
    0
}
