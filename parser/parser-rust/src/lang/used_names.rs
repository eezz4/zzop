//! Identifier-reference collection — dead-export analysis substrate, mirroring
//! `zzop_parser_python_3::lang::used_names::parse_local_identifier_refs`'s purpose: every EXPRESSION path
//! reference (`syn::ExprPath`, covering a bare identifier read AND a call/qualified reference like
//! `foo()` or `Type::method()`) and every TYPE path reference (`syn::TypePath`, a parameter/return/field
//! type annotation) contributes its LAST path segment to the set. A declaration's own name (a function's
//! `sig.ident`, a struct's `ident`, ...) is a plain `syn::Ident` FIELD on its item node, never visited as
//! an `Expr`/`Type` by `syn::visit::Visit`'s generated walk, so it is excluded automatically — the same
//! "declaration name is a field, not a child expression" mechanism
//! `zzop_parser_python_3::lang::used_names` relies on for `StmtFunctionDef::name`/`StmtClassDef::name`. A
//! pattern binding (a `let` target, a function PARAMETER name, a `match` arm binding) is likewise a
//! `syn::Pat` node, visited via `visit_pat_ident` (not overridden here), so a plain binding never
//! contributes a reference either — only a later READ of that name (which parses as an `ExprPath`) does.
//!
//! Only the LAST segment of a multi-segment path is kept (`Type::method` -> `"method"`, `crate::a::b` ->
//! `"b"`) — this crate's chosen simple-name convention (module doc's brief), matching
//! `zzop_parser_python_3::lang::used_names`'s own "simple name" scope rather than tracking full dotted
//! paths.
//!
//! Out of v1 scope: identifiers used only inside a macro invocation's argument tokens (`println!("{}",
//! x)`'s `x`) — see the crate root doc's "Scope note: macros".

use std::collections::BTreeSet;
use syn::visit::{self, Visit};
use syn::{ExprPath, TypePath};

/// Extract every identifier/type-path REFERENCE (last segment only) in `text`. Empty on parse failure
/// (never panics).
pub fn parse_local_identifier_refs(text: &str) -> BTreeSet<String> {
    let Some(file) = crate::parse_file(text) else {
        return BTreeSet::new();
    };
    let mut collector = RefCollector {
        refs: BTreeSet::new(),
    };
    collector.visit_file(&file);
    collector.refs
}

struct RefCollector {
    refs: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for RefCollector {
    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        if let Some(seg) = node.path.segments.last() {
            self.refs.insert(seg.ident.to_string());
        }
        visit::visit_expr_path(self, node);
    }

    fn visit_type_path(&mut self, node: &'ast TypePath) {
        if let Some(seg) = node.path.segments.last() {
            self.refs.insert(seg.ident.to_string());
        }
        visit::visit_type_path(self, node);
    }
}

#[cfg(test)]
mod tests;
