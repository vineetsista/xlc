//! Scalar interpreter + Excel semantics. THE SPINE (§8.4).

pub mod criteria;
pub mod dates;
mod funcs_range;
mod funcs_scalar;
pub mod interp;
pub mod workbook;
pub mod value;

pub use value::{ExcelError, Value};
