//! End-to-end coverage for the engine-level minified/generated-file DSL skip
//! (`zzop_core::dsl::is_minified_or_generated`, wired into `zzop_engine::pipeline::eval_packs`): a file
//! classified minified/generated (a 5000+ char single line, OR 500+ char lines dominating >= 50% of the
//! file's bytes — see that function's doc for the two-prong rule and its rationale) skips
//! EVERY DSL rule-pack matcher type entirely, while native structural extraction (symbols/imports/loc) and
//! the aggregate `AnalyzeOutput::warnings` self-report both keep working normally. See
//! `docs/ARCHITECTURE.md`'s "Minified/generated files (DSL skip)" section for the user-facing contract this
//! proves, and its explicit distinction from "degraded" (which still runs line-scan rules).
//!
//! Loads the REAL shipped packs from `rules/dsl/` (not stubs) so this exercises two independently-shipped
//! packs' rules at once: `be-db/empty-catch-on-write` (method-scan) and `be-security/raw-query-interpolation`
//! (line-scan) — proving the skip applies across matcher types and across packs, not just to the one rule
//! the 500-char heuristic used to be hard-coded onto.

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

/// Every real shipped pack under `rules/dsl/` — resolved from `CARGO_MANIFEST_DIR`
/// (`crates/engine` -> up two -> repo root -> `rules/dsl`), same resolution shape
/// `zzop_engine`'s own `lib.rs` test module (`be_security_java_pack`) and `analyze_cache.rs`'s
/// `typescript_pack` already use.
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

/// A single fat line (6000+ chars, no newline) containing BOTH trigger shapes at once: a DB write followed
/// by an empty `catch {}` in the same function (`be-db/empty-catch-on-write`, a method-scan rule) and a
/// `$queryRawUnsafe` call (`be-security/raw-query-interpolation`, a line-scan rule). The long string literal
/// (`bundled`) is only padding — 6000 chars deliberately trips the classifier's ABSOLUTE prong (5000+ char
/// single line, minified regardless of file-byte ratio), so this fixture stays classified minified no
/// matter what other lines future edits add to it; none of the padding is itself part of either trigger
/// shape.
fn minified_line() -> String {
    format!(
        "import {{ helper }} from './helper.mjs'; export function run() {{ const bundled = \"{}\"; try {{ prisma.order.update({{}}); }} catch (e) {{}} if (Math.random()) {{ db.$queryRawUnsafe(bundled); }} return helper(bundled); }}\n",
        "x".repeat(6000)
    )
}

/// The exact same trigger shapes as `minified_line`, but spread across ordinary short lines — the control
/// case proving the DSL skip is heuristic-specific (keyed on a giant physical line), not a blanket
/// regression that would also silence these rules on normal code.
fn normal_lines() -> String {
    "import { helper } from './helper.mjs';\n\
     export function run() {\n\
     \x20 const bundled = \"short\";\n\
     \x20 try {\n\
     \x20   prisma.order.update({});\n\
     \x20 } catch (e) {}\n\
     \x20 if (Math.random()) {\n\
     \x20   db.$queryRawUnsafe(bundled);\n\
     \x20 }\n\
     \x20 return helper(bundled);\n\
     }\n"
    .to_string()
}

/// The anti-collateral shape this ratio prong exists for: an ordinary hand-written file containing ONE
/// long (~700 char) string-literal line — a prompt template — among many normal lines, plus a real rule
/// trigger (`$queryRawUnsafe`) on a normal line. Under the classifier's ratio prong the single long line
/// is well under 50% of the file's bytes, so this file must keep FULL DSL coverage; a naive "any 500+ char
/// line is minified" rule would have silently dropped it.
fn long_prompt_lines() -> String {
    let mut text = String::new();
    text.push_str("export function toolPrompt(input) {\n");
    for i in 0..30 {
        text.push_str(&format!(
            "  const filler{i} = computeSomethingOrdinary(input, {i});\n"
        ));
    }
    text.push_str(&format!("  const prompt = \"{}\";\n", "word ".repeat(140))); // ~700-char line
    text.push_str("  db.$queryRawUnsafe(prompt);\n");
    text.push_str("  return prompt;\n");
    text.push_str("}\n");
    text
}

fn fixture_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-minified-fixture");
    dir.write("src/bundle/minified.mjs", &minified_line());
    dir.write("src/normal.mjs", &normal_lines());
    dir.write("src/prompt-tool.mjs", &long_prompt_lines());
    dir
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "minified-fixture".to_string(),
        packs: all_shipped_packs(),
        ..EngineConfig::default()
    }
}

#[test]
fn minified_file_produces_zero_dsl_findings_across_every_loaded_pack() {
    let dir = fixture_tree();
    let pack_ids: Vec<String> = all_shipped_packs().into_iter().map(|p| p.id).collect();
    let out = analyze_tree(dir.path(), &config());

    // Restricted to DSL findings (rule_id = "{pack.id}/{rule.id}") — this deliberately does NOT assert
    // zero findings of every kind for this file: whole-graph NATIVE analyses (`dead-candidates`/
    // `dead-exports`, both bare ids with no pack prefix) still run over this file's structural extraction
    // exactly as normal (see `native_structural_extraction_still_covers_the_minified_file` below) and are
    // expected to fire here (an unimported, single-file `run` export is a legitimate dead-candidate/
    // dead-export in this tiny fixture tree) — only the DSL rule-pack matchers are what the minified skip
    // silences.
    let minified_dsl_findings: Vec<&zzop_core::Finding> = out
        .findings
        .iter()
        .filter(|f| f.file == "src/bundle/minified.mjs")
        .filter(|f| {
            pack_ids
                .iter()
                .any(|id| f.rule_id.starts_with(&format!("{id}/")))
        })
        .collect();
    assert!(
        minified_dsl_findings.is_empty(),
        "expected zero DSL findings from the minified file, got: {minified_dsl_findings:?}"
    );
}

#[test]
fn minified_skip_is_reported_in_warnings_with_the_fixtures_path() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings.iter().any(|w| w.contains("minified")
            && w.contains("generated")
            && w.contains("src/bundle/minified.mjs")),
        "expected a minified/generated skip warning naming the fixture path, got: {:?}",
        out.warnings
    );
}

#[test]
fn the_same_trigger_shapes_on_normal_length_lines_still_fire() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config());

    let normal_findings: Vec<&str> = out
        .findings
        .iter()
        .filter(|f| f.file == "src/normal.mjs")
        .map(|f| f.rule_id.as_str())
        .collect();
    assert!(
        normal_findings.contains(&"be-db/empty-catch-on-write"),
        "expected empty-catch-on-write to fire on the normal-length control file, got: {normal_findings:?}"
    );
    assert!(
        normal_findings.contains(&"be-security/raw-query-interpolation"),
        "expected raw-query-interpolation to fire on the normal-length control file, got: {normal_findings:?}"
    );
}

#[test]
fn one_long_string_literal_line_in_a_normal_file_does_not_cost_it_dsl_coverage() {
    // Engine-level anti-collateral regression test for the classifier's ratio prong (see
    // `long_prompt_lines`'s doc): a hand-written file with a single ~700-char prompt string among many
    // normal lines must NOT be classified minified — its DSL findings must still fire, and it must not
    // be named in the minified/generated warning.
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.findings
            .iter()
            .any(|f| f.file == "src/prompt-tool.mjs"
                && f.rule_id == "be-security/raw-query-interpolation"),
        "expected raw-query-interpolation to still fire on the one-long-line normal file, got: {:?}",
        out.findings
    );
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("src/prompt-tool.mjs")),
        "the one-long-line normal file must not appear in the minified/generated warning, got: {:?}",
        out.warnings
    );
}

#[test]
fn native_structural_extraction_still_covers_the_minified_file() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config());

    let loc = out
        .ir
        .ir
        .loc
        .get("src/bundle/minified.mjs")
        .copied()
        .unwrap_or(0);
    assert!(
        loc > 0,
        "expected the minified file to still contribute a loc count (native extraction unaffected)"
    );

    let has_symbol = out
        .ir
        .ir
        .symbols
        .iter()
        .any(|s| s.file == "src/bundle/minified.mjs" && s.name == "run");
    assert!(
        has_symbol,
        "expected the minified file's `run` function symbol to still be extracted, got: {:?}",
        out.ir.ir.symbols
    );
}

#[test]
fn two_runs_over_the_same_tree_produce_identical_warnings_and_findings() {
    let dir = fixture_tree();
    let cfg = config();
    let out1 = analyze_tree(dir.path(), &cfg);
    let out2 = analyze_tree(dir.path(), &cfg);

    assert_eq!(out1.warnings, out2.warnings);
    assert_eq!(
        serde_json::to_value(&out1.findings).unwrap(),
        serde_json::to_value(&out2.findings).unwrap()
    );
}

#[test]
fn warm_cache_rerun_still_reports_the_minified_warning() {
    // Regression guard for the exact bug `CACHE_SCHEMA_VERSION` bumping to `zzop-cache-v6` exists to prevent:
    // a stale/pre-`minified`-field cache entry silently defaulting `minified` to `false` on a warm rerun
    // would make this warning vanish nondeterministically. This test proves the warm path still carries it.
    let dir = fixture_tree();
    let cache_dir = TempDir::new("zzop-engine-minified-cache-store");
    let cfg = EngineConfig {
        cache_dir: Some(cache_dir.path().to_path_buf()),
        ..config()
    };

    let cold = analyze_tree(dir.path(), &cfg);
    assert!(
        cold.warnings
            .iter()
            .any(|w| w.contains("minified") && w.contains("src/bundle/minified.mjs")),
        "expected the minified warning on the cold run, got: {:?}",
        cold.warnings
    );

    let warm = analyze_tree(dir.path(), &cfg);
    let stats = warm.cache.expect("expected cache stats on warm run");
    assert_eq!(
        stats.hits, warm.file_count,
        "expected every file to hit on the warm rerun"
    );
    assert!(
        warm.warnings
            .iter()
            .any(|w| w.contains("minified") && w.contains("src/bundle/minified.mjs")),
        "expected the minified warning to survive a warm cache rerun, got: {:?}",
        warm.warnings
    );
}
