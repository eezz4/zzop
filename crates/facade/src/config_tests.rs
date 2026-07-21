//! Unit tests for request -> `EngineConfig` plumbing (`crate::config`).

use crate::config::build_engine_config;
use crate::AnalyzeRequest;

#[test]
fn analyze_request_adapter_overlays_flow_into_engine_config() {
    // Plumbing-only: proves the wire-facing `adapterOverlays` JSON field deserializes into
    // `AnalyzeRequest::adapter_overlays` and survives `build_engine_config` into
    // `EngineConfig::adapter_overlays` unchanged. The overlay MERGE itself (into a real
    // `analyze_tree` run) is already covered end-to-end by
    // `crates/engine/tests/analyze_adapter_overlay.rs` — this test never touches a filesystem
    // root, since `build_engine_config` doesn't need one to build the config.
    let config_json = r#"{
        "root": "unused",
        "sourceId": "t",
        "adapterOverlays": [
            {
                "format": "zzop-normalized-ast",
                "version": 1,
                "parser": "test-adapter/1",
                "source": "legacy",
                "files": [
                    {
                        "path": "a.ts",
                        "loc": 10,
                        "io": {
                            "provides": [
                                {"kind": "http", "key": "GET /foo", "file": "a.ts", "line": 1}
                            ],
                            "consumes": []
                        }
                    }
                ]
            }
        ]
    }"#;
    let req: AnalyzeRequest = serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
    assert_eq!(
        req.adapter_overlays.len(),
        1,
        "expected the field to deserialize"
    );

    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);
    assert_eq!(
        config.adapter_overlays.len(),
        1,
        "expected adapterOverlays to flow into EngineConfig::adapter_overlays"
    );
    assert_eq!(config.adapter_overlays[0].parser, "test-adapter/1");
    assert_eq!(
        config.adapter_overlays[0].files[0].io.provides[0].key, "GET /foo",
        "expected the overlay's io.provides entry to survive the round trip"
    );
}

#[test]
fn injected_routes_expand_into_a_synthetic_overlay_with_normalized_keys() {
    // The lightweight `routes` field: a provide (default role) and a consume, both keyed through the same
    // `http_interface_key` normalization the extractors use — so lowercase/trailing-slash input keys
    // canonically. Expanded into ONE synthetic overlay appended to `EngineConfig::adapter_overlays`, whose
    // `source` matches the tree so it makes no intra-source-mismatch claim. The join itself is covered
    // end-to-end by `crates/engine/tests/analyze_multi_tree.rs`.
    let config_json = r#"{
        "root": "unused",
        "sourceId": "be",
        "routes": [
            {"key": "get /api/users/"},
            {"key": "GET /articles?limit=10", "role": "consume"}
        ]
    }"#;
    let req: AnalyzeRequest = serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
    assert_eq!(
        req.routes.len(),
        2,
        "expected the routes field to deserialize"
    );

    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);
    assert_eq!(
        config.adapter_overlays.len(),
        1,
        "expected routes to expand into exactly one synthetic overlay"
    );
    let overlay = &config.adapter_overlays[0];
    assert_eq!(overlay.parser, "zzop-route-injection/1");
    assert_eq!(overlay.source, "be", "overlay source must match the tree");
    let io = &overlay.files[0].io;
    assert_eq!(
        io.provides
            .iter()
            .map(|p| p.key.as_str())
            .collect::<Vec<_>>(),
        vec!["GET /api/users"],
        "provide key must be normalized (uppercased, trailing slash stripped)"
    );
    assert_eq!(
        io.consumes
            .iter()
            .filter_map(|c| c.key.as_deref())
            .collect::<Vec<_>>(),
        vec!["GET /articles"],
        "consume role must key through http_consume_interface_key — the ?query is dropped so it joins a \
         native GET /articles provide"
    );
    assert!(
        warnings.is_empty(),
        "valid routes must not warn: {warnings:?}"
    );
}

#[test]
fn a_malformed_injected_route_key_is_skipped_with_a_warning() {
    // An injected fact that can never join (no METHOD/PATH split) is surfaced, never silently dropped.
    let config_json = r#"{
        "root": "unused",
        "sourceId": "be",
        "routes": [
            {"key": "not-a-route-key"},
            {"key": "GET /api/ok"}
        ]
    }"#;
    let req: AnalyzeRequest = serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);

    // The valid one still expands; only the malformed one is dropped.
    assert_eq!(config.adapter_overlays.len(), 1);
    assert_eq!(
        config.adapter_overlays[0].files[0].io.provides.len(),
        1,
        "only the valid route should survive"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("not-a-route-key") && w.contains("METHOD PATH")),
        "the malformed key must be surfaced: {warnings:?}"
    );
}

#[test]
fn no_routes_adds_no_overlay() {
    let config_json = r#"{ "root": "unused", "sourceId": "be" }"#;
    let req: AnalyzeRequest = serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
    assert!(req.routes.is_empty(), "routes must default to empty");
    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);
    assert!(
        config.adapter_overlays.is_empty(),
        "no routes must append no synthetic overlay"
    );
}

#[test]
fn analyze_request_git_commit_type_patterns_flow_into_engine_config() {
    // Plumbing-only, same spirit as `analyze_request_adapter_overlays_flow_into_engine_config`: proves
    // the wire-facing `git.commitTypePatterns` JSON field deserializes into
    // `GitOptionsRequest::commit_type_patterns` and survives `build_engine_config` into
    // `EngineConfig::git`'s `GitOptions::commit_type_patterns` unchanged, as `(String, String)` tuple
    // pairs. The end-to-end tagging behavior (a custom table actually reclassifying a commit) is
    // covered by `crates/engine/tests/analyze_git.rs`'s git-fixture tests instead.
    let config_json = r#"{
        "root": "unused",
        "sourceId": "t",
        "git": {
            "commitTypePatterns": [
                { "pattern": "^\\s*corrige\\b", "tag": "FIX" },
                { "pattern": "^\\s*nouveau\\b", "tag": "FEAT" }
            ]
        }
    }"#;
    let req: AnalyzeRequest = serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
    let git_req = req.git.as_ref().expect("expected git to deserialize");
    let patterns = git_req
        .commit_type_patterns
        .as_ref()
        .expect("expected commitTypePatterns to deserialize");
    assert_eq!(patterns.len(), 2);
    assert_eq!(patterns[0].pattern, "^\\s*corrige\\b");
    assert_eq!(patterns[0].tag, "FIX");

    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);
    let git_cfg = config.git.expect("expected EngineConfig::git to be Some");
    assert_eq!(
        git_cfg.commit_type_patterns,
        Some(vec![
            ("^\\s*corrige\\b".to_string(), "FIX".to_string()),
            ("^\\s*nouveau\\b".to_string(), "FEAT".to_string()),
        ])
    );
}

#[test]
fn analyze_request_git_without_commit_type_patterns_leaves_it_none() {
    // Absence must round-trip to `None` (falls back to the default table downstream), not an empty
    // `Some(vec![])` that would also be treated as "fall back" but is a different wire shape to pin.
    let config_json = r#"{"root": "unused", "sourceId": "t", "git": {}}"#;
    let req: AnalyzeRequest = serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
    let git_req = req.git.as_ref().expect("expected git to deserialize");
    assert!(git_req.commit_type_patterns.is_none());

    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);
    let git_cfg = config.git.expect("expected EngineConfig::git to be Some");
    assert!(git_cfg.commit_type_patterns.is_none());
}

#[test]
fn analyze_request_mounted_at_mounts_hosts_flow_into_engine_config() {
    // Plumbing-only, same spirit as `analyze_request_adapter_overlays_flow_into_engine_config`: proves
    // `mountedAt`/`mounts`/`hosts` deserialize and that `build_engine_config` folds every `mounts[]`
    // entry in array order FIRST, followed by `mountedAt` as the implicit `dir: ""` entry LAST — so
    // the engine's first-wins equal-length tie-break favors an explicit mount over the shorthand.
    let config_json = r#"{
        "root": "unused",
        "sourceId": "t",
        "mountedAt": "/gateway",
        "mounts": [
            { "dir": "apps/api", "at": "/api" },
            { "dir": "apps/admin", "at": "/admin" }
        ],
        "hosts": ["internal.example.com"]
    }"#;
    let req: AnalyzeRequest = serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
    assert_eq!(req.mounted_at.as_deref(), Some("/gateway"));
    assert_eq!(req.mounts.len(), 2);
    assert_eq!(req.hosts, vec!["internal.example.com".to_string()]);

    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);
    assert_eq!(
        config.mounts.len(),
        3,
        "expected both mounts[] entries first, then mountedAt"
    );
    assert_eq!(config.mounts[0].dir, "apps/api");
    assert_eq!(config.mounts[0].at, "/api");
    assert_eq!(config.mounts[1].dir, "apps/admin");
    assert_eq!(config.mounts[1].at, "/admin");
    assert_eq!(
        config.mounts[2].dir, "",
        "mountedAt becomes the dir \"\" entry, appended LAST so an explicit equal-length dir entry \
         (e.g. an explicit {{dir:\"\", at:...}} mount) wins the engine's first-wins tie-break over \
         the mountedAt shorthand"
    );
    assert_eq!(config.mounts[2].at, "/gateway");
    assert_eq!(config.hosts, vec!["internal.example.com".to_string()]);
}

#[test]
fn analyze_request_without_mounted_at_omits_the_implicit_whole_tree_mount() {
    let config_json = r#"{
        "root": "unused",
        "sourceId": "t",
        "mounts": [ { "dir": "apps/api", "at": "/api" } ]
    }"#;
    let req: AnalyzeRequest = serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
    assert!(req.mounted_at.is_none());

    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);
    assert_eq!(
        config.mounts.len(),
        1,
        "no mountedAt -> no implicit dir \"\" entry"
    );
    assert_eq!(config.mounts[0].dir, "apps/api");
    assert_eq!(config.mounts[0].at, "/api");
}

#[test]
fn analyze_request_defaults_mounted_at_mounts_hosts_to_empty() {
    let config_json = r#"{"root": "unused", "sourceId": "t"}"#;
    let req: AnalyzeRequest = serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
    assert!(req.mounted_at.is_none());
    assert!(req.mounts.is_empty());
    assert!(req.hosts.is_empty());

    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);
    assert!(config.mounts.is_empty());
    assert!(config.hosts.is_empty());
}
