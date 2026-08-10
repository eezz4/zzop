//! The three AST-derived per-file SPAN projections, and the per-language table that decides which of
//! them a given file gets. Ground truth for `crates/engine/tests/rule_contracts/capability_matrix.rs`'s
//! `loop_spans` column and for the two "deliberately not a column" notes beside it.
//!
//! ## Why they live together
//! All three answer the same shape of question ("which LINES of this file are X") and all three share
//! one gate: a real, non-degraded parse of a language that projects the fact, empty otherwise. Reading
//! them side by side is how a reader sees that the coverage differences (`loop_spans` every structural
//! statement-loop language — TS/Go/Python/Java/C#/Rust — while Prisma/SQL have no loop syntax to span;
//! `function_spans` TypeScript, `test_spans` Rust) are per-FACT choices rather than an accident of where
//! each one happened to get written. They were three adjacent `let ... = match language` blocks in
//! `fresh.rs` until that file hit this repo's 300-line limit; the seam was already here.
//!
//! Note: `loop_spans` is produced for more languages than consume it, and the gap is now **Rust only**.
//! `reliability/console-in-loop` admits `ts|tsx|js|jsx|mjs|cjs|py|go|java|cs`, so Python/Java/C# — which
//! this note listed as silent alongside Rust until 2026-08-06 — are consumed. Rust is not, and that is
//! deliberate: `println!` is a CLI's normal output, so the console family never accepts `.rs`
//! (`projection-contract.md` records that as a permanent blank, not a gap to close).
//!
//! The note exists at all because this exact silence, left unrecorded, is how `loop_spans` once sat
//! TS+Go-only without anyone noticing — and it went stale in the same way, by naming languages instead
//! of pointing at the pattern that decides them:
//! `grep -A6 '"console-in-loop"' rules/dsl/reliability/reliability.json`.
//!
//! ## The degrade direction is NOT uniform, and that is the important part
//! - `loop_spans` absent ⇒ `MethodScan::trigger_in_loop` SILENTLY SKIPS the file (a rule using it cannot
//!   fire).
//! - `function_spans` absent ⇒ `MethodScan::after_in_same_function` degrades to a NO-OP (the rule keeps
//!   its coarser pre-gate behavior — it over-reports rather than going quiet).
//! - `test_spans` absent ⇒ nothing is SUBTRACTED, so every rule keeps its full judgment.
//!
//! Each field's own doc on `zzop_core::dsl::SourceFile` states its direction; they are collected here
//! because "empty" means something different in each row and a reader scanning three identical-looking
//! `_ => Vec::new()` arms would reasonably assume otherwise.

use crate::dispatch::Language;

/// One file's span projections, grouped so `compute_fresh_artifact` threads one value instead of three
/// same-typed ones it could silently transpose — the same reason `findings::SpanFacts` (the borrowed
/// view of this, handed to the DSL pass) exists.
pub(super) struct ProjectedSpans {
    pub loop_spans: Vec<(u32, u32)>,
    pub function_spans: Vec<(u32, u32)>,
    pub test_spans: Vec<(u32, u32)>,
}

/// Project all three for one file. `degraded` is the caller's parse verdict: every fact here is
/// AST-derived, so a file whose parse failed contributes nothing rather than a guess — the same
/// `symbols`-style gate, never the raw-text regex-scan gate `field_usage_tokens` uses.
pub(super) fn project(
    language: Option<Language>,
    degraded: bool,
    rel: &str,
    text: &str,
) -> ProjectedSpans {
    ProjectedSpans {
        // `loop-spans-v1` — `MethodScan::trigger_in_loop`'s substrate. All six structural
        // statement-loop languages (TS/Go/Python/Java/C#/Rust); the remaining blanks are Prisma/SQL
        // (no loop syntax exists) and the lexical fallback. Each parser's own module doc pins the
        // eager/lazy arm boundary (`SourceFile::loop_spans`'s doc owns the shared rule): eager
        // callback/comprehension forms are spans (TS array-iteration callbacks, Python
        // comprehensions), lazy ones are silent (Python genexp, Rust iterator adapters, Java
        // Streams, C# LINQ).
        loop_spans: match language {
            Some(Language::TypeScript) if !degraded => {
                zzop_parser_typescript::extract_loop_spans(rel, text)
            }
            Some(Language::Go) if !degraded => zzop_parser_go::extract_loop_spans(rel, text),
            Some(Language::Python) if !degraded => {
                zzop_parser_python_3::extract_loop_spans(rel, text)
            }
            Some(Language::Java21) if !degraded => {
                zzop_parser_java_21::extract_loop_spans(rel, text)
            }
            Some(Language::CSharp) if !degraded => {
                zzop_parser_csharp::extract_loop_spans(rel, text)
            }
            Some(Language::Rust) if !degraded => zzop_parser_rust::extract_loop_spans(rel, text),
            _ => Vec::new(),
        },
        // `function-spans-v1` — `MethodScan::after_in_same_function`'s substrate. TypeScript only; every
        // other language is a documented matrix blank where the gate degrades to a no-op, not silence.
        function_spans: match language {
            Some(Language::TypeScript) if !degraded => {
                zzop_parser_typescript::extract_function_spans(rel, text)
            }
            _ => Vec::new(),
        },
        // `test-spans-v1` — the SUBTRACTIVE one (`zzop_core::dsl::eval`'s test-region gate). Rust only,
        // because Rust is the only language this workspace parses whose dominant test convention
        // (`#[cfg(test)] mod tests`) lives INSIDE the shipping file, where no path pattern can reach it.
        // Every other language names its tests in the PATH, which the DSL packs' `${test-paths-stories}`
        // exclusion already covers — so a blank here is a statement that the path axis suffices for that
        // language, not a missing capability.
        test_spans: match language {
            Some(Language::Rust) if !degraded => zzop_parser_rust::extract_test_spans(rel, text),
            _ => Vec::new(),
        },
    }
}

impl ProjectedSpans {
    /// The all-empty projection — what an oversized (lexical-fallback) file gets, where there is no AST
    /// to derive anything from but DSL line-scan rules still run over the text.
    pub(super) fn none() -> Self {
        ProjectedSpans {
            loop_spans: Vec::new(),
            function_spans: Vec::new(),
            test_spans: Vec::new(),
        }
    }

    /// Borrowed view for the DSL pass.
    pub(super) fn facts(&self) -> crate::pipeline::findings::SpanFacts<'_> {
        crate::pipeline::findings::SpanFacts {
            loop_spans: &self.loop_spans,
            function_spans: &self.function_spans,
            test_spans: &self.test_spans,
        }
    }
}
