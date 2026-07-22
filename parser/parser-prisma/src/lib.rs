//! zzop-parser-prisma — Prisma Schema Language (PSL) frontend. Line-based parser turning schema.prisma
//! into the core schema IR (`SchemaModel[]`) — a grammar the TypeScript parser does not handle. Extracts
//! model blocks, field declarations, field attributes, and @@map/@@unique/@@index. `parse_schema_enums` is
//! a separate top-level pass extracting `enum` blocks into `SchemaEnum[]`, kept out of `parse_schema`'s
//! return shape so existing call sites stay untouched; a caller that also needs enum data calls both.

/// Cache key ingredient for `zzop-cache` (see `zzop_parser_typescript::PARSER_FINGERPRINT`'s doc for the
/// scheme this mirrors). This crate has no external version pin to track (the parser is a local regex/line
/// scanner, not a wrapped third-party crate) — bump the trailing `/vN` counter whenever `parse_schema`'s
/// projection logic OR `build_common_ir`'s bridge output changes for the same schema text.
///
/// - `v2`: `build_common_ir` now ALSO emits a `(kind="db-table", key="table:<accessor-cased name>")` io
///   PROVIDE per model (see `analysis::accessor_casing`'s doc for the canonical casing choice, joined
///   against `zzop_parser_typescript::adapters::db_table_consume`'s bare-receiver consume follow-up).
///   Strictly additive (new `MinimalIr::io` field populated, no existing field's output changes), but
///   cached entries from before this marker must not be served as fresh since they lack the new facts.
pub const PARSER_FINGERPRINT: &str = "prisma/0.21.0";

mod analysis;
mod discover;
mod parse;

pub use analysis::{
    build_common_ir, model_decl_line, prisma_schema_analysis, DEFAULT_PRISMA_CLIENT_GETTER_FN,
};
pub use discover::find_prisma_schemas;
pub use parse::{parse_schema, parse_schema_enums};

#[cfg(test)]
mod orchestrator_tests;
#[cfg(test)]
mod tests;
