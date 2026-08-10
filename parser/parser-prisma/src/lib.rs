//! zzop-parser-prisma — Prisma Schema Language (PSL) frontend. Line-based parser turning schema.prisma
//! into the core schema IR (`SchemaModel[]`) — a grammar the TypeScript parser does not handle. Extracts
//! model blocks, field declarations, field attributes, and @@map/@@unique/@@index. `parse_schema_enums` is
//! a separate top-level pass extracting `enum` blocks into `SchemaEnum[]`, kept out of `parse_schema`'s
//! return shape so existing call sites stay untouched; a caller that also needs enum data calls both.

/// Cache key ingredient for `zzop-cache` (see `zzop_parser_typescript::PARSER_FINGERPRINT`'s doc for the
/// scheme this mirrors). This crate has no external version pin to track — the parser is a local
/// regex/line scanner, not a wrapped third-party crate.
///
/// **This string is an ID, not a version — it no longer has to be bumped.** `crates/engine/build.rs`
/// hashes this crate's whole dependency closure into the cache key beside it, so a change to any
/// source here invalidates on its own. What is left is the part a person reads in a cache path or a
/// bug report: which frontend parsed the file. Change it when the FRONTEND changes; correctness no
/// longer depends on remembering.
pub const PARSER_FINGERPRINT: &str = "prisma/0.22.0";

mod analysis;
mod parse;

pub use analysis::{build_common_ir, model_decl_line, DEFAULT_PRISMA_CLIENT_GETTER_FN};
pub use parse::{parse_schema, parse_schema_enums};

#[cfg(test)]
mod orchestrator_tests;
#[cfg(test)]
mod tests;

use zzop_core::recognizer::{channel, FrameworkRecognizer};

/// Frameworks this parser recognizes — see [`zzop_core::recognizer`].
///
/// A DECLARATION FORMAT has no framework tier: `.prisma` IS the schema, so there is no middleware
/// layer to recognize and the layer-2 population (`parser-expansion.md` §0) is empty BY CONSTRUCTION,
/// not by omission. The single row names the format itself so a reader scanning for "what does zzop
/// know" finds it, and so an empty list is never mistaken for an undeclared parser.
pub const FRAMEWORK_RECOGNIZERS: &[FrameworkRecognizer] = &[FrameworkRecognizer {
    framework: "prisma schema",
    extensions: &["prisma"],
    emits: &[channel::DB],
}];
