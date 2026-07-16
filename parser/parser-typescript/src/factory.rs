//! Factory sub-symbol extraction — object-literal `key: value` members as `parent.key`
//! sub-symbols, with `...spread` flattening through same-file top-level const object literals.

use std::collections::{HashMap, HashSet};

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    Decl, Expr, Function, Module, ModuleDecl, ModuleItem, ObjectLit, Pat, Prop, PropName,
    PropOrSpread, Stmt, VarDeclarator,
};
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::line_of;

/// Top-level object literal consts (`const X = {...}`, incl. `export const X = {...}`) keyed by name — feeds factory spread flattening.
pub(crate) type ObjectLitMap = HashMap<String, ObjectLit>;

pub(crate) fn collect_top_level_object_lits(module: &Module) -> ObjectLitMap {
    let mut map = ObjectLitMap::new();
    for item in &module.body {
        let decls: &[VarDeclarator] = match item {
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(v))) => &v.decls,
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => match &e.decl {
                Decl::Var(v) => &v.decls,
                _ => continue,
            },
            _ => continue,
        };
        for d in decls {
            if let Pat::Ident(bi) = &d.name {
                if let Some(Expr::Object(obj)) = d.init.as_deref() {
                    map.insert(bi.id.sym.to_string(), obj.clone());
                }
            }
        }
    }
    map
}

/// Extracts `fn.method` sub-symbols when a function body contains `return { ... }`; spread (`...other`) is flattened up to 2 hops when `other` is a same-file top-level const ObjectLit.
pub(crate) fn extract_factory_methods(
    cm: &SourceMap,
    file: &str,
    fn_name: &str,
    function: &Function,
    object_lits_by_name: &ObjectLitMap,
    out: &mut Vec<SourceSymbol>,
) {
    let Some(body) = &function.body else {
        return;
    };
    for stmt in &body.stmts {
        let Stmt::Return(ret) = stmt else { continue };
        let Some(Expr::Object(obj)) = ret.arg.as_deref() else {
            continue;
        };
        extract_object_methods(
            cm,
            file,
            fn_name,
            obj,
            object_lits_by_name,
            &mut HashSet::new(),
            out,
        );
    }
}

/// Extracts object-literal `key: value` properties as `parent.key` sub-symbols (method-shorthand /
/// getter / setter / plain-shorthand members are skipped). `...other` spreads are flattened when `other`
/// resolves to a same-file top-level const ObjectLit; `visited` guards against spread cycles.
pub(crate) fn extract_object_methods(
    cm: &SourceMap,
    file: &str,
    parent: &str,
    obj: &ObjectLit,
    object_lits_by_name: &ObjectLitMap,
    visited: &mut HashSet<String>,
    out: &mut Vec<SourceSymbol>,
) {
    let mut seen_names: HashSet<String> = HashSet::new();
    let prefix = format!("{parent}.");
    for prop in &obj.props {
        match prop {
            PropOrSpread::Spread(sp) => {
                let Expr::Ident(id) = &*sp.expr else { continue };
                let target_name = id.sym.to_string();
                if visited.contains(&target_name) {
                    continue;
                }
                let Some(target) = object_lits_by_name.get(&target_name) else {
                    continue;
                };
                visited.insert(target_name);
                let mut inner = Vec::new();
                extract_object_methods(
                    cm,
                    file,
                    parent,
                    target,
                    object_lits_by_name,
                    visited,
                    &mut inner,
                );
                for sym in inner {
                    let base_name = sym
                        .name
                        .strip_prefix(&prefix)
                        .unwrap_or(&sym.name)
                        .to_string();
                    if seen_names.contains(&base_name) {
                        continue;
                    }
                    seen_names.insert(base_name);
                    out.push(sym);
                }
            }
            PropOrSpread::Prop(p) => {
                let Prop::KeyValue(kv) = &**p else { continue };
                let PropName::Ident(name_id) = &kv.key else {
                    continue;
                };
                let name = name_id.sym.to_string();
                if seen_names.contains(&name) {
                    continue;
                }
                seen_names.insert(name.clone());
                let (is_fn, body_start, body_end) = match &*kv.value {
                    Expr::Arrow(a) => (
                        true,
                        Some(line_of(cm, a.span.lo)),
                        Some(line_of(cm, a.span.hi)),
                    ),
                    Expr::Fn(f) => (
                        true,
                        Some(line_of(cm, f.function.span.lo)),
                        Some(line_of(cm, f.function.span.hi)),
                    ),
                    _ => (false, None, None),
                };
                let full = format!("{parent}.{name}");
                out.push(SourceSymbol {
                    id: format!("{file}#{full}"),
                    file: file.into(),
                    name: full,
                    kind: if is_fn {
                        SourceSymbolKind::Function
                    } else {
                        SourceSymbolKind::Const
                    },
                    line: line_of(cm, name_id.span.lo),
                    exported: false,
                    is_default: false,
                    body_start,
                    body_end,
                    write_sites: Vec::new(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_symbols;
    use crate::test_util::names;
    use zzop_core::SourceSymbolKind as K;

    // --- parseSymbols factory sub-symbols ---

    #[test]
    fn factory_const_object_literal_methods() {
        let s = parse_symbols(
            "x.ts",
            "export const api = {\n  getA: () => 1,\n  getB: async () => 2,\n};\n",
        );
        assert_eq!(names(&s), vec!["api", "api.getA", "api.getB"]);
        assert_eq!(
            s.iter().find(|s| s.name == "api.getA").unwrap().kind,
            K::Function
        );
    }

    #[test]
    fn factory_function_return_object() {
        let s = parse_symbols(
            "x.ts",
            "export function createApi(deps) {\n  return {\n    deleteMe: async () => 1,\n    getUser: async () => 2,\n  };\n}\n",
        );
        assert_eq!(
            names(&s),
            vec!["createApi", "createApi.deleteMe", "createApi.getUser"]
        );
    }

    #[test]
    fn factory_spread_two_hop_flatten() {
        let s = parse_symbols(
            "x.ts",
            "const base = {\n  shared: () => 1,\n};\nfunction createApi() {\n  return { ...base, direct: () => 2 };\n}\n",
        );
        assert_eq!(
            names(&s),
            vec![
                "base",
                "base.shared",
                "createApi",
                "createApi.shared",
                "createApi.direct"
            ]
        );
    }

    #[test]
    fn factory_spread_target_not_in_file_skipped() {
        let s = parse_symbols(
            "x.ts",
            "function createApi() {\n  return { ...unknown, direct: () => 1 };\n}\n",
        );
        assert_eq!(names(&s), vec!["createApi", "createApi.direct"]);
    }

    #[test]
    fn factory_spread_cycle_prevented() {
        // depth-first flattening — visited guard cuts a->b->a after one pass; no infinite loop, each key appears exactly once.
        let s = parse_symbols(
            "x.ts",
            "const a = { ...b, x: 1 };\nconst b = { ...a, y: 1 };\nfunction createApi() {\n  return { ...a, z: () => 1 };\n}\n",
        );
        assert_eq!(
            names(&s),
            vec![
                "a",
                "a.x",
                "a.y",
                "b",
                "b.y",
                "b.x",
                "createApi",
                "createApi.y",
                "createApi.x",
                "createApi.z"
            ]
        );
    }

    #[test]
    fn factory_spread_priority_later_key_wins() {
        // spread processed first so `foo` is inserted; a subsequent PropertyAssignment `foo` is a duplicate id -> skipped.
        let s = parse_symbols(
            "x.ts",
            "const base = { foo: () => 1 };\nfunction createApi() {\n  return { ...base, foo: () => 2, bar: () => 3 };\n}\n",
        );
        assert_eq!(
            names(&s),
            vec![
                "base",
                "base.foo",
                "createApi",
                "createApi.foo",
                "createApi.bar"
            ]
        );
    }

    #[test]
    fn factory_const_object_lit_spread_two_hop() {
        let s = parse_symbols(
            "x.ts",
            "const base = { foo: () => 1 };\nexport const ext = { ...base, bar: () => 2 };\n",
        );
        assert_eq!(
            names(&s),
            vec!["base", "base.foo", "ext", "ext.foo", "ext.bar"]
        );
    }
}
