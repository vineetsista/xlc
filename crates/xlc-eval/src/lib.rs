//! Scalar interpreter + Excel semantics. THE SPINE (§8.4).

pub mod dates;
pub mod interp;
pub mod value;

pub use value::{ExcelError, Value};
