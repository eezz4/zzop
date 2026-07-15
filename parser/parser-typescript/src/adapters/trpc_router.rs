//! tRPC ROUTER-FRAGMENT extractor — projects the STATIC shape of every top-level `router({...})` /
//! `createTRPCRouter({...})` initializer in one TS file into a [`ProcedureRouterFragment`]: an ordered list of
//! [`ProcedureRouterEntry`] describing what each object key resolves to (a leaf procedure, an inline nested
//! router, or a reference to a sub-router by identifier).
//!
//! ## Why a fragment, not an `IoProvide`
//! A tRPC router composes across files: `viewerRouter` typically imports `bookingsRouter` from
//! `./bookings/_router` and re-mounts it under a key, so the FULL route path for a leaf several
//! sub-routers deep (`viewerRouter.bookings.create` -> key path `bookings.create`) is only knowable
//! once every file's fragment has been assembled together. This module reports exactly what THIS
//! file's text says, as a small ordered tree, and leaves the cross-file composition to the engine's
//! assembly pass.
//!
//! ## Recognized vocabulary (v1)
//! Router factory callee: a plain identifier call, `router(...)` or `createTRPCRouter(...)`, matched
//! by lexical name only, captured from `const <name> = router({...})` at module top level after
//! unwrapping `as`/`(...)`/`satisfies`/`!` wrappers. Object keys are a plain identifier or
//! string-literal key; a **computed key** (`[someExpr()]: ...`) skips just that one entry. A property
//! value is classified in this order: (1) a bare identifier -> [`ProcedureRouterEntry::Ref`] (`specifier` =
//! `Some(source)` when the ident is one of this file's own import bindings, `None` otherwise —
//! assumed a same-file local router); (2) a call to `router(...)`/`createTRPCRouter(...)` ->
//! [`ProcedureRouterEntry::Nested`], recursing the same way; (3) a builder-chain call with a
//! `query`/`mutation`/`subscription` member call anywhere down the chain ->
//! [`ProcedureRouterEntry::Leaf`] (e.g. `authedProcedure.input(z...).use(mw).mutation(fn)` is MUTATION);
//! (4) anything else is skipped.
//!
//! `mergeRouters(a, b, ...)` as the top-level initializer produces one
//! `ProcedureRouterEntry::Ref { key: String::new(), ident, specifier }` per bare-identifier argument, in
//! argument order — the empty key signals "splice this sub-router's entries in here" (there is no
//! key: `mergeRouters` flattens its arguments into ONE namespace). A non-identifier argument is
//! skipped, not recursed into — v1 only recognizes a plain sub-router
//! `mergeRouters(fooRouter, barRouter)` call. A top-level `const` whose initializer is neither a
//! router factory nor `mergeRouters(...)` produces no fragment at all.
//!
//! Object-literal spread properties (`...someSubRouter`) are not expanded into entries — skipped, same
//! "never guess" stance as everything else above. Shorthand object properties (`{ bookings }`) ARE
//! supported as a `Ref` to the same-named identifier.

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    Callee, Decl, Expr, MemberProp, ModuleDecl, ModuleItem, ObjectLit, Pat, Prop, PropName,
    PropOrSpread, Stmt,
};
use zzop_core::{ImportMap, ProcedureRouterEntry, ProcedureRouterFragment};

/// Extract every top-level tRPC router fragment from one file's raw source. Returns an empty `Vec`
/// (never panics) when the file fails to parse at all.
pub fn extract_procedure_router_fragments(rel: &str, text: &str) -> Vec<ProcedureRouterFragment> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let imports = crate::parse_imports(rel, text);
    let mut out = Vec::new();
    for item in &module.body {
        let decl = match item {
            ModuleItem::Stmt(Stmt::Decl(d)) => Some(d),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => Some(&e.decl),
            _ => None,
        };
        let Some(Decl::Var(v)) = decl else { continue };
        for d in &v.decls {
            let Pat::Ident(bi) = &d.name else { continue };
            let Some(init) = &d.init else { continue };
            let name = bi.id.sym.to_string();
            if let Some(fragment) = classify_top_level_init(name, init, &imports, &cm) {
                out.push(fragment);
            }
        }
    }
    out
}

/// Classifies one top-level `const <name> = <init>` binding as a router fragment, or `None` when `init`
/// is neither a recognized router-factory call nor `mergeRouters(...)` — see module doc.
fn classify_top_level_init(
    name: String,
    init: &Expr,
    imports: &ImportMap,
    cm: &SourceMap,
) -> Option<ProcedureRouterFragment> {
    let Expr::Call(call) = unwrap_expr(init) else {
        return None;
    };
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Ident(id) = unwrap_expr(callee) else {
        return None;
    };
    if is_router_factory(&id.sym) {
        let entries = call
            .args
            .first()
            .and_then(|a| match unwrap_expr(&a.expr) {
                Expr::Object(o) => Some(o),
                _ => None,
            })
            .map(|o| parse_object_entries(o, imports, cm))
            .unwrap_or_default();
        Some(ProcedureRouterFragment { name, entries })
    } else if id.sym == "mergeRouters" {
        let mut entries = Vec::new();
        for arg in &call.args {
            if arg.spread.is_some() {
                continue; // a spread argument — not a plain sub-router ident, never guessed
            }
            if let Expr::Ident(aid) = unwrap_expr(&arg.expr) {
                entries.push(ProcedureRouterEntry::Ref {
                    key: String::new(),
                    ident: aid.sym.to_string(),
                    specifier: resolve_specifier(&aid.sym, imports),
                });
            }
            // a non-identifier argument (inline router(...), nested mergeRouters(...), literal, ...) is skipped.
        }
        Some(ProcedureRouterFragment { name, entries })
    } else {
        None
    }
}

fn is_router_factory(name: &str) -> bool {
    matches!(name, "router" | "createTRPCRouter")
}

/// `ident`'s import source when it is one of this file's own import bindings, `None` otherwise
/// (assumed same-file local).
fn resolve_specifier(ident: &str, imports: &ImportMap) -> Option<String> {
    imports.get(ident).map(|b| b.specifier.clone())
}

/// Parses one `router({...})` call's object-literal argument into its ordered entries — recursed into
/// for inline `Nested` routers.
fn parse_object_entries(
    obj: &ObjectLit,
    imports: &ImportMap,
    cm: &SourceMap,
) -> Vec<ProcedureRouterEntry> {
    let mut out = Vec::new();
    for prop in &obj.props {
        let PropOrSpread::Prop(p) = prop else {
            continue; // `...spread` property — not expanded, see module doc
        };
        let entry = match &**p {
            Prop::KeyValue(kv) => {
                let Some(key) = prop_key_name(&kv.key) else {
                    continue; // computed/num/bigint key — never guessed, skip just this entry
                };
                classify_entry(key, &kv.value, imports, cm)
            }
            Prop::Shorthand(id) => {
                // `{ bookings }` sugar for `{ bookings: bookings }` — see module doc.
                classify_entry(id.sym.to_string(), &Expr::Ident(id.clone()), imports, cm)
            }
            _ => None, // method/getter/setter/assign shorthand — not a recognized entry shape
        };
        if let Some(entry) = entry {
            out.push(entry);
        }
    }
    out
}

fn prop_key_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}

/// Classifies one object property's value expression into a `ProcedureRouterEntry` — see module doc.
fn classify_entry(
    key: String,
    value: &Expr,
    imports: &ImportMap,
    cm: &SourceMap,
) -> Option<ProcedureRouterEntry> {
    match unwrap_expr(value) {
        Expr::Ident(id) => Some(ProcedureRouterEntry::Ref {
            key,
            ident: id.sym.to_string(),
            specifier: resolve_specifier(&id.sym, imports),
        }),
        Expr::Call(call) => {
            let Callee::Expr(callee) = &call.callee else {
                return None;
            };
            if let Expr::Ident(id) = unwrap_expr(callee) {
                if is_router_factory(&id.sym) {
                    let entries = call
                        .args
                        .first()
                        .and_then(|a| match unwrap_expr(&a.expr) {
                            Expr::Object(o) => Some(o),
                            _ => None,
                        })
                        .map(|o| parse_object_entries(o, imports, cm))
                        .unwrap_or_default();
                    return Some(ProcedureRouterEntry::Nested { key, entries });
                }
            }
            let verb = verb_of_call_chain(call)?;
            Some(ProcedureRouterEntry::Leaf {
                key,
                verb,
                line: crate::line_of(cm, call.span.lo),
            })
        }
        _ => None, // never guess — literal, conditional, other expression shape
    }
}

/// Walks a builder-chain call's callee links (`x.a(...).b(...).c(...)`, outermost call first, since swc
/// nests each earlier step inside the next call's `callee.obj`) looking for a member call named
/// `query`/`mutation`/`subscription`. Returns the uppercase verb on the first match, or `None`.
fn verb_of_call_chain(call: &swc_core::ecma::ast::CallExpr) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(m) = unwrap_expr(callee) else {
        return None;
    };
    if let MemberProp::Ident(name) = &m.prop {
        if let Some(verb) = verb_name(&name.sym) {
            return Some(verb.to_string());
        }
    }
    match unwrap_expr(&m.obj) {
        Expr::Call(inner) => verb_of_call_chain(inner),
        _ => None,
    }
}

fn verb_name(s: &str) -> Option<&'static str> {
    match s {
        "query" => Some("QUERY"),
        "mutation" => Some("MUTATION"),
        "subscription" => Some("SUBSCRIPTION"),
        _ => None,
    }
}

/// Strip wrappers between a value position and its real expression: `... as const`/`... as T`, `(...)`,
/// `... satisfies T`, `...!` — identical set to `egress.rs`'s own `unwrap_expr`.
fn unwrap_expr(e: &Expr) -> &Expr {
    let mut n = e;
    loop {
        n = match n {
            Expr::TsAs(a) => &a.expr,
            Expr::TsConstAssertion(c) => &c.expr,
            Expr::Paren(p) => &p.expr,
            Expr::TsSatisfies(s) => &s.expr,
            Expr::TsNonNull(nn) => &nn.expr,
            other => return other,
        };
    }
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_procedure_router_fragments`: leaf verb detection, `Ref` specifier resolution,
    //! inline `Nested` routers, string-literal/computed keys, `mergeRouters` ordering, and non-router
    //! consts producing no fragment.
    use super::*;

    fn frag<'a>(out: &'a [ProcedureRouterFragment], name: &str) -> &'a ProcedureRouterFragment {
        out.iter().find(|f| f.name == name).unwrap()
    }

    #[test]
    fn leaf_plain_mutation_and_query_builder_chain() {
        let src = concat!(
            "export const appRouter = router({\n",
            "  hello: publicProcedure.query(() => \"hi\"),\n",
            "  create: authedProcedure.input(z.object({})).mutation(async () => 1),\n",
            "});\n"
        );
        let out = extract_procedure_router_fragments("router.ts", src);
        let f = frag(&out, "appRouter");
        assert_eq!(
            f.entries,
            vec![
                ProcedureRouterEntry::Leaf {
                    key: "hello".into(),
                    verb: "QUERY".into(),
                    line: 2
                },
                ProcedureRouterEntry::Leaf {
                    key: "create".into(),
                    verb: "MUTATION".into(),
                    line: 3
                },
            ]
        );
    }

    #[test]
    fn leaf_verb_found_through_a_use_middleware_link() {
        let src =
            "export const r = router({\n  sub: proc.input(x).use(mw).subscription(fn),\n});\n";
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            frag(&out, "r").entries,
            vec![ProcedureRouterEntry::Leaf {
                key: "sub".into(),
                verb: "SUBSCRIPTION".into(),
                line: 2
            }]
        );
    }

    #[test]
    fn ref_with_import_specifier_resolved_from_this_files_imports() {
        let src = concat!(
            "import { router } from \"../../trpc\";\n",
            "import { bookingsRouter } from \"./bookings/_router\";\n",
            "export const viewerRouter = router({\n",
            "  bookings: bookingsRouter,\n",
            "});\n"
        );
        let out = extract_procedure_router_fragments("viewer/_router.ts", src);
        assert_eq!(
            frag(&out, "viewerRouter").entries,
            vec![ProcedureRouterEntry::Ref {
                key: "bookings".into(),
                ident: "bookingsRouter".into(),
                specifier: Some("./bookings/_router".into()),
            }]
        );
    }

    #[test]
    fn ref_to_a_same_file_local_router_has_no_specifier() {
        let src = concat!(
            "import { router } from \"../../trpc\";\n",
            "const inner = router({});\n",
            "export const outer = router({\n",
            "  nested: inner,\n",
            "});\n"
        );
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            frag(&out, "outer").entries,
            vec![ProcedureRouterEntry::Ref {
                key: "nested".into(),
                ident: "inner".into(),
                specifier: None,
            }]
        );
        // The same-file `inner` router is itself captured as its own (empty) fragment.
        assert_eq!(frag(&out, "inner").entries, Vec::new());
    }

    #[test]
    fn shorthand_property_is_a_ref_to_the_same_named_ident() {
        let src = concat!(
            "import { router } from \"../../trpc\";\n",
            "import { bookings } from \"./bookings/_router\";\n",
            "export const viewerRouter = router({\n  bookings,\n});\n"
        );
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            frag(&out, "viewerRouter").entries,
            vec![ProcedureRouterEntry::Ref {
                key: "bookings".into(),
                ident: "bookings".into(),
                specifier: Some("./bookings/_router".into()),
            }]
        );
    }

    #[test]
    fn inline_nested_router() {
        let src = concat!(
            "export const viewerRouter = router({\n",
            "  greeting: router({\n",
            "    hello: publicProcedure.query(() => \"hi\"),\n",
            "  }),\n",
            "});\n"
        );
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            frag(&out, "viewerRouter").entries,
            vec![ProcedureRouterEntry::Nested {
                key: "greeting".into(),
                entries: vec![ProcedureRouterEntry::Leaf {
                    key: "hello".into(),
                    verb: "QUERY".into(),
                    line: 3
                }],
            }]
        );
    }

    #[test]
    fn create_trpc_router_callee_name_is_also_recognized() {
        let src = "export const appRouter = createTRPCRouter({\n  a: proc.query(fn),\n});\n";
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            frag(&out, "appRouter").entries,
            vec![ProcedureRouterEntry::Leaf {
                key: "a".into(),
                verb: "QUERY".into(),
                line: 2
            }]
        );
    }

    #[test]
    fn string_literal_key_is_supported() {
        let src = "export const r = router({\n  \"weird-key\": proc.query(fn),\n});\n";
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            frag(&out, "r").entries,
            vec![ProcedureRouterEntry::Leaf {
                key: "weird-key".into(),
                verb: "QUERY".into(),
                line: 2
            }]
        );
    }

    #[test]
    fn computed_key_is_skipped_sibling_entries_survive() {
        let src = concat!(
            "export const r = router({\n",
            "  [dynamicKey()]: proc.query(fn),\n",
            "  ok: proc.query(fn),\n",
            "});\n"
        );
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            frag(&out, "r").entries,
            vec![ProcedureRouterEntry::Leaf {
                key: "ok".into(),
                verb: "QUERY".into(),
                line: 3
            }]
        );
    }

    #[test]
    fn merge_routers_yields_empty_key_refs_in_argument_order() {
        let src = concat!(
            "import { mergeRouters } from \"../../trpc\";\n",
            "import { aRouter } from \"./a\";\n",
            "import { bRouter } from \"./b\";\n",
            "export const combined = mergeRouters(aRouter, bRouter);\n"
        );
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            frag(&out, "combined").entries,
            vec![
                ProcedureRouterEntry::Ref {
                    key: String::new(),
                    ident: "aRouter".into(),
                    specifier: Some("./a".into()),
                },
                ProcedureRouterEntry::Ref {
                    key: String::new(),
                    ident: "bRouter".into(),
                    specifier: Some("./b".into()),
                },
            ]
        );
    }

    #[test]
    fn merge_routers_non_ident_argument_is_skipped() {
        let src = concat!(
            "import { mergeRouters } from \"../../trpc\";\n",
            "import { aRouter } from \"./a\";\n",
            "export const combined = mergeRouters(aRouter, router({}));\n"
        );
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            frag(&out, "combined").entries,
            vec![ProcedureRouterEntry::Ref {
                key: String::new(),
                ident: "aRouter".into(),
                specifier: Some("./a".into()),
            }]
        );
    }

    #[test]
    fn non_router_consts_produce_no_fragment() {
        let src = "export const PAGE_SIZE = 10;\nconst helper = () => 1;\n";
        let out = extract_procedure_router_fragments("r.ts", src);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn multiple_fragments_in_one_file_keep_declaration_order() {
        let src = concat!(
            "const a = router({ x: proc.query(fn) });\n",
            "const b = router({ y: proc.query(fn) });\n"
        );
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            out.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn empty_file_yields_no_fragments() {
        assert!(extract_procedure_router_fragments("e.ts", "").is_empty());
    }

    #[test]
    fn as_const_and_paren_wrappers_are_unwrapped_on_the_initializer() {
        let src = "export const r = (router({ a: proc.query(fn) })) as any;\n";
        let out = extract_procedure_router_fragments("r.ts", src);
        assert_eq!(
            frag(&out, "r").entries,
            vec![ProcedureRouterEntry::Leaf {
                key: "a".into(),
                verb: "QUERY".into(),
                line: 1
            }]
        );
    }
}
