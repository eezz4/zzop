//! Rule-level admission self-report: `packsLoaded[].zeroAdmissionRules` (engine field
//! `PackLoaded::zero_admission_rules`) — the RULE-granularity half of the applicability census whose
//! pack half is `filesInScope`. H3's open question is telling SILENCE apart from GREEN: a rule whose
//! path gates admitted zero files reads exactly like a rule that judged 500 files cleanly, and the
//! pack-level count cannot see it (a pack with 100 in-scope files can still carry a rule whose own
//! `file_pattern` matches nothing here).
//!
//! "Admitted" = the rule's PATH gates as evaluation applies them: `file_pattern` matches the analyzed
//! rel AND `file_exclude_pattern` (when present) does not. Content gates (`require_file*`) are
//! deliberately JUDGMENT, not admission — they are decided by reading the file's text, so a rule that
//! ran its `require_file` probe over N files did judge those N files (and content-gate counting would
//! also need file text, which a warm cache run does not re-read — the census must be byte-identical
//! warm and cold). `0` therefore means structurally vacuous: the rule could not have read a single
//! byte of this tree, so its zero findings are scope, never a clean bill.
//!
//! Two suppressions are pinned below: a pack whose OWN `filesInScope` is 0 lists no rule ids (every
//! rule is trivially zero there — the pack-level zero already says it, and repeating up to a whole
//! pack's rule list per out-of-scope pack would bloat every single-language tree's reply), and an
//! empty tree lists none (the "root produced 0 analyzable files" self-report owns that case, same
//! gate the zero-scope pack warning uses).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{parse_dsl_pack, RulePackDef};
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

fn pack(json: &str) -> RulePackDef {
    parse_dsl_pack(json).expect("fixture pack must parse")
}

fn config(packs: Vec<RulePackDef>) -> EngineConfig {
    EngineConfig {
        source_id: "rule-admission-fixture".to_string(),
        packs,
        ..EngineConfig::default()
    }
}

/// A `.ts`-only tree (two files, so admitted counts are >1 and not accidentally boolean).
fn ts_tree(prefix: &str) -> TempDir {
    let dir = TempDir::new(prefix);
    dir.write("src/app.ts", "export const x = 1;\n");
    dir.write("src/lib.ts", "export const y = 2;\n");
    dir
}

/// The H3 distinction this field exists for: a rule that admitted files and found nothing is NOT
/// listed; a rule whose gates admitted zero files IS — the two used to be byte-identical on the wire.
#[test]
fn a_rule_admitting_zero_files_is_told_apart_from_a_rule_finding_nothing() {
    let dir = ts_tree("zzop-engine-rule-admission-core");
    let out = analyze_tree(
        dir.path(),
        &config(vec![pack(
            r#"{"id": "mixed", "rules": [
                {"id": "ts-quiet", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.ts$", "line_pattern": "zzzz-never-matches"}},
                {"id": "py-blind", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.py$", "line_pattern": "zzzz-never-matches"}}
            ]}"#,
        )]),
    );

    let entry = out
        .packs_loaded
        .iter()
        .find(|p| p.id == "mixed")
        .expect("pack must be loaded");
    assert!(entry.files_in_scope > 0, "{:?}", out.packs_loaded);
    assert_eq!(
        entry.zero_admission_rules,
        vec!["py-blind".to_string()],
        "the rule that admitted files but found nothing must NOT be listed: {:?}",
        out.packs_loaded
    );
    assert!(
        out.findings.iter().all(|f| f.rule_id != "mixed/ts-quiet"),
        "fixture invariant: ts-quiet must find nothing, or this test proves nothing"
    );
}

/// The gate definition: `file_exclude_pattern` is part of admission. A rule whose pattern matches
/// files that its exclude then vetoes wholesale judged NOTHING — while the pack-level `filesInScope`
/// (deliberately pattern-only, an upper bound) stays positive. This is exactly the silence the pack
/// count cannot see.
#[test]
fn a_rule_whose_exclude_vetoes_every_matched_file_counts_as_zero_admission() {
    let dir = ts_tree("zzop-engine-rule-admission-exclude");
    let out = analyze_tree(
        dir.path(),
        &config(vec![pack(
            r#"{"id": "vetoed", "rules": [
                {"id": "all-vetoed", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.ts$",
                             "file_exclude_pattern": "(^|/)src/", "line_pattern": "zzzz-never-matches"}}
            ]}"#,
        )]),
    );

    let entry = out
        .packs_loaded
        .iter()
        .find(|p| p.id == "vetoed")
        .expect("pack must be loaded");
    assert!(
        entry.files_in_scope > 0,
        "pack-level count is pattern-only candidacy and must stay positive: {:?}",
        out.packs_loaded
    );
    assert_eq!(
        entry.zero_admission_rules,
        vec!["all-vetoed".to_string()],
        "{:?}",
        out.packs_loaded
    );
}

/// Suppression 1 — a pack at `filesInScope: 0` lists no rule ids: every rule of it is trivially
/// zero-admission (admission is a subset of the pattern-only pack count), so the ids would repeat the
/// pack-level fact in up to a whole pack's worth of wire.
#[test]
fn a_fully_out_of_scope_pack_lists_no_rule_ids() {
    let dir = ts_tree("zzop-engine-rule-admission-out-of-scope");
    let out = analyze_tree(
        dir.path(),
        &config(vec![pack(
            r#"{"id": "python", "rules": [
                {"id": "r1", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.py$", "line_pattern": "x"}},
                {"id": "r2", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.pyi$", "line_pattern": "x"}}
            ]}"#,
        )]),
    );

    let entry = out
        .packs_loaded
        .iter()
        .find(|p| p.id == "python")
        .expect("pack must be loaded");
    assert_eq!(entry.files_in_scope, 0, "{:?}", out.packs_loaded);
    assert!(
        entry.zero_admission_rules.is_empty(),
        "derivable from the pack-level zero, must not be repeated: {:?}",
        out.packs_loaded
    );
}

/// Suppression 2 — an empty tree admits nothing anywhere, which carries no per-rule information; the
/// root-scope self-report owns that case (the same gate `zero_scope_packs_warning` pins).
#[test]
fn an_empty_tree_lists_no_rule_ids() {
    let dir = TempDir::new("zzop-engine-rule-admission-empty");
    let out = analyze_tree(
        dir.path(),
        &config(vec![pack(
            r#"{"id": "ts", "rules": [
                {"id": "r1", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.ts$", "line_pattern": "x"}}
            ]}"#,
        )]),
    );

    assert_eq!(out.file_count, 0);
    assert!(
        out.packs_loaded
            .iter()
            .all(|p| p.zero_admission_rules.is_empty()),
        "{:?}",
        out.packs_loaded
    );
}

/// Ids are sorted (not pack-definition order), matching the id-sorting convention every other
/// pack-census surface uses — the same tree must always produce the same bytes.
#[test]
fn listed_rule_ids_are_sorted() {
    let dir = ts_tree("zzop-engine-rule-admission-sorted");
    let out = analyze_tree(
        dir.path(),
        &config(vec![pack(
            r#"{"id": "many", "rules": [
                {"id": "zz-blind", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.py$", "line_pattern": "x"}},
                {"id": "ts-live", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.ts$", "line_pattern": "x"}},
                {"id": "aa-blind", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.go$", "line_pattern": "x"}}
            ]}"#,
        )]),
    );

    let entry = out.packs_loaded.iter().find(|p| p.id == "many").unwrap();
    assert_eq!(
        entry.zero_admission_rules,
        vec!["aa-blind".to_string(), "zz-blind".to_string()],
        "{:?}",
        out.packs_loaded
    );
}

/// Determinism across the cache lane (H3 constraint): on a warm run per-file evaluation is skipped and
/// findings are replayed, so a counter derived from EXECUTION would flap to zero. Admission is derived
/// from the scope census over the walked rel list, which both runs compute identically — the whole
/// `packs_loaded` block must be byte-identical warm and cold, with the cache proven warm.
#[test]
fn warm_and_cold_runs_report_identical_admission() {
    let dir = ts_tree("zzop-engine-rule-admission-warmcold");
    let cache = TempDir::new("zzop-engine-rule-admission-cache");
    let cfg = EngineConfig {
        cache_dir: Some(cache.path().to_path_buf()),
        ..config(vec![pack(
            r#"{"id": "mixed", "rules": [
                {"id": "ts-quiet", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.ts$", "line_pattern": "zzzz-never-matches"}},
                {"id": "py-blind", "severity": "info", "message": "m",
                 "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.py$", "line_pattern": "x"}}
            ]}"#,
        )])
    };

    let cold = analyze_tree(dir.path(), &cfg);
    let warm = analyze_tree(dir.path(), &cfg);
    let warm_stats = warm.cache.expect("cache stats present when cache_dir set");
    assert_eq!(warm_stats.misses, 0, "second run must be fully warm");
    assert!(
        warm_stats.hits > 0,
        "second run must actually hit the cache"
    );
    assert_eq!(
        cold.packs_loaded, warm.packs_loaded,
        "admission must never flap between warm and cold"
    );
    assert_eq!(
        warm.packs_loaded
            .iter()
            .find(|p| p.id == "mixed")
            .unwrap()
            .zero_admission_rules,
        vec!["py-blind".to_string()]
    );
}
