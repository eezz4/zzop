//! `http-url-literal` + `get-and-body` + comment-skip/test-path exclusion tests (split from `egress.rs`; shared fixtures live in the crate root).
//!
//! `http-url-literal`'s `exclude_pattern` names localhost and every private range, and the reason is this
//! rule's own subject rather than anyone else's coverage: what it measures is a plain-`http://` literal that
//! could be a request crossing a public network — the wire a browser refuses as mixed content, and the wire
//! an eavesdropper sits on. A loopback or private-range address is not that wire. Plain http to `127.0.0.1`
//! or `10.0.4.12` is the normal shape of talking to your own machine or your own network, so flagging it
//! would be naming a downgrade where no downgrade exists. That holds whatever else is or is not loaded.
//!
//! `localhost-url-literal-committed` — a DIFFERENT question about the same addresses, namely whether the
//! literal should have been config — lives in `examples/packs/code-hygiene.json` (`axis: opinion`), with its
//! tests (`examples/packs/tests/localhost_egress.rs`). `examples/packs/tests/egress_handoff.rs` loads both
//! packs so that neither rule's silence is vacuous; the test below is the bundled-side mirror.

use super::*;

// --- http-url-literal ---

#[test]
fn plain_http_url_literal_is_flagged() {
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"http://example.com/api\"); }\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "http-url-literal");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 1);
}

#[test]
fn https_url_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"https://example.com/api\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "http-url-literal").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn xml_namespace_uri_is_excluded() {
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/xmlns.ts",
        "export const ns = \"http://www.w3.org/2000/svg\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "http-url-literal").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mixed_content_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"http://example.com/api\"); } // zzop-http-url-literal-ok\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "http-url-literal").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- get-and-body ---

#[test]
fn get_request_with_body_in_the_same_function_is_flagged() {
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, {\n    method: 'GET',\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "get-and-body");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 4);
}

#[test]
fn generic_request_wrapper_with_type_union_method_is_not_flagged() {
    // A generic wrapper's signature
    // `method: "GET" | "POST"` is a TYPE-position union — the method is a parameter, not a
    // committed GET — and its `body:` is the conditional passthrough. The value-position anchor
    // (`[,})]` or end-of-line after the literal, never a union `|`) is what keeps this silent.
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/lib/api.ts",
        "async function request<T>(method: \"GET\" | \"POST\", path: string, opts: any = {}): Promise<T> {\n  const res = await fetch(base + path, {\n    method,\n    body: opts.body ? JSON.stringify(opts.body) : undefined,\n  });\n  return res.json();\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-and-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn value_position_get_at_end_of_line_still_fires() {
    // The value-position anchor accepts end-of-line too, not just a trailing comma/brace — a
    // `method: 'GET'` line with the comma on the next line (unusual but valid) must still count.
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, {\n    method: 'GET'\n    ,\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "get-and-body").len(), 1, "{:?}", out.findings);
}

#[test]
fn get_request_without_body_is_not_flagged() {
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, { method: 'GET' });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-and-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn post_request_with_body_is_not_flagged() {
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/client.ts",
        "export function save() {\n  return fetch(url, {\n    method: 'POST',\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-and-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn get_body_ok_marker_above_the_body_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, {\n    method: 'GET',\n    // zzop-get-and-body-ok: legacy proxy requires it, verified server-side\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-and-body").is_empty(), "{:?}", out.findings);
}

// --- skip_comment_lines + test-path file_exclude_pattern ---
// A commented-out GET-with-body shape must not fire `get-and-body`; `http-url-literal` shares the same test-path `file_exclude_pattern` as the exported `code-hygiene/localhost-url-literal-committed`.

#[test]
fn get_with_body_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/client.ts",
        "export function load() {\n  // fetch(url, { method: 'GET', body: JSON.stringify(data) }) -- old, fixed below\n  return fetch(url, { method: 'GET' });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-and-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn plain_http_url_literal_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-egress");
    dir.write(
        "src/__tests__/client.test.ts",
        "export function load() { return fetch(\"http://example.com/api\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "http-url-literal").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- the localhost/private-range exclusion, from the bundled side ---

/// `http-url-literal` declines every localhost/private-range literal because none of them is the thing it
/// measures: a plain-`http://` literal that could be a request over a public wire. Loopback never leaves the
/// machine and a private range never leaves the network, so there is no TLS to have been downgraded and no
/// mixed-content block to warn about — the exclusion is a scope statement, not a deferral, and it would read
/// the same if no other pack existed. Pinned so a future widening of the `exclude_pattern` cannot land
/// silently. The cross-pack half of the same picture lives in `examples/packs/tests/egress_handoff.rs`,
/// which loads both packs and shows the OTHER question about these same addresses — should the literal have
/// been config — being answered by the pack that owns it.
#[test]
fn localhost_shapes_are_outside_this_rules_public_wire_scope() {
    for url in [
        "http://localhost:3000/api",
        "http://127.0.0.1:5432/orders",
        "http://0.0.0.0:8080/health",
        "http://192.168.1.10/api",
        "http://10.0.4.12/v1/users",
        "http://172.16.0.5/internal",
    ] {
        let dir = TempDir::new("zzop-egress");
        dir.write(
            "src/client.ts",
            &format!("export const base = \"{url}\";\n"),
        );
        let out = scan(&dir);
        assert!(
            hits(&out, "http-url-literal").is_empty(),
            "{url} is inside this rule's exclude_pattern: {:?}",
            out.findings
        );
    }
}
