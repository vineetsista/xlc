//! The `xlc` binary. Phase 0 ships the corpus tooling
//! (`corpus-filter`, `corpus-subset`, `corpus-verify`); product verbs land in
//! Phase 8. The corpus tools deliberately go through the same calamine ingest
//! path the compiler uses, so every corpus run is an integration test of §8.1.

mod census;
mod check;
mod ir_verify;
mod parse_corpus;
mod receipt;
mod corpus;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("corpus-filter") => corpus::filter_cmd(&args[1..]),
        Some("corpus-subset") => corpus::subset_cmd(&args[1..]),
        Some("corpus-verify") => corpus::verify_cmd(&args[1..]),
        Some("census") => census::census_cmd(&args[1..]),
        Some("parse-corpus") => parse_corpus::parse_corpus_cmd(&args[1..]),
        Some("receipt") => receipt::receipt_cmd(&args[1..]),
        Some("check") => check::check_cmd(&args[1..]),
        Some("lint-corpus") => check::lint_corpus_cmd(&args[1..]),
        Some("ir-verify") => ir_verify::ir_verify_cmd(&args[1..]),
        Some(cmd) => {
            eprintln!("xlc: unknown or not-yet-implemented command `{cmd}`");
            2
        }
        None => {
            eprintln!("xlc — an optimizing compiler for Excel");
            eprintln!("usage: xlc <corpus-filter|corpus-subset|corpus-verify> [args]");
            2
        }
    };
    std::process::exit(code);
}
