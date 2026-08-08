//! The first three detectors (§8.8), all over parsed formulas — never text.
//!
//! D1 `inconsistent-region`: inside a contiguous run of copied formulas,
//!    an interior minority whose shape deviates from a strong majority.
//! D2 `range-off-by-one`: the D1 subclass where the deviation is exactly
//!    one range boundary moved by one row/column — reported with sharper
//!    evidence and its own precision track.
//! D3 `ref-error`: the formula references a deleted cell or sheet
//!    (#REF! in any position).
//!
//! Precision guards (Law 7): runs must be >=6 cells, the majority >=75%
//! of the run, deviants strictly interior (edges of a run are routinely
//! intentional — totals, headers), and at most 2 deviants per run.

use crate::shape::{shapes, Shapes};
use serde::Serialize;
use std::collections::HashMap;
use xlc_eval::workbook::Workbook;

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub detector: String,
    pub sheet: String,
    pub cell: String,
    pub formula: String,
    /// Machine-checkable evidence, legible to a human (Law 8).
    pub proof: String,
}

pub fn cell_a1(row: u32, col: u32) -> String {
    format!("{}{}", xlc_parse::ast::col_letters(col), row + 1)
}

struct CellShape {
    row: u32,
    col: u32,
    formula: String,
    shapes: Shapes,
}

pub fn analyze(wb: &Workbook) -> Vec<Finding> {
    let mut findings = Vec::new();
    for sheet in &wb.sheets {
        // Parse every formula cell once.
        let mut cells: Vec<CellShape> = Vec::new();
        for (&(row, col), src) in &sheet.formulas {
            let Ok(parsed) = xlc_parse::parse_formula(src) else { continue };
            let sh = shapes(&parsed.expr, row, col);
            if sh.has_ref_error {
                findings.push(Finding {
                    detector: "ref-error".into(),
                    sheet: sheet.name.clone(),
                    cell: cell_a1(row, col),
                    formula: src.clone(),
                    proof: format!(
                        "{}!{} contains a reference to a deleted cell or sheet (#REF!): {}",
                        sheet.name,
                        cell_a1(row, col),
                        src
                    ),
                });
            }
            cells.push(CellShape { row, col, formula: src.clone(), shapes: sh });
        }

        // Sheet-wide shape frequencies: a deviant whose shape appears
        // more than once on the sheet is itself a copied pattern
        // (alternating-region layouts), not a one-off slip.
        let mut shape_counts: HashMap<&str, usize> = HashMap::new();
        for c in &cells {
            *shape_counts.entry(c.shapes.full.as_str()).or_insert(0) += 1;
        }

        // Horizontal runs (fixed row, contiguous columns) and vertical
        // runs (fixed column, contiguous rows).
        let mut sheet_findings: Vec<(Finding, bool, u32)> = Vec::new();
        run_findings(&mut sheet_findings, &sheet.name, &cells, &shape_counts, true);
        run_findings(&mut sheet_findings, &sheet.name, &cells, &shape_counts, false);

        // (f) lane-repeat filter, generic findings only: the same column
        // deviating the same way in several horizontal runs (or row in
        // several vertical runs) is deliberate column/row semantics, not
        // repeated accidents.
        let mut lane_counts: HashMap<(bool, u32), usize> = HashMap::new();
        for (f, horiz, lane) in &sheet_findings {
            if f.detector == "inconsistent-region" {
                *lane_counts.entry((*horiz, *lane)).or_insert(0) += 1;
            }
        }
        for (f, horiz, lane) in sheet_findings {
            if f.detector == "inconsistent-region"
                && lane_counts.get(&(horiz, lane)).copied().unwrap_or(0) > 1
            {
                continue;
            }
            findings.push(f);
        }
    }
    findings
}

fn run_findings(
    findings: &mut Vec<(Finding, bool, u32)>,
    sheet: &str,
    cells: &[CellShape],
    shape_counts: &HashMap<&str, usize>,
    horizontal: bool,
) {
    // Group by the fixed axis.
    let mut lanes: HashMap<u32, Vec<&CellShape>> = HashMap::new();
    for c in cells {
        lanes.entry(if horizontal { c.row } else { c.col }).or_default().push(c);
    }
    for lane in lanes.values_mut() {
        lane.sort_by_key(|c| if horizontal { c.col } else { c.row });
        // Split into contiguous runs.
        let mut run: Vec<&CellShape> = Vec::new();
        let mut prev: Option<u32> = None;
        for c in lane.iter() {
            let pos = if horizontal { c.col } else { c.row };
            if prev.is_some_and(|p| pos != p + 1) {
                flag_run(findings, sheet, &run, shape_counts, horizontal);
                run.clear();
            }
            run.push(c);
            prev = Some(pos);
        }
        flag_run(findings, sheet, &run, shape_counts, horizontal);
    }
}

fn flag_run(
    findings: &mut Vec<(Finding, bool, u32)>,
    sheet: &str,
    run: &[&CellShape],
    shape_counts: &HashMap<&str, usize>,
    horizontal: bool,
) {
    if run.len() < 6 {
        return;
    }
    // Majority shape.
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for c in run {
        *counts.entry(c.shapes.full.as_str()).or_insert(0) += 1;
    }
    let (&majority_shape, &majority_n) = counts.iter().max_by_key(|(_, &n)| n).unwrap();
    if majority_n * 4 < run.len() * 3 || majority_n == run.len() {
        return; // majority under 75%, or no deviants at all
    }
    let deviants: Vec<&&CellShape> =
        run.iter().filter(|c| c.shapes.full != majority_shape).collect();
    if deviants.len() > 2 {
        return;
    }
    let exemplar = run.iter().find(|c| c.shapes.full == majority_shape).unwrap();
    let first = run.first().unwrap();
    let last = run.last().unwrap();
    for d in deviants {
        // Interior only: run edges are routinely intentional.
        let pos = if horizontal { d.col } else { d.row };
        let (lo, hi) = if horizontal { (first.col, last.col) } else { (first.row, last.row) };
        if pos == lo || pos == hi {
            continue;
        }
        // The v1 corpus audit measured ~25-30% precision without the next
        // three guards (docs/precision/inconsistent-region-v1.md). What
        // ships is the SLIPPED-REFERENCE detector:
        // (a) identical structure — different operators/functions mean a
        //     deliberate subtotal, plug, or override, not a bad copy;
        if d.shapes.structural != exemplar.shapes.structural
            || d.shapes.ranges.len() != exemplar.shapes.ranges.len()
        {
            continue;
        }
        // (b) no relative<->absolute flips — pinning a seed cell with $
        //     anchors is an idiom, not a slip;
        let kind_flip = d
            .shapes
            .ranges
            .iter()
            .zip(&exemplar.shapes.ranges)
            .any(|(ra, rb)| {
                ra != rb
                    && (ra.r0.diff(rb.r0).is_none()
                        || ra.c0.diff(rb.c0).is_none()
                        || ra.r1.diff(rb.r1).is_none()
                        || ra.c1.diff(rb.c1).is_none())
            });
        if kind_flip {
            continue;
        }
        let run_desc = format!(
            "{}:{}",
            cell_a1(first.row, first.col),
            cell_a1(last.row, last.col)
        );
        // Off-by-one refinement: identical structure, exactly one range
        // boundary moved by exactly one row/column. Checked BEFORE the
        // uniqueness guard: a bad formula drag-copied to sibling cells
        // repeats its shape, and the boundary evidence stands on its own
        // (corpus audit: 34/34 tp including repeated slips).
        if let Some(proof) = off_by_one_proof(sheet, d, exemplar, &run_desc, run.len()) {
            findings.push((
                Finding {
                    detector: "range-off-by-one".into(),
                    sheet: sheet.to_string(),
                    cell: cell_a1(d.row, d.col),
                    formula: d.formula.clone(),
                    proof,
                },
                horizontal,
                if horizontal { d.col } else { d.row },
            ));
        } else if shape_counts.get(d.shapes.full.as_str()).copied().unwrap_or(0) > 1 {
            // (c) generic findings only: a deviant shape that repeats on
            // this sheet is a second copied pattern (alternating-region
            // layout), not a mistake.
            continue;
        } else if orientation_flip(d, exemplar) {
            // (d) a range that flips from row-vector to column-vector is
            // the embedded-subtotal idiom (R40 totals R27:R39 inside a
            // family of row sums), not a slip.
            continue;
        } else if multiset_equal_ranges(d, exemplar) {
            // (e) same ranges in a different order: semantically identical
            // for symmetric functions (SUM(C3,C4) vs SUM(C4,C3)) — noise.
            continue;
        } else {
            findings.push((
                Finding {
                detector: "inconsistent-region".into(),
                sheet: sheet.to_string(),
                cell: cell_a1(d.row, d.col),
                formula: d.formula.clone(),
                proof: format!(
                    "{sheet}!{run_desc}: {} of {} cells share the copied formula pattern (e.g. {} = {}), but {} = {}",
                    run.len() - 1,
                    run.len(),
                    cell_a1(exemplar.row, exemplar.col),
                    exemplar.formula,
                    cell_a1(d.row, d.col),
                    d.formula
                ),
                },
                horizontal,
                if horizontal { d.col } else { d.row },
            ));
        }
    }
}

/// Did any range flip between row-vector and column-vector orientation?
fn orientation_flip(d: &CellShape, exemplar: &CellShape) -> bool {
    d.shapes.ranges.iter().zip(&exemplar.shapes.ranges).any(|(ra, rb)| {
        if ra == rb || ra.single || rb.single {
            return false;
        }
        let a_horiz = ra.r0 == ra.r1 && ra.c0 != ra.c1;
        let a_vert = ra.c0 == ra.c1 && ra.r0 != ra.r1;
        let b_horiz = rb.r0 == rb.r1 && rb.c0 != rb.c1;
        let b_vert = rb.c0 == rb.c1 && rb.r0 != rb.r1;
        (a_horiz && b_vert) || (a_vert && b_horiz)
    })
}

/// Same multiset of range descriptors in a different order?
fn multiset_equal_ranges(d: &CellShape, exemplar: &CellShape) -> bool {
    if d.shapes.ranges.len() != exemplar.shapes.ranges.len() {
        return false;
    }
    let key = |r: &crate::shape::RangeDesc| format!("{r:?}");
    let mut a: Vec<String> = d.shapes.ranges.iter().map(key).collect();
    let mut b: Vec<String> = exemplar.shapes.ranges.iter().map(key).collect();
    if a == b {
        return false; // identical order — the difference lies elsewhere
    }
    a.sort();
    b.sort();
    a == b
}

fn off_by_one_proof(
    sheet: &str,
    deviant: &CellShape,
    exemplar: &CellShape,
    run_desc: &str,
    run_len: usize,
) -> Option<String> {
    let a = &deviant.shapes;
    let b = &exemplar.shapes;
    if a.structural != b.structural || a.ranges.len() != b.ranges.len() {
        return None;
    }
    let mut diffs = 0usize;
    let mut detail = String::new();
    for (ra, rb) in a.ranges.iter().zip(&b.ranges) {
        if ra == rb {
            continue;
        }
        // Exactly one boundary, off by exactly one.
        let boundary_diffs: Vec<i64> = [
            ra.r0.diff(rb.r0),
            ra.c0.diff(rb.c0),
            ra.r1.diff(rb.r1),
            ra.c1.diff(rb.c1),
        ]
        .into_iter()
        .map(|d| d.unwrap_or(i64::MAX))
        .filter(|&d| d != 0)
        .collect();
        if boundary_diffs.len() != 1 || boundary_diffs[0].abs() != 1 {
            return None;
        }
        diffs += 1;
        let mut da = String::new();
        ra.print(&mut da);
        let mut db = String::new();
        rb.print(&mut db);
        detail = format!("its range spans {da} where siblings span {db}");
    }
    if diffs != 1 {
        return None;
    }
    Some(format!(
        "{sheet}!{run_desc}: {} of {run_len} cells use the pattern of {} = {}; {} = {} — {detail} (one boundary short/long by one)",
        run_len - 1,
        cell_a1(exemplar.row, exemplar.col),
        exemplar.formula,
        cell_a1(deviant.row, deviant.col),
        deviant.formula
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use xlc_eval::workbook::Workbook;

    fn wb_with(formulas: &[(u32, u32, &str)]) -> Workbook {
        let mut wb = Workbook::default();
        let id = wb.add_sheet("S");
        for &(r, c, f) in formulas {
            wb.set_formula(id, r, c, f.to_string());
        }
        wb
    }

    #[test]
    fn detects_interior_deviant_in_row_run() {
        // Row 9: C..J sum their columns; G sums one row short.
        let mut cells = Vec::new();
        for col in 2..10u32 {
            let letter = xlc_parse::ast::col_letters(col);
            let f = if col == 6 {
                format!("SUM({letter}1:{letter}7)")
            } else {
                format!("SUM({letter}1:{letter}8)")
            };
            cells.push((9u32, col, f));
        }
        let owned: Vec<(u32, u32, String)> = cells;
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        let wb = wb_with(&refs);
        let fs = analyze(&wb);
        assert_eq!(fs.len(), 1, "{fs:?}");
        assert_eq!(fs[0].detector, "range-off-by-one");
        assert_eq!(fs[0].cell, "G10");
        assert!(fs[0].proof.contains("one boundary short/long by one"), "{}", fs[0].proof);
    }

    #[test]
    fn edge_deviants_are_not_flagged() {
        // Same as above but the deviant is the last cell — intentional
        // variation territory, stays silent.
        let mut owned = Vec::new();
        for col in 2..10u32 {
            let letter = xlc_parse::ast::col_letters(col);
            let f = if col == 9 {
                format!("SUM({letter}1:{letter}7)")
            } else {
                format!("SUM({letter}1:{letter}8)")
            };
            owned.push((9u32, col, f));
        }
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        assert!(analyze(&wb_with(&refs)).is_empty());
    }

    #[test]
    fn changed_constant_is_suppressed_as_deliberate() {
        // Column B rows 1..8: A*2 copied down, one interior cell uses A*3.
        // A different constant is a plug/override, not a slipped copy —
        // the v1 corpus audit priced this class at ~25-30% precision.
        let mut owned = Vec::new();
        for row in 0..8u32 {
            let f = if row == 3 {
                format!("A{}*3", row + 1)
            } else {
                format!("A{}*2", row + 1)
            };
            owned.push((row, 1u32, f));
        }
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        assert!(analyze(&wb_with(&refs)).is_empty());
    }

    #[test]
    fn slipped_reference_is_flagged() {
        // Column C rows 1..8: A{r}*2 copied down, one interior cell reads
        // column B instead — same structure, slipped reference.
        let mut owned = Vec::new();
        for row in 0..8u32 {
            let f = if row == 3 {
                format!("B{}*2", row + 1)
            } else {
                format!("A{}*2", row + 1)
            };
            owned.push((row, 2u32, f));
        }
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        let fs = analyze(&wb_with(&refs));
        assert_eq!(fs.len(), 1, "{fs:?}");
        assert_eq!(fs[0].cell, "C4");
        // A single-cell ref slip moves both boundaries of the degenerate
        // range, so it reports as inconsistent-region, not off-by-one.
        assert_eq!(fs[0].detector, "inconsistent-region");
    }

    #[test]
    fn alternating_pattern_is_suppressed() {
        // Two interleaved copied patterns: the minority repeats on the
        // sheet, so it is a layout, not a mistake.
        let mut owned = Vec::new();
        for row in 0..12u32 {
            let f = if row == 3 || row == 9 {
                format!("B{}*2", row + 1)
            } else {
                format!("A{}*2", row + 1)
            };
            owned.push((row, 2u32, f));
        }
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        assert!(analyze(&wb_with(&refs)).is_empty());
    }

    #[test]
    fn anchor_flip_is_suppressed_as_seed_idiom() {
        // Family D{r}=C{r}*2 with one cell pinned to $C$1 — anchoring is
        // an idiom, not a slip.
        let mut owned = Vec::new();
        for row in 0..8u32 {
            let f = if row == 3 {
                "$C$1*2".to_string()
            } else {
                format!("C{}*2", row + 1)
            };
            owned.push((row, 3u32, f));
        }
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        assert!(analyze(&wb_with(&refs)).is_empty());
    }

    #[test]
    fn consistent_runs_are_silent() {
        let mut owned = Vec::new();
        for row in 0..20u32 {
            owned.push((row, 1u32, format!("A{}*2", row + 1)));
        }
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        assert!(analyze(&wb_with(&refs)).is_empty());
    }

    #[test]
    fn ref_error_detected() {
        let wb = wb_with(&[(0, 0, "SUM(#REF!)")]);
        let fs = analyze(&wb);
        assert_eq!(fs.len(), 1);
        assert_eq!(fs[0].detector, "ref-error");
        assert!(fs[0].proof.contains("#REF!"));
    }

    #[test]
    fn short_runs_are_silent() {
        // 5 cells with a deviant: below the run-length floor.
        let mut owned = Vec::new();
        for col in 2..7u32 {
            let letter = xlc_parse::ast::col_letters(col);
            let f = if col == 4 {
                format!("SUM({letter}1:{letter}7)")
            } else {
                format!("SUM({letter}1:{letter}8)")
            };
            owned.push((9u32, col, f));
        }
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        assert!(analyze(&wb_with(&refs)).is_empty());
    }
}
