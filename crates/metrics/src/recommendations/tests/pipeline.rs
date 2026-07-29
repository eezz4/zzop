//! Pipeline-level behavior: rule inclusion, ROI/severity ordering, per-item enrichment fields, and
//! the config-exclude post-filter.

use super::*;

use zzop_core::GlobalExclude;

#[test]
fn every_applicable_rule_is_included_no_persona_filtering() {
    let nodes = [
        FileNode {
            tag_counts: tags(6),
            risk_score: 100.0,
            ..node("bug.ts")
        },
        FileNode {
            fan_out: 10,
            risk_score: 80.0,
            ..node("fat.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        excludes: &[],
        findings: &[],
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    assert!(recs.iter().any(|r| r.id == RecId::BugProne));
    assert!(recs.iter().any(|r| r.id == RecId::FatFanout));
}

#[test]
fn sorted_descending_by_roi_within_the_same_rule() {
    let nodes = [
        FileNode {
            tag_counts: tags(10),
            risk_score: 200.0,
            loc: 40,
            fan_in: 1,
            ..node("hi.ts")
        },
        FileNode {
            tag_counts: tags(6),
            risk_score: 40.0,
            loc: 200,
            fan_in: 10,
            ..node("lo.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        excludes: &[],
        findings: &[],
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let bug = &recs[0];
    assert_eq!(bug.items[0].path, "hi.ts");
    assert!(bug.items[0].roi > bug.items[1].roi);
}

#[test]
fn each_item_carries_roi_estimated_reduction_estimated_cost_action_hint_key_fan_in() {
    let nodes = [FileNode {
        fan_out: 10,
        loc: 50,
        risk_score: 60.0,
        fan_in: 4,
        ..node("fat.ts")
    }];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = &recs[0];
    assert_eq!(fat.items[0].path, "fat.ts");
    assert!(fat.items[0].roi.is_finite());
    assert!(fat.items[0].estimated_reduction.is_finite());
    assert!(fat.items[0].estimated_cost.is_finite());
    assert_eq!(fat.items[0].action_hint_key, ActionHintKey::FatFanoutSmall);
    assert_eq!(fat.items[0].fan_in, 4);
}

#[test]
fn config_excludes_drop_matching_items_by_glob() {
    let nodes = [
        FileNode {
            fan_out: 10,
            ..node("core/i18n/en.ts")
        },
        FileNode {
            fan_out: 10,
            ..node("src/HotFile.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let excludes = [GlobalExclude {
        path: None,
        glob: Some("core/i18n/**".to_string()),
    }];
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        excludes: &excludes,
        findings: &[],
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["src/HotFile.ts"]);
}

#[test]
fn config_excludes_drop_matching_items_by_substring_path() {
    let nodes = [
        FileNode {
            fan_out: 10,
            ..node("legacy/A.ts")
        },
        FileNode {
            fan_out: 10,
            ..node("B.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let excludes = [GlobalExclude {
        path: Some("legacy/".to_string()),
        glob: None,
    }];
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        excludes: &excludes,
        findings: &[],
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["B.ts"]);
}

/// Rule-agnostic, exactly like the findings-side filter: one exclude covers every recommendation group a
/// path could land in, not just the group the test happened to look at.
#[test]
fn config_excludes_apply_to_every_rule_group_at_once() {
    let nodes = [FileNode {
        fan_out: 10,
        author_count: 7,
        tag_counts: tags(6),
        risk_score: 100.0,
        ..node("legacy/A.ts")
    }];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();

    let baseline_input = empty_input(&nodes, &dep, &coupling);
    let baseline = build_recommendations(&baseline_input, &RecommendationGates::default());
    assert!(
        baseline.len() >= 2,
        "fixture must trip 2+ rules, else this test is vacuous: {:?}",
        baseline.iter().map(|r| r.id).collect::<Vec<_>>()
    );

    let excludes = [GlobalExclude {
        path: None,
        glob: Some("legacy/**".to_string()),
    }];
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        excludes: &excludes,
        findings: &[],
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    assert!(
        recs.is_empty(),
        "one exclude must empty every group the path appeared in: {:?}",
        recs
    );
}

/// A `GlobalExclude` with neither filter matches NOTHING (`zzop_core::global_exclude_matches_path`'s
/// deliberate divergence from `Suppression`) — pinned here so this channel can never become the one place
/// where an empty entry silently drops every recommendation.
#[test]
fn a_filterless_exclude_entry_drops_nothing() {
    let nodes = [FileNode {
        fan_out: 10,
        ..node("A.ts")
    }];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let excludes = [GlobalExclude {
        path: None,
        glob: None,
    }];
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        excludes: &excludes,
        findings: &[],
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    assert_eq!(fat.items[0].path, "A.ts");
}

#[test]
fn severity_order_critical_then_warning_then_info() {
    let nodes = [
        FileNode {
            tag_counts: tags(6),
            risk_score: 100.0,
            ..node("bug.ts")
        },
        FileNode {
            fan_out: 10,
            ..node("fat.ts")
        },
        FileNode {
            author_count: 7,
            ..node("silo.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let sevs: Vec<Severity> = recs.iter().map(|r| r.severity).collect();
    let idx_of = |s: Severity| sevs.iter().position(|&x| x == s).unwrap();
    assert!(idx_of(Severity::Critical) < idx_of(Severity::Warning));
    assert!(idx_of(Severity::Warning) < idx_of(Severity::Info));
}
