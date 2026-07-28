//! The cache-honesty half of the convention-vocabulary axis: a WARM cache must never answer a run
//! whose declared `vocabulary` has changed.
//!
//! Why this file exists separately from `analyze_cache.rs` (which already covers content/pack
//! invalidation): the vocabulary is the one cache input that is neither the file's own bytes nor the rule
//! packs. It reaches BOTH cached lanes — `vocabulary.prismaClientGetter` decides which `db-table` consumes
//! a file's cached `FileIrSlice` carries, and `vocabulary.moneyTokens` decides which `schema/float-money`
//! findings its cached findings entry carries — so a key that omitted it would serve the previous
//! vocabulary's answer for as long as the file's bytes stayed the same. That is the exact silent-staleness
//! failure this axis exists to close, so it is sealed by measurement rather than by inspection.
//!
//! Each test is a triple: cold run, warm-and-verified-warm run, then the change. The middle step is not
//! decoration — without it a "the answer changed" assertion is satisfied by a cache that was never warm,
//! which would make the whole file vacuous.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_cache::{AnalysisCache, CacheKey, FileIrSlice};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig, VocabularyConfig};

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

/// One `analyze_tree` call against `tree`, writing its cache into `cache` (shared across the calls in a
/// test, which is what makes the second call warm).
fn run(tree: &TempDir, cache: &Path, vocabulary: VocabularyConfig) -> AnalyzeOutput {
    analyze_tree(
        tree.path(),
        &EngineConfig {
            cache_dir: Some(cache.to_path_buf()),
            vocabulary,
            ..EngineConfig::default()
        },
    )
}

fn float_money_count(out: &AnalyzeOutput) -> usize {
    out.findings
        .iter()
        .filter(|f| f.rule_id == "schema/float-money")
        .count()
}

fn db_table_consume_count(out: &AnalyzeOutput) -> usize {
    out.ir
        .ir
        .io
        .as_ref()
        .map(|io| io.consumes.iter().filter(|c| c.kind == "db-table").count())
        .unwrap_or(0)
}

/// Seals the FINDINGS half: after a warm cache is established, narrowing `vocabulary.moneyTokens` so it no
/// longer contains `amount` must drop the `schema/float-money` finding — the cached findings entry must not
/// be served for the new vocabulary.
#[test]
fn a_warm_cache_does_not_serve_the_old_findings_after_the_money_vocabulary_narrows() {
    let tree = TempDir::new("zzop-vocab-cache-findings");
    let cache = TempDir::new("zzop-vocab-cache-findings-dir");
    tree.write(
        "schema.prisma",
        "model Invoice {\n  id String @id\n  amount Float\n}\n",
    );

    let cold = run(&tree, cache.path(), VocabularyConfig::built_in());
    assert_eq!(
        float_money_count(&cold),
        1,
        "the declared money vocabulary contains `amount`, so the Float column must be flagged"
    );

    // The cache really is warm now — without this the narrowing assertion below would also pass against a
    // cache that never stored anything.
    let warm = run(&tree, cache.path(), VocabularyConfig::built_in());
    assert!(
        warm.cache.as_ref().is_some_and(|c| c.hits > 0),
        "the second identical run must hit the cache, or this test proves nothing"
    );
    assert_eq!(float_money_count(&warm), 1, "a warm hit repeats the answer");

    let narrowed = run(
        &tree,
        cache.path(),
        VocabularyConfig {
            money_tokens: vec!["price".to_string()],
            ..VocabularyConfig::built_in()
        },
    );
    assert_eq!(
        float_money_count(&narrowed),
        0,
        "a declared money vocabulary without `amount` must re-run the file, not reuse the warm finding"
    );

    // And back again, against the same warm directory: the original vocabulary's entry is still correct
    // for the original vocabulary. Both generations coexist because they key differently.
    let restored = run(&tree, cache.path(), VocabularyConfig::built_in());
    assert_eq!(float_money_count(&restored), 1);
}

/// Seals the IR half: `vocabulary.prismaClientGetter` decides which `db-table` consumes a file's cached
/// `FileIrSlice` carries, so changing it against a warm cache must change the projected io — the findings
/// lane is not the only one the vocabulary reaches.
#[test]
fn a_warm_cache_does_not_serve_the_old_ir_after_the_client_getter_changes() {
    let tree = TempDir::new("zzop-vocab-cache-ir");
    let cache = TempDir::new("zzop-vocab-cache-ir-dir");
    tree.write(
        "src/orders.ts",
        "export async function listOrders() {\n  return getPrisma().order.findMany();\n}\n",
    );

    let cold = run(&tree, cache.path(), VocabularyConfig::built_in());
    assert_eq!(
        db_table_consume_count(&cold),
        1,
        "the built-in client getter is `getPrisma`, so this call projects one db-table consume"
    );

    let warm = run(&tree, cache.path(), VocabularyConfig::built_in());
    assert!(
        warm.cache.as_ref().is_some_and(|c| c.hits > 0),
        "the second identical run must hit the cache, or this test proves nothing"
    );

    let renamed = run(
        &tree,
        cache.path(),
        VocabularyConfig {
            prisma_client_getter: Some("db".to_string()),
            ..VocabularyConfig::built_in()
        },
    );
    assert_eq!(
        db_table_consume_count(&renamed),
        0,
        "a project that calls its accessor `db()` must re-project, not reuse the warm `getPrisma` IR"
    );
}

/// The INVALIDATION check the two tests above need to mean anything: it shows, at the store layer, what a
/// key WITHOUT `vocabulary_fingerprint` would do. Two lookups against one stored entry — one whose key
/// carries a different vocabulary fingerprint (the shipped behavior: a miss, so the run recomputes) and one
/// whose key carries the same fingerprint (what a vocabulary-blind key amounts to: a HIT, serving the
/// previous vocabulary's slice for byte-identical content).
///
/// Written against `zzop_cache` directly rather than by reverting the engine, because the point is the
/// key's own arithmetic: with the field, the two runs address different entries; without it, they address
/// the same one.
#[test]
fn a_vocabulary_blind_key_would_serve_the_stale_entry_and_the_real_key_does_not() {
    let dir = TempDir::new("zzop-vocab-cache-key");
    let cache = AnalysisCache::open(dir.path(), "test-schema").unwrap();

    let under_first_vocabulary = CacheKey {
        content_hash: AnalysisCache::content_hash(b"export const a = 1;"),
        parser_fingerprint: "ts/test".to_string(),
        scope: "src\u{0}a.ts".to_string(),
        vocabulary_fingerprint: "vocab-A".to_string(),
        ruleset_fingerprint: "rules/test".to_string(),
    };
    let stored = FileIrSlice {
        loc: 42,
        ..FileIrSlice::default()
    };
    cache.put_ir(&under_first_vocabulary, &stored).unwrap();

    // What the shipped key does when only the vocabulary changed: a miss.
    let under_second_vocabulary = CacheKey {
        vocabulary_fingerprint: "vocab-B".to_string(),
        ..under_first_vocabulary.clone()
    };
    assert!(
        cache.get_ir(&under_second_vocabulary).is_none(),
        "a changed vocabulary must not find the entry written under the previous one"
    );

    // What a vocabulary-blind key would do: the same lookup, with the field held constant, is a HIT that
    // hands back the previous vocabulary's slice. This is the defect the field closes, made visible.
    let vocabulary_blind = CacheKey {
        vocabulary_fingerprint: "vocab-A".to_string(),
        ..under_second_vocabulary
    };
    assert_eq!(
        cache.get_ir(&vocabulary_blind).map(|ir| ir.loc),
        Some(42),
        "holding the vocabulary fingerprint constant is exactly what serving a stale answer looks like"
    );
}
