//! Gate 2 oracle: round-trip the parser over every formula cell in the
//! corpus. `parse(f).print() == f`, byte-exact. Failures are aggregated by
//! formula text so systematic grammar gaps surface as one line with a huge
//! count — "failures are self-reporting, so this converges fast" (§8.2).

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use calamine::{open_workbook, Reader, Xlsx};
use rayon::prelude::*;
use serde::Serialize;

#[derive(Default)]
struct Tally {
    formulas: usize,
    parsed: usize,
    roundtrip_ok: usize,
    panics: usize,
}

#[derive(Serialize)]
struct FailureEntry {
    formula: String,
    count: usize,
    kind: String, // parse_error | roundtrip_mismatch | panic
    detail: String,
    example_file: String,
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

fn is_zip(path: &Path) -> bool {
    let mut magic = [0u8; 2];
    matches!(
        fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic)),
        Ok(())
    ) && &magic == b"PK"
}

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}

pub fn parse_corpus_cmd(args: &[String]) -> i32 {
    let dirs: Vec<&String> = args.iter().take_while(|a| !a.starts_with("--")).collect();
    if dirs.is_empty() {
        eprintln!(
            "usage: xlc parse-corpus <dir>... --out <artifact.json> --failures <failures.jsonl>"
        );
        return 2;
    }
    let out_path = arg_value(args, "--out").unwrap_or("docs/benchmarks/parse-roundtrip.json");
    let failures_path =
        arg_value(args, "--failures").unwrap_or("docs/benchmarks/parse-failures.jsonl");

    let mut files = Vec::new();
    for d in &dirs {
        walk(Path::new(d.as_str()), &mut files);
    }
    files.sort();
    files.retain(|p| is_zip(p));
    eprintln!("parse-corpus: {} candidate workbooks", files.len());

    let failures: Mutex<HashMap<String, FailureEntry>> = Mutex::new(HashMap::new());
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let done = AtomicUsize::new(0);
    let start = Instant::now();

    let tally = files
        .par_iter()
        .map(|path| {
            let mut t = Tally::default();
            let Ok(mut wb) = open_workbook::<Xlsx<_>, _>(path) else {
                return t;
            };
            let names = wb.sheet_names();
            for name in &names {
                let Ok(range) = wb.worksheet_formula(name) else {
                    continue;
                };
                for (_, _, f) in range.used_cells() {
                    if f.is_empty() {
                        continue;
                    }
                    t.formulas += 1;
                    let result = panic::catch_unwind(AssertUnwindSafe(|| {
                        xlc_parse::parse_formula(f).map(|e| e.to_formula_string())
                    }));
                    let (kind, detail) = match result {
                        Ok(Ok(printed)) => {
                            t.parsed += 1;
                            if printed == *f {
                                t.roundtrip_ok += 1;
                                continue;
                            }
                            ("roundtrip_mismatch", printed)
                        }
                        Ok(Err(e)) => ("parse_error", e.msg),
                        Err(_) => {
                            t.panics += 1;
                            ("panic", String::new())
                        }
                    };
                    let mut fl = failures.lock().unwrap();
                    let entry = fl.entry(f.clone()).or_insert_with(|| FailureEntry {
                        formula: f.clone(),
                        count: 0,
                        kind: kind.into(),
                        detail,
                        example_file: path.display().to_string(),
                    });
                    entry.count += 1;
                }
            }
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 5_000 == 0 {
                eprintln!("  …{n} workbooks");
            }
            t
        })
        .reduce(Tally::default, |a, b| Tally {
            formulas: a.formulas + b.formulas,
            parsed: a.parsed + b.parsed,
            roundtrip_ok: a.roundtrip_ok + b.roundtrip_ok,
            panics: a.panics + b.panics,
        });
    panic::set_hook(prev_hook);
    let secs = start.elapsed().as_secs_f64();

    let mut fails: Vec<FailureEntry> = failures.into_inner().unwrap().into_values().collect();
    fails.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.formula.cmp(&b.formula))
    });
    {
        let mut out = String::new();
        for f in fails.iter().take(100_000) {
            out.push_str(&serde_json::to_string(f).unwrap());
            out.push('\n');
        }
        if let Some(parent) = Path::new(failures_path).parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::write(failures_path, out).is_err() {
            eprintln!("parse-corpus: cannot write {failures_path}");
            return 1;
        }
    }

    let artifact = serde_json::json!({
        "formulas_total": tally.formulas,
        "parsed": tally.parsed,
        "roundtrip_ok": tally.roundtrip_ok,
        "panics": tally.panics,
        "distinct_failures": fails.len(),
        "throughput_formulas_per_s": (tally.formulas as f64 / secs) as u64,
        "elapsed_s": secs,
        "machine": cpu_model(),
        "threads": rayon::current_num_threads(),
        "failures_log": failures_path,
    });
    if let Some(parent) = Path::new(out_path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    if fs::write(out_path, serde_json::to_vec_pretty(&artifact).unwrap()).is_err() {
        eprintln!("parse-corpus: cannot write {out_path}");
        return 1;
    }
    let rate = if tally.formulas > 0 {
        tally.roundtrip_ok as f64 / tally.formulas as f64
    } else {
        0.0
    };
    println!(
        "parse-corpus: {}/{} round-trip ({:.4}%) | {} parse-ok | {} panics | {:.0} formulas/s",
        tally.roundtrip_ok,
        tally.formulas,
        rate * 100.0,
        tally.parsed,
        tally.panics,
        tally.formulas as f64 / secs
    );
    if rate >= 0.995 && tally.panics == 0 {
        0
    } else {
        1
    }
}
