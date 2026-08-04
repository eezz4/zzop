//! The per-file BOUND-STRING-LITERAL projection, and the per-language table that decides which files
//! get one. Ground truth for `crates/engine/tests/rule_contracts/capability_matrix.rs`'s
//! `string_literals` column, exactly as `call_sites` beside it is for its own.
//!
//! ## Who fills it
//! All six structural-parser languages, landed together in the A17 wave: TypeScript/JavaScript, Python,
//! Java, C#, Go, Rust — each via its crate's `extract_string_literals`, whose module doc owns that
//! language's recognized binding shapes and deliberate silences. Prisma and SQL have no named
//! string-binding declaration to project (the same statement their `call_sites` blanks make).
//!
//! ## Degrade direction: RECALL
//! Empty `string_literals` ⇒ every `LiteralScan` rule SILENTLY SKIPS the file — same direction as
//! `call_sites`, see `zzop_core::dsl::SourceFile::string_literals`. The `!degraded` gate is where this
//! projection COSTS something relative to the line-scan it complements: a file no parser can read used
//! to still yield line-scan hits and contributes nothing here.

use zzop_core::BoundStringLiteral;

use crate::dispatch::Language;

/// Project this file's bound string literals. `degraded` is the caller's parse verdict, threaded for
/// the same reason `call_sites::project` takes it: these are AST-derived facts, so a failed parse
/// contributes nothing rather than a lexical guess.
pub(super) fn project(
    language: Option<Language>,
    degraded: bool,
    rel: &str,
    text: &str,
) -> Vec<BoundStringLiteral> {
    // `string-literals-v1` — `Matcher::LiteralScan`'s substrate. A producer joins by adding one arm
    // here and flipping its row in the capability matrix in the same change.
    match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::extract_string_literals(rel, text)
        }
        Some(Language::Python) if !degraded => {
            zzop_parser_python_3::extract_string_literals(rel, text)
        }
        Some(Language::Java21) if !degraded => {
            zzop_parser_java_21::extract_string_literals(rel, text)
        }
        Some(Language::CSharp) if !degraded => {
            zzop_parser_csharp::extract_string_literals(rel, text)
        }
        Some(Language::Go) if !degraded => zzop_parser_go::extract_string_literals(rel, text),
        Some(Language::Rust) if !degraded => zzop_parser_rust::extract_string_literals(rel, text),
        _ => Vec::new(),
    }
}
