//! `cross-layer/duplicate-route` tests — moved out of `duplicate_route.rs` when that file reached the
//! 300-line cap (the cap exempts `tests.rs`; see `scripts/check-max-file-lines.sh`).

use super::*;
use zzop_core::io::{AmbiguousConsume, TaggedProvide};
use zzop_core::{IoConsume, IoProvide};

fn dead(key: &str, source: &str, file: &str, line: u32) -> TaggedProvide {
    TaggedProvide {
        source: source.to_string(),
        provide: IoProvide {
            response: None,
            body: None,
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: None,
            ..Default::default()
        },
    }
}

#[test]
fn key_provided_by_two_trees_with_no_consumer_is_flagged_from_unconsumed_provides() {
    let cl = CrossLayerResult {
        unconsumed_provides: vec![
            dead("DELETE /api/me", "svc-a", "a.ts", 3),
            dead("DELETE /api/me", "svc-b", "b.ts", 9),
        ],
        ..Default::default()
    };
    let out = cross_layer_duplicate_route_findings(&cl);
    // One copy PER PROVIDING SOURCE, each anchored in its own tree (2026-07-29) — see the module doc.
    assert_eq!(out.len(), 2);
    let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
    assert_eq!(sites, vec![("a.ts", 3), ("b.ts", 9)]);
    for f in &out {
        assert_eq!(f.rule_id, "cross-layer/duplicate-route");
        assert_eq!(f.severity, Severity::Warning);
        assert!(f.message.contains("svc-a"), "{}", f.message);
        assert!(f.message.contains("svc-b"), "{}", f.message);
        assert!(f.message.contains("disabledRules"), "{}", f.message);
    }
    assert_eq!(out[0].data.as_ref().unwrap()["provideSource"], "svc-a");
    assert_eq!(out[1].data.as_ref().unwrap()["provideSource"], "svc-b");
}

#[test]
fn two_verb_unknown_sentinels_at_the_same_path_are_not_a_duplicate_route() {
    // A verb-unknown sentinel (`"? <path>"`) is never a duplicate-route candidate — its method is
    // unknown, so its key must never be grouped or surfaced in a finding message (it is disclosed via
    // `cross-layer/unknown-verb-route` instead). Two serve-all handlers at the same path across trees
    // must NOT produce a `? /api/foo` duplicate-route warning (opus review F1 regression pin).
    let cl = CrossLayerResult {
        unconsumed_provides: vec![
            dead("? /api/foo", "svc-a", "a/pages/api/foo.ts", 1),
            dead("? /api/foo", "svc-b", "b/foo.go", 1),
        ],
        ..Default::default()
    };
    assert!(cross_layer_duplicate_route_findings(&cl).is_empty());
}

#[test]
fn key_provided_by_two_trees_and_referenced_by_a_consume_is_flagged_from_ambiguous() {
    let cl = CrossLayerResult {
        ambiguous_consumes: vec![AmbiguousConsume {
            source: "gateway".to_string(),
            consume: IoConsume {
                client: None,
                body: None,
                kind: "http".to_string(),
                key: Some("GET /health".to_string()),
                file: "gw.ts".to_string(),
                line: 1,
                raw: None,
                method: None,
                retry_configured: None,
            },
            candidates: vec![
                dead("GET /health", "svc-a", "svc-a/health.ts", 3),
                dead("GET /health", "svc-b", "svc-b/health.ts", 7),
            ],
        }],
        ..Default::default()
    };
    let out = cross_layer_duplicate_route_findings(&cl);
    assert_eq!(out.len(), 2, "one per providing source");
    assert_eq!(out[0].file, "svc-a/health.ts");
    assert_eq!(out[0].line, 3);
    assert_eq!(out[1].file, "svc-b/health.ts");
    for f in &out {
        assert!(f.message.contains("svc-a"), "{}", f.message);
        assert!(f.message.contains("svc-b"), "{}", f.message);
    }
}

#[test]
fn provider_site_in_a_test_fixture_file_does_not_count_toward_duplication() {
    // svc-b's "registration" is a test fixture — not deployed surface. With it skipped, only one
    // real provider tree remains, so no duplicate-route finding.
    let cl = CrossLayerResult {
        unconsumed_provides: vec![
            dead("DELETE /api/me", "svc-a", "src/api/routes.ts", 3),
            dead(
                "DELETE /api/me",
                "svc-b",
                "src/api/__test__/handlers.test.ts",
                125,
            ),
        ],
        ..Default::default()
    };
    assert!(cross_layer_duplicate_route_findings(&cl).is_empty());
}

#[test]
fn key_provided_by_only_one_tree_is_not_flagged() {
    let cl = CrossLayerResult {
        unconsumed_provides: vec![
            dead("GET /api/users", "svc-a", "a.ts", 3),
            dead("GET /api/users", "svc-a", "a2.ts", 5),
        ],
        ..Default::default()
    };
    assert!(cross_layer_duplicate_route_findings(&cl).is_empty());
}

#[test]
fn non_http_kind_is_ignored() {
    let cl = CrossLayerResult {
        unconsumed_provides: vec![
            TaggedProvide {
                source: "svc-a".to_string(),
                provide: IoProvide {
                    response: None,
                    body: None,
                    kind: "db-table".to_string(),
                    key: "table:users".to_string(),
                    file: "a.sql".to_string(),
                    line: 1,
                    symbol: None,
                    ..Default::default()
                },
            },
            TaggedProvide {
                source: "svc-b".to_string(),
                provide: IoProvide {
                    response: None,
                    body: None,
                    kind: "db-table".to_string(),
                    key: "table:users".to_string(),
                    file: "b.sql".to_string(),
                    line: 1,
                    symbol: None,
                    ..Default::default()
                },
            },
        ],
        ..Default::default()
    };
    assert!(cross_layer_duplicate_route_findings(&cl).is_empty());
}
