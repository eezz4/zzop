//! Mode A's hand-built dep-graph edges — the envelope lane's counterpart of
//! `zzop_parser_typescript::lang::resolve::build_dep_impl`, which it cannot call because an envelope's
//! specifiers are already-projected paths rather than things to resolve.
//!
//! Two lanes writing the same fact is exactly the shape that drifts, so the DECISION is not duplicated:
//! both lanes only RECORD into `zzop_core::NoncycleCandidates`, and the single
//! `NoncycleCandidates::refine` decides. What lives here is the envelope's own edge-admission rule
//! (specifier must name a projected file and must not be the file itself), nothing more.

use std::collections::HashSet;

use super::file_pass::FilePassState;

/// Appends one file's dep-graph edges and noncycle candidates to `state`.
///
/// Every file gets a `dep` entry — even an empty edge list — so `dep_stats_from_dep` downstream counts
/// it as a graph node, letting an isolated (import-free) file still get a `FileNode`.
pub(super) fn collect_dep_edges(
    file: &zzop_core::FileProjection,
    all_paths: &HashSet<&str>,
    state: &mut FilePassState,
) {
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    for binding in file.imports.values() {
        // Non-relative specifier naming no projected file = a package import — summarized for
        // `cross-layer/untraced-client-import-no-visible-consume`.
        if !binding.specifier.starts_with('.')
            && !binding.specifier.starts_with('/')
            && !all_paths.contains(binding.specifier.as_str())
        {
            state
                .package_import_files
                .entry(binding.specifier.clone())
                .or_default()
                .insert(file.path.clone());
        }
        if binding.deferred {
            continue; // lazy import: no module-load edge.
        }
        if binding.specifier != file.path && all_paths.contains(binding.specifier.as_str()) {
            state.noncycle_candidates.record(
                &file.path,
                &binding.specifier,
                &binding.original,
                binding.type_only,
            );
            if seen.insert(binding.specifier.clone()) {
                targets.push(binding.specifier.clone());
            }
        }
    }
    // Defect A/1 (envelope parity): fold each re-export's specifier in too, mirroring
    // `build_dep_impl`'s own re-export merge — a barrel `export { x } from './impl'` with no local
    // import of `impl` must still give `impl` a dep edge (fan-in), or `dead-candidates`
    // false-positives it. A type-only re-export (Defect 1) gets the same edge-but-excluded-from-cycles
    // treatment as a type-only import binding, rather than being dropped entirely.
    for re in &file.re_exports {
        if re.specifier != file.path && all_paths.contains(re.specifier.as_str()) {
            state
                .noncycle_candidates
                .record(&file.path, &re.specifier, &re.original, re.type_only);
            if seen.insert(re.specifier.clone()) {
                targets.push(re.specifier.clone());
            }
        }
    }
    // Defect 2 (envelope parity): a dynamic `import()` specifier gives its target fan-in but is never a
    // synchronous-load cycle edge — always excludable whatever it names, mirroring `build_dep_impl`.
    for spec in &file.dynamic_imports {
        if spec != &file.path && all_paths.contains(spec.as_str()) {
            state.noncycle_candidates.record(&file.path, spec, "", true);
            if seen.insert(spec.clone()) {
                targets.push(spec.clone());
            }
        }
    }
    state.dep.insert(file.path.clone(), targets);
}
