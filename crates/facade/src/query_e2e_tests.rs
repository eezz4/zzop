//! End-to-end tests for `query.rs` through the REAL serializer: `analyze_trees_json` over real
//! temp trees -> `query_io_json` over its byte-real output. `query_tests.rs`'s handcrafted fixtures
//! pin the query core's own logic; what they CANNOT catch is a rename/shape drift in the analysis
//! output itself (e.g. a `crossLayer` bucket key renamed on the serializer side would sail past a
//! handcrafted fixture that still spells the old name). This module is that rename-drift tripwire:
//! one fixture set engineered to populate ALL SIX join buckets, asserted through the sealed verdict
//! vocabulary end to end.

use serde_json::Value;

use crate::test_support::TempDir;
use crate::{analyze_trees_json, query_io_json};

/// Three trees engineered so every one of the six `crossLayer` buckets is non-empty, all keys
/// sharing the substring `users` so one broad query hits every bucket at once:
/// - `edges`: fe's `fetch("/api/users/list")` joins be1's lone `GET /api/users/list` provide.
/// - `unconsumedProvides`: be1's `GET /api/users/export` — nothing consumes it.
/// - `unprovidedConsumes`: fe's `fetch("/api/users/ghost")` — nothing provides it.
/// - `unresolvedConsumes`: fe's `fetch(usersUrl(x))` — dynamic key, `key: null` + `raw` recorded.
/// - `externalConsumes`: fe's absolute-URL `fetch("https://vendor.example.com/users")`.
/// - `ambiguousConsumes`: fe's `fetch("/api/users/dup")` — provided by BOTH be1 and be2 (two
///   distinct source trees), so it is never auto-linked.
fn six_bucket_analysis() -> String {
    let fe = TempDir::new("zzop-facade-query-fe");
    fe.write(
        "src/api.ts",
        "export function a() { return fetch(\"/api/users/list\"); }\n\
         export function b() { return fetch(\"/api/users/ghost\"); }\n\
         export function c() { return fetch(\"https://vendor.example.com/users\"); }\n\
         export function d() { return fetch(usersUrl(x)); }\n\
         export function e() { return fetch(\"/api/users/dup\"); }\n",
    );
    let be1 = TempDir::new("zzop-facade-query-be1");
    be1.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\n\
         apiRoutes.get(\"/api/users/list\", api.list);\n\
         apiRoutes.get(\"/api/users/export\", api.exportUsers);\n\
         apiRoutes.get(\"/api/users/dup\", api.dup);\n",
    );
    let be2 = TempDir::new("zzop-facade-query-be2");
    be2.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users/dup\", api.dup2);\n",
    );

    let config = format!(
        r#"{{"trees": [
            {{"root": {:?}, "sourceId": "fe"}},
            {{"root": {:?}, "sourceId": "be1"}},
            {{"root": {:?}, "sourceId": "be2"}}
        ]}}"#,
        fe.path().display(),
        be1.path().display(),
        be2.path().display()
    );
    analyze_trees_json(&config).expect("analyze_trees_json should succeed")
}

fn query(analysis: &str, pattern: &str) -> Value {
    let out = query_io_json(
        analysis,
        &serde_json::json!({ "pattern": pattern }).to_string(),
    )
    .expect("query should succeed");
    serde_json::from_str(&out).unwrap()
}

#[test]
fn real_serializer_output_populates_and_matches_all_six_buckets() {
    let analysis = six_bucket_analysis();

    // One broad pattern hits every bucket at once — six distinct classes => "mixed", and every
    // count is exactly 1. A serializer-side bucket rename/shape change zeroes its count here.
    let v = query(&analysis, "users");
    assert_eq!(v["verdict"], "mixed", "got: {v}");
    for bucket in [
        "edges",
        "unconsumedProvides",
        "unprovidedConsumes",
        "unresolvedConsumes",
        "externalConsumes",
        "ambiguousConsumes",
    ] {
        assert_eq!(
            v["counts"][bucket], 1,
            "bucket {bucket} must hold exactly the one engineered entry, got: {v}"
        );
    }
    // Matched objects are the engine's own serialized rows — spot-pin fields that would silently
    // vanish on a rename (`from`/`to` on edges, `raw` on an unresolved consume, `candidates` on an
    // ambiguous one).
    assert_eq!(v["matches"]["edges"][0]["key"], "GET /api/users/list");
    assert_eq!(v["matches"]["edges"][0]["from"]["source"], "fe");
    assert_eq!(v["matches"]["edges"][0]["to"]["source"], "be1");
    assert_eq!(
        v["matches"]["unresolvedConsumes"][0]["key"],
        Value::Null,
        "the dynamic consume must stay unkeyed, got: {v}"
    );
    assert_eq!(
        v["matches"]["unresolvedConsumes"][0]["raw"], "usersUrl(x)",
        "raw call-arg text must survive serialization, got: {v}"
    );
    assert_eq!(
        v["matches"]["ambiguousConsumes"][0]["candidates"]
            .as_array()
            .map(Vec::len),
        Some(2),
        "both provider trees must ride as candidates, got: {v}"
    );
}

#[test]
fn each_bucket_yields_its_verdict_token_through_the_real_serializer() {
    let analysis = six_bucket_analysis();
    // Per-bucket patterns chosen to match exactly ONE bucket's entry, so each sealed verdict token
    // is exercised end to end (key-based buckets on `key`, the unresolved one on `raw`).
    for (pattern, verdict) in [
        ("users/list", "linked"),
        ("users/export", "provided-only"),
        ("users/ghost", "consumed-unprovided"),
        ("usersUrl", "unresolved-only"),
        ("vendor.example.com", "external"),
        ("users/dup", "ambiguous"),
    ] {
        let v = query(&analysis, pattern);
        assert_eq!(
            v["verdict"], verdict,
            "pattern {pattern:?} must yield {verdict:?}, got: {v}"
        );
    }
    // And a pattern matching nothing anywhere: the not-found lane over real output.
    let v = query(&analysis, "no-such-endpoint-anywhere");
    assert_eq!(v["verdict"], "not-found", "got: {v}");
}
