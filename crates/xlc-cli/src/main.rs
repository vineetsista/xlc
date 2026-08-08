//! The `xlc` binary. Phase 0 ships only the corpus tooling
//! (`xlc corpus-filter`, `xlc corpus-subset`); product verbs land in Phase 8.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some(cmd) => {
            eprintln!("xlc: unknown or not-yet-implemented command `{cmd}`");
            std::process::exit(2);
        }
        None => {
            eprintln!("xlc — an optimizing compiler for Excel (Phase 0 scaffold)");
            eprintln!("usage: xlc <command> [args]");
            std::process::exit(2);
        }
    }
}
