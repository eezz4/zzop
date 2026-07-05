//! Aggregates cross-layer co-churn: commit co-changes between files in **different layers**.
//!
//! Measures how often architectural boundaries are disturbed together — a layer coupling debt signal.
//! `layer_of` is injected (path -> layer classification is the caller's responsibility); it always returns a layer
//! string (an unlayered file gets whatever sentinel the caller chooses, e.g. "(root)" — it is not "skipped", it
//! just forms its own layer, so pairs *within* that sentinel layer are naturally excluded by the same-layer check).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::coupling::{MAX_FILES_PER_COMMIT, MIN_FILES_PER_COMMIT};
use zpz_core::CommitFileSet;

/// `AnalyzeOutput::layer_co_churn`'s `layer_of` classifier: the top-level path segment is the layer (e.g.
/// `api/routes/x.ts` -> `"api"`), except a segment in `shared_dirs` — the FSD hierarchy shared/
/// cross-cutting-dir vocabulary (`ScoresConfig::hierarchy_shared_dirs`; the same set `scores::shared`'s
/// `is_upward_import` and `scores::sibling_cross`'s `compute_sibling_cross` already treat as "shared infra,
/// not a layer") — which folds into a `"(shared)"` sentinel instead of forming its own layer, so two
/// shared-dir files co-changing together is not counted as a cross-*architectural-boundary* churn. A file
/// with no folder (no `/`) folds into `"(root)"`, per `build_cross_layer_co_churn`'s own doc on unlayered
/// files forming their own excluded-from-crossing sentinel layer.
///
/// zpz has no dedicated layer/architecture vocabulary beyond this FSD set, so this is the minimal
/// defensible classifier — a future, denser layer taxonomy (explicit layer config, framework-specific
/// convention detection, ...) can replace this function without changing `build_cross_layer_co_churn`'s
/// own signature (it accepts any `Fn(&str) -> String`).
pub fn layer_of(path: &str, shared_dirs: &BTreeSet<String>) -> String {
    match path.split_once('/') {
        None => "(root)".to_string(),
        Some((first, _)) => {
            if shared_dirs.contains(first) {
                "(shared)".to_string()
            } else {
                first.to_string()
            }
        }
    }
}

/// Exclude layer pairs with fewer co-changes (removes one-off coincidences).
const MIN_CO_CHANGES: u32 = 2;
/// Max example file-pairs per layer pair.
const EXAMPLES_PER_PAIR: usize = 5;
/// Max layer pairs returned.
const TOP_PAIRS: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossLayerExample {
    pub a: String,
    pub b: String,
    /// Co-change count for this file pair.
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossLayerCoChurn {
    /// Layer pair, sorted alphabetically.
    pub layer_a: String,
    pub layer_b: String,
    /// Distinct file pairs co-changed in this layer pair.
    pub pairs: u32,
    /// Total co-change count (commit-weighted).
    pub co_changes: u32,
    /// Representative file pairs, sorted by co_changes desc.
    pub examples: Vec<CrossLayerExample>,
}

/// All fields default to the constants above (`MAX_FILES_PER_COMMIT`/`MIN_CO_CHANGES`/`EXAMPLES_PER_PAIR`/
/// `TOP_PAIRS`) when omitted — an options struct with `Option` fields, since Rust has no optional
/// function arguments.
#[derive(Debug, Clone, Default)]
pub struct CrossLayerCoChurnOptions {
    /// Skip commits with more than this many files (large-refactor noise suppression).
    pub max_files_per_commit: Option<usize>,
    /// Max layer pairs to return.
    pub top_pairs: Option<usize>,
    /// Max examples per layer pair.
    pub examples_per_pair: Option<usize>,
    /// Exclude layer pairs with fewer co-changes.
    pub min_co_changes: Option<u32>,
}

pub fn build_cross_layer_co_churn<F>(
    commits: &[CommitFileSet],
    layer_of: F,
    opts: &CrossLayerCoChurnOptions,
) -> Vec<CrossLayerCoChurn>
where
    F: Fn(&str) -> String,
{
    let max_files = opts.max_files_per_commit.unwrap_or(MAX_FILES_PER_COMMIT);
    let min_co_changes = opts.min_co_changes.unwrap_or(MIN_CO_CHANGES);
    let examples_per_pair = opts.examples_per_pair.unwrap_or(EXAMPLES_PER_PAIR);
    let top_pairs = opts.top_pairs.unwrap_or(TOP_PAIRS);

    let mut agg: BTreeMap<(String, String), LayerAgg> = BTreeMap::new();
    for c in commits {
        if c.files.len() < MIN_FILES_PER_COMMIT || c.files.len() > max_files {
            continue;
        }
        for i in 0..c.files.len() {
            for j in (i + 1)..c.files.len() {
                record_pair(&mut agg, &c.files[i], &c.files[j], &layer_of);
            }
        }
    }
    finalize(agg, min_co_changes, examples_per_pair, top_pairs)
}

struct LayerAgg {
    layer_a: String,
    layer_b: String,
    co_changes: u32,
    file_pairs: BTreeMap<(String, String), CrossLayerExample>,
}

fn record_pair<F>(
    agg: &mut BTreeMap<(String, String), LayerAgg>,
    file_a: &str,
    file_b: &str,
    layer_of: &F,
) where
    F: Fn(&str) -> String,
{
    let la = layer_of(file_a);
    let lb = layer_of(file_b);
    if la == lb {
        return;
    }
    let (layer_a, layer_b) = if la < lb { (la, lb) } else { (lb, la) };
    let (a, b) = if file_a < file_b {
        (file_a.to_string(), file_b.to_string())
    } else {
        (file_b.to_string(), file_a.to_string())
    };

    let entry = agg
        .entry((layer_a.clone(), layer_b.clone()))
        .or_insert_with(|| LayerAgg {
            layer_a,
            layer_b,
            co_changes: 0,
            file_pairs: BTreeMap::new(),
        });
    entry.co_changes += 1;
    let fp = entry
        .file_pairs
        .entry((a.clone(), b.clone()))
        .or_insert_with(|| CrossLayerExample { a, b, count: 0 });
    fp.count += 1;
}

fn finalize(
    agg: BTreeMap<(String, String), LayerAgg>,
    min_co_changes: u32,
    examples_per_pair: usize,
    top_pairs: usize,
) -> Vec<CrossLayerCoChurn> {
    let mut out: Vec<CrossLayerCoChurn> = Vec::new();
    for (_, e) in agg {
        if e.co_changes < min_co_changes {
            continue;
        }
        let pairs = e.file_pairs.len() as u32;
        let mut examples: Vec<CrossLayerExample> = e.file_pairs.into_values().collect();
        examples.sort_by_key(|e| std::cmp::Reverse(e.count));
        examples.truncate(examples_per_pair);
        out.push(CrossLayerCoChurn {
            layer_a: e.layer_a,
            layer_b: e.layer_b,
            pairs,
            co_changes: e.co_changes,
            examples,
        });
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.co_changes));
    out.truncate(top_pairs);
    out
}

#[cfg(test)]
mod tests {
    //! Exercises cross-layer co-churn aggregation.
    use super::*;

    /// Local slash-prefix layer fixture for `build_cross_layer_co_churn` tests below — distinct from the
    /// real `layer_of` (this module's function under test in the "--- layer_of ---" section further down),
    /// which additionally takes a `shared_dirs` set.
    fn fixture_layer_of(p: &str) -> String {
        match p.find('/') {
            Some(i) => p[..i].to_string(),
            None => "(root)".to_string(),
        }
    }

    fn commit(sha: &str, files: &[&str]) -> CommitFileSet {
        CommitFileSet {
            sha: sha.to_string(),
            files: files.iter().map(|s| s.to_string()).collect(),
            tags: vec![],
            date: None,
        }
    }

    // --- layer_of ---

    fn shared_dirs() -> BTreeSet<String> {
        ["utils", "types"].into_iter().map(String::from).collect()
    }

    #[test]
    fn layer_of_uses_the_top_level_path_segment() {
        assert_eq!(layer_of("api/routes/x.ts", &shared_dirs()), "api");
        assert_eq!(layer_of("domains/y.ts", &shared_dirs()), "domains");
    }

    #[test]
    fn layer_of_folds_shared_dirs_into_a_sentinel() {
        assert_eq!(layer_of("utils/format.ts", &shared_dirs()), "(shared)");
        assert_eq!(layer_of("types/index.ts", &shared_dirs()), "(shared)");
    }

    #[test]
    fn layer_of_folds_root_level_files_into_a_sentinel() {
        assert_eq!(layer_of("App.tsx", &shared_dirs()), "(root)");
    }

    #[test]
    fn only_cross_layer_pairs_are_counted() {
        let commits = vec![
            commit("a", &["api/x.ts", "domains/y.ts"]), // cross: api<->domains
            commit("b", &["api/x.ts", "api/z.ts"]),     // same layer -> excluded
            commit("c", &["api/x.ts", "domains/y.ts"]), // cross recurrence (co-change 2)
        ];
        let out = build_cross_layer_co_churn(
            &commits,
            fixture_layer_of,
            &CrossLayerCoChurnOptions {
                min_co_changes: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].layer_a, "api");
        assert_eq!(out[0].layer_b, "domains");
        assert_eq!(out[0].co_changes, 2);
        assert_eq!(out[0].pairs, 1);
        assert_eq!(
            out[0].examples[0],
            CrossLayerExample {
                a: "api/x.ts".to_string(),
                b: "domains/y.ts".to_string(),
                count: 2
            }
        );
    }

    #[test]
    fn layer_pairs_below_min_co_changes_are_excluded() {
        let commits = vec![commit("a", &["api/x.ts", "lib/u.ts"])]; // co-change 1
        assert_eq!(
            build_cross_layer_co_churn(
                &commits,
                fixture_layer_of,
                &CrossLayerCoChurnOptions {
                    min_co_changes: Some(2),
                    ..Default::default()
                }
            )
            .len(),
            0
        );
        assert_eq!(
            build_cross_layer_co_churn(
                &commits,
                fixture_layer_of,
                &CrossLayerCoChurnOptions {
                    min_co_changes: Some(1),
                    ..Default::default()
                }
            )
            .len(),
            1
        );
    }

    #[test]
    fn commits_exceeding_max_files_per_commit_are_skipped() {
        let mut files: Vec<String> = (0..30).map(|i| format!("api/f{i}.ts")).collect();
        files.push("domains/y.ts".to_string());
        let file_refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
        let big = commit("big", &file_refs);
        let out = build_cross_layer_co_churn(
            &[big],
            fixture_layer_of,
            &CrossLayerCoChurnOptions {
                max_files_per_commit: Some(25),
                min_co_changes: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn sorted_descending_by_co_changes_plus_top_pairs_cap() {
        let commits = vec![
            commit("a", &["api/x.ts", "domains/y.ts"]),
            commit("b", &["api/x.ts", "domains/y.ts"]),
            commit("c", &["lib/u.ts", "ui/v.ts"]),
        ];
        let out = build_cross_layer_co_churn(
            &commits,
            fixture_layer_of,
            &CrossLayerCoChurnOptions {
                min_co_changes: Some(1),
                top_pairs: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].co_changes, 2); // api<->domains ranks above lib<->ui(1)
    }
}
