//! Exercises transitive blast-radius criticality scoring, plus the two properties that make the config's
//! top-level `exclude` a REPORTING filter here and not a computation one: an excluded file still counts
//! toward everyone else's blast radius, and it never eats a `limit` slot.
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

fn exclude(glob: &str) -> Vec<GlobalExclude> {
    vec![GlobalExclude {
        path: None,
        glob: Some(glob.to_string()),
    }]
}

/// The filter is a REPORT filter, not a graph edit: `vendor/wrapper.ts` disappears from the output while
/// still counting as one of `core.ts`'s importers. If the exclusion had been pushed into the computation,
/// `core.ts`'s blast radius would read 1 instead of 2 — a corrupted metric, not a filtered report.
#[test]
fn an_excluded_file_is_dropped_from_the_report_but_still_counts_toward_blast_radius() {
    let d = dep(&[
        ("app.ts", &["vendor/wrapper.ts"]),
        ("vendor/wrapper.ts", &["core.ts"]),
        ("core.ts", &[]),
    ]);
    let nodes = vec![
        node("app.ts", P::default()),
        node(
            "vendor/wrapper.ts",
            P {
                fan_in: 1,
                ..P::default()
            },
        ),
        node(
            "core.ts",
            P {
                fan_in: 1,
                ..P::default()
            },
        ),
    ];
    let crit = compute_criticality(
        &nodes,
        &d,
        &exclude("vendor/**"),
        1,
        CRITICALITY_SILENT_CHANGE_MAX,
        CRITICALITY_LIMIT,
    );
    let ranked: Vec<(&str, usize)> = crit
        .iter()
        .map(|c| (c.path.as_str(), c.blast_radius))
        .collect();
    assert_eq!(
        ranked,
        vec![("core.ts", 2)],
        "the excluded wrapper must vanish from the report while still counting as one of core.ts's two \
         transitive dependents"
    );
}

/// The filter runs BEFORE `truncate(limit)`. With the exclusion applied after truncation instead, the two
/// excluded hubs would consume both reported slots and this run would answer with an empty list while a
/// real, un-excluded hub sat right behind them.
#[test]
fn an_excluded_hub_never_eats_a_report_slot() {
    let d = dep(&[
        ("a.ts", &["vendor/big1.ts", "vendor/big2.ts", "core.ts"]),
        ("b.ts", &["vendor/big1.ts", "vendor/big2.ts", "core.ts"]),
        ("vendor/big1.ts", &[]),
        ("vendor/big2.ts", &[]),
        ("core.ts", &[]),
    ]);
    let hub = |path: &str, loc: u32| {
        node(
            path,
            P {
                fan_in: 2,
                loc,
                ..P::default()
            },
        )
    };
    let nodes = vec![
        node("a.ts", P::default()),
        node("b.ts", P::default()),
        // The two excluded hubs outrank `core.ts` (equal blast, far bigger), so they would fill a limit of 2.
        hub("vendor/big1.ts", 900),
        hub("vendor/big2.ts", 800),
        hub("core.ts", 10),
    ];
    let crit = compute_criticality(
        &nodes,
        &d,
        &exclude("vendor/**"),
        2,
        CRITICALITY_SILENT_CHANGE_MAX,
        2,
    );
    let paths: Vec<&str> = crit.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["core.ts"],
        "excluded hubs must be removed before ranking, not after — otherwise they consume the report's \
         limited slots and hide a hub the user asked to see"
    );
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
        &[],
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
        &[],
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
        &[],
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
        &[],
        2,
        CRITICALITY_SILENT_CHANGE_MAX,
        CRITICALITY_LIMIT
    )
    .is_empty());
    assert_eq!(
        compute_criticality(
            &nodes,
            &d,
            &[],
            1,
            CRITICALITY_SILENT_CHANGE_MAX,
            CRITICALITY_LIMIT
        )
        .len(),
        2
    );
}
