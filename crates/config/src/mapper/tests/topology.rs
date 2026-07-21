use std::path::Path;

use serde_json::json;

use super::analyze_request;
use crate::mapper::config_to_request;

// --- mountedAt / mounts / hosts gates -------------------------------------------------------

#[test]
fn mounted_at_gate_error_texts() {
    let run = |v: serde_json::Value| {
        config_to_request(
            &json!({"trees": [{"root": ".", "mountedAt": v}]}),
            Path::new("/base"),
        )
        .unwrap_err()
        .0
    };
    assert_eq!(run(json!(5)), "trees[0].mountedAt must be a string.");
    assert_eq!(
        run(json!("///")),
        "trees[0].mountedAt must be a non-empty path after trimming slashes."
    );
    assert_eq!(
        run(json!("api")),
        "trees[0].mountedAt must start with \"/\"."
    );
    // The leading-"/" check runs BEFORE the scheme check (same order as the JS source), so a bare
    // "https://api" (which does not start with "/") trips the "/" message, not the scheme one —
    // the scheme check only fires for a value that already starts with "/".
    assert_eq!(
        run(json!("/gateway://oops")),
        "trees[0].mountedAt must not contain a scheme (\"://\") — it is a path prefix, not a full URL."
    );
    assert_eq!(
        run(json!("/api/{}")),
        "trees[0].mountedAt must not contain a path-param placeholder (\"{}\")."
    );
    assert_eq!(
        run(json!("/a b")),
        "trees[0].mountedAt must not contain whitespace."
    );
}

#[test]
fn mounts_gate_error_texts() {
    let run = |v: serde_json::Value| {
        config_to_request(
            &json!({"trees": [{"root": ".", "mounts": v}]}),
            Path::new("/base"),
        )
        .unwrap_err()
        .0
    };
    assert_eq!(
        run(json!("x")),
        "trees[0].mounts must be an array of { dir, at } objects."
    );
    assert_eq!(
        run(json!(["x"])),
        "trees[0].mounts[0] must be an object with \"dir\" and \"at\" strings."
    );
    assert_eq!(
        run(json!([{"dir": "/abs", "at": "/api"}])),
        "trees[0].mounts[0].dir must be tree-relative and must not start with \"/\"."
    );
    assert_eq!(
        run(json!([{"dir": "a\\b", "at": "/api"}])),
        "trees[0].mounts[0].dir must use forward slashes, not backslashes."
    );
}

#[test]
fn hosts_gate_error_texts() {
    let run = |v: serde_json::Value| {
        config_to_request(
            &json!({"trees": [{"root": ".", "hosts": v}]}),
            Path::new("/base"),
        )
        .unwrap_err()
        .0
    };
    assert_eq!(
        run(json!("x")),
        "trees[0].hosts must be an array of host strings."
    );
    assert_eq!(
        run(json!([""])),
        "trees[0].hosts[0] must be a non-empty string."
    );
    assert_eq!(
        run(json!(["https://x"])),
        "trees[0].hosts[0] must be a bare host, not a full URL (\"://\" is not allowed)."
    );
    assert_eq!(
        run(json!(["x/y"])),
        "trees[0].hosts[0] must be a bare host, not a path (\"/\" is not allowed)."
    );
    assert_eq!(
        run(json!(["x y"])),
        "trees[0].hosts[0] must not contain whitespace."
    );
}

#[test]
fn well_formed_mounted_at_mounts_hosts_flow_into_the_tree_request() {
    let mapped = config_to_request(
        &json!({"trees": [{
            "root": ".",
            "mountedAt": "/gateway",
            "mounts": [{"dir": "apps/api", "at": "/api"}],
            "hosts": ["internal.example.com"]
        }]}),
        Path::new("/base"),
    )
    .unwrap();
    let tree = &mapped.request["trees"][0];
    assert_eq!(tree["mountedAt"], "/gateway");
    assert_eq!(tree["mounts"][0]["dir"], "apps/api");
    assert_eq!(tree["hosts"][0], "internal.example.com");
}

#[test]
fn empty_mounts_and_hosts_arrays_are_omitted_from_the_request() {
    let mapped = config_to_request(
        &json!({"trees": [{"root": ".", "mounts": [], "hosts": []}]}),
        Path::new("/base"),
    )
    .unwrap();
    let tree = &mapped.request["trees"][0];
    assert!(tree.get("mounts").is_none());
    assert!(tree.get("hosts").is_none());
}

// --- routes injection gate + flow -----------------------------------------------------------

#[test]
fn routes_gate_error_texts() {
    let run = |v: serde_json::Value| {
        config_to_request(
            &json!({"trees": [{"root": ".", "routes": v}]}),
            Path::new("/base"),
        )
        .unwrap_err()
        .0
    };
    assert_eq!(
        run(json!("x")),
        "trees[0].routes must be an array of { key, role? } objects."
    );
    assert_eq!(
        run(json!([5])),
        "trees[0].routes[0] must be an object with a \"key\" string."
    );
    assert_eq!(
        run(json!([{"role": "provide"}])),
        "trees[0].routes[0].key must be a non-empty \"METHOD PATH\" string (e.g. \"GET /api/users\")."
    );
    assert_eq!(
        run(json!([{"key": "GET /x", "role": "middleware"}])),
        "trees[0].routes[0].role must be \"provide\" or \"consume\"."
    );
}

#[test]
fn well_formed_routes_flow_into_the_tree_request() {
    let mapped = config_to_request(
        &json!({"trees": [{
            "root": ".",
            "routes": [
                {"key": "GET /api/users"},
                {"key": "POST /api/orders", "role": "consume"}
            ]
        }]}),
        Path::new("/base"),
    )
    .unwrap();
    let tree = &mapped.request["trees"][0];
    assert_eq!(tree["routes"][0]["key"], "GET /api/users");
    assert_eq!(tree["routes"][1]["role"], "consume");
}

#[test]
fn an_unknown_key_inside_a_routes_entry_warns_but_does_not_reject() {
    let mapped = config_to_request(
        &json!({"trees": [{"root": ".", "routes": [{"key": "GET /x", "note": "typo-field"}]}]}),
        Path::new("/base"),
    )
    .unwrap();
    assert!(
        mapped
            .warnings
            .iter()
            .any(|w| w.contains("trees[0].routes[0].note")),
        "an unknown per-route key must drift-warn: {:?}",
        mapped.warnings
    );
    // ...but the valid route still flows through (drift never rejects).
    assert_eq!(mapped.request["trees"][0]["routes"][0]["key"], "GET /x");
}

#[test]
fn roots_shorthand_never_reads_mounted_at_mounts_hosts() {
    // These keys have no meaning off the `trees[]` shape; a `roots` config simply has nowhere to
    // put them, and the resulting tree request carries none of them.
    let mapped = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
    let req = analyze_request(&mapped.request);
    assert!(req.get("mountedAt").is_none());
    assert!(req.get("mounts").is_none());
    assert!(req.get("hosts").is_none());
}
