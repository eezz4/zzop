//! Dep-graph projection: `build_dep`/`build_dep_with_workspace` fold per-file imports, re-exports,
//! and dynamic `import()`s into a deduped internal-edge `DepGraph` plus the noncycle-exclusion set.

use std::collections::{BTreeMap, HashMap, HashSet};

use zzop_core::{DepGraph, ImportMap, ReExport};

use super::specifier::resolve_file;
use super::tsconfig::TsconfigPaths;
use super::workspace::{resolve_file_with_workspace, WorkspacePkg};

/// Shared implementation behind `build_dep`/`build_dep_with_workspace`. Per file: resolves each
/// non-deferred import binding via `resolve`, keeping deduped internal edges (external/deferred
/// specifiers excluded) — feeding circular detection and fan-in/out. Also merges in each file's
/// re-export specifiers (`export {x} from './y'` / `export * from './y'`, including type-only ones) and
/// dynamic-`import()` specifiers as the same kind of edge, resolved+deduped into the same vector: a bare
/// re-export used to be invisible to the dep graph (Defect A), undercounting a barrel file's fan-in and
/// false-positiving `dead-candidates`; a code-split-only module reached solely via `import('./x')` had
/// the same problem (Defect 2) since dynamic imports never reached this builder at all. A type-only
/// re-export (`export type {X} from './y'`) now gets the same edge-but-excluded-from-cycles treatment as
/// a type-only import binding, rather than being dropped entirely — see below.
///
/// Separately returns an ephemeral `(from, to)` exclusion set consulted only by
/// `zzop_core::circular_from_dep_excluding` (never cached/serialized, lives only for one analysis run):
/// pairs where EVERY edge contributing that target is excludable from cycle detection — a type-only
/// import/re-export (`import type`/per-specifier `{ type X }`/`export type {X} from`, erased at compile
/// time) or a dynamic `import()` (async; never a synchronous module-load cycle, and specifically how
/// people BREAK cycles). The returned `DepGraph` still includes these edges (fan-in/dead-exports/every
/// other metric legitimately count a type import or a dynamic import as a "use" of the target). A pair
/// with at least one plain synchronous value edge to the same target is not excluded (a real runtime
/// cycle edge exists).
fn build_dep_impl<F>(
    files: &[(String, ImportMap)],
    re_exports: &[(String, Vec<ReExport>)],
    dynamic_imports: &[(String, Vec<String>)],
    mut resolve: F,
) -> (DepGraph, HashSet<(String, String)>)
where
    F: FnMut(&str, &str) -> Option<String>,
{
    let re_export_map: HashMap<&str, &Vec<ReExport>> = re_exports
        .iter()
        .map(|(rel, rs)| (rel.as_str(), rs))
        .collect();
    let dyn_import_map: HashMap<&str, &Vec<String>> = dynamic_imports
        .iter()
        .map(|(rel, ds)| (rel.as_str(), ds))
        .collect();
    let mut dep = DepGraph::new();
    let mut noncycle_edges = HashSet::new();
    for (rel, imports) in files {
        let mut seen = HashSet::new();
        let mut resolved = Vec::new();
        // target -> true iff EVERY edge resolving to it so far is excluded from cycle detection: a
        // type-only import/re-export (erased at compile time) or a dynamic `import()` (async — never a
        // synchronous module-load cycle). Any one plain value edge flips it false. The target still
        // gains a real dep edge (fan-in) either way; only `circular_from_dep_excluding` consults this set.
        let mut target_noncycle: HashMap<String, bool> = HashMap::new();
        for binding in imports.values() {
            if binding.deferred {
                continue; // lazy require/import: no module-load edge
            }
            if let Some(target) = resolve(&binding.specifier, rel) {
                target_noncycle
                    .entry(target.clone())
                    .and_modify(|all| *all &= binding.type_only)
                    .or_insert(binding.type_only);
                if seen.insert(target.clone()) {
                    resolved.push(target);
                }
            }
        }
        if let Some(res) = re_export_map.get(rel.as_str()) {
            for re in res.iter() {
                // Defect 1: a type-only re-export used to be dropped entirely (no edge, no fan-in). It
                // now gets the same treatment as a type-only binding — a real edge, excluded from cycles.
                if let Some(target) = resolve(&re.specifier, rel) {
                    target_noncycle
                        .entry(target.clone())
                        .and_modify(|all| *all &= re.type_only)
                        .or_insert(re.type_only);
                    if seen.insert(target.clone()) {
                        resolved.push(target);
                    }
                }
            }
        }
        if let Some(dyns) = dyn_import_map.get(rel.as_str()) {
            for spec in dyns.iter() {
                // Defect 2: a dynamic `import()` gives the target fan-in (it IS used) but is never a
                // synchronous-load cycle edge — always excludable. `or_insert(true)` (not `&= true`)
                // because `& true` is identity: a pre-existing `false` from a value edge must stay false.
                if let Some(target) = resolve(spec, rel) {
                    target_noncycle.entry(target.clone()).or_insert(true);
                    if seen.insert(target.clone()) {
                        resolved.push(target);
                    }
                }
            }
        }
        for (target, all_noncycle) in target_noncycle {
            if all_noncycle {
                noncycle_edges.insert((rel.clone(), target));
            }
        }
        dep.insert(rel.clone(), resolved);
    }
    (dep, noncycle_edges)
}

/// `build_dep`, aware of workspace packages and tsconfig `paths`/`baseUrl`: resolves each binding/
/// re-export/dynamic-import via `resolve_file_with_workspace`. Behaviorally equivalent to `build_dep`
/// when both maps are empty. See `build_dep_impl`'s doc for the merged-re-export/dynamic-import-edge and
/// noncycle-exclusion-set behavior both `build_dep`/`build_dep_with_workspace` share.
pub fn build_dep_with_workspace(
    files: &[(String, ImportMap)],
    re_exports: &[(String, Vec<ReExport>)],
    dynamic_imports: &[(String, Vec<String>)],
    all_paths: &HashSet<String>,
    workspace_pkgs: &HashMap<String, WorkspacePkg>,
    tsconfigs: &BTreeMap<String, TsconfigPaths>,
) -> (DepGraph, HashSet<(String, String)>) {
    build_dep_impl(files, re_exports, dynamic_imports, |specifier, rel| {
        resolve_file_with_workspace(specifier, rel, all_paths, workspace_pkgs, tsconfigs)
    })
}

/// Build a file-level dep graph: per file, resolve each non-deferred import, re-export (type-only
/// included — see `build_dep_impl`'s doc), and dynamic-`import()` specifier, keeping deduped internal
/// edges (external/deferred specifiers excluded), feeding circular detection and fan-in/out. See
/// `build_dep_impl`'s doc for the full re-export/dynamic-import-merge/noncycle-exclusion-set behavior (the
/// second return value).
pub fn build_dep(
    files: &[(String, ImportMap)],
    re_exports: &[(String, Vec<ReExport>)],
    dynamic_imports: &[(String, Vec<String>)],
    all_paths: &HashSet<String>,
) -> (DepGraph, HashSet<(String, String)>) {
    build_dep_impl(files, re_exports, dynamic_imports, |specifier, rel| {
        resolve_file(specifier, rel, all_paths)
    })
}

#[cfg(test)]
mod edge_tests;
#[cfg(test)]
mod noncycle_tests;
