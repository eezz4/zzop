//! The tree-wide class/interface shape MERGE — the shared substrate both assemble-time DTO
//! resolutions read: `body_refs` (`IoProvide::body.dto_ref`, `body-shape-v1`) and `response_refs`
//! (`IoProvide::response.dto_ref`, `response-shape-v1`). Built ONCE per tree from every file's
//! `zzop_core::ClassShapeFragment`s, so the two passes can never disagree on what a name resolves to.
//!
//! ## Merge (never guess)
//! Input pairs are scanned in sorted-file order for determinism (mirrors
//! `merge_const_map_fragments`'s rationale), folded into one `name -> ClassShapeFragment` map:
//! - A name declared identically (same `fields` + `complete`) in one file, or repeated identically
//!   across 2+ files, resolves normally.
//! - A name declared with CONFLICTING shapes (different `fields` or `complete`) across 2+ files is
//!   POISONED — it resolves to nothing for every ref naming it, and [`ShapeMerge::poisoned_disclosures`]
//!   builds ONE aggregated warning per referenced poisoned name (naming the class/interface and every
//!   declaring file) rather than guessing which declaration is authoritative. TypeScript's legitimate
//!   cross-file interface declaration-merging lands here too: merging partial declarations would guess
//!   at a union this analysis never verified, so it is dropped and disclosed like any other conflict.

use std::collections::{BTreeMap, BTreeSet, HashSet};

/// See the module doc. Both consumers hold one instance built by [`ShapeMerge::build`] and query it
/// read-only; poisoning is checked FIRST at every resolution site (`get` deliberately does not fold
/// it in, so a caller can distinguish "poisoned" from "missing" for its own warning wording).
pub(crate) struct ShapeMerge {
    shapes_by_name: BTreeMap<String, zzop_core::ClassShapeFragment>,
    files_by_name: BTreeMap<String, BTreeSet<String>>,
    poisoned: HashSet<String>,
}

impl ShapeMerge {
    pub(crate) fn build(class_shapes: &[(String, Vec<zzop_core::ClassShapeFragment>)]) -> Self {
        let mut sorted: Vec<&(String, Vec<zzop_core::ClassShapeFragment>)> =
            class_shapes.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));

        let mut merge = ShapeMerge {
            shapes_by_name: BTreeMap::new(),
            files_by_name: BTreeMap::new(),
            poisoned: HashSet::new(),
        };
        for (file, frags) in sorted {
            for frag in frags {
                merge
                    .files_by_name
                    .entry(frag.name.clone())
                    .or_default()
                    .insert(file.clone());
                match merge.shapes_by_name.get(&frag.name) {
                    None => {
                        merge.shapes_by_name.insert(frag.name.clone(), frag.clone());
                    }
                    Some(existing) => {
                        if existing.fields != frag.fields || existing.complete != frag.complete {
                            merge.poisoned.insert(frag.name.clone());
                        }
                    }
                }
            }
        }
        merge
    }

    pub(crate) fn is_poisoned(&self, name: &str) -> bool {
        self.poisoned.contains(name)
    }

    /// The merged shape for `name`, poisoning NOT folded in — check [`Self::is_poisoned`] first.
    pub(crate) fn get(&self, name: &str) -> Option<&zzop_core::ClassShapeFragment> {
        self.shapes_by_name.get(name)
    }

    /// One aggregated conflicting-shape warning per poisoned name in `referenced`, sorted by name.
    /// Only names some ref actually references are disclosed: fragments cover EVERY class/interface
    /// declaration, so same-name/different-shape non-DTO types (`Config`, `Options`, React `Props`)
    /// are common and legitimate — warning on an unreferenced collision would disclose a drop that
    /// never happened (a phantom disclosure, the same stance `unmatched_suppression_warnings`
    /// codifies). `what` names the dropping pass ("request-body" / "declared-response") so a reader
    /// knows which contract was lost.
    pub(crate) fn poisoned_disclosures(
        &self,
        referenced: &HashSet<&str>,
        what: &str,
    ) -> Vec<String> {
        let mut names: Vec<&String> = self
            .poisoned
            .iter()
            .filter(|n| referenced.contains(n.as_str()))
            .collect();
        names.sort();
        names
            .into_iter()
            .map(|name| {
                let files: Vec<&str> = self.files_by_name[name]
                    .iter()
                    .map(String::as_str)
                    .collect();
                // TS declaration-merging makes a SAME-FILE conflict legal input, so the phrase
                // must match the file set: "in <file>" for one file, "across N files" for more.
                let where_clause = if files.len() == 1 {
                    format!("in {}", files[0])
                } else {
                    format!("across {} files ({})", files.len(), files.join(", "))
                };
                format!(
                    "class/interface `{name}` is declared with conflicting field shapes \
                     {where_clause} — {what} resolution for `{name}` is dropped, never guessed"
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
