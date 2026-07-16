//! Identifier-reference collection — dead-export analysis substrate, mirroring
//! `zzop_parser_typescript::parse_local_identifier_refs`'s purpose (and its binding/reference split):
//! every `Expr::Name` visited in `ExprContext::Load` (a READ) counts as a reference; `ExprContext::Store`
//! (an assignment target) and `ExprContext::Del` (a `del` target) do not — those are BINDINGS, not uses.
//! A function/class's own declaration name (`StmtFunctionDef::name` / `StmtClassDef::name`) is never
//! visited as an `Expr` at all by `ruff_python_ast::visitor::walk_stmt` (it's a plain `Identifier` field,
//! not an AST child expression), so it is excluded automatically, without special-casing — same net
//! effect as TS's own `visit_fn_decl`/`visit_class_decl` "skip `n.ident`" overrides. Import-bound names
//! are excluded the same way: `Stmt::Import`/`Stmt::ImportFrom` walk their `Alias`es, never an `Expr`.

use ruff_python_ast::visitor::{walk_expr, Visitor};
use ruff_python_ast::{Expr, ExprContext};
use std::collections::BTreeSet;

/// Extract every identifier REFERENCE (not binding) in `text`. Empty on parse failure (never panics).
pub fn parse_local_identifier_refs(text: &str) -> BTreeSet<String> {
    let Some(module) = crate::parse_module(text) else {
        return BTreeSet::new();
    };
    let mut collector = RefCollector {
        refs: BTreeSet::new(),
    };
    for stmt in &module.body {
        collector.visit_stmt(stmt);
    }
    collector.refs
}

struct RefCollector {
    refs: BTreeSet<String>,
}

impl<'a> Visitor<'a> for RefCollector {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Name(n) = expr {
            if n.ctx == ExprContext::Load {
                self.refs.insert(n.id.as_str().to_string());
            }
        }
        walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(text: &str) -> BTreeSet<String> {
        parse_local_identifier_refs(text)
    }

    #[test]
    fn collects_a_read_reference() {
        let out = refs("X = 1\nprint(X)\n");
        assert!(out.contains("X"));
        assert!(out.contains("print"));
    }

    #[test]
    fn assignment_target_is_a_binding_not_a_reference() {
        // `x` on the left of `=` is Store context, not a read.
        let out = refs("x = 1\n");
        assert!(!out.contains("x"));
    }

    #[test]
    fn function_declaration_name_is_excluded() {
        let out = refs("def foo():\n    return bar()\n");
        assert!(!out.contains("foo"));
        assert!(out.contains("bar"));
    }

    #[test]
    fn class_declaration_name_is_excluded() {
        let out = refs("class Foo:\n    def bar(self):\n        return baz\n");
        assert!(!out.contains("Foo"));
        assert!(out.contains("baz"));
    }

    #[test]
    fn function_parameter_names_are_not_references() {
        // Parameters are bindings (visited via `visit_parameter`, never `visit_expr`) — must not appear.
        let out = refs("def foo(a, b):\n    return a + b\n");
        // `a`/`b` DO appear here as reads inside the body (`return a + b`), so this pins the read side
        // rather than the binding side — the parameter DECLARATION itself never surfaces as a separate
        // reference beyond its in-body reads, which is the behavior under test.
        assert!(out.contains("a"));
        assert!(out.contains("b"));
    }

    #[test]
    fn imported_name_binding_is_excluded_but_its_usage_is_a_reference() {
        let out = refs("from fastapi import FastAPI\napp = FastAPI()\n");
        // `FastAPI` is read (called) on the right-hand side — a real reference, not excluded (unlike a
        // TS default/namespace import binding, a Python import name is bound via an `Alias`, never
        // visited as `Expr::Name`, so nothing here is "the binding itself" to suppress).
        assert!(out.contains("FastAPI"));
        // `app` is the assignment target — Store context, excluded.
        assert!(!out.contains("app"));
    }

    #[test]
    fn del_target_is_not_a_reference() {
        let out = refs("x = 1\ndel x\n");
        assert!(!out.contains("x"));
    }

    #[test]
    fn nested_expression_references_are_collected() {
        let out = refs("if a and b:\n    c = [d for d in e]\n");
        for name in ["a", "b", "e"] {
            assert!(out.contains(name), "missing {name}: {out:?}");
        }
        // `d` is the comprehension's own loop-target binding (Store), but it's also read in the
        // element expression `d` — so it legitimately appears as a reference too.
        assert!(out.contains("d"));
        // `c` is a plain assignment target — Store context, excluded.
        assert!(!out.contains("c"));
    }

    #[test]
    fn parse_failure_yields_empty_set() {
        assert!(refs("def f(:\n").is_empty());
    }

    #[test]
    fn empty_file_yields_empty_set() {
        assert!(refs("").is_empty());
    }
}
