//! Excel expression grammar → AST. Pure, no I/O (§8.2).
//!
//! Round-trip contract: `parse(s).print() == s` for ≥99.5% of real-world
//! formula text (Gate 2). Numeric lexemes, explicit parens, and reference
//! spellings are preserved to make that possible.

pub mod ast;
pub mod lexer;
