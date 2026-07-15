//! Parity fixture for `zzop_core::http_interface_key` / `http_consume_interface_key` — the two
//! functions that normalize an HTTP method+path into the join key used to link a cross-tree
//! consume to a provide (see `crates/core/src/io.rs`'s `link_cross_layer_io`).
//!
//! Any adapter written OUTSIDE this crate (a different language, a hand-rolled extractor) must
//! reproduce these functions' output byte-for-byte, or its provides/consumes silently fail to
//! join — no error, just a missing edge. This test is the machine-verifiable contract: it computes
//! the key for a curated input list using the REAL Rust functions, then byte-compares that table
//! against the committed `docs/adapters/key-normalization.fixture.json`. Any adapter, in any
//! language, can replay the same table against its own normalizer to prove parity.
//!
//! Regenerating the fixture (after a deliberate normalization-rule change): run once with
//! `UPDATE_KEY_FIXTURE=1` set, then commit the resulting diff. A diff in the committed fixture is
//! a BREAKING adapter-facing change — call it out in release notes.

use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use zzop_core::{http_consume_interface_key, http_interface_key};

/// Field order is the fixture's canonical (and only) serialization order — declared here, not
/// alphabetical, so `serde_json`'s struct-order-preserving output is deterministic run-to-run.
#[derive(Serialize)]
struct FixtureRow {
    side: &'static str,
    method: &'static str,
    path: &'static str,
    key: String,
}

/// One `(side, method, rawInput)` per row. Comments group rows by the normalization rule (or
/// input-edge case) they exercise; see `http_interface_key` / `http_consume_interface_key` docs
/// in `crates/core/src/io.rs` for the rules themselves.
fn rows() -> Vec<FixtureRow> {
    let mut rows = Vec::new();

    let mut provide = |method: &'static str, path: &'static str| {
        let key = http_interface_key(method, path);
        rows.push(FixtureRow {
            side: "provide",
            method,
            path,
            key,
        });
    };

    // -- provide-side: leading-slash addition --
    provide("GET", "users");
    provide("GET", "/users");

    // -- provide-side: duplicate-slash collapse --
    provide("GET", "//users//profile");

    // -- provide-side: trailing-slash drop --
    provide("GET", "/users/");

    // -- provide-side: {x} param -> {} --
    provide("GET", "/users/{id}");

    // -- provide-side: :x param -> {} --
    provide("GET", "/users/:id");

    // -- provide-side: mixed {a}/:b params --
    provide("GET", "/users/{id}/posts/:postId");

    // -- provide-side: method upper-casing --
    provide("get", "/users");
    provide("Get", "/users");

    // -- provide-side: edge inputs --
    provide("GET", "");
    provide("GET", "/");
    provide("GET", "{id}"); // path that is only a param, no leading slash
    provide("GET", ":id"); // path that is only a param, colon style
    provide("GET", "///"); // pure-slash path

    // -- provide-side: '?' is NOT a query separator in a route pattern (e.g. Spring's single-char
    // wildcard) — must NOT be dropped, unlike the consume side.
    provide("GET", "/search?q={}");

    // -- provide-side: verb coverage --
    provide("POST", "/users");
    provide("PUT", "/users/{id}");
    provide("DELETE", "/users/{id}");
    provide("PATCH", "/users/{id}");

    let mut consume = |method: &'static str, raw_url: &'static str| {
        let key = http_consume_interface_key(method, raw_url);
        rows.push(FixtureRow {
            side: "consume",
            method,
            path: raw_url,
            key,
        });
    };

    // -- consume-side: leading-slash addition --
    consume("GET", "users");

    // -- consume-side: '?' query suffix drop --
    consume("GET", "/users?page=2");
    consume("POST", "articles?limit=10"); // cited in http_consume_interface_key's doc

    // -- consume-side: '#' fragment suffix drop --
    consume("GET", "/users#section");

    // -- consume-side: query drop + param normalization together --
    consume("GET", "/users/{id}?include=posts");
    consume("PATCH", "/users/{id}/posts/:postId?sort=desc");

    // -- consume-side: method upper-casing + colon param --
    consume("get", "/users/:id");

    // -- consume-side: duplicate-slash collapse + trailing-slash drop --
    consume("GET", "//api//users//");

    // -- consume-side: '#' takes priority over a later '?' (split on whichever comes first) --
    consume("DELETE", "/users/{id}#frag?stillquery");

    // -- consume-side: edge inputs --
    consume("GET", "");
    consume("GET", "/");
    consume("GET", "?page=2"); // query-only, empty path
    consume("GET", "/users/{id}#"); // empty fragment
    consume("GET", "/users/{id}?"); // empty query

    // -- consume-side: path casing is left untouched (only the method is upper-cased) --
    consume("GET", "/Users/Profile");

    rows
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/adapters/key-normalization.fixture.json")
}

fn canonical_json(rows: &[FixtureRow]) -> String {
    let mut s = serde_json::to_string_pretty(rows).expect("fixture rows must serialize");
    s.push('\n');
    s
}

#[test]
fn key_normalization_fixture_matches_committed_file() {
    let rows = rows();
    assert!(
        rows.len() >= 25 && rows.len() <= 40,
        "fixture row count ({}) drifted outside the curated 25-40 range — add/remove rows deliberately",
        rows.len()
    );
    let expected = canonical_json(&rows);
    let path = fixture_path();

    if std::env::var("UPDATE_KEY_FIXTURE").is_ok() {
        fs::write(&path, &expected).unwrap_or_else(|e| {
            panic!(
                "UPDATE_KEY_FIXTURE=1: failed to write {}: {e}",
                path.display()
            )
        });
    }

    let actual = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "failed to read committed fixture at {}: {e}\n\
             If this is a fresh checkout the file must already be committed — it is not generated \
             at build time.",
            path.display()
        )
    });

    assert_eq!(
        actual, expected,
        "\n\ndocs/adapters/key-normalization.fixture.json is out of sync with \
         zzop_core::http_interface_key / http_consume_interface_key.\n\
         This fixture is the public adapter-parity contract — a diff here is a BREAKING \
         adapter-facing change and must be called out in release notes.\n\
         To regenerate: run `UPDATE_KEY_FIXTURE=1 cargo test -p zzop-core --test \
         key_normalization_fixture`, then review and commit the diff.\n"
    );
}
