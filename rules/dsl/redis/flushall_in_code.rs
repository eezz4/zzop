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
        "declare const client: any;\nexport async function resetCache() {\n  // zzop-flushall-in-code-ok: dedicated cache-reset job, vetted\n  await client.flushAll();\n}\n",
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

// --- token-as-data vs token-as-code (mono-hub measurement: a self-referential FP on a CRITICAL) ---

#[test]
fn a_string_denylist_defining_the_forbidden_commands_is_not_flagged() {
    // Calibration pin: the rule fired on a lint config that merely NAMES the forbidden commands. A
    // `new Set([...])` of quoted words is data, not a call — the exact class the S1 controller-silence
    // fix addressed with a line-leading anchor after zzop's own docs/fixtures caused 57 false silences.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/lint/forbidden.ts",
        "export const FORBIDDEN = new Set([\"keys\", \"flushDb\", \"flushAll\", \"scan\"]);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "flushall-in-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_denylist_array_leading_with_the_flush_token_is_also_not_flagged() {
    // Order-independence pin: the measured fixture happened to list `keys` first, so command-position
    // alone would have masked the bug. A bare array whose FIRST member is the flush token is caught by
    // the three-consecutive-quoted-words arm instead.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/lint/denied.ts",
        "export const DENIED = [\"flushAll\", \"flushDb\", \"keys\"];\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "flushall-in-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_flush_command_with_an_async_modifier_still_fires() {
    // Positive pin for the documented residual boundary: a genuine two-element command array is NOT a
    // denylist (only three-or-more consecutive quoted words are), so `FLUSHDB ASYNC` still fires.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/cacheAsync.ts",
        "declare const client: any;\nexport async function wipeAsync() {\n  await client.sendCommand([\"FLUSHDB\", \"ASYNC\"]);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "flushall-in-code").len(),
        1,
        "{:?}",
        out.findings
    );
}
