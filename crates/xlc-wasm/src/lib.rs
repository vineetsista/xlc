//! wasm-bindgen surface: the entire analysis — ingest, receipt, detectors,
//! capability report — runs in the browser. Nothing leaves the machine
//! (Law 1); the page hands this module bytes and gets JSON back.

use wasm_bindgen::prelude::*;

/// Analyze a workbook from raw .xlsx bytes. Returns a JSON string:
/// { ok, error?, sheets, formula_cells, receipt: { cells, pass, verifiable,
///   rate, ulp1, sig15, excluded{}, mismatches{}, no_cached },
///   findings: [{detector, sheet, cell, formula, proof}],
///   capability: {feature: excluded_cells} }
#[wasm_bindgen]
pub fn analyze(bytes: &[u8]) -> String {
    let result = std::panic::catch_unwind(|| analyze_inner(bytes));
    match result {
        Ok(json) => json,
        Err(_) => serde_json::json!({
            "ok": false,
            "error": "internal panic while analyzing — please report this workbook"
        })
        .to_string(),
    }
}

fn analyze_inner(bytes: &[u8]) -> String {
    let wb = match xlc_ingest::ingest_bytes(bytes) {
        Ok(wb) => wb,
        Err(e) => {
            return serde_json::json!({
                "ok": false,
                "error": format!("could not read workbook: {e}")
            })
            .to_string()
        }
    };
    let formula_cells: usize = wb.sheets.iter().map(|s| s.formulas.len()).sum();
    let receipt = xlc_ingest::run_receipt(&wb, |_| {});
    let verifiable = receipt.verifiable();
    let rate = if verifiable > 0 {
        receipt.pass as f64 / verifiable as f64
    } else {
        1.0
    };
    let findings = xlc_lint::analyze(&wb);
    let capability = xlc_ingest::capability_report(&wb);

    serde_json::json!({
        "ok": true,
        "sheets": wb.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "formula_cells": formula_cells,
        "receipt": {
            "cells": receipt.cells,
            "pass": receipt.pass,
            "verifiable": verifiable,
            "rate": rate,
            "ulp1": receipt.ulp1,
            "sig15": receipt.sig15,
            "excluded": receipt.excluded,
            "mismatches": receipt.mismatches,
            "no_cached": receipt.no_cached,
        },
        "findings": findings,
        "capability": capability,
        "ulp_policy": "pass iff bit-identical, within 1 ULP, or equal at 15 significant decimal digits",
    })
    .to_string()
}
