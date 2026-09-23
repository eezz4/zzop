//! What one file contributes to the DEPENDENCY GRAPH, and the six-plus-one substrates that carry it.
//!
//! ## Why this is its own module
//! `collect`'s loop buckets an artifact's fields into per-tree substrates, and most of those buckets
//! are independent of each other — an `io` fact and a `loc` count never meet. These seven do meet:
//! they are all gated on the SAME question (does this file participate in the dep graph, i.e. did the
//! per-file pass give it an `ImportMap`), they are all keyed by the same `rel`, and every one of them
//! is read by the dep-graph builder or by a pass that walks its node set. Splitting them out of
//! `collect.rs` at the 300-line cap therefore cut along a seam that was already there rather than at a
//! line number.
//!
//! ## The gate is `imports.is_some()`, not a language
//! A file is in the slot when the per-file pass handed it an `ImportMap` — which includes an EMPTY one
//! (`pipeline::fresh::ts_slot`'s note owns why: an empty map still buys a dep-graph node). Mode B
//! overlays reach the same state by a different route, through `envelope::merge`. So "TS slot" names
//! the channel, not the extension, and the `ts_` prefixes on these fields are historical.

use std::collections::{HashMap, HashSet};

use zzop_core::{ImportMap, ReExport};

use crate::pipeline::FileArtifact;

/// The dep-graph substrates, accumulated across the walk. Field names carry their `ts_` prefix because
/// that is what `Collected` calls them and a rename here would only move the mismatch.
#[derive(Default)]
pub(super) struct DepSlot {
    pub(super) ts_paths: HashSet<String>,
    pub(super) ts_import_pairs: Vec<(String, ImportMap)>,
    pub(super) ts_re_export_pairs: Vec<(String, Vec<ReExport>)>,
    pub(super) ts_dynamic_import_pairs: Vec<(String, Vec<String>)>,
    pub(super) ts_asset_ref_pairs: Vec<(String, Vec<String>)>,
    pub(super) ts_call_graph_pairs: Vec<(String, zzop_core::callgraph::CallGraphFacts)>,
    pub(super) dead_export_names_by_file: HashMap<String, crate::dead_exports::DeadExportNames>,
}

impl DepSlot {
    /// Folds one participating file in. `imports` is passed separately because the caller has already
    /// destructured it out of the artifact to decide that this file participates at all.
    ///
    /// The empty-check before each `push` is not an optimization detail: a pair list is walked in full
    /// by its consumer, so an empty entry is work with no possible effect, and `ts_call_graph_pairs`
    /// follows the same rule as its five older siblings rather than inventing one.
    pub(super) fn absorb(&mut self, artifact: &mut FileArtifact, imports: ImportMap) {
        // Cloned BEFORE the pushes below move them out: these two are dep-graph substrate AND two of
        // `unimported-export`'s four per-file inputs, and until 2026-09-08 that rule re-derived them
        // with its own parse rather than reading the copy that already existed (review ledger V110).
        let re_exports = artifact.re_exports.clone();
        let dynamic_imports = artifact.dynamic_imports.clone();
        let rel = artifact.rel.as_str();
        self.ts_paths.insert(rel.to_string());
        if !artifact.re_exports.is_empty() {
            self.ts_re_export_pairs
                .push((rel.to_string(), std::mem::take(&mut artifact.re_exports)));
        }
        if !artifact.dynamic_imports.is_empty() {
            self.ts_dynamic_import_pairs.push((
                rel.to_string(),
                std::mem::take(&mut artifact.dynamic_imports),
            ));
        }
        if !artifact.asset_refs.is_empty() {
            self.ts_asset_ref_pairs
                .push((rel.to_string(), std::mem::take(&mut artifact.asset_refs)));
        }
        if !artifact.call_graph.is_empty() {
            self.ts_call_graph_pairs
                .push((rel.to_string(), std::mem::take(&mut artifact.call_graph)));
        }
        self.ts_import_pairs.push((rel.to_string(), imports));
        self.dead_export_names_by_file.insert(
            rel.to_string(),
            crate::dead_exports::DeadExportNames {
                used: std::mem::take(&mut artifact.used_names),
                signature: std::mem::take(&mut artifact.exported_signature_names),
                re_exports,
                dynamic_imports,
                export_aliases: std::mem::take(&mut artifact.export_aliases),
                is_generated: artifact.has_generated_banner,
            },
        );
    }
}
