//! `mixed-content-egress` + `get-with-body` + comment-skip/test-path exclusion tests (split from `fullstack.rs`; shared fixtures live in the crate root).

use super::*;

// --- mixed-content-egress ---

#[test]
fn plain_http_url_literal_is_flagged() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"http://example.com/api\"); }\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "mixed-content-egress");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 1);
}

#[test]
fn https_url_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"https://example.com/api\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mixed-content-egress").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn xml_namespace_uri_is_excluded() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/xmlns.ts",
        "export const ns = \"http://www.w3.org/2000/svg\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mixed-content-egress").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mixed_content_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() { return fetch(\"http://example.com/api\"); } // mixed-content-ok\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mixed-content-egress").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- get-with-body ---

#[test]
fn get_request_with_body_in_the_same_function_is_flagged() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, {\n    method: 'GET',\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "get-with-body");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 4);
}

#[test]
fn generic_request_wrapper_with_type_union_method_is_not_flagged() {
    // A generic wrapper's signature
    // `method: "GET" | "POST"` is a TYPE-position union — the method is a parameter, not a
    // committed GET — and its `body:` is the conditional passthrough. The value-position anchor
    // (`[,})]` or end-of-line after the literal, never a union `|`) is what keeps this silent.
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/lib/api.ts",
        "async function request<T>(method: \"GET\" | \"POST\", path: string, opts: any = {}): Promise<T> {\n  const res = await fetch(base + path, {\n    method,\n    body: opts.body ? JSON.stringify(opts.body) : undefined,\n  });\n  return res.json();\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-with-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn value_position_get_at_end_of_line_still_fires() {
    // The value-position anchor accepts end-of-line too, not just a trailing comma/brace — a
    // `method: 'GET'` line with the comma on the next line (unusual but valid) must still count.
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, {\n    method: 'GET'\n    ,\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "get-with-body").len(), 1, "{:?}", out.findings);
}

#[test]
fn get_request_without_body_is_not_flagged() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, { method: 'GET' });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-with-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn post_request_with_body_is_not_flagged() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/client.ts",
        "export function save() {\n  return fetch(url, {\n    method: 'POST',\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-with-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn get_body_ok_marker_above_the_body_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() {\n  return fetch(url, {\n    method: 'GET',\n    // get-body-ok: legacy proxy requires it, verified server-side\n    body: JSON.stringify(data),\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-with-body").is_empty(), "{:?}", out.findings);
}

// --- skip_comment_lines + test-path file_exclude_pattern ---
// A commented-out GET-with-body shape must not fire `get-with-body`; `mixed-content-egress` shares the same test-path `file_exclude_pattern` as `localhost-egress-committed`.

#[test]
fn get_with_body_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/client.ts",
        "export function load() {\n  // fetch(url, { method: 'GET', body: JSON.stringify(data) }) -- old, fixed below\n  return fetch(url, { method: 'GET' });\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "get-with-body").is_empty(), "{:?}", out.findings);
}

#[test]
fn plain_http_url_literal_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/__tests__/client.test.ts",
        "export function load() { return fetch(\"http://example.com/api\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mixed-content-egress").is_empty(),
        "{:?}",
        out.findings
    );
}
