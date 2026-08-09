//! Reverse-mode AD over the scenario engine's schedule (§8.7).
//!
//! The topologically-ordered cone IS the tape: one forward pass records,
//! per cone cell, its local derivatives with respect to its numeric
//! sources (other cone cells or uncertain inputs); one backward sweep
//! yields ∂output/∂every-input at ~2x a forward evaluation.
//!
//! Honesty about non-smoothness: fast-path arithmetic and SUM are exact.
//! Cells evaluated through opaque calls (IF, lookups, everything else)
//! are STRUCTURAL boundaries in v1 — the gradient does not flow through
//! them and they are reported as such, never silently zeroed.

use std::collections::HashMap;

use crate::engine::{CellKey, Engine};
use crate::rng::DrawAddr;
use xlc_eval::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Cone(usize),
    Input(usize),
}

pub struct Gradient {
    /// ∂output/∂input for each spec input, by input index.
    pub d_inputs: Vec<f64>,
    /// Cone cells whose formulas were opaque to AD (structural).
    pub structural_cells: Vec<CellKey>,
    pub output_value: f64,
}

impl Engine<'_> {
    /// Gradient of `output` at scenario k. Fails (None) when the output
    /// itself is unreachable or non-numeric at this scenario.
    pub fn gradient(&self, output: CellKey, k: u32) -> Option<Gradient> {
        // Forward: evaluate every cone cell at scenario k (scalar) while
        // recording local derivatives for tape-able cells.
        let inputs: Vec<f64> = self
            .spec_inputs()
            .iter()
            .enumerate()
            .map(|(i, (_, d))| {
                d.sample(DrawAddr {
                    seed: self.seed(),
                    cell: i as u32,
                    scenario: k,
                    draw: 0,
                    attempt: 0,
                })
            })
            .collect();

        let solo = self.eval_scenario(k, &self.cone_keys());
        let mut value_of: HashMap<CellKey, f64> = HashMap::new();
        for (key, v) in &solo {
            if let Value::Num(x) = v {
                value_of.insert(*key, *x);
            }
        }

        let keys = self.cone_keys();
        let out_pos = keys.iter().position(|&kk| kk == output)?;
        let output_value = *value_of.get(&output)?;

        let mut tape: Vec<Vec<(Source, f64)>> = Vec::with_capacity(keys.len());
        let mut structural: Vec<CellKey> = Vec::new();
        for (pos, &key) in keys.iter().enumerate() {
            match self.local_derivatives(pos, &inputs, &value_of) {
                Some(entries) => tape.push(entries),
                None => {
                    structural.push(key);
                    tape.push(Vec::new());
                }
            }
        }

        // Backward sweep.
        let mut adj = vec![0.0f64; keys.len()];
        let mut d_inputs = vec![0.0f64; inputs.len()];
        adj[out_pos] = 1.0;
        for pos in (0..keys.len()).rev() {
            let a = adj[pos];
            if a == 0.0 {
                continue;
            }
            for &(src, d) in &tape[pos] {
                match src {
                    Source::Cone(p) => adj[p] += a * d,
                    Source::Input(i) => d_inputs[i] += a * d,
                }
            }
        }
        Some(Gradient {
            d_inputs,
            structural_cells: structural,
            output_value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dist::Dist;
    use crate::engine::ScenarioSpec;
    use xlc_eval::workbook::Workbook;

    /// y = (a*2 + b) * b  — hand-checkable gradient.
    #[test]
    fn hand_gradient() {
        let mut wb = Workbook::default();
        let id = wb.add_sheet("S");
        wb.set_value(id, 0, 0, Value::Num(3.0)); // A1 = a
        wb.set_value(id, 1, 0, Value::Num(4.0)); // A2 = b
        wb.set_formula(id, 0, 1, "A1*2+A2".into()); // B1
        wb.set_formula(id, 1, 1, "B1*A2".into()); // B2 = (2a+b)*b
        let spec = ScenarioSpec {
            seed: 1,
            inputs: vec![
                ((0, 0, 0), Dist::Point { value: 3.0 }),
                ((0, 1, 0), Dist::Point { value: 4.0 }),
            ],
        };
        let engine = Engine::new(&wb, spec);
        let g = engine.gradient((0, 1, 1), 0).unwrap();
        // y = (2a+b)b => dy/da = 2b = 8, dy/db = 2a+2b = 14.
        assert!((g.d_inputs[0] - 8.0).abs() < 1e-12, "{:?}", g.d_inputs);
        assert!((g.d_inputs[1] - 14.0).abs() < 1e-12, "{:?}", g.d_inputs);
        assert!(g.structural_cells.is_empty());
        assert!((g.output_value - 40.0).abs() < 1e-12);
    }

    /// SUM range gradient: y = SUM(B1:B3) with B_r = A_r * r.
    #[test]
    fn sum_gradient() {
        let mut wb = Workbook::default();
        let id = wb.add_sheet("S");
        for r in 0..3u32 {
            wb.set_value(id, r, 0, Value::Num(1.0 + r as f64));
            wb.set_formula(id, r, 1, format!("A{}*{}", r + 1, r + 1));
        }
        wb.set_formula(id, 3, 1, "SUM(B1:B3)".into());
        let spec = ScenarioSpec {
            seed: 2,
            inputs: (0..3)
                .map(|r| {
                    (
                        (0u32, r, 0u32),
                        Dist::Point {
                            value: 1.0 + r as f64,
                        },
                    )
                })
                .collect(),
        };
        let engine = Engine::new(&wb, spec);
        let g = engine.gradient((0, 3, 1), 0).unwrap();
        assert_eq!(g.d_inputs, vec![1.0, 2.0, 3.0]);
    }

    /// Opaque calls are structural, not silently zero.
    #[test]
    fn opaque_is_structural() {
        let mut wb = Workbook::default();
        let id = wb.add_sheet("S");
        wb.set_value(id, 0, 0, Value::Num(2.0));
        wb.set_formula(id, 0, 1, "ROUND(A1,1)*3".into());
        let spec = ScenarioSpec {
            seed: 3,
            inputs: vec![((0, 0, 0), Dist::Point { value: 2.0 })],
        };
        let engine = Engine::new(&wb, spec);
        let g = engine.gradient((0, 0, 1), 0).unwrap();
        assert_eq!(g.structural_cells, vec![(0, 0, 1)]);
        assert_eq!(g.d_inputs, vec![0.0]);
    }
}
