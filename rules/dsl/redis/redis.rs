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

// --- flushall-in-code ---

#[test]
fn flush_all_method_call_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/cache.ts",
        "declare const client: any;\nexport async function resetCache() {\n  await client.flushAll();\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "flushall-in-code");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn quoted_flushall_command_literal_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/cache2.ts",
        "declare const client: any;\nexport async function resetCacheRaw() {\n  client.sendCommand([\"FLUSHALL\"]);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "flushall-in-code");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn flushdb_method_call_case_insensitive_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/cache3.js",
        "const client = require(\"redis\").createClient();\nasync function wipe() {\n  await client.flushDb();\n}\nmodule.exports = { wipe };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "flushall-in-code");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn scoped_unlink_of_explicit_keys_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/session.ts",
        "declare const client: any;\nexport async function clearSession(id: string) {\n  await client.unlink(`session:${id}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "flushall-in-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn flush_all_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/cache.ts",
        "declare const client: any;\nexport async function resetCache() {\n  // await client.flushAll(); -- old implementation, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "flushall-in-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn redis_flush_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/cache.ts",
        "declare const client: any;\nexport async function resetCache() {\n  // redis-flush-ok: dedicated cache-reset job, vetted\n  await client.flushAll();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "flushall-in-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn flush_all_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/__tests__/cache.test.ts",
        "declare const client: any;\nexport async function resetCache() {\n  await client.flushAll();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "flushall-in-code").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- keys-glob-scan ---

#[test]
fn keys_call_with_glob_string_literal_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/users.ts",
        "declare const client: any;\nexport async function findUserKeys() {\n  return client.keys(\"user:*\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "keys-glob-scan");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn keys_call_with_bare_wildcard_string_literal_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/users2.ts",
        "declare const client: any;\nexport async function findAllKeys() {\n  return client.keys(\"*\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "keys-glob-scan");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn quoted_keys_command_literal_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/rawkeys.ts",
        "declare const client: any;\nexport async function scanKeysRaw() {\n  client.sendCommand([\"KEYS\", \"*\"]);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "keys-glob-scan");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn scan_cursor_iteration_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/scanUsers.ts",
        "declare const client: any;\nexport async function scanUserKeys() {\n  let cursor = \"0\";\n  const found: string[] = [];\n  do {\n    const [next, batch] = await client.scan(cursor, \"MATCH\", \"user:*\", \"COUNT\", 100);\n    cursor = next;\n    found.push(...batch);\n  } while (cursor !== \"0\");\n  return found;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "keys-glob-scan").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn redis_keys_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/debug.ts",
        "declare const client: any;\nexport async function debugListAllKeys() {\n  // redis-keys-ok: offline debug script, tiny fixed keyspace\n  return client.keys(\"*\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "keys-glob-scan").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn keys_glob_scan_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/__tests__/users.test.ts",
        "declare const client: any;\nexport async function findUserKeys() {\n  return client.keys(\"user:*\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "keys-glob-scan").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn object_keys_of_a_plain_object_is_not_flagged() {
    // FP guard: `Object.keys(config)` contains the literal substring `.keys(` (so the cheap `require_file`
    // pre-skip lets the file through), but its argument is a bare identifier, not a string literal — the
    // quote-required-right-after-the-paren anchor in `line_pattern` correctly rejects it.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/config.ts",
        "declare const config: Record<string, unknown>;\nexport function listConfigKeys() {\n  return Object.keys(config);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "keys-glob-scan").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn map_keys_no_arg_call_is_not_flagged() {
    // FP guard: `map.keys()` — no argument at all, so no quote follows the paren.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/mapUtils.ts",
        "declare const map: Map<string, string>;\nexport function listMapKeys() {\n  return Array.from(map.keys());\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "keys-glob-scan").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn bare_no_arg_keys_call_on_a_plain_object_is_not_flagged() {
    // FP guard: a python-ish no-arg `.keys()` call (e.g. a dict-like wrapper) must not fire either.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/dictLike.ts",
        "declare const data: { keys(): string[] };\nexport function listDataKeys() {\n  return data.keys();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "keys-glob-scan").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- client-no-error-listener ---

#[test]
fn node_redis_create_client_with_no_error_listener_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/redisClient.ts",
        "import { createClient } from \"redis\";\nexport const client = createClient({ url: process.env.REDIS_URL });\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "client-no-error-listener");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn ioredis_new_redis_with_no_error_listener_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/ioredisClient.ts",
        "import Redis from \"ioredis\";\nexport const redis = new Redis(process.env.REDIS_URL);\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "client-no-error-listener");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn ioredis_cluster_client_with_no_error_listener_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/ioredisCluster.ts",
        "import Redis from \"ioredis\";\nexport const cluster = new Redis.Cluster([{ host: \"127.0.0.1\", port: 7000 }]);\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "client-no-error-listener");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn ioredis_import_with_no_client_construction_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/ping.ts",
        "import type { Redis as RedisClient } from \"ioredis\";\nexport function ping(client: RedisClient) {\n  return client.ping();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-no-error-listener").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn ioredis_client_with_error_listener_attached_is_not_flagged() {
    // FP guard: the client is created AND `.on('error', ...)` is attached elsewhere in the same file —
    // `require_file_absent` skips the whole file once the veto pattern is found anywhere in it.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/ioredisClient.ts",
        "import Redis from \"ioredis\";\nexport const redis = new Redis(process.env.REDIS_URL);\nredis.on(\"error\", (err) => {\n  console.error(\"Redis connection error\", err);\n});\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-no-error-listener").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn supabase_create_client_without_a_redis_import_is_not_flagged() {
    // FP guard: `createClient(...)` is also the Supabase JS SDK's factory function name — without an
    // ioredis/redis/@redis/client import specifier in the file, the `require_file` gate never opts this
    // file in, so the same-named-but-unrelated call is not mistaken for a redis client.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/supabaseClient.ts",
        "import { createClient } from \"@supabase/supabase-js\";\nexport const supabase = createClient(process.env.SUPABASE_URL!, process.env.SUPABASE_KEY!);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-no-error-listener").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn redis_error_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/redisClient.ts",
        "import { createClient } from \"redis\";\n// redis-error-ok: error listener attached in bootstrap.ts\nexport const client = createClient({ url: process.env.REDIS_URL });\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-no-error-listener").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn create_client_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/redisClient.ts",
        "import { createClient } from \"redis\";\n// const client = createClient({ url: process.env.REDIS_URL }); -- old setup\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-no-error-listener").is_empty(),
        "{:?}",
        out.findings
    );
}
