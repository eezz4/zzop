//! The LANGUAGE axis of DSL applicability: a tree whose DOMINANT filetype no loaded DSL rule targets.
//! `zzop_engine::analyze::diagnostics::pack_scope::uncovered_extension_warning` (wired into both
//! `analyze::assemble` and `envelope::analyze_envelope` through `pack_scope_warnings`) is the only
//! report that fires for it, and the gap it closes was measured on this repo: 987 of ~1400 files were
//! `.rs`, not one of the 138 bundled DSL rules then carried a `.rs` `file_pattern` (that changed on
//! 2026-08-02 — bundled rules do carry `.rs` patterns now; `scripts/measure/self-analysis-gate.mjs`'s DSL
//! half owns that recount. This file's fixtures were deliberately built on a HAND-BUILT pack rather than
//! the shipped ones, which is why none of them moved), and every other report
//! stayed silent — `no_applicable_dsl_rule_warning` because the tree's `.ts` files DO match,
//! `zero_scope_packs_warning` because most packs therefore have non-zero scope, and
//! `unparsed_extension_warning` because `.rs` parses fine. "0 findings" read as "clean".
//!
//! What it must NOT say is pinned here too: the native structural/whole-graph analyses are never
//! `file_pattern`-gated, so the message may claim a DSL-pack gap and nothing wider.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RulePackDef;
use zzop_engine::{analyze_tree, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
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

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A one-rule pack that targets `.ts` and nothing else — the pack set under test, deliberately a
/// FIXTURE rather than the shipped `rules/dsl/**`.
///
/// The shipped set was tried first and rejected: it makes every assertion here depend on the shipped
/// packs continuing to carry no `.rs` `file_pattern`, so the day a Rust rule ships these tests go red
/// for a reason that has nothing to do with the report they cover. That tripwire is worth having ONCE,
/// and it already exists — `analyze_no_applicable_dsl_rules.rs` owns it, in the test whose own doc
/// records Go passing through exactly this transition when the `go` pack shipped. A second copy would
/// only be a second thing to fix on that day. What this file is for is the MECHANISM: given a pack set
/// that covers one extension and not another, does the report fire, and does one rule silence it.
fn ts_only_pack() -> RulePackDef {
    serde_json::from_str(
        r#"{
          "id": "ts-only-probe",
          "schema_version": 1,
          "rules": [{
            "id": "ts-todo",
            "severity": "warning",
            "message": "A TODO comment.",
            "matcher": { "type": "line-scan", "file_pattern": "(?i)\\.tsx?$", "line_pattern": "TODO" }
          }]
        }"#,
    )
    .expect("the fixture pack must parse")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "uncovered-language-fixture".to_string(),
        packs: vec![ts_only_pack()],
        ..EngineConfig::default()
    }
}

/// One `.rs`-targeting rule, injected as an extra pack. Nothing else about the tree changes, so it is
/// the single variable that decides whether the report fires — the invalidation the report needs to
/// mean anything.
fn rust_targeting_pack() -> RulePackDef {
    serde_json::from_str(
        r#"{
          "id": "rust-probe",
          "schema_version": 1,
          "rules": [{
            "id": "rust-todo",
            "severity": "warning",
            "message": "A TODO comment.",
            "matcher": { "type": "line-scan", "file_pattern": "(?i)\\.rs$", "line_pattern": "TODO" }
          }]
        }"#,
    )
    .expect("the probe pack must parse")
}

const UNCOVERED_HEAD: &str = "NO loaded DSL rule targets";

/// The motivating shape, in miniature: a mostly-Rust tree with enough `.ts` that the tree-wide
/// `no_applicable_dsl_rule_warning` stays SILENT (some rule does apply) while `.rs` — 90% of the tree —
/// is targeted by nothing.
#[test]
fn rust_dominant_tree_with_a_ts_file_reports_the_uncovered_language() {
    let dir = TempDir::new("zzop-engine-uncovered-rs");
    for i in 0..9 {
        dir.write(&format!("src/m{i}.rs"), "pub fn run() -> i32 {\n    1\n}\n");
    }
    dir.write("web/index.ts", "export function run() { return 1; }\n");

    let out = analyze_tree(dir.path(), &config());

    let hit = out
        .warnings
        .iter()
        .find(|w| w.starts_with(UNCOVERED_HEAD))
        .unwrap_or_else(|| {
            panic!(
                "expected the uncovered-language self-report, got: {:?}",
                out.warnings
            )
        });
    assert!(
        hit.contains(".rs (9 file(s), 90% of this tree)"),
        "the report must name the extension, its count and its share: {hit}"
    );
    // The overclaim guard: it may say the DSL packs cover nothing here, never that the language is
    // unanalyzed. The native analyses are not `file_pattern`-gated and did run over these files.
    assert!(
        hit.contains("native structural/whole-graph analyses are not `file_pattern`-gated and did cover these files"),
        "the report must disclaim the wider (false) reading: {hit}"
    );
    // ...and the tree-wide report must stay silent, or this one would be redundant noise.
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("no applicable rules")),
        "a .ts file is in scope, so the tree-wide report must not fire: {:?}",
        out.warnings
    );
}

/// INVALIDATION. Same tree, one extra pack whose single rule carries a `.rs` `file_pattern` — and the
/// report must go away. Without this, a warning that fired unconditionally would pass the test above.
#[test]
fn a_single_rust_targeting_rule_silences_the_report() {
    let dir = TempDir::new("zzop-engine-uncovered-rs-covered");
    for i in 0..9 {
        dir.write(&format!("src/m{i}.rs"), "pub fn run() -> i32 {\n    1\n}\n");
    }
    dir.write("web/index.ts", "export function run() { return 1; }\n");

    let mut cfg = config();
    cfg.packs.push(rust_targeting_pack());
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        !out.warnings.iter().any(|w| w.starts_with(UNCOVERED_HEAD)),
        "one rule with a `.rs` file_pattern covers the extension — the report must not fire: {:?}",
        out.warnings
    );
}

/// A filetype under the share threshold is not a "principal" filetype and is not named. Two `.py`
/// files in a nine-file tree is 22%... so this fixture makes it ONE in twelve (8%) instead, which is
/// the side of the line the constant exists to draw.
#[test]
fn a_minor_uncovered_filetype_is_below_the_share_threshold() {
    let dir = TempDir::new("zzop-engine-uncovered-minor");
    for i in 0..11 {
        dir.write(
            &format!("web/m{i}.ts"),
            "export function run() { return 1; }\n",
        );
    }
    dir.write("tools/gen.py", "def run():\n    return 1\n");

    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings.iter().any(|w| w.starts_with(UNCOVERED_HEAD)),
        ".py is 8% of this tree — below the share threshold, so nothing is reported: {:?}",
        out.warnings
    );
}

/// With no packs loaded the report is silent: `zero_packs_warning` owns that disclosure, and "no rule
/// targets .rs" is not news when there are no rules at all.
#[test]
fn no_packs_loaded_leaves_the_report_silent() {
    let dir = TempDir::new("zzop-engine-uncovered-nopacks");
    for i in 0..9 {
        dir.write(&format!("src/m{i}.rs"), "pub fn run() -> i32 {\n    1\n}\n");
    }
    dir.write("web/index.ts", "export function run() { return 1; }\n");

    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            source_id: "uncovered-language-nopacks".to_string(),
            ..EngineConfig::default()
        },
    );

    assert!(
        !out.warnings.iter().any(|w| w.starts_with(UNCOVERED_HEAD)),
        "no packs loaded — the report must be silent: {:?}",
        out.warnings
    );
}

/// A Rust-ONLY tree keeps its existing tree-wide report and does NOT also get this one: two lines
/// saying the same thing at different scopes is the noise readers learn to skip.
#[test]
fn rust_only_tree_keeps_the_tree_wide_report_and_gets_no_second_line() {
    let dir = TempDir::new("zzop-engine-uncovered-rs-only");
    dir.write("src/main.rs", "fn main() {\n    println!(\"hi\");\n}\n");

    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("no applicable rules")),
        "expected the tree-wide report on a Rust-only tree: {:?}",
        out.warnings
    );
    assert!(
        !out.warnings.iter().any(|w| w.starts_with(UNCOVERED_HEAD)),
        "the tree-wide report owns this case — no per-extension second line: {:?}",
        out.warnings
    );
}

const THIN_HEAD: &str = "THIN DSL rule reach on";

/// A pack whose `rules` are `ts_rules` rules targeting `.ts` plus `rs_rules` targeting `.rs` — the
/// single knob these tests turn. Built by hand for the same reason `ts_only_pack` is: the shipped set's
/// per-language reach is a real measurement that moves whenever a rule ships, and a test pinned to it
/// would go red for reasons that have nothing to do with the report.
fn mixed_reach_pack(ts_rules: usize, rs_rules: usize) -> RulePackDef {
    let rule = |id: String, ext: &str| {
        format!(
            r#"{{"id": "{id}", "severity": "warning", "message": "A TODO comment.",
                "matcher": {{ "type": "line-scan", "file_pattern": "(?i)\\.{ext}$", "line_pattern": "TODO" }}}}"#
        )
    };
    let rules: Vec<String> = (0..ts_rules)
        .map(|i| rule(format!("ts-{i}"), "tsx?"))
        .chain((0..rs_rules).map(|i| rule(format!("rs-{i}"), "rs")))
        .collect();
    serde_json::from_str(&format!(
        r#"{{"id": "mixed-reach-probe", "schema_version": 1, "rules": [{}]}}"#,
        rules.join(",")
    ))
    .expect("the fixture pack must parse")
}

fn config_with(pack: RulePackDef) -> EngineConfig {
    EngineConfig {
        source_id: "thin-reach-fixture".to_string(),
        packs: vec![pack],
        ..EngineConfig::default()
    }
}

fn rust_dominant_tree(prefix: &str) -> TempDir {
    let dir = TempDir::new(prefix);
    for i in 0..9 {
        dir.write(&format!("src/m{i}.rs"), "pub fn run() -> i32 {\n    1\n}\n");
    }
    dir.write("web/index.ts", "export function run() { return 1; }\n");
    dir
}

/// The gap the zero-reach report above cannot see: `.rs` IS targeted, so that report is silent by
/// construction — but by 1 rule out of 11, which is what a reader would have to derive for themselves
/// from `packsLoaded[].zeroAdmissionRules` to know that 10 rules were structurally unable to say
/// anything about 90% of their tree.
#[test]
fn a_barely_targeted_dominant_filetype_reports_thin_reach() {
    let dir = rust_dominant_tree("zzop-engine-thin-rs");
    let out = analyze_tree(dir.path(), &config_with(mixed_reach_pack(10, 1)));

    let hit = out
        .warnings
        .iter()
        .find(|w| w.starts_with(THIN_HEAD))
        .unwrap_or_else(|| panic!("expected the thin-reach report, got: {:?}", out.warnings));
    assert!(
        hit.contains(".rs (9 file(s), 90% of this tree): 1 rule(s) in range"),
        "the report must name the extension, its share and its REACH — the reach is the news: {hit}"
    );
    assert!(
        hit.contains("out of 11 rule(s)"),
        "reach is only readable against the size of the loaded rule set: {hit}"
    );
    // Same overclaim guard as its sibling: a path-reach fact, never "this language is unanalyzed".
    assert!(
        hit.contains("native structural/whole-graph analyses are not `file_pattern`-gated"),
        "the report must disclaim the wider (false) reading: {hit}"
    );
    // The two degrees must never both speak about one extension.
    assert!(
        !out.warnings.iter().any(|w| w.starts_with(UNCOVERED_HEAD)),
        "`.rs` is targeted, so the zero-reach report has nothing to say: {:?}",
        out.warnings
    );
}

/// INVALIDATION, on the axis the report is about: the tree, the shares and the file count are
/// unchanged, and only the REACH moves — 4 of 14 rules (28%) is past the threshold and the line goes
/// away. Without this a report that fired on every polyglot tree would pass the test above.
#[test]
fn broadening_the_reach_alone_silences_the_thin_report() {
    let dir = rust_dominant_tree("zzop-engine-thin-rs-covered");
    let out = analyze_tree(dir.path(), &config_with(mixed_reach_pack(10, 4)));

    assert!(
        !out.warnings.iter().any(|w| w.starts_with(THIN_HEAD)),
        "4 of 14 rules reach `.rs` — above the threshold, so the report must not fire: {:?}",
        out.warnings
    );
}

/// A filetype under the share threshold is not principal and is not named, whatever its reach — one
/// answer to "is this a language this tree is made of", shared with the zero-reach report.
#[test]
fn a_minor_thinly_reached_filetype_is_below_the_share_threshold() {
    let dir = TempDir::new("zzop-engine-thin-minor");
    for i in 0..11 {
        dir.write(
            &format!("web/m{i}.ts"),
            "export function run() { return 1; }\n",
        );
    }
    dir.write("src/one.rs", "pub fn run() -> i32 {\n    1\n}\n");

    let out = analyze_tree(dir.path(), &config_with(mixed_reach_pack(10, 1)));

    assert!(
        !out.warnings.iter().any(|w| w.starts_with(THIN_HEAD)),
        ".rs is 8% of this tree — below the share threshold: {:?}",
        out.warnings
    );
}
