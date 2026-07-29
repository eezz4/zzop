//! Arbitrary input must not panic the SQL frontend. Why that matters even though
//! `crates/engine/src/pipeline/parsers.rs` catches the panic, what the generated inputs look like, and
//! what is deliberately out of scope: see `parser/tests/input_strategy.rs`'s module doc — the
//! single owner of that rationale for all eight frontends.
//!
//! Entry points hammered: every `pub` function this crate re-exports that takes text
//! (`lib.rs`'s two `pub use` lines). Both directions of the `db-table` channel.

#[path = "../../tests/input_strategy.rs"]
mod input_strategy;

use proptest::prelude::*;

/// 4096 cases, measured at 0.49 s. The highest count of the eight: this crate is a line/regex scanner
/// with no AST step, so it buys ~30x the exploration of the tree-sitter frontends for the same wall
/// time. How the per-crate budget is set: `input_strategy`'s "Case budget" section.
const CASES: u32 = 4096;

/// One call per public text-taking entry point. A panic in any of them fails the property.
fn hammer(rel: &str, text: &str) {
    let _ = zzop_parser_sql::extract_db_table_provides(rel, text);
    let _ = zzop_parser_sql::extract_statement_table_refs(text);
}

proptest! {
    #![proptest_config(input_strategy::config(CASES))]

    #[test]
    fn no_public_entry_point_panics(text in input_strategy::source_text()) {
        hammer("db/migrations/0001_init.sql", &text);
    }
}

#[test]
fn no_public_entry_point_panics_on_fixed_edge_cases() {
    for text in input_strategy::FIXED_EDGE_CASES {
        hammer("db/migrations/0001_init.sql", text);
    }
}
