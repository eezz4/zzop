//! Bug-evidence assembly and critical-finding escalation behavior, plus determinism of the whole
//! `build_recommendations` pipeline.

use super::*;

// --- bug evidence + escalation ---

#[test]
fn escalates_item_with_critical_finding_into_urgent_group_and_removes_from_home() {
    let nodes = [FileNode {
        tag_counts: tags(6),
        risk_score: 100.0,
        ..node("bug.ts")
    }];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    // A second, still-`Critical`-severity group (circular) that does NOT carry a critical finding —
    // proves urgency_rank, not severity_rank alone, is what puts the urgent group first.
    let circular = vec![vec!["cyc-a.ts".to_string(), "cyc-b.ts".to_string()]];
    let findings = [critical_finding("bug.ts", "be-db/update-delete-no-where")];
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &circular,
        scope_excludes: &[],
        permanent_ignores: &[],
        untested_paths: empty_set(),
        amplification_by_path: empty_map(),
        findings: &findings,
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());

    assert_eq!(recs[0].id, RecId::UrgentBugRisk);
    assert_eq!(recs[0].severity, Severity::Critical);
    assert_eq!(recs[0].items.len(), 1);
    let escalated = &recs[0].items[0];
    assert_eq!(escalated.path, "bug.ts");
    assert_eq!(escalated.escalated_from, Some(RecId::BugProne));
    assert_eq!(
        escalated.bug_evidence,
        vec!["1 critical finding(s) in this file: be-db/update-delete-no-where".to_string()]
    );

    // Home group (bug-prone) had only this one item -> dropped entirely, never double-reported.
    assert!(recs.iter().all(|r| r.id != RecId::BugProne));
    // The still-Critical circular group survives, just not at the top.
    assert!(recs.iter().any(|r| r.id == RecId::Circular));
}

#[test]
fn fix_ratio_evidence_rides_along_without_escalating() {
    let nodes = [FileNode {
        fan_out: 10,
        change_count: 10,
        tag_counts: tags(6),
        ..node("fat.ts")
    }];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let item = &fat.items[0];
    assert_eq!(item.path, "fat.ts");
    assert_eq!(
        item.bug_evidence,
        vec!["6 of 10 changes are bug-fix commits".to_string()]
    );
    assert_eq!(item.escalated_from, None);
    assert!(recs.iter().all(|r| r.id != RecId::UrgentBugRisk));
}

#[test]
fn hotspot_blast_evidence_rides_along_without_escalating() {
    let nodes = [FileNode {
        fan_out: 10,
        fan_in: 5,
        hotspot_score: Some(42.0),
        ..node("fat.ts")
    }];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let item = &fat.items[0];
    assert_eq!(
        item.bug_evidence,
        vec!["frequently changed and imported by 5 files".to_string()]
    );
    assert_eq!(item.escalated_from, None);
}

#[test]
fn no_evidence_item_is_unaffected() {
    let nodes = [FileNode {
        fan_out: 10,
        ..node("fat.ts")
    }];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let item = &fat.items[0];
    assert!(item.bug_evidence.is_empty());
    assert_eq!(item.escalated_from, None);
}

#[test]
fn bug_evidence_order_is_critical_findings_then_fix_ratio_then_hotspot() {
    let nodes = [FileNode {
        tag_counts: tags(6),
        change_count: 10,
        fan_in: 5,
        hotspot_score: Some(42.0),
        risk_score: 100.0,
        ..node("bug.ts")
    }];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let findings = [critical_finding("bug.ts", "be-db/update-delete-no-where")];
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        scope_excludes: &[],
        permanent_ignores: &[],
        untested_paths: empty_set(),
        amplification_by_path: empty_map(),
        findings: &findings,
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let urgent = recs.iter().find(|r| r.id == RecId::UrgentBugRisk).unwrap();
    assert_eq!(
        urgent.items[0].bug_evidence,
        vec![
            "1 critical finding(s) in this file: be-db/update-delete-no-where".to_string(),
            "6 of 10 changes are bug-fix commits".to_string(),
            "frequently changed and imported by 5 files".to_string(),
        ]
    );
}

#[test]
fn build_recommendations_is_deterministic_across_two_runs() {
    let nodes = [
        FileNode {
            tag_counts: tags(6),
            risk_score: 100.0,
            ..node("bug.ts")
        },
        FileNode {
            fan_out: 10,
            change_count: 10,
            tag_counts: tags(6),
            ..node("fat.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let findings = [critical_finding("bug.ts", "be-db/update-delete-no-where")];
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &[],
        scope_excludes: &[],
        permanent_ignores: &[],
        untested_paths: empty_set(),
        amplification_by_path: empty_map(),
        findings: &findings,
    };
    let gates = RecommendationGates::default();
    let r1 = build_recommendations(&input, &gates);
    let r2 = build_recommendations(&input, &gates);
    assert_eq!(
        serde_json::to_value(&r1).unwrap(),
        serde_json::to_value(&r2).unwrap()
    );
}
