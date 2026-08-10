//! Arbitrary input must not panic the Rust frontend. Why that matters even though
//! `crates/engine/src/pipeline/parsers.rs` catches the panic, what the generated inputs look like, and
//! what is deliberately out of scope: see `parser/tests/input_strategy.rs`'s module doc — the
//! single owner of that rationale for all eight frontends.
//!
//! Entry points hammered: every `pub` function `lib.rs` re-exports that takes text. The declared-vocabulary
//! form of `parse_extractor_guards` is included with an arbitrary prefix list, because
//! `vocabulary.rustOptionalExtractorPrefixes` is user config and the empty list is a real setting.

#[path = "../../tests/input_strategy.rs"]
mod input_strategy;

use proptest::prelude::*;
use zzop_parser_rust as rs;

/// 2048 cases, measured at 0.52 s. `syn` rejects malformed input early and cheaply, so the budget buys
/// many cases. How the per-crate budget is set: `input_strategy`'s "Case budget" section.
const CASES: u32 = 2048;

/// One call per public text-taking entry point. A panic in any of them fails the property.
fn hammer(rel: &str, text: &str) {
    let _ = rs::count_loc(text);
    let _ = rs::parse_rust(rel, text);
    let _ = rs::parse_symbols(rel, text);
    let _ = rs::parse_imports(text);
    let _ = rs::parse_calls(rel, text);
    let _ = rs::parse_local_identifier_refs(text);
    let _ = rs::extract_loop_spans(rel, text);
    let _ = rs::extract_call_sites(rel, text);
    let _ = rs::extract_string_literals(rel, text);
    let _ = rs::rust_import_candidates(text, rel, &std::collections::BTreeSet::new());
    let _ = rs::extract_axum_router_fragments(rel, text);
    let _ = rs::extract_rust_http_consumes(rel, text);
    let _ = rs::extract_rust_raw_sql_db_table_consumes(rel, text);
    let _ = rs::extract_test_spans(rel, text);

    let built_in = rs::RustGuardVocab {
        optional_extractor_prefixes: rs::RUST_OPTIONAL_EXTRACTOR_PREFIXES,
    };
    let _ = rs::parse_extractor_guards(rel, text, &built_in);
    let declared = rs::RustGuardVocab {
        optional_extractor_prefixes: &[],
    };
    let _ = rs::parse_extractor_guards(rel, text, &declared);
    let prefixes = [text];
    let from_input = rs::RustGuardVocab {
        optional_extractor_prefixes: &prefixes,
    };
    let _ = rs::parse_extractor_guards(rel, text, &from_input);
}

proptest! {
    #![proptest_config(input_strategy::config(CASES))]

    #[test]
    fn no_public_entry_point_panics(text in input_strategy::source_text()) {
        hammer("src/lib.rs", &text);
    }
}

#[test]
fn no_public_entry_point_panics_on_fixed_edge_cases() {
    for text in input_strategy::FIXED_EDGE_CASES {
        hammer("src/lib.rs", text);
    }
}
