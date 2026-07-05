//! Diamond dependency — detects duplicate paths of the form root -> [a, b] -> leaf. A diamond is not
//! inherently bad, but a proliferation of them signals that the graph funnels through few common
//! dependencies from many directions — a maintenance and refactor-blast-radius smell.

use std::collections::{BTreeMap, HashSet};

use super::config::ScoresConfig;
use super::shared::is_external;
use super::types::{DiamondPair, DiamondScore};
use zzop_core::DepGraph;

/// A pair only counts as a diamond when at least two distinct first-hop nodes reach the same leaf.
const MIN_THROUGH: usize = 2;
/// Detail list is capped like every other scores/* violation list.
const MAX_DETAIL_ITEMS: usize = 50;

pub fn compute_diamond(dep: &DepGraph, cfg: &ScoresConfig) -> DiamondScore {
    let adj = build_adj(dep);
    let mut pairs: Vec<DiamondPair> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    // BTreeMap iteration order keeps root processing deterministic; ties are broken later by the final
    // sort — only leaf order matters, and that is deterministic per-root via `collect_two_hop_reach`'s
    // BTreeMap.
    for (root, first_hops) in &adj {
        let reach = collect_two_hop_reach(root, first_hops, &adj);
        for (leaf, through) in reach {
            if through.len() < MIN_THROUGH {
                continue;
            }
            let key = (root.clone(), leaf.clone());
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);
            pairs.push(DiamondPair {
                root: root.clone(),
                leaf,
                through: through.into_iter().collect(),
            });
        }
    }
    pairs.sort_by_key(|p| std::cmp::Reverse(p.through.len()));
    let penalty_weight = cfg.thresholds.diamond.penalty_weight;
    let score = (100.0 - pairs.len() as f64 * penalty_weight).max(0.0);
    pairs.truncate(MAX_DETAIL_ITEMS);
    DiamondScore {
        score: score.round(),
        pairs,
    }
}

fn build_adj(dep: &DepGraph) -> BTreeMap<String, HashSet<String>> {
    let mut adj = BTreeMap::new();
    for (from, imports) in dep {
        let set: HashSet<String> = imports
            .iter()
            .filter(|to| !is_external(to))
            .cloned()
            .collect();
        adj.insert(from.clone(), set);
    }
    adj
}

/// For each node reachable in exactly two hops from `root` (excluding `root` itself and any first-hop
/// node), collects the set of first-hop nodes ("through") that reach it. Uses a BTreeSet for `through`
/// so the pair's `through` list has a deterministic order before the final sort.
fn collect_two_hop_reach(
    root: &str,
    first_hops: &HashSet<String>,
    adj: &BTreeMap<String, HashSet<String>>,
) -> BTreeMap<String, std::collections::BTreeSet<String>> {
    let mut reach: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for a in first_hops {
        let Some(second) = adj.get(a) else { continue };
        for z in second {
            if z == root || first_hops.contains(z) {
                continue;
            }
            reach.entry(z.clone()).or_default().insert(a.clone());
        }
    }
    reach
}

#[cfg(test)]
mod tests {
    //! Covers the empty-graph baseline, a single diamond's score penalty, sub-threshold path counts,
    //! external second-hop targets being ignored, and a first-hop node never counting as its own leaf —
    //! all against `ScoresConfig::default()`.
    use super::*;

    fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
        pairs
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    #[test]
    fn empty_graph_score_100_no_pairs() {
        let r = compute_diamond(&DepGraph::new(), &ScoresConfig::default());
        assert_eq!(r.score, 100.0);
        assert_eq!(r.pairs, vec![]);
    }

    #[test]
    fn single_diamond_root_through_ab_leaf_one_pair_score_98() {
        let d = dep(&[
            ("r/r.ts", &["a/a.ts", "b/b.ts"]),
            ("a/a.ts", &["leaf/l.ts"]),
            ("b/b.ts", &["leaf/l.ts"]),
            ("leaf/l.ts", &[]),
        ]);
        let r = compute_diamond(&d, &ScoresConfig::default());
        assert_eq!(r.pairs.len(), 1);
        assert_eq!(r.pairs[0].root, "r/r.ts");
        assert_eq!(r.pairs[0].leaf, "leaf/l.ts");
        let mut through = r.pairs[0].through.clone();
        through.sort();
        assert_eq!(through, vec!["a/a.ts".to_string(), "b/b.ts".to_string()]);
        assert_eq!(r.score, 98.0); // 100 - 1*2
    }

    #[test]
    fn only_one_path_to_leaf_through_less_than_2_no_diamond() {
        let d = dep(&[
            ("r/r.ts", &["a/a.ts", "b/b.ts"]),
            ("a/a.ts", &["leaf/l.ts"]),
            ("b/b.ts", &[]),
            ("leaf/l.ts", &[]),
        ]);
        let r = compute_diamond(&d, &ScoresConfig::default());
        assert_eq!(r.pairs, vec![]);
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn external_second_hop_targets_are_ignored() {
        let d = dep(&[
            ("r/r.ts", &["a/a.ts", "b/b.ts"]),
            ("a/a.ts", &["react"]),
            ("b/b.ts", &["react"]),
        ]);
        let r = compute_diamond(&d, &ScoresConfig::default());
        assert_eq!(r.pairs, vec![]);
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn a_first_hop_is_never_counted_as_its_own_leaf() {
        // root->[a,b], a->b : b is a firstHop, so excluded as leaf
        let d = dep(&[
            ("r/r.ts", &["a/a.ts", "b/b.ts"]),
            ("a/a.ts", &["b/b.ts"]),
            ("b/b.ts", &[]),
        ]);
        let r = compute_diamond(&d, &ScoresConfig::default());
        assert_eq!(r.pairs, vec![]);
        assert_eq!(r.score, 100.0);
    }
}
