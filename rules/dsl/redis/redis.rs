//! End-to-end tests for `rules/dsl/redis/redis.json` — exercised via `zzop_engine::analyze_tree` so every rule runs against real fixture trees on disk, same convention as `be-reliability/be-reliability.rs`/`sql/sql.rs`/`http/http.rs`.
//!
//! Covers all rules in the pack, all `line-scan`: `flushall-in-code`, `keys-glob-scan`, `client-no-error-listener`.
//!
//! Each rule has >=1 positive fixture (count + line asserted), >=1 realistic negative, and a `suppress_marker`
//! case. `keys-glob-scan` additionally guards the documented FP shapes (`Object.keys(x)`, `map.keys()`, a
//! no-arg `.keys()` call) that a naive unscoped `.keys(` pattern would wrongly fire on — the quote-required-
//! right-after-the-paren anchor is what tells them apart. `client-no-error-listener` additionally guards a
//! same-named `createClient` from an unrelated library (`@supabase/supabase-js`) via the ioredis/redis import
//! gate, and an ioredis file that DOES attach `.on('error', ...)` via `require_file_absent`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — copied verbatim from `be-reliability/be-reliability.rs`).
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

/// Loads the real `rules/dsl/redis/redis.json` from the repo, filtered to just the `redis` pack so this test
/// is unaffected by sibling packs under concurrent development (same convention as `be-reliability/be-reliability.rs`).
///
/// `CARGO_MANIFEST_DIR` is the `rules` crate root (`rules/Cargo.toml`), so `dsl/` is `rules/dsl` — this pack's
/// own `redis.json` lives one level down, at `rules/dsl/redis/redis.json`.
fn redis_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
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
        .find(|p| p.id == "redis")
        .expect("redis pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "redis-fixture".to_string(),
        packs: vec![redis_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("redis/{rule}"))
        .collect()
}

// --- file_pattern language-scope regression ---

#[test]
fn java_file_is_out_of_scope_for_flushall_and_keys_glob() {
    // Regression for a blind field-test finding on corpus/oss/be-spring (pure Java, zero redis usage):
    // `flushall-in-code`/`keys-glob-scan` used to include `java` in `file_pattern` alongside the pack's
    // JS/TS extensions, while the pack's third rule (`client-no-error-listener`, whose vocabulary is
    // unambiguously ioredis/node-redis-specific: `createClient`/`.on('error', ...)`) already did not —
    // an inconsistent, apparently-accidental inclusion within one pack. Java client libraries (Jedis/
    // Lettuce) do expose similarly-named `.flushAll()`/`.keys()` methods, but `keys-glob-scan`'s bare
    // `.keys(` shape in particular collides broadly with ordinary Java Map/Collection APIs that have
    // nothing to do with Redis, and the confirmed corpus behavior was `filesInScope` inflated to every
    // `.java` file with none of them actually using Redis. Narrowed to match `client-no-error-listener`'s
    // JS/TS-only scope for pack-internal consistency.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/main/java/com/example/CacheService.java",
        "class CacheService {\n  void reset() { jedis.flushAll(); }\n  java.util.Set<String> allKeys() { return jedis.keys(\"*\"); }\n}\n",
    );
    let out = scan(&dir);
    assert!(out.findings.is_empty(), "{:?}", out.findings);
}

mod client_no_error_listener;
mod flushall_in_code;
mod keys_glob_scan;
