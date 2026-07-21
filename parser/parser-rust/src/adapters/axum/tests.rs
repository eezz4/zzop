use super::*;

fn frag<'a>(out: &'a [RouterMountFragment], name: &str) -> &'a RouterMountFragment {
    out.iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no fragment named {name:?} in {out:?}"))
}

#[test]
fn axum_verbs_match_http_key_verbs_vocabulary() {
    let upper: Vec<String> = VERB_METHODS.iter().map(|s| s.to_uppercase()).collect();
    let mut sorted_upper = upper.clone();
    sorted_upper.sort();
    let mut sorted_core: Vec<String> = zzop_core::HTTP_KEY_VERBS
        .iter()
        .map(|s| s.to_string())
        .collect();
    sorted_core.sort();
    assert_eq!(
        sorted_upper, sorted_core,
        "axum's VERB_METHODS must name the same HTTP-verb vocabulary as zzop_core::HTTP_KEY_VERBS"
    );
}

#[test]
fn no_axum_import_yields_nothing() {
    let src = "fn main() {\n    let app = Router::new().route(\"/x\", get(h));\n}\n";
    assert!(extract_axum_router_fragments("a.rs", src).is_empty());
}

#[test]
fn any_route_expands_to_every_http_verb() {
    // `any(handler)` is axum's every-method catch-all — it must expand to one Verb per HTTP_KEY_VERBS
    // (not vanish), keeping the route visible and its mutating surface reported.
    let src = concat!(
        "use axum::Router;\n",
        "use axum::routing::any;\n",
        "fn main() {\n",
        "    let app = Router::new().route(\"/proxy\", any(proxy));\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    let mut methods: Vec<&str> = frag(&out, "app")
        .entries
        .iter()
        .map(|e| match e {
            RouterMountEntry::Verb { method, path, .. } => {
                assert_eq!(path, "/proxy");
                method.as_str()
            }
            _ => panic!("expected Verb"),
        })
        .collect();
    methods.sort_unstable();
    assert_eq!(methods, vec!["DELETE", "GET", "PATCH", "POST", "PUT"]);
}

#[test]
fn concrete_verb_plus_any_on_one_route_does_not_duplicate_that_verb() {
    // `get(h).any(h2)` on one path: the concrete GET and the `any` expansion both yield GET /x. The
    // (method, path) dedup keeps exactly one GET so `duplicate-route` never sees a phantom second GET
    // from a single `.route()` registration — total is 5 verbs, not 6.
    let src = concat!(
        "use axum::Router;\n",
        "use axum::routing::get;\n",
        "fn main() {\n",
        "    let app = Router::new().route(\"/x\", get(h).any(h2));\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    let mut methods: Vec<&str> = frag(&out, "app")
        .entries
        .iter()
        .map(|e| match e {
            RouterMountEntry::Verb { method, path, .. } => {
                assert_eq!(path, "/x");
                method.as_str()
            }
            _ => panic!("expected Verb"),
        })
        .collect();
    methods.sort_unstable();
    assert_eq!(methods, vec!["DELETE", "GET", "PATCH", "POST", "PUT"]);
}

#[test]
fn single_let_chain_with_multiple_verbs() {
    let src = concat!(
        "use axum::Router;\n",
        "use axum::routing::get;\n",
        "fn main() {\n",
        "    let app = Router::new()\n",
        "        .route(\"/health\", get(health))\n",
        "        .route(\"/items\", get(list).post(create));\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    let app = frag(&out, "app");
    assert_eq!(app.entries.len(), 3);
    assert_eq!(
        app.entries[0],
        RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/health".into(),
            handler: Some("health".into()),
            line: 5,
            attr_keys: vec![],
        }
    );
    assert_eq!(
        app.entries[1],
        RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/items".into(),
            handler: Some("list".into()),
            line: 6,
            attr_keys: vec![],
        }
    );
    assert_eq!(
        app.entries[2],
        RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/items".into(),
            handler: Some("create".into()),
            line: 6,
            attr_keys: vec![],
        }
    );
}

#[test]
fn colon_and_brace_path_params_pass_through_raw() {
    let src = concat!(
        "use axum::Router;\n",
        "use axum::routing::get;\n",
        "fn main() {\n",
        "    let app = Router::new()\n",
        "        .route(\"/users/:id\", get(h1))\n",
        "        .route(\"/items/{id}\", get(h2));\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    let app = frag(&out, "app");
    let paths: Vec<&str> = app
        .entries
        .iter()
        .map(|e| match e {
            RouterMountEntry::Verb { path, .. } => path.as_str(),
            _ => panic!("expected verb"),
        })
        .collect();
    assert_eq!(paths, vec!["/users/:id", "/items/{id}"]);
}

#[test]
fn nest_with_literal_prefix_and_imported_child() {
    let src = concat!(
        "use axum::Router;\n",
        "use crate::routes::api_router;\n",
        "fn main() {\n",
        "    let app = Router::new().nest(\"/api\", api_router);\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/api".into(),
            ident: "api_router".into(),
            specifier: Some("crate::routes::api_router".into()),
            attr_keys: vec![],
        }]
    );
}

#[test]
fn nest_with_non_literal_prefix_is_skipped() {
    let src = concat!(
        "use axum::Router;\n",
        "fn main() {\n",
        "    let prefix = compute_prefix();\n",
        "    let app = Router::new().nest(prefix, api_router);\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    assert!(out.iter().all(|f| f.name != "app"), "{out:?}");
}

#[test]
fn merge_mounts_at_empty_prefix() {
    let src = concat!(
        "use axum::Router;\n",
        "fn main() {\n",
        "    let app = Router::new().merge(other_router);\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "".into(),
            ident: "other_router".into(),
            specifier: None,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn rebinding_via_shadowed_let_appends_to_the_same_fragment() {
    let src = concat!(
        "use axum::Router;\n",
        "use axum::routing::get;\n",
        "fn main() {\n",
        "    let app = Router::new().route(\"/a\", get(h1));\n",
        "    let app = app.route(\"/b\", get(h2));\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    assert_eq!(out.len(), 1);
    assert_eq!(frag(&out, "app").entries.len(), 2);
}

#[test]
fn rebinding_via_plain_reassignment_appends_to_the_same_fragment() {
    let src = concat!(
        "use axum::Router;\n",
        "use axum::routing::get;\n",
        "fn main() {\n",
        "    let mut app = Router::new().route(\"/a\", get(h1));\n",
        "    app = app.route(\"/b\", get(h2));\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    assert_eq!(out.len(), 1);
    assert_eq!(frag(&out, "app").entries.len(), 2);
}

#[test]
fn reassignment_from_an_unrelated_name_is_not_recognized() {
    let src = concat!(
        "use axum::Router;\n",
        "fn main() {\n",
        "    let mut app = Router::new();\n",
        "    app = other.route(\"/a\", get(h1));\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    // Neither statement contributes an entry: `Router::new()` alone has no chained calls to report, and
    // `other.route(...)`'s root ident ("other") does not match the reassignment target ("app").
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn standalone_tail_expression_uses_the_function_name() {
    let src = concat!(
        "use axum::Router;\n",
        "use axum::routing::get;\n",
        "fn make_app() -> Router {\n",
        "    Router::new().route(\"/x\", get(h))\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    assert_eq!(frag(&out, "make_app").entries.len(), 1);
}

#[test]
fn non_literal_handler_still_emits_an_entry_with_no_handler_name() {
    let src = concat!(
        "use axum::Router;\n",
        "use axum::routing::get;\n",
        "fn main() {\n",
        "    let app = Router::new().route(\"/x\", get(|| async {}));\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    match &frag(&out, "app").entries[0] {
        RouterMountEntry::Verb { handler, .. } => assert_eq!(*handler, None),
        other => panic!("expected verb, got {other:?}"),
    }
}

#[test]
fn unrecognized_chained_method_is_skipped_without_breaking_the_rest_of_the_chain() {
    let src = concat!(
        "use axum::Router;\n",
        "use axum::routing::get;\n",
        "fn main() {\n",
        "    let app = Router::new()\n",
        "        .route(\"/x\", get(h))\n",
        "        .layer(some_layer())\n",
        "        .route(\"/y\", get(h2));\n",
        "}\n",
    );
    let out = extract_axum_router_fragments("a.rs", src);
    assert_eq!(frag(&out, "app").entries.len(), 2);
}

#[test]
fn parse_failure_yields_empty_vec() {
    assert!(extract_axum_router_fragments("bad.rs", "fn f(:\n").is_empty());
}

#[test]
fn empty_file_yields_empty_vec() {
    assert!(extract_axum_router_fragments("e.rs", "").is_empty());
}
