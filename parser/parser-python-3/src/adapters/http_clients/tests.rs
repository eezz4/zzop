use super::*;

/// Policy-value pin (same-crate substitute for the cross-crate T2 pin pattern
/// `crates/engine/tests/policy_value_pins.rs` uses): `VERB_METHODS` (this module) and
/// `adapters::fastapi::VERB_DECORATORS` encode the identical five-verb vocabulary for two
/// independent reasons (called HTTP methods here, decorator names there) that happen to agree
/// one-for-one today — if one changes, this pin forces the other to be re-justified rather than
/// silently drifting apart.
#[test]
fn verb_methods_matches_fastapi_verb_decorators() {
    assert_eq!(
            VERB_METHODS,
            crate::adapters::fastapi::VERB_DECORATORS,
            "VERB_METHODS (adapters::http_clients) and VERB_DECORATORS (adapters::fastapi) both name \
             the same five-verb HTTP vocabulary for independent reasons; if one changes, re-justify the \
             other and update this pin."
        );
}

fn consumes(text: &str) -> Vec<IoConsume> {
    extract_python_http_consumes("a.py", text)
}

#[test]
fn no_requests_or_httpx_import_yields_nothing() {
    assert!(consumes("requests.get(\"/x\")\n").is_empty());
}

#[test]
fn literal_get_call_resolves_to_a_keyed_consume() {
    let out = consumes("import requests\nrequests.get(\"/users\")\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
    assert_eq!(out[0].raw, None);
    assert_eq!(out[0].method, None);
}

#[test]
fn httpx_literal_post_call_resolves() {
    let out = consumes("import httpx\nhttpx.post(\"/items\")\n");
    assert_eq!(out[0].key.as_deref(), Some("POST /items"));
}

#[test]
fn aliased_import_still_matches_via_its_local_binding() {
    let out = consumes("import requests as r\nr.get(\"/x\")\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /x"));
}

#[test]
fn non_literal_argument_still_emits_an_unresolved_consume() {
    let out = consumes("import requests\nurl = compute_url()\nrequests.get(url)\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("url"));
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

// --- F2: query-drop, absolute-URL, f-string reassembly ---

#[test]
fn query_suffix_is_dropped_from_the_consume_key() {
    // `http_consume_interface_key` drops `?...`/`#...` before normalizing — a call-site `?` is
    // always a query separator, and a route provide's key never carries one.
    let out = consumes("import requests\nrequests.get(\"/articles?limit=10\")\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /articles"));
}

#[test]
fn absolute_url_becomes_a_host_carrying_key_for_the_external_bucket() {
    let out = consumes("import requests\nrequests.get(\"https://api.stripe.com/v1/charges\")\n");
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].key.as_deref(),
        Some("GET https://api.stripe.com/v1/charges")
    );
    assert!(out[0].raw.is_none());
}

#[test]
fn base_relative_literal_with_no_leading_slash_stays_unresolved() {
    // Unlike TS egress, no base-relative-path bucket is ported here — never invent a `requests`/
    // `httpx` `baseURL` idiom this adapter's own call sites don't evidence.
    let out = consumes("import requests\nrequests.get(\"users/login\")\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("\"users/login\""));
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

#[test]
fn fstring_mid_interpolation_reassembles_and_keys() {
    let out = consumes("import requests\nrequests.get(f\"/users/{uid}\")\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users/{}"));
    assert!(out[0].raw.is_none());
}

#[test]
fn fstring_headed_by_an_interpolation_stays_unresolved() {
    let out = consumes("import requests\nrequests.get(f\"{base}/users\")\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].method.as_deref(), Some("GET"));
    assert!(out[0].raw.as_deref().unwrap().contains("base"));
}

#[test]
fn unrelated_receiver_is_not_a_client_call() {
    let out = consumes("import requests\ncache.get(\"/x\")\n");
    assert!(out.is_empty());
}

#[test]
fn non_verb_method_is_ignored() {
    let out = consumes("import requests\nrequests.Session()\n");
    assert!(out.is_empty());
}

#[test]
fn call_with_no_positional_argument_is_skipped() {
    let out = consumes("import requests\nrequests.get()\n");
    assert!(out.is_empty());
}

#[test]
fn nested_call_inside_a_function_body_is_still_found() {
    let src = concat!(
        "import requests\n",
        "def fetch_users():\n",
        "    return requests.get(\"/users\")\n",
    );
    let out = consumes(src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

// --- F3: nested call-site discovery via the generic visitor ---

#[test]
fn chained_call_on_the_response_is_still_found() {
    let out = consumes("import requests\nrequests.get(\"/users\").json()\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn call_inside_a_dict_literal_is_found() {
    let out = consumes("import requests\nd = {\"r\": requests.get(\"/users\")}\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn call_inside_a_list_literal_is_found() {
    let out = consumes("import requests\nxs = [requests.get(\"/users\")]\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn call_as_a_keyword_argument_value_is_found() {
    let out = consumes("import requests\nprint(resp=requests.get(\"/users\"))\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn with_statement_context_expr_call_is_found() {
    let out = consumes("import requests\nwith requests.get(\"/users\") as r:\n    pass\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn parse_failure_yields_empty_vec() {
    assert!(consumes("def f(:\n").is_empty());
}

#[test]
fn empty_file_yields_empty_vec() {
    assert!(consumes("").is_empty());
}
