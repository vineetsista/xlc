//! Phase 1: the census. One pass over every corpus workbook producing
//! corpus/census.json (aggregates) + a per-workbook capability report
//! (JSONL). Determines the function scope and the projected refusal /
//! partial-compile rates that decide the business model (XLC.md Phase 1).
//!
//! Per formula cell we record the *set* of functions it calls (plus an
//! EXTREF marker for external-workbook references), aggregated per workbook
//! as combo -> cell count. Formulas are overwhelmingly copied, so a workbook
//! has few distinct combos — and any candidate implemented-function set can
//! then be evaluated exactly, offline, without rescanning the corpus.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufWriter, Read, Write as _};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
pub(crate) use xlc_parse::scan::{extract_functions, EXTREF};

use calamine::{open_workbook, Reader, Xlsx};
use rayon::prelude::*;
use serde::Serialize;

const VOLATILE: &[&str] = &[
    "NOW",
    "TODAY",
    "RAND",
    "RANDBETWEEN",
    "RANDARRAY",
    "OFFSET",
    "INDIRECT",
    "CELL",
    "INFO",
];

#[derive(Serialize)]
struct ComboCount {
    funcs: Vec<String>,
    cells: usize,
}

#[derive(Serialize)]
struct WorkbookReport {
    path: String,
    source: String,
    status: String, // ok | open_error | not_workbook
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    sheets: usize,
    formula_cells: usize,
    vba: bool,
    external_links: bool,
    pivot_tables: bool,
    power_query: bool,
    tables: usize,
    defined_names: usize,
    epoch_1904: bool,
    volatile: Vec<String>,
    /// Distinct function-combination signatures -> cell counts.
    combos: Vec<ComboCount>,
}

/// Zip part-name sniffing for features calamine does not expose.
struct ZipFeatures {
    is_workbook: bool,
    vba: bool,
    external_links: bool,
    pivot_tables: bool,
    power_query: bool,
}

fn sniff_zip(path: &Path) -> Result<ZipFeatures, String> {
    let f = fs::File::open(path).map_err(|e| e.to_string())?;
    let z = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(|e| e.to_string())?;
    let mut feat = ZipFeatures {
        is_workbook: false,
        vba: false,
        external_links: false,
        pivot_tables: false,
        power_query: false,
    };
    for name in z.file_names() {
        if name.ends_with("workbook.xml") {
            feat.is_workbook = true;
        } else if name.ends_with("vbaProject.bin") {
            feat.vba = true;
        } else if name.contains("externalLinks/") {
            feat.external_links = true;
        } else if name.contains("pivotTables/") || name.contains("pivotCache") {
            feat.pivot_tables = true;
        } else if name.contains("queryTables/")
            || name.ends_with("connections.xml")
            || name.contains("customXml/item")
        {
            // Power Query models live in customXml / connections; query
            // tables are the legacy web-query cousin. Coarse but counted
            // as one "external data machinery" feature.
            feat.power_query = true;
        }
    }
    Ok(feat)
}

fn census_workbook(root: &Path, source: &str, path: &Path) -> Option<WorkbookReport> {
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    // Cheap zip-magic pre-filter.
    let mut magic = [0u8; 2];
    match fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic)) {
        Ok(()) if &magic == b"PK" => {}
        _ => return None,
    }
    let zf = match sniff_zip(path) {
        Ok(z) => z,
        Err(_) => return None, // not a readable zip at all
    };
    if !zf.is_workbook {
        return None; // zip, but not an OOXML workbook (odt, docx, plain zip…)
    }

    let mut rep = WorkbookReport {
        path: rel,
        source: source.to_string(),
        status: String::new(),
        error: None,
        sheets: 0,
        formula_cells: 0,
        vba: zf.vba,
        external_links: zf.external_links,
        pivot_tables: zf.pivot_tables,
        power_query: zf.power_query,
        tables: 0,
        defined_names: 0,
        epoch_1904: false,
        volatile: Vec::new(),
        combos: Vec::new(),
    };

    let mut wb = match open_workbook::<Xlsx<_>, _>(path) {
        Ok(wb) => wb,
        Err(e) => {
            rep.status = "open_error".into();
            rep.error = Some(e.to_string());
            return Some(rep);
        }
    };
    rep.epoch_1904 = wb.has_1904_epoch();
    rep.defined_names = wb.defined_names().len();
    if wb.load_tables().is_ok() {
        rep.tables = wb.table_names().len();
    }
    let names = wb.sheet_names();
    rep.sheets = names.len();

    let mut combo_counts: BTreeMap<BTreeSet<String>, usize> = BTreeMap::new();
    let mut volatile: BTreeSet<String> = BTreeSet::new();
    for name in &names {
        if let Ok(range) = wb.worksheet_formula(name) {
            for (_, _, f) in range.used_cells() {
                if f.is_empty() {
                    continue;
                }
                rep.formula_cells += 1;
                let mut funcs = BTreeSet::new();
                extract_functions(f, &mut funcs);
                for v in VOLATILE {
                    if funcs.contains(*v) {
                        volatile.insert((*v).to_string());
                    }
                }
                *combo_counts.entry(funcs).or_insert(0) += 1;
            }
        }
    }
    rep.volatile = volatile.into_iter().collect();
    rep.combos = combo_counts
        .into_iter()
        .map(|(funcs, cells)| ComboCount {
            funcs: funcs.into_iter().collect(),
            cells,
        })
        .collect();
    rep.status = "ok".into();
    Some(rep)
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

pub fn census_cmd(args: &[String]) -> i32 {
    // usage: xlc census <name>=<dir> [<name>=<dir>...] --out census.json --per-workbook reports.jsonl
    let sources: Vec<(&str, &str)> = args
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .filter_map(|a| a.split_once('='))
        .collect();
    if sources.is_empty() {
        eprintln!(
            "usage: xlc census <name>=<dir>... --out census.json --per-workbook reports.jsonl"
        );
        return 2;
    }
    let out_path = arg_value(args, "--out").unwrap_or("census.json");
    let jsonl_path = arg_value(args, "--per-workbook").unwrap_or("census-workbooks.jsonl");

    let mut files: Vec<(String, PathBuf, PathBuf)> = Vec::new();
    for (name, dir) in &sources {
        let root = PathBuf::from(dir);
        let mut fs_ = Vec::new();
        walk(&root, &mut fs_);
        eprintln!("census: {} files under {name}={dir}", fs_.len());
        files.extend(
            fs_.into_iter()
                .map(|p| ((*name).to_string(), root.clone(), p)),
        );
    }
    files.sort();

    let jsonl = Mutex::new(BufWriter::new(match fs::File::create(jsonl_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("census: cannot create {jsonl_path}: {e}");
            return 1;
        }
    }));

    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let done = AtomicUsize::new(0);
    let reports: Vec<WorkbookReport> = files
        .par_iter()
        .filter_map(|(source, root, path)| {
            let r = panic::catch_unwind(AssertUnwindSafe(|| census_workbook(root, source, path)))
                .unwrap_or_else(|_| {
                    Some(WorkbookReport {
                        path: path
                            .strip_prefix(root)
                            .unwrap_or(path)
                            .to_string_lossy()
                            .into_owned(),
                        source: source.clone(),
                        status: "panic".into(),
                        error: None,
                        sheets: 0,
                        formula_cells: 0,
                        vba: false,
                        external_links: false,
                        pivot_tables: false,
                        power_query: false,
                        tables: 0,
                        defined_names: 0,
                        epoch_1904: false,
                        volatile: Vec::new(),
                        combos: Vec::new(),
                    })
                });
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 10_000 == 0 {
                eprintln!("  …{n} files");
            }
            if let Some(ref rep) = r {
                let line = serde_json::to_string(rep).unwrap();
                let mut w = jsonl.lock().unwrap();
                let _ = writeln!(w, "{line}");
            }
            r
        })
        .collect();
    panic::set_hook(prev_hook);
    jsonl.lock().unwrap().flush().ok();

    // ---- aggregate ----
    let total = reports.len();
    let examined = reports.iter().filter(|r| r.status == "ok").count();
    let with_formulas = reports.iter().filter(|r| r.formula_cells > 0).count();
    let formula_cells_total: usize = reports.iter().map(|r| r.formula_cells).sum();

    let count = |pred: &dyn Fn(&WorkbookReport) -> bool| reports.iter().filter(|r| pred(r)).count();
    let features = serde_json::json!({
        "vba": count(&|r| r.vba),
        "external_links": count(&|r| r.external_links),
        "pivot_tables": count(&|r| r.pivot_tables),
        "power_query": count(&|r| r.power_query),
        "tables": count(&|r| r.tables > 0),
        "defined_names": count(&|r| r.defined_names > 0),
        "epoch_1904": count(&|r| r.epoch_1904),
        "volatile_any": count(&|r| !r.volatile.is_empty()),
    });

    let mut volatile_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for v in VOLATILE {
        volatile_counts.insert(v, count(&|r| r.volatile.iter().any(|x| x == v)));
    }

    // Function frequency: cells = formula cells whose combo includes the
    // function; workbooks = workbooks using it anywhere.
    let mut func_cells: BTreeMap<String, usize> = BTreeMap::new();
    let mut func_workbooks: BTreeMap<String, usize> = BTreeMap::new();
    for r in &reports {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for combo in &r.combos {
            for f in &combo.funcs {
                if f == EXTREF {
                    continue;
                }
                *func_cells.entry(f.clone()).or_insert(0) += combo.cells;
                seen.insert(f);
            }
        }
        for f in seen {
            *func_workbooks.entry(f.to_string()).or_insert(0) += 1;
        }
    }
    let mut ranked: Vec<(&String, &usize)> = func_cells.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let mut functions = serde_json::Map::new();
    for (name, cells) in &ranked {
        functions.insert(
            (*name).clone(),
            serde_json::json!({"cells": cells, "workbooks": func_workbooks.get(*name).copied().unwrap_or(0)}),
        );
    }

    let mut per_source: BTreeMap<&str, usize> = BTreeMap::new();
    for r in &reports {
        *per_source.entry(r.source.as_str()).or_insert(0) += 1;
    }

    let census = serde_json::json!({
        "workbooks_total": total,
        "workbooks_examined": examined,
        "workbooks_with_formulas": with_formulas,
        "formula_cells_total": formula_cells_total,
        "sources": per_source,
        "features": features,
        "volatile": volatile_counts,
        "functions": serde_json::Value::Object(functions),
    });
    if let Err(e) = fs::write(out_path, serde_json::to_vec_pretty(&census).unwrap()) {
        eprintln!("census: cannot write {out_path}: {e}");
        return 1;
    }
    println!(
        "census: {total} workbooks | examined {examined} | with formulas {with_formulas} | {formula_cells_total} formula cells | {} distinct functions",
        ranked.len()
    );
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn funcs(f: &str) -> Vec<String> {
        let mut s = BTreeSet::new();
        extract_functions(f, &mut s);
        s.into_iter().collect()
    }

    #[test]
    fn nested_and_operators() {
        assert_eq!(funcs("IF(SUM(A1:A2)>0,MAX(B:B),0)"), ["IF", "MAX", "SUM"]);
    }

    #[test]
    fn xlfn_prefix_stripped() {
        assert_eq!(funcs("_xlfn.XLOOKUP(A1,B:B,C:C)"), ["XLOOKUP"]);
        assert_eq!(funcs("_xlfn._xlws.FILTER(A:A,B:B)"), ["FILTER"]);
    }

    #[test]
    fn string_literal_not_a_call() {
        assert_eq!(funcs("\"SUM(\"&A1"), Vec::<String>::new());
        assert_eq!(funcs("CONCAT(\"IF(\",A1)"), ["CONCAT"]);
    }

    #[test]
    fn quoted_sheet_not_a_call() {
        assert_eq!(funcs("'MAX(no)'!A1+SUM(B:B)"), ["SUM"]);
    }

    #[test]
    fn external_ref_marked() {
        assert_eq!(funcs("[1]Sheet1!A1*2"), [EXTREF]);
        assert_eq!(funcs("SUM([3]Data!A1:A9)"), ["SUM", EXTREF]);
    }

    #[test]
    fn table_ref_not_external() {
        assert_eq!(funcs("SUM(Table1[Amount])"), ["SUM"]);
        assert_eq!(funcs("Table1[[#Headers],[Col]]"), Vec::<String>::new());
    }

    #[test]
    fn ref_followed_by_paren_is_not_a_call() {
        // A1(…) can't occur in valid formulas, but `IF (` with space can.
        assert_eq!(funcs("IF (A1>0,1,0)"), ["IF"]);
    }
}
