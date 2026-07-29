//! The help text is DERIVED-CHECKED against the dispatch table, not maintained beside it.
//!
//! # Why this file exists
//! Two surfaces shipped in the v0.24.0..v0.25.0 window and the help text missed both, in two different
//! ways, with every mechanical guard green:
//!
//! - `zzop file` (the file-target query) reached the elaboration table but never the `USAGE` line, so
//!   `zzop help`'s first line — the one a reader scans to learn what exists — did not list it.
//! - `zzop graph --domain <join|dep|risk|posture>` reached NEITHER. Worse than absent: the graph
//!   elaboration went on describing the join domain alone, quoting the join's `--top` default as if it
//!   were the subcommand's, while three of the four domains use different ones.
//!
//! Nothing could have caught either. The surface-parity registry governs analyze/cross REPLY FIELDS;
//! `config-surface.json` governs CONFIG keys and the flags that mirror them. A subcommand's own help
//! line had no owner but a person, and a person missed it twice in one release.
//!
//! # What each test asserts, and why it reads source text
//! The dispatch table and the argv parsers are plain Rust in sibling files; there is no runtime registry
//! to enumerate. So these tests read those files as TEXT — the same route
//! `crates/engine/tests/rule_contracts/surface_parity.rs` takes, and for the same reason: an already
//! drift-coupled truth source beats a hand-copied list that becomes a third mirror.
//!
//! The parse is deliberately dumb (find `Some("<name>")`, find `"--<flag>"` literals). A dumb parse can
//! only fail by finding too little, and every test below asserts its own subject set is non-empty first —
//! so a parse that silently stops matching turns RED instead of vouching for nothing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::elaborations;

fn src(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel)
}

fn read(rel: &str) -> String {
    let p = src(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

/// Subcommand names `main.rs` actually dispatches, minus the aliases and the help lane, which are
/// answered before the table is consulted and so have no elaboration row of their own.
fn dispatched_subcommands() -> BTreeSet<String> {
    let text = read("main.rs");
    let mut out = BTreeSet::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find("Some(\"") {
        rest = &rest[i + 6..];
        let Some(end) = rest.find('"') else { break };
        let name = &rest[..end];
        // `--version` is an alias of `version`; `help`/`-h`/`--help` are the help lane itself.
        if !name.starts_with('-') && name != "help" {
            out.insert(name.to_string());
        }
    }
    out
}

/// Every `.rs` file under this crate's `src/`, relative to it, in sorted order.
///
/// # Why this is walked and not listed
/// TEST 3 below used to iterate `["cli/run.rs", "cli/analysis.rs"]` — a hand list inside the test's
/// own file, which cannot see the set of source files grow. Its green meant "the two files I
/// remembered are documented", never "the CLI is documented". That is not hypothetical here: the
/// repo's 300-line ratchet was about to force the question, with `cli/run.rs` at 224 lines and
/// `cli/analysis.rs` at 210, and a split moves runners into a file the list does not name.
/// Demonstrated 2026-07-29 by dropping a `cli/probe.rs` holding a `pub fn run_init` that parses an
/// undocumented `--zzz-probe`: all three tests passed.
///
/// # Why the whole `src/` tree rather than `cli/`
/// A runner only becomes a subject if it is a top-level `pub fn run_<sub>` AND `<sub>` matches an
/// elaboration row, so a file with no such function contributes nothing and widening costs no false
/// positives. Narrowing to `cli/` would just be a second, subtler hand rule with the same failure
/// mode one directory up: a runner relocated to `src/subcommands/` would be exempt again. The
/// contract is "a function that parses this subcommand's argv", and that has never been a statement
/// about which directory it lives in.
fn all_source_files() -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("readable dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let rel = path
                    .strip_prefix(&root)
                    .expect("walked path is under src/")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(rel);
            }
        }
    }
    out.sort();
    // Empty-enumeration floor. A walk that comes back empty prints the same "nothing missing" as a
    // fully-documented CLI, and this file's whole premise is that silence and correctness must not
    // look alike.
    assert!(
        out.len() > 3,
        "the src/ walk found {} .rs file(s) — it has stopped matching this crate's layout, so every \
         test built on it would vouch for nothing",
        out.len()
    );
    out
}

/// Every long flag literal a subcommand's argv parser compares against, keyed by the `run_*` function
/// it appears in. Flags shared by the findings-view knobs live in their own helper and are excluded —
/// the help text prints those from one `FILTER_KNOBS` block rather than per subcommand.
fn flags_by_runner(rel: &str) -> Vec<(String, BTreeSet<String>)> {
    let text = read(rel);
    let mut out: Vec<(String, BTreeSet<String>)> = Vec::new();
    for chunk in text.split("\npub fn ").skip(1) {
        let name = chunk
            .split(['(', '<'])
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        let mut flags = BTreeSet::new();
        let mut rest = chunk;
        while let Some(i) = rest.find("\"--") {
            rest = &rest[i + 1..];
            let Some(end) = rest.find('"') else { break };
            let flag = &rest[..end];
            if flag.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                flags.insert(flag.to_string());
            }
        }
        out.push((name, flags));
    }
    out
}

/// TEST 1 — a dispatched subcommand must have an elaboration row.
/// This one already held; it is here so the pair below cannot be read as the whole contract.
#[test]
fn every_dispatched_subcommand_has_an_elaboration_row() {
    let dispatched = dispatched_subcommands();
    assert!(
        dispatched.len() > 5,
        "the dispatch parse found {} subcommand(s) — it has stopped matching `main.rs`, so this test \
         would vouch for nothing",
        dispatched.len()
    );
    let described: BTreeSet<&str> = elaborations().iter().map(|(n, _)| *n).collect();
    let missing: Vec<&String> = dispatched
        .iter()
        .filter(|d| !described.contains(d.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "these subcommands are dispatched by main.rs but have no help elaboration: {missing:?}"
    );
}

/// TEST 2 — the USAGE line must name every described subcommand.
///
/// This is the one `zzop file` broke: it had an elaboration but was absent from the first line, which is
/// the only part of `zzop help` a reader skims. A subcommand that exists but is not offered is, for
/// discovery purposes, a subcommand that does not exist.
#[test]
fn the_usage_line_names_every_described_subcommand() {
    let usage = crate::USAGE;
    let described: Vec<&str> = elaborations().iter().map(|(n, _)| *n).collect();
    assert!(
        described.len() > 5,
        "elaboration table has {} rows — too few to be the real table",
        described.len()
    );
    // Word-boundary match: `analyze` must not be satisfied by `analyze-envelope`.
    let words: BTreeSet<&str> = usage
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .collect();
    let missing: Vec<&&str> = described.iter().filter(|d| !words.contains(*d)).collect();
    assert!(
        missing.is_empty(),
        "these subcommands have help elaborations but are missing from the USAGE line that `zzop help` \
         prints first: {missing:?}. Add them to `USAGE` in main.rs — a reader who cannot see a \
         subcommand offered will not go looking for its elaboration."
    );
}

/// TEST 3 — a flag a subcommand's parser accepts must appear in that subcommand's own help line.
///
/// This is the one `--domain` broke, and it is the reason this file is worth its length: the graph
/// elaboration did not merely omit the flag, it kept describing a single domain's behaviour (including
/// that domain's `--top` default) as the whole subcommand's. Absence is a gap; a stale description is a
/// wrong answer, and only a derived check tells them apart.
#[test]
fn every_flag_a_subcommand_parses_appears_in_its_help_line() {
    // The findings-view knobs are printed from one shared block for the subcommands that take them, so
    // they are not expected inside an individual elaboration string.
    const SHARED_KNOBS: &[&str] = &["--severity", "--rule", "--limit"];
    // Flags whose absence from help is deliberate, each with the reason it is not a user-facing knob.
    const NOT_A_HELP_KNOB: &[&str] = &[
        // A help REQUEST, answered before any branch parses argv (see `handle_subcommand_help`), and
        // already advertised by the trailing "(every subcommand also takes --help/-h ...)" line.
        "--help",
    ];

    let described: Vec<(&str, String)> = elaborations();
    let mut checked = 0usize;
    let mut missing: Vec<String> = Vec::new();

    for file in all_source_files() {
        for (runner, flags) in flags_by_runner(&file) {
            let Some(sub) = runner.strip_prefix("run_") else {
                continue;
            };
            // `run_file_validate`/`run_lookup` are shared helpers serving several subcommands; they take
            // their usage text as a parameter, so there is no single elaboration to check them against.
            let Some((_, text)) = described.iter().find(|(n, _)| n.replace('-', "_") == sub) else {
                continue;
            };
            for flag in &flags {
                if SHARED_KNOBS.contains(&flag.as_str()) || NOT_A_HELP_KNOB.contains(&flag.as_str())
                {
                    continue;
                }
                checked += 1;
                if !text.contains(flag.as_str()) {
                    missing.push(format!(
                        "`zzop {sub}` parses {flag} but its help line never says so"
                    ));
                }
            }
        }
    }

    assert!(
        checked > 0,
        "no subcommand-specific flag was checked — the runner/flag parse has stopped matching, so this \
         test would vouch for nothing"
    );
    assert!(
        missing.is_empty(),
        "help text is behind the argv parsers:\n  {}\nA flag that parses but is undocumented is \
         invisible; worse, a help line written for one mode of a multi-mode subcommand states that \
         mode's defaults as if they were the subcommand's.",
        missing.join("\n  ")
    );
}
