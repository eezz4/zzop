//! Public-signature type references — the evidence `unimported-export` needs to tell a type that is part
//! of an exported value's PUBLIC API from one that is merely mentioned inside a body.
//!
//! ## Why this exists (and why `used_names` cannot do it)
//! `unimported-export` reports an export as `in-file-only` when the file's own `used_names` contains its
//! name. `used_names` (`ident_refs.rs`) is a flat set with no positions, so these three shapes are
//! indistinguishable to it even though only the first is a false positive:
//!
//! ```text
//! export interface XState {…}  export function useX(): XState      <- public API, must NOT report
//! export function useX() { const [s] = useState<XState>(…) }        <- body-only, MUST report
//! interface Props { x: XThing }   // Props not exported             <- private, MUST report
//! ```
//!
//! This module supplies the missing axis. The rule is a single sentence: **walk only EXPORTED
//! declarations, and within them only TYPE-ANNOTATION positions — never a function body.** That one
//! rule separates all three shapes above, because a return-type annotation is reachable from the
//! module's public surface while a `useState<T>` generic inside a body is not.
//!
//! ## What counts as exported
//! `export <decl>`, `export default <fn|class|interface>`, and a top-level declaration named by a
//! bare `export { X }` (no `from` — an `export { X } from "./y"` re-exports someone ELSE's
//! declaration, so this file's own signatures are not involved). A declaration reachable only
//! through a namespace/`export *` is NOT resolved here.
//!
//! ## What is collected, per exported declaration
//! Function/arrow: parameter type annotations, the return type, and type-parameter
//! constraints/defaults — never the body. Variable: the declared annotation, plus the signature of
//! an arrow/function initializer (`export const useX = (): XState => {…}`, the dominant React-hook
//! shape). Class: `extends`/`implements`, type params, and every member's annotation/method
//! signature — never a method body. Interface: `extends`, type params, and the whole body (an
//! exported interface's members ARE its public shape). Type alias: the whole right-hand side.
//! Enums, namespaces and ambient module bodies contribute nothing (their public surface names
//! values, not types).
//!
//! ## Accepted limits (under-collection is the safe direction)
//! Missing a name here only means a `unimported-export` finding that would have been exempted still
//! reports — the pre-existing behavior. Over-collecting would silently HIDE a real dead type, so
//! every judgment call below resolves toward collecting less: no cross-file resolution, no
//! `typeof x` value-to-type bridging, no namespace/`export *` reach, and no inference of an
//! un-annotated return type (a hook that returns `XState` without saying so keeps reporting — by
//! design, since without an annotation the type genuinely is not named in the public surface).
//!
//! The ONE known exception to that safe direction: this set is file-scoped NAME STRINGS with no
//! scope resolution, so a TYPE PARAMETER can shadow a same-named top-level type —
//! `export type Result = …` plus `export function f<Result>(x: Result)` collects `Result` from the
//! generic and would wrongly exempt the top-level alias (over-collection: it HIDES a finding).
//! Left unfixed on purpose: a 2026-07-25 whole-corpus audit of all 253 exemptions across 22 trees
//! found ZERO cases (every exemption cited a real member/parameter line), and scope resolution
//! bought by an unmeasured shape is pure regression risk. Revisit if a real instance appears.

use std::collections::BTreeSet;

use swc_core::ecma::ast::{
    ArrowExpr, Class, ClassMember, Decl, DefaultDecl, ExportSpecifier, Expr, Function, ModuleDecl,
    ModuleExportName, ModuleItem, ParamOrTsParamProp, Pat, Stmt, TsEntityName, TsExprWithTypeArgs,
    TsParamPropParam, TsTypeAnn, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::parse_module;

/// Names appearing in the public signature of some exported declaration in this file, sorted for
/// deterministic serialization (same convention as `used_names`). Empty for an unparseable file —
/// never panics, same graceful-degrade convention as every other projector in this crate.
pub fn parse_exported_signature_names(file: &str, source: &str) -> Vec<String> {
    let Some(module) = parse_module(file, source) else {
        return Vec::new();
    };

    // Pass 1: locals made public by a bare `export { X }`. Their declarations sit elsewhere at top
    // level, so pass 2 has to know the names before it walks.
    let mut exported_locals: BTreeSet<String> = BTreeSet::new();
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(named)) = item else {
            continue;
        };
        if named.src.is_some() {
            continue; // `export { X } from "./y"` — not this file's own declaration
        }
        for spec in &named.specifiers {
            if let ExportSpecifier::Named(n) = spec {
                if let ModuleExportName::Ident(id) = &n.orig {
                    exported_locals.insert(id.sym.to_string());
                }
            }
        }
    }

    let mut c = TypeRefCollector::default();
    for item in &module.body {
        match item {
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => collect_decl(&mut c, &e.decl),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(e)) => match &e.decl {
                DefaultDecl::Fn(f) => collect_fn(&mut c, &f.function),
                DefaultDecl::Class(cl) => collect_class(&mut c, &cl.class),
                DefaultDecl::TsInterfaceDecl(i) => {
                    collect_decl(&mut c, &Decl::TsInterface(i.clone()))
                }
            },
            ModuleItem::Stmt(Stmt::Decl(d)) if decl_is_exported(d, &exported_locals) => {
                collect_decl(&mut c, d)
            }
            _ => {}
        }
    }
    c.names.into_iter().collect()
}

/// Whether a top-level declaration's own name was published by a bare `export { … }`.
fn decl_is_exported(d: &Decl, exported_locals: &BTreeSet<String>) -> bool {
    match d {
        Decl::Fn(f) => exported_locals.contains(f.ident.sym.as_str()),
        Decl::Class(c) => exported_locals.contains(c.ident.sym.as_str()),
        Decl::TsInterface(i) => exported_locals.contains(i.id.sym.as_str()),
        Decl::TsTypeAlias(t) => exported_locals.contains(t.id.sym.as_str()),
        Decl::Var(v) => v.decls.iter().any(|d| match &d.name {
            Pat::Ident(b) => exported_locals.contains(b.id.sym.as_str()),
            _ => false,
        }),
        _ => false,
    }
}

fn collect_decl(c: &mut TypeRefCollector, d: &Decl) {
    match d {
        Decl::Fn(f) => collect_fn(c, &f.function),
        Decl::Class(cl) => collect_class(c, &cl.class),
        Decl::Var(v) => {
            for decl in &v.decls {
                collect_var_declarator(c, decl);
            }
        }
        Decl::TsInterface(i) => {
            if let Some(tp) = &i.type_params {
                tp.visit_with(c);
            }
            i.extends.visit_with(c);
            i.body.visit_with(c); // an exported interface's members ARE its public shape
        }
        Decl::TsTypeAlias(t) => {
            if let Some(tp) = &t.type_params {
                tp.visit_with(c);
            }
            t.type_ann.visit_with(c);
        }
        // Enum / namespace / ambient module: their public surface names values, not types.
        _ => {}
    }
}

fn collect_var_declarator(c: &mut TypeRefCollector, d: &VarDeclarator) {
    collect_pat(c, &d.name);
    match d.init.as_deref() {
        // `export const useX = (): XState => {…}` — the dominant React-hook shape.
        Some(Expr::Arrow(a)) => collect_arrow(c, a),
        Some(Expr::Fn(f)) => collect_fn(c, &f.function),
        _ => {}
    }
}

fn collect_fn(c: &mut TypeRefCollector, f: &Function) {
    for p in &f.params {
        collect_pat(c, &p.pat);
    }
    collect_ann(c, f.return_type.as_deref());
    if let Some(tp) = &f.type_params {
        tp.visit_with(c);
    }
    // f.body deliberately NOT visited — that is the whole point of this module.
}

fn collect_arrow(c: &mut TypeRefCollector, a: &ArrowExpr) {
    for p in &a.params {
        collect_pat(c, p);
    }
    collect_ann(c, a.return_type.as_deref());
    if let Some(tp) = &a.type_params {
        tp.visit_with(c);
    }
    // a.body deliberately NOT visited.
}

fn collect_class(c: &mut TypeRefCollector, class: &Class) {
    if let Some(sc) = &class.super_class {
        if let Expr::Ident(id) = &**sc {
            c.names.insert(id.sym.to_string());
        }
    }
    if let Some(tp) = &class.type_params {
        tp.visit_with(c);
    }
    if let Some(sp) = &class.super_type_params {
        sp.visit_with(c);
    }
    class.implements.visit_with(c);
    for m in &class.body {
        match m {
            ClassMember::Method(m) => collect_fn(c, &m.function),
            ClassMember::ClassProp(p) => collect_ann(c, p.type_ann.as_deref()),
            ClassMember::Constructor(ctor) => {
                for p in &ctor.params {
                    match p {
                        ParamOrTsParamProp::Param(param) => collect_pat(c, &param.pat),
                        ParamOrTsParamProp::TsParamProp(tpp) => match &tpp.param {
                            TsParamPropParam::Ident(b) => collect_ann(c, b.type_ann.as_deref()),
                            TsParamPropParam::Assign(a) => collect_pat(c, &a.left),
                        },
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_pat(c: &mut TypeRefCollector, pat: &Pat) {
    match pat {
        Pat::Ident(b) => collect_ann(c, b.type_ann.as_deref()),
        Pat::Array(a) => collect_ann(c, a.type_ann.as_deref()),
        Pat::Object(o) => collect_ann(c, o.type_ann.as_deref()),
        Pat::Rest(r) => collect_ann(c, r.type_ann.as_deref()),
        Pat::Assign(a) => collect_pat(c, &a.left),
        _ => {}
    }
}

fn collect_ann(c: &mut TypeRefCollector, ann: Option<&TsTypeAnn>) {
    if let Some(a) = ann {
        a.visit_with(c);
    }
}

/// Gathers every type NAME reachable from the type nodes it is pointed at. Only ever run on
/// type-annotation subtrees selected by the `collect_*` walkers above — it has no notion of
/// "signature" itself, so pointing it at a function body would silently defeat this module.
#[derive(Default)]
struct TypeRefCollector {
    names: BTreeSet<String>,
}

impl Visit for TypeRefCollector {
    /// `A` and the ROOT of `A.B.C` — a qualified name's later segments are members of `A`, not
    /// separately importable symbols, so only the root can be a dead-export candidate.
    fn visit_ts_entity_name(&mut self, n: &TsEntityName) {
        let mut cur = n;
        loop {
            match cur {
                TsEntityName::Ident(id) => {
                    self.names.insert(id.sym.to_string());
                    return;
                }
                TsEntityName::TsQualifiedName(q) => cur = &q.left,
            }
        }
    }

    /// `implements Foo<T>` / `extends Bar` carry their name as an `Expr`, not a `TsEntityName`.
    fn visit_ts_expr_with_type_args(&mut self, n: &TsExprWithTypeArgs) {
        if let Expr::Ident(id) = &*n.expr {
            self.names.insert(id.sym.to_string());
        }
        n.type_args.visit_with(self);
    }
}

#[cfg(test)]
mod tests;
