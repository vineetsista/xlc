//! Defect detectors + precision harness (§8.8). Detectors run over parsed
//! formulas, never text; every finding carries a machine-checkable proof
//! (Law 8); a detector ships only above ~90% audited precision (Law 7).

pub mod detectors;
pub mod shape;

pub use detectors::{analyze, Finding};
