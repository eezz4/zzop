//! Arbitrary input must not panic the C# frontend. Why that matters even though
//! `crates/engine/src/pipeline/parsers.rs` catches the panic, what the generated inputs look like, and
//! what is deliberately out of scope: see `parser/tests/input_strategy.rs`'s module doc — the
//! single owner of that rationale for all eight frontends.
//!
//! Entry points hammered: every `pub` function `lib.rs` re-exports that takes text, including the
//! project-level walker (`extract_csharp_http_provides_project`), which resolves across a file list
//! rather than within one file.

#[path = "../../tests/input_strategy.rs"]
mod input_strategy;

use proptest::prelude::*;
use zzop_parser_csharp as cs;

/// 64 cases, measured at 1.13 s — the most expensive per case of the eight (~18 ms), so the lowest
/// count for the same wall-time budget. How the per-crate budget is set: `input_strategy`'s "Case
/// budget" section.
const CASES: u32 = 64;

/// One call per public text-taking entry point. A panic in any of them fails the property.
fn hammer(rel: &str, text: &str) {
    let _ = cs::count_loc(text);
    let _ = cs::parse_csharp(rel, text);
    let _ = cs::parse_symbols(rel, text);
    let _ = cs::parse_imports(text);
    let _ = cs::parse_local_identifier_refs(text);
    let _ = cs::csharp_namespaces_of(text);
    let _ = cs::extract_csharp_http_provides(rel, text);
    let _ = cs::extract_csharp_http_consumes(rel, text);

    // Two files, same text: the project walker's cross-file class index has to survive two rows that
    // collide on every key it builds.
    let files = vec![
        (rel.to_string(), text.to_string()),
        (format!("Other/{rel}"), text.to_string()),
    ];
    let _ = cs::extract_csharp_http_provides_project(&files);
}

proptest! {
    #![proptest_config(input_strategy::config(CASES))]

    #[test]
    fn no_public_entry_point_panics(text in input_strategy::source_text()) {
        hammer("Api/Controllers/OrdersController.cs", &text);
    }
}

#[test]
fn no_public_entry_point_panics_on_fixed_edge_cases() {
    for text in input_strategy::FIXED_EDGE_CASES {
        hammer("Api/Controllers/OrdersController.cs", text);
    }
}
