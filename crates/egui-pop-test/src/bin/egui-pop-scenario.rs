//! Runs scenarios against a running application over its agent socket
//! (`egui_pop_test::scenario::live`, `docs/scenarios.md`).
//!
//! ```text
//! egui-pop-scenario --socket PATH [--out DIR] [--var NAME=VALUE]...
//!                   [--shot CAPABILITY ARGS_JSON] [--deadline-ms N]
//!                   [--connect-ms N] FILE_OR_DIR...
//! ```
//!
//! One JSON line per scenario on stdout (`{"scenario", "passed", ...}`);
//! with `--out`, each scenario's journal as `<scenario>-journal.json` and
//! a failure's report as `<scenario>-failure.txt`. Exits 1 when one fails.

#[cfg(unix)]
fn main() {
    std::process::exit(unix::main());
}

#[cfg(not(unix))]
fn main() {
    eprintln!("egui-pop-scenario: live runs need a Unix socket (not yet on this system)");
    std::process::exit(2);
}

#[cfg(unix)]
mod unix {
    pub fn main() -> i32 {
        egui_pop_test::scenario::live::cli::main(std::env::args().skip(1).collect())
    }
}
