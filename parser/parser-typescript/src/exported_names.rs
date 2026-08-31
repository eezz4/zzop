//! The PUBLIC NAMES a module publishes — every spelling of `export`, collapsed to one flat list.
//!
//! ## Why this is not any of the three facts that already exist
//! `SourceSymbol::exported` answers "is THIS FILE'S declaration public", `re_exports.rs` answers "which
//! module does this clause forward to", and `export_aliases.rs` answers "which local declaration hides
//! behind a renamed public name". None of the three answers "what may another file write to reach into
//! this one", and the difference is not theoretical: measured on nocodb 3a5cbd5,
//! `packages/nc-gui/lib/formBuilder.ts` is two lines — an `import { … } from 'nocodb-sdk'` and a
//! from-less `export { FormBuilderInputType, type FormBuilderElement, … }` — and `zzop file` reports
//! `symbols.count = 0` for it, because it declares nothing. `re_exports` skips the clause (no source
//! module) and `export_aliases` skips it too (no rename). Yet those four names ARE this file's public
//! surface, and under Nuxt auto-import they are the ONLY way anything reaches the file at all.
//!
//! ## Scope
//! Names only, no positions, no local mapping — a set-membership question ("does any other file mention
//! a name this one publishes?") has no use for either. `export * from "./y"` contributes nothing: the
//! names it republishes are not written here, and inventing them from the target module would need a
//! resolver this walk deliberately does not take. The literal `default` is emitted for an anonymous
//! `export default`, and callers that want bare-identifier names filter it out — dropping it here would
//! make the list silently incomplete for a caller that does care.

use swc_core::ecma::ast::{Decl, Module, ModuleDecl, ModuleItem, Pat};

use crate::imports::export_name;
use crate::parse_module;

/// Every public export name in `source`, in source order, deduplicated. An unparseable file yields an
/// empty list — the same graceful degrade the other whole-file entrypoints in this crate perform.
pub fn parse_exported_names(file: &str, source: &str) -> Vec<String> {
    let Some(module) = parse_module(file, source) else {
        return Vec::new();
    };
    exported_names_from_module(&module)
}

/// The walk itself, over an already-parsed module.
fn exported_names_from_module(module: &Module) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |name: String| {
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    };
    for item in &module.body {
        let ModuleItem::ModuleDecl(decl) = item else {
            continue;
        };
        match decl {
            // `export const x = …` / `export function f` / `export class C` / `export interface I` …
            ModuleDecl::ExportDecl(e) => decl_names(&e.decl, &mut push),
            // `export { a, b as c }` and `export { a } from "./y"` alike — the public half of each
            // specifier, which is what an importer writes either way.
            ModuleDecl::ExportNamed(named) => {
                for spec in &named.specifiers {
                    match spec {
                        swc_core::ecma::ast::ExportSpecifier::Named(n) => {
                            push(export_name(n.exported.as_ref().unwrap_or(&n.orig)));
                        }
                        // `export * as ns from "./y"` — `ns` is a real public name.
                        swc_core::ecma::ast::ExportSpecifier::Namespace(n) => {
                            push(export_name(&n.name));
                        }
                        // Legacy `export v from "./y"`.
                        swc_core::ecma::ast::ExportSpecifier::Default(n) => {
                            push(n.exported.sym.to_string());
                        }
                    }
                }
            }
            // `export default function f() {}` — `f` is reachable as a named binding in the modules
            // that auto-import it, and `default` is the name an `import x from` clause binds.
            ModuleDecl::ExportDefaultDecl(d) => {
                match &d.decl {
                    swc_core::ecma::ast::DefaultDecl::Class(c) => {
                        if let Some(id) = &c.ident {
                            push(id.sym.to_string());
                        }
                    }
                    swc_core::ecma::ast::DefaultDecl::Fn(f) => {
                        if let Some(id) = &f.ident {
                            push(id.sym.to_string());
                        }
                    }
                    swc_core::ecma::ast::DefaultDecl::TsInterfaceDecl(i) => {
                        push(i.id.sym.to_string());
                    }
                }
                push("default".to_string());
            }
            // `export default useFoo` — the identifier is the name Nuxt's auto-import publishes, and
            // the one every call site writes.
            ModuleDecl::ExportDefaultExpr(e) => {
                if let swc_core::ecma::ast::Expr::Ident(id) = &*e.expr {
                    push(id.sym.to_string());
                }
                push("default".to_string());
            }
            // `export * from "./y"` names nothing here — see the module doc.
            _ => {}
        }
    }
    out
}

/// The binding names an `export <decl>` publishes. Destructuring (`export const { a, b } = …`) is
/// walked, since each bound name is separately public.
fn decl_names(decl: &Decl, push: &mut impl FnMut(String)) {
    match decl {
        Decl::Class(c) => push(c.ident.sym.to_string()),
        Decl::Fn(f) => push(f.ident.sym.to_string()),
        Decl::Var(v) => {
            for d in &v.decls {
                pat_names(&d.name, push);
            }
        }
        Decl::TsInterface(i) => push(i.id.sym.to_string()),
        Decl::TsTypeAlias(t) => push(t.id.sym.to_string()),
        Decl::TsEnum(e) => push(e.id.sym.to_string()),
        // `export namespace N {}` / `export declare module "x" {}` — the id can be a string literal,
        // which is not a bare identifier anything can auto-import, so only the ident form is taken.
        Decl::TsModule(m) => {
            if let swc_core::ecma::ast::TsModuleName::Ident(id) = &m.id {
                push(id.sym.to_string());
            }
        }
        Decl::Using(_) => {}
    }
}

/// Every identifier a binding pattern binds.
fn pat_names(pat: &Pat, push: &mut impl FnMut(String)) {
    match pat {
        Pat::Ident(i) => push(i.id.sym.to_string()),
        Pat::Array(a) => {
            for e in a.elems.iter().flatten() {
                pat_names(e, push);
            }
        }
        Pat::Object(o) => {
            for p in &o.props {
                match p {
                    swc_core::ecma::ast::ObjectPatProp::KeyValue(kv) => pat_names(&kv.value, push),
                    swc_core::ecma::ast::ObjectPatProp::Assign(a) => push(a.key.sym.to_string()),
                    swc_core::ecma::ast::ObjectPatProp::Rest(r) => pat_names(&r.arg, push),
                }
            }
        }
        Pat::Rest(r) => pat_names(&r.arg, push),
        Pat::Assign(a) => pat_names(&a.left, push),
        Pat::Expr(_) | Pat::Invalid(_) => {}
    }
}

#[cfg(test)]
mod tests;
