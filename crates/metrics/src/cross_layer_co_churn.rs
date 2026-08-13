//! Aggregates cross-layer co-churn: commit co-changes between files in **different layers**.
//!
//! Measures how often architectural boundaries are disturbed together — a layer coupling debt signal.
//! `layer_of` is injected (path -> layer classification is the caller's responsibility); it always returns a layer
//! string (an unlayered file gets whatever sentinel the caller chooses, e.g. "(root)" — it is not "skipped", it
//! just forms its own layer, so pairs *within* that sentinel layer are naturally excluded by the same-layer check).
//!
//! ## The counts are over a FILTERED commit set
//! Every number this module emits — `pairs`, `co_changes`, each example's `count` — is counted over the
//! commits that survive [`crate::coupling::MIN_FILES_PER_COMMIT`]..=`max_files_per_commit` (2..=25 by
//! default, the same window [`crate::coupling`] uses so the two co-change substrates cannot disagree
//! about which commits are real). A 1-file commit couples nothing, and a 200-file mass rename would
//! couple everything with everything; both are dropped, so what is left is *deliberate* co-change. That
//! makes these SUBSET totals: the honest reading of `coChanges: 40` is "40 filtered co-changes", never
//! "these two layers changed together 40 times". Pairs below `min_co_changes` (2) are dropped again at
//! the end, and only the top `top_pairs` (20) rows are returned. `docs/modules/facade.md`'s
//! `layerCoChurn` row carries the same sentence for the consumer, since this field reaches no shaped
//! CLI/MCP reply and the raw facade contract is where its reader looks.
//!
//! ## An empty result and a wrong result have DIFFERENT causes, and both are the caller's
//!
//! This module was investigated on 2026-08-13 against a backlog claim that it is *structurally always
//! empty*. **It is not** — measured over the 17-repo corpus (`corpus/oss/zzop.config.jsonc`), the one
//! tree whose history produced a usable commit set produced layer pairs, and the rest produced none
//! for a reason that lives entirely outside this file: those checkouts are `--depth=1` clones, so
//! there is a single commit and no co-change substrate at all. That state is already disclosed —
//! `zzop_metrics::build_diagnostics` emits the thin-history warning naming the shallow clone and its
//! `git fetch --unshallow` remedy — so the reader who receives `[]` there is not being told a clean
//! story about a dark channel. The disclosure is threshold-based (it fires at one commit), which
//! leaves a narrower unlit case: a repo with several commits, none of which lands inside the
//! [`MIN_FILES_PER_COMMIT`]..=`max_files_per_commit` window, also yields `[]` with nothing said.
//!
//! The sharper hazard ran the other way and made a NON-empty result the dangerous one. Every path this
//! module sees arrives in `commits`, which is collected per GIT REPOSITORY and keyed by the repo root —
//! while the `nodes`/`folders`/`dep` paths the same reply publishes are relative to the analyzed TREE
//! root. Same-directory roots coincide; a tree that is a SUBDIRECTORY of its repository — exactly the
//! monorepo shape `zzop cross ./fe ./be` is built for — did not, and this module cannot tell the
//! difference, because a repo-relative path is still a well-formed path that `layer_of` classifies
//! happily. Measured on this repository's own subtrees the same day: not one co-change edge had both
//! endpoints among the tree's nodes, and every tree of a run received an identical layer-pair list —
//! the enclosing repository's, not its own.
//!
//! **That is fixed upstream, not here, and it stays that way.** The git-collection lane now rebases each
//! tree's copy of the shared collection onto the tree root before handing it over
//! (`zzop_git::GitCollection::rebase_onto`, called from `zzop_engine`'s `collect_git`), so `commits`
//! arrives in the coordinate system the rest of the reply speaks. This module is still handed `commits`
//! and a classifier and still joins them faithfully — it has no way to validate a path and must not grow
//! one, since a second opinion about coordinates is how two answers start disagreeing. What this note
//! owes the next reader is the shape of the remaining risk: `[]` here means "measured, nothing crossed a
//! layer", and it is the thin-history case above, not a dark channel.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::coupling::{MAX_FILES_PER_COMMIT, MIN_FILES_PER_COMMIT};
use zzop_core::CommitFileSet;

/// `AnalyzeOutput::layer_co_churn`'s `layer_of` classifier: the top-level path segment is the layer (e.g.
/// `api/routes/x.ts` -> `"api"`), except a segment in `shared_dirs` — the FSD hierarchy shared/
/// cross-cutting-dir vocabulary (`ScoresConfig::hierarchy_shared_dirs`; the same set `scores::shared`'s
/// `is_upward_import` and `scores::sibling_cross`'s `compute_sibling_cross` already treat as "shared infra,
/// not a layer") — which folds into a `"(shared)"` sentinel instead of forming its own layer, so two
/// shared-dir files co-changing together is not counted as a cross-*architectural-boundary* churn. A file
/// with no folder (no `/`) folds into `"(root)"`, per `build_cross_layer_co_churn`'s own doc on unlayered
/// files forming their own excluded-from-crossing sentinel layer.
///
/// zzop has no dedicated layer/architecture vocabulary beyond this FSD set, so this is the minimal
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
    /// Commit-weighted co-change count over the commits this metric JUDGES, which is not every commit
    /// in the window — see [`build_cross_layer_co_churn`] for the filter. A subset total, never a
    /// repository total, and it was documented as "Total co-change count" until 2026-07-31, which is
    /// the completeness the word "total" implied and the number never had.
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
mod tests;
