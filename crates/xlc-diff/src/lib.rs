//! Semantic workbook diff (§8.9): run both versions' engines over the
//! SAME scenario tiles (same seed, same input distributions) and report
//! where the models disagree — not "4,000 cells changed" but "on X% of
//! the sampled input space the outputs diverge; here is an input vector
//! where v1 = a and v2 = b".

use xlc_eval::workbook::Workbook;
use xlc_eval::Value;
use xlc_scenario::engine::{CellKey, Engine, ScenarioSpec};
use xlc_scenario::rng::DrawAddr;

pub struct DiffReport {
    pub scenarios: u32,
    pub divergent: u32,
    /// First divergent scenario with both outputs and the input vector.
    pub witness: Option<Witness>,
}

pub struct Witness {
    pub scenario: u32,
    pub inputs: Vec<(CellKey, f64)>,
    pub v1_output: Value,
    pub v2_output: Value,
}

/// Compare one watched output across N scenarios. The spec (seed +
/// distributions) must be identical for both versions — same tiles,
/// same draws, honest comparison.
pub fn diff_output(
    wb1: &Workbook,
    wb2: &Workbook,
    spec: &ScenarioSpec,
    output: CellKey,
    scenarios: u32,
) -> DiffReport {
    let e1 = Engine::new(wb1, spec.clone());
    let e2 = Engine::new(wb2, spec.clone());
    let r1 = e1.run(scenarios, 1024, &[output]);
    let r2 = e2.run(scenarios, 1024, &[output]);
    let v1 = &r1.watched[&output];
    let v2 = &r2.watched[&output];

    let mut divergent = 0u32;
    let mut witness = None;
    for k in 0..scenarios {
        let (a, b) = (&v1[k as usize], &v2[k as usize]);
        if !xlc_ir::bit_equal(a, b) {
            divergent += 1;
            if witness.is_none() {
                let inputs = spec
                    .inputs
                    .iter()
                    .enumerate()
                    .map(|(i, (key, d))| {
                        (
                            *key,
                            d.sample(DrawAddr {
                                seed: spec.seed,
                                cell: i as u32,
                                scenario: k,
                                draw: 0,
                                attempt: 0,
                            }),
                        )
                    })
                    .collect();
                witness = Some(Witness {
                    scenario: k,
                    inputs,
                    v1_output: a.clone(),
                    v2_output: b.clone(),
                });
            }
        }
    }
    DiffReport { scenarios, divergent, witness }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xlc_scenario::dist::Dist;

    #[test]
    fn planted_change_yields_witness() {
        let build = |coef: f64| {
            let mut wb = Workbook::default();
            let id = wb.add_sheet("S");
            for r in 0..20u32 {
                wb.set_value(id, r, 0, Value::Num(r as f64 + 1.0));
                let f = if r == 0 {
                    format!("A1*{coef}")
                } else {
                    format!("A{}*{coef}+B{}", r + 1, r)
                };
                wb.set_formula(id, r, 1, f);
            }
            wb
        };
        let wb1 = build(1.10);
        let wb2 = build(1.11); // the planted change
        let spec = ScenarioSpec {
            seed: 99,
            inputs: vec![((0, 0, 0), Dist::Normal { mean: 10.0, sd: 2.0 })],
        };
        let rep = diff_output(&wb1, &wb2, &spec, (0, 19, 1), 2000);
        assert_eq!(rep.divergent, 2000, "every scenario diverges");
        let w = rep.witness.unwrap();
        assert_ne!(w.v1_output, w.v2_output);
        assert_eq!(w.inputs.len(), 1);

        // Identical versions: zero divergence.
        let rep2 = diff_output(&wb1, &build(1.10), &spec, (0, 19, 1), 500);
        assert_eq!(rep2.divergent, 0);
        assert!(rep2.witness.is_none());
    }
}
