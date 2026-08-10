//! `matchers` coverage — callee recognition (bare/member/computed `fetch`·`axios`·`ky`), the
//! method fan-out, client tagging, and the three-way verb answer for a `fetch` options argument
//! (spec default / visible literal / unknowable). Split out of `matchers.rs` because the pair would
//! exceed the 300-line file budget — same reason and same shape as `url_resolve_tests.rs` beside it.

use crate::adapters::egress::{clients, extract_http_egress, files, keys};

#[test]
fn bare_fetch_with_no_options_keeps_the_spec_default_get() {
    // `fetch(url)` IS a GET by the platform spec — this is read, not guessed, and must survive
    // the tightening below.
    let out = extract_http_egress(&files(&[("a.ts", "fetch('/tags');")]));
    assert_eq!(keys(&out), vec![Some("GET /tags".to_string())]);
}

#[test]
fn an_inline_options_object_without_a_method_key_is_still_a_get() {
    // The object is transparent and states no verb, so the spec default still applies.
    let out = extract_http_egress(&files(&[("a.ts", "fetch('/tags', { headers: h });")]));
    assert_eq!(keys(&out), vec![Some("GET /tags".to_string())]);
}

#[test]
fn opaque_fetch_options_emit_nothing_rather_than_a_fabricated_get() {
    // `opts` may carry `method: 'DELETE'`. Defaulting to GET mis-keys the call BOTH ways: it
    // invents a GET consume that no route provides, and it erases the real DELETE consume that
    // the mutating-route rules read. Silence is the honest answer — the sibling `wrapper_calls`
    // channel already refuses this exact shape.
    let out = extract_http_egress(&files(&[("a.ts", "fetch('/articles', opts);")]));
    assert!(keys(&out).is_empty(), "{:?}", keys(&out));
}

#[test]
fn a_dynamic_method_value_emits_nothing_rather_than_a_fabricated_get() {
    // The code states that a method EXISTS and that its value is not visible here. Reading that
    // as "GET" contradicts the source.
    let out = extract_http_egress(&files(&[("a.ts", "fetch('/comments', { method: verb });")]));
    assert!(keys(&out).is_empty(), "{:?}", keys(&out));
}

#[test]
fn a_computed_options_key_is_unknowable_never_a_spec_default_get() {
    // `{ ["method"]: 'POST' }` names the key through a computed property. Whether a computed key
    // spells `method` is not decidable in general (`{ [k]: v }`), so the site is unknowable and
    // must be DROPPED — falling through to the spec default fabricated a GET here while the source
    // visibly says POST.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "fetch('/x', { [\"method\"]: 'POST' });",
    )]));
    assert!(keys(&out).is_empty(), "{:?}", keys(&out));
}

#[test]
fn bare_axios_with_no_config_keeps_the_axios_default_get() {
    // `axios(url)` IS a GET by axios's own default — read, not guessed, same as bare `fetch(url)`.
    let out = extract_http_egress(&files(&[("a.ts", "axios('/w');")]));
    assert_eq!(keys(&out), vec![Some("GET /w".to_string())]);
}

#[test]
fn bare_axios_with_a_method_in_its_config_keys_that_verb_not_get() {
    // The reproduced defect: `axios('/w', { method: 'POST' })` was keyed `GET /w` — the bare-axios
    // arm hardcoded GET and never read `args[1]`, though the verb sits in the same options-object
    // position the `fetch` arm already reads.
    let out = extract_http_egress(&files(&[("a.ts", "axios('/w', { method: 'POST' });")]));
    assert_eq!(keys(&out), vec![Some("POST /w".to_string())]);
}

#[test]
fn bare_axios_with_an_opaque_config_emits_nothing_rather_than_a_fabricated_get() {
    // `cfg` may carry `method: 'DELETE'` — same three-way judgment as the fetch arm: unknowable
    // drops the site (under-report, never mis-key).
    let out = extract_http_egress(&files(&[("a.ts", "axios('/w', cfg);")]));
    assert!(keys(&out).is_empty(), "{:?}", keys(&out));
}

#[test]
fn computed_member_ternary_callee_fans_out_the_method() {
    let out = extract_http_egress(&files(&[(
        "conduit.ts",
        "axios[favorited ? 'delete' : 'post'](`/articles/${slug}/favorite`);",
    )]));
    assert_eq!(
        keys(&out),
        vec![
            Some("DELETE /articles/{}/favorite".to_string()),
            Some("POST /articles/{}/favorite".to_string()),
        ]
    );
}

#[test]
fn computed_member_string_literal_callee_is_a_single_method() {
    let out = extract_http_egress(&files(&[("a.ts", "axios['post']('/a');")]));
    assert_eq!(keys(&out), vec![Some("POST /a".to_string())]);
}

#[test]
fn computed_member_identifier_callee_is_not_recognized() {
    let out = extract_http_egress(&files(&[("a.ts", "axios[verb]('/a');")]));
    assert!(out.is_empty());
}

#[test]
fn computed_member_ternary_with_an_arm_outside_the_verb_set_rejects_the_whole_site() {
    // `head` is not a recognized verb — one bad arm rejects the site entirely rather than silently
    // narrowing to just the `get` arm (never guess).
    let out = extract_http_egress(&files(&[("a.ts", "axios[cond ? 'get' : 'head']('/a');")]));
    assert!(out.is_empty());
}

#[test]
fn computed_member_ternary_callee_with_unresolved_url_carries_both_methods() {
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "axios[cond ? 'delete' : 'post'](buildUrl(x));",
    )]));
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|c| c.key.is_none()));
    assert!(out.iter().all(|c| c.raw.as_deref() == Some("buildUrl(x)")));
    assert_eq!(
        out.iter().map(|c| c.method.clone()).collect::<Vec<_>>(),
        vec![Some("DELETE".to_string()), Some("POST".to_string())]
    );
}

// --- client provenance tag (`axios-defaults-base-v1`) ---

#[test]
fn axios_member_call_is_tagged_axios() {
    let out = extract_http_egress(&files(&[("a.ts", r#"axios.get("/a");"#)]));
    assert_eq!(clients(&out), vec![Some("axios".to_string())]);
}

#[test]
fn bare_axios_call_is_tagged_axios() {
    let out = extract_http_egress(&files(&[("a.ts", r#"axios("/a");"#)]));
    assert_eq!(clients(&out), vec![Some("axios".to_string())]);
}

#[test]
fn axios_computed_member_call_is_tagged_axios() {
    let out = extract_http_egress(&files(&[("a.ts", "axios['post']('/a');")]));
    assert_eq!(clients(&out), vec![Some("axios".to_string())]);
}

#[test]
fn axios_computed_member_fanout_is_tagged_axios_for_every_variant() {
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "axios[favorited ? 'delete' : 'post'](`/articles/${slug}/favorite`);",
    )]));
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|c| c.client.as_deref() == Some("axios")));
}

#[test]
fn ky_member_call_is_tagged_ky() {
    let out = extract_http_egress(&files(&[("a.ts", r#"ky.get("/a");"#)]));
    assert_eq!(clients(&out), vec![Some("ky".to_string())]);
}

#[test]
fn bare_fetch_call_is_tagged_fetch() {
    let out = extract_http_egress(&files(&[("a.ts", r#"fetch("/a");"#)]));
    assert_eq!(clients(&out), vec![Some("fetch".to_string())]);
}

#[test]
fn dollar_fetch_call_is_tagged_dollar_fetch() {
    let out = extract_http_egress(&files(&[("a.ts", r#"$fetch("/a");"#)]));
    assert_eq!(clients(&out), vec![Some("$fetch".to_string())]);
}

#[test]
fn unresolved_consume_still_carries_its_client_tag() {
    // Dynamic URL (unresolved) — `client` is still set from the matcher, independent of whether the
    // URL itself resolved to a key.
    let out = extract_http_egress(&files(&[("a.ts", "axios.get(buildUrl(x));")]));
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none());
    assert_eq!(out[0].client.as_deref(), Some("axios"));
}
