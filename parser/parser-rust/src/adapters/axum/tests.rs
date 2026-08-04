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

// --- test surface is excluded (2026-08-02) ------------------------------------------------------------
// The last of this crate's three adapters to get the gate. A fixture's `Router::new().route(
// "/admin/reset", post(h))` was minted as a DEPLOYED provide and entered the cross-layer join as one.

/// The one shipped path every attribute-gated fixture below sits at, so nothing here can pass because
/// of the PATH gate instead of the gate it names. The path gate has its own test.
const SHIPPED_REL: &str = "src/router.rs";

/// `(name, source)` — shapes that DID leak a deployed route before the gate, measured 2026-08-02.
const TEST_GATED_ROUTERS: &[(&str, &str)] = &[
    (
        "file-level inner #![cfg(test)]",
        "#![cfg(test)]\nuse axum::Router;\nuse axum::routing::post;\nfn t() {\n    let app = Router::new().route(\"/admin/reset\", post(h));\n}\n",
    ),
    (
        "#[cfg(test)] fn at top level",
        "use axum::Router;\nuse axum::routing::post;\n#[cfg(test)]\nfn t() {\n    let app = Router::new().route(\"/admin/reset\", post(h));\n}\n",
    ),
    (
        "#[test] fn at top level",
        "use axum::Router;\nuse axum::routing::post;\n#[test]\nfn t() {\n    let app = Router::new().route(\"/admin/reset\", post(h));\n}\n",
    ),
    (
        "#[cfg(all(test, not(miri)))] fn",
        "use axum::Router;\nuse axum::routing::post;\n#[cfg(all(test, not(miri)))]\nfn t() {\n    let app = Router::new().route(\"/admin/reset\", post(h));\n}\n",
    ),
];

/// Shapes that were ALREADY silent before the gate — not because anything suppressed them, but because
/// this adapter reads `File::items` and scans only `Item::Fn`, so an `impl`, a `trait` and a nested
/// `mod` are out of its reach entirely. Pinned because the module doc claims exactly that, and it is
/// what makes the missing `ImplItem`/`TraitItem` axes (which both sibling adapters do carry) correct
/// here rather than a gap.
const OUT_OF_REACH_ROUTERS: &[(&str, &str)] = &[
    (
        "#[cfg(test)] mod tests",
        "use axum::Router;\nuse axum::routing::post;\n#[cfg(test)]\nmod tests {\n    fn t() {\n        let app = Router::new().route(\"/admin/reset\", post(h));\n    }\n}\n",
    ),
    (
        "#[test] fn inside an impl block",
        "use axum::Router;\nuse axum::routing::post;\nstruct S;\nimpl S {\n    #[test]\n    fn t() {\n        let app = Router::new().route(\"/admin/reset\", post(h));\n    }\n}\n",
    ),
    (
        "#[cfg(test)] default trait method",
        "use axum::Router;\nuse axum::routing::post;\ntrait T {\n    #[cfg(test)]\n    fn t() {\n        let app = Router::new().route(\"/admin/reset\", post(h));\n    }\n}\n",
    ),
];

#[test]
fn a_fixtures_route_is_not_a_deployed_provide() {
    for (name, src) in TEST_GATED_ROUTERS.iter().chain(OUT_OF_REACH_ROUTERS) {
        let out = extract_axum_router_fragments(SHIPPED_REL, src);
        assert!(out.is_empty(), "{name}: fixture route leaked — got {out:?}");
    }
}

#[test]
fn a_test_path_file_yields_nothing() {
    // The gate both sibling adapters have and this one did not: a router in `tests/it.rs` carries no
    // attribute at all, so only the PATH axis can see it.
    let out = extract_axum_router_fragments(
        "tests/it.rs",
        "use axum::Router;\nuse axum::routing::post;\nfn t() {\n    let app = Router::new().route(\"/admin/reset\", post(h));\n}\n",
    );
    assert!(out.is_empty(), "got {out:?}");
}

#[test]
fn the_same_router_in_shipped_code_still_provides_it() {
    // The BIDIRECTIONAL half — without it every assertion above passes just as well on a dead extractor.
    // Each source is a gated fixture with only its gate changed to one that SHIPS.
    let shipping = [
        (
            "no attribute",
            "use axum::Router;\nuse axum::routing::post;\nfn ship() {\n    let app = Router::new().route(\"/admin/reset\", post(h));\n}\n",
        ),
        (
            // The exact inverse of `cfg(test)`: this code is compiled OUT of the test build and INTO
            // the release binary. Reading it as test-only is the mistake `implies_test` exists to avoid.
            "#[cfg(not(test))]",
            "use axum::Router;\nuse axum::routing::post;\n#[cfg(not(test))]\nfn ship() {\n    let app = Router::new().route(\"/admin/reset\", post(h));\n}\n",
        ),
        (
            // Ships whenever the feature is on, so gating it would delete a route a user really serves.
            "#[cfg(any(test, feature = \"testkit\"))]",
            "use axum::Router;\nuse axum::routing::post;\n#[cfg(any(test, feature = \"testkit\"))]\nfn ship() {\n    let app = Router::new().route(\"/admin/reset\", post(h));\n}\n",
        ),
    ];
    for (name, src) in shipping {
        let out = extract_axum_router_fragments(SHIPPED_REL, src);
        assert_eq!(
            frag(&out, "app").entries,
            vec![RouterMountEntry::Verb {
                method: "POST".into(),
                path: "/admin/reset".into(),
                handler: Some("h".into()),
                line: if name == "no attribute" { 4 } else { 5 },
                attr_keys: vec![],
            }],
            "{name}"
        );
    }
}

#[test]
fn a_router_whose_only_axum_import_is_test_gated_still_provides_its_shipped_route() {
    // Why `parse_imports`/`imports_axum` are NOT narrowed to non-test items, mirroring the reason
    // `adapters::http_clients` keeps its `BindingCollector` file-wide. Here the sole `use axum::...` is
    // `#[cfg(test)]`-gated while the SHIPPED router is written fully qualified — narrowing the import
    // scan would take `imports_axum` to false and delete a real route, which is worse than the fixture
    // this gate suppresses.
    let src = concat!(
        "#[cfg(test)]\n",
        "use axum::Router;\n",
        "fn ship() -> axum::Router {\n",
        "    axum::Router::new().route(\"/ship\", axum::routing::get(h))\n",
        "}\n",
    );
    let out = extract_axum_router_fragments(SHIPPED_REL, src);
    assert_eq!(frag(&out, "ship").entries.len(), 1, "{out:?}");
}

#[test]
fn a_test_fragment_sharing_a_shipped_fragments_name_drops_only_its_own_entries() {
    // Fragment names are tracked FILE-GLOBALLY, so a test item can append to the very fragment a
    // shipped one opened. The gate must subtract the test entries and nothing else.
    let src = concat!(
        "use axum::Router;\n",
        "use axum::routing::get;\n",
        "fn app() -> Router {\n",
        "    Router::new().route(\"/ship\", get(h))\n",
        "}\n",
        "#[cfg(test)]\n",
        "fn app_test() -> Router {\n",
        "    let app = Router::new().route(\"/t\", get(t));\n",
        "    app\n",
        "}\n",
    );
    let paths: Vec<String> = frag(&extract_axum_router_fragments(SHIPPED_REL, src), "app")
        .entries
        .iter()
        .map(|e| match e {
            RouterMountEntry::Verb { path, .. } => path.clone(),
            other => panic!("expected verb, got {other:?}"),
        })
        .collect();
    assert_eq!(paths, vec!["/ship"]);
}

#[test]
fn the_gate_agrees_with_the_test_span_axis_line_for_line() {
    // The seam pin both sibling adapters carry: this adapter SKIPS where `lang::test_spans` RECORDS —
    // deliberately different uses of one predicate — and the two must call the same regions test-only.
    // The out-of-reach shapes are held to the same bar: they are silent for a scope reason, but a rule
    // pack still has to be able to subtract those lines, so a span must exist for them too.
    for (name, src) in TEST_GATED_ROUTERS.iter().chain(OUT_OF_REACH_ROUTERS) {
        assert!(
            !crate::extract_test_spans(SHIPPED_REL, src).is_empty(),
            "{name}: no test span, so a rule pack could not subtract this region either"
        );
        assert!(
            extract_axum_router_fragments(SHIPPED_REL, src).is_empty(),
            "{name}: test_spans calls these lines test-only but this adapter minted a provide from them"
        );
    }
}

#[test]
fn empty_file_yields_empty_vec() {
    assert!(extract_axum_router_fragments("e.rs", "").is_empty());
}
