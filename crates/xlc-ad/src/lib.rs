//! Reverse-mode automatic differentiation (§8.7). The implementation
//! lives with the scenario engine (the topologically-ordered cone is the
//! tape); this crate is the stable public surface.

pub use xlc_scenario::ad::{Gradient, Source};
