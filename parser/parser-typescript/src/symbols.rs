//! Top-level declaration -> `SourceSymbol` projection (`parse_symbols`): function/class/interface/
//! type/const + `export default` fn/class, factory sub-symbols (via `factory`), binding-pattern
//! consts, and CommonJS exports (via `cjs_exports`). Symbol constructors live in `symbol_shapes`.

use std::collections::HashSet;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    Decl, DefaultDecl, Expr, ModuleDecl, ModuleItem, Pat, Stmt, VarDeclarator,
};
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::cjs_exports::collect_common_js_exports;
use crate::factory::{
    collect_top_level_object_lits, extract_factory_methods, extract_object_methods, ObjectLitMap,
};
use crate::symbol_shapes::{
    collect_binding_names, emit_class, fn_symbol, is_require_init, simple_symbol,
};
use crate::{lang, line_of, parse_with_cm};

/// Top-level declarations -> `SourceSymbol[]`: function/class/interface/type/const + `export default`
/// fn/class, factory sub-symbols, binding-pattern consts, and CommonJS exports. Declaration order preserved.
pub fn parse_symbols(file: &str, source: &str) -> Vec<SourceSymbol> {
    let Some((cm, module)) = parse_with_cm(file, source) else {
        return Vec::new();
    };
    let object_lits_by_name = collect_top_level_object_lits(&module);
    let mut out = Vec::new();
    for item in &module.body {
        match item {
            ModuleItem::Stmt(Stmt::Decl(decl)) => emit_decl(
                &cm,
                file,
                decl,
                false,
                false,
                &object_lits_by_name,
                &mut out,
            ),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => emit_decl(
                &cm,
                file,
                &e.decl,
                true,
                false,
                &object_lits_by_name,
                &mut out,
            ),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(e)) => match &e.decl {
                DefaultDecl::Fn(fe) => {
                    let name = fe
                        .ident
                        .as_ref()
                        .map_or_else(|| "default".into(), |i| i.sym.to_string());
                    out.push(fn_symbol(&cm, file, name.clone(), &fe.function, true, true));
                    extract_factory_methods(
                        &cm,
                        file,
                        &name,
                        &fe.function,
                        &object_lits_by_name,
                        &mut out,
                    );
                }
                DefaultDecl::Class(ce) => {
                    let name = ce
                        .ident
                        .as_ref()
                        .map_or_else(|| "default".into(), |i| i.sym.to_string());
                    emit_class(&cm, file, name, &ce.class, true, true, &mut out);
                }
                DefaultDecl::TsInterfaceDecl(_) => {}
            },
            _ => {}
        }
    }
    // CommonJS exports (module.exports / exports.x = ...) — additive; names already emitted above win (deduped here).
    let declared: HashSet<String> = out.iter().map(|s| s.name.clone()).collect();
    for cjs in collect_common_js_exports(&cm, file, &module) {
        if !declared.contains(&cjs.name) {
            out.push(cjs);
        }
    }
    // Write-site detection is a pure function of (this symbol's own body span, constant vocab), so it
    // runs as a final pass over the fully-built list rather than being threaded through every symbol
    // constructor above.
    for sym in &mut out {
        sym.write_sites = lang::write_site::write_sites_for_symbol(sym, source);
    }
    out
}

fn emit_decl(
    cm: &SourceMap,
    file: &str,
    decl: &Decl,
    exported: bool,
    is_default: bool,
    object_lits_by_name: &ObjectLitMap,
    out: &mut Vec<SourceSymbol>,
) {
    match decl {
        Decl::Fn(f) => {
            let name = f.ident.sym.to_string();
            out.push(fn_symbol(
                cm,
                file,
                name.clone(),
                &f.function,
                exported,
                is_default,
            ));
            extract_factory_methods(cm, file, &name, &f.function, object_lits_by_name, out);
        }
        Decl::Class(c) => emit_class(
            cm,
            file,
            c.ident.sym.to_string(),
            &c.class,
            exported,
            is_default,
            out,
        ),
        Decl::TsInterface(i) => out.push(simple_symbol(
            cm,
            file,
            i.id.sym.to_string(),
            SourceSymbolKind::Interface,
            i.span.lo,
            exported,
        )),
        Decl::TsTypeAlias(t) => out.push(simple_symbol(
            cm,
            file,
            t.id.sym.to_string(),
            SourceSymbolKind::Type,
            t.span.lo,
            exported,
        )),
        Decl::Var(v) => {
            for d in &v.decls {
                emit_var_declarator(cm, file, d, exported, object_lits_by_name, out);
            }
        }
        _ => {}
    }
}

fn emit_var_declarator(
    cm: &SourceMap,
    file: &str,
    d: &VarDeclarator,
    exported: bool,
    object_lits_by_name: &ObjectLitMap,
    out: &mut Vec<SourceSymbol>,
) {
    match &d.name {
        Pat::Ident(bi) => {
            // `var X = require('...')` is an import alias (owned by parseImports) -> skip.
            if is_require_init(d) {
                return;
            }
            let name = bi.id.sym.to_string();
            let line = line_of(cm, d.span.lo);
            let fn_span = match d.init.as_deref() {
                Some(Expr::Arrow(a)) => Some(a.span),
                Some(Expr::Fn(f)) => Some(f.function.span),
                _ => None,
            };
            let (kind, body_start, body_end) = match fn_span {
                Some(sp) => (
                    SourceSymbolKind::Function,
                    Some(line_of(cm, sp.lo)),
                    Some(line_of(cm, sp.hi)),
                ),
                None => (SourceSymbolKind::Const, None, None),
            };
            out.push(SourceSymbol {
                id: format!("{file}#{name}"),
                file: file.into(),
                name: name.clone(),
                kind,
                line,
                exported,
                is_default: false,
                body_start,
                body_end,
                write_sites: Vec::new(),
            });
            // Factory: `const api = { m: () => {} }` -> api.m sub-symbols.
            if let Some(Expr::Object(obj)) = d.init.as_deref() {
                extract_object_methods(
                    cm,
                    file,
                    &name,
                    obj,
                    object_lits_by_name,
                    &mut HashSet::new(),
                    out,
                );
            }
        }
        Pat::Object(_) | Pat::Array(_) => {
            // `const { a, b } = ...` / `const [x] = ...` -> one const symbol per binding.
            let line = line_of(cm, d.span.lo);
            for name in collect_binding_names(&d.name) {
                out.push(SourceSymbol {
                    id: format!("{file}#{name}"),
                    file: file.into(),
                    name,
                    kind: SourceSymbolKind::Const,
                    line,
                    exported,
                    is_default: false,
                    body_start: None,
                    body_end: None,
                    write_sites: Vec::new(),
                });
            }
        }
        _ => {}
    }
}
