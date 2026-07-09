//! End-to-end tests for the cache wiring (see `docs/ARCHITECTURE.md`'s "Caching"; wired through
//! `zzop_engine::EngineConfig::cache_dir`): a cold run against a fresh cache directory misses every file, a
//! warm rerun hits every file and produces byte-for-byte identical output (`cache` stats aside), editing one
//! file invalidates exactly that file, changing the active rule-pack set re-runs findings while still
//! reusing the cached IR (no reparse), a corrupted cache entry degrades to a miss rather than a panic, and
//! `cache_dir: None` leaves `AnalyzeOutput::cache` at `None` (the pre-cache-wiring behavior every other test
//! in this crate already relies on).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RuleConfig, RulePackDef};
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

/// Fires on a `// TODO` line-comment in any `.ts` file.
fn todo_pack() -> RulePackDef {
    let json = r#"{
        "id": "test-cache-todo",
        "framework": "any",
        "rules": [
            {
                "id": "todo",
                "severity": "info",
                "message": "TODO comment found.",
                "matcher": {
                    "type": "line-scan",
                    "file_pattern": "\\.ts$",
                    "line_pattern": "TODO"
                }
            }
        ]
    }"#;
    serde_json::from_str(json).expect("parse inline test-cache-todo pack")
}

/// Fires on a `// FIXME` line-comment in any `.ts` file — a second, independent pack so a test can change
/// *which* packs are active without touching `todo_pack`'s own id/content.
fn fixme_pack() -> RulePackDef {
    let json = r#"{
        "id": "test-cache-fixme",
        "framework": "any",
        "rules": [
            {
                "id": "fixme",
                "severity": "info",
                "message": "FIXME comment found.",
                "matcher": {
                    "type": "line-scan",
                    "file_pattern": "\\.ts$",
                    "line_pattern": "FIXME"
                }
            }
        ]
    }"#;
    serde_json::from_str(json).expect("parse inline test-cache-fixme pack")
}

fn fixture_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-cache-fixture");
    dir.write(
        "a.ts",
        "// TODO: refactor this\n// FIXME: audit this\nexport function a() { return 1; }\n",
    );
    dir.write("b.ts", "export function b() { return 2; }\n");
    dir
}

/// Fires on a `MARKER` line-comment, but only in files under `routes/` — used to prove a per-rule
/// `file_pattern` gating decision is not baked into a cache entry shared by a byte-identical file at a
/// DIFFERENT path (see `two_files_with_identical_content_do_not_alias_each_others_cache_entry`).
fn routes_only_pack() -> RulePackDef {
    let json = r#"{
        "id": "test-cache-scope",
        "framework": "any",
        "rules": [
            {
                "id": "routes-only",
                "severity": "info",
                "message": "matched.",
                "matcher": {
                    "type": "line-scan",
                    "file_pattern": "^routes/",
                    "line_pattern": "MARKER"
                }
            }
        ]
    }"#;
    serde_json::from_str(json).expect("parse inline test-cache-scope pack")
}

/// Two files, `routes/x.ts` and `other/x.ts`, with BYTE-IDENTICAL content — the exact shape that used to
/// alias one cache entry before `CacheKey` grew a `scope` field (see `zzop_engine::cache`'s module doc,
/// "Scope: the path-identity gap `CacheKey::scope` closes"): same content hash, same dispatched language,
/// same active ruleset, but different paths.
fn duplicate_content_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-cache-scope-fixture");
    let content = "// MARKER\nexport const x = 1;\n";
    dir.write("routes/x.ts", content);
    dir.write("other/x.ts", content);
    dir
}

fn config(cache_dir: &Path, packs: Vec<RulePackDef>) -> EngineConfig {
    EngineConfig {
        source_id: "fixture".to_string(),
        packs,
        cache_dir: Some(cache_dir.to_path_buf()),
        ..EngineConfig::default()
    }
}

/// Loads the real `rules/dsl/typescript/typescript.json` from the repo, filtered to just the `typescript`
/// pack — same pattern as `rules/dsl/typescript/typescript.rs`'s own `typescript_pack` helper (duplicated here
/// rather than shared, since these are independent `#[test]` binaries with no common test-support crate).
fn typescript_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "typescript")
        .expect("typescript.json pack present")
}

/// Every `*.json` file directly under `<cache_root>/ir` and `<cache_root>/findings` — the on-disk layout
/// `zzop_cache::AnalysisCache`'s own module doc documents (this crate has no access to that crate's private
/// key/path derivation, nor does it need it: the layout itself is documented contract, not an internal
/// detail this test reaches past).
fn cache_entry_files(cache_root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for sub in ["ir", "findings"] {
        let dir = cache_root.join(sub);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if entry.path().is_file() {
                out.push(entry.path());
            }
        }
    }
    out.sort();
    out
}

fn ir_entry_files(cache_root: &Path) -> Vec<PathBuf> {
    let dir = cache_root.join("ir");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    out.sort();
    out
}

#[test]
fn cold_run_against_a_fresh_cache_dir_is_all_misses() {
    let dir = fixture_tree();
    let cache_dir = TempDir::new("zzop-engine-cache-store");
    let out = analyze_tree(dir.path(), &config(cache_dir.path(), vec![todo_pack()]));

    let stats = out.cache.expect("cache_dir was set, expected Some stats");
    assert_eq!(stats.hits, 0, "a fresh cache dir must produce zero hits");
    assert_eq!(stats.misses, out.file_count);
    assert!(!cache_entry_files(cache_dir.path()).is_empty());
}

#[test]
fn warm_rerun_is_all_hits_and_output_is_otherwise_identical() {
    let dir = fixture_tree();
    let cache_dir = TempDir::new("zzop-engine-cache-store");
    let cfg = config(cache_dir.path(), vec![todo_pack()]);

    let out1 = analyze_tree(dir.path(), &cfg);
    let out2 = analyze_tree(dir.path(), &cfg);

    let stats1 = out1.cache.expect("expected cache stats on cold run");
    assert_eq!(stats1.misses, out1.file_count);

    let stats2 = out2.cache.expect("expected cache stats on warm run");
    assert_eq!(
        stats2.hits, out2.file_count,
        "every file must hit on an unchanged warm rerun"
    );
    assert_eq!(stats2.misses, 0);

    // AnalyzeOutput is identical modulo the `cache` field itself.
    assert_eq!(
        serde_json::to_value(&out1.ir).unwrap(),
        serde_json::to_value(&out2.ir).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&out1.findings).unwrap(),
        serde_json::to_value(&out2.findings).unwrap()
    );
    assert_eq!(out1.degraded, out2.degraded);
    assert_eq!(out1.file_count, out2.file_count);
}

#[test]
fn editing_one_file_causes_exactly_one_miss_on_the_next_run() {
    let dir = fixture_tree();
    let cache_dir = TempDir::new("zzop-engine-cache-store");
    let cfg = config(cache_dir.path(), vec![todo_pack()]);

    let cold = analyze_tree(dir.path(), &cfg);
    assert_eq!(cold.cache.unwrap().misses, cold.file_count);

    // Edit exactly one file's content (content hash changes; b.ts's does not).
    dir.write("b.ts", "export function b() { return 999; }\n");

    let warm = analyze_tree(dir.path(), &cfg);
    let stats = warm.cache.expect("expected cache stats");
    assert_eq!(stats.misses, 1, "only the edited file should miss");
    assert_eq!(stats.hits, warm.file_count - 1);
}

#[test]
fn ruleset_change_reruns_findings_but_reuses_the_cached_ir() {
    let dir = fixture_tree();
    let cache_dir = TempDir::new("zzop-engine-cache-store");

    let cold = analyze_tree(dir.path(), &config(cache_dir.path(), vec![todo_pack()]));
    assert!(
        !cold
            .findings
            .iter()
            .any(|f| f.rule_id == "test-cache-fixme/fixme"),
        "the fixme pack was not active yet"
    );
    let ir_files_before = ir_entry_files(cache_dir.path());
    let ir_bytes_before: Vec<Vec<u8>> = ir_files_before
        .iter()
        .map(|p| fs::read(p).unwrap())
        .collect();

    // Same tree, same cache dir, a DIFFERENT active pack set -> different ruleset fingerprint.
    let warm = analyze_tree(
        dir.path(),
        &config(cache_dir.path(), vec![todo_pack(), fixme_pack()]),
    );

    // Findings actually re-ran: the newly active pack's finding now shows up.
    assert!(
        warm.findings
            .iter()
            .any(|f| f.rule_id == "test-cache-fixme/fixme" && f.file == "a.ts"),
        "expected a fixme finding once that pack is active, got: {:?}",
        warm.findings
    );

    // The IR cache was reused, not rewritten: same set of `ir/*.json` files, byte-identical contents (a
    // fresh parse would still produce the same bytes here, so this asserts "not overwritten", not merely
    // "would be the same if it were" — see the module doc's stronger claim being the fingerprint/counter
    // assertion below, this is corroborating evidence at the storage layer).
    let ir_files_after = ir_entry_files(cache_dir.path());
    assert_eq!(
        ir_files_before, ir_files_after,
        "no IR entries were added or removed"
    );
    let ir_bytes_after: Vec<Vec<u8>> = ir_files_after
        .iter()
        .map(|p| fs::read(p).unwrap())
        .collect();
    assert_eq!(
        ir_bytes_before, ir_bytes_after,
        "IR entries must not be rewritten"
    );

    // Every file's findings had to be recomputed under the new ruleset fingerprint, so — per
    // `AnalyzeOutput::cache`'s doc — every file counts as a miss even though its IR was reused.
    let stats = warm.cache.expect("expected cache stats");
    assert_eq!(stats.misses, warm.file_count);
    assert_eq!(stats.hits, 0);
}

#[test]
fn corrupted_cache_entry_degrades_to_a_miss_instead_of_panicking() {
    let dir = fixture_tree();
    let cache_dir = TempDir::new("zzop-engine-cache-store");
    let cfg = config(cache_dir.path(), vec![todo_pack()]);

    let baseline_uncached = analyze_tree(
        dir.path(),
        &EngineConfig {
            cache_dir: None,
            ..config(cache_dir.path(), vec![todo_pack()])
        },
    );
    let cold = analyze_tree(dir.path(), &cfg);
    assert_eq!(cold.file_count, baseline_uncached.file_count);

    let ir_files = ir_entry_files(cache_dir.path());
    assert!(
        !ir_files.is_empty(),
        "expected at least one cached IR entry"
    );
    fs::write(
        &ir_files[0],
        b"{ not valid json at all, corrupted on purpose",
    )
    .unwrap();

    // Must not panic, and must still produce a correct (uncorrupted) result for every file.
    let recovered = analyze_tree(dir.path(), &cfg);
    let stats = recovered
        .cache
        .expect("expected cache stats even with a corrupted entry");
    assert!(
        stats.misses >= 1,
        "the corrupted entry must count as a miss"
    );
    assert_eq!(stats.hits + stats.misses, recovered.file_count);
    assert_eq!(recovered.file_count, baseline_uncached.file_count);
    assert_eq!(
        serde_json::to_value(&recovered.ir).unwrap(),
        serde_json::to_value(&baseline_uncached.ir).unwrap(),
        "a corrupted cache entry must not corrupt the recomputed output"
    );
    assert_eq!(
        serde_json::to_value(&recovered.findings).unwrap(),
        serde_json::to_value(&baseline_uncached.findings).unwrap()
    );
}

#[test]
fn cache_dir_none_leaves_cache_stats_none() {
    let dir = fixture_tree();
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![todo_pack()],
        cache_dir: None,
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    assert!(out.cache.is_none());
}

#[test]
fn two_files_with_identical_content_do_not_alias_each_others_cache_entry() {
    let dir = duplicate_content_tree();
    let cache_dir = TempDir::new("zzop-engine-cache-store");
    let cfg = config(cache_dir.path(), vec![routes_only_pack()]);

    // Cold run populates the cache — which of the two byte-identical files "wins" the on-disk entry (a
    // rayon-scheduling detail) no longer matters: `scope` makes each file's entry independent regardless.
    let cold = analyze_tree(dir.path(), &cfg);
    // Warm rerun: every lookup is now guaranteed to find SOME on-disk entry already written by the cold
    // run. This is the deterministic reproduction of the bug `CacheKey::scope` closes (see
    // `zzop_engine::cache`'s module doc): without `scope`, both files' `get_ir`/`get_findings` calls would
    // target the exact same on-disk path (same content_hash + parser_fingerprint + ruleset_fingerprint)
    // and one of them would silently receive the OTHER file's cached payload.
    let warm = analyze_tree(dir.path(), &cfg);

    for out in [&cold, &warm] {
        // Findings: `file_pattern: "^routes/"` must fire for routes/x.ts only, never other/x.ts, even
        // though the two files are byte-identical and share every fingerprint component.
        let hits: Vec<&str> = out
            .findings
            .iter()
            .filter(|f| f.rule_id == "test-cache-scope/routes-only")
            .map(|f| f.file.as_str())
            .collect();
        assert_eq!(
            hits,
            vec!["routes/x.ts"],
            "the routes-only rule must fire for routes/x.ts and never other/x.ts, got: {:?}",
            out.findings
        );

        // IR: each file's own SourceSymbol must carry ITS OWN path/id, not the other file's.
        let mut xs: Vec<(&str, &str)> = out
            .ir
            .ir
            .symbols
            .iter()
            .filter(|s| s.name == "x")
            .map(|s| (s.file.as_str(), s.id.as_str()))
            .collect();
        xs.sort();
        assert_eq!(
            xs,
            vec![
                ("other/x.ts", "other/x.ts#x"),
                ("routes/x.ts", "routes/x.ts#x"),
            ],
            "each byte-identical file's symbol must carry its own file/id, not the other's"
        );
    }

    // Not over-invalidating: byte-identical content still hits the cache once correctly scoped — a warm
    // rerun over an unchanged tree is still all hits, per file (each file has its OWN scoped entry from
    // the cold run).
    let warm_stats = warm.cache.expect("expected cache stats on warm run");
    assert_eq!(
        warm_stats.hits, warm.file_count,
        "byte-identical content across different files must still hit the cache once scoped correctly"
    );
}

#[test]
fn changing_io_router_names_on_a_warm_rerun_produces_the_new_routes_not_stale_cached_ones() {
    let dir = TempDir::new("zzop-engine-cache-io-fixture");
    dir.write("routes.ts", "customRouter.get(\"/items\", api.list);\n");
    let cache_dir = TempDir::new("zzop-engine-cache-store");

    let mut cfg = config(cache_dir.path(), Vec::new());
    // Default `io.router_names` is `["apiRoutes"]` — `customRouter` is not a recognized router yet, so
    // this file projects no `io` at all.
    let cold = analyze_tree(dir.path(), &cfg);
    assert!(
        cold.ir.ir.io.is_none(),
        "customRouter is not a recognized router name yet, expected no io facts"
    );

    // Same cache dir, same file content — only `io.router_names` changes.
    cfg.io.router_names = vec!["customRouter".to_string()];
    let warm = analyze_tree(dir.path(), &cfg);
    let io = warm
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected io facts once customRouter is a recognized router name");
    assert_eq!(io.provides.len(), 1, "got: {:?}", io.provides);
    assert_eq!(io.provides[0].key, "GET /items");
    assert_eq!(io.provides[0].file, "routes.ts");

    // The option change must invalidate the cached IR for this TypeScript file — not serve the stale
    // (empty-io) entry from the cold run.
    let stats = warm.cache.expect("expected cache stats");
    assert_eq!(
        stats.misses, warm.file_count,
        "an io.router_names change must invalidate every cached TypeScript file's IR"
    );
    assert_eq!(stats.hits, 0);
}

/// The field-verified bug this test guards: a `"<pack>/<rule>"` id (e.g. `"typescript/as-cast"`) in
/// `disabled_rules` used to do nothing at all — `pipeline::run_file_pass` only ever filtered packs by
/// bare pack id, so a full `pack/rule` id matched nothing and the rule kept firing (see
/// `zzop_engine::pipeline::gate_pack_rules`, the fix). This proves the fix end to end AND proves the cache
/// does not serve a stale (pre-disable) finding on the next run: warm the cache with the real
/// `typescript` pack fully enabled, then rerun against the SAME cache dir with `typescript/as-cast`
/// disabled — the `as-cast` findings must be gone, `no-explicit-any` (a different rule in the same pack)
/// must still fire, and the `disabled_rules` change must show up as a `ruleset_fingerprint` change (every
/// file counted as a miss, not a stale hit under the old ruleset).
#[test]
fn per_rule_disable_of_a_real_pack_rule_removes_only_that_rules_findings_and_invalidates_the_cache()
{
    let dir = TempDir::new("zzop-engine-cache-per-rule-disable");
    dir.write(
        "a.ts",
        "export function f(x: any) {\n  return x as unknown as string;\n}\n",
    );
    let cache_dir = TempDir::new("zzop-engine-cache-store");
    let pack = typescript_pack();

    let cfg = |disabled_rules: Vec<String>| EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![pack.clone()],
        cache_dir: Some(cache_dir.path().to_path_buf()),
        rule_config: RuleConfig {
            disabled_rules,
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };

    // Warm the cache with the whole pack enabled — both rules fire and get cached.
    let warm_all = analyze_tree(dir.path(), &cfg(Vec::new()));
    assert!(
        warm_all
            .findings
            .iter()
            .any(|f| f.rule_id == "typescript/as-cast"),
        "expected as-cast to fire before disabling it, got: {:?}",
        warm_all.findings
    );
    assert!(
        warm_all
            .findings
            .iter()
            .any(|f| f.rule_id == "typescript/no-explicit-any"),
        "expected no-explicit-any to fire, got: {:?}",
        warm_all.findings
    );

    // Same tree, SAME cache dir — only `typescript/as-cast` is now disabled.
    let out = analyze_tree(dir.path(), &cfg(vec!["typescript/as-cast".to_string()]));

    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "typescript/as-cast"),
        "typescript/as-cast must be fully disabled, got: {:?}",
        out.findings
    );
    assert!(
        out.findings
            .iter()
            .any(|f| f.rule_id == "typescript/no-explicit-any"),
        "no-explicit-any (a different rule in the same pack) must still fire, got: {:?}",
        out.findings
    );

    // Not a stale cache hit: the ruleset fingerprint folds `disabled_rules` in directly (see
    // `zzop_engine::cache::ruleset_fingerprint`), so this run must miss and recompute, never silently
    // reuse findings cached under the "everything enabled" ruleset.
    let stats = out.cache.expect("expected cache stats");
    assert_eq!(
        stats.misses, out.file_count,
        "a disabled_rules change must invalidate every cached file's findings, not serve a stale hit"
    );
    assert_eq!(stats.hits, 0);
}

/// A bare pack id (`"typescript"`, disabling the WHOLE pack) still works after the per-rule gating fix —
/// the pack-level path is untouched by `gate_pack_rules` (it runs only for packs that already survived
/// the pack-level `is_enabled` filter).
#[test]
fn disabling_a_whole_pack_id_still_disables_every_rule_in_it() {
    let dir = TempDir::new("zzop-engine-cache-pack-level-disable");
    dir.write(
        "a.ts",
        "export function f(x: any) {\n  return x as unknown as string;\n}\n",
    );
    let cache_dir = TempDir::new("zzop-engine-cache-store");
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![typescript_pack()],
        cache_dir: Some(cache_dir.path().to_path_buf()),
        rule_config: RuleConfig {
            disabled_rules: vec!["typescript".to_string()],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };

    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id.starts_with("typescript/")),
        "the whole pack must be disabled, got: {:?}",
        out.findings
    );
}

/// An unknown/garbage id in `disabled_rules` must never crash the run — it simply matches nothing (see
/// `registry::is_enabled`'s exact-string-match contract) and every real rule keeps running.
#[test]
fn unknown_garbage_id_in_disabled_rules_does_not_crash_and_other_rules_still_run() {
    let dir = TempDir::new("zzop-engine-cache-garbage-disable");
    dir.write(
        "a.ts",
        "export function f(x: any) {\n  return x as unknown as string;\n}\n",
    );
    let cache_dir = TempDir::new("zzop-engine-cache-store");
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![typescript_pack()],
        cache_dir: Some(cache_dir.path().to_path_buf()),
        rule_config: RuleConfig {
            disabled_rules: vec![
                "totally-unknown-pack".to_string(),
                "typescript/not-a-real-rule".to_string(),
                "typescript/as-cast-typo".to_string(),
            ],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };

    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        out.findings
            .iter()
            .any(|f| f.rule_id == "typescript/as-cast"),
        "garbage disabled_rules entries must not disable real rules, got: {:?}",
        out.findings
    );
    assert!(
        out.findings
            .iter()
            .any(|f| f.rule_id == "typescript/no-explicit-any"),
        "got: {:?}",
        out.findings
    );
}
