//! The ceiling half of the parse census — its own test BINARY, and that is load-bearing.
//!
//! 🔴 `zzop_parser_typescript`'s parse counter is a process-global `AtomicU64` and `cargo test` runs the
//! tests inside one binary CONCURRENTLY. Two tests that each call `reset_parse_count()` and then read
//! the delta therefore corrupt each other — measured, not reasoned: putting this test next to
//! `one_analyze_pass_parses_one_ts_file_this_many_times` turned that pin red at `1 -> 5`, which is this
//! fixture's parses landing in that fixture's window. A separate `tests/*.rs` is a separate process,
//! which is the cheapest isolation available without a serialization dependency.
//!
//! ⚠ So: **at most one parse-count test per test binary.** A third one needs a third file.

//! How many times does ONE analyze pass hand ONE TypeScript file to swc?
//!
//! This file exists because that number used to live in prose. `pipeline::parsers::parse_typescript`'s
//! doc said a well-formed file is "parsed by swc three times per pass (four counting `parse_ok`'s
//! probe)" — a correct statement about that one function and a wrong one about a run, since the per-file
//! lane around it (`pipeline::fresh`, `pipeline::io`) runs many further independent extractors over the
//! same text, each parsing again. The first measurement below came back an order of magnitude above the
//! documented figure. A written count in that position rots the moment an extractor is added, and there
//! is no way to notice: nothing reads a comment.
//!
//! `zzop_parser_typescript::parse_count` counts the real thing (it sits on `parse_with_cm`, the crate's
//! sole swc entry), so this test replaces the sentence with a measurement. When the number moves, this
//! test says so and the new number is recorded here — that is the intended maintenance, not a nuisance:
//! per-file parse count is the dominant cost of a cold run, and a silent doubling of it is exactly the
//! regression worth a failing test.
//!
//! ## Why its own test binary
//! The census is a process-wide counter and cargo runs each `tests/*.rs` as its own process, but tests
//! WITHIN a binary run on parallel threads. So this file deliberately holds exactly one test — any second
//! test here that touched a `.ts` file would race the counter and make both numbers meaningless.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        // A clock is not a unique name. Windows' `SystemTime` granularity is coarse enough
        // that two threads entering here together read the SAME nanos, and two tests that then
        // `git init` one directory collide inside git's own template copy — a red gate with
        // nothing to do with the change under test. The counter is what makes the name unique;
        // the clock only keeps runs apart, and this file was one of the last without it.
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The sibling gate for the population the equality pin above CANNOT cover: a tree that has an HTTP
/// route, and therefore a second whole-tree pass.
///
/// 🔴 Why a ceiling and not an equality. Review ledger V22 wanted a noise-free perf gate and reached for
/// a deterministic count, since this repo already gates on counts rather than on wall clock. V48 then
/// measured that the count is NOT deterministic here — 2..6 over eight runs, still 5..6 at
/// `RAYON_NUM_THREADS=1`, still 5..6 on a fresh process's first run (the table on
/// `PARSES_PER_TS_FILE` in `analyze_parse_census.rs` has the readings). The memo is thread-local and the counter is process-global,
/// so which worker picks up a file on the second pass decides whether that thread already holds it. That
/// closed equality at every setting — but it does not close a CEILING, because the regression worth
/// gating on does not nudge the count, it multiplies it.
///
/// 🔵 What this actually catches: `parse_with_cm`'s memo failing. Before the 2026-08-08 fold, a file was
/// parsed once per extractor (~37×); if a caller starts re-reading or rewriting the text between
/// extractors, the key changes and the count returns to that order of magnitude. Two files across two
/// passes floor at 4 parses and were measured at most 6, so a ceiling of 12 sits 2× above the observed
/// maximum and ~6× below a memo failure. It cannot flake on scheduling and it cannot miss the defect.
///
/// ⚠ What it does NOT do: bound wall clock. A shared-runner timing band is still an open judgement
/// (V22), and this gate is deliberately not that — it constrains WORK, which is the axis this repo can
/// measure without inventing a tolerance.
const MAX_PARSES_ROUTE_BEARING_TREE: u64 = 12;

#[test]
fn a_route_bearing_tree_stays_under_the_parse_ceiling() {
    let dir = TempDir::new("zzop-engine-parse-ceiling");
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/routes.ts"),
        "import express from 'express';\n\
         const r = express.Router();\n\
         r.post('/things', (q, s) => s.json({}));\n\
         export default r;\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/util.ts"),
        "export function u(): number { return 1; }\n",
    )
    .unwrap();

    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        ..EngineConfig::default()
    };

    zzop_parser_typescript::reset_parse_count();
    let out = analyze_tree(dir.path(), &cfg);
    let parses = zzop_parser_typescript::parse_count();

    // Non-vacuous twice over: both files were analyzed, AND the route was actually extracted — without
    // the route there is no second pass and this test would silently degrade into the one above.
    assert!(
        out.nodes.iter().any(|n| n.path == "src/routes.ts")
            && out.nodes.iter().any(|n| n.path == "src/util.ts"),
        "both fixture files must have been analyzed: {:?}",
        out.nodes.iter().map(|n| &n.path).collect::<Vec<_>>()
    );
    assert!(
        out.ir
            .ir
            .io
            .as_ref()
            .is_some_and(|io| io.provides.iter().any(|p| p.kind == "http")),
        "fixture must provide an http route, or there is no second pass to bound: {:?}",
        out.ir.ir.io
    );

    assert!(
        parses <= MAX_PARSES_ROUTE_BEARING_TREE,
        "parses on a route-bearing tree ({parses}) exceeded the ceiling \
         ({MAX_PARSES_ROUTE_BEARING_TREE}). The count here is not deterministic (see \
         `PARSES_PER_TS_FILE`'s table) so a small change is scheduling, not a defect — but this \
         ceiling sits ~6x below a `parse_with_cm` memo failure, so a breach means the memo stopped \
         hitting and every TypeScript file in every repo is being parsed once per extractor again."
    );
}
