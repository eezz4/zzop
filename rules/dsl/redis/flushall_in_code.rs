use super::{assert_disqualifier_clause_precedes_imperative, hits, scan, TempDir};

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
/// §27 pin (2026-08-29). `critical`, so the reader acts fast. The disqualifier is the rule's own
/// RESIDUAL: the denylist gate counts words, so a TWO-word denylist is indistinguishable from a
/// two-element command array and fires anyway. A reader of that finding who follows "Scope deletes to
/// explicit keys ..." rewrites a table of forbidden NAMES into `UNLINK` calls — which is the very
/// deletion the rule exists to prevent. At HEAD that sentence sat at byte 1155 and the imperative at
/// byte 263.
///
/// The IMPERATIVE moved rather than the residual, and the residual is why: it opens on "a two-element
/// command array" and "a two-element denylist", both of which are defined by the TOKEN-AS-DATA
/// paragraph above it. Lifting it to the front would put those terms in front of their own definitions.
/// The imperative carries no pronoun and no back-reference, so moving it costs no bridging sentence; it
/// now sits directly in front of the suppression marker, where "what to do" reads as one block. 1972
/// chars before and after, character multiset identical.
///
/// Deliberately NOT pinned: the two FALSE-NEGATIVE disclosures in this message (the gate dropping a real
/// `.flushall(` call on a denylist-shaped line, and "Second residual, the FALSE-NEGATIVE side"). §27's
/// criterion excludes false-negative disclosure by name — a silent rule misleads nobody into an edit.
#[test]
fn flushall_message_puts_the_two_word_denylist_residual_before_the_scope_deletes_imperative() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/cache.ts",
        "declare const client: any;\nexport async function resetCache() {\n  await client.flushAll();\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "flushall-in-code");
    // Sentence repair only — the finding itself is unchanged.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);

    assert_disqualifier_clause_precedes_imperative(
        "flushall-in-code",
        &h[0].message,
        "Residual: a two-element command array is indistinguishable from a two-element denylist",
        "Scope deletes to explicit keys",
    );
}
