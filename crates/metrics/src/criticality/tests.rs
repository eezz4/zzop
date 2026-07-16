//! Exercises transitive blast-radius criticality scoring.
use super::*;
use std::collections::HashMap;

struct P {
    fan_in: u32,
    change_count: u32,
    risk_score: f64,
    loc: u32,
}

impl Default for P {
    fn default() -> Self {
        P {
            fan_in: 0,
            change_count: 0,
            risk_score: 0.0,
            loc: 10,
        }
    }
}

fn node(path: &str, p: P) -> FileNode {
    FileNode {
        id: path.into(),
        path: path.into(),
        change_count: p.change_count,
        churn: 0,
        last_modified: None,
        author_count: 1,
        loc: p.loc,
        tag_counts: HashMap::new(),
        fan_in: p.fan_in,
        fan_out: 0,
        total_connections: 0,
        risk_score: p.risk_score,
        ..Default::default()
    }
}

fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
    pairs
        .iter()
        .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
        .collect()
}

#[test]
fn ranks_by_transitive_blast_radius() {
    // a -> b -> c (a imports b, b imports c). c's dependents = {a, b} = 2; b's = {a} = 1.
    let d = dep(&[("a.ts", &["b.ts"]), ("b.ts", &["c.ts"]), ("c.ts", &[])]);
    let nodes = vec![
        node("a.ts", P::default()),
        node(
            "b.ts",
            P {
                fan_in: 1,
                ..P::default()
            },
        ),
        node(
            "c.ts",
            P {
                fan_in: 1,
                ..P::default()
            },
        ),
    ];
    let crit = compute_criticality(
        &nodes,
        &d,
        1,
        CRITICALITY_SILENT_CHANGE_MAX,
        CRITICALITY_LIMIT,
    );
    let ranked: Vec<(&str, usize)> = crit
        .iter()
        .map(|c| (c.path.as_str(), c.blast_radius))
        .collect();
    assert_eq!(ranked, vec![("c.ts", 2), ("b.ts", 1)]);
}

#[test]
fn flags_high_blast_low_churn_hub_as_silent() {
    let d = dep(&[
        ("app.ts", &["core.ts"]),
        ("svc.ts", &["core.ts"]),
        ("core.ts", &[]),
    ]);
    let nodes = vec![
        node(
            "app.ts",
            P {
                change_count: 5,
                ..P::default()
            },
        ),
        node(
            "svc.ts",
            P {
                change_count: 5,
                ..P::default()
            },
        ),
        // rarely changed, depended on by 2
        node(
            "core.ts",
            P {
                fan_in: 2,
                change_count: 0,
                risk_score: 3.0,
                ..P::default()
            },
        ),
    ];
    let crit = compute_criticality(
        &nodes,
        &d,
        2,
        CRITICALITY_SILENT_CHANGE_MAX,
        CRITICALITY_LIMIT,
    );
    assert_eq!(crit.len(), 1);
    assert_eq!(crit[0].path, "core.ts");
    assert_eq!(crit[0].blast_radius, 2);
    assert!(crit[0].silent);
}

#[test]
fn weights_blast_by_hub_size() {
    // both imported by the same 3 files (equal blast 3); barrel is 5 LOC, core is 400 LOC.
    let d = dep(&[
        ("a.ts", &["barrel.ts", "core.ts"]),
        ("b.ts", &["barrel.ts", "core.ts"]),
        ("c.ts", &["barrel.ts", "core.ts"]),
        ("barrel.ts", &[]),
        ("core.ts", &[]),
    ]);
    let nodes = vec![
        node("a.ts", P::default()),
        node("b.ts", P::default()),
        node("c.ts", P::default()),
        node(
            "barrel.ts",
            P {
                fan_in: 3,
                loc: 5,
                ..P::default()
            },
        ),
        node(
            "core.ts",
            P {
                fan_in: 3,
                loc: 400,
                ..P::default()
            },
        ),
    ];
    let crit = compute_criticality(
        &nodes,
        &d,
        3,
        CRITICALITY_SILENT_CHANGE_MAX,
        CRITICALITY_LIMIT,
    );
    let paths: Vec<&str> = crit.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, vec!["core.ts", "barrel.ts"]); // equal blast, bigger hub ranked first
}

#[test]
fn cycle_safe_and_respects_min_blast_radius() {
    let d = dep(&[("x.ts", &["y.ts"]), ("y.ts", &["x.ts"])]); // cycle
    let nodes = vec![
        node(
            "x.ts",
            P {
                fan_in: 1,
                ..P::default()
            },
        ),
        node(
            "y.ts",
            P {
                fan_in: 1,
                ..P::default()
            },
        ),
    ];
    // each depends on the other -> blast 1 each; minBlastRadius 2 filters both out
    assert!(compute_criticality(
        &nodes,
        &d,
        2,
        CRITICALITY_SILENT_CHANGE_MAX,
        CRITICALITY_LIMIT
    )
    .is_empty());
    assert_eq!(
        compute_criticality(
            &nodes,
            &d,
            1,
            CRITICALITY_SILENT_CHANGE_MAX,
            CRITICALITY_LIMIT
        )
        .len(),
        2
    );
}
