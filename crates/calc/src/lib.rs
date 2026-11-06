//! GridCraft's calculation engine: the evaluator (references, names, tables, LET/LAMBDA,
//! dynamic arrays) and dependency-driven recalculation with spills and cycle detection.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod eval;
mod precision;
pub mod recalc;
mod trimrange;

pub use eval::{Area, SPECIAL_FUNCTIONS, is_known_function};
pub use precision::DisplayedPrecision;
pub use recalc::{Calc, Key, evaluate, now_serial};

#[cfg(test)]
mod groupby_auto_headers_tests;
#[cfg(test)]
mod if_omitted_branch_tests;
#[cfg(test)]
mod indirect_ref_array_tests;
#[cfg(test)]
mod indirect_sheet_name_tests;
#[cfg(test)]
mod lookup_database_parity_tests;
#[cfg(test)]
mod subtotal_counta_error_tests;
#[cfg(test)]
mod tests;
