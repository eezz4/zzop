use super::{hits, scan, TempDir};

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
