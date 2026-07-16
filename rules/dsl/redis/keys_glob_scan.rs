use super::{hits, scan, TempDir};

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
