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

use zzop_engine::{analyze_tree, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
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

/// Measured on a default `EngineConfig` (no cache, no packs, no git) over a tree holding exactly one
/// well-formed `.ts` file. Update this constant WITH the measurement when the pipeline's extractor set
/// changes; never adjust it to make a red test green without knowing which extractor moved.
/// 36 -> 37 (2026-08-03): the A17 `string_literals` extractor joined `pipeline::fresh` — one more
/// whole-file swc parse per pass, the same one-parse-per-extractor pattern as its 36 siblings.
///
/// 37 -> 1 (2026-08-08): `parse_with_cm` gained a one-entry thread-local memo, so the ~37 extractors
/// that each handed swc the SAME `(rel, text)` now share one parse. The number stopped being
/// "one per extractor" and became **one per file**, which is why this constant is no longer expected
/// to move when an extractor is added — a new extractor in `pipeline::fresh` should keep it at 1, and
/// a rise means the memo stopped hitting (the likely cause is a caller that re-reads or rewrites the
/// text between extractors, which breaks the key). Falling to 0 means the file was not parsed at all;
/// the fixture assertion below catches that separately.
const PARSES_PER_TS_FILE: u64 = 1;

#[test]
fn one_analyze_pass_parses_one_ts_file_this_many_times() {
    let dir = TempDir::new("zzop-engine-parse-census");
    fs::write(
        dir.path().join("a.ts"),
        "export function a(): number { return 1; }\n",
    )
    .unwrap();

    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        ..EngineConfig::default()
    };

    // Baseline taken AFTER any lazily-initialized work the harness itself might have done, and the tree
    // holds one source file, so the delta is attributable to that file alone.
    zzop_parser_typescript::reset_parse_count();
    let out = analyze_tree(dir.path(), &cfg);
    let parses = zzop_parser_typescript::parse_count();

    // Non-vacuous: the file really was analyzed structurally, not skipped as oversized/lexical-only.
    assert!(
        out.nodes.iter().any(|n| n.path == "a.ts"),
        "fixture file must have been analyzed: {:?}",
        out.nodes.iter().map(|n| &n.path).collect::<Vec<_>>()
    );

    assert_eq!(
        parses, PARSES_PER_TS_FILE,
        "swc parses per .ts file per pass changed ({PARSES_PER_TS_FILE} -> {parses}). \
         That is the cold-run cost of every TypeScript file in every repo zzop analyzes. Since \
         2026-08-08 the expected value is ONE PER FILE, not one per extractor: a rise means \
         `parse_with_cm`'s one-entry memo stopped hitting (look for a caller that re-reads or \
         rewrites the file text between extractors, which changes the key), not that an extractor \
         was added."
    );
}
