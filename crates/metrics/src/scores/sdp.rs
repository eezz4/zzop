//! Stable Dependencies Principle — ratio of cross-module imports flowing from a high-instability module to a
//! low-instability one (unstable-depends-on-unstable is fine; a stable module reaching into something unstable is
//! the SDP violation, since the stable module now inherits the unstable one's churn). Instability I = out / (in +
//! out) per module (Robert Martin's metric).

use std::collections::BTreeMap;

use super::config::ScoresConfig;
use super::shared::{is_external, module_of, round};
use super::types::{SdpScore, SdpViolation};
use zzop_core::DepGraph;

#[derive(Default, Clone, Copy)]
struct Degree {
    inbound: u32,
    outbound: u32,
}

pub fn compute_sdp(dep: &DepGraph, cfg: &ScoresConfig) -> SdpScore {
    let (deg, pairs) = collect_deg_and_pairs(dep, cfg);
    let instability = compute_instability(&deg);
    let (mut violations, total, bad) = classify_pairs(&pairs, &instability);

    violations.sort_by(|a, b| {
        b.edge_count
            .cmp(&a.edge_count)
            .then_with(|| a.from_slice.cmp(&b.from_slice))
            .then_with(|| a.to_slice.cmp(&b.to_slice))
    });

    let score = if total == 0 {
        100.0
    } else {
        (100.0 - (bad as f64 / total as f64) * 100.0).max(0.0)
    };

    SdpScore {
        score: round(score),
        total_cross_slice_edges: total,
        violations,
    }
}

/// `pairs` keyed by `"{from_module}|{to_module}"` (BTreeMap gives deterministic traversal order over a
/// HashMap-backed DepGraph). Only cross-module, non-external edges count.
fn collect_deg_and_pairs(
    dep: &DepGraph,
    cfg: &ScoresConfig,
) -> (BTreeMap<String, Degree>, BTreeMap<String, u32>) {
    let mut deg: BTreeMap<String, Degree> = BTreeMap::new();
    let mut pairs: BTreeMap<String, u32> = BTreeMap::new();

    let mut froms: Vec<&String> = dep.keys().collect();
    froms.sort();

    for from in froms {
        let fm = module_of(cfg, from);
        for to in &dep[from] {
            if is_external(to) {
                continue;
            }
            let tm = module_of(cfg, to);
            let (fm, tm) = match (&fm, &tm) {
                (Some(f), Some(t)) if f != t => (f, t),
                _ => continue,
            };
            let key = format!("{}|{}", fm, tm);
            *pairs.entry(key).or_insert(0) += 1;
            deg.entry(fm.clone()).or_default().outbound += 1;
            deg.entry(tm.clone()).or_default().inbound += 1;
        }
    }
    (deg, pairs)
}

fn compute_instability(deg: &BTreeMap<String, Degree>) -> BTreeMap<String, f64> {
    deg.iter()
        .map(|(m, d)| {
            let i = if d.inbound + d.outbound == 0 {
                0.0
            } else {
                f64::from(d.outbound) / f64::from(d.inbound + d.outbound)
            };
            (m.clone(), i)
        })
        .collect()
}

fn classify_pairs(
    pairs: &BTreeMap<String, u32>,
    instability: &BTreeMap<String, f64>,
) -> (Vec<SdpViolation>, u32, u32) {
    let mut violations = Vec::new();
    let mut total: u32 = 0;
    let mut bad: u32 = 0;

    for (key, &edge_count) in pairs {
        total += edge_count;
        let (from_slice, to_slice) = match key.split_once('|') {
            Some((f, t)) => (f, t),
            None => continue,
        };
        let from_i = instability.get(from_slice).copied().unwrap_or(0.0);
        let to_i = instability.get(to_slice).copied().unwrap_or(0.0);
        if from_i < to_i {
            bad += edge_count;
            violations.push(SdpViolation {
                from_slice: from_slice.to_string(),
                to_slice: to_slice.to_string(),
                from_i: round_two(from_i),
                to_i: round_two(to_i),
                edge_count,
            });
        }
    }
    (violations, total, bad)
}

fn round_two(n: f64) -> f64 {
    round(n * 100.0) / 100.0
}

#[cfg(test)]
mod tests {
    //! Covers the empty-graph baseline, an unstable module importing a stable one (compliant), a stable
    //! module depending on an unstable one (a violation), and intra-module/external edges being excluded.
    use super::*;

    fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
        pairs
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    fn cfg() -> ScoresConfig {
        ScoresConfig::default()
    }

    #[test]
    fn empty_graph_score_100_no_violations() {
        let r = compute_sdp(&DepGraph::new(), &cfg());
        assert_eq!(r.score, 100.0);
        assert_eq!(r.total_cross_slice_edges, 0);
        assert!(r.violations.is_empty());
    }

    #[test]
    fn unstable_importing_stable_no_violation_score_100() {
        // a: out1 -> instability 1 ; b: in1 -> instability 0
        // fromI(1) < toI(0) is false -> compliant
        let d = dep(&[("a/x.ts", &["b/y.ts"]), ("b/y.ts", &[])]);
        let r = compute_sdp(&d, &cfg());
        assert_eq!(r.total_cross_slice_edges, 1);
        assert!(r.violations.is_empty());
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn stable_module_depending_on_unstable_one_violation_score_80() {
        // a: in2 out1 -> instability 0.33 ; b: in1 out2 -> instability 0.67
        // pairs: a|b, c|a, d|a, b|e, b|f -> total 5
        // only a|b has fromI(0.33) < toI(0.67) -> bad 1
        // score = 100 - (1/5)*100 = 80
        let d = dep(&[
            ("a/x.ts", &["b/y.ts"]),
            ("c/z.ts", &["a/x.ts"]),
            ("d/z.ts", &["a/x.ts"]),
            ("b/y.ts", &["e/q.ts"]),
            ("b/w.ts", &["f/q.ts"]),
        ]);
        let r = compute_sdp(&d, &cfg());
        assert_eq!(r.total_cross_slice_edges, 5);
        assert_eq!(r.violations.len(), 1);
        assert_eq!(r.violations[0].from_slice, "a");
        assert_eq!(r.violations[0].to_slice, "b");
        assert_eq!(r.violations[0].from_i, 0.33);
        assert_eq!(r.violations[0].to_i, 0.67);
        assert_eq!(r.violations[0].edge_count, 1);
        assert_eq!(r.score, 80.0);
    }

    #[test]
    fn intra_module_and_external_edges_are_excluded() {
        let d = dep(&[("a/x.ts", &["a/y.ts", "react"]), ("a/y.ts", &[])]);
        let r = compute_sdp(&d, &cfg());
        assert_eq!(r.total_cross_slice_edges, 0);
        assert_eq!(r.score, 100.0);
    }
}
