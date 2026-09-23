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
//! - [`fail_on`] — the CI GATE (`--fail-on <severity>`): its argv lift, its per-lane refusal, and the
//!   terminal step that prints a reply and then moves the exit code with the findings. Its own file
//!   because the exit-code contract below gains a THIRD code there, and a code is a wire promise.
//! - [`run`] — the remaining subcommand RUNNERS whose argv parsing is big enough to deserve a function of
//!   their own; each diverges (parse, call `zzop_summary`, print, exit).
//!
//! This module keeps only the two terminal steps both halves need ([`read_or_exit`], [`print_or_exit`])
//! and the re-exports `main.rs` imports. The exit-code contract every `run_*` carries: 2 = argument-shape
//! error, 1 = runtime failure (unreadable file / invalid / refused), and — only on a lane asked for it
//! with `--fail-on` — 3 = the run SUCCEEDED and its findings met the declared threshold
//! ([`fail_on::FAIL_ON_EXIT`]). The third code exists so a CI log can tell a broken config apart from a
//! real critical finding; folding it into 1 would have made those two indistinguishable.

pub mod analysis;
pub mod args;
pub mod baseline;
pub mod fail_on;
pub mod help;
pub mod run;

pub use args::{parse_trees_args, reject_flag_like_args};
pub use help::print_help;
pub use run::{run_diff, run_explain, run_file_validate, run_graph, run_init, run_map};

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
/// EVERY lane's error lands here (`main::print_result` delegates), which is what makes the hint below
/// reliable — it used to live in one of two hand-kept copies of this match, and the analyze lane,
/// routed through the other, printed nothing.
pub fn print_or_exit(result: Result<String, String>) -> ! {
    match result {
        Ok(text) => {
            println!("{text}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("zzop: {e}");
            // THIS host's spelling of the way out, appended at the display layer — never inside the
            // shared string, which also reaches MCP clients that have no shell (2026-08-09 ruling;
            // the MCP host's `orientation` text carries the equivalent in ITS spelling). Matched on
            // the shared marker so it fires for exactly the missing-config refusal and not for every
            // exit-1 error; `contains` rather than `starts_with` because a lane may prefix context of
            // its own before the flattened ConfigError text.
            if e.contains(zzop_summary::contracts::MISSING_CONFIG_MARKER) {
                eprintln!("Run `zzop init` in that tree to write the starter config.");
            }
            // Same ruling, the refusal it was never applied to. A multi-tree config's refusal named
            // the join in prose and no host spelled it, so `zzop cross <that dir>` (the literal
            // transcription) exits 2 and `zzop analyze <tree root>` exits 1 — the declared roots
            // carry no config of their own. Both halves now have a runnable line.
            // The same ruling's third refusal (2026-09-23): paths mode, where one of the directories
            // carries a config that declares its own tree set. "CONFIG MODE" is a concept, not a
            // spelling — this is the spelling.
            if e.contains(zzop_summary::contracts::PATHS_MODE_CONFIG_MARKER) {
                eprintln!(
                    "Pass that config with `--config` instead of the path list — e.g. `zzop cross --config <that config>`."
                );
            }
            if e.contains(zzop_summary::contracts::MULTI_TREE_MARKER) {
                eprintln!(
                    "Run `zzop cross --config <that config>` to analyze those trees together, or `zzop analyze --config <a config declaring one tree>` for a single one."
                );
            }
            std::process::exit(1);
        }
    }
}
