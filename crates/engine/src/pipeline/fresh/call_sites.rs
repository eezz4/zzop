//! The per-file CALL-SITE projection, and the per-language table that decides which files get one.
//! Ground truth for `crates/engine/tests/rule_contracts/capability_matrix.rs`'s `call_sites` column,
//! exactly as its `spans` sibling is for the `loop_spans` one.
//!
//! ## Why it is its own module rather than a fourth arm inside `spans`
//! Every fact in `spans` answers "which LINES of this file are X" and is a `Vec<(u32, u32)>`; a call site
//! is a NAMED fact (`kind` + `callee`) that happens to carry a line. Grouping them would put two
//! different questions behind one word and make `ProjectedSpans` a bag. The gate they share — a real,
//! non-degraded parse of a language that projects the fact, empty otherwise — is stated per module rather
//! than centralized, which is what keeps each module's degrade note honest about its own direction.
//!
//! ## Who fills it
//! Six languages. TypeScript/JavaScript (`zzop_parser_typescript::extract_call_sites`), Python
//! (`zzop_parser_python_3::…`), Go (`zzop_parser_go::…`), Java (`zzop_parser_java_21::…`) and C#
//! (`zzop_parser_csharp::…`) produce both families (`console-write` + `env-read`); Rust
//! (`zzop_parser_rust::…`) produces `env-read` ONLY — its module doc owns the `println!` judgment (a
//! fact-layer console write whose consuming rules never admit `.rs`, so producing it would carry a
//! fact nothing can read). Each producer module's own doc owns that language's recognized idioms and
//! deliberate silences. The remaining arms (Prisma, SQL) are empty because those languages have no
//! console write or environment read to write down; an empty arm means the rules reading these kinds
//! are silent there, not that those files are clean.
//!
//! What must NOT drift apart is a producer and its consuming RULE — the `loop_spans` lesson (six
//! languages projecting, two languages' rules reading, four silent) applies to the kind/language axis
//! here, and the matrix's rule-side sweep is the machine that catches it: a `CallScan` rule whose
//! `file_pattern` admits an environment this table leaves empty fails that sweep as FOREVER-SILENT. The
//! duty runs the other way too — a language ARM added here must flip its `call_sites` cell in the
//! capability matrix in the same change, or the declaration understates the build.
//!
//! ## Degrade direction: RECALL
//! Empty `call_sites` ⇒ every `CallScan` rule SILENTLY SKIPS the file. It cannot over-report; it can only
//! fail to report. Same direction as `loop_spans`, and the opposite of `spans`' other two members — see
//! `zzop_core::dsl::SourceFile::call_sites`.

use zzop_core::CallSite;

use crate::dispatch::Language;

/// Project this file's call sites. `degraded` is the caller's parse verdict, threaded for the same reason
/// `spans::project` takes it: these are AST-derived facts, so a file whose parse failed contributes
/// nothing rather than a lexical guess. That gate is also where the transfer from a line scan to this
/// channel COSTS something — a file swc cannot parse used to still yield line-scan hits and now yields
/// nothing — which is the recall direction this module's degrade note names.
pub(super) fn project(
    language: Option<Language>,
    degraded: bool,
    rel: &str,
    text: &str,
) -> Vec<CallSite> {
    // `call-sites-v1` — `Matcher::CallScan`'s substrate. A producer joins by adding one arm here,
    // flipping its row in the capability matrix, and landing the rule that reads the kind in the same
    // change (see this module's doc).
    match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::extract_call_sites(rel, text)
        }
        Some(Language::Python) if !degraded => zzop_parser_python_3::extract_call_sites(rel, text),
        Some(Language::Go) if !degraded => zzop_parser_go::extract_call_sites(rel, text),
        Some(Language::Java21) if !degraded => zzop_parser_java_21::extract_call_sites(rel, text),
        Some(Language::CSharp) if !degraded => zzop_parser_csharp::extract_call_sites(rel, text),
        Some(Language::Rust) if !degraded => zzop_parser_rust::extract_call_sites(rel, text),
        _ => Vec::new(),
    }
}
