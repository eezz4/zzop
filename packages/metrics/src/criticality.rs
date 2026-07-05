//! Criticality — the "stable but critical" axis that churn-weighted risk underweights. A legacy system's core is
//! often *old, rarely changed (low churn) yet depended on by everything (high blast radius)* — touch it and the whole
//! system shakes. The default risk score leans on churn, so a churn-0 hub scores low and hides as a silent time bomb.
//!
//! criticality = transitive **blast radius** (count of files that directly or transitively import this file). Reported
//! alongside churn so the consumer can isolate `high criticality x low churn` = pin with a characterization test /
//! document before anyone dares change it. Pure over (nodes, dep) — language-agnostic (works on adapter IR too).

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use zpz_core::{DepGraph, FileNode};

/// Minimum blast radius to qualify as a hub.
pub const CRITICALITY_MIN_BLAST_RADIUS: usize = 3;
/// changeCount at/below which a high-blast file is "silent".
pub const CRITICALITY_SILENT_CHANGE_MAX: u32 = 2;
/// Max rows returned (highest blast first).
pub const CRITICALITY_LIMIT: usize = 20;
/// `log(loc + OFFSET)` smoothing so a tiny re-export hub doesn't tie a large core's danger weight.
const LOG_LOC_OFFSET: f64 = 2.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CriticalFile {
    pub path: String,
    /// Transitive dependents — how many files break/shift if this one changes.
    pub blast_radius: usize,
    pub fan_in: u32,
    pub change_count: u32,
    pub risk_score: f64,
    /// Lines of code — the hub's own size; a big hub is a costlier bomb than a tiny re-export barrel of equal blast.
    pub loc: u32,
    /// Distinct authors (bus-factor proxy); 1 = single owner.
    pub author_count: u32,
    /// High blast radius yet rarely changed — the signal risk-by-churn misses.
    pub silent: bool,
}

pub fn compute_criticality(
    nodes: &[FileNode],
    dep: &DepGraph,
    min_blast_radius: usize,
    silent_change_max: u32,
    limit: usize,
) -> Vec<CriticalFile> {
    let dependents = build_dependents(dep); // imported -> set of direct importers
    let mut out: Vec<CriticalFile> = Vec::new();
    for n in nodes {
        let blast_radius = transitive_dependents(&n.path, &dependents);
        if blast_radius < min_blast_radius {
            continue;
        }
        out.push(CriticalFile {
            path: n.path.clone(),
            blast_radius,
            fan_in: n.fan_in,
            change_count: n.change_count,
            risk_score: n.risk_score,
            loc: n.loc,
            author_count: n.author_count,
            silent: n.change_count <= silent_change_max,
        });
    }
    // Rank by blast radius WEIGHTED by the hub's own size: two hubs with equal blast are not equal danger — a 5-line
    // re-export barrel is cheap to fix, a 400-line core is the real bomb. log(loc) dampens so size nudges, not
    // dominates.
    let weight = |c: &CriticalFile| -> f64 {
        c.blast_radius as f64 * (f64::from(c.loc) + LOG_LOC_OFFSET).ln()
    };
    out.sort_by(|a, b| {
        weight(b)
            .partial_cmp(&weight(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.blast_radius.cmp(&a.blast_radius))
            .then_with(|| a.path.cmp(&b.path))
    });
    out.truncate(limit);
    out
}

/// Reverse the import graph: imported file -> set of files that import it directly.
fn build_dependents(dep: &DepGraph) -> HashMap<&str, HashSet<&str>> {
    let mut rev: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (importer, imports) in dep {
        for imported in imports {
            rev.entry(imported.as_str())
                .or_default()
                .insert(importer.as_str());
        }
    }
    rev
}

/// Count of files transitively depending on `start` (BFS over reversed edges; cycle-safe via visited set).
fn transitive_dependents(start: &str, dependents: &HashMap<&str, HashSet<&str>>) -> usize {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    seen.insert(start);
    queue.push_back(start);
    while let Some(cur) = queue.pop_front() {
        if let Some(deps) = dependents.get(cur) {
            for &d in deps {
                if seen.insert(d) {
                    queue.push_back(d);
                }
            }
        }
    }
    seen.len() - 1 // exclude start itself
}

#[cfg(test)]
mod tests {
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
}
