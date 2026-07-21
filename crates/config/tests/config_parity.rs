//! Config-front-end golden harness — replays every case of the committed
//! `docs/contracts/config-parity.fixture.json` through `zzop_config::mapper::config_to_request` and
//! deep-equals the emitted request against the fixture's expected JSON, pinning the mapper's output
//! against regression. (The fixture originated as a JS<->Rust parity anchor; the JS front-end was
//! removed 2026-07-20, so it is now the Rust front-end's sole golden.) This is the harness behind
//! `crates/config/Cargo.toml`'s "the emitted request JSON is pinned" claim.
//!
//! Two documented deltas are reversed before comparing (see the fixture's `_docs.normalization` and
//! the crate doc): (1) the mapper resolves `root`/`cacheDir`/`packsDir` against the config
//! directory — path values are mapped back to their raw form against the known base; (2) the
//! `withDefaults` layer is folded in — the bundled `packDefs` and default `git: {}` injections are
//! asserted, then stripped. Everything else must match exactly.

use serde_json::Value;
use std::path::Path;

const FIXTURE: &str = include_str!("../../../docs/contracts/config-parity.fixture.json");

/// The resolution base handed to `config_to_request`. A fixed lexical path (path resolution in the
/// mapper is purely lexical — no filesystem access), spelled with a drive prefix so it round-trips
/// identically on Windows (`Z:\...` native form, slash-normalized back) and Unix (plain string).
const BASE: &str = "Z:/zzop-parity-base";

/// Maps one emitted path value back to the fixture's raw (JS-side) form: slash-normalize, then
/// strip the `BASE` prefix (the base itself maps to `"."`, the default root). A value outside
/// `BASE` is returned as-is and fails the deep-equal — surfacing the difference, never hiding it.
fn unresolve(value: &str) -> String {
    let slashed = value.replace('\\', "/");
    if slashed == BASE {
        return ".".to_string();
    }
    match slashed.strip_prefix(&format!("{BASE}/")) {
        Some(rest) => rest.to_string(),
        None => slashed,
    }
}

/// Reverses the two documented Rust-side deltas on one tree request, in place:
/// path resolution (`root`/`cacheDir`/`packsDir`) and the `withDefaults` fold-in (`packDefs`
/// always; `git: {}` only when the fixture config declared no `git` key). The injections are
/// asserted before being stripped — a Rust mapper that STOPPED injecting them fails here.
fn normalize_tree(tree: &mut serde_json::Map<String, Value>, config_has_git: bool) {
    for key in ["root", "cacheDir"] {
        if let Some(s) = tree.get(key).and_then(Value::as_str) {
            let raw = unresolve(s);
            tree.insert(key.to_string(), Value::String(raw));
        }
    }
    if let Some(dirs) = tree.get_mut("packsDir").and_then(Value::as_array_mut) {
        for entry in dirs {
            if let Some(s) = entry.as_str() {
                *entry = Value::String(unresolve(s));
            }
        }
    }

    let pack_defs = tree
        .remove("packDefs")
        .expect("the Rust mapper must inject the bundled packs as packDefs on every tree");
    assert_eq!(
        pack_defs.as_array().map(Vec::len),
        Some(zzop_config::BUNDLED_PACK_SOURCES.len()),
        "packDefs must carry one entry per bundled pack"
    );
    if !config_has_git {
        let git = tree
            .remove("git")
            .expect("the Rust mapper must inject the default git: {} when the config has none");
        assert_eq!(
            git,
            serde_json::json!({}),
            "the injected git default must be exactly the empty object"
        );
    }
}

#[test]
fn fixture_is_well_formed_and_non_empty() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture must be valid JSON");
    let cases = fixture["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "the parity fixture must carry cases");
}

#[test]
fn rust_mapper_emits_the_committed_request_json_for_every_fixture_case() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture must be valid JSON");
    for case in fixture["cases"].as_array().expect("cases array") {
        let name = case["name"].as_str().expect("case name");
        let config = &case["config"];
        let expected_method = case["expected"]["method"].as_str().expect("method string");
        let expected_request = &case["expected"]["request"];

        let mapped = zzop_config::mapper::config_to_request(config, Path::new(BASE))
            .unwrap_or_else(|e| panic!("case {name:?}: config_to_request failed: {e}"));

        let method = match mapped.method {
            zzop_config::Method::Analyze => "analyze",
            zzop_config::Method::AnalyzeTrees => "analyzeTrees",
        };
        assert_eq!(method, expected_method, "case {name:?}: method drifted");

        let config_has_git = config.get("git").is_some();
        let mut request = mapped.request;
        match &mut request {
            Value::Object(single) if single.contains_key("trees") => {
                for tree in single
                    .get_mut("trees")
                    .and_then(Value::as_array_mut)
                    .expect("trees array")
                {
                    let tree_obj = tree.as_object_mut().expect("tree object");
                    normalize_tree(tree_obj, config_has_git);
                }
            }
            Value::Object(single) => normalize_tree(single, config_has_git),
            other => panic!("case {name:?}: request is not an object: {other}"),
        }

        assert_eq!(
            &request, expected_request,
            "case {name:?}: the Rust mapper's request JSON drifted from the committed parity \
             fixture (docs/contracts/config-parity.fixture.json) — if the change is intentional, \
             update the fixture to match the mapper's new output"
        );
    }
}
