//! IR-query matcher tests: symbol-scan (declaration queries) and io-scan (per-file IO-fact queries).

use crate::io::IoFacts;
use crate::ir::SourceSymbolKind;

use super::test_support::{io_consume, io_provide, io_scan_pack, scan_io, scan_symbols, symbol};
use super::{eval_pack, RuleContext, SourceFile};

// --- symbol-scan ---

#[test]
fn symbol_scan_non_negated_flags_names_matching_the_pattern() {
    let f = scan_symbols(
        "f.ts",
        vec![
            symbol("useFoo", SourceSymbolKind::Function, 3, true),
            symbol("bar", SourceSymbolKind::Function, 8, true),
        ],
        r#"{"type":"symbol-scan","file_pattern":"\\.ts$","name_pattern":"^use[A-Z]"}"#,
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 3);
    assert_eq!(f[0].data.as_ref().unwrap()["snippet"], "useFoo");
}

#[test]
fn symbol_scan_negate_flags_exported_functions_not_matching_naming_convention() {
    let f = scan_symbols(
        "f.ts",
        vec![
            symbol("handleClick", SourceSymbolKind::Function, 1, true),
            symbol("onClick", SourceSymbolKind::Function, 5, true),
            symbol("helper", SourceSymbolKind::Function, 9, false), // not exported -> filtered out
        ],
        r#"{"type":"symbol-scan","file_pattern":"\\.ts$","kind":"function","exported":true,"name_pattern":"^handle[A-Z]","negate":true}"#,
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 5);
    assert_eq!(f[0].data.as_ref().unwrap()["snippet"], "onClick");
}

#[test]
fn symbol_scan_kind_and_exported_filters_combine_with_and() {
    let f = scan_symbols(
        "f.ts",
        vec![
            symbol("Widget", SourceSymbolKind::Class, 1, true),
            symbol("Config", SourceSymbolKind::Type, 4, true), // wrong kind -> excluded
            symbol("widget", SourceSymbolKind::Class, 7, false), // not exported -> excluded
        ],
        r#"{"type":"symbol-scan","file_pattern":"\\.ts$","kind":"class","exported":true}"#,
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 1);
}

#[test]
fn symbol_scan_negate_with_no_name_pattern_behaves_as_plain_and_filter() {
    let f = scan_symbols(
        "f.ts",
        vec![
            symbol("a", SourceSymbolKind::Function, 1, true),
            symbol("b", SourceSymbolKind::Function, 2, false),
        ],
        r#"{"type":"symbol-scan","file_pattern":"\\.ts$","exported":true,"negate":true}"#,
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 1);
}

// --- io-scan ---

#[test]
fn io_scan_negate_flags_provide_keys_not_matching_the_pattern() {
    let io = IoFacts {
        provides: vec![
            io_provide("http", "GET /authen/getUserInfo", 10),
            io_provide("http", "GET /api/v1/users", 20),
        ],
        consumes: vec![],
    };
    let f = scan_io(
        "f.ts",
        io,
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","kind":"http","key_pattern":"^GET /api/v[0-9]+/","negate":true}"#,
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 10);
    assert_eq!(
        f[0].data.as_ref().unwrap()["snippet"],
        "GET /authen/getUserInfo"
    );
}

#[test]
fn io_scan_key_none_never_matches_pattern_so_negate_flags_it() {
    let io = IoFacts {
        provides: vec![],
        consumes: vec![
            io_consume("http", None, 5), // unresolved dynamic target
            io_consume("http", Some("GET /api/v1/orders"), 6), // versioned, compliant
        ],
    };
    let f = scan_io(
        "f.ts",
        io,
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"consumes","kind":"http","key_pattern":"^GET /api/v[0-9]+/","negate":true}"#,
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 5);
    assert_eq!(f[0].data.as_ref().unwrap()["snippet"], "<unresolved>");
}

#[test]
fn io_scan_kind_filter_excludes_non_matching_entries() {
    let io = IoFacts {
        provides: vec![
            io_provide("http", "GET /a", 1),
            io_provide("queue", "topic:jobs", 2),
        ],
        consumes: vec![],
    };
    let f = scan_io(
        "f.ts",
        io,
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","kind":"queue"}"#,
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].data.as_ref().unwrap()["snippet"], "topic:jobs");
}

#[test]
fn io_scan_any_direction_scans_both_provides_and_consumes() {
    let io = IoFacts {
        provides: vec![io_provide("http", "GET /a", 1)],
        consumes: vec![io_consume("http", Some("GET /b"), 2)],
    };
    let f = scan_io(
        "f.ts",
        io,
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"any","kind":"http"}"#,
    );
    assert_eq!(f.len(), 2);
}

#[test]
fn io_scan_skips_files_with_no_io_projection() {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        rel: "f.ts".into(),
        text: String::new(),
        symbols: vec![],
        io: None,
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    let pack = io_scan_pack(r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"any"}"#);
    assert!(eval_pack(&pack, &ctx).is_empty());
}
