//! The noncycle-exclusion accumulator both dep-graph lanes share — the ONE place that decides whether
//! a `(from, to)` file pair carries a runtime module-load edge, and therefore whether
//! [`crate::circular_from_dep_excluding`] may skip it.
//!
//! ## Why an accumulator instead of a folded `bool`
//!
//! Both producers used to fold each edge to `target -> bool` ("every binding so far is erasable") the
//! moment they saw it. That threw away the IMPORTED NAME, and the export-side half of the question —
//! *is the declaration `X` resolves to in the target file an `export type` / `export interface`?* —
//! cannot be asked without it. Worse, the envelope lane streams files one at a time, so at the moment
//! it sees `a.ts` importing `X` from `b.ts` the symbols of `b.ts` may not have arrived yet. Keeping the
//! names and folding ONCE, after every file is in, is what makes the export-side arm expressible at
//! all — and folding through one shared [`NoncycleCandidates::refine`] is what makes the two lanes
//! structurally unable to drift apart.
//!
//! ## The two arms of "erasable", and why absence is never one of them
//!
//! An edge contributed by one binding is erasable when EITHER
//! * the IMPORT says so — `import type { X }` / `{ type X }` / `export type {X} from`, or a dynamic
//!   `import()` (async: never a synchronous module-load cycle); the producer passes `erased: true`, or
//! * the EXPORT says so — every declaration of that name in the target file is a `type`/`interface`.
//!
//! Both are DECLARATIONS in scanned source, which is what this engine requires of any evidence that
//! DELETES a finding: a name or a usage pattern may RAISE one, since a reader then judges it, but a
//! finding removed on a guess leaves no trace for anyone to notice. Usage inference ("it is only ever used in type position") is not admissible and is not
//! computed here.
//!
//! An edge pair is excluded only when EVERY binding contributing it is erasable. One value binding to
//! the same target means a real runtime edge exists.
//!
//! **Absence of a symbol is never evidence.** A TypeScript `enum` survives to runtime yet projects no
//! symbol at all (`parser/parser-typescript/src/symbols.rs` drops `Decl::TsEnum`), and a degraded
//! (lexical-fallback) file projects none either. Reading either silence as "it must be a type" would
//! delete real cycles and leave no trace, so the export-side arm fires only on a symbol it actually
//! found.

use std::collections::{BTreeMap, HashSet};

use crate::ir::{SourceSymbol, SourceSymbolKind};

/// The ONE sentence every surface that publishes BOTH a dep-graph edge and a cycle verdict must show,
/// and its only owner — the companion to [`crate::DEP_GRAPH_RESOLVED_ONLY`], which says what the graph
/// leaves OUT and could not say this.
///
/// # Why it has to be said at all
/// This module deletes edges from the CYCLE question and from nothing else. `ir.dep` keeps them, because
/// the import is real source text and a dep graph that quietly dropped it would misreport fan-in,
/// blast radius and `queryFile`'s dependencies for a defensible reason nobody could see. Both halves
/// are right; the pair is what needs disclosing, because a consumer holding both reads them as one
/// picture and they are not one picture.
///
/// The measured instance (2026-08-31, `corpus/oss/fe-axios`): `--format cosmograph-links` emits BOTH
/// `src/components/App/App.slice.ts -> src/types/user.ts` and its reverse, while both nodes carry
/// `inCycle: false`, every link carries `endpointsInCycle: false`, and the census says
/// `0 circular finding(s)`. Every one of those five statements is correct about the set it was computed
/// over, and nothing on any surface named the two sets. `cosmograph.rs`'s own 2026-07-31 note had even
/// written the sentence that went false — *"the NODE-level `inCycle` is untouched: there the claim and
/// the computation agree"* — which is what a fact with no owner does to the comment that assumed it.
pub const CYCLE_GRAPH_EXCLUDES_ERASED_IMPORTS: &str =
    "the cycle verdict and the edge list are computed over DIFFERENT edge sets: an import whose every \
     binding is erased at compile time (`import type`, a dynamic `import()`, or a plain `import { X }` \
     whose target declares X as an `export type`/`export interface`) carries no runtime module load, so \
     it is subtracted before cycle detection and is NOT subtracted from the graph, where the import is \
     still real source. So two files can import each other here and still be in no reported cycle — \
     that is the two axes disagreeing by design, not a missed cycle.";

/// File extensions whose `export type` / `export interface` declarations TypeScript erases.
///
/// [`SourceSymbolKind::Interface`] is NOT a TypeScript-only concept: `ir/kinds.rs`'s collapse table
/// maps a Java `interface`, a Rust `trait` and a Go `interface` onto the same variant, and none of
/// those is erased by anything. Non-TypeScript `ImportMap`s ride the same dep-graph builder (see
/// `pipeline::FileArtifact::imports`), so without this gate the export-side arm would silently delete
/// real cycles in four other languages.
const ERASING_EXTENSIONS: [&str; 6] = [".ts", ".tsx", ".js", ".jsx", ".mts", ".cts"];

fn target_language_erases_types(path: &str) -> bool {
    ERASING_EXTENSIONS.iter().any(|ext| path.ends_with(ext))
}

/// Per-edge record of every binding that contributed it, kept unfolded until
/// [`NoncycleCandidates::refine`] runs. Purely ephemeral: never cached, never serialized, alive for
/// one analysis run only — the same contract [`crate::circular_from_dep_excluding`]'s `excluded`
/// argument already carries.
#[derive(Debug, Clone, Default)]
pub struct NoncycleCandidates {
    /// `(from, to)` -> `(imported name, erased-by-the-import-statement)` per contributing binding.
    /// `BTreeMap`, not `HashMap`: the fold below walks it, and an ordered walk removes the question
    /// of whether that order can reach the result.
    edges: BTreeMap<(String, String), Vec<(String, bool)>>,
}

impl NoncycleCandidates {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one binding contributing the edge `from -> to`.
    ///
    /// `imported_name` is the ORIGINAL exported name (`ImportBinding::original` /
    /// `ReExport::original`), not the local alias — it is what gets looked up among the target's
    /// declarations. `erased` is the import statement's own verdict (`type_only`, or a dynamic
    /// `import()`); when it is true the name is never consulted.
    ///
    /// The synthetic originals (`"default"`, `"*"`, `"_"`) need no special case: no declaration is
    /// named any of them, so the export-side arm finds nothing and the edge is kept — which is the
    /// right answer for all three (a namespace import and a side-effect import are runtime loads, and
    /// a default export is not something this arm has evidence about).
    pub fn record(&mut self, from: &str, to: &str, imported_name: &str, erased: bool) {
        self.edges
            .entry((from.to_string(), to.to_string()))
            .or_default()
            .push((imported_name.to_string(), erased));
    }

    /// Fold the accumulator into the `(from, to)` exclusion set
    /// [`crate::circular_from_dep_excluding`] consumes.
    ///
    /// `export_side_gate` is the tree-level switch for the export-side arm. It must be FALSE when the
    /// tree's TypeScript configuration keeps type imports in the emitted output
    /// (`verbatimModuleSyntax`, `preserveValueImports`, `importsNotUsedAsValues: "preserve"`): there
    /// the statement is emitted verbatim, the module really is loaded, and the cycle is real. With the
    /// gate off this reduces exactly to the import-side-only behavior that predates it.
    pub fn refine(
        &self,
        symbols: &[SourceSymbol],
        export_side_gate: bool,
    ) -> HashSet<(String, String)> {
        // `(file, name)` -> (every declaration under it is a type/interface, at least one is exported).
        // Keyed on `(file, name)` and NOT on `SourceSymbol::id`, which deliberately collapses
        // declaration-merged siblings (`interface Foo` + `const Foo`) onto one entry — see that
        // field's doc in `ir.rs`. Requiring EVERY sibling to be a type is what makes the merge safe.
        let mut declarations: BTreeMap<(&str, &str), (bool, bool)> = BTreeMap::new();
        if export_side_gate {
            for symbol in symbols {
                let is_type = matches!(
                    symbol.kind,
                    SourceSymbolKind::Type | SourceSymbolKind::Interface
                );
                declarations
                    .entry((symbol.file.as_str(), symbol.name.as_str()))
                    .and_modify(|(all_types, any_exported)| {
                        *all_types &= is_type;
                        *any_exported |= symbol.exported;
                    })
                    .or_insert((is_type, symbol.exported));
            }
        }
        let mut excluded = HashSet::new();
        for ((from, to), bindings) in &self.edges {
            let export_arm_available = export_side_gate && target_language_erases_types(to);
            let all_erasable = bindings.iter().all(|(name, erased)| {
                *erased
                    || (export_arm_available
                        && matches!(
                            declarations.get(&(to.as_str(), name.as_str())),
                            Some((true, true))
                        ))
            });
            if all_erasable {
                excluded.insert((from.clone(), to.clone()));
            }
        }
        excluded
    }
}

#[cfg(test)]
mod tests;
