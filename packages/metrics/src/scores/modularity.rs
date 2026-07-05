//! Newman Q modularity — community clarity of the module partition. Treats each FSD module as a community
//! and computes the standard modularity statistic Q = sum_m(e_m - d_m^2), where e_m is the fraction of edges
//! internal to module m and d_m is module m's fraction of total edge-endpoints. Q >= 0.3 is the conventional
//! "clearly modular" threshold; the score linearly maps Q against that target.

use std::collections::BTreeMap;

use super::config::ScoresConfig;
use super::shared::{is_external, module_of};
use super::types::ModularityScore;
use zpz_core::DepGraph;

/// Edges are counted twice in the degree sum (Newman's convention): once per endpoint.
const TOTAL_DEGREE_FACTOR: f64 = 2.0;

pub fn compute_modularity(dep: &DepGraph, cfg: &ScoresConfig) -> ModularityScore {
    let edges = collect_edges(dep, cfg);
    if edges.is_empty() {
        return ModularityScore {
            score: 100.0,
            q: 0.0,
            edge_count: 0,
            slice_count: 0,
        };
    }
    let (deg, inside) = accumulate(&edges);
    let m = edges.len() as f64;
    let mut q = 0.0;
    for (module, &d) in &deg {
        let e = inside.get(module).copied().unwrap_or(0) as f64 / m;
        let dd = d as f64 / (TOTAL_DEGREE_FACTOR * m);
        q += e - dd * dd;
    }
    let target_q = cfg.thresholds.modularity.target_q;
    let score = (q / target_q * 100.0).clamp(0.0, 100.0);
    ModularityScore {
        score: score.round(),
        q: (q * 100.0).round() / 100.0,
        edge_count: edges.len() as u32,
        slice_count: deg.len() as u32,
    }
}

fn collect_edges(dep: &DepGraph, cfg: &ScoresConfig) -> Vec<(String, String)> {
    let mut edges = Vec::new();
    for (from, imports) in dep {
        let Some(fm) = module_of(cfg, from) else {
            continue;
        };
        for to in imports {
            if is_external(to) {
                continue;
            }
            let Some(tm) = module_of(cfg, to) else {
                continue;
            };
            edges.push((fm.clone(), tm));
        }
    }
    edges
}

fn accumulate(edges: &[(String, String)]) -> (BTreeMap<String, u32>, BTreeMap<String, u32>) {
    let mut deg: BTreeMap<String, u32> = BTreeMap::new();
    let mut inside: BTreeMap<String, u32> = BTreeMap::new();
    for (a, b) in edges {
        *deg.entry(a.clone()).or_insert(0) += 1;
        *deg.entry(b.clone()).or_insert(0) += 1;
        if a == b {
            *inside.entry(a.clone()).or_insert(0) += 1;
        }
    }
    (deg, inside)
}

#[cfg(test)]
mod tests {
    //! Covers the empty-graph baseline, Q and score for two cohesive modules plus one cross-module edge,
    //! the Q=0 case when all edges are inside one module, and external imports being excluded from the
    //! edge count — all against `ScoresConfig::default()`.
    use super::*;

    fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
        pairs
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    #[test]
    fn empty_graph_score_100_q_0() {
        let r = compute_modularity(&DepGraph::new(), &ScoresConfig::default());
        assert_eq!(r.score, 100.0);
        assert_eq!(r.q, 0.0);
        assert_eq!(r.edge_count, 0);
        assert_eq!(r.slice_count, 0);
    }

    #[test]
    fn two_cohesive_modules_plus_one_cross_edge_q_approx_017_score_56() {
        // edges: [a,a], [a,b], [b,b]  -> M = 3
        // deg a = 3, deg b = 3; inside a = 1, inside b = 1
        // each term: 1/3 - (3/6)^2 = 0.3333 - 0.25 = 0.08333
        // q = 0.16667 -> round 0.17 ; score = 0.16667/0.3*100 = 55.56 -> 56
        let d = dep(&[
            ("a/1.ts", &["a/2.ts"]),
            ("a/2.ts", &["b/1.ts"]),
            ("b/1.ts", &["b/2.ts"]),
            ("b/2.ts", &[]),
        ]);
        let r = compute_modularity(&d, &ScoresConfig::default());
        assert_eq!(r.edge_count, 3);
        assert_eq!(r.slice_count, 2);
        assert_eq!(r.q, 0.17);
        assert_eq!(r.score, 56.0);
    }

    #[test]
    fn all_edges_inside_one_module_q_0_score_0() {
        // edges [a,a],[a,a] -> M=2, deg a=4, inside a=2
        // e=2/2=1, d=4/4=1 -> 1 - 1 = 0
        let d = dep(&[
            ("a/x.ts", &["a/y.ts"]),
            ("a/y.ts", &["a/z.ts"]),
            ("a/z.ts", &[]),
        ]);
        let r = compute_modularity(&d, &ScoresConfig::default());
        assert_eq!(r.edge_count, 2);
        assert_eq!(r.slice_count, 1);
        assert_eq!(r.q, 0.0);
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn external_imports_are_excluded_from_edges() {
        let d = dep(&[("a/x.ts", &["react", "@scope/pkg"])]);
        let r = compute_modularity(&d, &ScoresConfig::default());
        assert_eq!(r.edge_count, 0);
        assert_eq!(r.score, 100.0);
    }
}
