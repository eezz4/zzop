//! Phase 3: whole-tree dependency graph + git-history-dependent analyses. `dep`/`cycles`/`nodes`/
//! `commits`/`git_active` all feed later phases (`super::rules`, `super::metrics`, and the final
//! `AnalyzeOutput` assembly in `super::assemble`).

use std::collections::HashSet;

use zzop_core::{build_file_nodes, circular_from_dep_excluding, ir::DepGraph, ImportMap};
use zzop_metrics::{build_folder_aggregates, FolderAggregates, DEFAULT_FOLDER_DEPTH};

use crate::analyze::diagnostics::{collect_git, git_not_requested_warning, zero_packs_warning};
use crate::analyze::native_rules::dep_stats_from_dep;
use crate::pipeline::{GoModuleMap, JavaIndex, PackageJsonScan, RustWorkspaceMap};
use crate::EngineConfig;

mod merge;

use merge::{
    merge_go_dep_edges, merge_java_dep_edges, merge_python_dep_edges, merge_rust_dep_edges,
};

pub(super) struct DepGraphResult {
    pub(super) dep: DepGraph,
    pub(super) cycles: Vec<Vec<String>>,
    pub(super) nodes: Vec<zzop_core::FileNode>,
    pub(super) folders: Option<FolderAggregates>,
    pub(super) commits: Vec<zzop_core::CommitFileSet>,
    pub(super) git_active: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    root: &std::path::Path,
    config: &EngineConfig,
    warnings: &mut Vec<String>,
    loc_by_path: &std::collections::HashMap<String, u32>,
    ts_import_pairs: &[(String, ImportMap)],
    ts_re_export_pairs: &[(String, Vec<zzop_core::ReExport>)],
    ts_dynamic_import_pairs: &[(String, Vec<String>)],
    ts_paths: &HashSet<String>,
    pkg_scan: &PackageJsonScan,
    tsconfigs: &std::collections::BTreeMap<String, zzop_parser_typescript::TsconfigPaths>,
    rust_workspace: &RustWorkspaceMap,
    go_modules: &GoModuleMap,
    java_index: &JavaIndex,
) -> DepGraphResult {
    // `type_only_edges` is the ephemeral noncycle-exclusion set (never cached/serialized — see
    // `circular_from_dep_excluding`'s doc): a pair present here is contributed ONLY by edges excludable
    // from cycle detection — type-only bindings/re-exports, or a dynamic `import()` (Defect 2) — so
    // `circular_findings` in `super::rules` must not count it as a cycle edge even though `dep` itself
    // (fan-in/dead-exports/every other metric) still includes it.
    let (mut dep, type_only_edges): (DepGraph, HashSet<(String, String)>) =
        zzop_parser_typescript::build_dep_with_workspace(
            ts_import_pairs,
            ts_re_export_pairs,
            ts_dynamic_import_pairs,
            ts_paths,
            &pkg_scan.workspace_pkgs,
            tsconfigs,
        );
    // Python dep-graph edges — a separate, engine-side pass (NOT routed through
    // `build_dep_with_workspace`'s own resolver) — see `merge_python_dep_edges`'s doc for the resolver
    // wiring shape and why. Every Python file already has an entry in `dep` (possibly empty) from the
    // call above, since `ts_import_pairs` carries its `ImportMap` too (`ts_slot`'s shared participation —
    // see `pipeline::FileArtifact::imports`'s doc); this only adds edges, never removes what's there.
    merge_python_dep_edges(&mut dep, ts_import_pairs, ts_paths);
    // Rust dep-graph edges — an additive, separate post-hoc pass mirroring `merge_python_dep_edges`
    // exactly (deliberately NOT generalized together: the two resolvers have different shapes —
    // `resolve_rust_import` also needs `rust_workspace`, `resolve_python_import` does not — and folding
    // them into one generic function would obscure both languages' own resolution semantics for no
    // real reuse win, one `for` loop each).
    merge_rust_dep_edges(&mut dep, ts_import_pairs, ts_paths, rust_workspace);
    // Go dep-graph edges — an additive, separate post-hoc pass mirroring `merge_python_dep_edges`/
    // `merge_rust_dep_edges` exactly (deliberately NOT generalized together with either: all three
    // resolvers have different shapes — `resolve_go_import_package_dir` resolves to a PACKAGE DIRECTORY
    // whose every file then needs its own edge, `resolve_rust_import` needs `rust_workspace` and resolves
    // to ONE file, `resolve_python_import` needs neither — and folding them into one generic function
    // would obscure all three languages' own resolution semantics for no real reuse win, one `for` loop
    // each; same reasoning already documented at `merge_rust_dep_edges`'s own call site here).
    merge_go_dep_edges(&mut dep, ts_import_pairs, ts_paths, go_modules);
    // Java dep-graph edges — an additive, separate post-hoc pass mirroring the Python/Rust/Go trio above
    // exactly (deliberately NOT generalized together with any of them: `resolve_java_import` resolves to
    // MULTIPLE files for a glob import and needs `java_index`, a shape none of the other three resolvers
    // share — same "one `for` loop each, no forced-generic reuse" reasoning `merge_go_dep_edges`'s own
    // call site documents). `.java` joined the shared dep graph only in this batch — see
    // `pipeline::FileArtifact::imports`'s doc for the "`Language::Java21` now" update.
    merge_java_dep_edges(&mut dep, ts_import_pairs, java_index);
    // Rust module cycles are structural, not architectural: cargo forbids cross-CRATE cycles outright,
    // and intra-crate parent<->child module edges (`mod x;` down + the child's `use super::`/`use
    // crate::...` back up) are idiomatic — rustc compiles a crate as one unit, so an all-`.rs` cycle
    // carries none of the "extract the shared piece" signal the circular rule exists to surface (found
    // by the first self-analysis dogfood run: 33/33 circular findings were this shape). Same exclusion
    // class as `type_only_edges` — a visible but not load-bearing edge. A mixed-language cycle cannot
    // occur (no cross-language import edge exists), so the all-`.rs` test is exact and TS/Python cycle
    // reporting is untouched.
    //
    // Go gets NO analogous exclusion, by deliberate analysis (not oversight): `merge_go_dep_edges` never
    // emits a same-package edge (two files in the SAME package share symbols with no import statement
    // between them at all — Go's own compilation-unit model), so every edge it emits is INTER-package.
    // A cycle built entirely from `.go` edges therefore always reflects two (or more) DISTINCT packages
    // importing each other, directly or transitively — a REAL Go import cycle, which `go build` itself
    // rejects at compile time. Unlike Rust's intra-crate parent<->child shape, there is no idiomatic,
    // compiler-accepted Go source shape that would produce an all-`.go` cycle through this pass's own
    // file-fanout (package A's file importing package B fans out to EVERY B file; package B's file
    // importing package A back fans out to EVERY A file — the resulting file-level cycle is a faithful,
    // non-spurious projection of the real A<->B package cycle, not an artifact of the fanout itself).
    // Verified by construction, not just argued: `analyze_go_module.rs`'s
    // `cross_package_mutual_import_cycle_is_reported_not_excluded` builds exactly this two-package,
    // multi-file-per-package shape and asserts the cycle IS reported.
    let cycles: Vec<Vec<String>> = circular_from_dep_excluding(&dep, &type_only_edges)
        .into_iter()
        .filter(|cycle| !cycle.iter().all(|f| f.ends_with(".rs")))
        .collect();

    let dep_stats = dep_stats_from_dep(&dep);

    // Git-history-dependent analyses. `None`/failed-collection both fall through to a default
    // (all-zero) `GitStats` and no commits — `nodes` still builds (dep-graph + LOC signal only) and
    // scores/health/recommendations/critical/seams stay empty. (`warnings` was declared by
    // `super::provides`, at the global-prefix seam.)
    if let Some(w) = git_not_requested_warning(config) {
        warnings.push(w);
    }
    if let Some(w) = zero_packs_warning(config) {
        warnings.push(w);
    }
    let (git_stats, commits, git_active) = collect_git(root, config, warnings);

    // `is_source`: reuses the same dispatch classification the fused pass used to pick a parser
    // frontend, so `risk_score`/`hotspot_score` are zeroed for non-source files (data/config/assets)
    // right where `FileNode`s are built.
    let is_source = |id: &str| crate::dispatch::dispatch(id, &config.dispatch).is_some();
    let nodes = build_file_nodes(
        &dep_stats,
        &git_stats,
        loc_by_path,
        &zzop_core::DEFAULT_WEIGHTS,
        is_source,
    );

    // `AnalyzeOutput::folders` is not git-gated: `nodes`/`dep` are both already built unconditionally.
    let folders = Some(build_folder_aggregates(&nodes, &dep, DEFAULT_FOLDER_DEPTH));

    DepGraphResult {
        dep,
        cycles,
        nodes,
        folders,
        commits,
        git_active,
    }
}
