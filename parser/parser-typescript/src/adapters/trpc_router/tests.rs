//! Coverage for `extract_procedure_router_fragments`: leaf verb detection, `Ref` specifier resolution,
//! inline `Nested` routers, string-literal/computed keys, `mergeRouters` ordering, and non-router
//! consts producing no fragment.

use super::extract_procedure_router_fragments;
use zzop_core::{ProcedureRouterEntry, ProcedureRouterFragment};

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
    let src = "export const r = router({\n  sub: proc.input(x).use(mw).subscription(fn),\n});\n";
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
