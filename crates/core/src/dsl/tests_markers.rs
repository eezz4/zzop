//! Inline ok-marker suppression tests (v2 extension #3): `//`-comment markers for line-scan and
//! method-scan, plus the `--`-comment recognition gated to `.sql` files.

use super::test_support::{method, rule_pack, scan_pack};
use super::RulePackDef;

// --- extension #3: inline ok-marker suppression ---

fn marker_line_pack() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"info","message":"m","suppress_marker":"as-ok","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bas\\b"}}"#,
    )
}

#[test]
fn suppress_marker_on_the_same_line_suppresses_line_scan_finding() {
    let f = scan_pack(
        &marker_line_pack(),
        "f.ts",
        "const x = y as Foo; // as-ok: guaranteed by caller\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn suppress_marker_on_the_line_above_suppresses_line_scan_finding() {
    let f = scan_pack(
        &marker_line_pack(),
        "f.ts",
        "// as-ok: guaranteed by caller\nconst x = y as Foo;\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn suppress_marker_two_lines_above_does_not_suppress() {
    let f = scan_pack(
        &marker_line_pack(),
        "f.ts",
        "// as-ok: guaranteed by caller\nfunction f() {\n  return (\n    y as Foo);\n}\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn suppress_marker_four_lines_above_does_not_suppress() {
    let f = scan_pack(
        &marker_line_pack(),
        "f.ts",
        "// as-ok: too far\nfunction f() {\n  const a = 1;\n  return (\n    y as Foo);\n}\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn suppress_marker_does_not_reach_a_sibling_finding_two_lines_below_it() {
    let f = scan_pack(
        &marker_line_pack(),
        "f.ts",
        "// as-ok: vetted for the next line only\nconst a = x as Foo;\nconst b = y as Bar;\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 3);
}

#[test]
fn unrelated_marker_text_does_not_suppress() {
    let f = scan_pack(
        &marker_line_pack(),
        "f.ts",
        "const x = y as Foo; // unrelated-ok\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn no_marker_at_all_does_not_suppress() {
    let f = scan_pack(&marker_line_pack(), "f.ts", "const x = y as Foo;\n", vec![]);
    assert_eq!(f.len(), 1, "{f:?}");
}

// --- `--`-comment marker recognition, gated to `.sql` files (destructive-migration ergonomics) ---

fn marker_line_pack_sql() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"info","message":"m","suppress_marker":"as-ok","matcher":{"type":"line-scan","file_pattern":"\\.sql$","line_pattern":"\\bas\\b"}}"#,
    )
}

#[test]
fn dash_dash_marker_on_the_same_line_suppresses_line_scan_finding_in_a_sql_file() {
    let f = scan_pack(
        &marker_line_pack_sql(),
        "f.sql",
        "SELECT id as x; -- as-ok: guaranteed by caller\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn dash_dash_marker_on_the_line_above_suppresses_line_scan_finding_in_a_sql_file() {
    let f = scan_pack(
        &marker_line_pack_sql(),
        "f.sql",
        "-- as-ok: guaranteed by caller\nSELECT id as x;\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn dash_dash_marker_is_not_recognized_outside_a_sql_file() {
    // Same rule, same marker text, but a `.ts` file — `--` is not a comment there (`--x` is a
    // decrement), so the `--`-marker recognizer must never activate for it.
    let pack = rule_pack(
        r#"{"id":"r","severity":"info","message":"m","suppress_marker":"as-ok","matcher":{"type":"line-scan","file_pattern":"\\.(ts|sql)$","line_pattern":"\\bas\\b"}}"#,
    );
    let f = scan_pack(
        &pack,
        "f.ts",
        "const x = y as Foo; -- as-ok: nope\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn slash_slash_marker_still_suppresses_in_a_sql_file() {
    // The `--` recognizer is additive: a `.sql` file's `//`-form marker (unusual, but not forbidden)
    // still suppresses too.
    let f = scan_pack(
        &marker_line_pack_sql(),
        "f.sql",
        "SELECT id as x; // as-ok: guaranteed by caller\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn dash_dash_unrelated_marker_text_does_not_suppress_in_a_sql_file() {
    let f = scan_pack(
        &marker_line_pack_sql(),
        "f.sql",
        "SELECT id as x; -- unrelated-ok\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

fn marker_method_pack_sql() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","suppress_marker":"n+1-ok","matcher":{"type":"method-scan","file_pattern":"\\.sql$","patterns":[{"pattern":"\\bfor\\s*\\(","label":"loop"},{"pattern":"\\bfindOne\\(","label":"call"}],"trigger":"call"}}"#,
    )
}

#[test]
fn dash_dash_marker_suppresses_method_scan_finding_in_a_sql_file() {
    let src = "async function f(ids) {\n  for (const id of ids) {\n    -- n+1-ok: batched elsewhere\n    await t.findOne(id);\n  }\n}\n";
    let f = scan_pack(
        &marker_method_pack_sql(),
        "f.sql",
        src,
        vec![method("f", 1, 5)],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn dash_dash_marker_absent_leaves_method_scan_finding_intact_in_a_sql_file() {
    let src =
        "async function f(ids) {\n  for (const id of ids) {\n    await t.findOne(id);\n  }\n}\n";
    let f = scan_pack(
        &marker_method_pack_sql(),
        "f.sql",
        src,
        vec![method("f", 1, 4)],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

fn marker_method_pack() -> RulePackDef {
    rule_pack(
        r#"{"id":"r","severity":"warning","message":"m","suppress_marker":"n+1-ok","matcher":{"type":"method-scan","file_pattern":"\\.ts$","patterns":[{"pattern":"\\bfor\\s*\\(","label":"loop"},{"pattern":"\\bfindOne\\(","label":"call"}],"trigger":"call"}}"#,
    )
}

#[test]
fn suppress_marker_with_regex_metacharacters_suppresses_method_scan_finding() {
    let src = "async function f(ids) {\n  for (const id of ids) {\n    // n+1-ok: batched elsewhere\n    await t.findOne(id);\n  }\n}\n";
    let f = scan_pack(&marker_method_pack(), "f.ts", src, vec![method("f", 1, 5)]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn suppress_marker_absent_leaves_method_scan_finding_intact() {
    let src =
        "async function f(ids) {\n  for (const id of ids) {\n    await t.findOne(id);\n  }\n}\n";
    let f = scan_pack(&marker_method_pack(), "f.ts", src, vec![method("f", 1, 4)]);
    assert_eq!(f.len(), 1, "{f:?}");
}
