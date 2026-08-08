//! Scenario axis (§8.6): counter-based RNG, five distributions, and the
//! vectorized engine over the coarsened IR.

pub mod dist;
pub mod engine;
pub mod rng;

pub use dist::Dist;
pub use engine::{Engine, ScenarioSpec};
