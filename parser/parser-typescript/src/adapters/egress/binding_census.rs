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
    ArrowExpr, AssignExpr, AssignTarget, BinaryOp, BindingIdent, BlockStmt, BlockStmtOrExpr,
    Callee, ClassDecl, ClassExpr, Expr, FnDecl, FnExpr, Function, ImportDecl, ImportSpecifier, Lit,
    Module, Pat, SimpleAssignTarget, Stmt, TsEnumDecl, TsImportEqualsDecl, TsModuleDecl,
    TsModuleName, UpdateExpr, VarDecl, VarDeclKind,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use super::unwrap_expr;

/// One file's census output: the constant declarators and the zero-argument helper returns that clear
/// the same three gates. Two lists rather than one map because they live in different NAMESPACES at a
/// URL position — `X` names a value, `X()` names a call — and folding them would let a function name
/// answer a bare-identifier lookup.
pub(super) struct UnambiguousBindings {
    /// `(name, wrapper-stripped initializer)` — see [`unambiguous_bindings`].
    pub(super) consts: Vec<(String, Expr)>,
    /// `(name, wrapper-stripped single return expression)` for zero-parameter, non-async,
    /// non-generator function declarations / arrow or function expressions bound to a `const`/`let`
    /// — see [`unambiguous_bindings`]'s `same-file-fn-url-v1` paragraph.
    pub(super) fn_returns: Vec<(String, Expr)>,
}

/// Every `const`/`let` declarator in this file whose name is bound EXACTLY ONCE in the whole file and is
/// never an assignment/update target, as `(name, wrapper-stripped initializer)` — plus, in the sibling
/// `fn_returns` list, every same-file ZERO-ARGUMENT helper whose whole body is one `return <expr>`.
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
///
/// **`same-file-fn-url-v1` — `fn_returns`.** The same three gates decide a helper's name, and two more
/// decide its SHAPE, both structural rather than inferred: the helper takes **no parameters** (so the
/// call site's `f()` passes nothing that could change the returned string — a `f(id)` has an argument the
/// return value depends on and is left alone), and its whole body is **exactly one `return <expr>`** (so
/// there is one return value, not a branch this census would have to choose between). `async` and
/// generator helpers are refused outright: `f()` on those evaluates to a Promise/iterator, not the
/// string, and `await f()` is a shape the URL resolver does not unwrap — admitting them would substitute
/// a value the call site does not actually hold.
pub(super) fn unambiguous_bindings(module: &Module) -> UnambiguousBindings {
    let mut census = BindingCensus::default();
    module.visit_with(&mut census);
    let BindingCensus {
        candidates,
        fn_candidates,
        bindings,
        assigned,
    } = census;
    let admitted = |name: &String| bindings.get(name) == Some(&1) && !assigned.contains(name);
    UnambiguousBindings {
        consts: candidates
            .into_iter()
            .filter(|(name, _)| admitted(name))
            .collect(),
        fn_returns: fn_candidates
            .into_iter()
            .filter(|(name, _)| admitted(name))
            .collect(),
    }
}

/// The single `return <expr>` a zero-parameter, non-async, non-generator [`Function`] body consists of,
/// wrapper-stripped and cost-filtered by [`is_candidate_shape`]; `None` for every other shape.
fn function_return_expr(f: &Function) -> Option<&Expr> {
    if !f.params.is_empty() || f.is_async || f.is_generator {
        return None;
    }
    single_return_expr(f.body.as_ref()?)
}

/// The [`function_return_expr`] counterpart for an arrow, whose body is either a bare expression
/// (`() => '/x'`) or a block (`() => { return '/x'; }`).
fn arrow_return_expr(a: &ArrowExpr) -> Option<&Expr> {
    if !a.params.is_empty() || a.is_async || a.is_generator {
        return None;
    }
    match &*a.body {
        BlockStmtOrExpr::Expr(e) => keep_return(e),
        BlockStmtOrExpr::BlockStmt(b) => single_return_expr(b),
    }
}

/// A block whose ONLY statement is `return <expr>`. Any other statement count — a log line, a guard, a
/// second return — means the returned value is not this one expression, so the helper is dropped.
fn single_return_expr(b: &BlockStmt) -> Option<&Expr> {
    match b.stmts.as_slice() {
        [Stmt::Return(r)] => keep_return(r.arg.as_deref()?),
        _ => None,
    }
}

/// Wrapper-strip a return value and apply the same cost filter declarator initializers get.
fn keep_return(e: &Expr) -> Option<&Expr> {
    let e = unwrap_expr(e);
    is_candidate_shape(e).then_some(e)
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
        // `same-file-fn-url-v1`: a ZERO-ARGUMENT bare-identifier call is the one call shape the URL
        // resolver can answer, and it is cheap to clone by construction (no argument subtrees).
        Expr::Call(c) => {
            c.args.is_empty()
                && matches!(&c.callee, Callee::Expr(e) if matches!(unwrap_expr(e), Expr::Ident(_)))
        }
        Expr::Lit(Lit::Str(_))
        | Expr::Tpl(_)
        | Expr::Cond(_)
        | Expr::Ident(_)
        | Expr::Member(_) => true,
        _ => false,
    }
}

/// How many times each name is BOUND (any declaration form), whether it is ever an assignment/update
/// target, the `const`/`let` declarator initializers worth keeping, and the zero-argument helper returns
/// worth keeping.
#[derive(Default)]
struct BindingCensus {
    bindings: HashMap<String, usize>,
    assigned: HashSet<String>,
    candidates: Vec<(String, Expr)>,
    fn_candidates: Vec<(String, Expr)>,
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
                    // `same-file-fn-url-v1`: the same declarator can instead bind a zero-argument
                    // helper. Disjoint from the branch above by shape — an arrow/function expression
                    // is not a candidate initializer shape.
                    let ret = match init {
                        Expr::Arrow(a) => arrow_return_expr(a),
                        Expr::Fn(f) => function_return_expr(&f.function),
                        _ => None,
                    };
                    if let Some(ret) = ret {
                        self.fn_candidates
                            .push((bi.id.sym.to_string(), ret.clone()));
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
        if let Some(ret) = function_return_expr(&n.function) {
            self.fn_candidates
                .push((n.ident.sym.to_string(), ret.clone()));
        }
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
