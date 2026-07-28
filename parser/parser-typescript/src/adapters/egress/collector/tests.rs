//! Unit tests for the egress call-site collector (`super`) — split out of it for the file-size cap.
//! Coverage for `extract_http_egress`: HTTP call-site detection, URL resolution
//! (literal/template/const-indirection), and internal-vs-external classification.
use super::extract_http_egress;
use crate::adapters::egress::{files, keys};

#[test]
fn captures_internal_axios_string_literal() {
    let out = extract_http_egress(&files(&[("a.tsx", r#"axios.get("/authen/getUserInfo")"#)]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, "http");
    assert_eq!(out[0].key.as_deref(), Some("GET /authen/getUserInfo"));
    assert_eq!(out[0].file, "a.tsx");
    assert_eq!(out[0].line, 1);
    assert!(out[0].raw.is_none());
    // No retry context and a GET — never tagged.
    assert_eq!(out[0].retry_configured, None);
}

// retry_configured (`egress-retry-v1`) is covered end-to-end — parser tag through cross-layer join —
// in `crates/engine/tests/analyze_cross_layer_retry_write.rs` (axios-retry file gate, `pRetry(...)`
// wrapper, read-verb and non-retry negatives). The inline assertion above pins the common untagged
// case (a plain GET with no retry context).

#[test]
fn resolves_cross_file_controlkey_indirection() {
    let out = extract_http_egress(&files(&[
        (
            "protocol/ControlKey.ts",
            r#"export const ControlKey = { AUTHEN: { getUserInfo: "/authen/getUserInfo", getSignout: "/authen/getSignout" } };"#,
        ),
        (
            "Ctx.tsx",
            "axios.get(ControlKey.AUTHEN.getUserInfo); axios.get(ControlKey.AUTHEN.getSignout);",
        ),
    ]));
    assert_eq!(
        keys(&out),
        vec![
            Some("GET /authen/getUserInfo".to_string()),
            Some("GET /authen/getSignout".to_string())
        ]
    );
}

#[test]
fn resolves_as_const() {
    let out = extract_http_egress(&files(&[
        (
            "protocol/ControlKey.ts",
            r#"export const ControlKey = { AUTHEN: { getUserInfo: "/authen/getUserInfo" } } as const;"#,
        ),
        ("Ctx.tsx", "axios.get(ControlKey.AUTHEN.getUserInfo)"),
    ]));
    assert_eq!(out[0].key.as_deref(), Some("GET /authen/getUserInfo"));
}

#[test]
fn derives_method_from_post_and_fetch_options() {
    let out = extract_http_egress(&files(&[
        ("k.ts", r#"const K = { create: "/items/create" };"#),
        (
            "p.tsx",
            r#"axios.post(K.create); fetch("/items/create", { method: "delete" });"#,
        ),
    ]));
    assert_eq!(
        keys(&out),
        vec![
            Some("POST /items/create".to_string()),
            Some("DELETE /items/create".to_string())
        ]
    );
}

#[test]
fn normalizes_template_literal_params() {
    let out = extract_http_egress(&files(&[("t.tsx", "axios.get(`/api/users/${id}/posts`)")]));
    assert_eq!(out[0].key.as_deref(), Some("GET /api/users/{}/posts"));
}

#[test]
fn absolute_url_becomes_a_host_carrying_key_for_the_external_bucket() {
    let out = extract_http_egress(&files(&[(
        "e.tsx",
        r#"axios.get("https://api.stripe.com/v1/charges")"#,
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].key.as_deref(),
        Some("GET https://api.stripe.com/v1/charges")
    );
    assert!(out[0].raw.is_none());
}

#[test]
fn marks_dynamic_url_as_null_with_raw() {
    let out = extract_http_egress(&files(&[("d.tsx", "axios.get(buildUrl(x))")]));
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none());
    assert_eq!(out[0].raw.as_deref(), Some("buildUrl(x)"));
    // Carried for late re-resolution even though `buildUrl(x)` is not itself a dotted chain.
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

#[test]
fn cross_file_constant_indirection_unresolved_consume_carries_its_method() {
    // Only THIS file is visible, so `ControlKey` never resolves here — but `method` must still be set
    // so a caller with a wider constant map can key the consume once it does resolve.
    let out = extract_http_egress(&files(&[(
        "Ctx.tsx",
        "axios.post(ControlKey.AUTHEN.getUserInfo);",
    )]));
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none());
    assert_eq!(out[0].raw.as_deref(), Some("ControlKey.AUTHEN.getUserInfo"));
    assert_eq!(out[0].method.as_deref(), Some("POST"));
}

#[test]
fn ignores_non_http_calls() {
    let out = extract_http_egress(&files(&[("x.ts", r#"foo.get("/a"); console.log("/b");"#)]));
    assert!(out.is_empty());
}
