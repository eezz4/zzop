//! Fan-in bumps that mark a target file LIVE without adding a `dep`-graph node/edge — the shared shape
//! behind the `.vue`/`.svelte` SFC `<script>`-import pre-scan and runtime asset-URL loads. Both resolve a
//! reference the static import graph can't see to an EXISTING `ts_paths` participant and increment its
//! `fan_in` ONLY (never `dep`/`all_paths`), so no new `FileNode`/`dead-candidates` false positive is
//! minted (the F3 pin). Both also RETURN their resolved targets so `super::super::rules` can seed them
//! into `unreachable`'s `extra_entries` — a file reached only by a mechanism the graph can't see is
//! effectively an entrypoint, so what it reaches must not read as a dead island.

use std::collections::HashSet;

use zzop_core::{DepStats, ImportMap};

/// Bumps `dep_stats.fan_in` for every `.ts`/`.tsx`/... target a `.vue`/`.svelte` SFC's `<script>`-block
/// imports resolve to — SOURCE-ONLY: the `.vue`/`.svelte` file itself is never inserted as a `dep`/
/// `DepStats::all_paths` key or edge target, so it never becomes a `dep`-graph "participant"
/// (`zzop_rules_graph::dead_candidates::dep_graph_participants` reads straight off the raw `dep: DepGraph`
/// this function never touches) and never mints a new `FileNode`/`dead-candidates` false positive of its
/// own (the parser-owner-reviewed F3 pin this task exists to satisfy). The resolved TARGET is always an
/// existing `dep` key already (every real `ts_import_pairs` member gets one from
/// `build_dep_with_workspace`), so bumping its `fan_in` count alone — without touching
/// `dep_stats.all_paths` — adds no new participant either.
///
/// One resolved edge per (SFC file, target) pair at most (a `seen` set per file, mirroring
/// `merge_python_dep_edges`'s own dedup convention) — two named imports from the same module must not
/// double-count the same target's fan-in.
///
/// Returns the SET of resolved `.ts` targets across ALL SFCs — the `unreachable` analysis seeds these as
/// `extra_entries` ("loaded by a mechanism this graph can't see"): a `.ts` imported ONLY by a `.vue`/
/// `.svelte` component has real fan-in (so it is no longer a `dead-candidates` FP) but is NOT reachable
/// through any `dep` edge (the SFC is not a graph node), so without this it would flip from a dead
/// candidate to a false `unreachable` island. A framework-mounted component is effectively an entrypoint,
/// so what it imports is reachable.
pub(super) fn merge_sfc_fan_in(
    dep_stats: &mut DepStats,
    sfc_import_pairs: &[(String, ImportMap)],
    ts_paths: &HashSet<String>,
    workspace_pkgs: &std::collections::HashMap<String, zzop_parser_typescript::WorkspacePkg>,
    tsconfigs: &std::collections::BTreeMap<String, zzop_parser_typescript::TsconfigPaths>,
) -> HashSet<String> {
    let mut targets: HashSet<String> = HashSet::new();
    for (rel, imports) in sfc_import_pairs {
        let mut seen: HashSet<String> = HashSet::new();
        for binding in imports.values() {
            if let Some(target) = zzop_parser_typescript::resolve_file_with_workspace(
                &binding.specifier,
                rel,
                ts_paths,
                workspace_pkgs,
                tsconfigs,
            ) {
                if seen.insert(target.clone()) {
                    *dep_stats.fan_in.entry(target.clone()).or_insert(0) += 1;
                }
                targets.insert(target);
            }
        }
    }
    targets
}

/// Bumps `dep_stats.fan_in` for every file a runtime asset-URL reference resolves to — the same
/// SOURCE-ONLY, no-`dep`-node/edge shape as `merge_sfc_fan_in` (the F3 pin): the resolved target is
/// always an existing `ts_paths` participant (a `public/*.js` worklet/worker is dispatched to
/// `Language::TypeScript` and already has a `dep` key + `FileNode`), so a count-only `fan_in` bump mints
/// no new node/`dead-candidates` FP. One bump per (referencing file, target) pair (a `seen` set per
/// file, mirroring the SFC pass's dedup). Returns the SET of resolved targets across all files — seeded
/// into `unreachable`'s `extra_entries` (see `super::super::rules`): an asset loaded by a URL string is
/// effectively an entrypoint (real fan-in, but NO incoming `dep` edge since we add none), so without the
/// seed it would flip from a `dead-candidates` false positive to a false `unreachable` island — exactly
/// the flip `merge_sfc_fan_in`'s own return value prevents for SFC-mounted `.ts` targets.
pub(super) fn merge_asset_ref_fan_in(
    dep_stats: &mut DepStats,
    asset_ref_pairs: &[(String, Vec<String>)],
    ts_paths: &HashSet<String>,
    workspace_pkgs: &std::collections::HashMap<String, zzop_parser_typescript::WorkspacePkg>,
    tsconfigs: &std::collections::BTreeMap<String, zzop_parser_typescript::TsconfigPaths>,
) -> HashSet<String> {
    let mut targets: HashSet<String> = HashSet::new();
    for (rel, refs) in asset_ref_pairs {
        let mut seen: HashSet<String> = HashSet::new();
        for s in refs {
            for target in resolve_asset_ref(s, rel, ts_paths, workspace_pkgs, tsconfigs) {
                if seen.insert(target.clone()) {
                    *dep_stats.fan_in.entry(target.clone()).or_insert(0) += 1;
                }
                targets.insert(target);
            }
        }
    }
    targets
}

/// Resolves ONE captured runtime asset-URL reference string to zero or more tree-relative file paths:
/// - **Served-absolute** (`/x`): a `public/`-served path (Vite/CRA/Next serve `public/` — SvelteKit/Vite
///   also `static/` — at root `/`). Matches every tracked file whose path ends, at a segment boundary,
///   with `public/<x>` or `static/<x>`. A monorepo may legitimately have more than one match (two apps
///   shipping the same asset path); bumping ALL is FP-safe — it only ever increments fan-in on existing
///   nodes, never mints one — and is commutative, so determinism holds against the `BTreeMap` fan-in.
/// - **Relative** (`./x`/`../x`, e.g. `new URL("./worker.ts", import.meta.url)`): normal module
///   resolution relative to the referencing file, via `resolve_file_with_workspace` (exactly as
///   `merge_sfc_fan_in` resolves an SFC import) — at most one file.
/// - **Bare specifier / full URL / `blob:` / `data:`**: no anchor or off-tree — resolves to nothing
///   (never guess a target, which would false-positive liveness).
///
/// A trailing `?query`/`#hash` (e.g. `?worker`, a cache-bust hash) is stripped before matching.
fn resolve_asset_ref(
    s: &str,
    rel: &str,
    ts_paths: &HashSet<String>,
    workspace_pkgs: &std::collections::HashMap<String, zzop_parser_typescript::WorkspacePkg>,
    tsconfigs: &std::collections::BTreeMap<String, zzop_parser_typescript::TsconfigPaths>,
) -> Vec<String> {
    let clean = s.split(['?', '#']).next().unwrap_or(s);
    if let Some(served) = clean.strip_prefix('/') {
        let served = served.trim_end_matches('/');
        if served.is_empty() {
            return Vec::new();
        }
        let public_suffix = format!("public/{served}");
        let static_suffix = format!("static/{served}");
        return ts_paths
            .iter()
            .filter(|p| {
                ends_with_segment(p, &public_suffix) || ends_with_segment(p, &static_suffix)
            })
            .cloned()
            .collect();
    }
    if clean.starts_with("./") || clean.starts_with("../") {
        return zzop_parser_typescript::resolve_file_with_workspace(
            clean,
            rel,
            ts_paths,
            workspace_pkgs,
            tsconfigs,
        )
        .into_iter()
        .collect();
    }
    Vec::new()
}

/// True when `path` ends with `suffix` at a PATH-SEGMENT boundary — `.../public/x` or exactly `public/x`,
/// but never `mypublic/x`. Avoids the alloc a `format!("/{suffix}")` compare would cost per candidate.
fn ends_with_segment(path: &str, suffix: &str) -> bool {
    path.strip_suffix(suffix)
        .is_some_and(|pre| pre.is_empty() || pre.ends_with('/'))
}
