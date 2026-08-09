//! Workbook ingest (§8.1) shared by the CLI and the wasm surface: calamine
//! streaming reader → the xlc-eval workbook model, plus the array-formula
//! XML sniffer, the capability report, and the per-cell receipt.
//!
//! Never `worksheet_range` (dense materialization — corpus files lie about
//! their dimensions; one declared A1:XFD1048576 and the dense path
//! attempted a 512 GiB allocation).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use calamine::{open_workbook, open_workbook_from_rs, Data, Reader, Xlsx};
use xlc_eval::interp::{Interp, Origin};
use xlc_eval::workbook::Workbook;
use xlc_eval::{ExcelError, Value};
use xlc_parse::scan::{extract_functions, VOLATILE_NONDETERMINISTIC};

pub fn lower_cached(d: &Data) -> Value {
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

/// Load a workbook from a filesystem path.
pub fn ingest_path(path: &Path) -> Result<Workbook, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let array_map = {
        let f2 = std::fs::File::open(path).map_err(|e| e.to_string())?;
        sniff_array_cells(std::io::BufReader::new(f2))
    };
    let xl = open_workbook::<Xlsx<_>, _>(path).map_err(|e| e.to_string())?;
    drop(f);
    build(xl, array_map)
}

/// Load a workbook from in-memory bytes (the browser path).
pub fn ingest_bytes(bytes: &[u8]) -> Result<Workbook, String> {
    let array_map = sniff_array_cells(Cursor::new(bytes));
    let xl = open_workbook_from_rs::<Xlsx<_>, _>(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    build(xl, array_map)
}

fn build<RS: Read + Seek>(
    mut xl: Xlsx<RS>,
    array_map: HashMap<String, HashSet<(u32, u32)>>,
) -> Result<Workbook, String> {
    let mut wb = Workbook::default();
    wb.epoch_1904 = xl.has_1904_epoch();
    // Defined names: bodies are formula text. Unparseable ones (LAMBDA…)
    // stay absent → #NAME? → excluded, never a false mismatch. Names whose
    // body references another workbook poison their users (Law 9).
    for (name, body) in xl.defined_names().to_vec() {
        if let Ok(f) = xlc_parse::parse_formula(&body) {
            if f.expr.has_external_ref() {
                wb.external_names.insert(name.to_uppercase());
            } else {
                wb.names.insert(name.to_uppercase(), f.expr);
            }
        }
    }
    let sheet_names = xl.sheet_names();
    for name in &sheet_names {
        let id = wb.add_sheet(name);
        if let Some(cells) = array_map.get(name.as_str()) {
            wb.sheets[id as usize].array_cells = cells.clone();
        }
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

/// Map sheet name -> cells holding `<f t="array">` anchors. calamine does
/// not expose the array attribute, so we sniff the worksheet XML directly.
pub fn sniff_array_cells<RS: Read + Seek>(rs: RS) -> HashMap<String, HashSet<(u32, u32)>> {
    let mut out: HashMap<String, HashSet<(u32, u32)>> = HashMap::new();
    let Ok(mut z) = zip::ZipArchive::new(rs) else {
        return out;
    };

    fn read_str<RS: Read + Seek>(z: &mut zip::ZipArchive<RS>, name: &str) -> Option<String> {
        let mut s = String::new();
        z.by_name(name).ok()?.read_to_string(&mut s).ok()?;
        Some(s)
    }
    let Some(workbook_xml) = read_str(&mut z, "xl/workbook.xml") else {
        return out;
    };
    let rels_xml = read_str(&mut z, "xl/_rels/workbook.xml.rels").unwrap_or_default();

    let mut rid_to_path: HashMap<String, String> = HashMap::new();
    for chunk in rels_xml.split("<Relationship ").skip(1) {
        if let (Some(id), Some(target)) = (attr(chunk, "Id"), attr(chunk, "Target")) {
            let path = if let Some(stripped) = target.strip_prefix('/') {
                stripped.to_string()
            } else {
                format!("xl/{target}")
            };
            rid_to_path.insert(id.to_string(), path);
        }
    }
    let mut sheets: Vec<(String, String)> = Vec::new();
    for chunk in workbook_xml.split("<sheet ").skip(1) {
        if let (Some(name), Some(rid)) = (attr(chunk, "name"), attr(chunk, "r:id")) {
            sheets.push((unescape_xml(name), rid.to_string()));
        }
    }
    for (name, rid) in sheets {
        let Some(sheet_path) = rid_to_path.get(&rid) else {
            continue;
        };
        let Some(xml) = read_str(&mut z, sheet_path) else {
            continue;
        };
        let mut cells = HashSet::new();
        let mut from = 0usize;
        while let Some(hit) = xml[from..].find("t=\"array\"") {
            let abs = from + hit;
            if let Some(rpos) = xml[..abs].rfind("r=\"") {
                let rest = &xml[rpos + 3..];
                if let Some(endq) = rest.find('"') {
                    if let Some(rc) = a1_to_rc(&rest[..endq]) {
                        cells.insert(rc);
                    }
                }
            }
            from = abs + 1;
        }
        if !cells.is_empty() {
            out.insert(name, cells);
        }
    }
    out
}

fn attr<'x>(chunk: &'x str, name: &str) -> Option<&'x str> {
    let pat = format!("{name}=\"");
    let start = chunk.find(&pat)? + pat.len();
    let end = chunk[start..].find('"')? + start;
    Some(&chunk[start..end])
}

fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn a1_to_rc(cell: &str) -> Option<(u32, u32)> {
    let letters_end = cell.bytes().take_while(|b| b.is_ascii_uppercase()).count();
    if letters_end == 0 || letters_end == cell.len() {
        return None;
    }
    let col = xlc_parse::ast::letters_col(&cell[..letters_end])?;
    let row: u32 = cell[letters_end..].parse().ok()?;
    Some((row.checked_sub(1)?, col))
}

// ---- the per-cell receipt (§8.4), shared by CLI and wasm ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// exact | ulp1 | sig15
    Pass(&'static str),
    /// external_ref | volatile | array_formula | unimplemented | parse
    Excluded(&'static str),
    /// numeric | text | type | error
    Mismatch(&'static str),
    NoCachedValue,
}

pub struct CellReport<'a> {
    pub sheet_idx: usize,
    pub row: u32,
    pub col: u32,
    pub formula: &'a str,
    pub expected: Value,
    pub got: Option<Value>,
    pub outcome: Outcome,
}

#[derive(Default, Debug, Clone, serde::Serialize)]
pub struct ReceiptSummary {
    pub cells: usize,
    pub pass: usize,
    pub ulp1: usize,
    pub sig15: usize,
    pub excluded: BTreeMap<&'static str, usize>,
    pub mismatches: BTreeMap<&'static str, usize>,
    pub no_cached: usize,
}

impl ReceiptSummary {
    pub fn verifiable(&self) -> usize {
        self.cells - self.no_cached - self.excluded.values().sum::<usize>()
    }
}

/// Run the per-cell receipt over an ingested workbook. `on_cell` sees every
/// formula cell's outcome (per-function tallies, failure logs — the
/// callers' business). Policy: pass iff bit-identical, within 1 ULP (same
/// sign), or equal at 15 significant decimal digits.
pub fn run_receipt(wb: &Workbook, mut on_cell: impl FnMut(&CellReport)) -> ReceiptSummary {
    let mut sum = ReceiptSummary::default();
    let mut fns: BTreeSet<String> = BTreeSet::new();
    for (sid, sheet) in wb.sheets.iter().enumerate() {
        for (&(row, col), src) in &sheet.formulas {
            sum.cells += 1;
            let mut report = |expected: Value, got: Option<Value>, outcome: Outcome| {
                match outcome {
                    Outcome::Pass(class) => {
                        sum.pass += 1;
                        if class == "ulp1" {
                            sum.ulp1 += 1;
                        } else if class == "sig15" {
                            sum.sig15 += 1;
                        }
                    }
                    Outcome::Excluded(kind) => *sum.excluded.entry(kind).or_insert(0) += 1,
                    Outcome::Mismatch(class) => *sum.mismatches.entry(class).or_insert(0) += 1,
                    Outcome::NoCachedValue => sum.no_cached += 1,
                }
                on_cell(&CellReport {
                    sheet_idx: sid,
                    row,
                    col,
                    formula: src,
                    expected,
                    got,
                    outcome,
                });
            };
            let parsed = match xlc_parse::parse_formula(src) {
                Ok(p) => p,
                Err(_) => {
                    report(Value::Blank, None, Outcome::Excluded("parse"));
                    continue;
                }
            };
            if parsed.expr.has_external_ref() || uses_external_name(&parsed.expr, wb) {
                report(Value::Blank, None, Outcome::Excluded("external_ref"));
                continue;
            }
            if sheet.array_cells.contains(&(row, col)) {
                report(Value::Blank, None, Outcome::Excluded("array_formula"));
                continue;
            }
            fns.clear();
            extract_functions(src, &mut fns);
            if VOLATILE_NONDETERMINISTIC.iter().any(|f| fns.contains(*f)) {
                report(Value::Blank, None, Outcome::Excluded("volatile"));
                continue;
            }
            let expected = sheet
                .values
                .get(&(row, col))
                .cloned()
                .unwrap_or(Value::Blank);
            if matches!(expected, Value::Blank) {
                report(Value::Blank, None, Outcome::NoCachedValue);
                continue;
            }
            let origin = Origin {
                sheet: sid as u32,
                row,
                col,
            };
            let got = Interp::new(wb, origin).eval_formula(&parsed.expr);
            if got == Value::Err(ExcelError::Name) && expected != Value::Err(ExcelError::Name) {
                report(expected, Some(got), Outcome::Excluded("unimplemented"));
                continue;
            }
            let outcome = classify(&expected, &got);
            report(expected, Some(got), outcome);
        }
    }
    sum
}

fn uses_external_name(e: &xlc_parse::Expr, wb: &Workbook) -> bool {
    let mut found = false;
    e.walk(&mut |n| {
        if let xlc_parse::Expr::Name { name, .. } = n {
            if wb.external_names.contains(&name.to_uppercase()) {
                found = true;
            }
        }
    });
    found
}

/// 15-significant-digit decimal rendering (Excel's stored-literal precision).
fn sig15(x: f64) -> String {
    format!("{x:.14e}")
}

fn ulps_apart(a: f64, b: f64) -> u64 {
    if a == b {
        return 0;
    }
    if a.is_nan() || b.is_nan() || a.is_sign_positive() != b.is_sign_positive() {
        return u64::MAX;
    }
    a.abs().to_bits().abs_diff(b.abs().to_bits())
}

pub fn classify(expected: &Value, got: &Value) -> Outcome {
    match (expected, got) {
        (Value::Num(e), Value::Num(g)) => {
            let u = ulps_apart(*e, *g);
            if u == 0 {
                Outcome::Pass("exact")
            } else if u <= 1 {
                Outcome::Pass("ulp1")
            } else if sig15(*e) == sig15(*g) {
                Outcome::Pass("sig15")
            } else {
                Outcome::Mismatch("numeric")
            }
        }
        (Value::Text(e), Value::Text(g)) => {
            if e == g {
                Outcome::Pass("exact")
            } else {
                Outcome::Mismatch("text")
            }
        }
        (Value::Bool(e), Value::Bool(g)) => {
            if e == g {
                Outcome::Pass("exact")
            } else {
                Outcome::Mismatch("type")
            }
        }
        (Value::Err(e), Value::Err(g)) => {
            if e == g {
                Outcome::Pass("exact")
            } else {
                Outcome::Mismatch("error")
            }
        }
        _ => Outcome::Mismatch("type"),
    }
}

/// Per-feature exclusion counts (Law 9): what the capability report shows.
pub fn capability_report(wb: &Workbook) -> BTreeMap<String, usize> {
    let mut cr: BTreeMap<String, usize> = BTreeMap::new();
    let mut fns = BTreeSet::new();
    for sheet in &wb.sheets {
        for src in sheet.formulas.values() {
            let Ok(parsed) = xlc_parse::parse_formula(src) else {
                *cr.entry("unparseable".into()).or_insert(0) += 1;
                continue;
            };
            if parsed.expr.has_external_ref() {
                *cr.entry("external_ref".into()).or_insert(0) += 1;
            }
            fns.clear();
            extract_functions(src, &mut fns);
            if VOLATILE_NONDETERMINISTIC.iter().any(|f| fns.contains(*f)) {
                *cr.entry("volatile_nondeterministic".into()).or_insert(0) += 1;
            }
        }
        if !sheet.array_cells.is_empty() {
            *cr.entry("array_formula".into()).or_insert(0) += sheet.array_cells.len();
        }
    }
    cr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_and_path_ingest_agree() {
        let path = std::path::Path::new("../../tests/cases/basic-sum.xlsx");
        let by_path = ingest_path(path).unwrap();
        let bytes = std::fs::read(path).unwrap();
        let by_bytes = ingest_bytes(&bytes).unwrap();
        assert_eq!(by_path.sheets.len(), by_bytes.sheets.len());
        assert_eq!(
            by_path.sheets[0].formulas.len(),
            by_bytes.sheets[0].formulas.len()
        );
        let sum = run_receipt(&by_bytes, |_| {});
        assert_eq!(sum.cells, 2);
        assert_eq!(sum.pass, 2);
    }
}
