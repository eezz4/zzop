//! `client_base` coverage — the literal happy paths, the same-file binding hop and its refusals,
//! and every never-guess veto (non-literal value, concat, interpolated template, wrong receiver,
//! wrong property, query/fragment in base, host-only). Split out of `client_base.rs` because the
//! pair would exceed the 300-line file budget — same shape as `egress/url_resolve_tests.rs`.

use super::client_base::*;

#[test]
fn literal_with_host_and_path_yields_the_path_part() {
    let src = r#"axios.defaults.baseURL = "https://api.example.io/api/";"#;
    let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
    assert_eq!(m.kind, CLIENT_BASE_PREFIX_KIND);
    assert_eq!(m.key.as_deref(), Some("/api"));
    assert_eq!(m.client.as_deref(), Some("axios"));
    assert_eq!(m.file, "main.ts");
    assert_eq!(m.line, 1);
    assert!(m.raw.is_none() && m.method.is_none() && m.body.is_none());
}

#[test]
fn a_same_file_const_binding_resolves_one_hop() {
    // The measured blocker on the base-prefix axis: almost nobody writes the base inline. They
    // bind it once and assign the binding, and the value is right there in the same file — read,
    // not inferred. Without this hop the whole client-base channel stayed dark on the common shape.
    let src = concat!(
        "const API_BASE = 'https://api.example.io/api';\n",
        "axios.defaults.baseURL = API_BASE;\n"
    );
    let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
    assert_eq!(m.key.as_deref(), Some("/api"));
}

#[test]
fn a_binding_declared_twice_in_one_file_is_refused() {
    // The same-file map only admits names bound EXACTLY once; two declarations mean the value at
    // the assignment is not readable, and picking either would be a guess.
    let src = concat!(
        "const API_BASE = '/api';\n",
        "const API_BASE = '/v2';\n",
        "axios.defaults.baseURL = API_BASE;\n"
    );
    assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
}

#[test]
fn a_binding_whose_value_is_not_a_literal_stays_unresolved() {
    // `process.env.X` is a deployment fact that enters by injection, never by inference.
    let src = concat!(
        "const API_BASE = process.env.API_BASE;\n",
        "axios.defaults.baseURL = API_BASE;\n"
    );
    assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
}

#[test]
fn bare_path_literal_is_kept_as_is() {
    let src = r#"axios.defaults.baseURL = "/api";"#;
    let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
    assert_eq!(m.key.as_deref(), Some("/api"));
}

#[test]
fn trailing_slash_is_trimmed() {
    let src = r#"axios.defaults.baseURL = "/api/";"#;
    let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
    assert_eq!(m.key.as_deref(), Some("/api"));
}

#[test]
fn host_only_base_yields_none() {
    let src = r#"axios.defaults.baseURL = "https://api.example.io";"#;
    assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
    let src2 = r#"axios.defaults.baseURL = "https://api.example.io/";"#;
    assert!(extract_client_base_prefix_marker("main.ts", src2).is_none());
}

#[test]
fn zero_interpolation_template_literal_works() {
    let src = "axios.defaults.baseURL = `https://api.example.io/api`;";
    let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
    assert_eq!(m.key.as_deref(), Some("/api"));
}

#[test]
fn member_expression_value_is_never_guessed() {
    let src = "axios.defaults.baseURL = settings.baseApiUrl;";
    assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
}

#[test]
fn string_concatenation_value_is_never_guessed() {
    let src = r#"axios.defaults.baseURL = HOST + "/api";"#;
    assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
}

#[test]
fn interpolated_template_value_is_never_guessed() {
    let src = "axios.defaults.baseURL = `${HOST}/api`;";
    assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
}

#[test]
fn assignment_inside_a_function_body_is_found() {
    let src = r#"
        function setup() {
            axios.defaults.baseURL = "https://api.example.io/api";
        }
    "#;
    let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
    assert_eq!(m.key.as_deref(), Some("/api"));
}

#[test]
fn unrelated_receiver_is_never_matched() {
    let src = r#"x.defaults.baseURL = "/api";"#;
    assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
}

#[test]
fn unrelated_property_is_never_matched() {
    let src = r#"axios.defaults.headers = { "X-Foo": "1" };"#;
    assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
}

#[test]
fn query_or_fragment_in_base_is_never_guessed() {
    let src = r#"axios.defaults.baseURL = "https://api.example.io/api?v=1";"#;
    assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
    let src2 = r#"axios.defaults.baseURL = "/api#frag";"#;
    assert!(extract_client_base_prefix_marker("main.ts", src2).is_none());
}

#[test]
fn protocol_relative_base_strips_the_host_like_an_absolute_url() {
    // `//host/api` is a HOST carrier, not a path — taking it verbatim would bake the host into
    // every prefixed key (the exact "host is deploy config, not contract" breach).
    let src = r#"axios.defaults.baseURL = "//cdn.acme.com/api";"#;
    let m = extract_client_base_prefix_marker("main.ts", src).expect("marker");
    assert_eq!(m.key.as_deref(), Some("/api"));
}

#[test]
fn protocol_relative_host_only_base_is_a_no_op() {
    let src = r#"axios.defaults.baseURL = "//cdn.acme.com";"#;
    assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
    let src2 = r#"axios.defaults.baseURL = "//cdn.acme.com/";"#;
    assert!(extract_client_base_prefix_marker("main.ts", src2).is_none());
}
