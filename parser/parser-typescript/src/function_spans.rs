//! Per-file FUNCTION line spans — every function-like node's span, with promise-continuation callback
//! arguments MERGED into their call site; feeds `MethodScan::after_in_same_function`. The
//! [`PROMISE_CONTINUATION_METHODS`] policy vocabulary itself stays in the crate root (policy census).

use std::collections::HashMap;

use swc_core::common::{BytePos, SourceMap, Span};
use swc_core::ecma::ast::{
    ArrowExpr, CallExpr, Callee, Constructor, Expr, Function, GetterProp, MemberProp, SetterProp,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::{line_of, parse_with_cm, PROMISE_CONTINUATION_METHODS};

/// Projects per-file function line spans (1-based, inclusive) — see `zzop_core::dsl::SourceFile::
/// function_spans`'s doc comment for the exact contract this mirrors. Emitted in node source order via a
/// single recursive walk (nested functions freely overlap; consumers resolve the INNERMOST containing
/// span). Sibling crate `loop_spans` is the structural precedent for the whole projection path.
///
/// Every function-like node contributes one span: `function` declarations/expressions (incl. class and
/// object-literal methods, via the shared `Function` node), arrow expressions, constructors, and
/// object-literal getters/setters.
///
/// **The merge rule** (the whole point of this fact — a plain "nearest function" partition was measured
/// and rejected): a function-shaped ARGUMENT of a `.then(...)`/`.catch(...)`/`.finally(...)` member call
/// has its span START pulled up to the line of that call's PROPERTY token, so the continuation callback
/// and the boundary token that schedules it land in ONE span. Without it, a matcher scoping on "nearest
/// function" splits `p\n  .then((d) => {\n setX(d);\n })` into a callback span that no longer contains
/// the `.then(` that proves the async boundary.
///
/// Deliberately NARROW, and the narrowness is the contract:
/// - Only the three `Promise.prototype` continuation methods, and only as a MEMBER-call property
///   (`p.then(cb)`) — an identifier-shaped `then(cb)` or an aliased continuation (`const t = p.then;
///   t(cb)`) is not recognized. No receiver-type proof, same "syntactic, not type-checked" tradeoff every
///   other adapter in this crate makes: a same-named method on an unrelated object merges too.
/// - Only the CALLBACK's own start moves; the RECEIVER is never swept in (`(await load()).p.then(cb)`
///   keeps `await load()` outside the merged span), mirroring `extract_loop_spans`'s receiver exclusion.
/// - Every OTHER callback argument (`.map`, `setTimeout`, `addEventListener`, `useEffect`, a custom
///   `onDone(cb)`) is left unmerged — those are not promise continuations, and merging them would
///   re-create the sibling-closure pairing this fact exists to break.
pub fn extract_function_spans(file: &str, source: &str) -> Vec<(u32, u32)> {
    let Some((cm, module)) = parse_with_cm(file, source) else {
        return Vec::new();
    };
    let mut collector = FunctionSpanCollector {
        cm: &cm,
        out: Vec::new(),
        merge_from: HashMap::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}

struct FunctionSpanCollector<'a> {
    cm: &'a SourceMap,
    out: Vec<(u32, u32)>,
    /// `span.lo` of a promise-continuation callback -> the 1-based line of the `.then`/`.catch`/
    /// `.finally` property token that owns it. Populated by `visit_call_expr` BEFORE the walk descends
    /// into the call's arguments, so the callback's own visit always sees its entry.
    merge_from: HashMap<BytePos, u32>,
}

impl FunctionSpanCollector<'_> {
    fn push_span(&mut self, span: Span) {
        let start = line_of(self.cm, span.lo);
        let merged = self
            .merge_from
            .get(&span.lo)
            .map_or(start, |&call_line| call_line.min(start));
        self.out.push((merged, line_of(self.cm, span.hi)));
    }
}

impl Visit for FunctionSpanCollector<'_> {
    // Covers `function` declarations AND expressions, class methods, and object-literal methods — all
    // four wrap the same `Function` node.
    fn visit_function(&mut self, n: &Function) {
        self.push_span(n.span);
        n.visit_children_with(self);
    }

    fn visit_arrow_expr(&mut self, n: &ArrowExpr) {
        self.push_span(n.span);
        n.visit_children_with(self);
    }

    fn visit_constructor(&mut self, n: &Constructor) {
        self.push_span(n.span);
        n.visit_children_with(self);
    }

    fn visit_getter_prop(&mut self, n: &GetterProp) {
        self.push_span(n.span);
        n.visit_children_with(self);
    }

    fn visit_setter_prop(&mut self, n: &SetterProp) {
        self.push_span(n.span);
        n.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Callee::Expr(callee) = &call.callee {
            if let Expr::Member(m) = &**callee {
                if let MemberProp::Ident(name) = &m.prop {
                    if PROMISE_CONTINUATION_METHODS.contains(&name.sym.as_str()) {
                        // The PROPERTY token's line, never the whole call expression's — a multi-line
                        // receiver (`fetchRates()\n  .then(cb)`) must not be swept into the callback's
                        // span, exactly as `extract_loop_spans` excludes an iteration call's receiver.
                        let call_line = line_of(self.cm, name.span.lo);
                        // Both `.then(onOk, onErr)` arguments are continuations, so every function-shaped
                        // argument merges — not only the first.
                        for arg in &call.args {
                            match &*arg.expr {
                                Expr::Arrow(a) => {
                                    self.merge_from.insert(a.span.lo, call_line);
                                }
                                Expr::Fn(f) => {
                                    self.merge_from.insert(f.function.span.lo, call_line);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        call.visit_children_with(self); // recurse: nested calls/callbacks, and the receiver expression.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_decl_and_nested_arrow_both_emit_in_source_order() {
        let src =
            "export function widget() {\n  const f = () => {\n    use();\n  };\n  return f;\n}\n";
        let spans = extract_function_spans("f.ts", src);
        assert_eq!(spans, vec![(1, 6), (2, 4)]);
    }

    #[test]
    fn a_then_callback_on_a_later_line_merges_up_to_the_then_token() {
        // The merge rule's reason for existing: without it the callback span starts at line 3 and no
        // longer contains the `.then(` on line 2 that proves the async boundary.
        let src = "function load() {\n  fetchRates()\n    .then(\n      (d) => {\n        setFx(d);\n      },\n    );\n}\n";
        let spans = extract_function_spans("f.ts", src);
        assert_eq!(
            spans,
            vec![(1, 8), (3, 6)],
            "the callback span must start on the `.then` line (3), not its own line (4)"
        );
    }

    #[test]
    fn a_then_callback_never_sweeps_in_a_multiline_receiver() {
        // `fetchRates()` on line 2 is a ONE-SHOT call on the receiver; only the `.then` property line
        // may be merged, mirroring `extract_loop_spans`'s receiver exclusion.
        let src =
            "function load() {\n  fetchRates()\n    .then((d) => {\n      setFx(d);\n    });\n}\n";
        let spans = extract_function_spans("f.ts", src);
        assert_eq!(spans, vec![(1, 6), (3, 5)]);
    }

    #[test]
    fn catch_and_finally_merge_too_and_a_chain_emits_one_span_per_callback() {
        let src = "function load() {\n  p.then((d) => {\n    setFx(d);\n  }).catch((e) => {\n    setErr(e);\n  }).finally(() => {\n    setDone();\n  });\n}\n";
        let spans = extract_function_spans("f.ts", src);
        // Note the two ADJACENT span pairs sharing a line ((2,4)/(4,6) and (4,6)/(6,8)): a chain link's
        // `})` closing line is also its successor's `.catch`/`.finally` line. Neither contains the other,
        // so a consumer resolving "innermost containing span" must break that tie deterministically —
        // `zzop_core::dsl::SourceFile::innermost_function_start` prefers the LATER-starting span.
        assert_eq!(spans, vec![(1, 9), (2, 4), (4, 6), (6, 8)]);
    }

    #[test]
    fn both_then_arguments_merge_not_only_the_first() {
        let src =
            "function load() {\n  p.then(\n    (d) => setFx(d),\n    (e) => setErr(e),\n  );\n}\n";
        let spans = extract_function_spans("f.ts", src);
        assert_eq!(spans, vec![(1, 6), (2, 3), (2, 4)]);
    }

    #[test]
    fn a_non_continuation_callback_argument_is_not_merged() {
        // The narrowness pin: `.map`/`setTimeout`/`useEffect` callbacks keep their own start line, so a
        // consumer's "same innermost span" test still separates them from their call site's line.
        let src = "function load() {\n  items.map(\n    (x) => use(x),\n  );\n}\n";
        let spans = extract_function_spans("f.ts", src);
        assert_eq!(spans, vec![(1, 5), (3, 3)]);
    }

    #[test]
    fn an_identifier_call_named_then_is_not_a_member_call_and_does_not_merge() {
        let src = "function load() {\n  then(\n    (d) => setFx(d),\n  );\n}\n";
        let spans = extract_function_spans("f.ts", src);
        assert_eq!(spans, vec![(1, 5), (3, 3)]);
    }

    #[test]
    fn class_methods_constructors_and_object_accessors_all_emit_spans() {
        let src = "class C {\n  constructor() {\n    this.x = 1;\n  }\n  run() {\n    return 1;\n  }\n}\nconst o = {\n  get v() {\n    return 2;\n  },\n};\n";
        let spans = extract_function_spans("f.ts", src);
        assert_eq!(spans, vec![(2, 4), (5, 7), (10, 12)]);
    }

    #[test]
    fn a_file_with_no_functions_yields_empty() {
        let src = "export const x = 1;\nexport type T = { a: number };\n";
        assert!(extract_function_spans("f.ts", src).is_empty());
    }

    #[test]
    fn an_unparseable_file_yields_empty_rather_than_failing() {
        assert!(extract_function_spans("f.ts", "function (((").is_empty());
    }
}
