//! Arbitrary input must not panic the Prisma frontend. Why that matters even though
//! `crates/engine/src/pipeline/parsers.rs` catches the panic, what the generated inputs look like, and
//! what is deliberately out of scope: see `parser/tests/input_strategy.rs`'s module doc — the
//! single owner of that rationale for all eight frontends.
//!
//! Entry points hammered: the four `pub` functions `lib.rs` re-exports that take text. `model_decl_line`
//! is fed the model names `parse_schema` just found in the same input, which is the only way to reach
//! its scan loop with a name that actually occurs.

#[path = "../../tests/input_strategy.rs"]
mod input_strategy;

use proptest::prelude::*;

/// 2048 cases, measured at 0.36 s. A line/regex scanner like the SQL frontend, so the budget buys a lot
/// of cases. How the per-crate budget is set: `input_strategy`'s "Case budget" section.
const CASES: u32 = 2048;

/// One call per public text-taking entry point. A panic in any of them fails the property.
fn hammer(rel: &str, text: &str) {
    let models = zzop_parser_prisma::parse_schema(text, Some(rel), Some("db"));
    let _ = zzop_parser_prisma::parse_schema(text, None, None);
    let _ = zzop_parser_prisma::parse_schema_enums(text);
    let _ = zzop_parser_prisma::build_common_ir("db", &[(rel.to_string(), text.to_string())]);
    for model in models.iter().take(8) {
        let _ = zzop_parser_prisma::model_decl_line(text, &model.name);
    }
    // A name that is NOT in the schema, and one drawn from the raw input, are separate paths.
    let _ = zzop_parser_prisma::model_decl_line(text, "NoSuchModel");
    let _ = zzop_parser_prisma::model_decl_line(text, text);
}

proptest! {
    #![proptest_config(input_strategy::config(CASES))]

    #[test]
    fn no_public_entry_point_panics(text in input_strategy::source_text()) {
        hammer("prisma/schema.prisma", &text);
    }
}

#[test]
fn no_public_entry_point_panics_on_fixed_edge_cases() {
    for text in input_strategy::FIXED_EDGE_CASES {
        hammer("prisma/schema.prisma", text);
    }
}
