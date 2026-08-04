//! Per-file loop-body line spans (1-based, inclusive), feeding `MethodScan::trigger_in_loop` —
//! mirrors `zzop_parser_typescript::extract_loop_spans`/`zzop_parser_go::lang::loop_spans`'s
//! cross-language contract exactly (see `zzop_core::dsl::SourceFile::loop_spans`'s doc for the shared
//! definition every parser projects onto): a call sitting textually inside one of these spans is
//! proven to run once per iteration, not just "somewhere in the same function".
//!
//! ## What is a span here — the eager/lazy boundary
//! - **Statement loops** — `for`/`async for`/`while`: always a span (header line included — the
//!   `while` condition and the `for` iterator-expression line re-evaluate per iteration/once, same
//!   header-inclusive convention as TS/Go). The loop's `else:` block is deliberately EXCLUDED — it
//!   runs at most ONCE (after normal completion), so a span covering it would claim per-iteration
//!   execution for code that provably has none; the span ends at the loop BODY's last statement.
//! - **Eager comprehensions** — list `[...]`, set `{...}`, dict `{k: v ...}`: a span. Evaluating the
//!   comprehension expression itself runs its element/condition code once per iteration immediately —
//!   the same "eager, proven per-iteration" ground the TS side's `ARRAY_ITERATION_METHODS` callback
//!   arm stands on. The whole comprehension node is the span (like a loop's header-inclusive span,
//!   the `for x in <iterable>` clause line is covered even though the iterable evaluates once — spans
//!   are line-granular and the construct is one syntactic unit). A comprehension that starts and ends
//!   on ONE line is NOT emitted: a `(n, n)` span cannot be told apart, line-granularly, from the
//!   one-shot calls sharing that line (`print("ids:", [r.id for r in rows])` — the `print` runs once,
//!   but it sits on the comprehension's only line), so emitting it claims per-iteration execution
//!   for code that provably has none. Skipping is the never-guess direction (intended
//!   under-reporting: a genuine per-iteration call inside a one-line comprehension is lost).
//!   Statement loops keep their one-line spans (`for x in xs: use(x)`) — the residual line-share
//!   ambiguity there is published, not fixed. `SourceFile::loop_spans`'s doc owns the shared rule;
//!   the TS callback arm makes the same call.
//! - **Generator expressions** — `(f(x) for x in xs)`: NEVER a span. A genexp is LAZY: evaluating the
//!   expression only builds a generator object; if it is never consumed, the element code runs ZERO
//!   times — "proven to run once per iteration" cannot be honored, so this module stays silent, the
//!   same call Rust's `.iter().map(...)` / Java Streams / C# LINQ sides make for their lazy adapters.
//!   The walk still DESCENDS into a genexp (an eager comprehension nested inside one is recorded on
//!   its own conditional terms, exactly like a loop inside a function body that may never be called).

use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::Ranged;

/// Extract this file's loop-body line spans — see module doc. Empty on parse failure (never panics),
/// the same degrade-to-nothing contract every `extract_*`/`parse_*` in this crate upholds. `_rel` is
/// unused (ruff parsing needs no filename) — kept to match the engine's uniform `(rel, text)` call
/// convention across every language's `extract_loop_spans`.
pub fn extract_loop_spans(_rel: &str, text: &str) -> Vec<(u32, u32)> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let idx = crate::LineIndex::new(text);
    let mut collector = LoopSpanCollector {
        idx: &idx,
        out: Vec::new(),
    };
    for stmt in &module.body {
        collector.visit_stmt(stmt);
    }
    collector.out
}

/// Preorder walk (a node is recorded, THEN its children are visited), so nested loops emit
/// outer-before-inner — the same source-order convention the TS/Go siblings' nested-loop tests pin.
struct LoopSpanCollector<'a> {
    idx: &'a crate::LineIndex,
    out: Vec<(u32, u32)>,
}

impl LoopSpanCollector<'_> {
    fn push(&mut self, start: ruff_text_size::TextSize, end: ruff_text_size::TextSize) {
        self.out
            .push((self.idx.line_of(start), self.idx.line_of(end)));
    }
}

impl<'a> Visitor<'a> for LoopSpanCollector<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // `for`/`async for` — one `StmtFor` node either way (`is_async` is a flag, not a kind).
            // End at the BODY's last statement, not the node's own end: the node range covers the
            // `else:` block too, which runs at most once (module doc). An empty body cannot parse
            // (grammar requires at least one statement), so the fallback arm is defensive only.
            Stmt::For(f) => {
                let end = f.body.last().map_or(f.end(), |s| s.end());
                self.push(f.start(), end);
            }
            Stmt::While(w) => {
                let end = w.body.last().map_or(w.end(), |s| s.end());
                self.push(w.start(), end);
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            // Eager comprehensions — a span, but only when MULTILINE: a single-line comprehension's
            // `(n, n)` span cannot prove containment on a line-granular channel (module doc).
            // `Expr::Generator` is deliberately NOT an arm: lazy, never a span (module doc);
            // `walk_expr` below still descends into it.
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) => {
                let (start, end) = (self.idx.line_of(expr.start()), self.idx.line_of(expr.end()));
                if start < end {
                    self.out.push((start, end));
                }
            }
            _ => {}
        }
        walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests;
