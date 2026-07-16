//! Per-rule gates and notes: fat-fanout barrel/re-export/orchestrator exclusions and LOC branch,
//! hidden-coupling dedup/import-edge skip, versioning-candidate gates, action-hint branches, and
//! the circular-cycle note format.

use super::*;

#[test]
fn fat_fanout_auto_excludes_barrel_files() {
    let nodes = [
        FileNode {
            fan_out: 20,
            ..node("src/index.ts")
        },
        FileNode {
            fan_out: 20,
            ..node("features/some/index.tsx")
        },
        FileNode {
            fan_out: 20,
            ..node("RealFat.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["RealFat.ts"]);
}

#[test]
fn fat_fanout_excludes_structural_reexport_barrels() {
    let nodes = [
        // 0.83 — barrel of `Pkg.X = require(...)` lines
        FileNode {
            fan_out: 30,
            loc: 36,
            ..node("module/main.js")
        },
        // 0.016 — real dispatcher with logic
        FileNode {
            fan_out: 9,
            loc: 574,
            ..node("core/Engine.js")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["core/Engine.js"]);
}

#[test]
fn fat_fanout_excludes_orchestrators() {
    let nodes = [
        FileNode {
            fan_out: 15,
            ..node("App.tsx")
        },
        FileNode {
            fan_out: 15,
            ..node("pages/recommendation/RecommendationPage.tsx")
        },
        FileNode {
            fan_out: 15,
            ..node("api/apiRoutes.ts")
        },
        FileNode {
            fan_out: 15,
            ..node("features/evidence/Real.tsx")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, vec!["features/evidence/Real.tsx"]);
}

#[test]
fn fat_fanout_loc_branch_small_vs_large() {
    let nodes = [
        FileNode {
            fan_out: 10,
            loc: 50,
            ..node("small.ts")
        },
        FileNode {
            fan_out: 10,
            loc: 200,
            ..node("large.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
    let small = fat.items.iter().find(|i| i.path == "small.ts").unwrap();
    let large = fat.items.iter().find(|i| i.path == "large.ts").unwrap();
    assert_eq!(small.action_hint_key, ActionHintKey::FatFanoutSmall);
    assert_eq!(large.action_hint_key, ActionHintKey::FatFanoutLarge);
}

// --- hidden coupling dedup / rule gates ---

#[test]
fn hidden_coupling_dedups_symmetric_pairs_and_skips_importers() {
    let nodes: [FileNode; 0] = [];
    let dep = DepGraph::new();
    let mut coupling = CouplingMap::new();
    coupling.insert(
        "a.ts".to_string(),
        vec![crate::coupling::CouplingEntry {
            path: "b.ts".to_string(),
            count: 12,
        }],
    );
    coupling.insert(
        "b.ts".to_string(),
        vec![crate::coupling::CouplingEntry {
            path: "a.ts".to_string(),
            count: 12,
        }],
    );
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let hidden = recs.iter().find(|r| r.id == RecId::HiddenCoupling).unwrap();
    assert_eq!(hidden.items.len(), 1); // a|b and b|a collapse to a single item
    assert_eq!(hidden.items[0].path, "a.ts");
    assert_eq!(hidden.items[0].note.as_deref(), Some("12x ↔ b.ts"));
}

#[test]
fn hidden_coupling_skips_pairs_with_a_static_import_edge() {
    let nodes: [FileNode; 0] = [];
    let mut dep = DepGraph::new();
    dep.insert("a.ts".to_string(), vec!["b.ts".to_string()]);
    let mut coupling = CouplingMap::new();
    coupling.insert(
        "a.ts".to_string(),
        vec![crate::coupling::CouplingEntry {
            path: "b.ts".to_string(),
            count: 12,
        }],
    );
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    assert!(recs.iter().all(|r| r.id != RecId::HiddenCoupling));
}

#[test]
fn versioning_candidate_requires_volatile_fan_in_and_fix() {
    let nodes = [FileNode {
        lifecycle: Some(Lifecycle::Volatile),
        fan_in: 5,
        tag_counts: tags(4),
        ..node("legacy.ts")
    }];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let vc = recs
        .iter()
        .find(|r| r.id == RecId::VersioningCandidate)
        .unwrap();
    assert_eq!(vc.items[0].path, "legacy.ts");
    assert_eq!(
        vc.items[0].note.as_deref(),
        Some("volatile · fan_in 5 · FIX 4")
    );
    assert_eq!(
        vc.items[0].action_hint_key,
        ActionHintKey::VersioningCandidate
    );
}

#[test]
fn bug_prone_action_hint_key_branches_on_fan_in() {
    let nodes = [
        FileNode {
            tag_counts: tags(6),
            fan_in: 3,
            ..node("shared.ts")
        },
        FileNode {
            tag_counts: tags(6),
            fan_in: 0,
            ..node("isolated.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let bug = recs.iter().find(|r| r.id == RecId::BugProne).unwrap();
    let shared = bug.items.iter().find(|i| i.path == "shared.ts").unwrap();
    let isolated = bug.items.iter().find(|i| i.path == "isolated.ts").unwrap();
    assert_eq!(shared.action_hint_key, ActionHintKey::BugProneShared);
    assert_eq!(isolated.action_hint_key, ActionHintKey::BugProneIsolated);
}

#[test]
fn hot_churn_action_hint_key_branches_on_fan_in() {
    let nodes = [
        FileNode {
            loc: 40,
            churn: 500,
            fan_in: 5,
            ..node("core.ts")
        },
        FileNode {
            loc: 40,
            churn: 500,
            fan_in: 0,
            ..node("leaf.ts")
        },
    ];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let input = empty_input(&nodes, &dep, &coupling);
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let hot = recs.iter().find(|r| r.id == RecId::HotChurn).unwrap();
    let core = hot.items.iter().find(|i| i.path == "core.ts").unwrap();
    let leaf = hot.items.iter().find(|i| i.path == "leaf.ts").unwrap();
    assert_eq!(core.action_hint_key, ActionHintKey::HotChurnCore);
    assert_eq!(leaf.action_hint_key, ActionHintKey::HotChurnLeaf);
}

#[test]
fn circular_note_joins_cycle_with_arrow_back_to_start() {
    let nodes: [FileNode; 0] = [];
    let dep = DepGraph::new();
    let coupling = CouplingMap::new();
    let circular = vec![vec![
        "a.ts".to_string(),
        "b.ts".to_string(),
        "c.ts".to_string(),
    ]];
    let input = BuildRecInput {
        nodes: &nodes,
        dep: &dep,
        coupling: &coupling,
        circular: &circular,
        scope_excludes: &[],
        permanent_ignores: &[],
        untested_paths: empty_set(),
        amplification_by_path: empty_map(),
        findings: &[],
    };
    let recs = build_recommendations(&input, &RecommendationGates::default());
    let circ = recs.iter().find(|r| r.id == RecId::Circular).unwrap();
    assert_eq!(circ.items[0].path, "a.ts");
    assert_eq!(
        circ.items[0].note.as_deref(),
        Some("a.ts → b.ts → c.ts → a.ts")
    );
    assert_eq!(circ.items[0].action_hint_key, ActionHintKey::Circular);
}
