//! Per-file loop-body line spans (1-based, inclusive), feeding `MethodScan::trigger_in_loop` —
//! mirrors `zzop_parser_typescript::extract_loop_spans`/`zzop_parser_go::lang::loop_spans`'s
//! cross-language contract exactly (see `zzop_core::dsl::SourceFile::loop_spans`'s doc for the
//! shared definition every parser projects onto): a call sitting textually inside one of these spans
//! is proven to run once per iteration, not just "somewhere in the same function".
//!
//! ## What is a span here — the eager/lazy boundary
//! Rust's three loop EXPRESSIONS: `for ... in ...`, `while`/`while let`, and `loop`. Every one is
//! the node's OWN whole span, header line included — a `while` condition re-evaluates per iteration,
//! the same header-inclusive convention TS/Go pin (`for`'s iterator expression evaluates once, but
//! spans are line-granular and the construct is one syntactic unit, same accepted convention as
//! every sibling's `for` header).
//!
//! **Iterator-adapter closures are deliberately NOT spans**: `xs.iter().map(|x| f(x))` is LAZY — an
//! adapter's closure runs zero times unless the iterator is consumed (`collect`/`sum`/a `for` loop/
//! ...), so "proven to run once per iteration" cannot be honored for the closure's body. This module
//! stays silent there, the same call Python makes for generator expressions, Java for Stream
//! lambdas, and C# for LINQ (and unlike TS's `ARRAY_ITERATION_METHODS` arm, whose `.map`/`.forEach`
//! are EAGER). Distinguishing an eager consumer chain (`.map(f).collect()`) would require judging
//! the whole method chain's terminal — a guess this crate's never-guess discipline rules out for v1.
//! A closure inside a recorded loop is still covered textually by that loop's span, which is the
//! correct containment either way.
//!
//! ## Scope note: macros
//! A loop written inside a macro invocation's argument tokens (`tokio::select! { ... => loop { … } }`)
//! is invisible — syn parses macro arguments as an opaque `TokenStream` (crate root doc's shared
//! macro scope note), so no span is emitted there. Degrade direction: a missing span only makes
//! `trigger_in_loop` rules quieter, never wrong.

use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{ExprForLoop, ExprLoop, ExprWhile};

/// Extract this file's loop-body line spans — see module doc. Empty for an unparseable file (the
/// same degrade-to-nothing contract every `extract_*` in this crate upholds). `_rel` is unused (syn
/// parsing needs no filename) — kept to match the engine's uniform `(rel, text)` call convention.
pub fn extract_loop_spans(_rel: &str, text: &str) -> Vec<(u32, u32)> {
    let Some(file) = crate::parse_file(text) else {
        return Vec::new();
    };
    let mut collector = LoopSpanCollector { out: Vec::new() };
    collector.visit_file(&file);
    collector.out
}

/// Preorder walk (a node is recorded, THEN its children are visited), so nested loops emit
/// outer-before-inner — the same source-order convention the TS/Go siblings' nested-loop tests pin.
struct LoopSpanCollector {
    out: Vec<(u32, u32)>,
}

impl LoopSpanCollector {
    /// Whole-node `(start, end)` lines via proc-macro2's span-locations feature — the same mechanism
    /// `lang::test_spans` derives its inclusive item spans from (crate root doc's "Line numbers").
    fn push<T: Spanned>(&mut self, node: &T) {
        let span = node.span();
        self.out
            .push((span.start().line as u32, span.end().line as u32));
    }
}

impl<'ast> Visit<'ast> for LoopSpanCollector {
    fn visit_expr_for_loop(&mut self, n: &'ast ExprForLoop) {
        self.push(n);
        visit::visit_expr_for_loop(self, n);
    }

    /// Covers `while let ...` too — the pattern condition is an `Expr::Let` inside the same node.
    fn visit_expr_while(&mut self, n: &'ast ExprWhile) {
        self.push(n);
        visit::visit_expr_while(self, n);
    }

    fn visit_expr_loop(&mut self, n: &'ast ExprLoop) {
        self.push(n);
        visit::visit_expr_loop(self, n);
    }
}

#[cfg(test)]
mod tests;
