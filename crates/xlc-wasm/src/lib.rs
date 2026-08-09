//! wasm surface: the entire toolchain in the browser tab (Law 1 — nothing
//! ever leaves the machine).
//!
//! `analyze(bytes)` is the stateless one-shot. `Session` keeps the
//! ingested workbook and a prepared scenario engine alive across calls,
//! which is what makes the what-if slider instant: prepare() builds the
//! cone + schedule once; what_if() re-evaluates only that cone.

use wasm_bindgen::prelude::*;

use xlc_eval::workbook::Workbook;
use xlc_eval::Value;
use xlc_scenario::dist::Dist;
use xlc_scenario::engine::{auto_inputs, CellKey, Engine, ScenarioSpec};

#[wasm_bindgen]
pub fn analyze(bytes: &[u8]) -> String {
    match std::panic::catch_unwind(|| match xlc_ingest::ingest_bytes(bytes) {
        Ok(wb) => analyze_wb(&wb),
        Err(e) => serde_json::json!({"ok": false, "error": format!("could not read workbook: {e}")})
            .to_string(),
    }) {
        Ok(json) => json,
        Err(_) => serde_json::json!({
            "ok": false,
            "error": "internal panic while analyzing — please report this workbook"
        })
        .to_string(),
    }
}

fn analyze_wb(wb: &Workbook) -> String {
    let formula_cells: usize = wb.sheets.iter().map(|s| s.formulas.len()).sum();
    let receipt = xlc_ingest::run_receipt(wb, |_| {});
    let verifiable = receipt.verifiable();
    let rate = if verifiable > 0 {
        receipt.pass as f64 / verifiable as f64
    } else {
        1.0
    };
    let findings = xlc_lint::analyze(wb);
    let capability = xlc_ingest::capability_report(wb);
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

fn cell_name(wb: &Workbook, k: CellKey) -> String {
    format!(
        "{}!{}{}",
        wb.sheets[k.0 as usize].name,
        xlc_parse::ast::col_letters(k.2),
        k.1 + 1
    )
}

fn to_num(v: &Value) -> Option<f64> {
    match v {
        Value::Num(x) => Some(*x),
        _ => None,
    }
}

#[wasm_bindgen]
pub struct Session {
    // SAFETY invariant: `engine` borrows `*wb` with a transmuted 'static
    // lifetime. `wb` is boxed (stable heap address, never moved out) and
    // `engine` is declared FIRST so it drops before `wb`. All methods take
    // &self/&mut self, so the borrow never escapes the Session.
    engine: Option<Engine<'static>>,
    input: Option<CellKey>,
    watch: Option<CellKey>,
    wb: Box<Workbook>,
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<Session, JsValue> {
        let wb = xlc_ingest::ingest_bytes(bytes).map_err(|e| JsValue::from_str(&e))?;
        Ok(Session {
            engine: None,
            input: None,
            watch: None,
            wb: Box::new(wb),
        })
    }

    pub fn analyze(&self) -> String {
        analyze_wb(&self.wb)
    }

    /// Top-k uncertain-input candidates, ranked by how many formulas
    /// depend on them.
    pub fn input_candidates(&self, k: usize) -> String {
        let cands = auto_inputs(&self.wb, k);
        serde_json::json!(cands
            .iter()
            .map(|&(key, value, impact)| serde_json::json!({
                "sheet": key.0, "row": key.1, "col": key.2,
                "name": cell_name(&self.wb, key),
                "value": value,
                "impact": impact,
            }))
            .collect::<Vec<_>>())
        .to_string()
    }

    fn wb_static(&self) -> &'static Workbook {
        // SAFETY: see struct invariant.
        unsafe { &*(self.wb.as_ref() as *const Workbook) }
    }

    fn rebuild(&mut self, dist: Dist) -> Option<usize> {
        let input = self.input?;
        self.engine = None; // drop the old borrow first
        let engine = Engine::new(
            self.wb_static(),
            ScenarioSpec {
                seed: 42,
                inputs: vec![(input, dist)],
            },
        );
        let cone = engine.cone_cells();
        if self.watch.is_none() || !engine.cone_keys().contains(&self.watch.unwrap()) {
            self.watch = engine.cone_keys().last().copied();
        }
        self.engine = Some(engine);
        Some(cone)
    }

    /// Build the cone + schedule for one input cell. Returns cone size,
    /// the default watched output, and up to 12 watchable sinks.
    pub fn prepare(&mut self, sheet: u32, row: u32, col: u32) -> String {
        let key = (sheet, row, col);
        let current = match self.wb.sheets.get(sheet as usize).and_then(|s| s.values.get(&(row, col))) {
            Some(Value::Num(x)) => *x,
            _ => return r#"{"ok":false,"error":"input cell is not a static number"}"#.into(),
        };
        self.input = Some(key);
        self.watch = None;
        let Some(cone) = self.rebuild(Dist::Point { value: current }) else {
            return r#"{"ok":false,"error":"no input prepared"}"#.into();
        };
        if cone == 0 {
            return r#"{"ok":false,"error":"no formula depends on that cell"}"#.into();
        }
        let engine = self.engine.as_ref().unwrap();
        let keys = engine.cone_keys();
        let sinks: Vec<_> = keys
            .iter()
            .rev()
            .take(12)
            .map(|&k| serde_json::json!({
                "sheet": k.0, "row": k.1, "col": k.2,
                "name": cell_name(&self.wb, k),
            }))
            .collect();
        serde_json::json!({
            "ok": true,
            "cone_cells": cone,
            "current": current,
            "watch": self.watch.map(|w| cell_name(&self.wb, w)),
            "sinks": sinks,
        })
        .to_string()
    }

    pub fn set_watch(&mut self, sheet: u32, row: u32, col: u32) {
        self.watch = Some((sheet, row, col));
    }

    /// One instant recompute of the prepared cone at an explicit value.
    pub fn what_if(&self, value: f64) -> String {
        let (Some(engine), Some(watch)) = (&self.engine, self.watch) else {
            return r#"{"ok":false,"error":"prepare() first"}"#.into();
        };
        let out = engine.eval_with_inputs(&[value], &[watch]);
        match out.get(&watch) {
            Some(Value::Num(x)) => serde_json::json!({"ok": true, "output": x}).to_string(),
            Some(v) => serde_json::json!({"ok": true, "output_text": format!("{v:?}")}).to_string(),
            None => r#"{"ok":false,"error":"watch cell unreachable"}"#.into(),
        }
    }

    /// The response curve: output over `steps` values across [lo, hi].
    pub fn sweep(&self, lo: f64, hi: f64, steps: usize) -> String {
        let (Some(engine), Some(watch)) = (&self.engine, self.watch) else {
            return "[]".into();
        };
        let steps = steps.clamp(2, 201);
        let mut pts = Vec::with_capacity(steps);
        for i in 0..steps {
            let x = lo + (hi - lo) * i as f64 / (steps - 1) as f64;
            let out = engine.eval_with_inputs(&[x], &[watch]);
            if let Some(y) = out.get(&watch).and_then(to_num) {
                pts.push(serde_json::json!([x, y]));
            }
        }
        serde_json::json!(pts).to_string()
    }

    /// Monte-Carlo over the prepared input. Returns stats + 40 histogram
    /// bins for the watched output. Deterministic: seed 42, scenario k
    /// derivable from (seed, k).
    pub fn monte(&mut self, kind: &str, p1: f64, p2: f64, p3: f64, n: u32) -> String {
        let Some(_) = self.input else {
            return r#"{"ok":false,"error":"prepare() first"}"#.into();
        };
        let dist = match kind {
            "normal" => Dist::Normal { mean: p1, sd: p2.max(1e-12) },
            "uniform" => Dist::Uniform { a: p1.min(p2), b: p2.max(p1 + 1e-12) },
            "triangular" => Dist::Triangular { a: p1, m: p2, b: p3 },
            "pert" => Dist::Pert { a: p1, m: p2, b: p3 },
            "lognormal" => Dist::LogNormal { mu: p1, sigma: p2.max(1e-12) },
            _ => return r#"{"ok":false,"error":"unknown distribution"}"#.into(),
        };
        if self.rebuild(dist).is_none() {
            return r#"{"ok":false,"error":"prepare() first"}"#.into();
        }
        let (engine, watch) = (self.engine.as_ref().unwrap(), self.watch.unwrap());
        let n = n.clamp(100, 200_000);
        let res = engine.run(n, 1024, &[watch]);
        let mut xs: Vec<f64> = res.watched[&watch].iter().filter_map(to_num).collect();
        if xs.is_empty() {
            return r#"{"ok":false,"error":"output is non-numeric across scenarios"}"#.into();
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        let var = xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / xs.len() as f64;
        let q = |p: f64| xs[(((xs.len() - 1) as f64) * p) as usize];
        let (lo, hi) = (xs[0], xs[xs.len() - 1]);
        let nbins = 40usize;
        let width = ((hi - lo) / nbins as f64).max(1e-300);
        let mut bins = vec![0u32; nbins];
        for &x in &xs {
            let b = (((x - lo) / width) as usize).min(nbins - 1);
            bins[b] += 1;
        }
        serde_json::json!({
            "ok": true,
            "n": xs.len(),
            "cone_cells": engine.cone_cells(),
            "mean": mean, "sd": var.sqrt(),
            "p5": q(0.05), "p50": q(0.50), "p95": q(0.95),
            "min": lo, "max": hi,
            "bins": bins,
            "bin_lo": lo, "bin_width": width,
            "seed": 42,
        })
        .to_string()
    }
}

/// Semantic diff of two workbook versions over identical scenario tiles.
/// Inputs: common static numeric cells perturbed ±10%; output: the
/// deepest cone sink of version A.
#[wasm_bindgen]
pub fn diff_books(a: &[u8], b: &[u8], n: u32) -> String {
    let inner = || -> Result<String, String> {
        let wa = xlc_ingest::ingest_bytes(a)?;
        let wbk = xlc_ingest::ingest_bytes(b)?;
        let mut inputs = Vec::new();
        for (sid, sheet) in wa.sheets.iter().enumerate() {
            for (&(r, c), v) in &sheet.values {
                if sheet.formulas.contains_key(&(r, c)) {
                    continue;
                }
                if let Value::Num(x) = v {
                    let same = wbk
                        .sheets
                        .get(sid)
                        .and_then(|s2| s2.values.get(&(r, c)))
                        .is_some_and(|v2| matches!(v2, Value::Num(y) if y == x));
                    if same {
                        inputs.push((
                            (sid as u32, r, c),
                            Dist::Normal { mean: *x, sd: x.abs() * 0.1 + 1e-9 },
                        ));
                    }
                }
            }
        }
        inputs.sort_by_key(|(k, _)| *k);
        inputs.truncate(64);
        if inputs.is_empty() {
            return Err("no common numeric inputs between the two versions".into());
        }
        let spec = ScenarioSpec { seed: 2026, inputs };
        let ea = Engine::new(&wa, spec.clone());
        let Some(&output) = ea.cone_keys().last() else {
            return Err("no formula depends on the common inputs".into());
        };
        let n = n.clamp(100, 50_000);
        let rep = xlc_diff::diff_output(&wa, &wbk, &spec, output, n);
        Ok(serde_json::json!({
            "ok": true,
            "output": cell_name(&wa, output),
            "scenarios": rep.scenarios,
            "divergent": rep.divergent,
            "witness": rep.witness.map(|w| serde_json::json!({
                "scenario": w.scenario,
                "inputs": w.inputs.iter().map(|(k, v)| serde_json::json!({
                    "cell": cell_name(&wa, *k), "value": v,
                })).collect::<Vec<_>>(),
                "v1": format!("{:?}", w.v1_output),
                "v2": format!("{:?}", w.v2_output),
            })),
        })
        .to_string())
    };
    match inner() {
        Ok(s) => s,
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}
