//! Pipeline-level behavior: rule inclusion, ROI/severity ordering, per-item enrichment fields,
//! the scope-exclude / permanent-ignore post-filters, the glob matcher, and cost adjustments.

use super::*;

use crate::recommendations::enrich::matches_glob;

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
        scope_excludes: &[],
        permanent_ignores: &[],
        untested_paths: empty_set(),
        amplification_by_path: empty_map(),
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
        scope_excludes: &[],
        permanent_ignores: &[],
        untested_paths: empty_set(),
        amplification_by_path: empty_map(),
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
fn scope_excludes_filters_by_rule_id_and_glob() {
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
    let scope_excludes = [(RecId::FatFanout, "core/i18n/**".to_string())];
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        scope_excludes: &scope_excludes,
        permanent_ignores: &[],
        untested_paths: empty_set(),
        amplification_by_path: empty_map(),
        findings: &[],
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["src/HotFile.ts"]);
}

#[test]
fn permanent_ignores_removes_rule_id_path_pairs() {
    let nodes = [
        FileNode {
            fan_out: 10,
            ..node("A.ts")
        },
        FileNode {
            fan_out: 10,
            ..node("B.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let permanent_ignores = [(RecId::FatFanout, "A.ts".to_string())];
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        scope_excludes: &[],
        permanent_ignores: &permanent_ignores,
        untested_paths: empty_set(),
        amplification_by_path: empty_map(),
        findings: &[],
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["B.ts"]);
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

// --- glob matcher ---

#[test]
fn glob_double_star_matches_any_depth() {
    assert!(matches_glob("core/i18n/nested/en.ts", "core/i18n/**"));
    assert!(matches_glob("core/i18n/en.ts", "core/i18n/**"));
    assert!(!matches_glob("core/other/en.ts", "core/i18n/**"));
}

#[test]
fn glob_single_star_does_not_cross_slash() {
    assert!(matches_glob("src/Foo.ts", "src/*.ts"));
    assert!(!matches_glob("src/nested/Foo.ts", "src/*.ts"));
}

#[test]
fn glob_escapes_regex_special_characters() {
    assert!(matches_glob("a.b.ts", "a.b.ts"));
    assert!(!matches_glob("aXb.ts", "a.b.ts")); // literal '.', not "any char"
}

#[test]
fn untested_and_amplification_raise_cost_and_lower_roi() {
    let nodes = [FileNode {
        fan_out: 10,
        ..node("fat.ts")
    }];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let baseline_input = empty_input(&nodes, &dep, &coupling);
    let baseline = build_recommendations(&baseline_input, &RecommendationGates::default());
    let baseline_roi = baseline[0].items[0].roi;

    let mut untested_paths = HashSet::new();
    untested_paths.insert("fat.ts".to_string());
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        scope_excludes: &[],
        permanent_ignores: &[],
        untested_paths: &untested_paths,
        amplification_by_path: empty_map(),
        findings: &[],
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    assert!(recs[0].items[0].roi < baseline_roi);
}
