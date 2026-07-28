//! The zero-scope-pack advice: `packsLoaded[].filesInScope` has always reported which loaded packs
//! matched no file in the tree, and `packs.disabled` has always been able to drop such a pack — but
//! nothing joined the two, so the evidence sat in the output with no stated lever.
//! `zzop_engine::analyze::diagnostics::zero_scope_packs_warning` (wired into both `analyze::assemble`
//! and `envelope::ingest`) emits ONE aggregated `warnings` line naming every such pack.
//!
//! This is ADVICE, never action: the packs still load and still run. The engine does not inspect the
//! tree for evidence of a stack and skip packs on its own — that would be guessing at what the user can
//! declare, and a wrong evidence vocabulary would make security rules silently not run. The warning
//! therefore claims exactly one thing (no path pattern matched) and hands the decision back.
//!
//! Three suppressions are pinned below, each because its absence would make the line noise:
//! a pack with `filesInScope > 0`, a pack the user ALREADY disabled (`packsLoaded` reflects loading,
//! not gating — a disabled pack still appears there), and a tree that analyzed no files at all.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{
    parse_dsl_pack, FileProjection, NormalizedEnvelope, RuleConfig, RulePackDef,
    NORMALIZED_AST_FORMAT,
};
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

/// One healthy single-rule pack scoped to `ext`, so `filesInScope` is decided purely by the fixture's
/// filenames — no shipped-pack vocabulary is involved and the assertions stay exact.
fn pack(id: &str, ext: &str) -> RulePackDef {
    parse_dsl_pack(&format!(
        r#"{{"id": "{id}", "rules": [
            {{"id": "r1", "severity": "info", "message": "m",
             "matcher": {{"type": "line-scan", "file_pattern": "(?i)\\.{ext}$", "line_pattern": "zzzz-never-matches"}}}}
        ]}}"#
    ))
    .expect("fixture pack must parse")
}

fn config(packs: Vec<RulePackDef>, disabled: &[&str]) -> EngineConfig {
    EngineConfig {
        source_id: "zero-scope-packs-fixture".to_string(),
        packs,
        rule_config: RuleConfig {
            disabled_rules: disabled.iter().map(|s| (*s).to_string()).collect(),
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    }
}

/// A `.ts`-only tree: the `ts` pack is in scope, `java`/`python` are not.
fn ts_tree(prefix: &str) -> TempDir {
    let dir = TempDir::new(prefix);
    dir.write("src/app.ts", "export const x = 1;\n");
    dir
}

fn zero_scope_line(warnings: &[String]) -> Option<&String> {
    warnings.iter().find(|w| w.contains("had 0 files in scope"))
}

/// The core case: some packs in scope, some not — ONE aggregated line, ids sorted, naming the lever.
#[test]
fn packs_with_no_files_in_scope_get_one_aggregated_warning_naming_the_lever() {
    let dir = ts_tree("zzop-engine-zero-scope-core");
    let out = analyze_tree(
        dir.path(),
        &config(
            vec![pack("python", "py"), pack("ts", "ts"), pack("java", "java")],
            &[],
        ),
    );

    let hits: Vec<&String> = out
        .warnings
        .iter()
        .filter(|w| w.contains("had 0 files in scope"))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "exactly ONE aggregated line, never one per pack — got: {:?}",
        out.warnings
    );
    let hit = hits[0];
    assert!(hit.contains("2 loaded pack(s)"), "{hit}");
    // Sorted, quoted, copy-pasteable into the config — and `ts` (in scope) is NOT named.
    assert!(
        hit.contains(r#"`packs: { disabled: ["java", "python"] }`"#),
        "{hit}"
    );
    assert!(
        !hit.contains("\"ts\""),
        "an in-scope pack must not be named: {hit}"
    );
    assert!(hit.contains("file_pattern"), "{hit}");
    assert!(hit.contains("zzop.config.jsonc"), "{hit}");
    // `filesInScope` is the machine half of the same fact — the two must agree.
    let zero_scope: Vec<&str> = out
        .packs_loaded
        .iter()
        .filter(|p| p.files_in_scope == 0)
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(zero_scope, vec!["java", "python"], "{:?}", out.packs_loaded);
}

/// Suppression 1 — an ALREADY-DISABLED pack is not nagged about. `packsLoaded` still lists it
/// (`filesInScope: 0`, since loading is not gating), so the warning cannot simply read that field.
#[test]
fn an_already_disabled_pack_is_not_named() {
    let dir = ts_tree("zzop-engine-zero-scope-disabled");
    let out = analyze_tree(
        dir.path(),
        &config(
            vec![pack("python", "py"), pack("ts", "ts"), pack("java", "java")],
            &["java"],
        ),
    );

    let hit = zero_scope_line(&out.warnings)
        .unwrap_or_else(|| panic!("expected the zero-scope line, got: {:?}", out.warnings));
    assert!(hit.contains("1 loaded pack(s)"), "{hit}");
    assert!(
        hit.contains(r#"`packs: { disabled: ["python"] }`"#),
        "only the still-running pack is named: {hit}"
    );
    assert!(
        !hit.contains("\"java\""),
        "a pack the user already disabled must not be named: {hit}"
    );
    // The disabled pack is still in `packsLoaded` with `filesInScope: 0` — the field the warning
    // deliberately does NOT read on its own.
    assert!(
        out.packs_loaded
            .iter()
            .any(|p| p.id == "java" && p.files_in_scope == 0),
        "{:?}",
        out.packs_loaded
    );
}

/// A pack whose rules are ALL individually disabled (`"<pack>/<rule>"`) is equally already-off — the
/// user made the same decision one level down, so nagging is the same mistake.
#[test]
fn a_pack_with_every_rule_individually_disabled_is_not_named() {
    let dir = ts_tree("zzop-engine-zero-scope-rule-disabled");
    let out = analyze_tree(
        dir.path(),
        &config(
            vec![pack("python", "py"), pack("ts", "ts"), pack("java", "java")],
            &["java/r1"],
        ),
    );

    let hit = zero_scope_line(&out.warnings)
        .unwrap_or_else(|| panic!("expected the zero-scope line, got: {:?}", out.warnings));
    assert!(
        hit.contains(r#"`packs: { disabled: ["python"] }`"#) && !hit.contains("\"java\""),
        "{hit}"
    );
}

/// Suppression 2 — every loaded pack in scope: total silence. A channel that always fires is a channel
/// readers learn to skip.
#[test]
fn no_warning_when_every_pack_has_files_in_scope() {
    let dir = ts_tree("zzop-engine-zero-scope-all-in-scope");
    let out = analyze_tree(dir.path(), &config(vec![pack("ts", "ts")], &[]));

    assert!(
        zero_scope_line(&out.warnings).is_none(),
        "expected silence, got: {:?}",
        out.warnings
    );
}

/// Suppression 3 — an empty tree makes EVERY pack zero-scope, which would render the advice as
/// "disable all your packs" on the strength of no evidence at all. The root-scope self-report already
/// owns that case.
#[test]
fn no_warning_when_the_tree_analyzed_no_files() {
    let dir = TempDir::new("zzop-engine-zero-scope-empty-tree");
    let out = analyze_tree(
        dir.path(),
        &config(vec![pack("python", "py"), pack("java", "java")], &[]),
    );

    assert_eq!(out.file_count, 0);
    assert!(
        zero_scope_line(&out.warnings).is_none(),
        "expected silence on an empty tree, got: {:?}",
        out.warnings
    );
}

/// Envelope mode (Mode A) runs the same config-derived census, so it gets the same disclosure — the
/// parity `no_applicable_dsl_rule_warning`/`uncompilable_rule_warnings` already hold.
#[test]
fn envelope_mode_gets_the_same_warning() {
    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "fixture/1".to_string(),
        source: "zero-scope-envelope".to_string(),
        files: vec![FileProjection {
            path: "src/app.ts".to_string(),
            loc: 1,
            ..FileProjection::default()
        }],
    };
    let out = zzop_engine::analyze_envelope(
        &envelope,
        &config(vec![pack("python", "py"), pack("ts", "ts")], &[]),
    );

    let hit = zero_scope_line(&out.warnings)
        .unwrap_or_else(|| panic!("expected the zero-scope line, got: {:?}", out.warnings));
    assert!(
        hit.contains(r#"`packs: { disabled: ["python"] }`"#),
        "{hit}"
    );
}
