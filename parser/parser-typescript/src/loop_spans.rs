//! Per-file loop-body line spans — `for`/`while`/`do-while` statement spans plus recognized
//! array-iteration callback-argument spans; feeds `MethodScan::trigger_in_loop`. The
//! [`ARRAY_ITERATION_METHODS`] policy vocabulary itself stays in the crate root (policy census).

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    CallExpr, Callee, DoWhileStmt, Expr, ForInStmt, ForOfStmt, ForStmt, MemberProp, WhileStmt,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::{line_of, parse_with_cm, ARRAY_ITERATION_METHODS};

/// Projects per-file loop-body line spans (1-based, inclusive) — see `zzop_core::dsl::SourceFile::
/// loop_spans`'s doc comment for the exact contract this mirrors. Two span sources, both emitted in
/// source order via a single recursive walk (nested loops/callbacks freely overlap; consumers do
/// any-span containment):
/// - Every `for`/`for-in`/`for-of` (incl. `for await`)/`while`/`do-while` statement's WHOLE span (header
///   line included — a call in the loop condition runs once per iteration too).
/// - The callback-ARGUMENT-ONLY span of a recognized array-iteration call (an [`ARRAY_ITERATION_METHODS`]
///   member-call whose first argument is an `Arrow`/`Function` expression) — never the whole call
///   expression, so a one-shot call on the RECEIVER (`(await fetch(u)).items.map(...)`) is not
///   misclassified as loop-body.
pub fn extract_loop_spans(file: &str, source: &str) -> Vec<(u32, u32)> {
    let Some((cm, module)) = parse_with_cm(file, source) else {
        return Vec::new();
    };
    let mut collector = LoopSpanCollector {
        cm: &cm,
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}

struct LoopSpanCollector<'a> {
    cm: &'a SourceMap,
    out: Vec<(u32, u32)>,
}

impl LoopSpanCollector<'_> {
    fn push_span(&mut self, span: swc_core::common::Span) {
        self.out
            .push((line_of(self.cm, span.lo), line_of(self.cm, span.hi)));
    }
}

impl Visit for LoopSpanCollector<'_> {
    fn visit_for_stmt(&mut self, n: &ForStmt) {
        self.push_span(n.span);
        n.visit_children_with(self);
    }

    fn visit_for_in_stmt(&mut self, n: &ForInStmt) {
        self.push_span(n.span);
        n.visit_children_with(self);
    }

    fn visit_for_of_stmt(&mut self, n: &ForOfStmt) {
        self.push_span(n.span); // covers `for await (...)` too — is_await doesn't change the span.
        n.visit_children_with(self);
    }

    fn visit_while_stmt(&mut self, n: &WhileStmt) {
        self.push_span(n.span);
        n.visit_children_with(self);
    }

    fn visit_do_while_stmt(&mut self, n: &DoWhileStmt) {
        self.push_span(n.span);
        n.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Callee::Expr(callee) = &call.callee {
            if let Expr::Member(m) = &**callee {
                if let MemberProp::Ident(name) = &m.prop {
                    if ARRAY_ITERATION_METHODS.contains(&name.sym.as_str()) {
                        if let Some(first) = call.args.first() {
                            match &*first.expr {
                                Expr::Arrow(a) => self.push_span(a.span),
                                Expr::Fn(f) => self.push_span(f.function.span),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        call.visit_children_with(self); // recurse: nested loops/callbacks, and the receiver expression.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_loop_spans ---

    #[test]
    fn extract_loop_spans_for_while_do_while_include_header() {
        let src = "for (let i = 0; i < 10; i++) {\n  doThing();\n}\nwhile (cond()) {\n  step();\n}\ndo {\n  step();\n} while (cond());\n";
        let spans = extract_loop_spans("f.ts", src);
        assert_eq!(spans, vec![(1, 3), (4, 6), (7, 9)]);
    }

    #[test]
    fn extract_loop_spans_for_of_await() {
        let src = "async function f() {\n  for await (const x of gen()) {\n    use(x);\n  }\n}\n";
        let spans = extract_loop_spans("f.ts", src);
        assert_eq!(spans, vec![(2, 4)]);
    }

    /// The receiver `(await fetch(u))` is a one-shot call on an earlier line — it must not be swept into
    /// the loop span; only the callback argument's own span counts.
    #[test]
    fn extract_loop_spans_map_callback_excludes_receiver_line() {
        let src = "const items = (await fetch(u))\n  .items.map((x) => {\n    use(x);\n  });\n";
        let spans = extract_loop_spans("f.ts", src);
        assert_eq!(spans, vec![(2, 4)]);
    }

    #[test]
    fn extract_loop_spans_single_line_arrow_callback_has_equal_start_end() {
        let src = "arr.forEach(x => use(x));\n";
        let spans = extract_loop_spans("f.ts", src);
        assert_eq!(spans, vec![(1, 1)]);
    }

    #[test]
    fn extract_loop_spans_nested_loops_emit_both_in_source_order() {
        let src =
            "for (const i of outer()) {\n  for (const j of inner()) {\n    use(i, j);\n  }\n}\n";
        let spans = extract_loop_spans("f.ts", src);
        assert_eq!(spans, vec![(1, 5), (2, 4)]);
    }

    #[test]
    fn extract_loop_spans_no_loops_or_callbacks_yields_empty() {
        let src = "export function f(x: number): number {\n  return x + 1;\n}\n";
        assert!(extract_loop_spans("f.ts", src).is_empty());
    }
}
