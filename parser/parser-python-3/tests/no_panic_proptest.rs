//! Arbitrary input must not panic the Python frontend. Why that matters even though
//! `crates/engine/src/pipeline/parsers.rs` catches the panic, what the generated inputs look like, and
//! what is deliberately out of scope: see `parser/tests/input_strategy.rs`'s module doc — the
//! single owner of that rationale for all eight frontends.
//!
//! Entry points hammered: every `pub` function `lib.rs` re-exports that takes text, including both the
//! built-in-vocabulary and declared-vocabulary forms of the four guard extractors — the declared form is
//! the one a user's config reaches, and it is the newer code path.

#[path = "../../tests/input_strategy.rs"]
mod input_strategy;

use std::collections::BTreeSet;

use proptest::prelude::*;
use zzop_parser_python_3 as py;

/// 512 cases, measured at 0.72 s — ruff re-parses per entry point and there are 22 of them. How the
/// per-crate budget is set: `input_strategy`'s "Case budget" section.
const CASES: u32 = 512;

/// One call per public text-taking entry point. A panic in any of them fails the property.
fn hammer(rel: &str, text: &str) {
    let vocab = py::PythonGuardVocab::built_in();

    let _ = py::count_loc(text);
    let _ = py::parse_python(rel, text);
    let _ = py::parse_symbols(rel, text);
    let _ = py::parse_imports(text);
    let _ = py::parse_calls(rel, text);
    let _ = py::parse_local_identifier_refs(text);
    let _ = py::extract_loop_spans(rel, text);
    let _ = py::extract_call_sites(rel, text);
    let _ = py::extract_string_literals(rel, text);
    // `package_roots` gets the arbitrary text too — a declared entry is untrusted config input and the
    // `=`-split/normalize path must hold up to the same hammering as the specifier itself.
    let _ = py::python_import_candidates(text, None, rel, &[]);
    let _ = py::python_import_candidates(text, Some(text), rel, &[text, "tml=", "backend/"]);

    let _ = py::extract_django_db_table_provides(rel, text);
    let _ = py::extract_django_db_table_consumes(rel, text);
    let _ = py::extract_django_route_fragments(rel, text);
    let _ = py::extract_django_view_guard_classes(text);
    let _ = py::extract_django_view_guard_classes_with_vocab(text, &vocab);
    let _ = py::extract_fastapi_router_fragments(rel, text);
    let _ = py::extract_python_http_consumes(rel, text);
    let _ = py::extract_sqlalchemy_db_table_provides(rel, text);
    let _ = py::extract_sqlalchemy_db_table_consumes(rel, text);

    // The guard-alias set feeds the line extractor, so it is derived from this same input rather than
    // stubbed — a stub would never carry an alias the input actually declares.
    let aliases: BTreeSet<String> = py::extract_python_guard_aliases(text)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let _ = py::extract_python_guard_aliases_with_vocab(text, &vocab);
    let _ = py::extract_fastapi_guarded_lines(rel, text, &aliases);
    let _ = py::extract_fastapi_guarded_lines_with_vocab(rel, text, &aliases, &vocab);
}

proptest! {
    #![proptest_config(input_strategy::config(CASES))]

    #[test]
    fn no_public_entry_point_panics(text in input_strategy::source_text()) {
        hammer("app/views.py", &text);
    }
}

#[test]
fn no_public_entry_point_panics_on_fixed_edge_cases() {
    for text in input_strategy::FIXED_EDGE_CASES {
        hammer("app/views.py", text);
    }
}
