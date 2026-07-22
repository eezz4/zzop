//! `Matcher::SymbolScan` tests — per-file declaration queries, unaffected by the io-scan whole-tree
//! redesign (see the `tests_ir_scan` module doc).

use crate::ir::SourceSymbolKind;

use super::super::test_support::{scan_symbols, symbol};

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
