//! End-to-end proof that `zzop_parser_rust::extract_test_spans` REACHES the DSL rule packs — the whole
//! chain `parser -> pipeline::fresh -> SourceFile::test_spans -> dsl::eval`'s test-region gate, driven
//! through the real `analyze_tree` entry point with the real shipped packs.
//!
//! ## Why a parser unit test was not enough
//! `parser/parser-rust/src/lang/test_spans/tests.rs` proves the FACT exists. It cannot prove the fact
//! ARRIVES: every span channel in this workspace crosses four struct boundaries (`FileArtifact`,
//! `FileIrSlice`, `SpanFacts`, `SourceFile`) on the way to a matcher, and a channel that is extracted and
//! then dropped one hop later is indistinguishable — from the parser's own tests — from one that works.
//! That break is the failure this repo has taken repeatedly, so the reaching claim gets its own file.
//!
//! ## Every test here asserts BOTH directions
//! Silence alone would be satisfied by a rule that simply died. So each case plants the SAME violation
//! twice — once inside a test region, once in shipped code — and asserts one finding, on the shipped
//! line. A regression in either direction (gate too wide, gate absent) fails.
//!
//! `sql/delete-no-where` is the probe rule throughout: a plain `line-scan` whose `file_pattern` admits
//! `.rs` and `.ts` alike, whose trigger is a closed string literal (so the same text is a violation in
//! any language), and whose `${test-paths-migrations}` path exclusion does NOT match the fixture paths
//! — so a finding that disappears here disappeared because of the SPAN, never because of a path.
//!
//! It was `sql/select-star` until 2026-08-12, when that rule left the bundle for
//! `examples/packs/sql-preferences.json` and `all_shipped_packs()` below — which loads `rules/dsl` and
//! nothing else — stopped finding it, turning all six tests red at once. The replacement is
//! deliberately another BUNDLED rule rather than the exported one: the subject here is the span
//! channel, and pointing the probe at a pack the default configuration does not load would couple this
//! proof to a retrieval step that has nothing to do with what it measures.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
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

fn all_shipped_packs() -> Vec<RulePackDef> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result.packs.into_iter().map(|(_, pack)| pack).collect()
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "rust-test-spans-fixture".to_string(),
        packs: all_shipped_packs(),
        ..EngineConfig::default()
    }
}

/// Lines a rule anchored on, in file order — the shape every assertion below compares against.
fn hit_lines(out: &zzop_engine::AnalyzeOutput, rule_id: &str, rel: &str) -> Vec<u32> {
    let mut lines: Vec<u32> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == rule_id && f.file == rel)
        .map(|f| f.line)
        .collect();
    lines.sort_unstable();
    lines
}

/// The shipped violation is on line 2 and the fixture's is on line 8. Written once so the two directions
/// below cannot drift apart, and so the `#[cfg(test)]` line is the only difference between them.
const SHIPPED_LINE: u32 = 2;
const FIXTURE_LINE: u32 = 8;

/// `gate` is the attribute under test — `"#[cfg(test)]"` for the real shape, `""` for the control that
/// proves the rule is alive on the very same bytes. The inner function deliberately carries NO `#[test]`
/// attribute of its own: the module gate has to be the only thing making the region test-only, or the
/// control would still be gated (measured — the first draft of this file failed exactly there) and the
/// firing half of the claim would be untestable.
fn source(gate: &str) -> String {
    format!(
        "pub fn ship() -> &'static str {{\n    \"DELETE FROM accounts\"\n}}\n\n{gate}\nmod tests {{\n    fn t() {{\n        let _ = \"DELETE FROM accounts\";\n    }}\n}}\n"
    )
}

#[test]
fn a_violation_inside_cfg_test_is_silent_while_the_same_violation_outside_still_fires() {
    let dir = TempDir::new("zzop-engine-rust-test-spans");
    dir.write("src/lib.rs", &source("#[cfg(test)]"));

    let out = analyze_tree(dir.path(), &config());

    assert_eq!(
        hit_lines(&out, "sql/delete-no-where", "src/lib.rs"),
        vec![SHIPPED_LINE],
        "exactly the SHIPPED literal must be judged; the one inside `#[cfg(test)] mod tests` must not"
    );
}

#[test]
fn the_same_file_without_the_cfg_test_attribute_fires_twice() {
    // The other half of the claim: without this, "silent" above is indistinguishable from "the rule is
    // dead on `.rs`". The two fixtures differ by exactly one attribute line, both keep the same line
    // numbering (`""` still occupies the gate's line), so the delta is attributable to nothing else.
    let dir = TempDir::new("zzop-engine-rust-test-spans-control");
    dir.write("src/lib.rs", &source(""));

    let out = analyze_tree(dir.path(), &config());

    assert_eq!(
        hit_lines(&out, "sql/delete-no-where", "src/lib.rs"),
        vec![SHIPPED_LINE, FIXTURE_LINE],
        "with no `#[cfg(test)]` there is no test region, so BOTH literals must be judged"
    );
}

#[test]
fn a_bare_test_attribute_on_a_free_function_also_silences_only_that_function() {
    // The `#[test] fn` shape (no enclosing gated module) — the layout of every `rules/dsl/**/*.rs` file
    // in this repo, which is where 62 of its 98 measured `.rs` findings came from.
    let dir = TempDir::new("zzop-engine-rust-test-spans-bare");
    dir.write(
        "src/lib.rs",
        "pub fn ship() -> &'static str {\n    \"DELETE FROM accounts\"\n}\n\n#[test]\nfn t() {\n    let _ = \"DELETE FROM accounts\";\n}\n",
    );

    let out = analyze_tree(dir.path(), &config());

    assert_eq!(
        hit_lines(&out, "sql/delete-no-where", "src/lib.rs"),
        vec![SHIPPED_LINE]
    );
}

#[test]
fn cfg_not_test_code_is_shipped_code_and_stays_judged() {
    // The negation, end to end: `cfg(not(test))` compiles INTO the release binary, so silencing it would
    // delete a real judgment rather than a fixture.
    let dir = TempDir::new("zzop-engine-rust-test-spans-not");
    dir.write(
        "src/lib.rs",
        "#[cfg(not(test))]\npub fn ship() -> &'static str {\n    \"DELETE FROM accounts\"\n}\n",
    );

    let out = analyze_tree(dir.path(), &config());

    assert_eq!(
        hit_lines(&out, "sql/delete-no-where", "src/lib.rs"),
        vec![3]
    );
}

#[test]
fn a_typescript_file_is_untouched_by_the_gate() {
    // The blast-radius claim, pinned rather than argued: no TypeScript projection produces `test_spans`,
    // so the gate cannot reach a `.ts` finding. This is the machine half of the measured
    // `detection-gate.sh` TS delta of zero.
    let dir = TempDir::new("zzop-engine-rust-test-spans-ts");
    dir.write(
        "src/query.ts",
        "export function ship() {\n  return \"DELETE FROM accounts\";\n}\n\ndescribe(\"x\", () => {\n  it(\"y\", () => {\n    const q = \"DELETE FROM accounts\";\n  });\n});\n",
    );

    let out = analyze_tree(dir.path(), &config());

    assert_eq!(
        hit_lines(&out, "sql/delete-no-where", "src/query.ts"),
        vec![2, 7],
        "a `describe`/`it` block is not a parser-proved test region — TS test exclusion is the PATH's \
         job, and this file's path is not a test path"
    );
}

#[test]
fn the_gate_survives_a_warm_cache() {
    // `test_spans` is a CACHED field (`FileIrSlice`), and the failure mode of dropping it is the loud
    // direction: a warm run would resurrect every finding inside every `#[cfg(test)] mod tests`. Two
    // runs against the same cache directory, same assertion both times.
    let dir = TempDir::new("zzop-engine-rust-test-spans-cache");
    dir.write("src/lib.rs", &source("#[cfg(test)]"));
    let cache_dir = TempDir::new("zzop-engine-rust-test-spans-cachedir");
    let cfg = EngineConfig {
        cache_dir: Some(cache_dir.path().to_path_buf()),
        ..config()
    };

    let cold = analyze_tree(dir.path(), &cfg);
    assert_eq!(
        hit_lines(&cold, "sql/delete-no-where", "src/lib.rs"),
        vec![SHIPPED_LINE],
        "cold run"
    );

    let warm = analyze_tree(dir.path(), &cfg);
    assert_eq!(
        hit_lines(&warm, "sql/delete-no-where", "src/lib.rs"),
        vec![SHIPPED_LINE],
        "warm run — a cache that lost `test_spans` would report the fixture line here"
    );
}
