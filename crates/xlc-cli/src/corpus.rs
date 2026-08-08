//! Corpus tooling: filter a raw spreadsheet dump down to valid OOXML
//! workbooks with formulas, build the deterministic regression subset, and
//! verify a subset directory loads.
//!
//! Never-Stall rung 2 applies throughout: a corpus drawn from Common Crawl
//! WILL contain garbage — truncated zips, mislabeled HTML, zip bombs. One bad
//! file must never stop a run; it is recorded and counted.

use std::fs;
use std::io::Read;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use calamine::{open_workbook, Reader, Xlsx};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Files larger than this are skipped (recorded, not silently dropped).
/// FUSE averages ~40 KB/file; anything this big is an outlier we can
/// revisit in the Phase 1 census.
const MAX_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone)]
pub struct FileRecord {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub status: String, // ok | open_error | not_zip | too_large | read_error | panic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub sheets: usize,
    pub formula_cells: usize,
}

#[derive(Serialize, Deserialize)]
pub struct FilterOutput {
    pub root: String,
    pub scanned: usize,
    pub ok: usize,
    pub with_formulas: usize,
    pub skipped: usize,
    pub records: Vec<FileRecord>,
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

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

/// Open as OOXML and count formula cells across all sheets.
fn probe_workbook(path: &Path) -> Result<(usize, usize), String> {
    let mut wb = open_workbook::<Xlsx<_>, _>(path).map_err(|e| e.to_string())?;
    let names = wb.sheet_names();
    let mut formula_cells = 0usize;
    for name in &names {
        if let Ok(range) = wb.worksheet_formula(name) {
            formula_cells += range.used_cells().filter(|(_, _, f)| !f.is_empty()).count();
        }
    }
    Ok((names.len(), formula_cells))
}

fn examine(root: &Path, path: &Path) -> FileRecord {
    let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned();
    let bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut rec = FileRecord {
        path: rel,
        sha256: String::new(),
        bytes,
        status: String::new(),
        error: None,
        sheets: 0,
        formula_cells: 0,
    };

    if bytes > MAX_BYTES {
        rec.status = "too_large".into();
        return rec;
    }
    // Cheap OOXML pre-filter: zip local-file-header magic.
    let mut magic = [0u8; 4];
    match fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic).map(|_| f)) {
        Ok(_) if &magic[..2] == b"PK" => {}
        Ok(_) => {
            rec.status = "not_zip".into();
            return rec;
        }
        Err(e) => {
            rec.status = "read_error".into();
            rec.error = Some(e.to_string());
            return rec;
        }
    }
    match fs::read(path) {
        Ok(data) => rec.sha256 = sha256_hex(&data),
        Err(e) => {
            rec.status = "read_error".into();
            rec.error = Some(e.to_string());
            return rec;
        }
    }
    match panic::catch_unwind(AssertUnwindSafe(|| probe_workbook(path))) {
        Ok(Ok((sheets, formula_cells))) => {
            rec.status = "ok".into();
            rec.sheets = sheets;
            rec.formula_cells = formula_cells;
        }
        Ok(Err(e)) => {
            rec.status = "open_error".into();
            rec.error = Some(e);
        }
        Err(_) => {
            rec.status = "panic".into();
        }
    }
    rec
}

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
}

pub fn filter_cmd(args: &[String]) -> i32 {
    let Some(root) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: xlc corpus-filter <dir> --out <filtered.json>");
        return 2;
    };
    let out_path = arg_value(args, "--out").unwrap_or("filtered.json");
    let root = PathBuf::from(root);

    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    eprintln!("corpus-filter: {} files under {}", files.len(), root.display());

    // calamine may panic on hostile input; keep the run quiet and count them.
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let done = AtomicUsize::new(0);
    let records: Vec<FileRecord> = files
        .par_iter()
        .map(|p| {
            let r = examine(&root, p);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 10_000 == 0 {
                eprintln!("  …{n} files examined");
            }
            r
        })
        .collect();
    panic::set_hook(prev_hook);

    let ok = records.iter().filter(|r| r.status == "ok").count();
    let with_formulas = records.iter().filter(|r| r.status == "ok" && r.formula_cells > 0).count();
    let out = FilterOutput {
        root: root.display().to_string(),
        scanned: records.len(),
        ok,
        with_formulas,
        skipped: records.len() - ok,
        records,
    };
    if let Err(e) = fs::write(out_path, serde_json::to_vec_pretty(&out).unwrap()) {
        eprintln!("corpus-filter: cannot write {out_path}: {e}");
        return 1;
    }
    println!(
        "corpus-filter: scanned {} | valid OOXML {} | with formulas {} | skipped {}",
        out.scanned, out.ok, out.with_formulas, out.skipped
    );
    0
}

#[derive(Serialize)]
struct ManifestEntry {
    file: String,
    sha256: String,
    bytes: u64,
    formula_cells: usize,
    source: String,
}

#[derive(Serialize)]
struct Manifest {
    rule: String,
    workbooks: Vec<ManifestEntry>,
}

pub fn subset_cmd(args: &[String]) -> i32 {
    let Some(filtered) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: xlc corpus-subset <filtered.json> --n 500 --min-formulas 10 --out-dir subset --manifest manifest.json");
        return 2;
    };
    let n: usize = arg_value(args, "--n").and_then(|v| v.parse().ok()).unwrap_or(500);
    let min_formulas: usize =
        arg_value(args, "--min-formulas").and_then(|v| v.parse().ok()).unwrap_or(10);
    let out_dir = PathBuf::from(arg_value(args, "--out-dir").unwrap_or("subset"));
    let manifest_path = arg_value(args, "--manifest").unwrap_or("manifest.json");

    let data = match fs::read(filtered) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("corpus-subset: cannot read {filtered}: {e}");
            return 1;
        }
    };
    let input: FilterOutput = match serde_json::from_slice(&data) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("corpus-subset: bad json: {e}");
            return 1;
        }
    };
    let filtered_root = PathBuf::from(&input.root);

    // Deterministic rule (docs/decisions.md): status ok, >= min formula
    // cells, dedupe by content sha256, order by sha256 ascending, first n.
    let mut candidates: Vec<&FileRecord> = input
        .records
        .iter()
        .filter(|r| r.status == "ok" && r.formula_cells >= min_formulas)
        .collect();
    candidates.sort_by(|a, b| a.sha256.cmp(&b.sha256));
    candidates.dedup_by(|a, b| a.sha256 == b.sha256);
    if candidates.len() < n {
        eprintln!(
            "corpus-subset: only {} candidates meet the rule (need {n})",
            candidates.len()
        );
        return 1;
    }

    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("corpus-subset: cannot create {}: {e}", out_dir.display());
        return 1;
    }
    let mut entries = Vec::with_capacity(n);
    for rec in candidates.into_iter().take(n) {
        let src = filtered_root.join(&rec.path);
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .filter(|e| matches!(e.as_str(), "xlsx" | "xlsm" | "xltx" | "xltm"))
            .unwrap_or_else(|| "xlsx".into());
        let file = format!("{}.{ext}", &rec.sha256[..16]);
        if let Err(e) = fs::copy(&src, out_dir.join(&file)) {
            eprintln!("corpus-subset: copy {} failed: {e}", src.display());
            return 1;
        }
        entries.push(ManifestEntry {
            file,
            sha256: rec.sha256.clone(),
            bytes: rec.bytes,
            formula_cells: rec.formula_cells,
            source: rec.path.clone(),
        });
    }
    let manifest = Manifest {
        rule: format!(
            "valid OOXML, >={min_formulas} formula cells, dedupe by sha256, order by sha256 asc, first {n}"
        ),
        workbooks: entries,
    };
    if let Err(e) = fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()) {
        eprintln!("corpus-subset: cannot write {manifest_path}: {e}");
        return 1;
    }
    println!("corpus-subset: wrote {n} workbooks to {} + {manifest_path}", out_dir.display());
    0
}

pub fn verify_cmd(args: &[String]) -> i32 {
    let Some(dir) = args.first() else {
        eprintln!("usage: xlc corpus-verify <dir>");
        return 2;
    };
    let mut files = Vec::new();
    walk(Path::new(dir), &mut files);
    files.sort();
    if files.is_empty() {
        eprintln!("corpus-verify: no files under {dir}");
        return 1;
    }
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let failures: Vec<String> = files
        .par_iter()
        .filter_map(|p| {
            match panic::catch_unwind(AssertUnwindSafe(|| probe_workbook(p))) {
                Ok(Ok((_, formulas))) if formulas > 0 => None,
                Ok(Ok((_, _))) => Some(format!("{}: loads but has no formula cells", p.display())),
                Ok(Err(e)) => Some(format!("{}: {e}", p.display())),
                Err(_) => Some(format!("{}: panic while loading", p.display())),
            }
        })
        .collect();
    panic::set_hook(prev_hook);
    for f in &failures {
        eprintln!("corpus-verify: {f}");
    }
    println!("corpus-verify: {}/{} workbooks load with >=1 formula", files.len() - failures.len(), files.len());
    if failures.is_empty() {
        0
    } else {
        1
    }
}
