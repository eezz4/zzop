//! Arbitrary input must not panic the Go frontend. Why that matters even though
//! `crates/engine/src/pipeline/parsers.rs` catches the panic, what the generated inputs look like, and
//! what is deliberately out of scope: see `parser/tests/input_strategy.rs`'s module doc — the
//! single owner of that rationale for all eight frontends.
//!
//! Entry points hammered: every `pub` function `lib.rs` re-exports that takes text. `go_package_dir_of`
//! takes an import path and a module path rather than source, so it is fed the generated string in both
//! positions — prefix arithmetic on two unrelated strings is exactly its failure shape.

#[path = "../../tests/input_strategy.rs"]
mod input_strategy;

use proptest::prelude::*;
use zzop_parser_go as go;

/// 128 cases, measured at 0.96 s. A tree-sitter frontend re-parsing once per entry point is ~7 ms a
/// case, two orders of magnitude above the regex scanners — hence the low count for the same wall-time
/// budget. How the per-crate budget is set: `input_strategy`'s "Case budget" section.
const CASES: u32 = 128;

/// One call per public text-taking entry point. A panic in any of them fails the property.
fn hammer(rel: &str, text: &str) {
    let _ = go::count_loc(text);
    let _ = go::parse_go(rel, text);
    let _ = go::parse_symbols(rel, text);
    let _ = go::parse_imports(text);
    let _ = go::parse_local_identifier_refs(text);
    let _ = go::extract_loop_spans(rel, text);
    let _ = go::extract_go_router_fragments(rel, text);
    let _ = go::extract_go_http_consumes(rel, text);
    let _ = go::extract_gorm_db_table_provides(rel, text);
    let _ = go::extract_gorm_db_table_consumes(rel, text);
    let _ = go::go_package_dir_of(text, "example.com/mod");
    let _ = go::go_package_dir_of("example.com/mod/pkg", text);
    let _ = go::go_package_dir_of(text, text);
}

proptest! {
    #![proptest_config(input_strategy::config(CASES))]

    #[test]
    fn no_public_entry_point_panics(text in input_strategy::source_text()) {
        hammer("cmd/server/main.go", &text);
    }
}

#[test]
fn no_public_entry_point_panics_on_fixed_edge_cases() {
    for text in input_strategy::FIXED_EDGE_CASES {
        hammer("cmd/server/main.go", text);
    }
}
