//! `Matcher::IoScan` core-behavior tests: `file_exclude_pattern`, `direction`/`kind`/`key_pattern`/`negate`
//! (rewritten against `eval_pack_io_scan` + `IoScanTreeContext`, whole-tree since the 2026 projection
//! redesign) and `symbol_pattern` (new, provides-only evidence). See the `tests_ir_scan` module doc and
//! `gates_tests` for `attr_present`/`attr_absent`/`anchor_exclude_pattern`/suppress-marker/determinism
//! coverage.

use super::super::test_support::{
    io_consume, io_provide, io_provide_symbol, io_scan_pack, scan_io_tree,
};
use crate::io::IoProvide;

// --- file_exclude_pattern ---

#[test]
fn file_exclude_pattern_skips_an_entry_whose_file_matches() {
    let provides = vec![
        IoProvide {
            file: "src/api/users.test.ts".into(),
            ..io_provide("http", "GET /api/users", 1)
        },
        io_provide("http", "GET /api/orders", 2),
    ];
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","file_exclude_pattern":"\\.test\\.ts$","direction":"provides"}"#,
    );
    let f = scan_io_tree(&pack, provides, vec![]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 2, "only the non-test-file entry should surface");
}

#[test]
fn file_exclude_pattern_absent_lets_every_file_through() {
    let provides = vec![
        IoProvide {
            file: "src/api/users.test.ts".into(),
            ..io_provide("http", "GET /api/users", 1)
        },
        io_provide("http", "GET /api/orders", 2),
    ];
    let pack = io_scan_pack(r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides"}"#);
    let f = scan_io_tree(&pack, provides, vec![]);
    assert_eq!(
        f.len(),
        2,
        "no file_exclude_pattern configured -> nothing skipped"
    );
}

#[test]
fn io_scan_negate_flags_provide_keys_not_matching_the_pattern() {
    let provides = vec![
        io_provide("http", "GET /authen/getUserInfo", 10),
        io_provide("http", "GET /api/v1/users", 20),
    ];
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","kind":"http","key_pattern":"^GET /api/v[0-9]+/","negate":true}"#,
    );
    let f = scan_io_tree(&pack, provides, vec![]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 10);
    assert_eq!(
        f[0].data.as_ref().unwrap()["snippet"],
        "GET /authen/getUserInfo"
    );
}

#[test]
fn io_scan_key_none_never_matches_pattern_so_negate_flags_it() {
    let consumes = vec![
        io_consume("http", None, 5), // unresolved dynamic target
        io_consume("http", Some("GET /api/v1/orders"), 6), // versioned, compliant
    ];
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"consumes","kind":"http","key_pattern":"^GET /api/v[0-9]+/","negate":true}"#,
    );
    let f = scan_io_tree(&pack, vec![], consumes);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 5);
    assert_eq!(f[0].data.as_ref().unwrap()["snippet"], "<unresolved>");
}

#[test]
fn io_scan_kind_filter_excludes_non_matching_entries() {
    let provides = vec![
        io_provide("http", "GET /a", 1),
        io_provide("queue", "topic:jobs", 2),
    ];
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","kind":"queue"}"#,
    );
    let f = scan_io_tree(&pack, provides, vec![]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].data.as_ref().unwrap()["snippet"], "topic:jobs");
}

#[test]
fn io_scan_any_direction_scans_both_provides_and_consumes() {
    let provides = vec![io_provide("http", "GET /a", 1)];
    let consumes = vec![io_consume("http", Some("GET /b"), 2)];
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"any","kind":"http"}"#,
    );
    let f = scan_io_tree(&pack, provides, consumes);
    assert_eq!(f.len(), 2);
}

#[test]
fn io_scan_with_an_empty_tree_produces_no_findings() {
    let pack = io_scan_pack(r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"any"}"#);
    let f = scan_io_tree(&pack, vec![], vec![]);
    assert!(f.is_empty());
}

// --- symbol_pattern (provides-only evidence) ---

#[test]
fn symbol_pattern_hit_matches_a_provide_whose_symbol_matches() {
    let provides = vec![io_provide_symbol(
        "http",
        "POST /api/users",
        10,
        "UserController.create",
    )];
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","symbol_pattern":"^UserController\\."}"#,
    );
    let f = scan_io_tree(&pack, provides, vec![]);
    assert_eq!(f.len(), 1);
}

#[test]
fn symbol_pattern_miss_excludes_a_provide_whose_symbol_does_not_match() {
    let provides = vec![io_provide_symbol(
        "http",
        "POST /api/orders",
        10,
        "OrderController.create",
    )];
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","symbol_pattern":"^UserController\\."}"#,
    );
    let f = scan_io_tree(&pack, provides, vec![]);
    assert!(f.is_empty());
}

#[test]
fn symbol_pattern_never_matches_a_provide_whose_symbol_is_none() {
    let provides = vec![io_provide("http", "POST /api/users", 10)]; // symbol: None
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","symbol_pattern":".*"}"#,
    );
    let f = scan_io_tree(&pack, provides, vec![]);
    assert!(
        f.is_empty(),
        "a symbol-less provide must never-guess-match symbol_pattern"
    );
}

#[test]
fn symbol_pattern_never_matches_a_consume_even_under_any_direction() {
    let consumes = vec![io_consume("http", Some("POST /api/users"), 5)];
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"any","symbol_pattern":".*"}"#,
    );
    let f = scan_io_tree(&pack, vec![], consumes);
    assert!(
        f.is_empty(),
        "a consume never carries a symbol, so symbol_pattern must never match it"
    );
}
