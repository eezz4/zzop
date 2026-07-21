use super::*;

const IMPORT: &str = "using System.Net.Http;\n";

#[test]
fn literal_get_async_is_resolved() {
    let src = format!(
        "{IMPORT}class C {{ async void M() {{ var client = new HttpClient(); var r = client.GetAsync(\"/api/users\"); }} }}"
    );
    let out = extract_csharp_http_consumes("f.cs", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/users"));
    assert_eq!(out[0].client.as_deref(), Some("httpclient"));
}

#[test]
fn interpolated_url_is_normalized() {
    let src = format!(
        "{IMPORT}class C {{ async void M() {{ var r = client.GetAsync($\"/api/users/{{id}}\"); }} }}"
    );
    let out = extract_csharp_http_consumes("f.cs", &src);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/users/{}"));
}

#[test]
fn non_literal_url_is_unresolved() {
    let src = format!("{IMPORT}class C {{ async void M() {{ var r = client.GetAsync(url); }} }}");
    let out = extract_csharp_http_consumes("f.cs", &src);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("url"));
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

#[test]
fn absolute_url_is_keyed_verbatim() {
    let src = format!(
        "{IMPORT}class C {{ async void M() {{ var r = client.PostAsync(\"https://vendor.example/x\", body); }} }}"
    );
    let out = extract_csharp_http_consumes("f.cs", &src);
    assert_eq!(out[0].key.as_deref(), Some("POST https://vendor.example/x"));
}

#[test]
fn import_gate_blocks_extraction_without_using() {
    let src = "class C { async void M() { var r = client.GetAsync(\"/api/users\"); } }";
    assert!(extract_csharp_http_consumes("f.cs", src).is_empty());
}

#[test]
fn system_net_http_json_specifier_also_gates() {
    let src = "using System.Net.Http.Json;\nclass C { async void M() { var r = client.GetFromJsonAsync(\"/api/users\"); } }";
    let out = extract_csharp_http_consumes("f.cs", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/users"));
}

#[test]
fn empty_on_parse_failure() {
    assert!(extract_csharp_http_consumes("f.cs", "\u{0}\u{1}not csharp{{{{").is_empty());
}

#[test]
fn verb_methods_emit_only_core_key_verbs() {
    // Parity pin with every other parser's verb vocabulary (go/java/ts/rust): every emitted key verb
    // must be a `zzop_core::HTTP_KEY_VERBS` join member. `HttpClient`'s helper names all map to the five
    // core verbs (no HEAD helper exists on `HttpClient`), so — unlike the attribute-provides side's
    // `METHOD_ATTRIBUTES` HEAD carve-out — there is no divergence to allow here.
    for (_, verb) in super::VERB_METHODS {
        assert!(
            zzop_core::HTTP_KEY_VERBS.contains(verb),
            "VERB_METHODS emits {verb}, which is not a core HTTP_KEY_VERBS member"
        );
    }
}
