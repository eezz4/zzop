//! Arbitrary input must not panic the Java frontend. Why that matters even though
//! `crates/engine/src/pipeline/parsers.rs` catches the panic, what the generated inputs look like, and
//! what is deliberately out of scope: see `parser/tests/input_strategy.rs`'s module doc — the
//! single owner of that rationale for all eight frontends.
//!
//! Entry points hammered: every `pub` function `lib.rs` re-exports that takes text, including the
//! project-level walker (`extract_http_provides_project`), which resolves across a file list rather than
//! within one file and therefore has index arithmetic the per-file extractors do not.

#[path = "../../tests/input_strategy.rs"]
mod input_strategy;

use proptest::prelude::*;
use zzop_parser_java_21 as java;

/// 128 cases, measured at 1.22 s. A tree-sitter frontend re-parsing once per entry point, plus the
/// two-file project walk below, is ~10 ms a case. How the per-crate budget is set: `input_strategy`'s
/// "Case budget" section.
const CASES: u32 = 128;

/// One call per public text-taking entry point. A panic in any of them fails the property.
fn hammer(rel: &str, text: &str) {
    let _ = java::count_loc(text);
    let _ = java::parse_java(rel, text);
    let _ = java::parse_symbols(rel, text);
    let _ = java::parse_imports(text);
    let _ = java::parse_calls(rel, text);
    let _ = java::parse_local_identifier_refs(text);
    let _ = java::extract_loop_spans(rel, text);
    let _ = java::extract_call_sites(rel, text);
    let _ = java::extract_string_literals(rel, text);
    let _ = java::java_package_of(text);
    let _ = java::java_type_names(text);
    let _ = java::extract_http_provides(rel, text);
    let _ = java::extract_java_http_consumes(rel, text);
    let _ = java::extract_jpa_db_table_provides(rel, text);
    let _ = java::extract_spring_guarded_lines(rel, text);
    let _ = java::extract_spring_security_posture(rel, text);

    // Two files, same text: the project walker's cross-file `(package, type)` index has to survive two
    // rows that collide on every key it builds.
    let files = vec![
        (rel.to_string(), text.to_string()),
        (format!("other/{rel}"), text.to_string()),
    ];
    let _ = java::extract_http_provides_project(&files);
}

proptest! {
    #![proptest_config(input_strategy::config(CASES))]

    #[test]
    fn no_public_entry_point_panics(text in input_strategy::source_text()) {
        hammer("src/main/java/com/example/App.java", &text);
    }
}

#[test]
fn no_public_entry_point_panics_on_fixed_edge_cases() {
    for text in input_strategy::FIXED_EDGE_CASES {
        hammer("src/main/java/com/example/App.java", text);
    }
}
