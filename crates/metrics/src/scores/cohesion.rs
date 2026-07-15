//! Slice cohesion — weighted average of internal / (internal + outgoing) per FSD slice. A slice that mostly
//! imports its own files (`cohesion` near 1) is a clean, extractable unit; one that leans heavily on siblings is
//! not. The overall score is the per-slice cohesion weighted by file count, so a large entangled slice drags the
//! score down more than a tiny one.

use std::collections::{BTreeMap, HashSet};

use super::config::ScoresConfig;
use super::shared::{classify_path, is_external, round};
use super::types::{CohesionScore, SliceCohesion};
use zzop_core::DepGraph;

#[derive(Default)]
struct Acc {
    files: HashSet<String>,
    internal: u32,
    out: u32,
    incoming: u32,
}

pub fn compute_cohesion(dep: &DepGraph, cfg: &ScoresConfig) -> CohesionScore {
    // BTreeMap gives deterministic traversal order over a HashMap-backed DepGraph, independent of hasher-dependent
    // iteration order; final ordering of the output is decided explicitly below via the cohesion sort.
    let mut byslice: BTreeMap<String, Acc> = BTreeMap::new();

    let mut files: Vec<&String> = dep.keys().collect();
    files.sort();
    for f in &files {
        let info = classify_path(cfg, f);
        if let Some(slice) = info.slice {
            byslice.entry(slice).or_default().files.insert((*f).clone());
        }
    }

    for from in &files {
        let fi = classify_path(cfg, from);
        for to in &dep[*from] {
            if is_external(to) {
                continue;
            }
            let ti = classify_path(cfg, to);
            if fi.slice.is_some() && ti.slice.is_some() && fi.slice == ti.slice {
                byslice
                    .entry(fi.slice.clone().unwrap())
                    .or_default()
                    .internal += 1;
            } else if let Some(fs) = &fi.slice {
                byslice.entry(fs.clone()).or_default().out += 1;
            }
            if let Some(ts) = &ti.slice {
                if ti.slice != fi.slice {
                    byslice.entry(ts.clone()).or_default().incoming += 1;
                }
            }
        }
    }

    let mut slices = to_slices(byslice);
    // Sorted ascending by cohesion, with a tie-break on slice name for determinism since HashMap-backed
    // traversal above has no defined iteration order.
    slices.sort_by(|a, b| {
        a.cohesion
            .partial_cmp(&b.cohesion)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.slice.cmp(&b.slice))
    });

    CohesionScore {
        score: round(weighted_cohesion(&slices)),
        slices,
    }
}

fn to_slices(byslice: BTreeMap<String, Acc>) -> Vec<SliceCohesion> {
    byslice
        .into_iter()
        .map(|(slice, s)| {
            let cohesion = if s.internal + s.out == 0 {
                1.0
            } else {
                f64::from(s.internal) / f64::from(s.internal + s.out)
            };
            let instability = if s.incoming + s.out == 0 {
                0.0
            } else {
                f64::from(s.out) / f64::from(s.incoming + s.out)
            };
            SliceCohesion {
                slice,
                file_count: s.files.len(),
                internal_edges: s.internal,
                outgoing_edges: s.out,
                incoming_edges: s.incoming,
                cohesion: round_two(cohesion),
                instability: round_two(instability),
            }
        })
        .collect()
}

fn round_two(n: f64) -> f64 {
    round(n * 100.0) / 100.0
}

fn weighted_cohesion(slices: &[SliceCohesion]) -> f64 {
    if slices.is_empty() {
        return 100.0;
    }
    let sum: f64 = slices
        .iter()
        .map(|s| s.cohesion * s.file_count as f64)
        .sum();
    let files: usize = slices.iter().map(|s| s.file_count).sum();
    (sum / files as f64) * 100.0
}

#[cfg(test)]
mod tests {
    //! Covers empty/non-slice graphs, a single slice with only internal edges, weighted cohesion across two
    //! cross-referencing slices, and external imports being excluded from edge counts.
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
    fn empty_graph_score_100_no_slices() {
        let r = compute_cohesion(&DepGraph::new(), &cfg());
        assert_eq!(r.score, 100.0);
        assert!(r.slices.is_empty());
    }

    #[test]
    fn non_slice_files_only_no_slices_score_100() {
        // utils/* is L3 shared (no slice), so nothing is tracked.
        let d = dep(&[("utils/a.ts", &["utils/b.ts"]), ("utils/b.ts", &[])]);
        let r = compute_cohesion(&d, &cfg());
        assert!(r.slices.is_empty());
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn a_slice_with_only_internal_edges_cohesion_1_score_100() {
        let d = dep(&[
            ("features/auth/login.ts", &["features/auth/util.ts"]),
            ("features/auth/util.ts", &[]),
        ]);
        let r = compute_cohesion(&d, &cfg());
        assert_eq!(r.slices.len(), 1);
        let auth = &r.slices[0];
        assert_eq!(auth.slice, "features/auth");
        assert_eq!(auth.file_count, 2);
        assert_eq!(auth.internal_edges, 1);
        assert_eq!(auth.outgoing_edges, 0);
        assert_eq!(auth.cohesion, 1.0);
        // weighted: (1*2)/2 *100 = 100
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn two_slices_with_cross_edges_weighted_cohesion() {
        let d = dep(&[
            (
                "features/auth/login.ts",
                &["features/auth/util.ts", "features/cart/cart.ts"],
            ),
            ("features/auth/util.ts", &[]),
            ("features/cart/cart.ts", &["features/auth/login.ts"]),
        ]);
        let r = compute_cohesion(&d, &cfg());
        // sorted by cohesion asc: cart(0) then auth(0.5)
        let slice_order: Vec<&str> = r.slices.iter().map(|s| s.slice.as_str()).collect();
        assert_eq!(slice_order, vec!["features/cart", "features/auth"]);
        let cart = r
            .slices
            .iter()
            .find(|s| s.slice == "features/cart")
            .unwrap();
        let auth = r
            .slices
            .iter()
            .find(|s| s.slice == "features/auth")
            .unwrap();
        assert_eq!(auth.file_count, 2);
        assert_eq!(auth.internal_edges, 1);
        assert_eq!(auth.outgoing_edges, 1);
        assert_eq!(auth.incoming_edges, 1);
        assert_eq!(auth.cohesion, 0.5);
        assert_eq!(auth.instability, 0.5);
        assert_eq!(cart.file_count, 1);
        assert_eq!(cart.internal_edges, 0);
        assert_eq!(cart.outgoing_edges, 1);
        assert_eq!(cart.incoming_edges, 1);
        assert_eq!(cart.cohesion, 0.0);
        assert_eq!(cart.instability, 0.5);
        // weighted: (0.5*2 + 0*1)/3 *100 = 33.33 -> 33
        assert_eq!(r.score, 33.0);
    }

    #[test]
    fn external_imports_are_ignored() {
        let d = dep(&[
            (
                "features/auth/login.ts",
                &["react", "@scope/pkg", "features/auth/util.ts"],
            ),
            ("features/auth/util.ts", &[]),
        ]);
        let r = compute_cohesion(&d, &cfg());
        let auth = &r.slices[0];
        assert_eq!(auth.internal_edges, 1);
        assert_eq!(auth.outgoing_edges, 0);
        assert_eq!(r.score, 100.0);
    }
}
