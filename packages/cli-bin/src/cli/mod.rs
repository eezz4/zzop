//! CLI argv-dispatch helpers shared by the `zzop` binary's subcommand match (`src/main.rs`) — kept out of
//! the binary entry so it stays a thin dispatch table. Split by RESPONSIBILITY, not by size, so a change
//! lands in exactly one file:
//!
//! - [`args`] — argv SHAPE: the parsers and rejections (`reject_flag_like_args`, `parse_trees_args`,
//!   `extract_finding_filters`). Everything that answers "is this argument list well-formed?" and exits 2.
//! - [`help`] — help OUTPUT: the per-subcommand elaboration table, printed whole by `zzop help` and one
//!   row at a time by `zzop <sub> --help`.
//! - [`analysis`] — the four lanes that run an engine ANALYSIS from argv (`analyze`, `analyze-envelope`,
//!   `cross`, `endpoint`): each carries a source-mode choice and/or the findings-view knobs.
//! - [`run`] — the remaining subcommand RUNNERS whose argv parsing is big enough to deserve a function of
//!   their own; each diverges (parse, call `zzop_summary`, print, exit).
//!
//! This module keeps only the two terminal steps both halves need ([`read_or_exit`], [`print_or_exit`])
//! and the re-exports `main.rs` imports. The exit-code contract every `run_*` carries: 2 = argument-shape
//! error, 1 = runtime failure (unreadable file / invalid / refused).

pub mod analysis;
pub mod args;
pub mod help;
pub mod run;

pub use args::{parse_trees_args, reject_flag_like_args};
pub use help::print_help;
pub use run::{run_diff, run_file_validate, run_graph, run_init, run_lookup};

/// Reads a file argument or exits 1 (a runtime failure, never a usage error — the argument was
/// well-formed, the file just isn't readable). Shared by every file-taking subcommand.
pub fn read_or_exit(path: &str) -> String {
    match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("zzop: failed to read {path}: {e}");
            std::process::exit(1);
        }
    }
}

/// The shared terminal step: `Ok` to stdout + exit 0, `Err` as `zzop: <message>` to stderr + exit 1.
pub fn print_or_exit(result: Result<String, String>) -> ! {
    match result {
        Ok(text) => {
            println!("{text}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("zzop: {e}");
            std::process::exit(1);
        }
    }
}
