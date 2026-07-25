//! Whole-file binding census — the scope substrate both same-file URL-constant rules stand on.
//!
//! It answers one question about one file's AST: **which names are bound exactly once in this file, and
//! by what initializer?** Nothing here knows about URLs. The census is what lets [`super::local_consts`]
//! admit BARE identifiers that the project-wide, scope-insensitive [`super::consts`] map must refuse:
//! a name bound exactly once in a file cannot be a parameter shadow, a redeclaration, or an import of
//! something else, because every one of those forms IS a binding occurrence and would push the count to 2.
//!
//! Counting every binding form uniformly is the point — it makes "single binding" and "no parameter
//! shadow" ONE check instead of two enumerations that can drift apart, and it means the census cannot
//! forget a binding form it never enumerated. A name it drops keeps today's behavior (an opaque `{}` piece
//! or an unresolved consume), which is silence, not a wrong answer.

use std::collections::{HashMap, HashSet};

use swc_core::ecma::ast::{
    AssignExpr, AssignTarget, BinaryOp, BindingIdent, ClassDecl, ClassExpr, Expr, FnDecl, FnExpr,
    ImportDecl, ImportSpecifier, Lit, Module, Pat, SimpleAssignTarget, TsEnumDecl,
    TsImportEqualsDecl, TsModuleDecl, TsModuleName, UpdateExpr, VarDecl, VarDeclKind,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use super::unwrap_expr;

/// Every `const`/`let` declarator in this file whose name is bound EXACTLY ONCE in the whole file and is
/// never an assignment/update target, as `(name, wrapper-stripped initializer)`.
///
/// Three gates are decided here and are the same for every caller:
///
/// 1. **Single binding** — `== 1` occurrence across the file. A second binding ANYWHERE drops the name:
///    a redeclaration at any nesting depth, a parameter, a destructuring element, an import (including
///    `import X = require(...)`), or a `function`/`class`/`enum`/`namespace` declaration. Those last
///    three are TypeScript's other VALUE-namespace declaration forms; `interface`/`type` bind no value
///    and correctly do not shadow.
/// 2. **No reassignment** — no `X = …` / `X += …` / `X++` / destructuring-assignment to `X` anywhere.
///    `const` cannot be reassigned in well-formed code, but a `let` can, and a `let` whose value changes
///    after the declaration is not a constant.
/// 3. **`var` is not admitted at all** — its hoisting and legal redeclaration make a `var` binding a
///    weaker claim than the two block-scoped forms, and no measured case needs it.
///
/// **Nesting is deliberately not a gate.** A `const` declared inside a function qualifies exactly like a
/// top-level one, because gate 1 is what carries the scope argument: with one binding in the file there is
/// no second declaration for a reference to mean instead. The residual — a reference in a sibling scope
/// resolving to a same-named GLOBAL while this file's only binding sits inside some function — is
/// broken-or-pathological code (the reference would be a TDZ/ReferenceError against the local binding it
/// cannot see), and refusing it would mean scope-chain resolution, which is the thing this census exists
/// to avoid.
pub(super) fn unambiguous_bindings(module: &Module) -> Vec<(String, Expr)> {
    let mut census = BindingCensus::default();
    module.visit_with(&mut census);
    let BindingCensus {
        candidates,
        bindings,
        assigned,
    } = census;
    candidates
        .into_iter()
        .filter(|(name, _)| bindings.get(name) == Some(&1) && !assigned.contains(name))
        .collect()
}

/// COST filter, not a semantic one: which initializer shapes are worth cloning out of the AST. swc's
/// `Visit` hands out node references with no lifetime tie to the module, so a kept candidate must be
/// cloned — and a `const el = cond && <Big/>` initializer is not something to deep-copy in every file.
///
/// It must stay a SUPERSET of what `super::url_resolve::resolve_url_variants` can resolve, or it would be
/// narrowing semantics from the wrong place. That is why only shapes a SINGLE FIELD READ proves
/// unresolvable are excluded (a non-`+` binary operator, which `flatten_add_chain` rejects outright);
/// a ternary's arms are not re-judged here, because re-deriving `resolve_cond_arm`'s predicate is exactly
/// the duplicated-judgement drift this module's uniform counting exists to prevent.
fn is_candidate_shape(e: &Expr) -> bool {
    match e {
        Expr::Bin(b) => b.op == BinaryOp::Add,
        Expr::Lit(Lit::Str(_))
        | Expr::Tpl(_)
        | Expr::Cond(_)
        | Expr::Ident(_)
        | Expr::Member(_) => true,
        _ => false,
    }
}

/// How many times each name is BOUND (any declaration form), whether it is ever an assignment/update
/// target, and the `const`/`let` declarator initializers worth keeping.
#[derive(Default)]
struct BindingCensus {
    bindings: HashMap<String, usize>,
    assigned: HashSet<String>,
    candidates: Vec<(String, Expr)>,
}

impl BindingCensus {
    fn bind(&mut self, name: &str) {
        *self.bindings.entry(name.to_string()).or_insert(0) += 1;
    }
}

impl Visit for BindingCensus {
    /// Candidate collection. Every `const`/`let` declarator in the file reaches here — top-level,
    /// exported, block-nested, or function-local alike — and the declarator's own `Pat::Ident` is still
    /// counted as a binding by `visit_binding_ident` below, via the children walk.
    fn visit_var_decl(&mut self, n: &VarDecl) {
        if matches!(n.kind, VarDeclKind::Const | VarDeclKind::Let) {
            for d in &n.decls {
                if let (Pat::Ident(bi), Some(init)) = (&d.name, &d.init) {
                    let init = unwrap_expr(init);
                    if is_candidate_shape(init) {
                        self.candidates.push((bi.id.sym.to_string(), init.clone()));
                    }
                }
            }
        }
        n.visit_children_with(self);
    }

    /// Every pattern binding: `const`/`let`/`var` declarators, function/arrow/method/catch parameters,
    /// and every element of a destructuring pattern.
    fn visit_binding_ident(&mut self, n: &BindingIdent) {
        self.bind(n.id.sym.as_ref());
        n.visit_children_with(self);
    }

    fn visit_fn_decl(&mut self, n: &FnDecl) {
        self.bind(n.ident.sym.as_ref());
        n.visit_children_with(self);
    }

    fn visit_class_decl(&mut self, n: &ClassDecl) {
        self.bind(n.ident.sym.as_ref());
        n.visit_children_with(self);
    }

    /// A named function/class EXPRESSION binds its own name inside its body.
    fn visit_fn_expr(&mut self, n: &FnExpr) {
        if let Some(id) = &n.ident {
            self.bind(id.sym.as_ref());
        }
        n.visit_children_with(self);
    }

    fn visit_class_expr(&mut self, n: &ClassExpr) {
        if let Some(id) = &n.ident {
            self.bind(id.sym.as_ref());
        }
        n.visit_children_with(self);
    }

    /// TypeScript's three remaining VALUE-binding declaration forms. Type-only declarations
    /// (`interface`, `type`) bind no value and are correctly absent — but an `enum`, a `namespace`, and an
    /// `import X = require(...)` all put a name in the value namespace and can therefore shadow a
    /// constant, so leaving them out would be exactly the "a binding form the census never enumerated"
    /// failure this module's doc claims is impossible.
    fn visit_ts_enum_decl(&mut self, n: &TsEnumDecl) {
        self.bind(n.id.sym.as_ref());
        n.visit_children_with(self);
    }

    fn visit_ts_module_decl(&mut self, n: &TsModuleDecl) {
        if let TsModuleName::Ident(id) = &n.id {
            self.bind(id.sym.as_ref());
        }
        n.visit_children_with(self);
    }

    fn visit_ts_import_equals_decl(&mut self, n: &TsImportEqualsDecl) {
        self.bind(n.id.sym.as_ref());
        n.visit_children_with(self);
    }

    fn visit_import_decl(&mut self, n: &ImportDecl) {
        for s in &n.specifiers {
            let local = match s {
                ImportSpecifier::Named(x) => &x.local,
                ImportSpecifier::Default(x) => &x.local,
                ImportSpecifier::Namespace(x) => &x.local,
            };
            self.bind(local.sym.as_ref());
        }
        n.visit_children_with(self);
    }

    /// Gate 2, stated explicitly rather than left to fall out of the binding count: a destructuring
    /// assignment target does re-enter `visit_binding_ident`, but a plain `X = …` must be rejected on its
    /// own terms so the gate does not silently depend on how swc happens to type an assignment target.
    fn visit_assign_expr(&mut self, n: &AssignExpr) {
        if let AssignTarget::Simple(SimpleAssignTarget::Ident(i)) = &n.left {
            self.assigned.insert(i.id.sym.to_string());
        }
        n.visit_children_with(self);
    }

    fn visit_update_expr(&mut self, n: &UpdateExpr) {
        if let Expr::Ident(i) = &*n.arg {
            self.assigned.insert(i.sym.to_string());
        }
        n.visit_children_with(self);
    }
}
