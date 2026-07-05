//! Main Sequence — module abstractness + instability distance from the main sequence (Robert Martin).
//! A module sits "on the main sequence" when abstractness + instability ≈ 1: an unstable (many outgoing,
//! few incoming deps) module should be concrete, a stable (many incoming deps) module should be abstract.
//! `distance = |A + I - 1|` measures deviation; the score is 100 minus the file-count-weighted average
//! distance across modules.

use std::collections::{BTreeMap, HashSet};

use super::config::ScoresConfig;
use super::shared::{is_external, module_of};
use super::types::{FileKind, FileKinds, MainSequenceScore, ModuleMainSeq};
use zpz_core::DepGraph;

/// Per-module accumulator before ratios are derived.
struct Acc {
    files: HashSet<String>,
    abstract_count: u32,
    in_deg: u32,
    out_deg: u32,
}

impl Acc {
    fn new() -> Self {
        Acc {
            files: HashSet::new(),
            abstract_count: 0,
            in_deg: 0,
            out_deg: 0,
        }
    }
}

pub fn compute_main_sequence(
    dep: &DepGraph,
    kinds: &FileKinds,
    cfg: &ScoresConfig,
) -> MainSequenceScore {
    let mut acc: BTreeMap<String, Acc> = BTreeMap::new();
    for f in dep.keys() {
        let Some(m) = module_of(cfg, f) else { continue };
        let s = acc.entry(m).or_insert_with(Acc::new);
        s.files.insert(f.clone());
        if matches!(kinds.get(f), Some(FileKind::Abstract)) {
            s.abstract_count += 1;
        }
    }
    for (from, imports) in dep {
        let fm = module_of(cfg, from);
        for to in imports {
            if is_external(to) {
                continue;
            }
            let tm = module_of(cfg, to);
            let (Some(fm), Some(tm)) = (fm.clone(), tm) else {
                continue;
            };
            if fm == tm {
                continue;
            }
            acc.entry(fm).or_insert_with(Acc::new).out_deg += 1;
            acc.entry(tm).or_insert_with(Acc::new).in_deg += 1;
        }
    }

    let mut modules = to_modules(acc);
    modules.sort_by(|a, b| {
        b.distance
            .partial_cmp(&a.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if modules.is_empty() {
        return MainSequenceScore {
            score: 100.0,
            avg_distance: 0.0,
            modules: vec![],
        };
    }
    let total_files: usize = modules.iter().map(|m| m.file_count).sum();
    let weighted_d: f64 = modules
        .iter()
        .map(|m| m.distance * m.file_count as f64)
        .sum::<f64>()
        / total_files as f64;
    MainSequenceScore {
        score: ((1.0 - weighted_d) * 100.0).round(),
        avg_distance: (weighted_d * 100.0).round() / 100.0,
        modules,
    }
}

fn to_modules(acc: BTreeMap<String, Acc>) -> Vec<ModuleMainSeq> {
    let mut modules = Vec::new();
    for (module, s) in acc {
        if s.files.is_empty() {
            continue;
        }
        let abstractness = s.abstract_count as f64 / s.files.len() as f64;
        let instability = if s.in_deg + s.out_deg == 0 {
            0.0
        } else {
            s.out_deg as f64 / (s.in_deg + s.out_deg) as f64
        };
        let distance = (abstractness + instability - 1.0).abs();
        modules.push(ModuleMainSeq {
            module,
            file_count: s.files.len(),
            abstractness: (abstractness * 100.0).round() / 100.0,
            instability: (instability * 100.0).round() / 100.0,
            distance: (distance * 100.0).round() / 100.0,
        });
    }
    modules
}

#[cfg(test)]
mod tests {
    //! Covers the empty-graph baseline, weighted distance across two modules with no abstract files,
    //! mutual cross-module edges with one abstract file, and a single module with only internal edges
    //! (which contribute no in/out degree).
    use super::*;

    fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
        pairs
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    #[test]
    fn empty_graph_score_100_no_modules() {
        let r = compute_main_sequence(
            &DepGraph::new(),
            &FileKinds::new(),
            &ScoresConfig::default(),
        );
        assert_eq!(r.score, 100.0);
        assert_eq!(r.avg_distance, 0.0);
        assert_eq!(r.modules, vec![]);
    }

    #[test]
    fn two_modules_no_abstract_files_weighted_distance_half_score_50() {
        // a: instability 1 (out only), abstractness 0 -> D = |0+1-1| = 0
        // b: instability 0 (in only),  abstractness 0 -> D = |0+0-1| = 1
        // weighted = (0*1 + 1*1)/2 = 0.5
        let d = dep(&[("a/x.ts", &["b/y.ts"]), ("b/y.ts", &[])]);
        let r = compute_main_sequence(&d, &FileKinds::new(), &ScoresConfig::default());
        assert_eq!(r.avg_distance, 0.5);
        assert_eq!(r.score, 50.0);
        let a = r.modules.iter().find(|m| m.module == "a").unwrap();
        let b = r.modules.iter().find(|m| m.module == "b").unwrap();
        assert_eq!(a.file_count, 1);
        assert_eq!(a.abstractness, 0.0);
        assert_eq!(a.instability, 1.0);
        assert_eq!(a.distance, 0.0);
        assert_eq!(b.file_count, 1);
        assert_eq!(b.abstractness, 0.0);
        assert_eq!(b.instability, 0.0);
        assert_eq!(b.distance, 1.0);
    }

    #[test]
    fn mutual_edges_with_one_abstract_file_both_distance_half_score_50() {
        // a<->b: each in1 out1 -> instability 0.5
        // a abstract 1/1=1 -> D = |1+0.5-1| = 0.5
        // b abstract 0     -> D = |0+0.5-1| = 0.5
        let d = dep(&[("a/x.ts", &["b/y.ts"]), ("b/y.ts", &["a/x.ts"])]);
        let mut kinds = FileKinds::new();
        kinds.insert("a/x.ts".to_string(), FileKind::Abstract);
        let r = compute_main_sequence(&d, &kinds, &ScoresConfig::default());
        assert_eq!(r.avg_distance, 0.5);
        assert_eq!(r.score, 50.0);
        let a = r.modules.iter().find(|m| m.module == "a").unwrap();
        assert_eq!(a.abstractness, 1.0);
        assert_eq!(a.instability, 0.5);
        assert_eq!(a.distance, 0.5);
    }

    #[test]
    fn single_module_only_internal_edges_no_cross_degree_distance_1_score_0() {
        // intra-module imports are skipped, so in=out=0 -> instability 0
        // abstractness 0 -> D = |0+0-1| = 1, weighted over 2 files = 1
        let d = dep(&[("a/x.ts", &["a/y.ts"]), ("a/y.ts", &[])]);
        let r = compute_main_sequence(&d, &FileKinds::new(), &ScoresConfig::default());
        assert_eq!(r.modules.len(), 1);
        assert_eq!(r.modules[0].module, "a");
        assert_eq!(r.modules[0].file_count, 2);
        assert_eq!(r.modules[0].instability, 0.0);
        assert_eq!(r.modules[0].distance, 1.0);
        assert_eq!(r.avg_distance, 1.0);
        assert_eq!(r.score, 0.0);
    }
}
