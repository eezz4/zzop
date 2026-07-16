use super::*;

/// Policy-value pin (same-crate substitute for the cross-crate T2 pin pattern
/// `crates/engine/tests/policy_value_pins.rs` uses): `VERB_METHODS` (this module) and
/// `adapters::axum::VERB_METHODS` encode the identical five-verb HTTP vocabulary for two independent
/// reasons (called methods here, axum routing-fn names there) that happen to agree one-for-one today —
/// if one changes, this pin forces the other to be re-justified rather than silently drifting apart.
#[test]
fn verb_methods_matches_axum_verb_methods() {
    assert_eq!(
        VERB_METHODS,
        crate::adapters::axum::VERB_METHODS,
        "VERB_METHODS (adapters::http_clients) and VERB_METHODS (adapters::axum) both name the same \
         five-verb HTTP vocabulary for independent reasons; if one changes, re-justify the other and \
         update this pin."
    );
}

fn consumes(text: &str) -> Vec<IoConsume> {
    extract_rust_http_consumes("a.rs", text)
}

#[test]
fn no_reqwest_import_yields_nothing() {
    assert!(consumes("fn f() {\n    reqwest::get(\"/x\");\n}\n").is_empty());
}

#[test]
fn qualified_free_function_literal_get_resolves() {
    let out = consumes("use reqwest;\nfn f() {\n    reqwest::get(\"/users\");\n}\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
    assert_eq!(out[0].raw, None);
    assert_eq!(out[0].method, None);
}

#[test]
fn method_call_on_import_bound_param_resolves() {
    let out =
        consumes("use reqwest::Client;\nfn f(client: Client) {\n    client.get(\"/items\");\n}\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /items"));
}

#[test]
fn post_put_delete_patch_are_recognized() {
    let src = concat!(
        "use reqwest::Client;\n",
        "fn f(c: Client) {\n",
        "    c.post(\"/a\");\n",
        "    c.put(\"/b\");\n",
        "    c.delete(\"/c\");\n",
        "    c.patch(\"/d\");\n",
        "}\n",
    );
    let out = consumes(src);
    let keys: Vec<&str> = out.iter().map(|c| c.key.as_deref().unwrap()).collect();
    assert_eq!(keys, vec!["POST /a", "PUT /b", "DELETE /c", "PATCH /d"]);
}

#[test]
fn non_verb_method_is_ignored() {
    let out = consumes("use reqwest::Client;\nfn f(c: Client) {\n    c.execute_stub();\n}\n");
    assert!(out.is_empty());
}

#[test]
fn call_with_no_positional_argument_is_skipped() {
    let out = consumes("use reqwest::Client;\nfn f(c: Client) {\n    c.get();\n}\n");
    assert!(out.is_empty());
}

#[test]
fn non_literal_argument_still_emits_an_unresolved_consume() {
    let out =
        consumes("use reqwest::Client;\nfn f(c: Client, url: String) {\n    c.get(url);\n}\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("url"));
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

#[test]
fn query_suffix_is_dropped_from_the_consume_key() {
    let out = consumes(
        "use reqwest::Client;\nfn f(c: Client) {\n    c.get(\"/articles?limit=10\");\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /articles"));
}

#[test]
fn absolute_url_becomes_a_host_carrying_key() {
    let out = consumes(
        "use reqwest::Client;\nfn f(c: Client) {\n    c.get(\"https://api.stripe.com/v1/charges\");\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].key.as_deref(),
        Some("GET https://api.stripe.com/v1/charges")
    );
    assert!(out[0].raw.is_none());
}

#[test]
fn base_relative_literal_with_no_leading_slash_stays_unresolved() {
    let out = consumes("use reqwest::Client;\nfn f(c: Client) {\n    c.get(\"users/login\");\n}\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

#[test]
fn format_macro_bare_placeholder_reassembles_and_keys() {
    let out = consumes(
        "use reqwest::Client;\nfn f(c: Client, uid: i32) {\n    c.get(format!(\"/users/{}\", uid));\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users/{}"));
    assert!(out[0].raw.is_none());
}

#[test]
fn format_macro_named_placeholder_reassembles_and_keys() {
    let out = consumes(
        "use reqwest::Client;\nfn f(c: Client, uid: i32) {\n    c.get(format!(\"/users/{uid}\", uid = uid));\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users/{}"));
}

#[test]
fn reference_to_format_macro_is_also_resolved() {
    let out = consumes(
        "use reqwest::Client;\nfn f(c: Client, uid: i32) {\n    c.get(&format!(\"/users/{}\", uid));\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users/{}"));
}

#[test]
fn format_macro_headed_by_a_placeholder_still_keys_via_the_leading_slash_rule() {
    // Unlike Python's f-string handling (where a `{}`-headed reassembly is deliberately left
    // unresolved because there is no leading `/`), this string LITERALLY starts with `{` here, which
    // does not match the `/`-headed or `http(s)://`-headed keying rules, so it correctly stays
    // unresolved too — same net effect as the Python adapter's `{}`-headed case, reached by the same
    // "no leading slash, no scheme" rule rather than a special case.
    let out = consumes(
        "use reqwest::Client;\nfn f(c: Client, base: String) {\n    c.get(format!(\"{}/users\", base));\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, None);
}

#[test]
fn nested_call_site_through_send_await_json_is_found() {
    let out = consumes(
        "use reqwest::Client;\nasync fn f(c: Client) {\n    let _ = c.get(\"/users\").send().await;\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn call_inside_a_struct_literal_field_is_found() {
    let src = concat!(
        "use reqwest::Client;\n",
        "struct Wrap { v: i32 }\n",
        "fn f(c: Client) {\n",
        "    let _ = Wrap { v: { c.get(\"/users\"); 1 } };\n",
        "}\n",
    );
    let out = consumes(src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn parse_failure_yields_empty_vec() {
    assert!(consumes("fn f(:\n").is_empty());
}

#[test]
fn empty_file_yields_empty_vec() {
    assert!(consumes("").is_empty());
}

// --- Bound-receiver discipline (opus review F1 fix) ---------------------------------------------

#[test]
fn get_on_untracked_hashmap_receiver_yields_no_consume() {
    // (a) `cache.get(...)` on an unrelated `HashMap` in a file that separately imports `reqwest` — the
    // old any-receiver net misread this as a `reqwest` egress call; `cache` is never bound to `reqwest`.
    let src = concat!(
        "use reqwest::Client;\n",
        "use std::collections::HashMap;\n",
        "fn f(_c: Client, cache: HashMap<String, String>) {\n",
        "    cache.get(\"/config/db\");\n",
        "}\n",
    );
    assert!(consumes(src).is_empty());
}

#[test]
fn get_on_untracked_header_map_receiver_yields_no_consume() {
    // (b) `headers.get(...)` on an unrelated header-map-shaped type — same untracked-receiver rejection.
    let src = concat!(
        "use reqwest::Client;\n",
        "fn f(_c: Client, headers: HeaderMap) {\n",
        "    headers.get(\"content-type\");\n",
        "}\n",
    );
    assert!(consumes(src).is_empty());
}

#[test]
fn let_bound_client_constructor_receiver_resolves() {
    // (c) `let client = reqwest::Client::new(); client.get(...)`.
    let out = consumes(
        "use reqwest;\nfn f() {\n    let client = reqwest::Client::new();\n    client.get(\"/api/x\");\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/x"));
}

#[test]
fn fn_param_typed_reference_reqwest_client_resolves() {
    // (d) fn param `client: &reqwest::Client`.
    let out = consumes(
        "use reqwest;\nfn f(client: &reqwest::Client) {\n    client.get(\"/api/y\");\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/y"));
}

#[test]
fn use_reqwest_client_alias_constructor_let_binding_resolves() {
    // (e) `use reqwest::Client; let c = Client::new(); c.post(...)`.
    let out = consumes(
        "use reqwest::Client;\nfn f() {\n    let c = Client::new();\n    c.post(\"/api/y\");\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("POST /api/y"));
}

#[test]
fn direct_chain_on_constructor_resolves() {
    // (f) `reqwest::Client::new().get(...)` with no intermediate binding at all.
    let out = consumes("use reqwest;\nfn f() {\n    reqwest::Client::new().get(\"/z\");\n}\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /z"));
}

#[test]
fn builder_chain_direct_call_resolves() {
    let out = consumes(
        "use reqwest;\nfn f() {\n    reqwest::Client::builder().build().unwrap().get(\"/b\");\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /b"));
}

#[test]
fn blocking_free_function_literal_get_resolves() {
    // (g) blocking variant of the free-function call shape.
    let out = consumes("use reqwest;\nfn f() {\n    reqwest::blocking::get(\"/e\");\n}\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /e"));
}

#[test]
fn blocking_client_let_binding_resolves() {
    // (g) blocking variant of the bound-constructor call shape.
    let out = consumes(
        "use reqwest;\nfn f() {\n    let client = reqwest::blocking::Client::new();\n    client.get(\"/d\");\n}\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /d"));
}

#[test]
fn nested_call_site_with_format_url_on_bound_client_is_found() {
    // (h) a bound receiver's nested call site (`.get(url).send().await?.json()`) with a `format!` url.
    let src = concat!(
        "use reqwest::Client;\n",
        "async fn f(c: Client, id: i32) {\n",
        "    let _ = c.get(format!(\"/users/{}\", id)).send().await?.json();\n",
        "}\n",
    );
    let out = consumes(src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users/{}"));
}

#[test]
fn struct_field_typed_reqwest_client_receiver_resolves() {
    let src = concat!(
        "use reqwest::Client;\n",
        "struct Svc { http: Client }\n",
        "impl Svc {\n",
        "    fn call(&self) {\n",
        "        self.http.get(\"/x\");\n",
        "    }\n",
        "}\n",
    );
    let out = consumes(src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /x"));
}
