//! Dependency graph, Tarjan SCC, topological schedule (§8.3).

pub mod scc;

pub use scc::{is_cyclic, schedule, tarjan_scc, Adj};
